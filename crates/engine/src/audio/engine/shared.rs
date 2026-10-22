//! What the audio thread and the rest of the app share: messages, meters, the metronome, auditions, and what's thrown away off the audio thread.

use super::*;

/// From the main thread to the live engine.
pub enum Msg {
    Arrange(Box<Arrangement>),
    Insert(usize, Box<Lane>),
    Remove(usize),
    Play,
    Stop,
    Locate(u64),
    /// Start a lane's Live Loops cell (or stop it, with None) at a sample: its loop begins there.
    Launch {
        slot: usize,
        cell: Option<usize>,
        at: u64,
    },
    /// Play or release a note on a lane now, as when clicking a key in the piano roll.
    Note {
        slot: usize,
        pitch: u8,
        velocity: u8,
    },
    /// Play a sample now, over everything, as a browser previews it; or stop the one playing.
    Audition(Option<Box<Audition>>),
}

/// A sample being previewed: its audio, where it's got to, and how fast it's read (2 is an
/// octave up).
pub struct Audition {
    pub(crate) data: Arc<AudioData>,
    pub(crate) pos: f64,
    pub(crate) step: f64,
    pub(crate) gain: f32,
    /// A loop at the song's tempo: while the song plays it plays in time with it (from where
    /// the song is in it, over and over).
    pub(crate) synced: bool,
    pub(crate) started: bool,
}

impl Audition {
    pub fn new(data: Arc<AudioData>, semitones: f32, gain_db: f32) -> Audition {
        Audition {
            data,
            pos: 0.0,
            step: 2f64.powf(semitones as f64 / 12.0),
            gain: 10f32.powf(gain_db / 20.0),
            synced: false,
            started: false,
        }
    }

    /// Plays in time with the song while it plays, as a loop at its tempo does.
    pub fn synced(mut self) -> Audition {
        self.synced = true;
        self
    }

    pub(crate) fn done(&self) -> bool {
        self.pos as usize + 1 >= self.data.channels.first().map_or(0, Vec::len)
    }
}

/// What the live engine lets go of, for the main thread to drop.
pub enum Garbage {
    Arrangement(Box<Arrangement>),
    Lane(Box<Lane>),
    Audition(Box<Audition>),
}

/// What the live engine reports.
pub struct Shared {
    pub(crate) position: AtomicU64,
    pub(crate) playing: AtomicBool,
    /// The loudest sample (as f32 bits) per lane since last read, left then right; the master's
    /// last.
    pub(crate) meters: Vec<AtomicU32>,
    /// The most each lane's compressor (and the master's, last) turned down since last read, in
    /// dB as f32 bits.
    pub(crate) reduction: Vec<AtomicU32>,
    /// The lane live notes go to, or `usize::MAX` for none.
    pub(crate) armed: AtomicUsize,
    /// The busiest block since last read, as the share of its time spent working (f32 bits).
    pub(crate) load: AtomicU32,
    /// Frames the output device asks for at a time.
    pub(crate) buffer: AtomicU32,
    /// Click on every beat while playing.
    pub(crate) metronome: AtomicBool,
    /// Each lane's modulators' latest values (f32 bits), `MAX_MODS` a lane.
    pub(crate) mods: Vec<AtomicU32>,
    /// The EQ the EQ window shows: what it hears and plays, for its analyzer.
    pub eq_tap: crate::dsp::eq::Tap,
}

impl Shared {
    pub fn new() -> Self {
        Shared {
            position: AtomicU64::new(0),
            playing: AtomicBool::new(false),
            meters: (0..(SLOTS + 1) * 2).map(|_| AtomicU32::new(0)).collect(),
            reduction: (0..=SLOTS).map(|_| AtomicU32::new(0)).collect(),
            armed: AtomicUsize::new(usize::MAX),
            load: AtomicU32::new(0),
            buffer: AtomicU32::new(0),
            metronome: AtomicBool::new(false),
            mods: (0..SLOTS * MAX_MODS).map(|_| AtomicU32::new(0)).collect(),
            eq_tap: crate::dsp::eq::Tap::new(),
        }
    }

    /// Records a block's peaks and gain reduction on a lane (or `MASTER`).
    pub(crate) fn report(&self, slot: usize, peak: [f32; 2], reduction: f32) {
        self.meters[slot * 2].fetch_max(peak[0].to_bits(), Ordering::Relaxed);
        self.meters[slot * 2 + 1].fetch_max(peak[1].to_bits(), Ordering::Relaxed);
        self.reduction[slot].fetch_max(reduction.to_bits(), Ordering::Relaxed);
    }

    pub(crate) fn report_mod(&self, slot: usize, k: usize, value: f32) {
        if let Some(m) = self.mods.get(slot * MAX_MODS + k) {
            m.store(value.to_bits(), Ordering::Relaxed);
        }
    }

    /// A lane's modulators' latest values.
    pub fn mod_values(&self, slot: usize) -> [f32; MAX_MODS] {
        std::array::from_fn(|k| {
            self.mods
                .get(slot * MAX_MODS + k)
                .map_or(0.0, |m| f32::from_bits(m.load(Ordering::Relaxed)))
        })
    }

    /// The gain reduction on a lane (or `MASTER`) since the last call, in dB.
    pub fn take_reduction(&self, slot: usize) -> f32 {
        f32::from_bits(self.reduction[slot].swap(0, Ordering::Relaxed))
    }

    /// The busiest the audio thread has been since the last call: 1 is all of its time.
    pub fn take_load(&self) -> f32 {
        f32::from_bits(self.load.swap(0, Ordering::Relaxed))
    }

    /// The output device's buffer, in frames; 0 before it's asked for one.
    pub fn buffer(&self) -> u32 {
        self.buffer.load(Ordering::Relaxed)
    }

    pub fn set_metronome(&self, on: bool) {
        self.metronome.store(on, Ordering::Relaxed);
    }

    pub fn metronome(&self) -> bool {
        self.metronome.load(Ordering::Relaxed)
    }

    pub fn position(&self) -> u64 {
        self.position.load(Ordering::Relaxed)
    }

    pub fn playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    /// Sends live notes to a lane, or nowhere.
    pub fn arm(&self, slot: Option<usize>) {
        self.armed
            .store(slot.unwrap_or(usize::MAX), Ordering::Relaxed);
    }

    /// The peak level on a lane (or `MASTER`) since the last call, left and right.
    pub fn take_meter(&self, slot: usize) -> [f32; 2] {
        let take = |i: usize| f32::from_bits(self.meters[i].swap(0, Ordering::Relaxed));
        [take(slot * 2), take(slot * 2 + 1)]
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

/// The live engine's connections to the main thread and to MIDI input.
pub struct Link {
    pub rx: rtrb::Consumer<Msg>,
    pub garbage: rtrb::Producer<Garbage>,
    pub shared: Arc<Shared>,
    pub live: rtrb::Consumer<LiveNote>,
    pub played: rtrb::Producer<Played>,
}

/// Hands garbage back, or leaks it if the main thread has stopped collecting: freeing it here
/// could stall the audio thread.
pub(crate) fn trash(q: &mut rtrb::Producer<Garbage>, g: Garbage) {
    if let Err(rtrb::PushError::Full(g)) = q.push(g) {
        std::mem::forget(g);
    }
}
