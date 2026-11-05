//! The live engine, from the main thread: plays through the output device and follows the
//! project as it's edited, keeping each track's and aux's plugins through every edit. It also
//! opens plugins' editors and saves their settings.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use crate::{
    audio::engine::{
        AudioData, AudioFiles, AudioSetup, Engine, Garbage, LanePlugins, Link, LiveNote, MASTER,
        MAX_BLOCK, MAX_REORDER, Modules, Msg, Output, Played, Rearrange, SLOTS, Shared, Strips,
        compile, load_running, make_lane,
    },
    audio::midi::{self, LiveInput},
    audio::peaks::Peaks,
    plugin::{Editor, Plugin, PluginParam, Processor},
    song::project::{Fx, Instrument, PluginRef, Project, STATES_DIR},
};

/// A lane: which slot it's in, what it was built from, its latency, and its plugins'
/// main-thread halves.
struct Slot {
    slot: usize,
    sound: Sound,
    latency: u32,
    /// Whether its plugin instrument hears MPE, as last told.
    mpe: bool,
    /// What each of the lane's stacked built-ins' running state was made for.
    strips: Vec<Fx>,
    plugins: LanePlugins,
    /// Parameters the project sets, as last applied; and what each was before Pina Studio first set
    /// it, to go back to when the project stops setting it (as when that edit is undone).
    applied: HashMap<(Which, u32), f64>,
    original: HashMap<(Which, u32), f64>,
    /// The preset each plugin has loaded, as last applied; and its settings from before Pina Studio
    /// first loaded one, to go back to.
    presets: HashMap<Which, Option<crate::song::project::Preset>>,
    before_preset: HashMap<Which, Vec<u8>>,
}

impl Slot {
    fn plugin(&self, which: Which) -> Option<&Plugin> {
        match which {
            Which::Instrument => self.plugins.instrument.as_ref(),
            Which::Insert(i) => self.plugins.inserts.get(i),
        }
    }

    /// Gives a plugin the preset and parameter values its `PluginRef` sets.
    fn apply(&mut self, which: Which, r: &PluginRef) -> Result<(), String> {
        let plugins = &self.plugins;
        let Some(p) = (match which {
            Which::Instrument => plugins.instrument.as_ref(),
            Which::Insert(i) => plugins.inserts.get(i),
        }) else {
            return Ok(());
        };
        let mut problem = Ok(());
        let had = self.presets.get(&which).cloned().flatten();
        if r.preset != had {
            if had.is_none()
                && let Some(state) = p.state()
            {
                self.before_preset.insert(which, state);
            }
            match &r.preset {
                Some(preset) => problem = p.load_preset(preset),
                None => {
                    if let Some(state) = self.before_preset.remove(&which) {
                        problem = p.set_state(&state);
                    }
                }
            }
            self.presets.insert(which, r.preset.clone());
            // The preset set every parameter: what Pina Studio set before is set again over it.
            self.applied.retain(|(w, _), _| *w != which);
            self.original.retain(|(w, _), _| *w != which);
        }
        let mut changed = Vec::new();
        for (&id, &v) in &r.params {
            if self.applied.get(&(which, id)) != Some(&v) {
                changed.push((id, v, p.param_value(id)));
                p.set_param(id, v);
            }
        }
        let gone: Vec<(Which, u32)> = self
            .applied
            .keys()
            .filter(|(w, id)| *w == which && !r.params.contains_key(id))
            .copied()
            .collect();
        for key in gone {
            self.applied.remove(&key);
            if let Some(v) = self.original.remove(&key) {
                p.set_param(key.1, v);
            }
        }
        for (id, v, was) in changed {
            self.original.entry((which, id)).or_insert(was);
            self.applied.insert((which, id), v);
        }
        problem
    }
}

/// The id the master's plugins go by, where tracks and auxes use theirs.
pub const MASTER_ID: u64 = u64::MAX;

/// What makes a lane's sound, settings aside: its instrument ("synth", "drums" or a plugin) and
/// its inserts.
type Sound = (Option<String>, Vec<(PathBuf, String, u64)>);

fn sound(instrument: Option<&Instrument>, inserts: &[PluginRef]) -> Sound {
    let key = |r: &PluginRef| (r.path.clone(), r.class.clone(), r.instance);
    let instrument = instrument.map(|i| match i {
        Instrument::Synth => "synth".to_string(),
        Instrument::Drums => "drums".to_string(),
        Instrument::Kit => "kit".to_string(),
        // A sampler is rebuilt when its samples change; its settings it follows as it plays.
        Instrument::Sampler(s) => format!(
            "sampler {:?}",
            s.zones.iter().map(|z| &z.file).collect::<Vec<_>>()
        ),
        // A synth's one voice engine, whatever it's set to.
        Instrument::Poly(_) => "poly".to_string(),
        Instrument::Plugin(r) => format!("{}#{}", r.path.display(), r.class),
    });
    (instrument, inserts.iter().map(key).collect())
}

pub use crate::song::project::Which;

/// Where each of `now` was in `before`, if it was (the same one twice, in the order they came).
fn matching<K: PartialEq>(before: &[K], now: &[K]) -> Vec<Option<usize>> {
    let mut used = vec![false; before.len()];
    now.iter()
        .map(|k| {
            let i = (0..before.len()).find(|&i| !used[i] && before[i] == *k)?;
            used[i] = true;
            Some(i)
        })
        .collect()
}

