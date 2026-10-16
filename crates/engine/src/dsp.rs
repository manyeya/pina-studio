//! The channel strip's built-in processors, as Logic's Channel EQ and Compressor: a low cut, two
//! bells and a high shelf; a soft-knee compressor; and a saturator. The sound is Faust's
//! (`crates/engine/faust/*.dsp`, compiled into `faust/`); this runs it, and works out what the UI
//! draws: the EQ's curve and the saturator's.

use std::f64::consts::PI;

use crate::{
    engine::MAX_BLOCK,
    faust::{FaustDsp, ParamIndex, comp::CompDsp, controls, eq::EqDsp, sat::SatDsp},
    project::{Band, Comp, Eq, Saturator},
};

/// The lowest low cut: at or under this, it's off.
pub const LOW_CUT_OFF: f32 = 20.0;

/// Blocks of silence run through an effect that's just come on, so its smoothed controls start
/// where they're set rather than gliding there.
const PRIME_BLOCKS: usize = 8;

/// A channel's built-in effects, running: the Faust EQ, compressor and saturator, which keep their
/// memory through edits.
pub struct Strip {
    eq: Box<EqDsp>,
    comp: Box<CompDsp>,
    sat: Box<SatDsp>,
    eq_controls: [ParamIndex; 10],
    comp_controls: [ParamIndex; 6],
    sat_controls: [ParamIndex; 4],
    /// Which ran last block: one coming back starts afresh.
    on: [bool; 3],
    /// A copy of the block going in; and room for what comes out while priming, the third the
    /// compressor's gain reduction.
    input: [Vec<f32>; 2],
    out: [Vec<f32>; 3],
}

impl Strip {
    pub fn new(rate: f64) -> Self {
        let rate = rate as i32;
        let mut eq = Box::new(EqDsp::new());
        let mut comp = Box::new(CompDsp::new());
        let mut sat = Box::new(SatDsp::new());
        eq.init(rate);
        comp.init(rate);
        sat.init(rate);
        let eq_controls = controls(
            &*eq,
            [
                "low_cut",
                "low_mid_freq",
                "low_mid_gain",
                "low_mid_q",
                "high_mid_freq",
                "high_mid_gain",
                "high_mid_q",
                "shelf_freq",
                "shelf_gain",
                "shelf_q",
            ],
        );
        let comp_controls = controls(
            &*comp,
            ["threshold", "ratio", "attack", "release", "makeup", "rms"],
        );
        let sat_controls = controls(&*sat, ["drive", "tone", "mix", "output"]);
        let block = || vec![0.0; MAX_BLOCK];
        Strip {
            eq,
            comp,
            sat,
            eq_controls,
            comp_controls,
            sat_controls,
            on: [false; 3],
            input: [block(), block()],
            out: [block(), block(), block()],
        }
    }

    /// Runs a block (at most `MAX_BLOCK`) through whichever of the EQ, compressor and saturator
    /// are given, in place; returns the most the compressor turned it down, in dB.
    /// Allocation-free.
    pub fn process(
        &mut self,
        eq: Option<&Eq>,
        comp: Option<&Comp>,
        sat: Option<&Saturator>,
        l: &mut [f32],
        r: &mut [f32],
    ) -> f32 {
        let n = l.len().min(MAX_BLOCK);
        let (l, r) = (&mut l[..n], &mut r[..n]);
        if let Some(e) = eq {
            let values = [
                e.low_cut,
                e.low_mid.freq,
                e.low_mid.gain_db,
                e.low_mid.q,
                e.high_mid.freq,
                e.high_mid.gain_db,
                e.high_mid.q,
                e.high_shelf.freq,
                e.high_shelf.gain_db,
                e.high_shelf.q,
            ];
            set(&mut *self.eq, &self.eq_controls, &values);
            if !self.on[0] {
                prime(&mut *self.eq, &mut self.input, &mut self.out);
            }
            self.input[0][..n].copy_from_slice(l);
            self.input[1][..n].copy_from_slice(r);
            let [il, ir] = &self.input;
            self.eq
                .compute(n, &[&il[..n], &ir[..n]], &mut [&mut *l, &mut *r]);
        }
        let mut most = 0f32;
        if let Some(c) = comp {
            let values = [
                c.threshold_db,
                c.ratio,
                c.attack_ms,
                c.release_ms,
                c.makeup_db,
                c.rms as u8 as f32,
            ];
            set(&mut *self.comp, &self.comp_controls, &values);
            if !self.on[1] {
                prime(&mut *self.comp, &mut self.input, &mut self.out);
            }
            self.input[0][..n].copy_from_slice(l);
            self.input[1][..n].copy_from_slice(r);
            let [il, ir] = &self.input;
            let gr = &mut self.out[2][..n];
            self.comp
                .compute(n, &[&il[..n], &ir[..n]], &mut [&mut *l, &mut *r, &mut *gr]);
            most = gr.iter().fold(0.0, |m, &g| m.max(g));
        }
        if let Some(s) = sat {
            let values = [s.drive_db, s.tone, s.mix, s.output_db];
            set(&mut *self.sat, &self.sat_controls, &values);
            if !self.on[2] {
                prime(&mut *self.sat, &mut self.input, &mut self.out);
            }
            self.input[0][..n].copy_from_slice(l);
            self.input[1][..n].copy_from_slice(r);
            let [il, ir] = &self.input;
            self.sat.compute(n, &[&il[..n], &ir[..n]], &mut [l, r]);
        }
        self.on = [eq.is_some(), comp.is_some(), sat.is_some()];
        most
    }
}

