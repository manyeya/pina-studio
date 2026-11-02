//! The saturator's filters: the half-band filters it runs at twice (and four, eight times) the
//! sample rate through, the crossovers that split it into bands, and the small ones its styles
//! and tone are made of.

// The coefficients as halfband.py prints them.
#![allow(clippy::excessive_precision)]

use std::f32::consts::PI;

use crate::{dsp::comp::ftz, song::project::MAX_SAT_BANDS};

/// The half-band filter's two allpass chains (from scripts/halfband.py 8 0.04: flat to 0.42 of
/// the sample rate, 99 dB down past 0.58), H(z) = (A0(z^2) + z^-1 A1(z^2)) / 2.
const EVEN: [f32; 4] = [
    0.040633460924,
    0.300757055992,
    0.609524314896,
    0.849223810392,
];
const ODD: [f32; 4] = [
    0.150505129023,
    0.460774504961,
    0.738503841119,
    0.949742783705,
];
/// For the stages after the first, whose sound is already band-limited to well under their
/// Nyquist: halfband.py 4 0.2, flat to 0.20 of their rate, 117 dB down at 0.40.
const LIGHT_EVEN: [f32; 4] = [0.049551035313, 0.426736688756, 0.0, 0.0];
const LIGHT_ODD: [f32; 4] = [0.193570326347, 0.767070072813, 0.0, 0.0];

#[derive(Clone, Copy, Default)]
struct Allpass {
    x1: f32,
    y1: f32,
}

/// A chain of first-order allpasses in z^2's terms, one a phase of the half-band filter.
#[derive(Clone, Copy, Default)]
struct Chain([Allpass; 4]);

impl Chain {
    fn run(&mut self, mut x: f32, a: &[f32]) -> f32 {
        for (s, &a) in self.0.iter_mut().zip(a) {
            let y = ftz(a * x + s.x1 - a * s.y1);
            (s.x1, s.y1) = (x, y);
            x = y;
        }
        x
    }
}

/// Twice the sample rate and back: a sample in becomes two (2n and 2n+1); two back out as one.
/// `light`, for a stage after the first, with fewer allpasses.
#[derive(Clone, Copy, Default)]
pub struct Halfband {
    up: [Chain; 2],
    down: [Chain; 2],
    odd_was: f32,
    pub light: bool,
}

impl Halfband {
    fn coefficients(&self) -> (&'static [f32], &'static [f32]) {
        match self.light {
            true => (&LIGHT_EVEN[..2], &LIGHT_ODD[..2]),
            false => (&EVEN, &ODD),
        }
    }

    pub fn up(&mut self, x: f32) -> [f32; 2] {
        let (even, odd) = self.coefficients();
        [self.up[0].run(x, even), self.up[1].run(x, odd)]
    }

    pub fn down(&mut self, [e, o]: [f32; 2]) -> f32 {
        let (even, odd) = self.coefficients();
        let y = (self.down[0].run(e, even) + self.odd_was) / 2.0;
        self.odd_was = self.down[1].run(o, odd);
        y
    }
}

/// A biquad (direct form II, transposed).
#[derive(Clone, Copy, Default)]
pub struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

impl Biquad {
    /// A Butterworth-family section, low or high pass, at `hz` with `q`.
    pub fn pass(high: bool, hz: f32, q: f32, rate: f32) -> Self {
        let w = 2.0 * PI * (hz / rate).clamp(1e-5, 0.49);
        let (c, alpha) = (w.cos(), w.sin() / (2.0 * q));
        let a0 = 1.0 + alpha;
        let b = match high {
            true => [(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0],
            false => [(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0],
        };
        Biquad {
            b: b.map(|v| v / a0),
            a: [-2.0 * c / a0, (1.0 - alpha) / a0],
            z: [0.0; 2],
        }
    }

    /// A band pass at `hz`, peak gain 1, `q` wide.
    pub fn band(hz: f32, q: f32, rate: f32) -> Self {
        let w = 2.0 * PI * (hz / rate).clamp(1e-5, 0.49);
        let (c, alpha) = (w.cos(), w.sin() / (2.0 * q));
        let a0 = 1.0 + alpha;
        Biquad {
            b: [alpha / a0, 0.0, -alpha / a0],
            a: [-2.0 * c / a0, (1.0 - alpha) / a0],
            z: [0.0; 2],
        }
    }

    /// The same filter at new settings, its memory kept.
    pub fn retune(&mut self, to: Biquad) {
        (self.b, self.a) = (to.b, to.a);
    }

    pub fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = ftz(self.b[1] * x - self.a[0] * y + self.z[1]);
        self.z[1] = ftz(self.b[2] * x - self.a[1] * y);
        y
    }
}

/// A first-order low or high pass, by the bilinear transform.
#[derive(Clone, Copy, Default)]
pub struct OnePole {
    b0: f32,
    b1: f32,
    a: f32,
    x1: f32,
    y1: f32,
}

impl OnePole {
    pub fn low(hz: f32, rate: f32) -> Self {
        let c = 1.0 / (PI * (hz / rate).clamp(1e-5, 0.49)).tan();
        let d = 1.0 + c;
        OnePole {
            b0: 1.0 / d,
            b1: 1.0 / d,
            a: (1.0 - c) / d,
            ..Default::default()
        }
    }

