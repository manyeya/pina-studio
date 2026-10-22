//! Dynamic bands.

use super::*;

#[test]
fn a_dynamic_band_moves_with_how_loud_it_is() {
    let mut b = band(Shape::Bell, 1_000.0, 0.0);
    b.dynamic = Some(Dynamic {
        range_db: -6.0,
        threshold_db: Some(-40.0),
        attack_ms: Some(10.0),
        release_ms: Some(120.0),
        ..Dynamic::default()
    });
    // Loud (-12 dBFS, 28 over): down its whole range.
    let (db, _) = through(&eq(&[b]), 1_000.0, false);
    assert!(close(db[0], -6.0, 0.3), "{db:?}");
    // Under the threshold: untouched.
    b.dynamic.as_mut().unwrap().threshold_db = Some(-6.0);
    let (db, _) = through(&eq(&[b]), 1_000.0, false);
    assert!(close(db[0], 0.0, 0.05), "{db:?}");
    // It listens round its frequency: three octaves away, the same sine is 18 dB quieter to
    // it, under a threshold the sine at its frequency is well over.
    b.dynamic.as_mut().unwrap().threshold_db = Some(-24.0);
    let (db, _) = through(&eq(&[b]), 1_000.0, false);
    assert!(close(db[0], -6.0, 0.3), "{db:?}");
    let (db, _) = through(&eq(&[b]), 8_123.0, false);
    assert!(close(db[0], 0.0, 0.05), "{db:?}");
}

#[test]
fn a_dynamic_band_sets_its_own_threshold_and_hears_what_it_is_told() {
    // Its own threshold: a steady sound settles, a jump in it moves it.
    let mut b = band(Shape::Bell, 1_000.0, 0.0);
    b.dynamic = Some(Dynamic {
        range_db: -9.0,
        ..Dynamic::default()
    });
    let mut x = tone(1_000.0, 0.05, 96_000);
    for (i, v) in x.iter_mut().enumerate().skip(72_000) {
        *v = (2.0 * PI * 1_000.0 * i as f64 / RATE).sin() as f32 * 0.4;
    }
    let e = eq(&[b]);
    let (l, _, _) = run(&e, &x, &x, None);
    let steady = peak_db(&l[60_000..72_000]) - 20.0 * 0.05f64.log10();
    let jumped = peak_db(&l[74_000..80_000]) - 20.0 * 0.4f64.log10();
    assert!(steady > -1.0, "{steady}");
    assert!(jumped < -4.0, "{jumped}");
    // Listening between two other frequencies, it doesn't hear the band's.
    let mut b = band(Shape::Bell, 1_000.0, 0.0);
    b.dynamic = Some(Dynamic {
        range_db: -6.0,
        threshold_db: Some(-30.0),
        trigger: Some([5_000.0, 8_000.0]),
        ..Dynamic::default()
    });
    let loud = tone(1_000.0, 0.4, 24_000);
    let (l, _, _) = run(&eq(&[b]), &loud, &loud, None);
    assert!(close(peak_db(&l[12_000..]), 20.0 * 0.4f64.log10(), 0.1));
    // Listening to the sidechain: a loud one moves it on a quiet sound.
    let mut b = band(Shape::Bell, 1_000.0, 0.0);
    b.dynamic = Some(Dynamic {
        range_db: -6.0,
        threshold_db: Some(-40.0),
        external: true,
        ..Dynamic::default()
    });
    let quiet = tone(1_000.0, 0.005, 24_000);
    let (l, _, _) = run(&eq(&[b]), &quiet, &quiet, Some([&loud, &loud]));
    assert!(close(
        peak_db(&l[12_000..]),
        20.0 * 0.005f64.log10() - 6.0,
        0.4
    ));
}
