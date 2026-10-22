//! What the EQ's window hears of each channel: the analyzer's taps.

use super::*;

/// Samples the analyzer keeps: enough for 6 Hz bins at 48 kHz.
pub const TAP_LEN: usize = 8192;

/// Samples each lane keeps for the instance list.
pub const LANE_LEN: usize = 2048;

/// What the EQ window watches, shared with the audio thread: which lane's EQ, a band to hear
/// alone and how loud, and that EQ's latest sound going in (mono) and coming out (each side),
/// and its dynamic bands' moves.
pub struct Tap {
    /// The lane watched (`usize::MAX`: none).
    pub slot: AtomicUsize,
    /// A band of it heard alone, by id (0: none), and the level it's heard at (dB, f32 bits).
    pub solo: AtomicU32,
    pub solo_db: AtomicU32,
    pub(crate) pre: Vec<AtomicU32>,
    pub(crate) post: [Vec<AtomicU32>; 2],
    /// Samples written, ever: where the next goes.
    pub(crate) written: AtomicUsize,
    /// Each band's dynamic move now, and what it's hearing (its trigger level), dB (f32
    /// bits), by its place in the EQ.
    pub(crate) moved: Vec<AtomicU32>,
    pub(crate) level: Vec<AtomicU32>,
    /// Another lane, heard to compare with (its output, mono): for collisions and matching.
    pub ext_slot: AtomicUsize,
    pub(crate) ext: Vec<AtomicU32>,
    pub(crate) ext_written: AtomicUsize,
    /// Every lane's sound lately (mono, `LANE_LEN` each, the master's last), for the instance
    /// list: kept while it's open.
    pub lanes_on: AtomicBool,
    pub(crate) lanes: Vec<AtomicU32>,
    pub(crate) lanes_written: Vec<AtomicUsize>,
}

impl Default for Tap {
    fn default() -> Self {
        Tap::new()
    }
}

impl Tap {
    pub fn new() -> Self {
        let ring = || (0..TAP_LEN).map(|_| AtomicU32::new(0)).collect();
        Tap {
            slot: AtomicUsize::new(usize::MAX),
            solo: AtomicU32::new(0),
            solo_db: AtomicU32::new(0),
            pre: ring(),
            post: [ring(), ring()],
            written: AtomicUsize::new(0),
            moved: (0..MAX_BANDS).map(|_| AtomicU32::new(0)).collect(),
            level: (0..MAX_BANDS).map(|_| AtomicU32::new(0)).collect(),
            ext_slot: AtomicUsize::new(usize::MAX),
            ext: ring(),
            ext_written: AtomicUsize::new(0),
            lanes_on: AtomicBool::new(false),
            lanes: (0..(MASTER + 1) * LANE_LEN)
                .map(|_| AtomicU32::new(0))
                .collect(),
            lanes_written: (0..=MASTER).map(|_| AtomicUsize::new(0)).collect(),
        }
    }

    /// A lane's sound (`MASTER` the master's), while the instance list is open.
    pub fn write_lane(&self, slot: usize, l: &[f32], r: &[f32]) {
        if !self.lanes_on.load(Ordering::Relaxed) || slot > MASTER {
            return;
        }
        let ring = &self.lanes[slot * LANE_LEN..(slot + 1) * LANE_LEN];
        let mut at = self.lanes_written[slot].load(Ordering::Relaxed);
        for (a, b) in l.iter().zip(r) {
            ring[at % LANE_LEN].store((0.5 * (a + b)).to_bits(), Ordering::Relaxed);
            at += 1;
        }
        self.lanes_written[slot].store(at, Ordering::Release);
    }

    /// A lane's latest `LANE_LEN` samples, oldest first; how many it's had written ever.
    pub fn read_lane(&self, slot: usize, out: &mut [f32; LANE_LEN]) -> usize {
        if slot > MASTER {
            return 0;
        }
        let ring = &self.lanes[slot * LANE_LEN..(slot + 1) * LANE_LEN];
        let at = self.lanes_written[slot].load(Ordering::Acquire);
        for (i, o) in out.iter_mut().enumerate() {
            *o = f32::from_bits(ring[(at + i) % LANE_LEN].load(Ordering::Relaxed));
        }
        at
    }

    /// A lane's output, if it's the one compared with.
    pub fn write_ext(&self, slot: usize, l: &[f32], r: &[f32]) {
        if self.ext_slot.load(Ordering::Relaxed) != slot {
            return;
        }
        let mut at = self.ext_written.load(Ordering::Relaxed);
        for (a, b) in l.iter().zip(r) {
            self.ext[at % TAP_LEN].store((0.5 * (a + b)).to_bits(), Ordering::Relaxed);
            at += 1;
        }
        self.ext_written.store(at, Ordering::Release);
    }

    /// The latest `TAP_LEN` samples of the lane compared with, oldest first; and how many
    /// have been written ever.
    pub fn read_ext(&self, out: &mut [f32; TAP_LEN]) -> usize {
        let at = self.ext_written.load(Ordering::Acquire);
        for (i, o) in out.iter_mut().enumerate() {
            *o = f32::from_bits(self.ext[(at + i) % TAP_LEN].load(Ordering::Relaxed));
        }
        at
    }

    /// Whether `slot`'s EQ is watched.
    pub fn watches(&self, slot: usize) -> bool {
        self.slot.load(Ordering::Relaxed) == slot
    }

    /// The band heard alone, and its level.
    pub fn solo(&self) -> Option<(u16, f32)> {
        match self.solo.load(Ordering::Relaxed) {
            0 => None,
            id => Some((
                id as u16,
                f32::from_bits(self.solo_db.load(Ordering::Relaxed)),
            )),
        }
    }

    pub(crate) fn write(&self, pre_l: &[f32], pre_r: &[f32], l: &[f32], r: &[f32]) {
        let mut at = self.written.load(Ordering::Relaxed);
        for i in 0..l.len() {
            let k = at % TAP_LEN;
            let pre = 0.5 * (pre_l[i] + pre_r[i]);
            self.pre[k].store(pre.to_bits(), Ordering::Relaxed);
            self.post[0][k].store(l[i].to_bits(), Ordering::Relaxed);
            self.post[1][k].store(r[i].to_bits(), Ordering::Relaxed);
            at += 1;
        }
        self.written.store(at, Ordering::Release);
    }

    /// The latest `TAP_LEN` samples going in, and coming out on each side, oldest first; and
    /// how many have been written ever (unchanged: nothing new).
    pub fn read(
        &self,
        pre: &mut [f32; TAP_LEN],
        left: &mut [f32; TAP_LEN],
        right: &mut [f32; TAP_LEN],
    ) -> usize {
        let at = self.written.load(Ordering::Acquire);
        for i in 0..TAP_LEN {
            let k = (at + i) % TAP_LEN;
            pre[i] = f32::from_bits(self.pre[k].load(Ordering::Relaxed));
            left[i] = f32::from_bits(self.post[0][k].load(Ordering::Relaxed));
            right[i] = f32::from_bits(self.post[1][k].load(Ordering::Relaxed));
        }
        at
    }

    /// The dynamic move of the EQ's band at `i`, dB.
    pub fn moved(&self, i: usize) -> f32 {
        self.moved
            .get(i)
            .map_or(0.0, |m| f32::from_bits(m.load(Ordering::Relaxed)))
    }

    /// What the EQ's band at `i` hears to move by (its trigger level), dB.
    pub fn level(&self, i: usize) -> f32 {
        self.level
            .get(i)
            .map_or(-120.0, |m| f32::from_bits(m.load(Ordering::Relaxed)))
    }
}
