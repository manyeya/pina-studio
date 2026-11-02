//! The limiter's loudness meter, as BS.1770 has it: the sound K-weighted and measured in
//! 100 ms blocks. Momentary is the last 400 ms, short-term the last 3 s. Integrated is every
//! 400 ms window since it was reset (75% overlapped), gated at -70 LUFS and then 10 LU under
//! their own loudness. The windows are kept in a histogram, 0.1 LU a bin, so integrating never
//! grows. True peak is the loudest point, 4x interpolated.

use super::{Interp, TAPS, phases};
use crate::music::analysis::{Biquad, k_weighting};

/// The histogram: 0.1 LU a bin, -70 LUFS up to +5.
const BINS: usize = 750;
const FLOOR: f64 = -70.0;

pub fn lufs(power: f64) -> f32 {
    match power > 0.0 {
        true => (-0.691 + 10.0 * power.log10()) as f32,
        false => f32::NEG_INFINITY,
    }
}

pub struct Loudness {
    weights: [Biquad; 2],
    /// Each side's filters' memory.
    z: [[[f64; 2]; 2]; 2],
    block: usize,
    filled: usize,
    sum: f64,
    /// The last 30 blocks' power, oldest overwritten; how many there are.
    blocks: [f64; 30],
    at: usize,
    count: usize,
    windows: Vec<u32>,
    energy: Vec<f64>,
    peaks: [Interp; 2],
    phases: [[f32; TAPS]; 3],
    pub momentary: f32,
    pub short: f32,
    pub integrated: f32,
    /// The loudest true peak since reset (linear).
    pub true_peak: f32,
}

impl Loudness {
    pub fn new(rate: f64) -> Self {
        Loudness {
            weights: k_weighting(rate),
            z: [[[0.0; 2]; 2]; 2],
            block: ((rate / 10.0).round() as usize).max(1),
            filled: 0,
            sum: 0.0,
            blocks: [0.0; 30],
            at: 0,
            count: 0,
            windows: vec![0; BINS],
            energy: vec![0.0; BINS],
            peaks: [Interp::new(); 2],
            phases: phases(),
            momentary: f32::NEG_INFINITY,
            short: f32::NEG_INFINITY,
            integrated: f32::NEG_INFINITY,
            true_peak: 0.0,
        }
    }

    /// Starts integrating (and the true peak) over.
    pub fn reset(&mut self) {
        self.windows.fill(0);
        self.energy.fill(0.0);
        self.integrated = f32::NEG_INFINITY;
        self.true_peak = 0.0;
    }

    /// One side's next sample (left then right, a sample at a time).
    pub fn push(&mut self, side: usize, x: f32) {
        let (_, between) = self.peaks[side].push(x, &self.phases);
        self.true_peak = self.true_peak.max(x.abs()).max(between);
        let z = &mut self.z[side];
        let y = self.weights[0].step(x as f64, &mut z[0]);
        let y = self.weights[1].step(y, &mut z[1]);
        self.sum += y * y;
        if side == 1 {
            self.filled += 1;
            if self.filled == self.block {
                self.end_block();
            }
        }
    }

    fn end_block(&mut self) {
        self.blocks[self.at] = self.sum / self.block as f64;
        self.at = (self.at + 1) % self.blocks.len();
        self.count = (self.count + 1).min(self.blocks.len());
        (self.sum, self.filled) = (0.0, 0);
        let last = |n: usize| {
            let n = n.min(self.count);
            let len = self.blocks.len();
            (1..=n)
                .map(|k| self.blocks[(self.at + len - k) % len])
                .sum::<f64>()
                / n as f64
        };
        if self.count < 4 {
            return;
        }
        let window = last(4);
        self.momentary = lufs(window);
        self.short = lufs(last(30));
        let loud = lufs(window) as f64;
        if loud > FLOOR {
            let bin = (((loud - FLOOR) * 10.0) as usize).min(BINS - 1);
            self.windows[bin] += 1;
            self.energy[bin] += window;
        }
        self.integrated = self.gated();
    }

    /// The mean of the windows over -70 LUFS, then of those under 10 LU below that.
    fn gated(&self) -> f32 {
        let mean = |from: usize| {
            let n: u32 = self.windows[from..].iter().sum();
            let e: f64 = self.energy[from..].iter().sum();
            (n > 0).then(|| e / n as f64)
        };
        let Some(all) = mean(0) else {
            return f32::NEG_INFINITY;
        };
        let gate = lufs(all) as f64 - 10.0;
        let from = (((gate - FLOOR) * 10.0).ceil().max(0.0) as usize).min(BINS - 1);
        lufs(mean(from).unwrap_or(all))
    }
}