pub struct Player {
    rate: f64,
    tx: rtrb::Producer<Msg>,
    garbage: rtrb::Consumer<Garbage>,
    auditions_ended: u64,
    shared: Arc<Shared>,
    /// By track id.
    lanes: HashMap<u64, Slot>,
    /// Each kit track's pads (by id): their note, and their track.
    kits: HashMap<u64, Vec<(u8, u64)>>,
    free: Vec<usize>,
    modules: Modules,
    files: AudioFiles,
    output: Option<Output>,
    live: LiveInput,
    played: rtrb::Consumer<Played>,
    heard: rtrb::Consumer<(u64, crate::plugin::ControlKind)>,
    _midi: Vec<midir::MidiInputConnection<()>>,
    dir: PathBuf,
    /// Open plugin editors, by track or aux id.
    editors: HashMap<(u64, Which), Editor>,
    /// The main-thread halves of plugins taken out of a chain, kept until the engine gives back
    /// their processors (by the rearrangement they went in), and its count.
    retired: Vec<(u64, Vec<Plugin>)>,
    rearranged: u64,
}

impl Drop for Player {
    fn drop(&mut self) {
        // Editors close before their plugins go, and the audio stops (dropping the engine and its
        // processors) before the plugins' main-thread halves do.
        self.editors.clear();
        self.output = None;
    }
}

impl Player {
    /// A player and the engine it drives, at the project's sample rate. Empty: `sync` loads the
    /// project.
    fn new(dir: &Path, rate: f64) -> (Player, Engine) {
        let (tx, rx) = rtrb::RingBuffer::new(1024);
        let (gtx, garbage) = rtrb::RingBuffer::new(1024);
        let shared = Arc::new(Shared::new());
        let (live_tx, live) = rtrb::RingBuffer::new(1024);
        let (played_tx, played) = rtrb::RingBuffer::new(4096);
        let (moves_tx, moves) = rtrb::RingBuffer::new(4096);
        let (heard_tx, heard) = rtrb::RingBuffer::new(16384);
        let link = Link {
            rx,
            garbage: gtx,
            shared: shared.clone(),
            live,
            played: played_tx,
            moves,
            heard: heard_tx,
        };
        let engine = Engine::live(rate, link);
        let player = Player {
            rate,
            tx,
            garbage,
            auditions_ended: 0,
            shared,
            live: Arc::new(Mutex::new(midi::LiveSink {
                notes: live_tx,
                moves: moves_tx,
            })),
            played,
            heard,
            _midi: Vec::new(),
            lanes: HashMap::new(),
            kits: HashMap::new(),
            free: (0..SLOTS).rev().collect(),
            modules: Modules::new(),
            files: AudioFiles::new(dir, rate).in_background(),
            output: None,
            dir: dir.to_path_buf(),
            editors: HashMap::new(),
            retired: Vec::new(),
            rearranged: 0,
        };
        (player, engine)
    }

    /// Plays through `setup`'s output, with every MIDI input playing the armed track.
    pub fn start(project: &Project, dir: &Path, setup: &AudioSetup) -> Result<Player, String> {
        // At the project's rate if the output can run at it, else at the output's.
        let rate = crate::audio::engine::output_rate(project.sample_rate, setup)?;
        let (mut player, engine) = Player::new(dir, rate as f64);
        player.output = Some(engine.start_output(setup)?);
        player._midi = midi::connect(&player.live);
        Ok(player)
    }

    /// Moves the sound to `setup`'s output and buffer, the song staying as it is.
    pub fn set_output(&mut self, setup: &AudioSetup) -> Result<(), String> {
        self.output.as_mut().map_or(Ok(()), |o| o.change(setup))
    }

    /// Runs the engine in real time on a thread of its own, heard by nobody: for tests, and
    /// machines without audio. The thread ends with the player.
    pub fn silent(project: &Project, dir: &Path) -> Player {
        let (player, mut engine) = Player::new(dir, project.sample_rate as f64);
        let block = Duration::from_secs_f64(MAX_BLOCK as f64 / player.rate);
        thread::spawn(move || {
            let (mut l, mut r) = (vec![0f32; MAX_BLOCK], vec![0f32; MAX_BLOCK]);
            while !engine.abandoned() {
                engine.process([&mut l, &mut r], MAX_BLOCK);
                thread::sleep(block);
            }
        });
        player
    }

