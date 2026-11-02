use super::*;
use crate::song::project::{SatBands, sat_crossovers};
use std::f64::consts::PI;

const RATE: f64 = 48_000.0;

fn sine(freq: f64, n: usize, level: f32) -> Vec<f32> {
    (0..n)
        .map(|i| (2.0 * PI * freq * i as f64 / RATE).sin() as f32 * level)
        .collect()
}

fn run(s: &Saturator, input: &[f32]) -> Vec<f32> {
    let mut sat = Saturation::new(RATE);
    let (mut l, mut r) = (input.to_vec(), input.to_vec());
    for at in (0..l.len()).step_by(512) {
        let end = (at + 512).min(l.len());
        sat.process(s, &mut l[at..end], &mut r[at..end]);
    }
    assert_eq!(l, r);
    l
}

fn peak(s: &[f32]) -> f32 {
    s.iter().fold(0.0, |m, x| m.max(x.abs()))
}

/// A frequency's level in `s` (its second half), by correlation.
fn level_at(s: &[f32], f: f64) -> f64 {
    let s = &s[s.len() / 2..];
    let (mut re, mut im) = (0.0, 0.0);
    for (i, x) in s.iter().enumerate() {
        let a = 2.0 * PI * f * i as f64 / RATE;
        re += *x as f64 * a.cos();
        im += *x as f64 * a.sin();
    }
    20.0 * ((re * re + im * im).sqrt() * 2.0 / s.len() as f64)
        .max(1e-12)
        .log10()
}

/// Everything in `s` (its second half) but its `f` tone, in dB against that tone: what was added.
fn added(s: &[f32], f: f64) -> f64 {
    let s = &s[s.len() / 2..];
    let n = s.len() as f64;
    let (mut a, mut b) = (0.0, 0.0);
    for (i, x) in s.iter().enumerate() {
        let w = 2.0 * PI * f * i as f64 / RATE;
        (a, b) = (a + *x as f64 * w.sin(), b + *x as f64 * w.cos());
    }
    let (a, b) = (a * 2.0 / n, b * 2.0 / n);
    let rest: f64 = s
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let w = 2.0 * PI * f * i as f64 / RATE;
            (*x as f64 - a * w.sin() - b * w.cos()).powi(2)
        })
        .sum::<f64>()
        / n;
    10.0 * (rest / ((a * a + b * b) / 2.0)).log10()
}

fn band(style: SatStyle, drive_db: f32) -> SatBand {
    SatBand {
        style,
        drive_db,
        ..SatBand::default()
    }
}

fn with(bands: &[SatBand]) -> Saturator {
    Saturator {
        bands: SatBands::from_slice(bands),
        crossovers: sat_crossovers(bands.len()),
        ..Saturator::default()
    }
}

#[test]
fn bands_left_dry_add_back_flat_at_every_slope() {
    for slope in [12, 24, 48] {
        let mut s = with(&[band(SatStyle::Heavy, 24.0); 4]);
        s.slope = slope;
        for b in s.bands.iter_mut() {
            b.mix = 0.0;
        }
        for f in [60.0, 300.0, 1_000.0, 3_000.0, 9_000.0] {
            let out = run(&s, &sine(f, 24_000, 0.5));
            let db = level_at(&out, f) - level_at(&sine(f, 24_000, 0.5), f);
            assert!(db.abs() < 0.1, "{slope} dB/oct at {f} Hz: {db} dB");
        }
    }
}

#[test]
fn every_style_keeps_quiet_sounds_and_changes_loud_ones() {
    for style in SatStyle::ALL {
        let s = with(&[band(style, 18.0)]);
        let quiet = run(&s, &sine(1_000.0, 24_000, 0.001));
        let loud = run(&s, &sine(1_000.0, 24_000, 0.8));
        assert!(loud.iter().all(|x| x.is_finite()), "{style:?}");
        // What it adds to a pure tone: harmonics, steps, folds.
        let added = added(&loud, 1_000.0);
        assert!(added > -40.0, "{style:?}: {added} dB added");
        if curve(style).unity {
            let db = level_at(&quiet, 1_000.0) - 20.0 * 0.001f64.log10();
            assert!(db.abs() < 0.5, "{style:?}: quiet comes out {db} dB");
        }
    }
}