fn set(dsp: &mut dyn FaustDsp<T = f32>, controls: &[ParamIndex], values: &[f32]) {
    for (&c, &v) in controls.iter().zip(values) {
        dsp.set_param(c, v);
    }
}

/// Clears an effect that's just come on and runs silence through it, its controls already set,
/// so it starts settled.
fn prime(dsp: &mut dyn FaustDsp<T = f32>, silence: &mut [Vec<f32>; 2], out: &mut [Vec<f32>; 3]) {
    dsp.instance_clear();
    let outputs = dsp.get_num_outputs() as usize;
    for _ in 0..PRIME_BLOCKS {
        silence[0].fill(0.0);
        silence[1].fill(0.0);
        let [a, b, c] = out;
        let outs: &mut [&mut [f32]] = &mut [&mut a[..], &mut b[..], &mut c[..]];
        dsp.compute(
            MAX_BLOCK as i32,
            &[&silence[0][..], &silence[1][..]],
            &mut outs[..outputs],
        );
    }
}

/// Second-order filter coefficients, normalized so a0 is 1 (RBJ's audio EQ cookbook): the
/// responses of the Faust EQ's state-variable filters, for drawing its curve.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Biquad {
    fn new(b: [f64; 3], a: [f64; 3]) -> Self {
        Biquad {
            b0: b[0] / a[0],
            b1: b[1] / a[0],
            b2: b[2] / a[0],
            a1: a[1] / a[0],
            a2: a[2] / a[0],
        }
    }

    fn highpass(freq: f64, q: f64, rate: f64) -> Self {
        let w = 2.0 * PI * freq / rate;
        let (cos, alpha) = (w.cos(), w.sin() / (2.0 * q));
        Biquad::new(
            [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0],
            [1.0 + alpha, -2.0 * cos, 1.0 - alpha],
        )
    }

    fn bell(freq: f64, gain_db: f64, q: f64, rate: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let w = 2.0 * PI * freq / rate;
        let (cos, alpha) = (w.cos(), w.sin() / (2.0 * q));
        Biquad::new(
            [1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a],
            [1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a],
        )
    }

    fn high_shelf(freq: f64, gain_db: f64, q: f64, rate: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let w = 2.0 * PI * freq / rate;
        let (cos, alpha) = (w.cos(), w.sin() / (2.0 * q));
        let s = 2.0 * a.sqrt() * alpha;
        Biquad::new(
            [
                a * ((a + 1.0) + (a - 1.0) * cos + s),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                a * ((a + 1.0) + (a - 1.0) * cos - s),
            ],
            [
                (a + 1.0) - (a - 1.0) * cos + s,
                2.0 * ((a - 1.0) - (a + 1.0) * cos),
                (a + 1.0) - (a - 1.0) * cos - s,
            ],
        )
    }

    /// The gain at a frequency, in dB.
    fn response_db(&self, freq: f64, rate: f64) -> f64 {
        let w = 2.0 * PI * freq / rate;
        // H(z) at z = e^jw, as (re, im) of numerator and denominator in z^-1.
        let (c1, s1, c2, s2) = (w.cos(), -w.sin(), (2.0 * w).cos(), -(2.0 * w).sin());
        let num = (
            self.b0 + self.b1 * c1 + self.b2 * c2,
            self.b1 * s1 + self.b2 * s2,
        );
        let den = (
            1.0 + self.a1 * c1 + self.a2 * c2,
            self.a1 * s1 + self.a2 * s2,
        );
        let mag = (num.0.hypot(num.1) / den.0.hypot(den.1)).max(1e-9);
        20.0 * mag.log10()
    }
}

fn safe(band: &Band, rate: f64) -> (f64, f64, f64) {
    let freq = (band.freq as f64).clamp(10.0, rate * 0.45);
    (freq, band.gain_db as f64, (band.q as f64).clamp(0.1, 18.0))
}

