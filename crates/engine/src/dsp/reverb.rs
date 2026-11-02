//! The built-in reverb, running. The sound goes through a pre-delay; its first reflections are
//! taps off that line, each side its own. Its tail is an eight-line feedback delay network:
//! four allpasses blur what goes in, then eight delay lines (lengths that never line up, each
//! moved slowly so the tail never rings metallic) feed each other through a Householder matrix,
//! which loses nothing, so how long it rings is only each line's loss: set per band (the lows,
//! the middle, the highs past the damping) from the decay asked for. A shimmer takes the tail up
//! an octave as it goes round. Then its low and high cuts, its width, ducking under what goes in,
//! and the mix. Its lines are made when it is (for the longest it can be at its rate), so it
//! allocates nothing while it runs.

use std::f32::consts::{PI, TAU};

use crate::{
    dsp::comp::ftz,
    song::project::{Reverb, ReverbStyle},
};

const LINES: usize = 8;
/// The tail's lines, ms, at a scale of 1: no two share a factor, so their echoes don't line up.
const LENGTHS_MS: [f32; LINES] = [31.7, 37.9, 41.3, 47.1, 53.9, 61.3, 67.1, 73.7];
/// The input's blur, ms at a scale of 1, and how much each feeds back at full diffusion.
const DIFFUSERS_MS: [f32; 4] = [4.71, 3.59, 12.73, 9.31];
const DIFFUSER_G: [f32; 4] = [0.75, 0.75, 0.625, 0.625];
/// The first reflections, each side: when (ms at a scale of 1) and how loud.
const EARLY: [[(f32, f32); 10]; 2] = [
    [
        (7.1, 0.84),
        (11.3, -0.72),
        (17.9, 0.66),
        (23.3, -0.55),
        (31.7, 0.48),
        (39.1, -0.41),
        (47.9, 0.35),
        (58.3, -0.29),
        (67.1, 0.24),
        (79.3, -0.19),
    ],
    [
        (8.3, -0.81),
        (13.1, 0.70),
        (19.7, -0.62),
        (26.9, 0.53),
        (33.1, -0.46),
        (43.7, 0.40),
        (51.1, -0.33),
        (61.9, 0.28),
        (71.3, -0.22),
        (83.9, 0.18),
    ],
];
/// Each line's slow movement: its rate, Hz, a little apart.
const MOD_HZ: [f32; LINES] = [0.13, 0.21, 0.17, 0.29, 0.11, 0.23, 0.19, 0.31];
/// The most a line moves, ms, at full modulation.
const MOD_MS: f32 = 0.9;
/// The biggest scale a style reaches: what the lines are made long enough for.
const MOST_SCALE: f32 = 2.8;
/// How the lows split from the rest, Hz.
const BASS_HZ: f32 = 250.0;
/// How much faster than the rest the highs past the damping die away.
const HIGH_RATIO: f32 = 0.3;
/// The longest pre-delay, s.
const MOST_PREDELAY: f32 = 0.5;
/// The octave-up's window, s.
const SHIFT_WINDOW: f32 = 0.06;

/// A style's scale (how long its lines and first reflections are) at size 1, its reflections'
/// own scale, and how long after them its tail starts (a share of the reflections' span).
fn shape(style: ReverbStyle) -> (f32, f32, f32) {
    match style {
        ReverbStyle::Room => (0.55, 0.4, 0.3),
        ReverbStyle::Chamber => (0.9, 0.7, 0.35),
        ReverbStyle::Hall => (1.6, 1.0, 0.4),
        ReverbStyle::Plate => (0.8, 0.0, 0.0),
        ReverbStyle::Ambience => (0.4, 0.35, 0.6),
        ReverbStyle::Cathedral => (MOST_SCALE, 1.8, 0.45),
        ReverbStyle::Shimmer => (1.6, 1.0, 0.4),
    }
}

/// What a reverb does to a click, for drawing it: its first reflections (seconds, how loud;
/// left then right), and when its tail starts, seconds.
pub fn picture(r: &Reverb, bpm: f64) -> (Vec<(f32, f32)>, f32) {
    let (_, early_scale, tail_after) = shape(r.style);
    let size = r.size.clamp(0.0, 1.0);
    let er_scale = early_scale * (0.4 + 0.6 * size);
    let pre = r.predelay_s(bpm);
    let level = r.early.clamp(0.0, 1.0) * if er_scale > 0.0 { 1.0 } else { 0.0 };
    let early = EARLY
        .iter()
        .flatten()
        .map(|&(ms, g)| (pre + ms * er_scale / 1000.0, g.abs() * level))
        .filter(|e| e.1 > 0.0)
        .collect();
    (early, pre + EARLY[0][9].0 * er_scale * tail_after / 1000.0)
}

