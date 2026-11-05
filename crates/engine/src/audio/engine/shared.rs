//! What the audio thread and the rest of the app share: messages, meters, the metronome, auditions, and what's thrown away off the audio thread.

use super::*;
use crate::dsp::limit::{LimitMeter, Limiting};

/// What each lane's limiter reports: see `Shared::limit`.
const LIMIT_FIELDS: usize = 7;

/// From the main thread to the live engine.
/// The most inserts a lane can have put in a new order in place (more are loaded afresh).
pub const MAX_REORDER: usize = 64;

pub enum Msg {
    Arrange(Box<Arrangement>),
    Insert(usize, Box<Lane>),
    Remove(usize),
    /// A lane's inserts put in a new order, each kept as it is: `was[new]` the old place of each.
    Reorder {
        slot: usize,
        was: [u8; MAX_REORDER],
    },
    /// A lane's inserts changed in place: those kept run on as they are, new ones go in, and
    /// those taken out come back to be dropped off the audio thread.
    Inserts(Box<Rearrange>),
    /// More running state for a lane's other built-ins.
    Strips(Box<Strips>),
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
    /// A controller moved on a lane now, as the synth window's wheels do.
    Control {
        slot: usize,
        kind: crate::plugin::ControlKind,
    },
    /// Play a sample now, over everything, as a browser previews it; or stop the one playing.
    Audition(Option<Box<Audition>>),
    /// A lane's plugin instrument to hear each note on a channel of its own (MPE), or not.
    Mpe {
        slot: usize,
        on: bool,
    },
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

    /// Starts `seconds` into the audio, at the rate it was read at.
    pub fn from(mut self, seconds: f64, rate: f64) -> Audition {
        self.pos = (seconds * rate).max(0.0);
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
    Inserts(Box<Rearrange>),
    Strips(Box<Strips>),
    Audition(Box<Audition>),
}

/// Running state for a lane's other built-ins as it's to be, made off the audio thread: for
/// each place, a new strip (`fresh`), or (none) the one the lane has there. Room for everything
/// is made here (`old` an empty place for each it has, `out` the new list, `gone` those
/// replaced), so taking them in allocates nothing.
pub struct Strips {
    pub(crate) slot: usize,
    pub(crate) fresh: Vec<Option<crate::dsp::Strip>>,
    pub(crate) old: Vec<Option<crate::dsp::Strip>>,
    pub(crate) out: Vec<crate::dsp::Strip>,
    pub(crate) gone: Vec<crate::dsp::Strip>,
}

impl Strips {
    /// What the player sends: `fresh` for each place (None keeps the lane's), and room for the
    /// `had` the lane has.
    pub fn new(slot: usize, fresh: Vec<Option<crate::dsp::Strip>>, had: usize) -> Self {
        let n = fresh.len();
        Strips {
            slot,
            fresh,
            old: Vec::with_capacity(had),
            out: Vec::with_capacity(n.max(had)),
            gone: Vec::with_capacity(had),
        }
    }

