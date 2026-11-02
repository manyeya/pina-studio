//! A saturator set by listening: a sine bass, a beat and a lead, in different genres.

use super::*;

const RATE: f64 = 48_000.0;

/// `seconds` of a sine at `hz`, played a beat on and a beat off at 120 BPM.
fn sine(hz: f32, seconds: f32) -> [Vec<f32>; 2] {
    let n = (RATE as f32 * seconds) as usize;
    let x: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let on = ((t * 2.0) as usize).is_multiple_of(2);
            if on {
                0.5 * (std::f32::consts::TAU * hz * t).sin()
            } else {
                0.0
            }
        })
        .collect();
    [x.clone(), x]
}

/// Noise bursts every half second that die away: hits.
fn hits(seconds: f32) -> [Vec<f32>; 2] {
    let n = (RATE as f32 * seconds) as usize;
    let mut seed = 1u32;
    let x: Vec<f32> = (0..n)
        .map(|i| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
            let since = (i % (RATE as usize / 2)) as f32 / RATE as f32;
            0.8 * noise * (-since * 30.0).exp()
        })
        .collect();
    [x.clone(), x]
}

fn ear<'a>(part: &'a str, genre: &'a str) -> Ear<'a> {
    Ear {
        part,
        genre,
        lowest_hz: None,
        key: None,
    }
}

#[test]
fn a_sine_bass_keeps_its_fundamental_clean_and_drives_its_harmonics_as_the_genre_does() {
    let bass = sine(55.0, 4.0);
    let at = |genre| {
        let e = Ear {
            lowest_hz: Some(55.0),
            ..ear("bass", genre)
        };
        learn(&bass, RATE, &e).unwrap()
    };
    let (trap, why) = at("trap");
    assert_eq!(trap.bands.len(), 2);
    assert_eq!(trap.crossovers[0], 121.0);
    assert_eq!(trap.bands[0].style, SatStyle::CleanTape);
    assert_eq!(trap.bands[1].style, SatStyle::Heavy);
    // A sine has nothing over 250 Hz: it's driven harder for the harmonics, oversampled more
    // for the amp's curve.
    assert!(trap.bands[1].drive_db >= 14.0, "{trap:?}");
    assert_eq!(trap.quality, 4);
    assert!(why.iter().any(|w| w.contains("sine")), "{why:?}");
    let (house, _) = at("house");
    assert_eq!(house.bands[1].style, SatStyle::Transformer);
    let (soul, _) = at("neo_soul");
    assert_eq!(soul.bands[1].style, SatStyle::WarmTube);

    // As loud as it went in.
    let mut out = [bass[0].clone(), bass[1].clone()];
    let [l, r] = &mut out;
    Saturation::new(RATE).process(&trap, l, r);
    let moved = power_db(&out) - power_db(&bass);
    assert!(moved.abs() < 0.5, "{moved} dB louder");
}

#[test]
fn drums_split_in_three_their_body_by_genre() {
    let beat = hits(4.0);
    let body = |genre| learn(&beat, RATE, &ear("drums", genre)).unwrap().0.bands[1].style;
    assert_eq!(body("trap"), SatStyle::Crunch);
    assert_eq!(body("house"), SatStyle::Transformer);
    assert_eq!(body("lofi"), SatStyle::Lofi);
    assert_eq!(body("pop"), SatStyle::CleanTape);
    let (s, _) = learn(&beat, RATE, &ear("drums", "trap")).unwrap();
    assert_eq!(s.bands.len(), 3);
    assert_eq!(&s.crossovers[..2], &[120.0, 4500.0]);
    // The lows expand a little, for punch.
    assert!(s.bands[0].dynamics < 0.0);
}

#[test]
fn an_electronic_lead_rings_on_the_keys_tonic() {
    let lead = sine(660.0, 4.0);
    let e = Ear {
        key: Key::parse("A minor"),
        ..ear("lead", "techno")
    };
    let (s, why) = learn(&lead, RATE, &e).unwrap();
    let top = s.bands[1];
    assert!(top.feedback > 0.0);
    // A, moved into 1.2-2.4 kHz: 1760 Hz.
    assert!(
        (top.feedback_hz - 1760.0).abs() < 1.0,
        "{}",
        top.feedback_hz
    );
    assert!(why.iter().any(|w| w.contains("tonic")), "{why:?}");
    // In pop, no ringing.
    let (pop, _) = learn(&lead, RATE, &ear("lead", "pop")).unwrap();
    assert_eq!(pop.bands[1].feedback, 0.0);
}

#[test]
fn silence_sets_nothing() {
    let quiet = [vec![0.0; 48_000], vec![0.0; 48_000]];
    assert!(learn(&quiet, RATE, &ear("bass", "trap")).is_none());
}

#[test]
fn genres_lean_to_their_colours() {
    assert_eq!(Flavour::of_genre("lofi"), Flavour::Lofi);
    assert_eq!(Flavour::of_genre("drill"), Flavour::Hard);
    assert_eq!(Flavour::of_genre("amapiano"), Flavour::Club);
    assert_eq!(Flavour::of_genre("rnb"), Flavour::Warm);
    assert_eq!(Flavour::of_genre("ambient"), Flavour::Clean);
}
