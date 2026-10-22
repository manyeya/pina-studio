//! Band shapes and slopes: their analog curves, what plays matching what's drawn, no clicks.

use super::*;

#[test]
fn bells_shelves_and_cuts_have_their_analog_shapes() {
    let bell = band(Shape::Bell, 1_000.0, 6.0);
    assert!(
        close(at(&bell, 1_000.0), 6.0, 0.01),
        "{}",
        at(&bell, 1_000.0)
    );
    assert!(close(at(&bell, 30.0), 0.0, 0.05) && close(at(&bell, 18_000.0), 0.0, 0.1));
    // Near the top a bell keeps its shape: at 15 kHz, its gain where it should be and an
    // octave below the same as an analog bell's (a bilinear one's narrows).
    let top = band(Shape::Bell, 15_000.0, 9.0);
    assert!(close(at(&top, 15_000.0), 9.0, 0.01));
    let w = 2.0 * PI * 15_000.0;
    let a = 10f64.powf(9.0 / 40.0);
    let x = 2.0 * PI * 7_500.0;
    let d = (w * w - x * x).powi(2);
    let analog = 10.0 * ((d + (a * w * x).powi(2)) / (d + (w * x / a).powi(2))).log10();
    assert!(
        close(at(&top, 7_500.0), analog, 0.7),
        "{} {analog}",
        at(&top, 7_500.0)
    );
    // Cuts: -3 dB at the corner, then their slope an octave on.
    for (slope, octave) in [
        (6.0, -7.0),
        (12.0, -12.3),
        (24.0, -24.1),
        (48.0, -48.2),
        (96.0, -96.3),
    ] {
        let mut cut = band(Shape::HighCut, 1_000.0, 0.0);
        cut.slope = slope;
        assert!(
            close(at(&cut, 1_000.0), -3.01, 0.05),
            "{slope}: {}",
            at(&cut, 1_000.0)
        );
        assert!(
            close(at(&cut, 2_000.0), octave, 0.6),
            "{slope}: {}",
            at(&cut, 2_000.0)
        );
        assert!(close(at(&cut, 50.0), 0.0, 0.02));
        cut.shape = Shape::LowCut;
        assert!(
            close(at(&cut, 1_000.0), -3.01, 0.05),
            "{slope}: {}",
            at(&cut, 1_000.0)
        );
        assert!(
            close(at(&cut, 500.0), octave, 0.6),
            "{slope}: {}",
            at(&cut, 500.0)
        );
    }
    // A high cut near Nyquist is still -3 dB at its corner (a bilinear one's is pulled down).
    let mut top_cut = band(Shape::HighCut, 18_000.0, 0.0);
    top_cut.slope = 24.0;
    assert!(
        close(at(&top_cut, 18_000.0), -3.01, 0.2),
        "{}",
        at(&top_cut, 18_000.0)
    );
    // A resonant cut peaks.
    let mut resonant = band(Shape::LowCut, 100.0, 0.0);
    resonant.q = 4.0;
    assert!(at(&resonant, 100.0) > 10.0);
    // Shelves: their gain at the far end, half of it at their frequency.
    for slope in [6.0, 12.0, 18.0, 24.0, 36.0, 48.0] {
        let mut low = band(Shape::LowShelf, 500.0, 6.0);
        low.slope = slope;
        assert!(
            close(at(&low, 15.0), 6.0, 0.25),
            "{slope}: {}",
            at(&low, 15.0)
        );
        assert!(
            close(at(&low, 500.0), 3.0, 0.1),
            "{slope}: {}",
            at(&low, 500.0)
        );
        assert!(close(at(&low, 15_000.0), 0.0, 0.1));
        let mut high = band(Shape::HighShelf, 4_000.0, -6.0);
        high.slope = slope;
        assert!(
            close(at(&high, 4_000.0), -3.0, 0.1),
            "{slope}: {}",
            at(&high, 4_000.0)
        );
        assert!(
            close(at(&high, 23_000.0), -6.0, 0.4),
            "{slope}: {}",
            at(&high, 23_000.0)
        );
        assert!(close(at(&high, 50.0), 0.0, 0.05));
    }
    let tilt = band(Shape::TiltShelf, 1_000.0, 6.0);
    assert!(close(at(&tilt, 20.0), -3.0, 0.1) && close(at(&tilt, 20_000.0), 3.0, 0.15));
    assert!(close(at(&tilt, 1_000.0), 0.0, 0.05));
    let notch = band(Shape::Notch, 1_000.0, 0.0);
    assert!(at(&notch, 1_000.0) < -60.0 && close(at(&notch, 4_000.0), 0.0, 0.2));
    let pass = band(Shape::BandPass, 1_000.0, 0.0);
    assert!(close(at(&pass, 1_000.0), 0.0, 0.01) && at(&pass, 100.0) < -15.0);
    // Flat bands cost nothing.
    assert!(band_sections(&band(Shape::Bell, 1_000.0, 0.0), 1.0, RATE).is_empty());
}

