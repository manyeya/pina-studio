//! Stereo placement, the output stage and character.

use super::*;

#[test]
fn a_band_can_work_on_one_side_or_the_middle_or_the_edges() {
    let mut b = band(Shape::Bell, 1_000.0, -12.0);
    b.placement = Placement::Left;
    let (db, _) = through(&eq(&[b]), 1_000.0, false);
    assert!(
        close(db[0], -12.0, 0.2) && close(db[1], 0.0, 0.05),
        "{db:?}"
    );
    b.placement = Placement::Side;
    // A mono sound has no side: untouched; one all side is cut.
    let (mono, _) = through(&eq(&[b]), 1_000.0, false);
    assert!(
        close(mono[0], 0.0, 0.05) && close(mono[1], 0.0, 0.05),
        "{mono:?}"
    );
    let (side, _) = through(&eq(&[b]), 1_000.0, true);
    assert!(close(side[0], -12.0, 0.2), "{side:?}");
    b.placement = Placement::Mid;
    let (mid, _) = through(&eq(&[b]), 1_000.0, false);
    assert!(close(mid[0], -12.0, 0.2), "{mid:?}");
}

#[test]
fn the_output_pans_inverts_and_makes_up_its_own_gain() {
    let x = tone(1_000.0, 0.5, 24_000);
    // Panned hard left: the right goes; mid and side, all the way to the side: a mono
    // sound goes.
    let mut e = eq(&[]);
    e.pan = -1.0;
    let (l, r, _) = run(&e, &x, &x, None);
    assert!(peak_db(&l[12_000..]) > -6.1 && peak_db(&r[12_000..]) < -100.0);
    e.pan_ms = true;
    e.pan = 1.0;
    let (l, _, _) = run(&e, &x, &x, None);
    assert!(peak_db(&l[12_000..]) < -100.0);
    // Upside down.
    let mut e = eq(&[]);
    e.invert = true;
    let (l, _, _) = run(&e, &x, &x, None);
    assert!((l[5_000] + x[5_000]).abs() < 1e-6);
    // Auto gain: a broad boost turns the whole down by about as much.
    let mut boost = eq(&[band(Shape::LowShelf, 20_000.0, 6.0)]);
    assert!(
        close(auto_gain_db(&boost, RATE), -6.0, 0.3),
        "{}",
        auto_gain_db(&boost, RATE)
    );
    boost.auto_gain = true;
    let (l, _, _) = run(&boost, &x, &x, None);
    assert!(
        close(peak_db(&l[12_000..]), -6.02, 0.4),
        "{}",
        peak_db(&l[12_000..])
    );
}

#[test]
fn the_warm_and_subtle_characters_colour_what_the_bands_do() {
    let x = tone(500.0, 0.8, 24_000);
    let mut e = eq(&[band(Shape::Bell, 500.0, 9.0)]);
    let tail = |s: &[f32]| s[12_000..].to_vec();
    let (clean, _, _) = run(&e, &x, &x, None);
    e.character = crate::song::project::Character::Subtle;
    let (subtle, _, _) = run(&e, &x, &x, None);
    e.character = crate::song::project::Character::Warm;
    let (warm, _, _) = run(&e, &x, &x, None);
    let (c3, s3) = (
        harmonic(&tail(&clean), 500.0, 3.0),
        harmonic(&tail(&subtle), 500.0, 3.0),
    );
    let w2 = harmonic(&tail(&warm), 500.0, 2.0);
    assert!(c3 < -90.0, "{c3}");
    // Subtle: odd harmonics; warm: even ones too.
    assert!(s3 > -60.0, "{s3}");
    assert!(w2 > -60.0, "{w2}");
    // Quiet, it hardly colours.
    let quiet = tone(500.0, 0.005, 24_000);
    let (soft, _, _) = run(&e, &quiet, &quiet, None);
    assert!(harmonic(&tail(&soft), 500.0, 2.0) < -60.0);
}
