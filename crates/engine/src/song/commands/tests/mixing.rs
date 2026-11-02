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
    assert_eq!(
        (s.bands[0].drive_db, s.mix, s.bands[0].tone),
        (7.0, 0.45, 0.1)
    );
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
    // A limiter, set a piece at a time.
    run(
        &mut p,
        &format!(r#"{{"op": "set_limiter", {ch}, "limit": {{"style": "modern", "gain_db": 6}}}}"#),
    )
    .unwrap();
    run(
        &mut p,
        &format!(r#"{{"op": "set_limiter", {ch}, "limit": {{"ceiling_db": -0.3}}}}"#),
    )
    .unwrap();
    let l = p.tracks[0].limit.unwrap();
    assert_eq!(
        (l.style, l.gain_db, l.ceiling_db, l.true_peak),
        (crate::song::project::LimitStyle::Modern, 6.0, -0.3, true)
    );
    // Its gain automated, kept in its range.
    run(
        &mut p,
        r#"{"op": "set_automation", "track": 0, "param": "limit_gain", "points": [{"tick": 0, "value": 3}, {"tick": 960, "value": 99}]}"#,
    )
    .unwrap();
    let gain = p.tracks[0]
        .automation
        .points(crate::song::project::Param::Limit(
            crate::song::project::LimitParam::Gain,
        ));
    assert_eq!((gain[0].value, gain[1].value), (3.0, 30.0));
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

#[test]
fn built_ins_stack_any_number_of_each() {
    use crate::song::project::{Fx, Unit, chain};
    let mut p = Project::new(48_000);
    run(
        &mut p,
        r#"{"op": "add_track", "name": "Bass", "instrument": "synth"}"#,
    )
    .unwrap();
    let ch = r#""channel": {"track": 0}"#;
    // Its own compressor, then another after it: a gentle one and a hard one.
    run(
        &mut p,
        &format!(r#"{{"op": "set_comp", {ch}, "comp": {{"ratio": 2}}}}"#),
    )
    .unwrap();
    run(
        &mut p,
        &format!(
            r#"{{"op": "set_comp", {ch}, "which": 1, "comp": {{"ratio": 8, "attack_ms": 1}}}}"#
        ),
    )
    .unwrap();
    // Two more EQs: one after the other compressor, one to take out again.
    for _ in 0..2 {
        let n = p.tracks[0]
            .more
            .iter()
            .filter(|u| u.kind() == Fx::Eq)
            .count()
            + 1;
        run(
            &mut p,
            &format!(
                r#"{{"op": "set_eq", {ch}, "which": {n}, "eq": {{"bands": [{{"shape": "high_cut", "freq": 8000}}]}}}}"#
            ),
        )
        .unwrap();
    }
    let t = &p.tracks[0];
    assert_eq!(t.comp.unwrap().ratio, 2.0);
    assert!(matches!(t.more[0], Unit::Comp(c) if c.ratio == 8.0 && c.attack_ms == 1.0));
    assert_eq!(t.more.len(), 3);
    // A band on the second EQ.
    run(
        &mut p,
        &format!(r#"{{"op": "set_eq_band", {ch}, "which": 2, "band": 1, "shape": "bell", "freq": 300, "gain_db": -3}}"#),
    )
    .unwrap();
    assert!(matches!(p.tracks[0].more[2], Unit::Eq(e) if e.bands.len() == 2));
    // Nothing past one more than there are.
    let wrong = run(
        &mut p,
        &format!(r#"{{"op": "set_comp", {ch}, "which": 4, "comp": {{}}}}"#),
    )
    .unwrap_err();
    assert!(wrong.contains("which 2 adds one"), "{wrong}");
    // Taken out: the second extra EQ goes, the rest stay where they run.
    run(&mut p, &format!(r#"{{"op": "set_eq", {ch}, "which": 1}}"#)).unwrap();
    let t = &p.tracks[0];
    assert_eq!(t.more.len(), 2);
    assert_eq!(
        chain(&t.chain, 0, 2),
        [Fx::Eq, Fx::Comp, Fx::Sat, Fx::More, Fx::More, Fx::Limit]
    );
    // Moved before everything: the hard compressor first.
    run(
        &mut p,
        r#"{"op": "move_effect", "channel": {"track": 0}, "from": 3, "to": 0}"#,
    )
    .unwrap();
    assert!(matches!(p.tracks[0].more[0], Unit::Comp(_)));
    assert_eq!(chain(&p.tracks[0].chain, 0, 2)[0], Fx::More);
}

#[test]
fn reverbs_and_delays_stack_and_take_their_styles() {
    use crate::song::project::{DelayStyle, ReverbStyle, Unit};
    let mut p = Project::new(48_000);
    p.add_aux();
    let ch = r#""channel": {"aux": 0}"#;
    run(
        &mut p,
        &format!(r#"{{"op": "set_reverb", {ch}, "reverb": {{"style": "plate", "mix": 1}}}}"#),
    )
    .unwrap();
    // A plate as plates are, all reverb.
    let Unit::Reverb(r) = p.auxes[0].more[0] else {
        panic!("{:?}", p.auxes[0].more)
    };
    let plate = ReverbStyle::Plate.reverb();
    assert_eq!(
        (r.style, r.early, r.decay_s, r.mix),
        (ReverbStyle::Plate, 0.0, plate.decay_s, 1.0)
    );
    // Changed field by field; another after it; a delay too.
    run(
        &mut p,
        &format!(
            r#"{{"op": "set_reverb", {ch}, "reverb": {{"decay_s": 3.5, "predelay_beats": 0.125}}}}"#
        ),
    )
    .unwrap();
    run(
        &mut p,
        &format!(r#"{{"op": "set_reverb", {ch}, "which": 1, "reverb": {{"style": "room"}}}}"#),
    )
    .unwrap();
    run(
        &mut p,
        &format!(
            r#"{{"op": "set_delay", {ch}, "delay": {{"style": "ping_pong", "feedback": 0.6}}}}"#
        ),
    )
    .unwrap();
    let more = &p.auxes[0].more;
    assert!(
        matches!(more[0], Unit::Reverb(r) if r.decay_s == 3.5 && r.predelay_beats == Some(0.125) && r.style == ReverbStyle::Plate)
    );
    assert!(matches!(more[1], Unit::Reverb(r) if r.style == ReverbStyle::Room));
    assert!(
        matches!(more[2], Unit::Delay(d) if d.style == DelayStyle::PingPong && d.feedback == 0.6 && d.cross == 1.0)
    );
    let wrong = run(
        &mut p,
        &format!(r#"{{"op": "set_delay", {ch}, "which": 3, "delay": {{}}}}"#),
    )
    .unwrap_err();
    assert!(wrong.contains("which 1 adds one"), "{wrong}");
    // Taken out.
    run(
        &mut p,
        &format!(r#"{{"op": "set_reverb", {ch}, "which": 1}}"#),
    )
    .unwrap();
    assert_eq!(p.auxes[0].more.len(), 2);
}

#[test]
fn a_stacked_effects_knob_is_automated_in_its_own_units() {
    use crate::song::project::{Param, Unit};
    let mut p = Project::new(48_000);
    run(&mut p, r#"{"op": "add_track", "name": "Pad"}"#).unwrap();
    let ch = r#""channel": {"track": 0}"#;
    run(
        &mut p,
        &format!(r#"{{"op": "set_fx", {ch}, "fx": "gate", "settings": {{}}}}"#),
    )
    .unwrap();
    run(
        &mut p,
        &format!(r#"{{"op": "set_fx", {ch}, "fx": "filter", "settings": {{"mode": "ladder"}}}}"#),
    )
    .unwrap();
    assert!(matches!(p.tracks[0].more[1], Unit::Filter(_)));
    // 632 Hz is halfway up its knob; past 20 kHz stays at the top.
    run(&mut p, r#"{"op": "set_automation", "track": 0, "param": "fx", "fx": "filter", "knob": "cutoff", "points": [{"tick": 0, "value": 632.5}, {"tick": 960, "value": 99000}]}"#).unwrap();
    let points = p.tracks[0].automation.points(Param::Tool(1, 0));
    assert!((points[0].value - 0.5).abs() < 1e-3, "{points:?}");
    assert_eq!(points[1].value, 1.0);
    // Drawn as a shape: its low and high as the knob reads too.
    run(&mut p, r#"{"op": "shape_automation", "track": 0, "param": "fx", "fx": "filter", "knob": "resonance", "shape": "sine", "from": 0, "to": 960, "every": 960, "low": 0.25, "high": 0.75}"#).unwrap();
    let points = p.tracks[0].automation.points(Param::Tool(1, 1));
    assert!(
        points.iter().all(|x| (0.24..=0.76).contains(&x.value)),
        "{points:?}"
    );
    // The track's own compressor, by its knob's name.
    run(
        &mut p,
        &format!(r#"{{"op": "set_comp", {ch}, "comp": {{"threshold_db": -12}}}}"#),
    )
    .unwrap();
    run(&mut p, r#"{"op": "set_automation", "track": 0, "param": "fx", "fx": "comp", "own": true, "knob": "threshold", "points": [{"tick": 0, "value": -30}]}"#).unwrap();
    let points = p.tracks[0]
        .automation
        .points(Param::Tool(crate::song::project::OWN_COMP, 0));
    assert!((points[0].value - 0.5).abs() < 1e-3, "{points:?}");
    // What it doesn't have says what it does.
    let e = run(&mut p, r#"{"op": "set_automation", "track": 0, "param": "fx", "fx": "filter", "knob": "wobble", "points": []}"#).unwrap_err();
    assert!(e.contains("Cutoff") && e.contains("Resonance"), "{e}");
    let e = run(&mut p, r#"{"op": "set_automation", "track": 0, "param": "fx", "fx": "stereo", "knob": "mid", "points": []}"#).unwrap_err();
    assert!(e.contains("set_fx"), "{e}");
    let e = run(&mut p, r#"{"op": "set_automation", "track": 0, "param": "fx", "fx": "wah", "knob": "mid", "points": []}"#).unwrap_err();
    assert!(e.contains("`fx` is"), "{e}");
}
