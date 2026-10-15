//! The live engine, from the main thread: plays through the output device and follows the
//! project as it's edited, keeping each track's instrument and plugins through every edit.

use std::{collections::HashMap, path::Path, sync::Arc};

use crate::{
    engine::{
        AudioFiles, Engine, Garbage, MASTER, Modules, Msg, SLOTS, Shared, compile, make_lane,
    },
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
    _stream: cpal::Stream,
}

impl Player {
    /// Starts the output at the project's sample rate, empty: `sync` loads the project.
    pub fn start(project: &Project, dir: &Path) -> Result<Player, String> {
        let rate = project.sample_rate as f64;
        let (tx, rx) = rtrb::RingBuffer::new(1024);
        let (gtx, garbage) = rtrb::RingBuffer::new(1024);
        let shared = Arc::new(Shared::new());
        let stream = Engine::live(rate, rx, gtx, shared.clone()).start_output()?;
        Ok(Player {
            rate,
            tx,
            garbage,
            shared,
            lanes: HashMap::new(),
            free: (0..SLOTS).rev().collect(),
            modules: Modules::new(),
            files: AudioFiles::new(dir, rate),
            _stream: stream,
        })
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
