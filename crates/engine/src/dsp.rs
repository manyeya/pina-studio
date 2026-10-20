//! The channel strip's built-in processors: the EQ (`eq.rs`), a soft-knee compressor and a
//! saturator. The compressor's and saturator's sound is Faust's (`crates/engine/faust/*.dsp`,
//! compiled into `faust/`); this runs them, and works out the saturator's curve for the UI.

use crate::{
    engine::MAX_BLOCK,
    eq::{Made, Processor, Tap},
    faust::{FaustDsp, ParamIndex, comp::CompDsp, controls, sat::SatDsp},
    project::{Comp, Eq, Saturator},
};

pub use crate::eq::eq_response;

/// The lowest low cut: at or under this, it's off.
pub const LOW_CUT_OFF: f32 = 20.0;

/// Blocks of silence run through an effect that's just come on, so its smoothed controls start
/// where they're set rather than gliding there.
const PRIME_BLOCKS: usize = 8;

/// A channel's built-in effects, running: the EQ, compressor and saturator, which keep their
/// memory through edits.
pub struct Strip {
    eq: Box<Processor>,
    comp: Box<CompDsp>,
    sat: Box<SatDsp>,
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
        let eq = Box::new(Processor::new(rate));
        let rate = rate as i32;
        let mut comp = Box::new(CompDsp::new());
        let mut sat = Box::new(SatDsp::new());
        comp.init(rate);
        sat.init(rate);
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
            comp_controls,
            sat_controls,
            on: [false; 3],
            input: [block(), block()],
            out: [block(), block(), block()],
        }
    }

    /// Runs a block (at most `MAX_BLOCK`) through whichever of the EQ, compressor and saturator
    /// are given, in place; returns the most the compressor turned it down, in dB. `made` is
    /// what the EQ's linear phase and spectral dynamics need; `sidechain` what its dynamic bands
    /// can listen to; `tap`, when the EQ window watches this
    /// channel, gets what the EQ heard and played (and says which band to hear alone).
    /// Allocation-free.
    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        eq: Option<&Eq>,
        made: Option<&Made>,
        comp: Option<&Comp>,
        sat: Option<&Saturator>,
        l: &mut [f32],
        r: &mut [f32],
        sidechain: Option<[&[f32]; 2]>,
        tap: Option<&Tap>,
    ) -> f32 {
        let n = l.len().min(MAX_BLOCK);
        let (l, r) = (&mut l[..n], &mut r[..n]);
        match eq {
            Some(e) => {
                let solo = tap.and_then(Tap::solo);
                self.eq.process(e, made, solo, l, r, sidechain, tap);
            }
            None => self.eq.rest(),
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
    use crate::project::{Bands, EqBand, Shape};
    use std::f64::consts::PI;

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
        let made = eq.map(|e| Made::new(e, RATE));
        let (mut l, mut r) = (sine(freq, 48_000, level), sine(freq, 48_000, level));
        let mut most = 0f32;
        for at in (0..l.len()).step_by(MAX_BLOCK) {
            let end = (at + MAX_BLOCK).min(l.len());
            most = most.max(strip.process(
                eq,
                made.as_ref(),
                comp,
                sat,
                &mut l[at..end],
                &mut r[at..end],
                None,
                None,
            ));
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
    fn the_curve_drawn_is_what_the_strip_eq_does() {
        let mut cut = EqBand::new(Shape::LowCut, 120.0, 0.0);
        cut.slope = 24;
        let mut bell = EqBand::new(Shape::Bell, 300.0, -4.0);
        bell.q = 1.4;
        let eq = Eq {
            bands: Bands::from_slice(&[
                cut,
                bell,
                EqBand::new(Shape::Bell, 2_000.0, 6.0),
                EqBand::new(Shape::HighShelf, 10_000.0, -9.0),
            ]),
            ..Eq::default()
        };
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
                bands: crate::project::Bands::from_slice(&[
                    crate::project::EqBand::new(crate::project::Shape::LowCut, 80.0, 0.0),
                    crate::project::EqBand::new(crate::project::Shape::Bell, 300.0, -3.0),
                    crate::project::EqBand::new(crate::project::Shape::Bell, 3_000.0, 2.0),
                    crate::project::EqBand::new(crate::project::Shape::HighShelf, 10_000.0, 2.0),
                ]),
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
                strip.process(e, None, c, s, &mut l, &mut r, None, None);
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