    /// The lane's `pool` made as it's to be; what it replaces waits in `gone`.
    pub(crate) fn apply(&mut self, pool: &mut Vec<crate::dsp::Strip>) {
        for s in pool.drain(..) {
            match self.old.len() < self.old.capacity() {
                true => self.old.push(Some(s)),
                false => self.gone.push(s),
            }
        }
        for k in 0..self.fresh.len() {
            let s = match self.fresh[k].take() {
                Some(s) => {
                    if let Some(o) = self.old.get_mut(k).and_then(Option::take) {
                        self.gone.push(o);
                    }
                    Some(s)
                }
                None => self.old.get_mut(k).and_then(Option::take),
            };
            if let Some(s) = s.filter(|_| self.out.len() < self.out.capacity()) {
                self.out.push(s);
            }
        }
        // Any the lane had past the new list's end stay, after it.
        for o in self.old.iter_mut() {
            if let Some(s) = o.take() {
                match self.out.len() < self.out.capacity() {
                    true => self.out.push(s),
                    false => self.gone.push(s),
                }
            }
        }
        std::mem::swap(pool, &mut self.out);
    }
}

/// A lane's inserts as they're to be: for each place, the old insert kept there, or (none) the
/// new one in `fresh`. Everything the audio thread needs is made here, so it allocates nothing:
/// `old` (one empty place for each insert it has), `out` (room for the new chain) and `gone`
/// (room for those taken out).
pub struct Rearrange {
    pub(crate) slot: usize,
    /// Which rearrangement it is: the player's count.
    pub(crate) seq: u64,
    pub(crate) from: Vec<Option<usize>>,
    pub(crate) fresh: Vec<Option<Processor>>,
    pub(crate) old: Vec<Option<Processor>>,
    pub(crate) out: Vec<Processor>,
    pub(crate) gone: Vec<Processor>,
}

impl Rearrange {
    /// The new chain made from `inserts` (the lane's, emptied): each one kept goes to its new
    /// place; the rest wait in `gone`.
    pub(crate) fn apply(&mut self, inserts: &mut Vec<Processor>) {
        for (i, p) in inserts.drain(..).enumerate() {
            match self.old.get_mut(i) {
                Some(place) => *place = Some(p),
                None => self.gone.push(p),
            }
        }
        for (k, from) in self.from.iter().enumerate() {
            let p = match from {
                Some(j) => self.old.get_mut(*j).and_then(Option::take),
                None => self.fresh.get_mut(k).and_then(Option::take),
            };
            self.out.extend(p);
        }
        self.gone
            .extend(self.old.iter_mut().filter_map(Option::take));
        std::mem::swap(inserts, &mut self.out);
    }
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
    /// The loudest sample into and out of each lane's compressor (the master's last) since last
    /// read, as f32 bits.
    pub(crate) comp_levels: Vec<AtomicU32>,
    /// Each lane's limiter's meter (the master's last), `LIMIT_FIELDS` a lane, as f32 bits:
    /// its peaks in and out and the most it turned down since last read, then its loudness.
    pub(crate) limit: Vec<AtomicU32>,
    /// Each lane's limiter's loudness to start over (the master's last).
    pub(crate) loudness_reset: Vec<AtomicBool>,
    /// The lane live notes go to, or `usize::MAX` for none.
    pub(crate) armed: AtomicUsize,
    /// With a kit armed: each note's pad's lane (none: `usize::MAX`), live notes going there.
    pub(crate) pads: Vec<AtomicUsize>,
    pub(crate) kit_armed: AtomicBool,
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
    /// Which of a channel's compressors and limiters report to their meters: `usize::MAX` its
    /// own, else the one at that place among its other built-ins (whose window is open).
    pub(crate) fx_more: AtomicUsize,
    pub(crate) reading: [AtomicU32; 3],
    /// What the stacked built-in whose window is open just played, each side (f32 bits), and
    /// how many samples have gone in, ever.
    pub(crate) scope: [Vec<AtomicU32>; 2],
    pub(crate) scope_written: AtomicUsize,
    /// The lane the synth's window watches, or `usize::MAX`; each of its sources now (f32 bits);
    /// and what it just played, both sides as one, with how many samples have gone in, ever.
    pub(crate) poly_slot: AtomicUsize,
    pub(crate) poly_live: Vec<AtomicU32>,
    pub(crate) poly_scope: Vec<AtomicU32>,
    pub(crate) poly_written: AtomicUsize,
}

/// Samples of a stacked built-in's sound kept for its window.
pub const SCOPE_LEN: usize = 1024;
/// Samples of a synth's sound kept for its window: enough for a low note's wave, and a spectrum.
pub const POLY_SCOPE_LEN: usize = 4096;

impl Shared {
    pub fn new() -> Self {
        Shared {
            position: AtomicU64::new(0),
            playing: AtomicBool::new(false),
            meters: (0..(SLOTS + 1) * 2).map(|_| AtomicU32::new(0)).collect(),
            reduction: (0..=SLOTS).map(|_| AtomicU32::new(0)).collect(),
            comp_levels: (0..(SLOTS + 1) * 2).map(|_| AtomicU32::new(0)).collect(),
            // The loudnesses start at -inf: nothing heard yet.
            limit: (0..(SLOTS + 1) * LIMIT_FIELDS)
                .map(|k| match k % LIMIT_FIELDS {
                    3..=5 => AtomicU32::new(f32::NEG_INFINITY.to_bits()),
                    _ => AtomicU32::new(0),
                })
                .collect(),
            loudness_reset: (0..=SLOTS).map(|_| AtomicBool::new(false)).collect(),
            armed: AtomicUsize::new(usize::MAX),
            pads: (0..128).map(|_| AtomicUsize::new(usize::MAX)).collect(),
            kit_armed: AtomicBool::new(false),
            load: AtomicU32::new(0),
            buffer: AtomicU32::new(0),
            metronome: AtomicBool::new(false),
            mods: (0..SLOTS * MAX_MODS).map(|_| AtomicU32::new(0)).collect(),
            eq_tap: crate::dsp::eq::Tap::new(),
            fx_more: AtomicUsize::new(usize::MAX),
            reading: Default::default(),
            scope: std::array::from_fn(|_| (0..SCOPE_LEN).map(|_| AtomicU32::new(0)).collect()),
            scope_written: AtomicUsize::new(0),
            poly_slot: AtomicUsize::new(usize::MAX),
            poly_live: (0..crate::dsp::poly::POLY_LIVE)
                .map(|_| AtomicU32::new(0))
                .collect(),
            poly_scope: (0..POLY_SCOPE_LEN).map(|_| AtomicU32::new(0)).collect(),
            poly_written: AtomicUsize::new(0),
        }
    }