    pub fn high(hz: f32, rate: f32) -> Self {
        let c = 1.0 / (PI * (hz / rate).clamp(1e-5, 0.49)).tan();
        let d = 1.0 + c;
        OnePole {
            b0: c / d,
            b1: -c / d,
            a: (1.0 - c) / d,
            ..Default::default()
        }
    }

    /// The same filter at new settings, its memory kept.
    pub fn retune(&mut self, to: OnePole) {
        (self.b0, self.b1, self.a) = (to.b0, to.b1, to.a);
    }

    pub fn run(&mut self, x: f32) -> f32 {
        let y = ftz(self.b0 * x + self.b1 * self.x1 - self.a * self.y1);
        (self.x1, self.y1) = (x, y);
        y
    }
}

/// A DC blocker: a zero at DC, a pole just inside it.
#[derive(Clone, Copy, Default)]
pub struct DcBlock {
    x1: f32,
    y1: f32,
}

impl DcBlock {
    pub fn run(&mut self, x: f32) -> f32 {
        let y = ftz(x - self.x1 + 0.995 * self.y1);
        (self.x1, self.y1) = (x, y);
        y
    }
}

/// Butterworth sections' Qs for a Linkwitz-Riley crossover `slope` dB an octave steep (a 12 dB
/// one is two first-order halves, the rest squared Butterworth). LR8: BW4 twice.
fn qs(slope: u8) -> &'static [f32] {
    match slope {
        12 => &[],
        48 => &[0.541_196_1, 1.306_563, 0.541_196_1, 1.306_563],
        _ => &[
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
        ],
    }
}

/// One Linkwitz-Riley crossover: its low and high halves, which add back to an allpass.
#[derive(Clone, Copy, Default)]
pub struct Crossover {
    low: [Biquad; 4],
    high: [Biquad; 4],
    /// A 12 dB one's first-order halves: two low passes, two high passes.
    first: [OnePole; 4],
    sections: u8,
    slope: u8,
}

impl Crossover {
    /// At new settings, its memory kept (unless its slope changes).
    pub fn tune(&mut self, hz: f32, slope: u8, rate: f32) {
        if slope != self.slope {
            *self = Crossover {
                slope,
                ..Default::default()
            };
        }
        let q = qs(slope);
        self.sections = q.len() as u8;
        for (i, &q) in q.iter().enumerate() {
            self.low[i].retune(Biquad::pass(false, hz, q, rate));
            self.high[i].retune(Biquad::pass(true, hz, q, rate));
        }
        for (i, f) in self.first.iter_mut().enumerate() {
            f.retune(match i < 2 {
                true => OnePole::low(hz, rate),
                false => OnePole::high(hz, rate),
            });
        }
    }

    /// The sound split: (below, above), adding back to an allpass of it.
    pub fn split(&mut self, x: f32) -> (f32, f32) {
        match self.sections {
            // LR2: the high half's sign turned, so the two add back flat.
            0 => {
                let lo = self.first[0].run(x);
                let lo = self.first[1].run(lo);
                let hi = self.first[2].run(x);
                let hi = self.first[3].run(hi);
                (lo, -hi)
            }
            n => {
                let (mut lo, mut hi) = (x, x);
                for i in 0..n as usize {
                    lo = self.low[i].run(lo);
                    hi = self.high[i].run(hi);
                }
                (lo, hi)
            }
        }
    }
}

/// A sound split into bands that add back up to an allpass of it: each band below a crossover
/// through it, and (so they line up) through the allpasses of the crossovers above its own.
#[derive(Clone, Copy, Default)]
pub struct Splitter {
    splits: [Crossover; MAX_SAT_BANDS - 1],
    aligns: [[Crossover; MAX_SAT_BANDS - 1]; MAX_SAT_BANDS],
    tuned: Option<(usize, [f32; MAX_SAT_BANDS - 1], u8)>,
}

impl Splitter {
    /// For `n` bands split at `at` (Hz), `slope` dB an octave; its memory kept.
    pub fn tune(&mut self, n: usize, at: &[f32; MAX_SAT_BANDS - 1], slope: u8, rate: f32) {
        if self.tuned == Some((n, *at, slope)) {
            return;
        }
        for (k, &hz) in at.iter().enumerate().take(n.saturating_sub(1)) {
            let hz = hz.clamp(20.0, 20_000.0);
            self.splits[k].tune(hz, slope, rate);
            for band in 0..=k {
                self.aligns[band][k].tune(hz, slope, rate);
            }
        }
        self.tuned = Some((n, *at, slope));
    }

    /// A sample split into the `n` bands it's tuned for, low to high.
    pub fn split(&mut self, x: f32, n: usize) -> [f32; MAX_SAT_BANDS] {
        let mut parts = [0f32; MAX_SAT_BANDS];
        let mut rest = x;
        for (k, part) in parts.iter_mut().enumerate().take(n - 1) {
            let (lo, hi) = self.splits[k].split(rest);
            *part = lo;
            rest = hi;
        }
        parts[n - 1] = rest;
        for (band, part) in parts.iter_mut().enumerate().take(n - 1) {
            for k in band + 1..n - 1 {
                let (lo, hi) = self.aligns[band][k].split(*part);
                *part = lo + hi;
            }
        }
        parts
    }
}