    /// Brings the engine up to date with `project`: builds lanes for new tracks and auxes (and for
    /// those whose instrument or inserts changed), drops lanes of deleted ones, and swaps in what
    /// everything plays.
    pub fn sync(&mut self, project: &Project) -> Result<(), String> {
        self.collect();
        self.kits.clear();
        for t in &project.tracks {
            if let Some(pad) = t.pad {
                self.kits.entry(pad.kit).or_default().push((pad.note, t.id));
            }
        }
        let channels: Vec<(u64, Option<&Instrument>, &[PluginRef])> = project
            .tracks
            .iter()
            .map(|t| (t.id, t.instrument.as_ref(), &t.inserts[..]))
            .chain(project.auxes.iter().map(|a| (a.id, None, &a.inserts[..])))
            .chain([(MASTER_ID, None, &project.master_inserts[..])])
            .collect();
        let current: HashMap<u64, Sound> = channels
            .iter()
            .map(|&(id, instrument, inserts)| (id, sound(instrument, inserts)))
            .collect();
        let mut problems = Vec::new();
        // The same instrument: its inserts change in place. Those kept run on as they are
        // (moved, if they've moved), new ones load, those taken out go: nothing that stays is
        // loaded again, so nothing done to it since the song was saved is lost.
        let changes: Vec<(u64, Vec<Option<usize>>)> = self
            .lanes
            .iter()
            .filter_map(|(id, s)| {
                let now = current.get(id)?;
                let changed = s.sound != *now && s.sound.0 == now.0;
                changed.then(|| (*id, matching(&s.sound.1, &now.1)))
            })
            .collect();
        for (id, from) in changes {
            let had = self.lanes[&id].sound.1.len();
            let inserts = channels.iter().find(|c| c.0 == id).map_or(&[][..], |c| c.2);
            let kept: Option<Vec<usize>> = from.iter().copied().collect();
            match kept.filter(|k| k.len() == had && had <= MAX_REORDER) {
                Some(was) => self.permute(id, &was, current[&id].clone())?,
                None => {
                    problems.extend(self.rearrange(id, inserts, &from, current[&id].clone())?)
                }
            }
        }
        // Another instrument (or sampler samples): the lane is made again, its inserts loaded
        // with the settings they have now.
        let stale: Vec<u64> = self
            .lanes
            .iter()
            .filter(|(id, s)| current.get(id) != Some(&s.sound))
            .map(|(id, _)| *id)
            .collect();
        let mut held: HashMap<u64, Vec<Option<Vec<u8>>>> = HashMap::new();
        for id in stale {
            if let (Some(s), Some(now)) = (self.lanes.get(&id), current.get(&id)) {
                let states = matching(&s.sound.1, &now.1)
                    .into_iter()
                    .map(|j| {
                        j.and_then(|j| s.plugins.inserts.get(j))
                            .and_then(|p| p.state())
                    })
                    .collect();
                held.insert(id, states);
            }
            // Editors close before their plugins go; the engine lets the processors go after.
            self.editors.retain(|(of, _), _| *of != id);
            let s = self.lanes.remove(&id).unwrap();
            self.send(Msg::Remove(s.slot))?;
            self.free.push(s.slot);
        }
        for &(id, instrument, inserts) in &channels {
            if self.lanes.contains_key(&id) {
                continue;
            }
            let slot = self.free.pop().ok_or("too many tracks")?;
            let mut states = held.remove(&id).unwrap_or_default();
            let running: Vec<bool> = (0..inserts.len())
                .map(|i| states.get(i).is_some_and(Option::is_some))
                .collect();
            // A plugin that won't load is stood in for; samples that can't be read leave the
            // track silent rather than stopping the rest.
            let made = make_lane(
                instrument,
                inserts,
                &mut states,
                self.rate,
                &mut self.modules,
                &self.dir,
            );
            let (lane, mut plugins, latency) = made.unwrap_or_else(|e| {
                problems.push(e);
                let empty = make_lane(
                    None,
                    &[],
                    &mut Vec::new(),
                    self.rate,
                    &mut self.modules,
                    &self.dir,
                );
                empty.expect("a lane without plugins always builds")
            });
            problems.append(&mut plugins.problems);
            self.send(Msg::Insert(slot, Box::new(lane)))?;
            let sound = sound(instrument, inserts);
            // A preset its saved (or running) settings hold is loaded already.
            let refs = match instrument {
                Some(Instrument::Plugin(r)) => vec![(Which::Instrument, r, false)],
                _ => Vec::new(),
            };
            let all = refs.into_iter().chain(
                inserts
                    .iter()
                    .enumerate()
                    .map(|(i, r)| (Which::Insert(i), r, running[i])),
            );
            let presets = all
                .filter(|(_, r, running)| r.state.is_some() || *running)
                .map(|(w, r, _)| (w, r.preset.clone()))
                .collect();
            self.lanes.insert(
                id,
                Slot {
                    slot,
                    sound,
                    latency,
                    mpe: matches!(instrument, Some(Instrument::Plugin(r)) if r.mpe),
                    strips: Vec::new(),
                    plugins,
                    applied: HashMap::new(),
                    original: HashMap::new(),
                    presets,
                    before_preset: HashMap::new(),
                },
            );
        }
        let mut told = Vec::new();
        for &(id, instrument, inserts) in &channels {
            let slot = self.lanes.get_mut(&id).unwrap();
            if let Some(Instrument::Plugin(r)) = instrument
                && let Err(e) = slot.apply(Which::Instrument, r)
            {
                problems.push(e);
            }
            // MPE switched: the running instrument told, not loaded again.
            if let Some(Instrument::Plugin(r)) = instrument
                && r.mpe != slot.mpe
            {
                slot.mpe = r.mpe;
                told.push(Msg::Mpe {
                    slot: slot.slot,
                    on: r.mpe,
                });
            }
            for (i, r) in inserts.iter().enumerate() {
                if let Err(e) = slot.apply(Which::Insert(i), r) {
                    problems.push(e);
                }
            }
        }
        for msg in told {
            self.send(msg)?;
        }
        // Each lane with running state for its channel's stacked built-ins: a new strip where
        // there's none, or where a reverb or delay needs one with its lines.
        let others = project
            .tracks
            .iter()
            .map(|t| (t.id, &t.more))
            .chain(project.auxes.iter().map(|a| (a.id, &a.more)))
            .chain([(MASTER_ID, &project.master_more)]);
        for (id, more) in others {
            let Some(slot) = self.lanes.get_mut(&id) else {
                continue;
            };
            let want: Vec<Fx> = more.iter().map(|u| u.kind()).collect();
            // Those with state of their own need a strip made for them.
            let long = |k: Fx| !matches!(k, Fx::Eq | Fx::Comp | Fx::Sat | Fx::Limit);
            let new = |k: usize| match slot.strips.get(k) {
                None => true,
                Some(&had) => had != want[k] && long(want[k]),
            };
            if !(0..want.len()).any(new) {
                continue;
            }
            let fresh: Vec<Option<crate::dsp::Strip>> = (0..want.len())
                .map(|k| new(k).then(|| crate::dsp::Strip::for_kind(want[k], self.rate)))
                .collect();
            let had = slot.strips.len();
            for (k, &kind) in want.iter().enumerate() {
                match slot.strips.get_mut(k) {
                    Some(s) => *s = kind,
                    None => slot.strips.push(kind),
                }
            }
            let lane = slot.slot;
            self.send(Msg::Strips(Box::new(Strips::new(lane, fresh, had))))?;
        }
        let lane = |id: &u64| (self.lanes[id].slot, self.lanes[id].latency);
        let tracks: Vec<_> = project.tracks.iter().map(|t| lane(&t.id)).collect();
        let auxes: Vec<_> = project.auxes.iter().map(|a| lane(&a.id)).collect();
        let master = Some(self.lanes[&MASTER_ID].slot);
        let (arrangement, missing) =
            compile(project, self.rate, &tracks, &auxes, master, &mut self.files);
        problems.extend(missing);
        self.send(Msg::Arrange(Box::new(arrangement)))?;
        match problems.is_empty() {
            true => Ok(()),
            false => Err(problems.join("; ")),
        }
    }