#[test]
fn more_oversampling_folds_back_less() {
    // A loud 7 kHz sine through the heavy amp: what lands between its harmonics is aliasing.
    let aliased = |quality: u8| {
        let s = Saturator {
            quality,
            ..with(&[band(SatStyle::Heavy, 24.0)])
        };
        let out = run(&s, &sine(7_000.0, 48_000, 0.9));
        [1e3, 2e3, 5e3, 6e3, 8e3, 9e3, 12e3, 13e3]
            .map(|f| level_at(&out, f))
            .into_iter()
            .fold(f64::MIN, f64::max)
    };
    assert!(
        aliased(8) < aliased(2) - 10.0,
        "{} {}",
        aliased(8),
        aliased(2)
    );
}

#[test]
fn dynamics_bring_loud_bands_down_and_feedback_stays_bounded() {
    let level = |s: &Saturator| peak(&run(s, &sine(500.0, 24_000, 0.9))[12_000..]);
    let plain = with(&[band(SatStyle::CleanTube, 6.0)]);
    let squeezed = with(&[SatBand {
        dynamics: 1.0,
        ..band(SatStyle::CleanTube, 6.0)
    }]);
    assert!(level(&squeezed) < level(&plain) * 0.7);
    let ringing = with(&[SatBand {
        feedback: 1.0,
        feedback_hz: 800.0,
        ..band(SatStyle::WarmTube, 24.0)
    }]);
    let out = run(&ringing, &sine(200.0, 24_000, 0.9));
    assert!(out.iter().all(|x| x.is_finite()) && peak(&out) < 8.0);
}

#[test]
fn a_band_soloed_or_muted_is_all_or_none_of_what_comes_out() {
    let mut s = with(&[band(SatStyle::CleanTape, 6.0); 2]);
    s.crossovers[0] = 1_000.0;
    s.bands[1].solo = true;
    let low = run(&s, &sine(100.0, 24_000, 0.3));
    assert!(level_at(&low, 100.0) < -40.0);
    s.bands[1].solo = false;
    s.bands[0].mute = true;
    let high = run(&s, &sine(5_000.0, 24_000, 0.3));
    assert!((level_at(&high, 5_000.0) - 20.0 * 0.3f64.log10()).abs() < 1.0);
}

#[test]
fn a_saturator_saved_before_bands_opens_as_one_warm_tape_band() {
    let old: Saturator =
        serde_json::from_str(r#"{"drive_db": 9, "tone": -0.2, "mix": 0.5, "output_db": -1}"#)
            .unwrap();
    assert_eq!(old.bands.len(), 1);
    assert_eq!(old.bands[0].style, SatStyle::WarmTape);
    assert_eq!((old.bands[0].drive_db, old.bands[0].tone), (9.0, -0.2));
    assert_eq!((old.mix, old.output_db), (0.5, -1.0));
    // And the new kind, as written, reads back the same.
    let mut s = with(&[
        band(SatStyle::Transformer, 3.0),
        band(SatStyle::Crunch, 12.0),
    ]);
    s.quality = 8;
    let back: Saturator = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(back, s);
}

#[test]
fn bands_split_where_asked_and_come_out_again() {
    let mut s = Saturator::default();
    assert_eq!(s.split(500.0), Some(1));
    assert_eq!(s.split(4_000.0), Some(2));
    // Too near a split already, and outside what's heard.
    assert_eq!((s.split(560.0), s.split(30_000.0)), (None, None));
    assert_eq!(s.bands.len(), 3);
    assert_eq!(&s.crossovers[..2], &[500.0, 4_000.0]);
    assert_eq!(
        (s.band_at(100.0), s.band_at(1_000.0), s.band_at(9_000.0)),
        (0, 1, 2)
    );
    s.remove(1);
    assert_eq!((s.bands.len(), s.crossovers[0]), (2, 4_000.0));
    s.remove(0);
    s.remove(0);
    assert_eq!(s.bands.len(), 1);
}
