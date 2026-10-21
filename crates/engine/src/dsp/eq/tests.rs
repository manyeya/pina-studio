use super::*;
use crate::song::project::{Bands, Dynamic};

const RATE: f64 = 48_000.0;

fn eq(bands: &[EqBand]) -> Eq {
    Eq {
        bands: Bands::from_slice(bands),
        ..Eq::default()
    }
}

fn band(shape: Shape, freq: f32, gain: f32) -> EqBand {
    EqBand::new(shape, freq, gain)
}

fn at(b: &EqBand, freq: f64) -> f64 {
    band_response(b, 1.0, freq, RATE)
}

fn close(a: f64, b: f64, by: f64) -> bool {
    (a - b).abs() <= by
}

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

/// A sine's level through an EQ, once it's settled, in dB.
fn through(e: &Eq, freq: f64, side: bool) -> ([f64; 2], [f32; 2]) {
    let mut p = Processor::new(RATE);
    let n = 48_000;
    let sine = |i: usize| (2.0 * PI * freq * i as f64 / RATE).sin() as f32 * 0.25;
    let (mut peak, mut last) = ([0f64; 2], [0f32; 2]);
    let mut i = 0;
    while i < n {
        let frames = MAX_BLOCK.min(n - i);
        let mut l: Vec<f32> = (i..i + frames).map(sine).collect();
        let mut r: Vec<f32> = l.iter().map(|&x| if side { -x } else { x }).collect();
        p.process(e, None, None, &mut l, &mut r, None, None);
        if i > n / 2 {
            for k in 0..frames {
                peak = [
                    peak[0].max(l[k].abs() as f64),
                    peak[1].max(r[k].abs() as f64),
                ];
            }
        }
        last = [l[frames - 1], r[frames - 1]];
        i += frames;
    }
    (
        [
            20.0 * (peak[0] / 0.25).log10(),
            20.0 * (peak[1] / 0.25).log10(),
        ],
        last,
    )
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

/// Runs a second of stereo sound through an EQ, a block at a time; what came out.
fn run(e: &Eq, l: &[f32], r: &[f32], sc: Option<[&[f32]; 2]>) -> (Vec<f32>, Vec<f32>, Processor) {
    let mut p = Processor::new(RATE);
    let (mut ol, mut or) = (l.to_vec(), r.to_vec());
    for at in (0..l.len()).step_by(MAX_BLOCK) {
        let end = (at + MAX_BLOCK).min(l.len());
        let side = sc.map(|[a, b]| [&a[at..end], &b[at..end]]);
        p.process(
            e,
            None,
            None,
            &mut ol[at..end],
            &mut or[at..end],
            side,
            None,
        );
    }
    (ol, or, p)
}

fn tone(freq: f64, level: f32, n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| (2.0 * PI * freq * i as f64 / RATE).sin() as f32 * level)
        .collect()
}

fn peak_db(s: &[f32]) -> f64 {
    20.0 * (s.iter().fold(0f32, |m, x| m.max(x.abs())) as f64)
        .max(1e-9)
        .log10()
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

/// How loud a harmonic of `f` is in `s`, against `f` (dB).
fn harmonic(s: &[f32], f: f64, k: f64) -> f64 {
    let dft = |f: f64| {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &x) in s.iter().enumerate() {
            let w = 2.0 * PI * f * i as f64 / RATE;
            re += x as f64 * w.cos();
            im += x as f64 * w.sin();
        }
        (re * re + im * im).sqrt()
    };
    20.0 * (dft(f * k) / dft(f)).max(1e-12).log10()
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

/// An impulse through an EQ as the engine runs it, what it needs made first: what came out.
fn impulse(e: &Eq, n: usize) -> Vec<f32> {
    let made = Made::new(e, RATE);
    let mut p = Processor::new(RATE);
    let mut l = vec![0f32; n];
    l[0] = 1.0;
    let mut r = l.clone();
    for at in (0..n).step_by(MAX_BLOCK) {
        let end = (at + MAX_BLOCK).min(n);
        p.process(
            e,
            Some(&made),
            None,
            &mut l[at..end],
            &mut r[at..end],
            None,
            None,
        );
    }
    l
}

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
