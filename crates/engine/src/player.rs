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
    engine::{
        AudioData, AudioFiles, Engine, Garbage, LanePlugins, Link, LiveNote, MASTER, MAX_BLOCK,
        Modules, Msg, Played, SLOTS, Shared, compile, make_lane,
    },
    midi::{self, LiveInput},
    peaks::Peaks,
    plugin::{Editor, Plugin, PluginParam},
    project::{Instrument, PluginRef, Project, STATES_DIR},
};

/// A lane: which slot it's in, what it was built from, its latency, and its plugins'
/// main-thread halves.
struct Slot {
    slot: usize,
    sound: Sound,
    latency: u32,
    plugins: LanePlugins,
    /// Parameters the project sets, as last applied; and what each was before tempo first set
    /// it, to go back to when the project stops setting it (as when that edit is undone).
    applied: HashMap<(Which, u32), f64>,
    original: HashMap<(Which, u32), f64>,
    /// The preset each plugin has loaded, as last applied; and its settings from before tempo
    /// first loaded one, to go back to.
    presets: HashMap<Which, Option<crate::project::Preset>>,
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
            // The preset set every parameter: what tempo set before is set again over it.
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
type Sound = (Option<String>, Vec<(PathBuf, String)>);

fn sound(instrument: Option<&Instrument>, inserts: &[PluginRef]) -> Sound {
    let key = |r: &PluginRef| (r.path.clone(), r.class.clone());
    let instrument = instrument.map(|i| match i {
        Instrument::Synth => "synth".to_string(),
        Instrument::Drums => "drums".to_string(),
        // A sampler is rebuilt when its samples or their settings change.
        Instrument::Sampler(s) => format!("sampler {:?}", s.zones),
        Instrument::Plugin(r) => format!("{}#{}", r.path.display(), r.class),
    });
    (instrument, inserts.iter().map(key).collect())
}

pub use crate::project::Which;

pub struct Player {
    rate: f64,
    tx: rtrb::Producer<Msg>,
    garbage: rtrb::Consumer<Garbage>,
    shared: Arc<Shared>,
    /// By track id.
    lanes: HashMap<u64, Slot>,
    free: Vec<usize>,
    modules: Modules,
    files: AudioFiles,
    _stream: Option<cpal::Stream>,
    live: LiveInput,
    played: rtrb::Consumer<Played>,
    _midi: Vec<midir::MidiInputConnection<()>>,
    dir: PathBuf,
    /// Open plugin editors, by track or aux id.
    editors: HashMap<(u64, Which), Editor>,
}

impl Drop for Player {
    fn drop(&mut self) {
        // Editors close before their plugins go, and the audio stops (dropping the engine and its
        // processors) before the plugins' main-thread halves do.
        self.editors.clear();
        self._stream = None;
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
        let link = Link {
            rx,
            garbage: gtx,
            shared: shared.clone(),
            live,
            played: played_tx,
        };
        let engine = Engine::live(rate, link);
        let player = Player {
            rate,
            tx,
            garbage,
            shared,
            live: Arc::new(Mutex::new(live_tx)),
            played,
            _midi: Vec::new(),
            lanes: HashMap::new(),
            free: (0..SLOTS).rev().collect(),
            modules: Modules::new(),
            files: AudioFiles::new(dir, rate).in_background(),
            _stream: None,
            dir: dir.to_path_buf(),
            editors: HashMap::new(),
        };
        (player, engine)
    }

