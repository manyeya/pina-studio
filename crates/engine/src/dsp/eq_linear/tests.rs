use super::*;
use crate::song::project::{Bands, EqBand};
use std::f64::consts::PI;

const RATE: f64 = 48_000.0;

fn eq(bands: &[EqBand], resolution: u8) -> Eq {
    Eq {
        bands: Bands::from_slice(bands),
        resolution,
        ..Eq::default()
    }
}

/// An impulse through linear phase: what comes out.
fn impulse(e: &Eq, n: usize) -> Vec<f32> {
    let lin = Arc::new(Linear::new(e, RATE));
    let mut run = Runner::default();
    let mut l = vec![0f32; n];
    l[0] = 1.0;
    let mut r = l.clone();
    for at in (0..n).step_by(512) {
        let end = (at + 512).min(n);
        run.process(&lin, e, &mut l[at..end], &mut r[at..end]);
    }
    l
}

#[test]
fn linear_phase_is_the_curve_late_and_symmetric() {
    let bell = EqBand::new(Shape::Bell, 1_000.0, 6.0);
    let e = eq(&[bell], 1);
    let lat = latency(&e) as usize;
    assert_eq!(lat, 8192 / 2 + PART);
    let out = impulse(&e, lat * 2 + 2048);
    // Its peak where the latency says, and the same either side: linear phase.
    let peak = (0..out.len())
        .max_by(|&a, &b| out[a].abs().total_cmp(&out[b].abs()))
        .unwrap();
    assert_eq!(peak, lat);
    for d in [1, 10, 100, 1_000] {
        assert!((out[lat - d] - out[lat + d]).abs() < 1e-4, "{d}");
    }
    // And its magnitude the bands': the impulse's spectrum at 1 kHz up 6 dB, at 100 Hz flat.
    let at = |f: f64| {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &x) in out.iter().enumerate() {
            let w = 2.0 * PI * f * i as f64 / RATE;
            re += x as f64 * w.cos();
            im -= x as f64 * w.sin();
        }
        20.0 * (re * re + im * im).sqrt().log10()
    };
    assert!((at(1_000.0) - 6.0).abs() < 0.1, "{}", at(1_000.0));
    assert!(at(100.0).abs() < 0.1, "{}", at(100.0));
    assert!(at(10_000.0).abs() < 0.1, "{}", at(10_000.0));
}

#[test]
fn mid_or_side_bands_take_a_second_stage() {
    let mut side = EqBand::new(Shape::Bell, 1_000.0, -12.0);
    side.placement = Placement::Side;
    let e = eq(&[side], 0);
    assert_eq!(latency(&e) as usize, 2 * (4096 / 2 + PART));
    // A mono sine comes through untouched (late); one all side, 12 dB down.
    let n = 24_000;
    for (sign, want) in [(1.0f32, 0.0), (-1.0, -12.0)] {
        let lin = Arc::new(Linear::new(&e, RATE));
        let mut run = Runner::default();
        let mut l: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * 1_000.0 * i as f64 / RATE).sin() as f32 * 0.5)
            .collect();
        let mut r: Vec<f32> = l.iter().map(|x| x * sign).collect();
        for at in (0..n).step_by(512) {
            let end = (at + 512).min(n);
            run.process(&lin, &e, &mut l[at..end], &mut r[at..end]);
        }
        let peak = l[n / 2..].iter().fold(0f32, |m, x| m.max(x.abs()));
        let db = 20.0 * (peak as f64 / 0.5).log10();
        assert!((db - want).abs() < 0.3, "{sign}: {db}");
    }
}

#[test]
fn moving_it_remakes_the_kernels() {
    // Planned 6 dB up at 1 kHz; automation takes it back to flat as it plays: new kernels
    // come, made on their own thread, a few blocks on.
    let planned = eq(&[EqBand::new(Shape::Bell, 1_000.0, 6.0)], 0);
    let now = eq(&[EqBand::new(Shape::Bell, 1_000.0, 0.0)], 0);
    let lin = Arc::new(Linear::new(&planned, RATE));
    let mut run = Runner::default();
    let n = 512 * 80;
    let mut l: Vec<f32> = (0..n)
        .map(|i| (2.0 * PI * 1_000.0 * i as f64 / RATE).sin() as f32 * 0.5)
        .collect();
    let mut r = l.clone();
    for at in (0..n).step_by(512) {
        run.process(&lin, &now, &mut l[at..at + 512], &mut r[at..at + 512]);
        thread::sleep(Duration::from_millis(3));
    }
    let peak = l[n - 4096..].iter().fold(0f32, |m, x| m.max(x.abs()));
    let db = 20.0 * (peak as f64 / 0.5).log10();
    assert!(db.abs() < 0.2, "{db}");
}
