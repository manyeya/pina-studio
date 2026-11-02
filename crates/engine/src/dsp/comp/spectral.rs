//! The compressor's spectral stage: the sound split into bands that add back up (the
//! saturator's splitter), and the compressor's turning down given to each band by how much of
//! the sound it makes now. The band that pushed it over takes all of it; a quiet one alongside
//! takes less. No latency.

use super::*;
use crate::{dsp::sat::Splitter, song::project::MAX_SAT_BANDS};

const BANDS: usize = MAX_SAT_BANDS;
/// Where it splits, Hz.
const SPLITS: [f32; BANDS - 1] = [120.0, 350.0, 1_000.0, 3_000.0, 7_500.0];
/// How quickly each band's level follows it, seconds.
const LEVEL_TAU: f32 = 0.01;
/// How long a change of it on or off fades, seconds.
const FADE: f32 = 0.02;
/// Its weights are worked out every this many samples.
const CHUNK: usize = 16;

pub struct Spectral {
    split: [Splitter; 2],
    /// Each band's mean square, both sides.
    level: [f32; BANDS],
    /// How much of the turning down each band takes, 0 to 1.
    weight: [f32; BANDS],
    /// How far it's faded in, 0 (not split at all) to 1.
    on: f32,
    count: usize,
}

impl Spectral {
    pub fn new(rate: f32) -> Self {
        let mut split = [Splitter::default(); 2];
        for s in &mut split {
            s.tune(BANDS, &SPLITS, 24, rate);
        }
        Spectral {
            split,
            level: [0.0; BANDS],
            weight: [1.0; BANDS],
            on: 0.0,
            count: 0,
        }
    }

    /// Whether it's working, or fading out.
    pub fn running(&self, amount: f32) -> bool {
        amount > 0.0 || self.on > 0.0
    }

    /// A stereo sample, `gain` dB turned down (0 or under, makeup aside): (as it came in, through
    /// the bands untouched; and turned down band by band). `amount` 0 to 1.
    pub fn run(&mut self, x: [f32; 2], gain: f32, amount: f32, rate: f32) -> ([f32; 2], [f32; 2]) {
        let parts = [
            self.split[0].split(x[0], BANDS),
            self.split[1].split(x[1], BANDS),
        ];
        let p = pole(LEVEL_TAU, rate);
        for (level, (l, r)) in self.level.iter_mut().zip(parts[0].iter().zip(&parts[1])) {
            *level = ftz(p * *level + (1.0 - p) * 0.5 * (l * l + r * r));
        }
        if self.count.is_multiple_of(CHUNK) {
            // The loudest band takes it all; the rest by how loud they are next to it.
            let top = self.level.iter().fold(1e-12f32, |m, &l| m.max(l));
            for b in 0..BANDS {
                self.weight[b] = 1.0 - amount + amount * (self.level[b] / top).sqrt();
            }
        }
        self.count = self.count.wrapping_add(1);
        let step = 1.0 / (FADE * rate);
        self.on = match amount > 0.0 {
            true => (self.on + step).min(1.0),
            false => (self.on - step).max(0.0),
        };
        let (mut dry, mut wet) = ([0f32; 2], [0f32; 2]);
        for (b, weight) in self.weight.iter().enumerate() {
            let k = 10f32.powf(gain * weight / 20.0);
            for side in 0..2 {
                dry[side] += parts[side][b];
                wet[side] += parts[side][b] * k;
            }
        }
        // Fading in or out: the sound as it came, crossing over to the bands.
        let direct = 10f32.powf(gain / 20.0);
        for side in 0..2 {
            dry[side] = x[side] + (dry[side] - x[side]) * self.on;
            wet[side] = x[side] * direct + (wet[side] - x[side] * direct) * self.on;
        }
        (dry, wet)
    }
}
