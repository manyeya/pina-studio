//! Latency and natural phase.

use super::*;

#[test]
fn the_sound_comes_as_late_as_the_latency_says() {
    let still = |spectral| {
        let mut b = band(Shape::Bell, 1_000.0, 0.0);
        b.dynamic = Some(Dynamic {
            range_db: -6.0,
            threshold_db: Some(20.0),
            spectral,
            ..Dynamic::default()
        });
        b
    };
    let mut cases = Vec::new();
    for processing in Processing::ALL {
        cases.push(Eq {
            processing,
            ..eq(&[])
        });
        cases.push(Eq {
            processing,
            resolution: 0,
            ..eq(&[still(true)])
        });
    }
    // In linear phase any dynamic band takes the spectral stage.
    cases.push(Eq {
        processing: Processing::Linear,
        resolution: 0,
        ..eq(&[still(false)])
    });
    for &e in &cases {
        let lat = latency(&e) as usize;
        let out = impulse(&e, lat + 4096);
        let peak = (0..out.len())
            .max_by(|&a, &b| out[a].abs().total_cmp(&out[b].abs()))
            .unwrap();
        let how = (e.processing, eq_spectral::needed(&e));
        assert_eq!(peak, lat, "{how:?}");
        // Natural phase's filters stop short of Nyquist: an impulse comes out a little round.
        assert!((out[peak] - 1.0).abs() < 0.12, "{how:?}: {}", out[peak]);
    }
    assert_eq!(latency(&cases[2]), NATURAL_LATENCY);
    let off = Eq {
        bypass: true,
        ..cases[5]
    };
    assert_eq!(latency(&off), 0);
}

#[test]
fn natural_phase_has_the_analog_phase_up_high() {
    use rustfft::num_complex::Complex;
    // A bell high up: its phase near the top as an analog EQ's. Zero latency's is bent by
    // the sample rate; natural phase's much less. Their magnitudes alike.
    let (a, q, f0) = (10f64.powf(9.0 / 40.0), 1.0, 12_000.0);
    let mut b = band(Shape::Bell, f0 as f32, 9.0);
    b.q = q as f32;
    let analog = |f: f64| {
        let s = Complex::new(0.0, f / f0);
        ((s * s + s * (a / q) + 1.0) / (s * s + s / (a * q) + 1.0)).arg()
    };
    let response = |e: &Eq, f: f64| {
        let lat = latency(e) as f64;
        let out = impulse(e, 8192);
        let mut h = Complex::new(0.0, 0.0);
        for (i, &x) in out.iter().enumerate() {
            h += Complex::from_polar(x as f64, -2.0 * PI * f * (i as f64 - lat) / RATE);
        }
        h
    };
    let zero = eq(&[b]);
    let natural = Eq {
        processing: Processing::Natural,
        ..zero
    };
    for f in [15_000.0, 18_000.0] {
        let want = analog(f);
        let off = |e: &Eq| (response(e, f).arg() - want).abs();
        let (z, n) = (off(&zero), off(&natural));
        assert!(n < z / 2.0, "{f}: zero latency {z}, natural {n}");
    }
    for f in [100.0, 1_000.0, 10_000.0, 15_000.0] {
        let db = 20.0 * response(&natural, f).norm().log10();
        let drawn = eq_response(&natural, f, RATE);
        assert!(close(db, drawn, 0.3), "{f}: {db} vs {drawn}");
    }
}
