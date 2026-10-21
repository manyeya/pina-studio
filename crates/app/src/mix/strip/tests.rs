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
    ] {
        let before = comp_text(field, &c);
        let v = comp_pos(field, &c);
        comp_set(field, &mut c, v);
        assert_eq!(comp_text(field, &c), before, "{field:?}");
    }
}
