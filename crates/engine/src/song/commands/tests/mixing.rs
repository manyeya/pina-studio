//! Effects set field by field; pan modes.

use super::*;

#[test]
fn effects_change_only_what_is_given() {
    let mut p = Project::new(48_000);
    run(&mut p, r#"{"op": "add_track", "name": "Keys"}"#).unwrap();
    let ch = r#""channel": {"track": 0}"#;
    run(
        &mut p,
        &format!(r#"{{"op": "set_eq", {ch}, "eq": {{"low_cut": 150}}}}"#),
    )
    .unwrap();
    let shelf = r#"{"high_shelf": {"gain_db": 2.5}}"#;
    run(
        &mut p,
        &format!(r#"{{"op": "set_eq", {ch}, "eq": {shelf}}}"#),
    )
    .unwrap();
    // The old EQ's names: a low cut and a shelf, the shelf at its frequency and Q.
    let eq = p.tracks[0].eq.unwrap();
    let shapes: Vec<_> = eq
        .bands
        .iter()
        .map(|b| (b.shape, b.freq, b.gain_db))
        .collect();
    assert_eq!(
        shapes,
        [
            (crate::song::project::Shape::LowCut, 150.0, 0.0),
            (crate::song::project::Shape::HighShelf, 10_000.0, 2.5)
        ]
    );
    assert_eq!(eq.bands[1].q, std::f32::consts::FRAC_1_SQRT_2);
    // Bands by place: one changed, one added, a dynamic one, one taken out.
    let band = |b: &str| format!(r#"{{"op": "set_eq_band", {ch}, {b}}}"#);
    run(&mut p, &band(r#""band": 0, "slope": 24"#)).unwrap();
    run(
        &mut p,
        &band(r#""band": 2, "freq": 300, "gain_db": -3, "q": 1.5, "placement": "mid""#),
    )
    .unwrap();
    run(
        &mut p,
        &band(r#""band": 2, "dynamic": {"range_db": -4, "threshold_db": -24}"#),
    )
    .unwrap();
    let eq = p.tracks[0].eq.unwrap();
    assert_eq!(eq.bands[0].slope, 24.0);
    let b = eq.bands[2];
    assert_eq!((b.freq, b.gain_db, b.q), (300.0, -3.0, 1.5));
    assert_eq!(b.placement, crate::song::project::Placement::Mid);
    assert_eq!(b.dynamic.unwrap().range_db, -4.0);
    // Its attack and release its own, by its frequency.
    assert_eq!(b.dynamic.unwrap().attack_ms, None);
    assert_eq!(b.dynamic.unwrap().threshold_db, Some(-24.0));
    let wrong = run(&mut p, &band(r#""band": 0, "slope": 120"#)).unwrap_err();
    assert!(wrong.contains("band 0: slope 120"), "{wrong}");
    // Fractional slopes, on any shape: a bell steeper.
    run(&mut p, &band(r#""band": 0, "slope": 20.5"#)).unwrap();
    run(&mut p, &band(r#""band": 2, "slope": 36"#)).unwrap();
    let eq = p.tracks[0].eq.unwrap();
    assert_eq!((eq.bands[0].slope, eq.bands[2].slope), (20.5, 36.0));
    run(&mut p, &band(r#""band": 0, "slope": 24"#)).unwrap();
    // Spectral, and the EQ in linear phase at high resolution.
    run(
        &mut p,
        &band(r#""band": 2, "dynamic": {"spectral": true, "density": 0.8, "tilt": false}"#),
    )
    .unwrap();
    let d = p.tracks[0].eq.unwrap().bands[2].dynamic.unwrap();
    assert!(
        d.spectral && !d.tilt && d.density == 0.8 && d.range_db == -4.0,
        "{d:?}"
    );
    let set = |eq: &str| format!(r#"{{"op": "set_eq", {ch}, "eq": {eq}}}"#);
    run(&mut p, &set(r#"{"processing": "linear", "resolution": 2}"#)).unwrap();
    let eq = p.tracks[0].eq.unwrap();
    assert_eq!(
        (eq.processing, eq.resolution, eq.bands.len()),
        (crate::song::project::Processing::Linear, 2, 3)
    );
    let wrong = run(&mut p, &set(r#"{"resolution": 7}"#)).unwrap_err();
    assert!(wrong.contains("resolution 7"), "{wrong}");
    let wrong = run(&mut p, &band(r#""band": 2, "dynamic": {"density": 2}"#)).unwrap_err();
    assert!(wrong.contains("density"), "{wrong}");
    run(&mut p, &set(r#"{"processing": "zero_latency"}"#)).unwrap();
    assert!(
        run(&mut p, &band(r#""band": 5"#))
            .unwrap_err()
            .contains("no band 5")
    );
    run(&mut p, &band(r#""band": 1, "remove": true"#)).unwrap();
    assert_eq!(p.tracks[0].eq.unwrap().bands.len(), 2);
    // The whole list at once.
    run(
        &mut p,
        &format!(
            r#"{{"op": "set_eq", {ch}, "eq": {{"bands": [{{"shape": "high_cut", "freq": 9000, "slope": 48}}], "output_db": -2}}}}"#
        ),
    )
    .unwrap();
    let eq = p.tracks[0].eq.unwrap();
    assert_eq!(
        (eq.bands.len(), eq.bands[0].slope, eq.output_db),
        (1, 48.0, -2.0)
    );
    // A cut written without a width has a cut's: no bump at its corner.
    assert_eq!(eq.bands[0].q, std::f32::consts::FRAC_1_SQRT_2);
    let comp = r#"{"threshold_db": -9, "ratio": 2, "rms": true}"#;
    run(
        &mut p,
        &format!(r#"{{"op": "set_comp", {ch}, "comp": {comp}}}"#),
    )
    .unwrap();
    run(
        &mut p,
        &format!(r#"{{"op": "set_comp", {ch}, "comp": {{"makeup_db": 6}}}}"#),
    )
    .unwrap();
    let c = p.tracks[0].comp.unwrap();
    assert_eq!(
        (c.threshold_db, c.ratio, c.makeup_db, c.rms),
        (-9.0, 2.0, 6.0, true)
    );
    let sat = r#"{"drive_db": 7, "mix": 0.45}"#;
    run(
        &mut p,
        &format!(r#"{{"op": "set_saturator", {ch}, "sat": {sat}}}"#),
    )
    .unwrap();
    run(
        &mut p,
        &format!(r#"{{"op": "set_saturator", {ch}, "sat": {{"tone": 0.1}}}}"#),
    )
    .unwrap();
    let s = p.tracks[0].sat.unwrap();
    assert_eq!((s.drive_db, s.mix, s.tone), (7.0, 0.45, 0.1));
    // Bypassed and back, and taken out.
    run(
        &mut p,
        &format!(r#"{{"op": "set_comp", {ch}, "comp": {{"bypass": true}}}}"#),
    )
    .unwrap();
    assert!(p.tracks[0].comp.unwrap().bypass);
    assert_eq!(p.tracks[0].comp.unwrap().ratio, 2.0);
    run(&mut p, &format!(r#"{{"op": "set_comp", {ch}}}"#)).unwrap();
    assert!(p.tracks[0].comp.is_none());
    assert!(
        run(
            &mut p,
            &format!(r#"{{"op": "set_eq", {ch}, "eq": {{"low_cut": "x"}}}}"#)
        )
        .is_err()
    );
}

#[test]
fn pan_modes_spread_and_the_pan_law() {
    let mut p = Project::new(48_000);
    run(
        &mut p,
        r#"{"op": "add_track", "name": "Pad", "instrument": "synth"}"#,
    )
    .unwrap();
    run(
        &mut p,
        r#"{"op": "set_track", "track": 0, "pan_mode": "stereo", "spread": 0.4}"#,
    )
    .unwrap();
    run(
        &mut p,
        r#"{"op": "set_master", "pan_law": "minus3_compensated"}"#,
    )
    .unwrap();
    let t = &p.tracks[0];
    assert_eq!((t.pan_mode, t.spread), (PanMode::Stereo, Some(0.4)));
    assert_eq!(
        (p.pan_law, p.master_volume_db),
        (PanLaw::Minus3Compensated, 0.0)
    );
}