    /// The main-thread half of a track's or aux's plugin (the master's by `MASTER_ID`).
    pub fn plugin(&self, id: u64, which: Which) -> Option<&Plugin> {
        self.lanes.get(&id)?.plugin(which)
    }

    /// Where a track's built-in sampler's voices are playing, if it has one.
    pub fn sampler_tap(&self, id: u64) -> Option<Arc<crate::dsp::sampler::SamplerTap>> {
        self.lanes.get(&id)?.plugins.sampler.clone()
    }

    /// Lets every plugin do the main-thread work it asked for. Call it often.
    pub fn idle(&self) {
        for slot in self.lanes.values() {
            slot.plugins.instrument.iter().for_each(|p| p.idle());
            slot.plugins.inserts.iter().for_each(|p| p.idle());
        }
    }

    /// Opens (or brings back) a plugin's editor. Main thread only.
    pub fn open_editor(&mut self, id: u64, which: Which) -> Result<(), String> {
        if let Some(e) = self.editors.get(&(id, which)) {
            e.show();
            return Ok(());
        }
        let mtm =
            crate::plugin::MainThreadMarker::new().ok_or("editors open on the main thread")?;
        let plugin = self.plugin(id, which).ok_or("no plugin there")?;
        let editor = plugin.open_editor(mtm)?;
        self.editors.insert((id, which), editor);
        Ok(())
    }

    /// A picture of a plugin's open window, as a JPEG at `out`, and its size in pixels.
    pub fn editor_picture(&self, id: u64, which: Which, out: &Path) -> Result<(u32, u32), String> {
        self.editors
            .get(&(id, which))
            .ok_or("its window isn't open")?
            .window
            .picture(out)
    }

    /// Writes every plugin's settings into `Plugin States/` and points the project at them, so
    /// saving the project saves them. Files no longer used are removed.
    pub fn save_states(&self, project: &mut Project) -> Result<(), String> {
        self.save_states_in(project, Path::new(STATES_DIR))
    }