/// A delay line, read anywhere between its samples.
pub(crate) struct Line {
    buf: Vec<f32>,
    w: usize,
}

impl Line {
    pub(crate) fn new(len: usize) -> Self {
        Line {
            buf: vec![0.0; len.max(4)],
            w: 0,
        }
    }

    /// Silence, in place.
    pub(crate) fn clear(&mut self) {
        self.buf.fill(0.0);
    }

    pub(crate) fn push(&mut self, x: f32) {
        self.buf[self.w] = x;
        self.w = (self.w + 1) % self.buf.len();
    }

    /// `d` samples ago (at least 2), between samples by a cubic (Catmull-Rom) through the four
    /// around it.
    pub(crate) fn read(&self, d: f32) -> f32 {
        let n = self.buf.len();
        let d = d.clamp(2.0, (n - 3) as f32);
        let i = d as usize;
        let t = d - i as f32;
        let tap = |delay: usize| self.buf[(self.w + n - delay) % n];
        let (p0, p1, p2, p3) = (tap(i - 1), tap(i), tap(i + 1), tap(i + 2));
        let a = -0.5 * p0 + 1.5 * p1 - 1.5 * p2 + 0.5 * p3;
        let b = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
        let c = -0.5 * p0 + 0.5 * p2;
        ((a * t + b) * t + c) * t + p1
    }

    pub(crate) fn at(&self, d: usize) -> f32 {
        let n = self.buf.len();
        self.buf[(self.w + n - d.clamp(1, n - 1)) % n]
    }
}

/// A Schroeder allpass: blurs without colouring.
pub(crate) struct Allpass {
    pub(crate) line: Line,
}

impl Allpass {
    pub(crate) fn run(&mut self, x: f32, d: usize, g: f32) -> f32 {
        let v = self.line.at(d);
        let w = ftz(x + g * v);
        self.line.push(w);
        v - g * w
    }
}

/// A two-pole filter (the cookbook's), for the tail's cuts.
#[derive(Clone, Copy, Default)]
pub(crate) struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    s: [f32; 2],
}

impl Biquad {
    pub(crate) fn tune(&mut self, hz: f32, rate: f32, high: bool) {
        let w = TAU * hz.clamp(10.0, rate * 0.45) / rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let a0 = 1.0 + alpha;
        let b = match high {
            true => [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0],
            false => [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0],
        };
        self.b = b.map(|x| x / a0);
        self.a = [-2.0 * cos / a0, (1.0 - alpha) / a0];
    }

    pub(crate) fn run(&mut self, x: f32) -> f32 {
        // Transposed direct form II.
        let y = self.b[0] * x + self.s[0];
        self.s[0] = ftz(self.b[1] * x - self.a[0] * y + self.s[1]);
        self.s[1] = ftz(self.b[2] * x - self.a[1] * y);
        y
    }
}

/// A one-pole low-pass's coefficient for `hz`.
pub(crate) fn pole(hz: f32, rate: f32) -> f32 {
    1.0 - (-TAU * hz / rate).exp()
}

/// A reverb, running.
pub struct Reverbing {
    rate: f32,
    input: Line,
    diffusers: [[Allpass; 4]; 2],
    lines: Vec<Line>,
    /// Each line's two splitting filters' memories: the lows, and under the damping.
    low: [f32; LINES],
    under: [f32; LINES],
    /// Each line's length now (samples), gliding to where it's set.
    length: [f32; LINES],
    phase: [f32; LINES],
    /// The octave up: its line and where its window is.
    shift: Line,
    shift_phase: f32,
    cuts: [[Biquad; 2]; 2],
    /// The dry sound's level, followed, and the ducking now.
    envelope: f32,
    duck: f32,
    started: bool,
}