    /// Keeps a block of what the open window's built-in played.
    /// What the tool whose window is open has to say.
    pub(crate) fn write_reading(&self, read: [f32; 3]) {
        for (slot, v) in self.reading.iter().zip(read) {
            slot.store(v.to_bits(), Ordering::Relaxed);
        }
    }

    /// Whether the synth's window is watching this lane.
    pub(crate) fn watches_poly(&self, slot: usize) -> bool {
        self.poly_slot.load(Ordering::Relaxed) == slot
    }

    pub(crate) fn write_poly_live(&self, live: &[f32]) {
        for (slot, v) in self.poly_live.iter().zip(live) {
            slot.store(v.to_bits(), Ordering::Relaxed);
        }
    }

    pub(crate) fn write_poly_scope(&self, l: &[f32], r: &[f32]) {
        let at = self.poly_written.load(Ordering::Relaxed);
        for (i, (a, b)) in l.iter().zip(r).enumerate() {
            let k = (at + i) % POLY_SCOPE_LEN;
            self.poly_scope[k].store(((a + b) * 0.5).to_bits(), Ordering::Relaxed);
        }
        self.poly_written.store(at + l.len(), Ordering::Relaxed);
    }

    pub(crate) fn write_scope(&self, l: &[f32], r: &[f32]) {
        let at = self.scope_written.load(Ordering::Relaxed);
        for (i, (a, b)) in l.iter().zip(r).enumerate() {
            let k = (at + i) % SCOPE_LEN;
            self.scope[0][k].store(a.to_bits(), Ordering::Relaxed);
            self.scope[1][k].store(b.to_bits(), Ordering::Relaxed);
        }
        self.scope_written.store(at + l.len(), Ordering::Relaxed);
    }

    /// Whether the compressor or limiter `more` (None a channel's own, else its place among the
    /// other built-ins) reports to the meters.
    pub(crate) fn meters_fx(&self, more: Option<usize>) -> bool {
        self.fx_more.load(Ordering::Relaxed) == more.unwrap_or(usize::MAX)
    }

