//! Plugin parameters and presets named in words.

use super::*;

#[test]
fn parameters_and_presets_are_named_in_words() {
    let mut p = Project::new(48_000);
    run(
        &mut p,
        r#"{"op": "add_track", "name": "Pad", "instrument": "synth"}"#,
    )
    .unwrap();
    p.tracks[0].instrument = Some(Instrument::Plugin(PluginRef::default()));
    let said = |p: &mut Project, json: &str| {
        let cmd: Command = serde_json::from_str(json).unwrap();
        let cmd = resolve(p, cmd, &mut Shown)?;
        apply(p, Path::new("."), cmd)
    };
    let ch = r#""channel": {"track": 0}, "plugin": "instrument""#;
    let set = |what: &str| format!(r#"{{"op": "set_plugin_param", {ch}, {what}}}"#);
    said(&mut p, &set(r#""param": "cutoff", "text": "250 Hz""#)).unwrap();
    said(&mut p, &set(r#""param": "Wave", "text": "square""#)).unwrap();
    let r = p.tracks[0].plugin(Which::Instrument).unwrap().clone();
    assert!((r.params.get(&3).unwrap() - 0.25).abs() < 1e-3);
    assert_eq!(r.params.get(&9), Some(&0.5));
    // Set in that order, and kept in it.
    assert_eq!(
        r.params.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [3, 9]
    );
    let wrong = said(&mut p, &set(r#""param": "Wave", "text": "Triangle""#));
    assert!(wrong.unwrap_err().contains("Saw, Square, Sine"));
    let far = said(&mut p, &set(r#""param": "cutoff", "text": "5 kHz""#));
    assert!(far.unwrap_err().contains("out of its range"));
    // A preset by part of its name: it replaces what was set.
    let preset = format!(r#"{{"op": "set_preset", {ch}, "preset": "sub"}}"#);
    said(&mut p, &preset).unwrap();
    let r = p.tracks[0].plugin(Which::Instrument).unwrap();
    assert_eq!(r.preset.as_ref().unwrap().name, "Sub Bass");
    assert!(r.params.is_empty());
    // Automation points by text, the parameter by name.
    let auto = r#"{"op": "set_automation", "track": 0, "param": "plugin", "plugin": "instrument",
        "id": "Filter Cutoff", "points": [{"tick": 0, "text": "100 Hz"}, {"tick": 960, "value": 1}]}"#;
    said(&mut p, auto).unwrap();
    let lane = &p.tracks[0].automation.plugins[0];
    assert_eq!((lane.param, lane.name.as_str()), (3, "Filter Cutoff"));
    assert!((lane.points[0].value - 0.1).abs() < 1e-3);
    // Unresolved, a name can't be applied.
    let raw: Command = serde_json::from_str(&set(r#""param": "Wave", "value": 1"#)).unwrap();
    assert!(apply(&mut p, Path::new("."), raw).is_err());
}