    /// `save_states` into another folder of the project's.
    pub fn save_states_in(&self, project: &mut Project, folder: &Path) -> Result<(), String> {
        let dir = self.dir.join(folder);
        let err = |e: std::io::Error| format!("{}: {e}", dir.display());
        std::fs::create_dir_all(&dir).map_err(err)?;
        let mut kept = HashSet::new();
        let mut save = |r: &mut PluginRef, p: &Plugin, name: String| -> Result<(), String> {
            // What Pina Studio set may since have been turned in the plugin's own editor.
            for (&id, v) in r.params.iter_mut() {
                *v = p.param_value(id);
            }
            if let Some(bytes) = p.state() {
                let file = folder.join(name);
                std::fs::write(self.dir.join(&file), bytes).map_err(err)?;
                kept.insert(self.dir.join(&file));
                r.state = Some(file);
            }
            Ok(())
        };
        for (n, t) in project.tracks.iter_mut().enumerate() {
            let Some(slot) = self.lanes.get(&t.id) else {
                continue;
            };
            if let (Some(Instrument::Plugin(r)), Some(p)) =
                (&mut t.instrument, &slot.plugins.instrument)
            {
                save(r, p, format!("Track {} instrument.state", n + 1))?;
            }
            for (i, (r, p)) in t.inserts.iter_mut().zip(&slot.plugins.inserts).enumerate() {
                save(r, p, format!("Track {} insert {}.state", n + 1, i + 1))?;
            }
        }
        for a in &mut project.auxes {
            let Some(slot) = self.lanes.get(&a.id) else {
                continue;
            };
            for (i, (r, p)) in a.inserts.iter_mut().zip(&slot.plugins.inserts).enumerate() {
                save(r, p, format!("Aux {} insert {}.state", a.bus, i + 1))?;
            }
        }
        if let Some(slot) = self.lanes.get(&MASTER_ID) {
            let inserts = project.master_inserts.iter_mut();
            for (i, (r, p)) in inserts.zip(&slot.plugins.inserts).enumerate() {
                save(r, p, format!("Master insert {}.state", i + 1))?;
            }
        }
        for entry in std::fs::read_dir(&dir).map_err(err)?.flatten() {
            if !kept.contains(&entry.path()) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        Ok(())
    }

    /// A channel's inserts changed in place (`from[k]` the running insert, by its old place,
    /// at place `k`; none, a new one): new ones load, those kept stay as they are, those taken
    /// out go (their editors closing). Says what wouldn't load.
    fn rearrange(
        &mut self,
        id: u64,
        inserts: &[PluginRef],
        from: &[Option<usize>],
        sound: Sound,
    ) -> Result<Vec<String>, String> {
        let (rate, dir) = (self.rate, self.dir.clone());
        let mut problems = Vec::new();
        let mut fresh: Vec<Option<Processor>> = Vec::with_capacity(from.len());
        let mut loaded: Vec<Option<(Plugin, u32)>> = Vec::with_capacity(from.len());
        for (k, f) in from.iter().enumerate() {
            if f.is_some() {
                fresh.push(None);
                loaded.push(None);
                continue;
            }
            let (plugin, processor, problem) =
                load_running(&inserts[k], None, rate, &mut self.modules, &dir);
            problems.extend(problem);
            loaded.push(Some((plugin, processor.latency())));
            fresh.push(Some(processor));
        }
        let Some(slot) = self.lanes.get_mut(&id) else {
            return Ok(problems);
        };
        let had = slot.plugins.inserts.len();
        let before: u32 = slot.plugins.latencies.iter().sum();
        let latencies = std::mem::take(&mut slot.plugins.latencies);
        let mut old: Vec<Option<(Plugin, u32)>> = std::mem::take(&mut slot.plugins.inserts)
            .into_iter()
            .zip(latencies.into_iter().chain(std::iter::repeat(0)))
            .map(Some)
            .collect();
        for (k, f) in from.iter().enumerate() {
            let (plugin, latency) = match f {
                Some(j) => old[*j].take().expect("each running plugin is kept once"),
                None => loaded[k].take().expect("each new one is loaded"),
            };
            slot.plugins.inserts.push(plugin);
            slot.plugins.latencies.push(latency);
        }
        let gone: Vec<Plugin> = old.into_iter().flatten().map(|(p, _)| p).collect();
        let after: u32 = slot.plugins.latencies.iter().sum();
        slot.latency = slot.latency + after - before;
        // What was set on each, where it is now; the new ones' saved settings hold their preset.
        let moved = |w: Which| match w {
            Which::Insert(i) => from.iter().position(|f| *f == Some(i)).map(Which::Insert),
            w => Some(w),
        };
        slot.applied = std::mem::take(&mut slot.applied)
            .into_iter()
            .filter_map(|((w, p), v)| Some(((moved(w)?, p), v)))
            .collect();
        slot.original = std::mem::take(&mut slot.original)
            .into_iter()
            .filter_map(|((w, p), v)| Some(((moved(w)?, p), v)))
            .collect();
        slot.presets = std::mem::take(&mut slot.presets)
            .into_iter()
            .filter_map(|(w, v)| Some((moved(w)?, v)))
            .collect();
        slot.before_preset = std::mem::take(&mut slot.before_preset)
            .into_iter()
            .filter_map(|(w, v)| Some((moved(w)?, v)))
            .collect();
        for (k, f) in from.iter().enumerate() {
            if f.is_none() && inserts[k].state.is_some() {
                slot.presets
                    .insert(Which::Insert(k), inserts[k].preset.clone());
            }
        }
        slot.sound = sound;
        let lane = slot.slot;
        // The editors of those taken out close; the rest follow their plugins.
        self.editors = std::mem::take(&mut self.editors)
            .into_iter()
            .filter_map(|((of, w), e)| match of == id {
                true => Some(((of, moved(w)?), e)),
                false => Some(((of, w), e)),
            })
            .collect();
        self.rearranged += 1;
        self.retired.push((self.rearranged, gone));
        self.send(Msg::Inserts(Box::new(Rearrange {
            slot: lane,
            seq: self.rearranged,
            from: from.to_vec(),
            fresh,
            old: (0..had).map(|_| None).collect(),
            out: Vec::with_capacity(from.len()),
            gone: Vec::with_capacity(had),
        })))?;
        Ok(problems)
    }

    /// Puts a channel's running inserts in a new order (`was[new]` the old place of each), each
    /// as it is: its settings, what Pina Studio set on it, its open editor.
    fn permute(&mut self, id: u64, was: &[usize], sound: Sound) -> Result<(), String> {
        let Some(slot) = self.lanes.get_mut(&id) else {
            return Ok(());
        };
        let moved = |w: Which| match w {
            Which::Insert(i) => Which::Insert(was.iter().position(|&o| o == i).unwrap_or(i)),
            w => w,
        };
        let mut old: Vec<Option<Plugin>> = std::mem::take(&mut slot.plugins.inserts)
            .into_iter()
            .map(Some)
            .collect();
        slot.plugins.inserts = was
            .iter()
            .map(|&i| old[i].take().expect("a permutation takes each once"))
            .collect();
        slot.applied = std::mem::take(&mut slot.applied)
            .into_iter()
            .map(|((w, p), v)| ((moved(w), p), v))
            .collect();
        slot.original = std::mem::take(&mut slot.original)
            .into_iter()
            .map(|((w, p), v)| ((moved(w), p), v))
            .collect();
        slot.presets = std::mem::take(&mut slot.presets)
            .into_iter()
            .map(|(w, v)| (moved(w), v))
            .collect();
        slot.before_preset = std::mem::take(&mut slot.before_preset)
            .into_iter()
            .map(|(w, v)| (moved(w), v))
            .collect();
        slot.sound = sound;
        let lane = slot.slot;
        self.editors = std::mem::take(&mut self.editors)
            .into_iter()
            .map(|((of, w), e)| match of == id {
                true => ((of, moved(w)), e),
                false => ((of, w), e),
            })
            .collect();
        let mut packed = [0u8; MAX_REORDER];
        for (k, &w) in was.iter().enumerate() {
            packed[k] = w as u8;
        }
        self.send(Msg::Reorder {
            slot: lane,
            was: packed,
        })
    }

    fn send(&mut self, msg: Msg) -> Result<(), String> {
        self.tx
            .push(msg)
            .map_err(|_| "the audio engine isn't keeping up".to_string())
    }

    /// How many previews the engine has finished with.
    pub fn auditions_ended(&self) -> u64 {
        self.auditions_ended
    }

    /// Drops what the engine is done with. Call often, e.g. every frame.
    pub fn collect(&mut self) {
        while let Ok(g) = self.garbage.pop() {
            self.auditions_ended += matches!(g, Garbage::Audition(_)) as u64;
            let done = match &g {
                Garbage::Inserts(r) => Some(r.seq),
                _ => None,
            };
            // The processors go first, then their plugins' main-thread halves.
            drop(g);
            if let Some(seq) = done {
                self.retired.retain(|(s, _)| *s > seq);
            }
        }
    }

    pub fn play(&mut self) {
        let _ = self.send(Msg::Play);
    }

    pub fn stop(&mut self) {
        let _ = self.send(Msg::Stop);
    }

    pub fn locate(&mut self, sample: u64) {
        // Where it is now, for whoever asks before the engine's next block says so.
        self.shared
            .position
            .store(sample, std::sync::atomic::Ordering::Relaxed);
        let _ = self.send(Msg::Locate(sample));
    }

    pub fn rate(&self) -> f64 {
        self.rate
    }

    pub fn position(&self) -> u64 {
        self.shared.position()
    }

    pub fn playing(&self) -> bool {
        self.shared.playing()
    }

    /// Sends live notes (MIDI input) to a track's instrument, or nowhere.
    pub fn arm(&self, track: Option<u64>) {
        let slot = track.and_then(|id| self.lanes.get(&id)).map(|s| s.slot);
        // A kit: each note to its pad.
        if let (Some(slot), Some(pads)) = (slot, track.and_then(|id| self.kits.get(&id))) {
            let lanes: Vec<(u8, usize)> = pads
                .iter()
                .filter_map(|&(note, id)| Some((note, self.lanes.get(&id)?.slot)))
                .collect();
            return self.shared.arm_kit(slot, &lanes);
        }
        self.shared.arm(slot);
    }

    /// Plays a note as if from a MIDI keyboard: it goes to the armed track and is reported by
    /// `played`.
    pub fn send_live(&self, note: LiveNote) {
        let _ = self.live.lock().unwrap().notes.push(note);
    }

    /// Moves a controller as if from a MIDI keyboard: on the armed track, reported by `moves`.
    pub fn send_live_move(&self, kind: crate::plugin::ControlKind) {
        let _ = self.live.lock().unwrap().moves.push(kind);
    }

    /// Starts a track's Live Loops cell (its index in the track's cells), or stops it with None,
    /// when the playhead reaches `at` (samples).
    pub fn launch(&mut self, track: u64, cell: Option<usize>, at: u64) {
        if let Some(s) = self.lanes.get(&track) {
            let slot = s.slot;
            let _ = self.send(Msg::Launch { slot, cell, at });
        }
    }

    /// Previews a sample now, over whatever's playing, moved by `semitones` (to hear it in a
    /// song's key) at `gain_db`; it stops by itself at its end, or with `stop_audition`.
    pub fn audition(&mut self, path: &Path, semitones: f32, gain_db: f32) -> Result<(), String> {
        let data = std::sync::Arc::new(crate::audio::engine::load_wav(path, self.rate)?);
        self.audition_audio(data, semitones, gain_db)
    }

    /// Previews audio already read at the player's rate (see `crate::audio::engine::load_audio`), so a
    /// browser can read files off the main thread and keep them.
    pub fn audition_audio(
        &mut self,
        data: std::sync::Arc<AudioData>,
        semitones: f32,
        gain_db: f32,
    ) -> Result<(), String> {
        let a = crate::audio::engine::Audition::new(data, semitones, gain_db);
        self.send(Msg::Audition(Some(Box::new(a))))
    }

    /// Previews audio from `seconds` into it.
    pub fn audition_from(
        &mut self,
        data: std::sync::Arc<AudioData>,
        seconds: f64,
        gain_db: f32,
    ) -> Result<(), String> {
        let a = crate::audio::engine::Audition::new(data, 0.0, gain_db).from(seconds, self.rate);
        self.send(Msg::Audition(Some(Box::new(a))))
    }

    /// Previews a loop already at the song's tempo: while the song plays, in time with it, over
    /// and over.
    pub fn audition_synced(
        &mut self,
        data: std::sync::Arc<AudioData>,
        gain_db: f32,
    ) -> Result<(), String> {
        let a = crate::audio::engine::Audition::new(data, 0.0, gain_db).synced();
        self.send(Msg::Audition(Some(Box::new(a))))
    }

    /// Where a file the project plays has its hits, in frames at the project's rate `at`; and
    /// how long it is.
    pub fn hits(&self, file: &Path, at: f64) -> Option<(Arc<Vec<usize>>, usize)> {
        let to = |f: usize| (f as f64 * at / self.rate).round() as usize;
        let hits = self.files.hits(file)?;
        let frames = to(self.files.frames(file)?);
        Some((Arc::new(hits.iter().map(|&h| to(h)).collect()), frames))
    }

    /// Stretched audio is still being made: what plays stands in for it till it is.
    pub fn rendering(&self) -> bool {
        self.files.rendering()
    }

    /// Part of `file` is still being stretched: a stand-in plays there till it is.
    pub fn stretching(&self, file: &Path) -> bool {
        self.files.stretching(file)
    }

    /// Stretched audio has been made since the last call: `sync` puts it in.
    pub fn fresh(&self) -> bool {
        self.files.fresh()
    }

    pub fn stop_audition(&mut self) {
        let _ = self.send(Msg::Audition(None));
    }

    /// Plays or releases a note on a track now, as when clicking a key in the piano roll.
    pub fn preview(&mut self, track: u64, pitch: u8, velocity: u8) {
        // A kit's note plays its pad.
        if let Some(pad) = self
            .kits
            .get(&track)
            .and_then(|pads| pads.iter().find(|p| p.0 == pitch))
            .map(|p| p.1)
        {
            return self.preview(pad, pitch, velocity);
        }
        if let Some(s) = self.lanes.get(&track) {
            let slot = s.slot;
            let _ = self.send(Msg::Note {
                slot,
                pitch,
                velocity,
            });
        }
    }

    /// Moves a controller on a track now, as the synth window's wheels do.
    pub fn control(&mut self, track: u64, kind: crate::plugin::ControlKind) {
        if let Some(s) = self.lanes.get(&track) {
            let slot = s.slot;
            let _ = self.send(Msg::Control { slot, kind });
        }
    }

    /// Which track's synth the synth's window shows, if any.
    pub fn watch_poly(&self, track: Option<u64>) {
        let slot = track.and_then(|id| self.lanes.get(&id)).map(|s| s.slot);
        self.shared.poly_slot.store(
            slot.unwrap_or(usize::MAX),
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    /// The watched synth's sources now, as `PolySynth::live` gives them.
    pub fn poly_live(&self) -> [f32; crate::dsp::poly::POLY_LIVE] {
        use std::sync::atomic::Ordering;
        std::array::from_fn(|k| f32::from_bits(self.shared.poly_live[k].load(Ordering::Relaxed)))
    }

    /// What the watched synth just played: `out` filled, oldest first, if anything's gone in
    /// since `seen`; how many samples have, ever.
    pub fn poly_scope(&self, out: &mut Vec<f32>, seen: usize) -> usize {
        use crate::audio::engine::POLY_SCOPE_LEN;
        use std::sync::atomic::Ordering;
        let written = self.shared.poly_written.load(Ordering::Relaxed);
        if written == seen {
            return seen;
        }
        out.resize(POLY_SCOPE_LEN, 0.0);
        for (k, v) in out.iter_mut().enumerate() {
            let at = (written + k) % POLY_SCOPE_LEN;
            *v = f32::from_bits(self.shared.poly_scope[at].load(Ordering::Relaxed));
        }
        written
    }

    /// Live notes the engine has played since the last call, at the playhead.
    pub fn played(&mut self) -> Vec<Played> {
        std::iter::from_fn(|| self.played.pop().ok()).collect()
    }

    /// Live controller moves the engine has played since the last call, at the playhead.
    pub fn moves(&mut self) -> Vec<(u64, crate::plugin::ControlKind)> {
        std::iter::from_fn(|| self.heard.pop().ok()).collect()
    }

    /// A track's or aux's peak level since the last call, left and right.
    pub fn meter(&self, track: u64) -> [f32; 2] {
        self.lanes
            .get(&track)
            .map_or([0.0; 2], |s| self.shared.take_meter(s.slot))
    }

    /// A track's modulators' latest values, as the engine last worked them out.
    pub fn modulators(&self, track: u64) -> [f32; crate::dsp::modulate::MAX_MODS] {
        self.lanes
            .get(&track)
            .map_or([0.0; crate::dsp::modulate::MAX_MODS], |s| {
                self.shared.mod_values(s.slot)
            })
    }

    /// The channel whose EQ the EQ window shows, so the engine taps it: a track's or aux's by
    /// id, the master's with `Some(None)`; or none.
    /// Which of the watched channel's EQs the EQ window shows: None its own, else its place
    /// among the channel's other built-ins.
    pub fn watch_eq_more(&self, more: Option<usize>) {
        self.shared.eq_tap.more.store(
            more.unwrap_or(usize::MAX),
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    /// Which of each channel's compressors and limiters report to their meters: None its own,
    /// else the one at that place among its other built-ins.
    pub fn watch_fx_more(&self, more: Option<usize>) {
        self.shared.fx_more.store(
            more.unwrap_or(usize::MAX),
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    /// What the stacked built-in whose window is open just played: `out` filled, oldest first,
    /// each side, if anything's gone in since `seen`; how many samples have, ever.
    pub fn scope(&self, out: &mut [Vec<f32>; 2], seen: usize) -> usize {
        use crate::audio::engine::SCOPE_LEN;
        use std::sync::atomic::Ordering;
        let written = self.shared.scope_written.load(Ordering::Relaxed);
        if written == seen {
            return seen;
        }
        for (s, side) in out.iter_mut().enumerate() {
            side.resize(SCOPE_LEN, 0.0);
            for (k, v) in side.iter_mut().enumerate() {
                let at = (written + k) % SCOPE_LEN;
                *v = f32::from_bits(self.shared.scope[s][at].load(Ordering::Relaxed));
            }
        }
        written
    }

    /// What the stacked built-in whose window is open has to say.
    pub fn tool_reading(&self) -> [f32; 3] {
        use std::sync::atomic::Ordering;
        self.shared
            .reading
            .each_ref()
            .map(|v| f32::from_bits(v.load(Ordering::Relaxed)))
    }

    pub fn watch_eq(&self, channel: Option<Option<u64>>) {
        let slot = match channel {
            None => usize::MAX,
            Some(None) => MASTER,
            Some(Some(id)) => self.lanes.get(&id).map_or(usize::MAX, |s| s.slot),
        };
        self.shared
            .eq_tap
            .slot
            .store(slot, std::sync::atomic::Ordering::Relaxed);
    }

    /// Another channel the EQ window compares its EQ's with (a track's or aux's by id), or none.
    pub fn watch_ext(&self, channel: Option<u64>) {
        let slot = channel
            .and_then(|id| self.lanes.get(&id))
            .map_or(usize::MAX, |s| s.slot);
        self.shared
            .eq_tap
            .ext_slot
            .store(slot, std::sync::atomic::Ordering::Relaxed);
    }

    /// The instance list open or not: every lane's sound is kept for it while it is.
    pub fn watch_lanes(&self, on: bool) {
        let tap = &self.shared.eq_tap;
        tap.lanes_on.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    /// A channel's sound lately (a track's or aux's by id; none, the master's), oldest first;
    /// how many samples it's had ever (0: none yet).
    pub fn lane_sound(
        &self,
        channel: Option<u64>,
        out: &mut [f32; crate::dsp::eq::LANE_LEN],
    ) -> usize {
        let slot = match channel {
            None => crate::audio::engine::MASTER,
            Some(id) => match self.lanes.get(&id) {
                Some(s) => s.slot,
                None => return 0,
            },
        };
        self.shared.eq_tap.read_lane(slot, out)
    }

    /// A band of the watched EQ heard alone (by id) at a listening level (dB), or none.
    pub fn solo_band(&self, band: Option<(u16, f32)>) {
        let tap = &self.shared.eq_tap;
        let (id, db) = band.map_or((0, 0.0), |(id, db)| (u32::from(id), db));
        tap.solo_db
            .store(db.to_bits(), std::sync::atomic::Ordering::Relaxed);
        tap.solo.store(id, std::sync::atomic::Ordering::Relaxed);
    }

    /// What the watched EQ hears and plays.
    pub fn eq_tap(&self) -> &crate::dsp::eq::Tap {
        &self.shared.eq_tap
    }

    /// The master's peak level since the last call, left and right.
    pub fn master_meter(&self) -> [f32; 2] {
        self.shared.take_meter(MASTER)
    }

    /// How far a track's or aux's compressor (or the master's, with None) turned it down since the
    /// last call, in dB.
    pub fn reduction(&self, track: Option<u64>) -> f32 {
        match track {
            Some(id) => self
                .lanes
                .get(&id)
                .map_or(0.0, |s| self.shared.take_reduction(s.slot)),
            None => self.shared.take_reduction(MASTER),
        }
    }

    /// The loudest sample into and out of a track's or aux's compressor (the master's, with
    /// None) since the last call.
    pub fn comp_levels(&self, track: Option<u64>) -> [f32; 2] {
        match track {
            Some(id) => self
                .lanes
                .get(&id)
                .map_or([0.0; 2], |s| self.shared.take_comp(s.slot)),
            None => self.shared.take_comp(MASTER),
        }
    }

    /// A track's or aux's limiter's meter (the master's, with None) since the last call.
    pub fn limit_meter(&self, track: Option<u64>) -> crate::dsp::limit::LimitMeter {
        match track {
            Some(id) => self
                .lanes
                .get(&id)
                .map_or_else(Default::default, |s| self.shared.take_limit(s.slot)),
            None => self.shared.take_limit(MASTER),
        }
    }

    /// Starts a track's or aux's limiter's loudness (the master's, with None) over.
    pub fn reset_loudness(&self, track: Option<u64>) {
        match track {
            Some(id) => {
                if let Some(s) = self.lanes.get(&id) {
                    self.shared.reset_loudness(s.slot);
                }
            }
            None => self.shared.reset_loudness(MASTER),
        }
    }

    /// The busiest the audio thread has been since the last call: 1 is all of its time.
    pub fn load(&self) -> f32 {
        self.shared.take_load()
    }

    /// The output device's buffer, in frames; 0 before it's asked for one.
    pub fn buffer(&self) -> u32 {
        self.shared.buffer()
    }

    pub fn set_metronome(&self, on: bool) {
        self.shared.set_metronome(on);
    }

    pub fn metronome(&self) -> bool {
        self.shared.metronome()
    }

    /// A plugin's parameters, as its controller has them now.
    pub fn params(&self, id: u64, which: Which) -> Vec<PluginParam> {
        self.plugin(id, which).map_or(Vec::new(), |p| p.params())
    }

    /// Sets a plugin's parameter (0 to 1), as if turned in its editor.
    pub fn set_param(&self, id: u64, which: Which, param: u32, value: f64) {
        if let Some(p) = self.plugin(id, which) {
            p.set_param(param, value);
        }
    }

    /// Shows a parameter's value (0 to 1) in the plugin's controller and editor, without sending
    /// it to the sound: for automation, which the engine plays itself.
    pub fn show_param(&self, id: u64, which: Which, param: u32, value: f64) {
        if let Some(p) = self.plugin(id, which) {
            p.show_param(param, value);
        }
    }

    /// A plugin parameter's value now, from 0 to 1.
    pub fn param_value(&self, id: u64, which: Which, param: u32) -> f64 {
        self.plugin(id, which).map_or(0.0, |p| p.param_value(param))
    }

    /// A parameter's value as the plugin shows it, units and all.
    pub fn param_text(&self, id: u64, which: Which, param: u32, value: f64) -> String {
        self.plugin(id, which)
            .map_or(String::new(), |p| p.param_text(param, value))
    }

    /// The waveform overview of an audio file in the project.
    pub fn peaks(&self, file: &Path) -> Option<Arc<Peaks>> {
        self.files.peaks(file)
    }
}

#[cfg(test)]
mod tests;
