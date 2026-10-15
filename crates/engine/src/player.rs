//! The live engine, from the main thread: plays through the output device and follows the
//! project as it's edited, keeping each track's instrument and plugins through every edit.

use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use crate::{
    engine::{
        AudioFiles, Engine, Garbage, Link, LiveNote, MASTER, MAX_BLOCK, Modules, Msg, Played,
        SLOTS, Shared, compile, make_lane,
    },
    midi::{self, LiveInput},
    peaks::Peaks,
    plugin::Plugin,
    project::{Instrument, PluginRef, Project, Track},
};

/// A track's lane: which slot it's in, what it was built from, and its plugins' main-thread
/// halves.
struct Slot {
    slot: usize,
    sound: (Option<Instrument>, Vec<PluginRef>),
    _plugins: Vec<Plugin>,
}

fn sound(t: &Track) -> (Option<Instrument>, Vec<PluginRef>) {
    (t.instrument.clone(), t.inserts.clone())
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

    /// Brings the engine up to date with `project`: builds lanes for new tracks (and for tracks
    /// whose instrument or inserts changed), drops lanes of deleted tracks, and swaps in what
    /// every track plays.
    pub fn sync(&mut self, project: &Project) -> Result<(), String> {
        self.collect();
        let current: HashMap<u64, &Track> = project.tracks.iter().map(|t| (t.id, t)).collect();
        let stale: Vec<u64> = self
            .lanes
            .iter()
            .filter(|(id, s)| current.get(id).is_none_or(|t| sound(t) != s.sound))
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            let s = self.lanes.remove(&id).unwrap();
            self.send(Msg::Remove(s.slot))?;
            self.free.push(s.slot);
        }
        for t in &project.tracks {
            if self.lanes.contains_key(&t.id) {
                continue;
            }
            let slot = self.free.pop().ok_or("too many tracks")?;
            let (lane, plugins) = make_lane(t, self.rate, &mut self.modules).inspect_err(|_| {
                self.free.push(slot);
            })?;
            self.send(Msg::Insert(slot, Box::new(lane)))?;
            let sound = sound(t);
            self.lanes.insert(
                t.id,
                Slot {
                    slot,
                    sound,
                    _plugins: plugins,
                },
            );
        }
        let slots: Vec<usize> = project
            .tracks
            .iter()
            .map(|t| self.lanes[&t.id].slot)
            .collect();
        let arrangement = compile(project, self.rate, &slots, &mut self.files)?;
        self.send(Msg::Arrange(Box::new(arrangement)))
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

    /// A track's peak level since the last call.
    pub fn meter(&self, track: u64) -> f32 {
        self.lanes
            .get(&track)
            .map_or(0.0, |s| self.shared.take_meter(s.slot))
    }

    /// The master's peak level since the last call.
    pub fn master_meter(&self) -> f32 {
        self.shared.take_meter(MASTER)
    }

    /// The waveform overview of an audio file in the project.
    pub fn peaks(&self, file: &Path) -> Option<Arc<Peaks>> {
        self.files.peaks(file)
    }
}
