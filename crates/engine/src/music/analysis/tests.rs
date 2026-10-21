use super::*;

const RATE: f64 = 48_000.0;

fn sine(freq: f64, amp: f32, seconds: f64) -> Vec<f32> {
    (0..(seconds * RATE) as usize)
        .map(|i| amp * (std::f64::consts::TAU * freq * i as f64 / RATE).sin() as f32)
        .collect()
}

#[test]
fn a_full_scale_sine_in_one_channel_is_minus_3_lufs() {
    // BS.1770's own check: a 0 dBFS 997 Hz sine in one channel reads -3.01 LUFS.
    let tone = sine(997.0, 1.0, 5.0);
    let silence = vec![0.0; tone.len()];
    let a = analyze(&tone, &silence, RATE);
    assert!((a.lufs + 3.01).abs() < 0.05, "{}", a.lufs);
    assert!(a.peak_db[0].abs() < 0.01);
    assert!((a.rms_db[0] + 3.01).abs() < 0.01); // a sine's RMS is 3 dB under its peak
    assert_eq!(a.peak_db[1], FLOOR);
    assert_eq!(analyze(&silence, &silence, RATE).lufs, -70.0);
}

#[test]
fn bands_say_where_the_energy_is() {
    let bass = sine(80.0, 0.5, 1.0);
    let a = analyze(&bass, &bass, RATE);
    assert!(
        a.bands_db.low > -1.0 && a.bands_db.high < -40.0,
        "{:?}",
        a.bands_db
    );
    let hiss = sine(9_000.0, 0.5, 1.0);
    let a = analyze(&hiss, &hiss, RATE);
    assert!(
        a.bands_db.high > -1.0 && a.bands_db.low < -40.0,
        "{:?}",
        a.bands_db
    );
}

#[test]
fn the_spectrum_stereo_and_notes_say_what_is_playing() {
    // An A minor chord, A C E, with an 808-ish 50 Hz under it.
    let chord: Vec<f32> = (0..RATE as usize * 2)
        .map(|i| {
            let t = i as f64 / RATE;
            let n = |f: f64| (std::f64::consts::TAU * f * t).sin();
            (0.2 * (n(220.0) + n(261.63) + n(329.63)) + 0.3 * n(50.0)) as f32
        })
        .collect();
    let a = analyze(&chord, &chord, RATE);
    assert!(
        a.spectrum_db.sub > -6.0 && a.spectrum_db.air < -40.0,
        "{:?}",
        a.spectrum_db
    );
    assert!((a.stereo.correlation - 1.0).abs() < 1e-6 && a.stereo.width_db < -100.0);
    assert!(
        a.notes.key == "A minor" || a.notes.key == "C major",
        "{}",
        a.notes.key
    );
    let share = |name: &str| a.notes.classes.iter().find(|(n, _)| n == name).unwrap().1;
    assert!(share("A") > 15.0 && share("C") > 15.0 && share("E") > 15.0 && share("F#") < 5.0);
    // A sine is 3 dB of crest; opposite sides cancel.
    let tone = sine(440.0, 0.5, 1.0);
    let flipped: Vec<f32> = tone.iter().map(|x| -x).collect();
    let a = analyze(&tone, &flipped, RATE);
    assert!((a.crest_db - 3.01).abs() < 0.05, "{}", a.crest_db);
    assert!(a.stereo.correlation < -0.99);
}

#[test]
fn clipping_is_counted() {
    let loud: Vec<f32> = sine(100.0, 1.5, 0.1);
    let a = analyze(&loud, &loud, RATE);
    assert!(a.clipped > 0 && a.peak_db[0] > 3.0);
}
