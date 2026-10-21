use super::*;

fn tone(hz: impl Fn(f32) -> f32, seconds: f32, rate: f32, harmonics: &[f32]) -> Vec<f32> {
    let mut phase = 0.0f32;
    (0..(seconds * rate) as usize)
        .map(|i| {
            let t = i as f32 / rate;
            phase += std::f32::consts::TAU * hz(t) / rate;
            harmonics
                .iter()
                .enumerate()
                .map(|(h, a)| a * ((h + 1) as f32 * phase).sin())
                .sum::<f32>()
                * (-t * 1.5).exp()
        })
        .collect()
}

fn pitch_of(x: &[f32], rate: f32) -> Summary {
    summarize(&track(x, rate, 0, x.len(), 40)).unwrap()
}

#[test]
fn finds_a_sub_at_25_hz() {
    // G0 plus 40 cents, as Sub_01 is.
    let hz = 24.4997 * 2f32.powf(40.0 / 1200.0);
    let x = tone(|_| hz, 1.5, 44_100.0, &[1.0, 0.1]);
    let p = pitch_of(&x, 44_100.0);
    let cents = (midi(p.hz) - 19.0) * 100.0;
    assert!((cents - 40.0).abs() < 5.0, "{} Hz, {cents} cents", p.hz);
    assert!(p.confidence > 0.6, "{p:?}");
}

#[test]
fn an_808_glide_settles_on_its_note() {
    // Sliding down an octave into A1 (55 Hz) over 80 ms, then holding it.
    let x = tone(
        |t| 55.0 * 2f32.powf((1.0 - t / 0.08).max(0.0)),
        1.2,
        48_000.0,
        &[1.0, 0.3, 0.15],
    );
    let p = pitch_of(&x, 48_000.0);
    assert!((midi(p.hz) - 33.0).abs() < 0.2, "{p:?}");
}

#[test]
fn a_weak_fundamental_keeps_its_octave() {
    // A bass note (C2, 65.4 Hz) whose second and third harmonics are louder than it.
    let x = tone(|_| 65.406, 1.0, 44_100.0, &[0.4, 1.0, 0.8, 0.3]);
    let p = pitch_of(&x, 44_100.0);
    assert!((midi(p.hz) - 36.0).abs() < 0.2, "{p:?}");
}

#[test]
fn a_high_note_is_found_too() {
    let x = tone(|_| 1046.5, 0.5, 44_100.0, &[1.0, 0.5]);
    let p = pitch_of(&x, 44_100.0);
    assert!((midi(p.hz) - 84.0).abs() < 0.2, "{p:?}");
}

#[test]
fn noise_has_no_pitch() {
    let mut seed = 1u32;
    let x: Vec<f32> = (0..44_100)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
        })
        .collect();
    let frames = track(&x, 44_100.0, 0, x.len(), 20);
    assert!(summarize(&frames).is_none_or(|p| p.confidence < 0.3));
}
