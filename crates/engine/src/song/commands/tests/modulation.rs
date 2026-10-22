//! Modulators and stacks by command.

use super::*;

#[test]
fn a_stack_says_where_its_tracks_went_and_a_follower_names_its_track_by_its_place() {
    use crate::dsp::modulate::ModKind;
    let mut p = Project::new(48_000);
    for name in ["Kick", "Bass", "Snare"] {
        run(
            &mut p,
            &format!(r#"{{"op": "add_track", "name": "{name}", "instrument": "synth"}}"#),
        )
        .unwrap();
    }
    let made = run(&mut p, r#"{"op": "add_stack", "tracks": [0, 2]}"#).unwrap();
    assert_eq!(made["tracks"], json!([0, 1]));
    assert_eq!(made["moved"], json!([0, 2, 1]));
    // The bass (now third) ducks under the kick, named by its place.
    run(&mut p, r#"{"op": "add_modulator", "track": 2, "modulator": {"type": "follower", "source": 0, "attack_ms": 5, "release_ms": 150}}"#).unwrap();
    assert!(matches!(
        p.tracks[2].modulators[0].kind,
        ModKind::Follower { source, .. } if source == p.tracks[0].id
    ));
    let e = run(&mut p, r#"{"op": "add_modulator", "track": 2, "modulator": {"type": "follower", "source": 9, "attack_ms": 5, "release_ms": 150}}"#).unwrap_err();
    assert!(e.contains("track's place"), "{e}");
    let a = p.auxes.iter().position(|a| a.stack).unwrap();
    run(&mut p, &format!(r#"{{"op": "flatten_stack", "aux": {a}}}"#)).unwrap();
    assert!(p.auxes.is_empty() && p.tracks.iter().all(|t| t.output.is_none()));
}

#[test]
fn a_stack_shares_a_modulator_with_every_track_in_it() {
    use crate::dsp::modulate::{ModKind, ModParam};
    let mut p = Project::new(48_000);
    for name in ["Bass", "Pad", "Lead"] {
        run(
            &mut p,
            &format!(r#"{{"op": "add_track", "name": "{name}", "instrument": "synth"}}"#),
        )
        .unwrap();
    }
    run(
        &mut p,
        r#"{"op": "add_stack", "tracks": [0, 1], "name": "Keys"}"#,
    )
    .unwrap();
    let a = p.auxes.iter().position(|a| a.stack).unwrap();
    let made = run(
        &mut p,
        &format!(r#"{{"op": "share_modulator", "aux": {a}, "modulator": {{"type": "macro"}}}}"#),
    )
    .unwrap();
    assert_eq!(made["index"], 0);
    let shared = ModKind::Shared { modulator: 0 };
    assert_eq!(p.tracks[0].modulators[0].kind, shared);
    assert_eq!(p.tracks[1].modulators[0].kind, shared);
    assert!(p.tracks[2].modulators.is_empty(), "the lead isn't in it");
    // Routed on each, set (and automated) once for all.
    run(
        &mut p,
        r#"{"op": "modulate", "track": 1, "modulator": 0, "target": "volume", "depth": -12}"#,
    )
    .unwrap();
    run(&mut p, r#"{"op": "set_modulator", "track": 1, "index": 0, "modulator": {"type": "macro", "value": 0.5}}"#).unwrap();
    assert_eq!(p.mod_kind(0, 0), Some(&ModKind::Macro { value: 0.5 }));
    assert_eq!(p.tracks[1].modulators[0].kind, shared);
    assert_eq!(p.tracks[1].modulators[0].routes.len(), 1);
    run(&mut p, r#"{"op": "set_automation", "track": 0, "param": "modulator", "modulator": 0, "setting": "value", "points": [{"tick": 0, "value": 1}]}"#).unwrap();
    assert!(
        !p.tracks[0]
            .automation
            .points(Param::Mod(0, ModParam::Value))
            .is_empty()
    );
    // A track outside the stack can't play it.
    let e = run(
        &mut p,
        r#"{"op": "add_modulator", "track": 2, "modulator": {"type": "shared", "modulator": 0}}"#,
    )
    .unwrap_err();
    assert!(e.contains("isn't in a stack"), "{e}");
    // Saved and read back, as it was.
    let back: Project =
        serde_json::from_str(&serde_json::to_string(&p.portable()).unwrap()).unwrap();
    assert_eq!(back.auxes[a].modulators, p.auxes[a].modulators);
    assert_eq!(back.tracks[1].modulators, p.tracks[1].modulators);
    // Off the stack: off every track in it, its lanes too.
    run(
        &mut p,
        &format!(r#"{{"op": "unshare_modulator", "aux": {a}, "index": 0}}"#),
    )
    .unwrap();
    assert!(p.auxes[a].modulators.is_empty());
    assert!(p.tracks.iter().all(|t| t.modulators.is_empty()));
    assert!(p.tracks[0].automation.mods.is_empty());
}

#[test]
fn a_modulators_settings_are_automated_and_go_with_it() {
    use crate::dsp::modulate::ModParam;
    let mut p = Project::new(48_000);
    run(
        &mut p,
        r#"{"op": "add_track", "name": "Pad", "instrument": "synth"}"#,
    )
    .unwrap();
    for m in [
        r#"{"type": "lfo", "shape": "sine", "rate": {"beats": 1}}"#,
        r#"{"type": "macro"}"#,
    ] {
        run(
            &mut p,
            &format!(r#"{{"op": "add_modulator", "track": 0, "modulator": {m}}}"#),
        )
        .unwrap();
    }
    run(&mut p, r#"{"op": "set_automation", "track": 0, "param": "modulator", "modulator": 1, "setting": "value", "points": [{"tick": 0, "value": 0}, {"tick": 3840, "value": 2}]}"#).unwrap();
    let lane = p.tracks[0]
        .automation
        .points(Param::Mod(1, ModParam::Value));
    // Kept to its range: a macro's value is 0 to 1.
    assert_eq!(lane.iter().map(|x| x.value).collect::<Vec<_>>(), [0.0, 1.0]);
    // A setting it hasn't: said, with what it has.
    let e = run(&mut p, r#"{"op": "set_automation", "track": 0, "param": "modulator", "modulator": 1, "setting": "attack", "points": []}"#).unwrap_err();
    assert!(e.contains("has value"), "{e}");
    run(&mut p, r#"{"op": "shape_automation", "track": 0, "param": "modulator", "modulator": 0, "setting": "phase", "shape": "ramp_up", "from": 0, "to": 3840, "low": 0, "high": 1}"#).unwrap();
    assert!(
        !p.tracks[0]
            .automation
            .points(Param::Mod(0, ModParam::Phase))
            .is_empty()
    );
    // The LFO taken off: its lane goes, and the macro's moves up a place with it.
    run(
        &mut p,
        r#"{"op": "remove_modulator", "track": 0, "index": 0}"#,
    )
    .unwrap();
    assert!(
        p.tracks[0]
            .automation
            .points(Param::Mod(0, ModParam::Phase))
            .is_empty()
    );
    assert_eq!(
        p.tracks[0]
            .automation
            .points(Param::Mod(0, ModParam::Value))
            .len(),
        2
    );
}

#[test]
fn modulators_are_added_routed_and_removed() {
    use crate::dsp::modulate::{ModKind, ModTarget, SamplerParam};
    let mut p = Project::new(48_000);
    run(
        &mut p,
        r#"{"op": "add_track", "name": "Bass", "instrument": "synth"}"#,
    )
    .unwrap();
    run(
        &mut p,
        r#"{"op": "add_modulator", "track": 0, "modulator": {"type": "lfo", "shape": "square", "rate": {"beats": 0.5}}}"#,
    )
    .unwrap();
    run(&mut p, r#"{"op": "modulate", "track": 0, "modulator": 0, "target": {"sampler": "cutoff"}, "depth": 2}"#).unwrap();
    run(
        &mut p,
        r#"{"op": "modulate", "track": 0, "modulator": 0, "target": "pan", "depth": 5}"#,
    )
    .unwrap();
    let m = &p.tracks[0].modulators[0];
    assert!(matches!(m.kind, ModKind::Lfo { .. }));
    let routes: Vec<_> = m.routes.iter().map(|r| (r.target, r.depth)).collect();
    assert_eq!(
        routes,
        [
            (ModTarget::Sampler(SamplerParam::Cutoff), 2.0),
            (ModTarget::Pan, 2.0)
        ]
    );
    run(
        &mut p,
        r#"{"op": "modulate", "track": 0, "modulator": 0, "target": "pan", "depth": 0}"#,
    )
    .unwrap();
    assert_eq!(p.tracks[0].modulators[0].routes.len(), 1);
    let err = run(
        &mut p,
        r#"{"op": "remove_modulator", "track": 0, "index": 3}"#,
    );
    assert!(err.unwrap_err().contains("no modulator 3"));
    run(
        &mut p,
        r#"{"op": "remove_modulator", "track": 0, "index": 0}"#,
    )
    .unwrap();
    assert!(p.tracks[0].modulators.is_empty());
}