#[test]
fn what_plays_is_what_is_drawn() {
    let mut cut = band(Shape::LowCut, 200.0, 0.0);
    cut.slope = 48.0;
    let e = eq(&[
        band(Shape::Bell, 1_000.0, 6.0),
        cut,
        band(Shape::HighShelf, 8_000.0, -4.0),
    ]);
    for freq in [80.0, 200.0, 1_000.0, 3_000.0, 12_345.0] {
        let (db, _) = through(&e, freq, false);
        let drawn = eq_response(&e, freq, RATE);
        assert!(close(db[0], drawn, 0.15), "{freq}: {} vs {drawn}", db[0]);
        assert!(close(db[1], drawn, 0.15));
    }
    // The output level and the gain scale.
    let mut louder = e;
    louder.output_db = -6.0;
    louder.scale = 0.5;
    let (db, _) = through(&louder, 1_000.0, false);
    assert!(close(db[0], 3.0 - 6.0, 0.2), "{}", db[0]);
}

#[test]
fn bands_come_and_go_without_a_click() {
    let mut p = Processor::new(RATE);
    let mut e = eq(&[band(Shape::Bell, 1_000.0, 12.0)]);
    let mut phase = 0usize;
    let mut biggest_step = 0f32;
    let mut prev = 0f32;
    for block in 0..200 {
        // Halfway: the band's taken out, then another put in.
        if block == 80 {
            e.bands.remove(0);
        }
        if block == 140 {
            e.bands.push(band(Shape::HighCut, 400.0, 0.0));
        }
        let mut l: Vec<f32> = (0..MAX_BLOCK)
            .map(|i| (2.0 * PI * 1_000.0 * (phase + i) as f64 / RATE).sin() as f32 * 0.1)
            .collect();
        let mut r = l.clone();
        phase += MAX_BLOCK;
        p.process(&e, None, None, &mut l, &mut r, None, None);
        if block > 20 {
            for &x in &l {
                biggest_step = biggest_step.max((x - prev).abs());
                prev = x;
            }
        } else {
            prev = l[MAX_BLOCK - 1];
        }
    }
    // A 1 kHz sine at 0.4 (12 dB up from 0.1) moves at most 2π·1000/48000·0.4 ≈ 0.052 a
    // sample: a click would jump far more.
    assert!(biggest_step < 0.06, "{biggest_step}");
}

#[test]
fn the_eq_tempo_had_before_opens_as_bands() {
    let old = r#"{"low_cut": 80, "low_mid": {"freq": 300, "gain_db": -2, "q": 1.2},
        "high_mid": {"freq": 3000, "gain_db": 2, "q": 1},
        "high_shelf": {"freq": 9000, "gain_db": 1.5, "q": 0.7071}, "bypass": true}"#;
    let eq: Eq = serde_json::from_str(old).unwrap();
    let got: Vec<_> = eq
        .bands
        .iter()
        .map(|b| (b.shape, b.freq, b.gain_db, b.off))
        .collect();
    assert_eq!(
        got,
        [
            (Shape::LowCut, 80.0, 0.0, false),
            (Shape::Bell, 300.0, -2.0, false),
            (Shape::Bell, 3_000.0, 2.0, false),
            (Shape::HighShelf, 9_000.0, 1.5, false),
        ]
    );
    assert!(eq.bypass && eq.bands[1].q == 1.2);
    // Saved as bands, and read back the same.
    let json = serde_json::to_string(&eq).unwrap();
    assert!(
        !json.contains("low_mid") && json.contains("\"bands\""),
        "{json}"
    );
    assert_eq!(serde_json::from_str::<Eq>(&json).unwrap(), eq);
    // A low cut that was off is kept, off; an empty EQ has no bands.
    let off: Eq = serde_json::from_str(r#"{"low_cut": 0, "high_shelf": {"freq": 10000}}"#).unwrap();
    assert!(off.bands[0].off && off.bands.len() == 4);
    assert!(serde_json::from_str::<Eq>("{}").unwrap().bands.is_empty());
    // And it sounds as it did: the old curve was RBJ's bells and shelf, the same as these
    // but for the very top.
    for freq in [100.0, 300.0, 1_000.0, 3_000.0] {
        let db = eq_response(&eq, freq, RATE);
        let rbj = band_response(&eq.bands[1], 1.0, freq, RATE)
            + band_response(&eq.bands[2], 1.0, freq, RATE)
            + band_response(&eq.bands[3], 1.0, freq, RATE)
            + band_response(&eq.bands[0], 1.0, freq, RATE);
        assert!(close(db, rbj, 1e-9));
    }
}