/// What an EQ does at `freq`, in dB: for drawing its curve.
pub fn eq_response(eq: &Eq, freq: f64, rate: f64) -> f64 {
    let (f1, g1, q1) = safe(&eq.low_mid, rate);
    let (f2, g2, q2) = safe(&eq.high_mid, rate);
    let (f3, g3, q3) = safe(&eq.high_shelf, rate);
    let mut db = Biquad::bell(f1, g1, q1, rate).response_db(freq, rate)
        + Biquad::bell(f2, g2, q2, rate).response_db(freq, rate)
        + Biquad::high_shelf(f3, g3, q3, rate).response_db(freq, rate);
    if eq.low_cut > LOW_CUT_OFF {
        let cut = (eq.low_cut as f64).min(rate * 0.45);
        let q = std::f64::consts::FRAC_1_SQRT_2;
        db += Biquad::highpass(cut, q, rate).response_db(freq, rate);
    }
    db
}

/// The saturator's curve, what a steady level `x` comes out at, mix and all: for drawing it.
/// (The Faust code averages the same curve between samples, at twice the rate.)
pub fn sat_curve(s: &Saturator, x: f32) -> f32 {
    const BIAS: f32 = 0.15;
    let drive = 10f32.powf(s.drive_db.clamp(0.0, 24.0) / 20.0);
    let b = BIAS.tanh();
    let wet = ((drive * x + BIAS).tanh() - b) / (drive * (1.0 - b * b));
    x + (wet - x) * s.mix.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;

    fn sine(freq: f64, n: usize, level: f32) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * PI * freq * i as f64 / RATE).sin() as f32 * level)
            .collect()
    }

    fn peak(s: &[f32]) -> f32 {
        s.iter().fold(0.0, |m, x| m.max(x.abs()))
    }

    /// Runs a second of sine through a strip a block at a time; returns what came out and the
    /// most gain reduction reported.
    fn run(
        eq: Option<&Eq>,
        comp: Option<&Comp>,
        sat: Option<&Saturator>,
        freq: f64,
        level: f32,
    ) -> (Vec<f32>, f32) {
        let mut strip = Strip::new(RATE);
        let (mut l, mut r) = (sine(freq, 48_000, level), sine(freq, 48_000, level));
        let mut most = 0f32;
        for at in (0..l.len()).step_by(MAX_BLOCK) {
            let end = (at + MAX_BLOCK).min(l.len());
            most = most.max(strip.process(eq, comp, sat, &mut l[at..end], &mut r[at..end]));
        }
        assert_eq!(l, r); // both sides alike, as they went in
        (l, most)
    }

    /// What an EQ does to a sine, in dB, once it settles. (By the sine's own level: its sample
    /// peaks can miss its crests, as at a quarter of the sample rate.)
    fn through(eq: &Eq, freq: f64) -> f64 {
        let (out, _) = run(Some(eq), None, None, freq, 0.5);
        level_at(&out, freq) - 20.0 * 0.5f64.log10()
    }

    /// The level of one frequency in the last half second of `s`, in dB (one DFT bin).
    fn level_at(s: &[f32], f: f64) -> f64 {
        let tail = &s[s.len() / 2..];
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &x) in tail.iter().enumerate() {
            let w = 2.0 * PI * f * i as f64 / RATE;
            re += x as f64 * w.cos();
            im += x as f64 * w.sin();
        }
        20.0 * ((re * re + im * im).sqrt() * 2.0 / tail.len() as f64).log10()
    }

    #[test]
    fn a_flat_eq_does_nothing() {
        let eq = Eq::default();
        assert!(through(&eq, 1000.0).abs() < 0.01);
        assert_eq!(eq_response(&eq, 1000.0, RATE), 0.0);
    }

    #[test]
    fn the_curve_drawn_is_what_the_faust_eq_does() {
        let mut eq = Eq {
            low_cut: 120.0,
            ..Eq::default()
        };
        eq.low_mid = Band {
            freq: 300.0,
            gain_db: -4.0,
            q: 1.4,
        };
        eq.high_mid.gain_db = 6.0;
        eq.high_mid.freq = 2_000.0;
        eq.high_shelf.gain_db = -9.0;
        for freq in [50.0, 120.0, 300.0, 700.0, 2_000.0, 5_000.0, 12_000.0] {
            let (heard, drawn) = (through(&eq, freq), eq_response(&eq, freq, RATE));
            assert!(
                (heard - drawn).abs() < 0.15,
                "{freq} Hz: {heard} heard, {drawn} drawn"
            );
        }
        assert!(through(&eq, 30.0) < -20.0); // the low cut is steep below its corner
    }

    #[test]
    fn the_compressor_turns_loud_signals_down_by_the_ratio() {
        let comp = Comp {
            threshold_db: -20.0,
            ratio: 4.0,
            attack_ms: 1.0,
            release_ms: 50.0,
            ..Comp::default()
        };
        // 20 dB over the threshold comes out 5 dB over it: 15 dB down.
        let (out, most) = run(None, Some(&comp), None, 1000.0, 1.0);
        assert!((most - 15.0).abs() < 0.5, "{most}");
        let level = 20.0 * peak(&out[40_000..]).log10();
        assert!((level + 15.0).abs() < 1.0, "{level}");
        // Quiet: untouched.
        let (out, most) = run(None, Some(&comp), None, 1000.0, 0.01);
        assert_eq!(most, 0.0);
        assert!((peak(&out[24_000..]) - 0.01).abs() < 1e-4);
        // RMS: a sine's average is 3 dB under its peak, so it's turned down by less.
        let rms = Comp { rms: true, ..comp };
        let (_, gentler) = run(None, Some(&rms), None, 1000.0, 1.0);
        assert!((gentler - 12.75).abs() < 1.0, "{gentler}");
    }

    #[test]
    fn the_saturator_keeps_quiet_sounds_and_rounds_off_loud_ones() {
        let sat = Saturator {
            drive_db: 12.0,
            ..Saturator::default()
        };
        assert!((sat_curve(&sat, 0.001) / 0.001 - 1.0).abs() < 0.01); // quiet: unity
        assert!(sat_curve(&sat, 1.0) < 0.3); // loud: rounded off
        let level = |sat: &Saturator, freq: f64, level: f32| {
            peak(&run(None, None, Some(sat), freq, level).0[24_000..])
        };
        assert!((level(&sat, 1000.0, 0.01) - 0.01).abs() < 0.0005);
        assert!(level(&sat, 1000.0, 1.0) < 0.35);
        // Quiet highs come through too: the oversampling and ADAA cost little treble.
        assert!(level(&sat, 10_000.0, 0.01) > 0.0093);
        // All dry: untouched, but for the output level.
        let dry = Saturator {
            mix: 0.0,
            output_db: -6.0,
            ..sat
        };
        assert!((level(&dry, 1000.0, 1.0) - 0.501).abs() < 0.01);
    }

    #[test]
    fn hard_drive_hardly_aliases() {
        // A loud 7 kHz sine driven hard has harmonics far past Nyquist. A plain tanh folds them
        // back down between the harmonics: at 1, 6, 8 and 13 kHz (the 7th, 6th, 8th and 5th
        // folded at 48 kHz), and at 2, 5, 9 and 12 kHz (the 14th, 13th, 15th and 12th, folded at
        // the 96 kHz the saturator runs at).
        let sat = Saturator {
            drive_db: 24.0,
            ..Saturator::default()
        };
        let (out, _) = run(None, None, Some(&sat), 7_000.0, 0.9);
        let naive: Vec<f32> = sine(7_000.0, 48_000, 0.9)
            .iter()
            .map(|&x| sat_curve(&sat, x))
            .collect();
        let aliases = [1e3, 2e3, 5e3, 6e3, 8e3, 9e3, 12e3, 13e3];
        let worst = |s: &[f32]| {
            let fundamental = level_at(s, 7_000.0);
            let alias = aliases
                .map(|f| level_at(s, f))
                .into_iter()
                .fold(f64::MIN, f64::max);
            fundamental - alias
        };
        let (ours, plain) = (worst(&out), worst(&naive));
        assert!(
            ours > plain + 30.0 && ours > 45.0,
            "{ours} dB clear, a plain tanh {plain}"
        );
    }
}