    /// Records a block's peaks and gain reduction on a lane (or `MASTER`).
    pub(crate) fn report(&self, slot: usize, peak: [f32; 2], reduction: f32) {
        self.meters[slot * 2].fetch_max(peak[0].to_bits(), Ordering::Relaxed);
        self.meters[slot * 2 + 1].fetch_max(peak[1].to_bits(), Ordering::Relaxed);
        self.reduction[slot].fetch_max(reduction.to_bits(), Ordering::Relaxed);
    }

    pub(crate) fn report_comp(&self, slot: usize, levels: [f32; 2]) {
        self.comp_levels[slot * 2].fetch_max(levels[0].to_bits(), Ordering::Relaxed);
        self.comp_levels[slot * 2 + 1].fetch_max(levels[1].to_bits(), Ordering::Relaxed);
    }

    /// The loudest into and out of a lane's (or `MASTER`'s) compressor since the last call.
    pub fn take_comp(&self, slot: usize) -> [f32; 2] {
        let take = |i: usize| f32::from_bits(self.comp_levels[i].swap(0, Ordering::Relaxed));
        [take(slot * 2), take(slot * 2 + 1)]
    }

    pub(crate) fn report_limit(&self, slot: usize, l: &Limiting) {
        let f = &self.limit[slot * LIMIT_FIELDS..(slot + 1) * LIMIT_FIELDS];
        let (r, m) = (l.last, &l.meter);
        for (k, v) in [r.input, r.output, r.reduction].into_iter().enumerate() {
            f[k].fetch_max(v.to_bits(), Ordering::Relaxed);
        }
        for (k, v) in [m.momentary, m.short, m.integrated, m.true_peak]
            .into_iter()
            .enumerate()
        {
            f[3 + k].store(v.to_bits(), Ordering::Relaxed);
        }
    }

    /// A lane's (or `MASTER`'s) limiter's meter: its peaks and reduction since the last call.
    pub fn take_limit(&self, slot: usize) -> LimitMeter {
        let f = &self.limit[slot * LIMIT_FIELDS..(slot + 1) * LIMIT_FIELDS];
        let take = |k: usize| f32::from_bits(f[k].swap(0, Ordering::Relaxed));
        let load = |k: usize| f32::from_bits(f[k].load(Ordering::Relaxed));
        LimitMeter {
            input: take(0),
            output: take(1),
            reduction: take(2),
            momentary: load(3),
            short: load(4),
            integrated: load(5),
            true_peak: load(6),
        }
    }

    /// Starts a lane's (or `MASTER`'s) limiter's loudness over.
    pub fn reset_loudness(&self, slot: usize) {
        self.loudness_reset[slot].store(true, Ordering::Relaxed);
    }

    pub(crate) fn take_loudness_reset(&self, slot: usize) -> bool {
        self.loudness_reset[slot].swap(false, Ordering::Relaxed)
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
        self.kit_armed.store(false, Ordering::Relaxed);
    }

    /// Sends live notes to a kit's pads: each note to its pad's lane (`pads`: note, lane).
    pub fn arm_kit(&self, slot: usize, pads: &[(u8, usize)]) {
        for p in &self.pads {
            p.store(usize::MAX, Ordering::Relaxed);
        }
        for &(note, lane) in pads {
            self.pads[note as usize & 127].store(lane, Ordering::Relaxed);
        }
        self.armed.store(slot, Ordering::Relaxed);
        self.kit_armed.store(true, Ordering::Relaxed);
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
    /// Live controller moves (a note's own, from an MPE controller, among them), and each back
    /// as played, at the playhead.
    pub moves: rtrb::Consumer<crate::plugin::ControlKind>,
    pub heard: rtrb::Producer<(u64, crate::plugin::ControlKind)>,
}

/// Hands garbage back, or leaks it if the main thread has stopped collecting: freeing it here
/// could stall the audio thread.
pub(crate) fn trash(q: &mut rtrb::Producer<Garbage>, g: Garbage) {
    if let Err(rtrb::PushError::Full(g)) = q.push(g) {
        std::mem::forget(g);
    }
}
