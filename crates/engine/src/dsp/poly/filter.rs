//! The synth's filters. The circuits are solved with zero-delay feedback: each sample the loop is
//! worked out exactly, rather than a sample late, so the cutoff and resonance stay true right up
//! the keyboard and the filter sings as a real one does. Their nonlinearity is linearized about
//! where each stage is (after Teemu Voipio), which keeps that exact solve while every stage
//! saturates, and it's why they overdrive like circuits, not like a clipper in front of a filter.

use std::f32::consts::{PI, TAU};

use crate::{dsp::comp::ftz, song::project::PolyFilter};

/// The comb's longest delay, samples at the running rate: 20 Hz at 192 kHz.
const COMB: usize = 9600;

/// tanh(x)/x: what a stage's gain falls to as it's pushed, as the two sides of its fraction.
#[inline(always)]
fn soft_parts(x: f32) -> (f32, f32) {
    let a = x * x;
    ((a + 105.0) * a + 945.0, (15.0 * a + 420.0) * a + 945.0)
}

#[inline(always)]
fn soft(x: f32) -> f32 {
    let (top, under) = soft_parts(x);
    top / under
}

/// What a filter's set to, worked out each control step.
#[derive(Clone, Copy, Default)]
pub struct Tuned {
    /// The cutoff prewarped: tan(pi fc / fs).
    f: f32,
    /// A linear ladder stage's gain over its load.
    linear: f32,
    /// The feedback.
    r: f32,
    /// The cutoff itself, cycles a sample.
    cycle: f32,
    /// The input's gain into the circuit, and the output's back.
    drive: f32,
    makeup: f32,
    /// The state-variable filter's damping and first coefficients.
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    /// The formant filter's three.
    bands: [[f32; 3]; 3],
    /// The comb's delay, samples.
    delay: f32,
}

const VOWELS: [[f32; 3]; 5] = [
    [730.0, 1090.0, 2440.0],
    [530.0, 1840.0, 2480.0],
    [270.0, 2290.0, 3010.0],
    [570.0, 840.0, 2410.0],
    [300.0, 870.0, 2240.0],
];

impl Tuned {
    pub fn new(kind: PolyFilter, hz: f32, res: f32, drive: f32, bass: f32, rate: f32) -> Tuned {
        let hz = hz.clamp(16.0, rate * 0.46);
        let res = res.clamp(0.0, 1.0);
        let f = (PI * hz / rate).tan();
        let d = drive.clamp(0.0, 1.0);
        let push = 1.0 + 7.0 * d * d;
        let r = match kind {
            PolyFilter::Ladder | PolyFilter::Ladder2 => 4.05 * res,
            PolyFilter::Cascade => 3.9 * res,
            PolyFilter::Diode => 17.0 * res * res.sqrt(),
            _ => res,
        };
        // A ladder loses its lows as it resonates; `bass` brings them back.
        let up = match kind {
            PolyFilter::Ladder | PolyFilter::Ladder2 | PolyFilter::Cascade => 1.0 + 0.5 * r * bass,
            PolyFilter::Diode => 1.0 + 0.25 * r + 0.1 * r * bass,
            _ => 1.0,
        };
        let k = 2.0 - 1.98 * res;
        let a1 = 1.0 / (1.0 + f * (f + k));
        let mut t = Tuned {
            f,
            linear: 1.0 / (1.0 + f),
            r,
            cycle: hz / rate,
            drive: push,
            makeup: up / push.sqrt(),
            k,
            a1,
            a2: f * a1,
            a3: f * f * a1,
            bands: [[0.0; 3]; 3],
            delay: (rate / hz).clamp(2.0, COMB as f32 - 4.0),
        };
        if kind == PolyFilter::Formant {
            let at = ((hz / 20.0).ln() / 1000f32.ln()).clamp(0.0, 1.0) * 3.999;
            let (i, part) = (at as usize, at.fract());
            for (n, band) in t.bands.iter_mut().enumerate() {
                let v = VOWELS[i][n] + (VOWELS[i + 1][n] - VOWELS[i][n]) * part;
                let g = (PI * v.min(rate * 0.45) / rate).tan();
                let k = 0.1 + 0.3 * (1.0 - res);
                let a = 1.0 / (1.0 + g * (g + k));
                *band = [a, g * a, g * g * a];
            }
        }
        t
    }
}