#[cfg(test)]
mod cost {
    use super::*;

    /// How long a channel strip takes, all three effects on, as a share of real time: run with
    /// `cargo test --release -p tempo-engine strip_cost -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn strip_cost() {
        let rate = 48_000.0;
        let mut strip = Strip::new(rate);
        let (eq, comp, sat) = (
            Eq {
                low_cut: 80.0,
                ..Eq::default()
            },
            Comp::default(),
            Saturator::default(),
        );
        let seconds = 10;
        let mut l: Vec<f32> = (0..MAX_BLOCK)
            .map(|i| (i as f32 * 0.05).sin() * 0.5)
            .collect();
        let mut r = l.clone();
        let mut time = |e: Option<&Eq>, c: Option<&Comp>, s: Option<&Saturator>| {
            let began = std::time::Instant::now();
            for _ in 0..(seconds * 48_000) / MAX_BLOCK {
                strip.process(e, c, s, &mut l, &mut r);
            }
            began.elapsed().as_secs_f64() / seconds as f64 * 100.0
        };
        let (e, c, s) = (Some(&eq), Some(&comp), Some(&sat));
        let parts = [
            time(e, None, None),
            time(None, c, None),
            time(None, None, s),
        ];
        let all = time(e, c, s);
        eprintln!(
            "% of real time: EQ {:.3}, compressor {:.3}, saturator {:.3}, all {all:.3}",
            parts[0], parts[1], parts[2]
        );
        assert!(all < 1.0);
    }
}