impl Reverbing {
    pub fn new(rate: f64) -> Self {
        let rate = rate as f32;
        let samples = |s: f32| (s * rate) as usize + 8;
        let early = EARLY[1][9].0 * 1.8 / 1000.0;
        let diffuser = |k: usize| Allpass {
            line: Line::new(samples(DIFFUSERS_MS[k] * MOST_SCALE.sqrt() / 1000.0)),
        };
        Reverbing {
            rate,
            input: Line::new(samples(MOST_PREDELAY + early)),
            diffusers: std::array::from_fn(|_| std::array::from_fn(diffuser)),
            lines: LENGTHS_MS
                .iter()
                .map(|ms| Line::new(samples((ms * MOST_SCALE + MOD_MS * 2.0) / 1000.0)))
                .collect(),
            low: [0.0; LINES],
            under: [0.0; LINES],
            length: [0.0; LINES],
            phase: std::array::from_fn(|k| k as f32 / LINES as f32),
            shift: Line::new(samples(SHIFT_WINDOW * 1.1)),
            shift_phase: 0.0,
            cuts: [[Biquad::default(); 2]; 2],
            envelope: 0.0,
            duck: 1.0,
            started: false,
        }
    }

    /// Clears its memory: a reverb come back on starts silent.
    pub fn reset(&mut self) {
        self.input.clear();
        self.shift.clear();
        self.lines.iter_mut().for_each(Line::clear);
        self.diffusers
            .iter_mut()
            .flatten()
            .for_each(|d| d.line.clear());
        (self.low, self.under) = ([0.0; LINES], [0.0; LINES]);
        self.cuts = [[Biquad::default(); 2]; 2];
        (self.envelope, self.duck, self.started) = (0.0, 1.0, false);
    }