/// One channel of a filter in a voice.
#[derive(Clone)]
pub struct Stage {
    s: [f32; 4],
    z: f32,
    bands: [[f32; 2]; 3],
    /// The phaser's all-passes, and what it holds of the crush.
    pass: [f32; 6],
    held: f32,
    clock: f32,
    comb: Vec<f32>,
    at: usize,
    /// Whether the comb's been written: a new note mustn't clear 9600 samples for nothing.
    combed: bool,
}

impl Stage {
    pub fn new() -> Stage {
        Stage {
            s: [0.0; 4],
            z: 0.0,
            bands: [[0.0; 2]; 3],
            pass: [0.0; 6],
            held: 0.0,
            clock: 0.0,
            comb: vec![0.0; COMB],
            at: 0,
            combed: false,
        }
    }

    pub fn reset(&mut self) {
        self.s = [0.0; 4];
        self.z = 0.0;
        self.bands = [[0.0; 2]; 3];
        self.pass = [0.0; 6];
        self.held = 0.0;
        if std::mem::take(&mut self.combed) {
            self.comb.fill(0.0);
        }
    }

    /// The transistor ladder: `tap` the stage it's heard from.
    #[inline(always)]
    fn ladder(&mut self, t: &Tuned, x: f32, tap: usize, clean: bool) -> f32 {
        let (f, r) = (t.f, t.r);
        let s = &mut self.s;
        let half = 0.5 * (x + self.z);
        self.z = x;
        let t0 = soft(half - r * s[3]);
        // A stage's gain over its load, g, and that through its tanh, one division for the two.
        // Only the input and the feedback push when it's clean; the stages stay linear: glassier.
        let stage = |v: f32| match clean {
            true => (t.linear, t.linear),
            false => {
                let (top, under) = soft_parts(v);
                let d = 1.0 / (under + f * top);
                (top * d, under * d)
            }
        };
        let ((a0, _), (a1, _), (a2, _), (a3, g3)) =
            (stage(s[0]), stage(s[1]), stage(s[2]), stage(s[3]));
        let from_x = f * f * f * f * a0 * a1 * a2 * t0;
        let from_s = s[3] + f * a2 * (s[2] + f * a1 * (s[1] + f * a0 * s[0]));
        let y = (from_s + from_x * x) / (1.0 + r * from_x * g3);
        let y3 = g3 * y;
        let xx = t0 * (x - r * y3);
        let y0 = a0 * (s[0] + f * xx);
        let y1 = a1 * (s[1] + f * y0);
        let y2 = a2 * (s[2] + f * y1);
        s[0] = ftz(s[0] + 2.0 * f * (xx - y0));
        s[1] = ftz(s[1] + 2.0 * f * (y0 - y1));
        s[2] = ftz(s[2] + 2.0 * f * (y1 - y2));
        s[3] = ftz(s[3] + 2.0 * f * (y2 - a3 * y));
        [y0, y1, y2, y3][tap]
    }