    /// Plays through the default output device, with every MIDI input playing the armed track.
    pub fn start(project: &Project, dir: &Path) -> Result<Player, String> {
        // At the project's rate if the output can run at it, else at the output's.
        let rate = crate::engine::output_rate(project.sample_rate)?;
        let (mut player, engine) = Player::new(dir, rate as f64);
        player._stream = Some(engine.start_output()?);
        player._midi = midi::connect(&player.live);
        Ok(player)
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
        let stale: Vec<u64> = self
            .lanes
            .iter()
            .filter(|(id, s)| current.get(id) != Some(&s.sound))
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            // Editors close before their plugins go; the engine lets the processors go after.
            self.editors.retain(|(of, _), _| *of != id);
            let s = self.lanes.remove(&id).unwrap();
            self.send(Msg::Remove(s.slot))?;
            self.free.push(s.slot);
        }
        let mut problems = Vec::new();
        for &(id, instrument, inserts) in &channels {
            if self.lanes.contains_key(&id) {
                continue;
            }
            let slot = self.free.pop().ok_or("too many tracks")?;
            // A plugin that won't load leaves its track silent rather than stopping the rest.
            let made = make_lane(instrument, inserts, self.rate, &mut self.modules, &self.dir);
            let (lane, plugins, latency) = made.unwrap_or_else(|e| {
                problems.push(e);
                let empty = make_lane(None, &[], self.rate, &mut self.modules, &self.dir);
                empty.expect("a lane without plugins always builds")
            });
            self.send(Msg::Insert(slot, Box::new(lane)))?;
            let sound = sound(instrument, inserts);
            // A preset its saved settings hold is loaded already.
            let refs = match instrument {
                Some(Instrument::Plugin(r)) => vec![(Which::Instrument, r)],
                _ => Vec::new(),
            };
            let all = refs.into_iter().chain(
                inserts
                    .iter()
                    .enumerate()
                    .map(|(i, r)| (Which::Insert(i), r)),
            );
            let presets = all
                .filter(|(_, r)| r.state.is_some())
                .map(|(w, r)| (w, r.preset.clone()))
                .collect();
            self.lanes.insert(
                id,
                Slot {
                    slot,
                    sound,
                    latency,
                    plugins,
                    applied: HashMap::new(),
                    original: HashMap::new(),
                    presets,
                    before_preset: HashMap::new(),
                },
            );
        }
        for &(id, instrument, inserts) in &channels {
            let slot = self.lanes.get_mut(&id).unwrap();
            if let Some(Instrument::Plugin(r)) = instrument
                && let Err(e) = slot.apply(Which::Instrument, r)
            {
                problems.push(e);
            }
            for (i, r) in inserts.iter().enumerate() {
                if let Err(e) = slot.apply(Which::Insert(i), r) {
                    problems.push(e);
                }
            }
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
        let mtm = objc2::MainThreadMarker::new().ok_or("editors open on the main thread")?;
        let plugin = self.plugin(id, which).ok_or("no plugin there")?;
        let editor = plugin.open_editor(mtm)?;
        self.editors.insert((id, which), editor);
        Ok(())
    }

