use super::*;

#[test]
fn fader_taper_round_trips() {
    for db in [-60.0, -12.0, 0.0, 6.0] {
        assert!((fader_db(fader_pos(db)) - db).abs() < 1e-3);
    }
    assert!((fader_pos(0.0) - 0.708).abs() < 1e-3);
    assert_eq!(fader_db(0.0), SILENT);
}

#[test]
fn knob_scales_round_trip() {
    assert_eq!(low_cut_hz(low_cut_pos(80.0)), 80.0);
    assert_eq!(low_cut_hz(0.0), 0.0);
    assert_eq!(eq_gain_db(eq_gain_pos(3.5)), 3.5);
    let mut c = Comp::default();
    for field in [
        CompField::Threshold,
        CompField::Ratio,
        CompField::Attack,
        CompField::Release,
        CompField::Makeup,
        CompField::Mix,
        CompField::Sidechain,
        CompField::Spectral,
    ] {
        let before = comp_text(field, &c);
        let v = comp_pos(field, &c);
        comp_set(field, &mut c, v);
        assert_eq!(comp_text(field, &c), before, "{field:?}");
    }
}

#[test]
fn a_bus_compressor_turns_in_steps_and_bands_turn_alone() {
    let mut c = Comp {
        style: CompStyle::Bus,
        ..Comp::default()
    };
    let v = comp_pos(CompField::Ratio, &Comp { ratio: 5.0, ..c });
    comp_set(CompField::Ratio, &mut c, v);
    comp_set(CompField::Attack, &mut c, 0.5);
    assert_eq!(c.ratio, 4.0);
    assert!(
        CompStyle::BUS_ATTACKS.contains(&c.attack_ms),
        "{}",
        c.attack_ms
    );
    let mut s = Saturator::default();
    s.split(1_000.0);
    sat_set(SatField::Band(1, SatKnob::Drive), &mut s, 0.5);
    assert_eq!((s.bands[0].drive_db, s.bands[1].drive_db), (6.0, 12.0));
    for k in [
        SatKnob::Tone,
        SatKnob::Dynamics,
        SatKnob::Feedback,
        SatKnob::FeedbackHz,
        SatKnob::Mix,
        SatKnob::Level,
    ] {
        let f = SatField::Band(1, k);
        let before = sat_text(f, &s);
        let v = sat_pos(f, &s);
        sat_set(f, &mut s, v);
        assert_eq!(sat_text(f, &s), before, "{k:?}");
    }
}