    /// The diode ladder: each stage loads the one before, so it's solved as a whole.
    #[inline(always)]
    fn diode(&mut self, t: &Tuned, x: f32) -> f32 {
        // The stages load each other, which pulls its cutoff well down: this brings it back.
        let f = t.f * 2.0;
        let (r, s) = (t.r, &mut self.s);
        let t0 = soft(x - r * s[3]);
        let (t1, t2, t3) = (soft(s[0] - s[1]), soft(s[1] - s[2]), soft(s[2] - s[3]));
        // The tridiagonal system, swept forwards once for both of what's asked of it.
        let d0 = 1.0 / (1.0 + f * (t0 + t1));
        let (u1, c1) = (f * t1, f * t2);
        let c2 = f * t3;
        let w1 = u1 * d0;
        let d1 = 1.0 / (1.0 + f * (t1 + t2) - u1 * w1);
        let w2 = c1 * d1;
        let d2 = 1.0 / (1.0 + f * (t2 + t3) - c1 * w2);
        let w3 = 2.0 * c2 * d2;
        let d3 = 1.0 / (1.0 + 2.0 * c2 - c2 * w3);
        let sweep = |b: [f32; 4]| {
            let e1 = b[1] + w1 * b[0];
            let e2 = b[2] + w2 * e1;
            let e3 = b[3] + w3 * e2;
            [b[0], e1, e2, e3]
        };
        // The last stage's output is a line in the feedback's input: find where it meets.
        let free = sweep(*s);
        let unit = sweep([f * t0, 0.0, 0.0, 0.0]);
        let (a, c) = (free[3] * d3, unit[3] * d3);
        let u = (x - r * a) / (1.0 + r * c);
        let e = sweep([s[0] + f * t0 * u, s[1], s[2], s[3]]);
        let y3 = e[3] * d3;
        let y2 = (e[2] + c2 * y3) * d2;
        let y1 = (e[1] + c1 * y2) * d1;
        let y0 = (e[0] + u1 * y1) * d0;
        let y = [y0, y1, y2, y3];
        for (st, v) in s.iter_mut().zip(y) {
            *st = ftz(2.0 * v - *st);
        }
        y3
    }

    /// Andy Simper's state-variable filter, one sample: (band, low, high).
    #[inline(always)]
    fn svf(&mut self, t: &Tuned, x: f32, which: usize) -> (f32, f32, f32) {
        let (ic1, ic2) = (self.s[which], self.s[which + 1]);
        let v3 = x - ic2;
        let v1 = t.a1 * ic1 + t.a2 * v3;
        let v2 = ic2 + t.a2 * ic1 + t.a3 * v3;
        self.s[which] = ftz(2.0 * v1 - ic1);
        self.s[which + 1] = ftz(2.0 * v2 - ic2);
        (v1, v2, x - t.k * v1 - v2)
    }

    /// Two poles whose resonance is its own limiter: it opens up, then clips against itself.
    #[inline(always)]
    fn snarl(&mut self, t: &Tuned, x: f32) -> f32 {
        let (ic1, ic2) = (self.s[0], self.s[1]);
        let push = soft(ic1 * 1.6);
        let k = 2.0 - 2.15 * t.r + 2.15 * t.r * (1.0 - push);
        let a1 = 1.0 / (1.0 + t.f * (t.f + k));
        let (a2, a3) = (t.f * a1, t.f * t.f * a1);
        let v3 = x - ic2;
        let v1 = a1 * ic1 + a2 * v3;
        let v2 = ic2 + a2 * ic1 + a3 * v3;
        self.s[0] = ftz(2.0 * v1 - ic1);
        self.s[1] = ftz(2.0 * v2 - ic2);
        v2
    }

    fn comb(&mut self, t: &Tuned, x: f32, sign: f32) -> f32 {
        self.combed = true;
        let at = self.at as f32 + COMB as f32 - t.delay;
        let whole = at as usize % COMB;
        let part = at.fract();
        let (a, b) = (self.comb[whole], self.comb[(whole + 1) % COMB]);
        let late = a + (b - a) * part;
        let y = x + sign * late * (t.r * 0.98);
        self.comb[self.at] = ftz(y);
        self.at = (self.at + 1) % COMB;
        (x + y) * 0.5
    }