    /// Runs a block through it in place, the song at `bpm`.
    pub fn process(&mut self, r: &Reverb, bpm: f64, l: &mut [f32], rr: &mut [f32]) {
        let rate = self.rate;
        let (base, early_scale, tail_after) = shape(r.style);
        let size = r.size.clamp(0.0, 1.0);
        let scale = base * (0.35 + 0.65 * size);
        let er_scale = early_scale * (0.4 + 0.6 * size);
        let predelay = r.predelay_s(bpm) * rate;
        let tail_at = predelay + EARLY[0][9].0 * er_scale * tail_after / 1000.0 * rate;
        let decay = r.decay_s.clamp(0.1, 30.0);
        let bass = r.bass.clamp(0.25, 4.0);
        let damping = r.damping_hz.clamp(500.0, 20_000.0);
        let (a_low, a_damp) = (pole(BASS_HZ, rate), pole(damping, rate));
        // Each line's loss a time round, per band, for the decay asked for.
        let mut gains = [[1.0f32; 3]; LINES];
        let mut target = [0.0f32; LINES];
        for k in 0..LINES {
            target[k] = LENGTHS_MS[k] * scale / 1000.0 * rate;
            if !r.freeze {
                let g = |rt: f32| 10f32.powf(-3.0 * target[k] / (rt * rate));
                gains[k] = [g(decay * bass), g(decay), g(decay * HIGH_RATIO)];
            }
        }
        if !self.started {
            self.length = target;
            self.started = true;
        }
        let diffusion = r.diffusion.clamp(0.0, 1.0);
        let diffusers: [usize; 4] = std::array::from_fn(|k| {
            ((DIFFUSERS_MS[k] * scale.sqrt() / 1000.0 * rate) as usize).max(1)
        });
        let early_taps: [[(f32, f32); 10]; 2] = std::array::from_fn(|s| {
            std::array::from_fn(|k| {
                let (ms, g) = EARLY[s][k];
                (predelay + ms * er_scale / 1000.0 * rate, g)
            })
        });
        let early = r.early.clamp(0.0, 1.0) * if er_scale > 0.0 { 0.55 } else { 0.0 };
        let depth = r.modulation.clamp(0.0, 1.0) * MOD_MS / 1000.0 * rate;
        let shimmer = r.shimmer.clamp(0.0, 1.0);
        let window = SHIFT_WINDOW * rate;
        for s in 0..2 {
            self.cuts[s][0].tune(r.low_cut_hz.clamp(20.0, 2000.0), rate, true);
            self.cuts[s][1].tune(r.high_cut_hz.clamp(1000.0, 20_000.0), rate, false);
        }
        let width = r.width.clamp(0.0, 2.0);
        let mix = r.mix.clamp(0.0, 1.0);
        let out_gain = 10f32.powf(r.output_db.clamp(-24.0, 24.0) / 20.0);
        let (attack, release) = (pole(1.0 / 0.005 / TAU, rate), pole(1.0 / 0.25 / TAU, rate));
        let glide = pole(4.0, rate);
        // The tail's level: a 2 s hall about 6 dB under the sound going in, longer ones louder.
        let late_gain = 1.0;
        let norm = (1.0 / LINES as f32).sqrt();
        let n = l.len().min(rr.len());
        for i in 0..n {
            let (dl, dr) = (l[i], rr[i]);
            let input = if r.freeze { 0.0 } else { 0.5 * (dl + dr) };
            let side = if r.freeze { 0.0 } else { 0.5 * (dl - dr) };
            // The pre-delay, the first reflections off it, and what goes on into the tail.
            self.input.push(ftz(input));
            let mut er = [0.0f32; 2];
            if early > 0.0 {
                for (s, taps) in early_taps.iter().enumerate() {
                    for &(d, g) in taps {
                        er[s] += self.input.read(d) * g;
                    }
                }
            }
            let into = self.input.read(tail_at.max(1.0));
            // Blurred, each side its own way.
            let mut x = [into + side * 0.5, into - side * 0.5];
            for (s, x) in x.iter_mut().enumerate() {
                for k in 0..4 {
                    *x = self.diffusers[s][k].run(*x, diffusers[k], DIFFUSER_G[k] * diffusion);
                }
            }
            // The tail: each line read where it's moved to, its loss per band.
            let mut y = [0.0f32; LINES];
            for k in 0..LINES {
                self.length[k] += (target[k] - self.length[k]) * glide;
                self.phase[k] = (self.phase[k] + MOD_HZ[k] / rate) % 1.0;
                let d = self.length[k] + depth * (1.0 + (TAU * self.phase[k]).sin());
                let v = self.lines[k].read(d.max(2.0));
                self.low[k] = ftz(self.low[k] + a_low * (v - self.low[k]));
                self.under[k] = ftz(self.under[k] + a_damp * (v - self.under[k]));
                let [gl, gm, gh] = gains[k];
                y[k] = self.low[k] * gl
                    + (self.under[k] - self.low[k]) * gm
                    + (v - self.under[k]) * gh;
            }
            let (mut wet_l, mut wet_r) = (0.0, 0.0);
            for (k, &v) in y.iter().enumerate() {
                // Two rows of an 8 x 8 Hadamard: each side hears every line, unlike the other.
                wet_l += v * if k % 2 == 0 { 1.0 } else { -1.0 };
                wet_r += v * if (k / 2) % 2 == 0 { 1.0 } else { -1.0 };
            }
            // The octave up, from what's ringing, fed back in.
            let mut lift = 0.0;
            if shimmer > 0.0 {
                self.shift.push(ftz(wet_l + wet_r) * 0.5 * norm);
                self.shift_phase = (self.shift_phase + 1.0 / window) % 1.0;
                for k in 0..2 {
                    let p = (self.shift_phase + 0.5 * k as f32) % 1.0;
                    lift += self.shift.read(((1.0 - p) * window).max(1.0)) * (PI * p).sin();
                }
                lift *= shimmer * 0.5;
            }
            // Mixed losslessly, the input and the octave up added, and round again.
            let sum: f32 = y.iter().sum::<f32>() * 2.0 / LINES as f32;
            for k in 0..LINES {
                let feed = x[k % 2] * if k % 4 < 2 { 1.0 } else { -1.0 } * 0.5 + lift;
                self.lines[k].push(ftz(y[k] - sum + feed));
            }
            let mut wet = [
                wet_l * norm * late_gain + er[0] * early,
                wet_r * norm * late_gain + er[1] * early,
            ];
            for (s, w) in wet.iter_mut().enumerate() {
                let low_cut = self.cuts[s][0].run(*w);
                *w = self.cuts[s][1].run(low_cut);
            }
            // Its width, then ducked while the dry sound plays.
            let (m, sd) = ((wet[0] + wet[1]) * 0.5, (wet[0] - wet[1]) * 0.5 * width);
            let level = dl.abs().max(dr.abs());
            let a = if level > self.envelope {
                attack
            } else {
                release
            };
            self.envelope = ftz(self.envelope + a * (level - self.envelope));
            let over = ((20.0 * (self.envelope + 1e-9).log10() + 50.0) / 40.0).clamp(0.0, 1.0);
            let want = 10f32.powf(-r.ducking.clamp(0.0, 1.0) * 18.0 * over / 20.0);
            self.duck += (want - self.duck) * glide;
            let (wl, wr) = ((m + sd) * self.duck, (m - sd) * self.duck);
            l[i] = (dl * (1.0 - mix) + wl * mix) * out_gain;
            rr[i] = (dr * (1.0 - mix) + wr * mix) * out_gain;
        }
    }
}

#[cfg(test)]
mod tests;
