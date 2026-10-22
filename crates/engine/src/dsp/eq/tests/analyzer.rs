//! The analyzer, fitting bands to a curve, soloing and the instance list.

use super::*;

#[test]
fn the_analyzer_finds_a_sine_at_its_level() {
    let sine: Vec<f32> = (0..TAP_LEN)
        .map(|i| (2.0 * PI * 1_234.0 * i as f64 / RATE).sin() as f32 * 0.5)
        .collect();
    let freqs: Vec<f64> = (0..200)
        .map(|i| 20.0 * 1000f64.powf(i as f64 / 199.0))
        .collect();
    let db = spectrum_db(&sine, RATE, &freqs);
    let (loudest, at) = db
        .iter()
        .enumerate()
        .fold((f32::MIN, 0), |m, (i, &d)| if d > m.0 { (d, i) } else { m });
    // A sine between bins reads up to 1.4 dB low through a Hann window.
    assert!(close(loudest as f64, -6.02, 1.5), "{loudest}");
    assert!((freqs[at] / 1_234.0 - 1.0).abs() < 0.05, "{}", freqs[at]);
    assert!(db[10] < -60.0 && db[190] < -60.0);
}

#[test]
fn bands_are_fitted_to_a_difference_and_spectra_averaged() {
    let freqs: Vec<f64> = (0..300)
        .map(|i| 20.0 * 1000f64.powf(i as f64 / 299.0))
        .collect();
    // A curve two bells made: fitted, two bells near them, and what's left small.
    let mut a = band(Shape::Bell, 200.0, 5.0);
    a.q = 1.0;
    let mut b = band(Shape::Bell, 4_000.0, -4.0);
    b.q = 2.0;
    let curve: Vec<f32> = freqs
        .iter()
        .map(|&f| (at(&a, f) + at(&b, f)) as f32)
        .collect();
    let fitted = fit_bands(&curve, &freqs, 6, RATE);
    assert!(fitted.len() >= 2 && fitted.len() <= 6, "{fitted:?}");
    let near = |f: f32, g: f32| {
        fitted
            .iter()
            .any(|x| (x.freq / f - 1.0).abs() < 0.15 && (x.gain_db - g).abs() < 1.0)
    };
    assert!(near(200.0, 5.0) && near(4_000.0, -4.0), "{fitted:?}");
    let made: Vec<f32> = freqs
        .iter()
        .map(|&f| fitted.iter().map(|x| at(x, f)).sum::<f64>() as f32)
        .collect();
    let worst = curve
        .iter()
        .zip(&made)
        .fold(0f32, |m, (c, m2)| m.max((c - m2).abs()));
    assert!(worst < 1.0, "{worst}");
    // A sine's average spectrum: its level where it is.
    let x: Vec<f32> = (0..48_000)
        .map(|i| (2.0 * PI * 1_000.0 * i as f64 / RATE).sin() as f32 * 0.5)
        .collect();
    let avg = average_spectrum(&x, RATE, &freqs, 4096);
    let (i, loudest) = avg
        .iter()
        .enumerate()
        .fold((0, f32::MIN), |m, (i, &d)| if d > m.1 { (i, d) } else { m });
    assert!(
        (freqs[i] / 1_000.0 - 1.0).abs() < 0.05 && loudest > -9.0,
        "{loudest}"
    );
    // Smoothing keeps a flat curve flat.
    assert!(
        smooth_third(&vec![3.0; 300], &freqs)
            .iter()
            .all(|&v| (v - 3.0).abs() < 1e-5)
    );
}

#[test]
fn a_band_heard_alone_and_the_analyzer_tapped() {
    let mut e = eq(&[band(Shape::Bell, 4_000.0, 0.0)]);
    e.bands.push(band(Shape::Bell, 200.0, 3.0));
    let id = e.bands[0].id;
    let (low, _) = {
        // A 200 Hz sine, the 4 kHz band soloed: mostly gone.
        let mut p = Processor::new(RATE);
        let tap = Tap::new();
        let mut peak = 0f32;
        for k in 0..40 {
            let mut l: Vec<f32> = (0..MAX_BLOCK)
                .map(|i| (2.0 * PI * 200.0 * (k * MAX_BLOCK + i) as f64 / RATE).sin() as f32 * 0.5)
                .collect();
            let mut r = l.clone();
            p.process(&e, None, Some((id, 0.0)), &mut l, &mut r, None, Some(&tap));
            if k > 20 {
                peak = l.iter().fold(peak, |m, x| m.max(x.abs()));
            }
        }
        let (mut pre, mut post, mut right) = ([0f32; TAP_LEN], [0f32; TAP_LEN], [0f32; TAP_LEN]);
        let written = tap.read(&mut pre, &mut post, &mut right);
        assert_eq!(written, 40 * MAX_BLOCK);
        let loudest = |s: &[f32]| s.iter().fold(0f32, |m, x| m.max(x.abs()));
        assert!(close(loudest(&pre) as f64, 0.5, 0.01), "{}", loudest(&pre));
        assert!(loudest(&post) < 0.05);
        (peak, ())
    };
    assert!(low < 0.05, "{low}");
}

#[test]
fn every_lane_is_kept_for_the_instance_list_while_it_is_open() {
    let tap = Tap::new();
    let mut out = [0f32; LANE_LEN];
    tap.write_lane(3, &[0.5; 64], &[0.5; 64]);
    assert_eq!(tap.read_lane(3, &mut out), 0, "closed, nothing kept");
    tap.lanes_on.store(true, Ordering::Relaxed);
    tap.write_lane(3, &[0.5; 64], &[0.25; 64]);
    tap.write_lane(MASTER, &[1.0; 8], &[1.0; 8]);
    assert_eq!(tap.read_lane(3, &mut out), 64);
    assert_eq!(out[LANE_LEN - 1], 0.375);
    assert_eq!(out[LANE_LEN - 65], 0.0);
    assert_eq!(tap.read_lane(MASTER, &mut out), 8);
    assert_eq!(tap.read_lane(MASTER + 1, &mut out), 0);
}