#[test]
fn tilts_all_passes_brickwalls_and_gain_q() {
    // A flat tilt: a straight line through its frequency, its gain from end to end.
    let tilt = band(Shape::FlatTilt, 1_000.0, 6.0);
    assert!(
        close(at(&tilt, 1_000.0), 0.0, 0.05),
        "{}",
        at(&tilt, 1_000.0)
    );
    let per_octave = 6.0 / 10.0;
    for f in [100.0, 250.0, 4_000.0, 10_000.0] {
        let want = per_octave * (f / 1_000.0f64).log2();
        assert!(
            close(at(&tilt, f), want, 0.35),
            "{f}: {} vs {want}",
            at(&tilt, f)
        );
    }
    // An all pass leaves the level alone.
    let all = band(Shape::AllPass, 800.0, 0.0);
    for f in [50.0, 800.0, 12_000.0] {
        assert!(close(at(&all, f), 0.0, 1e-6));
    }
    // A brickwall: nothing an octave on.
    let mut wall = band(Shape::HighCut, 1_000.0, 0.0);
    wall.slope = Shape::BRICKWALL;
    assert!(at(&wall, 2_000.0) < -120.0 && close(at(&wall, 500.0), 0.0, 0.01));
    // Gain-Q: a big boost, narrower.
    let mut wide = band(Shape::Bell, 1_000.0, 12.0);
    let plain = at(&wide, 2_000.0);
    wide.gain_q = true;
    assert!(at(&wide, 2_000.0) < plain - 1.0);
    assert!(close(at(&wide, 1_000.0), 12.0, 0.01));
}

#[test]
fn steep_cuts_stay_steady() {
    let mut low = band(Shape::LowCut, 20.0, 0.0);
    low.slope = 96.0;
    let mut high = band(Shape::HighCut, 20_000.0, 0.0);
    high.slope = 96.0;
    high.q = 10.0;
    let e = eq(&[low, high]);
    let mut p = Processor::new(RATE);
    let mut seed = 1u32;
    for _ in 0..400 {
        let mut l: Vec<f32> = (0..MAX_BLOCK)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 8) as f32 / (1 << 24) as f32 - 0.5
            })
            .collect();
        let mut r = l.clone();
        p.process(&e, None, None, &mut l, &mut r, None, None);
        assert!(l.iter().chain(&r).all(|x| x.is_finite() && x.abs() < 50.0));
    }
}