    /// Writes every plugin's settings into `Plugin States/` and points the project at them, so
    /// saving the project saves them. Files no longer used are removed.
    pub fn save_states(&self, project: &mut Project) -> Result<(), String> {
        let dir = self.dir.join(STATES_DIR);
        let err = |e: std::io::Error| format!("{}: {e}", dir.display());
        std::fs::create_dir_all(&dir).map_err(err)?;
        let mut kept = HashSet::new();
        let mut save = |r: &mut PluginRef, p: &Plugin, name: String| -> Result<(), String> {
            // What tempo set may since have been turned in the plugin's own editor.
            for (&id, v) in r.params.iter_mut() {
                *v = p.param_value(id);
            }
            if let Some(bytes) = p.state() {
                let file = PathBuf::from(STATES_DIR).join(name);
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

    fn send(&mut self, msg: Msg) -> Result<(), String> {
        self.tx
            .push(msg)
            .map_err(|_| "the audio engine isn't keeping up".to_string())
    }

    /// Drops what the engine is done with. Call often, e.g. every frame.
    pub fn collect(&mut self) {
        while self.garbage.pop().is_ok() {}
    }

    pub fn play(&mut self) {
        let _ = self.send(Msg::Play);
    }

    pub fn stop(&mut self) {
        let _ = self.send(Msg::Stop);
    }

    pub fn locate(&mut self, sample: u64) {
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
        self.shared.arm(slot);
    }

    /// Plays a note as if from a MIDI keyboard: it goes to the armed track and is reported by
    /// `played`.
    pub fn send_live(&self, note: LiveNote) {
        let _ = self.live.lock().unwrap().push(note);
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
        let data = std::sync::Arc::new(crate::engine::load_wav(path, self.rate)?);
        self.audition_audio(data, semitones, gain_db)
    }

    /// Previews audio already read at the player's rate (see `crate::engine::load_audio`), so a
    /// browser can read files off the main thread and keep them.
    pub fn audition_audio(
        &mut self,
        data: std::sync::Arc<AudioData>,
        semitones: f32,
        gain_db: f32,
    ) -> Result<(), String> {
        let a = crate::engine::Audition::new(data, semitones, gain_db);
        self.send(Msg::Audition(Some(Box::new(a))))
    }

    /// Previews a loop already at the song's tempo: while the song plays, in time with it, over
    /// and over.
    pub fn audition_synced(
        &mut self,
        data: std::sync::Arc<AudioData>,
        gain_db: f32,
    ) -> Result<(), String> {
        let a = crate::engine::Audition::new(data, 0.0, gain_db).synced();
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

    /// Stretched audio has been made since the last call: `sync` puts it in.
    pub fn fresh(&self) -> bool {
        self.files.fresh()
    }

    pub fn stop_audition(&mut self) {
        let _ = self.send(Msg::Audition(None));
    }

    /// Plays or releases a note on a track now, as when clicking a key in the piano roll.
    pub fn preview(&mut self, track: u64, pitch: u8, velocity: u8) {
        if let Some(s) = self.lanes.get(&track) {
            let slot = s.slot;
            let _ = self.send(Msg::Note {
                slot,
                pitch,
                velocity,
            });
        }
    }

    /// Live notes the engine has played since the last call, at the playhead.
    pub fn played(&mut self) -> Vec<Played> {
        std::iter::from_fn(|| self.played.pop().ok()).collect()
    }

    /// A track's or aux's peak level since the last call, left and right.
    pub fn meter(&self, track: u64) -> [f32; 2] {
        self.lanes
            .get(&track)
            .map_or([0.0; 2], |s| self.shared.take_meter(s.slot))
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
mod tests {
    use super::*;
    use crate::project::Track;

    /// With a real instrument, which CI doesn't have:
    /// `TEMPO_TEST_VST3="/path/Some Synth.vst3" cargo test -p tempo-engine -- --ignored`
    #[test]
    #[ignore]
    fn plugin_settings_are_saved_with_the_project_and_loaded_back() {
        let path = std::env::var("TEMPO_TEST_VST3").expect("set TEMPO_TEST_VST3");
        let class = crate::scan::inspect(Path::new(&path))
            .unwrap()
            .remove(0)
            .class;
        let dir = std::env::temp_dir().join(format!("tempo-test-states-{}", std::process::id()));
        let plugin = PluginRef {
            path: path.into(),
            class,
            state: None,
            ..Default::default()
        };
        let mut project = Project::new(48_000);
        project.add_track(
            0,
            Track {
                name: "Synth".into(),
                instrument: Some(Instrument::Plugin(plugin)),
                ..Default::default()
            },
        );
        let mut player = Player::silent(&project, &dir);
        player.sync(&project).unwrap();
        player.save_states(&mut project).unwrap();
        let Some(Instrument::Plugin(r)) = &project.tracks[0].instrument else {
            unreachable!()
        };
        let file = r.state.clone().expect("a state file");
        assert!(dir.join(&file).exists());

        // A parameter the project sets is applied; when it stops setting it (an undo), it goes
        // back to what it was.
        let id = project.tracks[0].id;
        let param = player.params(id, Which::Instrument)[0].clone();
        let set = |project: &mut Project, v: Option<f64>| {
            let r = project.tracks[0].plugin_mut(Which::Instrument).unwrap();
            match v {
                Some(v) => drop(r.params.insert(param.id, v)),
                None => drop(r.params.remove(&param.id)),
            }
        };
        let other = if param.value > 0.5 { 0.1 } else { 0.9 };
        set(&mut project, Some(other));
        player.sync(&project).unwrap();
        assert!((player.param_value(id, Which::Instrument, param.id) - other).abs() < 0.02);
        set(&mut project, None);
        player.sync(&project).unwrap();
        let back = player.param_value(id, Which::Instrument, param.id);
        assert!((back - param.value).abs() < 0.02, "{back} {}", param.value);
        // A bounce of the saved project loads it.
        project.save(&dir).unwrap();
        let loaded = Project::load(&dir).unwrap();
        assert!(Engine::load(&loaded, &dir, 48_000.0).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