    fn formant(&mut self, t: &Tuned, x: f32) -> f32 {
        let mut out = 0.0;
        for (n, (b, c)) in self.bands.iter_mut().zip(&t.bands).enumerate() {
            let v3 = x - b[1];
            let v1 = c[0] * b[0] + c[1] * v3;
            let v2 = b[1] + c[1] * b[0] + c[2] * v3;
            b[0] = ftz(2.0 * v1 - b[0]);
            b[1] = ftz(2.0 * v2 - b[1]);
            out += v1 * [1.0, 0.7, 0.45][n];
        }
        out * 0.6
    }

    fn phaser(&mut self, t: &Tuned, x: f32) -> f32 {
        let c = (t.f - 1.0) / (t.f + 1.0);
        let mut y = x + self.held * t.r * 0.85;
        for p in &mut self.pass {
            let out = c * y + *p;
            *p = ftz(y - c * out);
            y = out;
        }
        self.held = y;
        (x + y) * 0.5
    }

    /// A block through `kind`, in place.
    pub fn run(&mut self, kind: PolyFilter, t: &Tuned, x: &mut [f32]) {
        let sides = Sides {
            one: self,
            two: None,
            t: [t, t],
            l: x,
            r: &mut [],
        };
        sides.through(kind);
    }

    /// Left and right together, in place, each with its own tuning.
    pub fn run_both(
        kind: PolyFilter,
        [one, two]: &mut [Stage; 2],
        t: [&Tuned; 2],
        [l, r]: [&mut [f32]; 2],
    ) {
        let two = Some(two);
        Sides { one, two, t, l, r }.through(kind);
    }
}

/// A filter's samples each lean on the last, so one side can't be hurried. Side by side in one
/// loop the processor runs both at once.
struct Sides<'a> {
    one: &'a mut Stage,
    two: Option<&'a mut Stage>,
    t: [&'a Tuned; 2],
    l: &'a mut [f32],
    r: &'a mut [f32],
}

impl Sides<'_> {
    #[inline(always)]
    fn each(self, f: impl Fn(&mut Stage, &Tuned, f32) -> f32) {
        let [t, u] = self.t;
        let one = self.one;
        match self.two {
            Some(two) => {
                for (a, b) in self.l.iter_mut().zip(self.r.iter_mut()) {
                    *a = f(one, t, *a);
                    *b = f(two, u, *b);
                }
            }
            None => {
                for a in self.l.iter_mut() {
                    *a = f(one, t, *a);
                }
            }
        }
    }

    fn through(self, kind: PolyFilter) {
        let (push, back) = (self.t[0].drive, self.t[0].makeup);
        match kind {
            PolyFilter::None => {}
            PolyFilter::Ladder => self.each(|s, t, v| s.ladder(t, v * push, 3, false) * back),
            PolyFilter::Ladder2 => self.each(|s, t, v| s.ladder(t, v * push, 1, false) * back),
            PolyFilter::Cascade => self.each(|s, t, v| s.ladder(t, v * push, 3, true) * back),
            PolyFilter::Diode => self.each(|s, t, v| s.diode(t, v * push) * back * 1.6),
            PolyFilter::Snarl => self.each(|s, t, v| s.snarl(t, v * push) * back),
            PolyFilter::Low12 => self.each(|s, t, v| s.svf(t, v, 0).1),
            PolyFilter::High12 => self.each(|s, t, v| s.svf(t, v, 0).2),
            PolyFilter::Band12 => self.each(|s, t, v| s.svf(t, v, 0).0 * t.k.max(0.3)),
            PolyFilter::Low24 => self.each(|s, t, v| {
                let a = s.svf(t, v, 0).1;
                s.svf(t, a, 2).1
            }),
            PolyFilter::High24 => self.each(|s, t, v| {
                let a = s.svf(t, v, 0).2;
                s.svf(t, a, 2).2
            }),
            PolyFilter::Band24 => self.each(|s, t, v| {
                let a = s.svf(t, v, 0).0;
                s.svf(t, a, 2).0 * t.k.max(0.3)
            }),
            PolyFilter::Notch => self.each(|s, t, v| {
                let (_, low, high) = s.svf(t, v, 0);
                low + high
            }),
            PolyFilter::Peak => self.each(|s, t, v| {
                let (_, low, high) = s.svf(t, v, 0);
                low - high
            }),
            PolyFilter::CombUp => self.each(|s, t, v| s.comb(t, v, 1.0)),
            PolyFilter::CombDown => self.each(|s, t, v| s.comb(t, v, -1.0)),
            PolyFilter::Formant => self.each(|s, t, v| s.formant(t, v)),
            PolyFilter::Phaser => self.each(|s, t, v| s.phaser(t, v)),
            PolyFilter::Ring => self.each(|s, t, v| {
                s.clock = (s.clock + t.cycle).fract();
                let m = (TAU * s.clock).sin();
                v * (1.0 - t.r) + v * m * t.r.max(0.5)
            }),
            PolyFilter::Crush => {
                let steps = 2f32.powf(12.0 - 9.0 * self.t[0].r);
                self.each(|s, t, v| {
                    s.clock += t.cycle * 2.0;
                    if s.clock >= 1.0 {
                        s.clock = s.clock.fract();
                        s.held = (v * steps).round() / steps;
                    }
                    s.held
                })
            }
        }
    }
}

