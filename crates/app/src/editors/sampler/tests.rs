use super::*;

#[test]
fn knobs_turn_settings_and_back() {
    let mut s = Sampler {
        zones: vec![Zone {
            note: 36,
            ..Zone::default()
        }],
        ..Sampler::default()
    };
    for f in [
        Field::Attack,
        Field::Release,
        Field::Cutoff,
        Field::Resonance,
        Field::LfoRate,
        Field::Volume,
        Field::Gain(0),
        Field::Pan(0),
        Field::Tune(0),
        Field::Drive,
        Field::Speed,
        Field::Position,
        Field::GrainSize,
        Field::Density,
        Field::Spray,
        Field::Detune,
        Field::PitchDecay,
        Field::Sub,
        Field::NoiseColor,
        Field::OscRatio,
        Field::CrushKhz,
        Field::ModRelease,
        Field::Route(2),
        Field::LoopFade(0),
    ] {
        field_set(&mut s, f, 0.7);
        assert!(
            (field_pos(&s, f) - 0.7).abs() < 0.02,
            "{f:?} {}",
            field_pos(&s, f)
        );
    }
    field_set(&mut s, Field::Cutoff, 1.0);
    assert!((s.voice.cutoff_hz - 20_000.0).abs() < 1.0);
    field_set(&mut s, Field::Attack, 0.0);
    assert_eq!(s.voice.attack_ms, 0.0);
    // A range's low note moves it whole.
    let mut s = Sampler {
        zones: vec![Zone {
            note: 24,
            high: Some(48),
            root: Some(36),
            ..Zone::default()
        }],
        ..Sampler::default()
    };
    field_set(&mut s, Field::Note(0), 36.0 / 127.0);
    assert_eq!(
        (s.zones[0].note, s.zones[0].high, s.zones[0].root),
        (36, Some(60), Some(48))
    );
    // Velocities kept in order, and none set when it's all of them.
    field_set(&mut s, Field::VelLow(0), 64.0 / 127.0);
    assert_eq!(s.zones[0].velocities, Some((64, 127)));
    field_set(&mut s, Field::VelLow(0), 0.0);
    assert_eq!(s.zones[0].velocities, None);
}

#[test]
fn stepped_settings_land_on_their_steps() {
    let mut s = Sampler::default();
    field_set(&mut s, Field::Unison, 3.0 / 7.0);
    field_set(&mut s, Field::Bits, 10.0 / 14.0);
    assert_eq!((s.voice.unison, s.voice.bits), (4, 12));
    // In time with the song, the LFO's rate is a note value.
    s.voice.lfo_beats = 1.0;
    field_set(&mut s, Field::LfoRate, 0.5);
    assert_eq!(s.voice.lfo_beats, 1.0);
    assert_eq!(field_text(&s, Field::LfoRate, None), "1/4");
    field_set(&mut s, Field::LfoRate, 1.0);
    assert_eq!(field_text(&s, Field::LfoRate, None), "4 bars");
}
