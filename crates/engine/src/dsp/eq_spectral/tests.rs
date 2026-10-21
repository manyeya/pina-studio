use super::*;
use crate::song::project::{Bands, Dynamic, Shape};
use std::f64::consts::PI;

const RATE: f64 = 48_000.0;

fn run(e: &Eq, l: &[f32]) -> Vec<f32> {
    let plan = Plan::new(e);
    let mut stage = Runner::default();
    let (mut a, mut b) = (l.to_vec(), l.to_vec());
    for at in (0..l.len()).step_by(512) {
        let end = (at + 512).min(l.len());
        stage.process(&plan, e, RATE, &mut a[at..end], &mut b[at..end], None);
    }
    a
}

fn level(s: &[f32], f: f64) -> f64 {
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &x) in s.iter().enumerate() {
        let w = 2.0 * PI * f * i as f64 / RATE;
        re += x as f64 * w.cos();
        im += x as f64 * w.sin();
    }
    20.0 * ((re * re + im * im).sqrt() * 2.0 / s.len() as f64).log10()
}

#[test]
fn untouched_it_is_the_sound_a_window_late() {
    let mut b = EqBand::new(Shape::Bell, 1_000.0, 0.0);
    b.dynamic = Some(Dynamic {
        range_db: -6.0,
        threshold_db: Some(0.0),
        spectral: true,
        ..Dynamic::default()
    });
    let e = Eq {
        bands: Bands::from_slice(&[b]),
        resolution: 1,
        ..Eq::default()
    };
    assert_eq!(latency(&e), 2048);
    let x: Vec<f32> = (0..20_000)
        .map(|i| ((i * 7919) % 1000) as f32 / 1000.0 - 0.5)
        .collect();
    let y = run(&e, &x);
    for i in 4_000..20_000 {
        assert!(
            (y[i] - x[i - 2048]).abs() < 1e-4,
            "{i}: {} vs {}",
            y[i],
            x[i - 2048]
        );
    }
}

#[test]
fn a_spectral_band_takes_down_what_stands_out_and_leaves_the_rest() {
    // Two sines in the band's reach, one loud: it comes down, the quiet one hardly.
    let mut b = EqBand::new(Shape::Bell, 2_000.0, 0.0);
    b.q = 0.5;
    b.dynamic = Some(Dynamic {
        range_db: -9.0,
        threshold_db: Some(-30.0),
        spectral: true,
        density: 1.0,
        tilt: false,
        ..Dynamic::default()
    });
    let e = Eq {
        bands: Bands::from_slice(&[b]),
        resolution: 2,
        ..Eq::default()
    };
    let n = 96_000;
    let x: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f64 / RATE;
            ((2.0 * PI * 1_500.0 * t).sin() * 0.4 + (2.0 * PI * 2_700.0 * t).sin() * 0.01) as f32
        })
        .collect();
    let y = run(&e, &x);
    let tail = |s: &[f32]| s[n / 2..].to_vec();
    let (yl, xl) = (tail(&y), tail(&x));
    let loud = level(&yl, 1_500.0) - level(&xl, 1_500.0);
    let quiet = level(&yl, 2_700.0) - level(&xl, 2_700.0);
    assert!(loud < -5.0, "{loud}");
    assert!(quiet > -1.5, "{quiet}");
}

#[test]
fn in_linear_phase_a_dynamic_band_moves_as_a_whole() {
    let mut b = EqBand::new(Shape::Bell, 1_000.0, 0.0);
    b.dynamic = Some(Dynamic {
        range_db: -6.0,
        threshold_db: Some(-40.0),
        ..Dynamic::default()
    });
    let e = Eq {
        bands: Bands::from_slice(&[b]),
        processing: Processing::Linear,
        resolution: 1,
        ..Eq::default()
    };
    assert!(needed(&e));
    let n = 72_000;
    let x: Vec<f32> = (0..n)
        .map(|i| (2.0 * PI * 1_000.0 * i as f64 / RATE).sin() as f32 * 0.25)
        .collect();
    let y = run(&e, &x);
    let moved = level(&y[n / 2..], 1_000.0) - level(&x[n / 2..], 1_000.0);
    assert!((moved + 6.0).abs() < 0.6, "{moved}");
}