#[test]
fn every_shape_takes_a_slope_fractions_too() {
    // As steep as it says, past a cut's corner; a fraction between its neighbours.
    let fall_at = |slope: f32, f: f64| {
        let mut cut = band(Shape::LowCut, 4_000.0, 0.0);
        cut.slope = slope;
        at(&cut, f * 2.0) - at(&cut, f)
    };
    let fall = |slope: f32| fall_at(slope, 500.0);
    // Steep, nearer the corner: further down is below what the filters can tell apart.
    let near = |slope: f32| fall_at(slope, 1_000.0);
    for slope in [3.0f32, 9.0, 15.5, 21.0, 33.0, 40.0] {
        let (lo, hi) = ((slope / 6.0).floor() * 6.0, (slope / 6.0).ceil() * 6.0);
        assert!(near(lo) < near(slope) && near(slope) < near(hi), "{slope}");
        if slope < 18.0 {
            assert!(
                close(fall(slope), slope as f64, 1.0),
                "{slope}: {}",
                fall(slope)
            );
        }
    }
    assert!(
        close(fall(6.0), 6.0, 0.3) && close(fall(3.0), 3.0, 0.5),
        "{}",
        fall(3.0)
    );
    // A bell steeper: its gain and its half-gain width the same, its sides lower.
    let width = |b: &EqBand| {
        let edge = 1_000.0 * ((1.0f64 + 0.25).sqrt() - 0.5);
        at(b, edge)
    };
    let mut wide = band(Shape::Bell, 1_000.0, 12.0);
    let mut steep = wide;
    steep.slope = 48.0;
    let mut between = wide;
    between.slope = 30.0;
    for b in [&wide, &steep, &between] {
        assert!(close(at(b, 1_000.0), 12.0, 0.15), "{}", at(b, 1_000.0));
        assert!(close(width(b), 6.0, 0.4), "{}: {}", b.slope, width(b));
    }
    let skirt = |b: &EqBand| at(b, 125.0);
    assert!(skirt(&steep) < skirt(&between) && skirt(&between) < skirt(&wide));
    assert!(skirt(&steep) < 0.2, "{}", skirt(&steep));
    wide.gain_db = -12.0;
    assert!(close(at(&wide, 1_000.0), -12.0, 0.15));
    // Shelves get there, half way at their frequency, steeper or in between.
    for slope in [6.0, 15.0, 30.0, 42.0, 96.0] {
        let mut shelf = band(Shape::HighShelf, 2_000.0, 9.0);
        shelf.slope = slope;
        assert!(
            close(at(&shelf, 2_000.0), 4.5, 0.6),
            "{slope}: {}",
            at(&shelf, 2_000.0)
        );
        assert!(close(at(&shelf, 18_000.0), 9.0, 0.6), "{slope}");
        assert!(at(&shelf, 100.0).abs() < 0.3, "{slope}");
    }
    // A notch wider and deeper, a band pass steeper (and gentler: 6 dB, 3 an octave a side).
    let mut notch = band(Shape::Notch, 1_000.0, 0.0);
    notch.q = 2.0;
    let mut deep = notch;
    deep.slope = 36.0;
    assert!(at(&deep, 1_100.0) < at(&notch, 1_100.0) - 6.0);
    assert!(at(&deep, 4_000.0).abs() < 0.5);
    for (slope, side) in [(6.0, 3.0), (12.0, 6.0), (24.0, 12.0), (30.0, 15.0)] {
        let mut bp = band(Shape::BandPass, 1_000.0, 0.0);
        bp.slope = slope;
        assert!(at(&bp, 1_000.0).abs() < 0.3, "{slope}");
        let fall = at(&bp, 120.0) - at(&bp, 60.0);
        assert!(close(fall, side, 1.0), "{slope}: {fall}");
    }
    // What a steep bell, a fractional shelf and cut play is what's drawn.
    let (mut b, mut sh, mut c) = (
        band(Shape::Bell, 1_000.0, 9.0),
        band(Shape::LowShelf, 300.0, -6.0),
        band(Shape::HighCut, 9_000.0, 0.0),
    );
    (b.slope, sh.slope, c.slope) = (48.0, 21.5, 27.0);
    let e = eq(&[b, sh, c]);
    for freq in [150.0, 600.0, 1_000.0, 1_700.0, 10_700.0] {
        let (db, _) = through(&e, freq, false);
        let drawn = eq_response(&e, freq, RATE);
        assert!(close(db[0], drawn, 0.2), "{freq}: {} vs {drawn}", db[0]);
    }
    // No shape, slope or place takes more sections than there's room for.
    let mut out = [Section::default(); MAX_SECTIONS];
    for shape in Shape::ALL {
        for slope in (0..=200).map(|i| i as f32 * 0.5).chain([Shape::BRICKWALL]) {
            for freq in [10.0, 40.0, 1_000.0, 12_000.0, 23_900.0] {
                for q in [0.1, 1.0, 12.0] {
                    let mut b = band(shape, freq, -8.0);
                    (b.slope, b.q) = (shape.fit_slope(slope), q);
                    let n = sections(&b, -8.0, RATE, &mut out);
                    let db = response_db(&out[..n], freq as f64 * 0.9, RATE);
                    assert!(!db.is_nan(), "{shape:?} {slope}");
                }
            }
        }
    }
}

#[test]
fn a_slope_moved_while_playing_keeps_its_sound() {
    // A cut's slope dragged through fractions as a sine plays: no jump bigger than the
    // sine's own step.
    let mut cut = band(Shape::LowCut, 200.0, 0.0);
    let mut p = Processor::new(RATE);
    let mut last = 0f32;
    let mut most = 0f32;
    for k in 0..200 {
        cut.slope = 6.0 + k as f32 * 0.2;
        let e = eq(&[cut]);
        let mut l: Vec<f32> = (0..64)
            .map(|i| (2.0 * PI * 800.0 * (k * 64 + i) as f64 / RATE).sin() as f32 * 0.5)
            .collect();
        let mut r = l.clone();
        p.process(&e, None, None, &mut l, &mut r, None, None);
        for &x in &l {
            if k > 2 {
                most = most.max((x - last).abs());
            }
            last = x;
        }
    }
    // The sine's own biggest step: 0.5 * 2π * 800 / 48000.
    assert!(most < 0.06, "{most}");
}
