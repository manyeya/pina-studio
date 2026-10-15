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
        AudioFiles, Engine, Garbage, LanePlugins, Link, LiveNote, MASTER, MAX_BLOCK, Modules, Msg,
        Played, SLOTS, Shared, compile, make_lane,
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
}

/// What makes a lane's sound, settings aside: its instrument ("synth", "drums" or a plugin) and
/// its inserts.
type Sound = (Option<String>, Vec<(PathBuf, String)>);

fn sound(instrument: Option<&Instrument>, inserts: &[PluginRef]) -> Sound {
    let key = |r: &PluginRef| (r.path.clone(), r.class.clone());
    let instrument = instrument.map(|i| match i {
        Instrument::Synth => "synth".to_string(),
        Instrument::Drums => "drums".to_string(),
        Instrument::Vst3(r) => format!("{}#{}", r.path.display(), r.class),
    });
    (instrument, inserts.iter().map(key).collect())
}

/// A plugin on a track or aux.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Which {
    Instrument,
    Insert(usize),
}

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

impl Player {
    /// A player and the engine it drives, at the project's sample rate. Empty: `sync` loads the
    /// project.
    fn new(project: &Project, dir: &Path) -> (Player, Engine) {
        let rate = project.sample_rate as f64;
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
            files: AudioFiles::new(dir, rate),
            _stream: None,
            dir: dir.to_path_buf(),
            editors: HashMap::new(),
        };
        (player, engine)
    }

    /// Plays through the default output device, with every MIDI input playing the armed track.
    pub fn start(project: &Project, dir: &Path) -> Result<Player, String> {
        let (mut player, engine) = Player::new(project, dir);
        player._stream = Some(engine.start_output()?);
        player._midi = midi::connect(&player.live);
        Ok(player)
    }

    /// Runs the engine in real time on a thread of its own, heard by nobody: for tests, and
    /// machines without audio. The thread ends with the player.
    pub fn silent(project: &Project, dir: &Path) -> Player {
        let (player, mut engine) = Player::new(project, dir);
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
            self.lanes.insert(
                id,
                Slot {
                    slot,
                    sound,
                    latency,
                    plugins,
                },
            );
        }
        let lane = |id: &u64| (self.lanes[id].slot, self.lanes[id].latency);
        let tracks: Vec<_> = project.tracks.iter().map(|t| lane(&t.id)).collect();
        let auxes: Vec<_> = project.auxes.iter().map(|a| lane(&a.id)).collect();
        let (arrangement, missing) = compile(project, self.rate, &tracks, &auxes, &mut self.files);
        problems.extend(missing);
        self.send(Msg::Arrange(Box::new(arrangement)))?;
        match problems.is_empty() {
            true => Ok(()),
            false => Err(problems.join("; ")),
        }
    }

    fn plugin(&self, id: u64, which: Which) -> Option<&Plugin> {
        let plugins = &self.lanes.get(&id)?.plugins;
        match which {
            Which::Instrument => plugins.instrument.as_ref(),
            Which::Insert(i) => plugins.inserts.get(i),
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
            if let (Some(Instrument::Vst3(r)), Some(p)) =
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
        self.plugin(id, which).map_or(Vec::new(), Plugin::params)
    }

    /// Sets a plugin's parameter (0 to 1), as if turned in its editor.
    pub fn set_param(&self, id: u64, which: Which, param: u32, value: f64) {
        if let Some(p) = self.plugin(id, which) {
            p.set_param(param, value);
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
        };
        let mut project = Project::new(48_000);
        project.add_track(
            0,
            Track {
                name: "Synth".into(),
                instrument: Some(Instrument::Vst3(plugin)),
                ..Default::default()
            },
        );
        let mut player = Player::silent(&project, &dir);
        player.sync(&project).unwrap();
        player.save_states(&mut project).unwrap();
        let Some(Instrument::Vst3(r)) = &project.tracks[0].instrument else {
            unreachable!()
        };
        let file = r.state.clone().expect("a state file");
        assert!(dir.join(&file).exists());
        // A bounce of the saved project loads it.
        project.save(&dir).unwrap();
        let loaded = Project::load(&dir).unwrap();
        assert!(Engine::load(&loaded, &dir, 48_000.0).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
