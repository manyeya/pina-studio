use super::*;
use crate::song::project::{Bands, EqBand, Shape};
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
    cut.slope = 24.0;
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