impl Default for Stage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 96_000.0;

    fn tone(hz: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| 0.2 * (TAU * hz * i as f32 / RATE).sin())
            .collect()
    }

    fn through(kind: PolyFilter, hz: f32, res: f32, input: &[f32]) -> Vec<f32> {
        let mut s = Stage::new();
        let t = Tuned::new(kind, hz, res, 0.0, 0.0, RATE);
        let mut x = input.to_vec();
        for c in x.chunks_mut(32) {
            s.run(kind, &t, c);
        }
        x
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
    }

    fn pitch(x: &[f32]) -> f32 {
        let ups: Vec<f32> = x
            .windows(2)
            .enumerate()
            .filter(|(_, w)| w[0] <= 0.0 && w[1] > 0.0)
            .map(|(i, w)| i as f32 + w[0] / (w[0] - w[1]))
            .collect();
        (ups.len() - 1) as f32 / (ups[ups.len() - 1] - ups[0]) * RATE
    }

    #[test]
    fn the_low_passes_fall_at_their_slopes() {
        // Two octaves over the cutoff: 24 dB an octave lets through about a sixty-fourth...
        for (kind, most) in [
            (PolyFilter::Ladder, 0.04),
            (PolyFilter::Cascade, 0.04),
            (PolyFilter::Low24, 0.08),
            (PolyFilter::Low12, 0.12),
            (PolyFilter::Ladder2, 0.2),
        ] {
            let high = tone(4000.0, 48_000);
            let y = through(kind, 1000.0, 0.0, &high);
            let gain = rms(&y[24_000..]) / rms(&high);
            assert!(gain < most, "{kind:?} {gain}");
            // ...and well under the cutoff, it all comes through.
            let low = tone(100.0, 48_000);
            let y = through(kind, 1000.0, 0.0, &low);
            let gain = rms(&y[24_000..]) / rms(&low);
            assert!((gain - 1.0).abs() < 0.15, "{kind:?} {gain}");
        }
    }

    #[test]
    fn the_circuits_sing_at_their_cutoff_when_pushed_to_it() {
        // A click into full resonance: a note, its pitch the cutoff, right up the keyboard.
        for hz in [220.0, 880.0, 3520.0] {
            for kind in [PolyFilter::Ladder, PolyFilter::Snarl] {
                let mut click = vec![0f32; 96_000];
                click[0] = 0.5;
                let y = through(kind, hz, 1.0, &click);
                let tail = &y[48_000..];
                assert!(rms(tail) > 0.02, "{kind:?} {hz}: {}", rms(tail));
                let p = pitch(tail);
                assert!((p / hz - 1.0).abs() < 0.04, "{kind:?} {hz}: sang {p}");
            }
        }
    }

    #[test]
    fn the_diode_filter_squelches_and_stays_in_bounds() {
        let saw: Vec<f32> = (0..48_000)
            .map(|i| 0.5 * (2.0 * (i as f32 * 110.0 / RATE).fract() - 1.0))
            .collect();
        let y = through(PolyFilter::Diode, 800.0, 0.8, &saw);
        assert!(y.iter().all(|v| v.is_finite()));
        assert!(
            rms(&y[24_000..]) > 0.05 && rms(&y[24_000..]) < 1.5,
            "{}",
            rms(&y[24_000..])
        );
    }

    #[test]
    fn every_filter_survives_anything() {
        for kind in PolyFilter::ALL {
            for (hz, res, drive) in [
                (20.0, 1.0, 1.0),
                (20_000.0, 1.0, 1.0),
                (800.0, 0.0, 0.0),
                (60.0, 0.5, 1.0),
            ] {
                let mut s = Stage::new();
                let t = Tuned::new(kind, hz, res, drive, 1.0, RATE);
                let mut x: Vec<f32> = (0..9600)
                    .map(|i| if i % 97 < 48 { 1.0 } else { -1.0 })
                    .collect();
                s.run(kind, &t, &mut x);
                assert!(x.iter().all(|v| v.is_finite()), "{kind:?} {hz}");
                let top = x.iter().fold(0f32, |m, v| m.max(v.abs()));
                assert!(top < 40.0, "{kind:?} {hz} {res}: {top}");
            }
        }
    }

    #[test]
    fn a_comb_rings_at_its_note() {
        let mut click = vec![0f32; 48_000];
        click[0] = 1.0;
        let y = through(PolyFilter::CombUp, 440.0, 1.0, &click);
        let p = pitch(&y[1000..12_000]);
        assert!((p / 440.0 - 1.0).abs() < 0.03, "{p}");
    }

    #[test]
    fn drive_saturates_it_rather_than_just_louder() {
        let x = tone(110.0, 48_000);
        let run = |drive| {
            let mut s = Stage::new();
            let t = Tuned::new(PolyFilter::Ladder, 2000.0, 0.2, drive, 0.0, RATE);
            let mut y = x.clone();
            s.run(PolyFilter::Ladder, &t, &mut y);
            y
        };
        let (clean, driven) = (run(0.0), run(1.0));
        // The third harmonic, nothing in a clean sine, comes up with the drive.
        let third = |y: &[f32]| {
            let (mut re, mut im) = (0f32, 0f32);
            for (i, s) in y[24_000..].iter().enumerate() {
                let ph = TAU * 330.0 * i as f32 / RATE;
                (re, im) = (re + s * ph.cos(), im + s * ph.sin());
            }
            (re * re + im * im).sqrt() / 24_000.0 * 2.0
        };
        assert!(
            third(&driven) > third(&clean) * 10.0,
            "{} {}",
            third(&clean),
            third(&driven)
        );
        assert!(rms(&driven[24_000..]) < rms(&clean[24_000..]) * 6.0);
    }
}

#[cfg(test)]
#[test]
#[ignore = "a benchmark: cargo test --release -p pina-engine filter_load -- --ignored --nocapture"]
fn filter_load() {
    for kind in PolyFilter::ALL {
        let mut s = Stage::new();
        let t = Tuned::new(kind, 2000.0, 0.4, 0.3, 0.0, 96_000.0);
        let mut x: Vec<f32> = (0..16)
            .map(|i| 0.5 * ((i as f32 * 0.21).sin() + (i as f32 * 0.77).sin()))
            .collect();
        let start = std::time::Instant::now();
        let rounds = 400_000;
        for _ in 0..rounds {
            s.run(kind, &t, &mut x);
            std::hint::black_box(&mut x);
        }
        let ns = start.elapsed().as_nanos() as f64 / (rounds * 16) as f64;
        println!("{:<12} {ns:>6.1} ns a sample", format!("{kind:?}"));
    }
}
