use super::*;

#[test]
fn reads_notes_on_any_channel() {
    let mut d = Decoder::default();
    let on = |pitch, velocity, release| {
        Some(Live::Note(LiveNote {
            pitch,
            velocity,
            release,
        }))
    };
    assert_eq!(d.decode(&[0x90, 60, 100]), on(60, 100, 0));
    assert_eq!(d.decode(&[0x95, 61, 0]), on(61, 0, 0)); // note on at velocity 0 is an off
    // A note off says how fast the key came up.
    assert_eq!(d.decode(&[0x83, 62, 90]), on(62, 0, 90));
    assert_eq!(d.decode(&[0x90]), None);
}

#[test]
fn a_channels_moves_are_everyones_and_a_notes_own_are_its() {
    let mut d = Decoder::default();
    let moved = |m| Some(Live::Move(m));
    // Channel 1: the bend, pressure and wheel of everything.
    assert_eq!(d.decode(&[0xE0, 0, 96]), moved(ControlKind::PitchBend(0.5)));
    assert_eq!(d.decode(&[0xD0, 127]), moved(ControlKind::Aftertouch(1.0)));
    let wheel = ControlKind::Cc {
        number: 1,
        value: 1.0,
    };
    assert_eq!(d.decode(&[0xB0, 1, 127]), moved(wheel));
    // An MPE controller: each note on its own channel, its bend, slide and pressure its own.
    assert!(d.decode(&[0x92, 64, 90]).is_some());
    let bent = ControlKind::Expression {
        pitch: 64,
        kind: ExpressionKind::Tuning,
        value: 24.0,
    };
    assert_eq!(d.decode(&[0xE2, 0, 96]), moved(bent));
    let slid = ControlKind::Expression {
        pitch: 64,
        kind: ExpressionKind::Brightness,
        value: 1.0,
    };
    assert_eq!(d.decode(&[0xB2, 74, 127]), moved(slid));
    let pressed = ControlKind::PolyPressure {
        pitch: 64,
        value: 1.0,
    };
    assert_eq!(d.decode(&[0xD2, 127]), moved(pressed));
    // Its note let go, its channel bends no one.
    assert!(d.decode(&[0x82, 64, 0]).is_some());
    assert_eq!(d.decode(&[0xE2, 0, 96]), None);
}

#[test]
fn with_mpe_off_every_channels_moves_are_everyones() {
    let mut d = Decoder::new(InputSetup {
        mpe: false,
        ..Default::default()
    });
    assert!(d.decode(&[0x92, 64, 90]).is_some());
    let moved = |m| Some(Live::Move(m));
    assert_eq!(d.decode(&[0xE2, 0, 96]), moved(ControlKind::PitchBend(0.5)));
    assert_eq!(d.decode(&[0xD2, 127]), moved(ControlKind::Aftertouch(1.0)));
    assert!(!d.found);
}

#[test]
fn a_controller_says_its_zone_and_its_notes_bend_range() {
    let mut d = Decoder::default();
    // An upper zone: channel 16 manages, 15 below it play notes.
    for m in [[0xBF, 101, 0], [0xBF, 100, 6], [0xBF, 6, 15]] {
        assert_eq!(d.decode(&m), None);
    }
    assert!(d.found);
    // Its member channels bend 24 semitones at the most.
    for m in [[0xB0, 101, 0], [0xB0, 100, 0], [0xB0, 6, 24]] {
        assert_eq!(d.decode(&m), None);
    }
    assert!(d.decode(&[0x90, 60, 90]).is_some());
    let bent = ControlKind::Expression {
        pitch: 60,
        kind: ExpressionKind::Tuning,
        value: 12.0,
    };
    assert_eq!(d.decode(&[0xE0, 0, 96]), Some(Live::Move(bent)));
    // Channel 16 is everyone's now.
    assert_eq!(
        d.decode(&[0xEF, 0, 96]),
        Some(Live::Move(ControlKind::PitchBend(0.5)))
    );
}
