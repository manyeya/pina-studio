use super::*;

/// An onset strength track of a beat: kicks on 1 and 3, snares on 2 and 4, hats on eighths.
fn beat(bpm: f32, bars: f32, fps: f32) -> (Vec<f32>, f32) {
    let seconds = bars * 4.0 * 60.0 / bpm;
    let frames = (seconds * fps) as usize;
    let mut x = vec![0.0; frames];
    let eighth = 30.0 / bpm;
    let mut i = 0;
    loop {
        let t = i as f32 * eighth;
        let f = (t * fps).round() as usize;
        if f >= frames {
            break;
        }
        x[f] += match i % 4 {
            0 => 3.0,
            2 => 2.5,
            _ => 0.8,
        };
        i += 1;
    }
    (x, seconds)
}

#[test]
fn a_beat_cut_to_bars_gives_its_tempo_exactly() {
    let fps = 44_100.0 / 512.0;
    for bpm in [63.0, 85.0, 95.0, 124.0, 140.0, 174.0] {
        let (x, seconds) = beat(bpm, 4.0, fps);
        let t = tempo(&x, fps, seconds, None).unwrap();
        let ratio = t.bpm / bpm;
        // Its tempo, or (at the ends) half or double.
        assert!(
            [0.5, 1.0, 2.0].iter().any(|k| (ratio - k).abs() < 0.005),
            "{bpm}: {t:?}"
        );
        if (80.0..=150.0).contains(&bpm) {
            assert!((t.bpm - bpm).abs() < 0.05, "{bpm}: {t:?}");
            assert_eq!(t.beats, Some(16));
        }
    }
}

#[test]
fn a_name_settles_the_octave() {
    let fps = 44_100.0 / 512.0;
    let (x, seconds) = beat(174.0, 4.0, fps);
    let t = tempo(&x, fps, seconds, Some(174.0)).unwrap();
    assert_eq!(t.bpm, 174.0);
}

#[test]
fn names_give_tempos() {
    assert_eq!(named_tempo("loops/keys_01(95bpm).wav"), Some((95.0, false)));
    assert_eq!(named_tempo("drums/loop 120 bpm.wav"), Some((120.0, false)));
    assert_eq!(
        named_tempo("808 loops/105bpm/e808_bd-01.wav"),
        Some((105.0, false))
    );
    assert_eq!(named_tempo("drumloops/twizt(95).wav"), Some((95.0, true)));
    assert_eq!(named_tempo("drum_hits/kick_01.wav"), None);
}

#[test]
fn hits_are_found_where_they_are() {
    let mut x = vec![0.0; 200];
    for &f in &[10, 60, 61, 120] {
        x[f] = 1.0;
    }
    x[62] = 0.5;
    assert_eq!(onsets(&x, 86.0), vec![10, 60, 120]);
}
