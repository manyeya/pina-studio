use super::*;

#[test]
fn a_sketch_makes_shelves_bells_and_cuts() {
    // Up on the left, a dip in the middle, a dive at the right.
    let points: Vec<(f64, f32)> = (0..200)
        .map(|i| {
            let f = 20.0 * 1000f64.powf(i as f64 / 199.0);
            let o = (f / 1_000.0).log2() as f32;
            let db = if f < 120.0 {
                4.0
            } else if (0.0..1.5).contains(&o) {
                -5.0
            } else if f > 15_000.0 {
                -20.0
            } else {
                0.0
            };
            (f, db)
        })
        .collect();
    let bands = sketch_bands(&points, 12.0);
    let shapes: Vec<Shape> = bands.iter().map(|b| b.shape).collect();
    assert_eq!(
        shapes,
        [Shape::LowShelf, Shape::Bell, Shape::HighCut],
        "{bands:?}"
    );
    assert_eq!(bands[0].gain_db, 4.0);
    let bell = bands[1];
    assert!(
        bell.gain_db == -5.0 && (1_000.0..2_900.0).contains(&bell.freq),
        "{bell:?}"
    );
}

#[test]
fn eq_match_fits_what_would_make_one_like_the_other() {
    let mut m = Match::new(MatchSource::None);
    let freqs = m.freqs().to_vec();
    // The reference is this track with 6 dB more round 3 kHz.
    let input: Vec<f32> = freqs.iter().map(|_| -40.0).collect();
    let reference: Vec<f32> = freqs
        .iter()
        .map(|&f| -40.0 + 6.0 * (-((f / 3_000.0).log2().powi(2)) / 0.5).exp() as f32)
        .collect();
    m.pretend(&input, &reference);
    assert!(m.ready());
    m.run(48_000.0);
    let b = m.proposed[0];
    assert_eq!(b.shape, Shape::Bell);
    assert!(
        (b.freq / 3_000.0 - 1.0).abs() < 0.1 && b.gain_db > 3.5,
        "{b:?}"
    );
    // Peaks: a spectrum with two, found.
    let spec: Vec<f32> = freqs
        .iter()
        .map(|&f| {
            let bump = |c: f64| 12.0 * (-((f / c).log2().powi(2)) / 0.01).exp() as f32;
            -50.0 + bump(200.0) + bump(4_000.0)
        })
        .collect();
    let found: Vec<f64> = peaks(&spec, -90.0).iter().map(|&i| freqs[i]).collect();
    assert_eq!(found.len(), 2, "{found:?}");
}
