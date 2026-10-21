use super::*;
#[test]
fn a_drum_pattern() {
    let mut p = Project::new(48_000);
    run(
        &mut p,
        r#"{"op": "add_track", "name": "Drums", "instrument": "drums"}"#,
    )
    .unwrap();
    let r = run(
        &mut p,
        r#"{"op": "add_pattern_region", "track": 0, "start": 0, "length": 3840,
            "rows": [{"pitch": 36, "cells": [100, 0, 0, 0, 100, 0, 0, 0]}]}"#,
    );
    assert_eq!(r, Ok(json!({"region": [0, 0]})));
    let set = r#"{"op": "set_pattern", "track": 0, "region": 0, "rows": [{"pitch": 38, "cells": [0, 0, 0, 0, 90]}]}"#;
    run(&mut p, set).unwrap();
    let notes = p.region((0, 0)).content.notes(3840).unwrap();
    // Five-step loop, a snare on step 5: sixteenths 4, 9 and 14 in a bar of 16.
    let starts: Vec<i64> = notes.iter().map(|n| n.start).collect();
    assert_eq!(starts, [960, 2160, 3360]);
}

fn run(p: &mut Project, json: &str) -> Result<Value, String> {
    let cmd: Command = serde_json::from_str(json).map_err(|e| e.to_string())?;
    apply(p, Path::new("/nonexistent"), cmd)
}

#[test]
fn an_agent_can_build_a_song() {
    let mut p = Project::new(48_000);
    run(&mut p, r#"{"op": "set_tempo", "bpm": 96}"#).unwrap();
    let t = run(
        &mut p,
        r#"{"op": "add_track", "name": "Keys", "instrument": "synth"}"#,
    )
    .unwrap();
    assert_eq!(t, json!({"track": 0}));
    let r = run(
        &mut p,
        r#"{"op": "add_midi_region", "track": 0, "start": 0, "length": 3840,
            "notes": [{"start": 0, "length": 960, "pitch": 60, "velocity": 100}]}"#,
    )
    .unwrap();
    assert_eq!(r, json!({"region": [0, 0]}));
    let half = run(
        &mut p,
        r#"{"op": "split_region", "track": 0, "region": 0, "at": 1920}"#,
    )
    .unwrap();
    assert_eq!(half, json!({"region": [0, 1]}));
    let bus = run(&mut p, r#"{"op": "add_send", "track": 0, "level_db": -6}"#).unwrap();
    assert_eq!(bus, json!({"bus": 1}));
    run(&mut p, r#"{"op": "set_aux", "aux": 0, "name": "Verb"}"#).unwrap();
    run(
        &mut p,
        r#"{"op": "set_automation", "track": 0, "param": "volume",
            "points": [{"tick": 960, "value": -6}, {"tick": 0, "value": 0}]}"#,
    )
    .unwrap();
    assert_eq!(p.tempo, 96.0);
    assert_eq!(p.tracks[0].regions.len(), 2);
    assert_eq!(p.auxes[0].name, "Verb");
    assert_eq!(p.tracks[0].automation.volume[0].tick, 0);
    // Bent and stepped points; a send's level and the mute, by send and as steps.
    run(
        &mut p,
        r#"{"op": "set_automation", "track": 0, "param": "pan",
            "points": [{"tick": 0, "value": -1, "curve": 0.5}, {"tick": 960, "value": 1, "step": true}]}"#,
    )
    .unwrap();
    let pan = &p.tracks[0].automation.pan;
    assert_eq!((pan[0].curve, pan[1].step), (0.5, true));
    run(
        &mut p,
        r#"{"op": "set_automation", "track": 0, "param": "send", "send": 0,
            "points": [{"tick": 0, "value": -20}]}"#,
    )
    .unwrap();
    assert_eq!(p.tracks[0].automation.sends[0].bus, 1);
    let no_send = run(
        &mut p,
        r#"{"op": "set_automation", "track": 0, "param": "send", "send": 3, "points": []}"#,
    );
    assert!(no_send.unwrap_err().contains("no send 3"));
    run(
        &mut p,
        r#"{"op": "shape_automation", "track": 0, "param": "mute", "shape": "square",
            "from": 0, "to": 3840, "every": 960, "low": 0, "high": 1}"#,
    )
    .unwrap();
    let mute = &p.tracks[0].automation.mute;
    assert!(mute.len() >= 8 && mute.iter().all(|p| p.step));
    // An S, then smoothed: a gentle hill made of more points.
    run(
        &mut p,
        r#"{"op": "set_automation", "track": 0, "param": "volume",
            "points": [{"tick": 0, "value": -40, "ease": true}, {"tick": 1920, "value": 0}, {"tick": 3840, "value": -40}]}"#,
    )
    .unwrap();
    assert!(p.tracks[0].automation.volume[0].ease);
    run(
        &mut p,
        r#"{"op": "smooth_automation", "track": 0, "param": "volume", "width": 960}"#,
    )
    .unwrap();
    assert!(p.tracks[0].automation.volume.len() > 3);
    let backwards = run(
        &mut p,
        r#"{"op": "smooth_automation", "track": 0, "param": "volume", "from": 9, "to": 3}"#,
    );
    assert!(backwards.unwrap_err().contains("after"));
    // The person's palette: sounds with what they're for, these only; then only changed.
    run(
        &mut p,
        r#"{"op": "set_palette", "sounds": [{"path": "/s/kick.wav", "role": "kick"},
            {"path": "/s/keys.wav"}], "only": true}"#,
    )
    .unwrap();
    assert_eq!(p.palette.sounds.len(), 2);
    assert_eq!(p.palette.sounds[0].role.as_deref(), Some("kick"));
    assert!(p.palette.only && p.palette.has(Path::new("/s/keys.wav")));
    run(&mut p, r#"{"op": "set_palette", "only": false}"#).unwrap();
    assert_eq!((p.palette.sounds.len(), p.palette.only), (2, false));
    let twice = run(
        &mut p,
        r#"{"op": "set_palette", "sounds": [{"path": "/s/a.wav"}, {"path": "/s/a.wav"}]}"#,
    );
    assert!(twice.unwrap_err().contains("twice"));
    run(
        &mut p,
        r#"{"op": "set_palette", "devices": [{"name": "Sine Synth", "instrument": true,
            "own": "synth", "role": "pad"}, {"name": "Room", "plugin": {"class": "Room",
            "path": "/p/Room.vst3"}}]}"#,
    )
    .unwrap();
    assert_eq!(p.palette.devices.len(), 2);
    assert_eq!(p.palette.sounds.len(), 2);
    assert!(p.palette.has_device(&p.palette.devices[1].clone()));
    let twice = run(
        &mut p,
        r#"{"op": "set_palette", "devices": [{"name": "A", "own": "eq"}, {"name": "B", "own": "eq"}]}"#,
    );
    assert!(twice.unwrap_err().contains("twice"));
    run(
        &mut p,
        r#"{"op": "set_automation_mode", "track": 0, "mode": "trim"}"#,
    )
    .unwrap();
    assert_eq!(p.tracks[0].automation.mode, AutoMode::Trim);
}

/// A plugin that only shows things: a cutoff from 0 to 1000 Hz, a three-way wave, two
/// presets. It reads no text itself, so `resolve` reads its display.
struct Shown;

impl PluginApi for Shown {
    fn name(&self) -> &str {
        "Shown"
    }
    fn params(&self) -> Vec<PluginParam> {
        let param = |id, name: &str, steps| PluginParam {
            id,
            name: name.into(),
            steps,
            ..Default::default()
        };
        vec![param(3, "Filter Cutoff", 0), param(9, "Wave", 2)]
    }
    fn set_param(&self, _: u32, _: f64) {}
    fn show_param(&self, _: u32, _: f64) {}
    fn param_value(&self, _: u32) -> f64 {
        0.0
    }
    fn param_text(&self, id: u32, v: f64) -> String {
        match id {
            3 => format!("{:.1} Hz", v * 1000.0),
            _ => ["Saw", "Square", "Sine"][(v * 2.0).round() as usize].into(),
        }
    }
    fn param_from_text(&self, _: u32, _: &str) -> Option<f64> {
        None
    }
    fn state(&self) -> Option<Vec<u8>> {
        None
    }
    fn set_state(&self, _: &[u8]) -> Result<(), String> {
        Ok(())
    }
    fn presets(&self) -> Vec<Preset> {
        let program = |n| PresetSource::Program {
            list: 0,
            program: n,
        };
        vec![
            Preset {
                name: "Warm Pad".into(),
                source: program(0),
            },
            Preset {
                name: "Sub Bass".into(),
                source: program(1),
            },
        ]
    }
    fn load_preset(&self, _: &Preset) -> Result<(), String> {
        Ok(())
    }
    fn save_preset(&self, _: &str) -> Result<PathBuf, String> {
        Err("no".into())
    }
    fn open_editor(&self, _: objc2::MainThreadMarker) -> Result<crate::plugin::Editor, String> {
        Err("no".into())
    }
}

impl PluginSource for Shown {
    fn plugin(&mut self, _: &Project, _: ChannelRef, _: Which) -> Result<&dyn PluginApi, String> {
        Ok(self)
    }
}

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
fn mistakes_say_what_went_wrong() {
    let mut p = Project::new(48_000);
    assert_eq!(
        run(&mut p, r#"{"op": "delete_track", "track": 2}"#),
        Err("no track 2: there are 0".into())
    );
    run(&mut p, r#"{"op": "add_track", "name": "Vox"}"#).unwrap(); // an audio track
    let midi = run(
        &mut p,
        r#"{"op": "add_midi_region", "track": 0, "start": 0, "length": 960}"#,
    );
    assert!(midi.unwrap_err().contains("audio track"));
    // A misspelt field is refused, not ignored.
    let typo = run(&mut p, r#"{"op": "set_track", "track": 0, "volume": -6}"#);
    assert!(typo.unwrap_err().contains("volume"));
    assert!(run(&mut p, r#"{"op": "set_tempo", "bpm": 0}"#).is_err());
    assert!(run(&mut p, r#"{"op": "add_send", "track": 0, "bus": 9}"#).is_err());
}

#[test]
fn audio_from_elsewhere_is_copied_in() {
    let base = std::env::temp_dir().join(format!("pina-test-cmd-{}", std::process::id()));
    let (dir, outside) = (base.join("song"), base.join("take.wav"));
    std::fs::create_dir_all(&dir).unwrap();
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&outside, spec).unwrap();
    (0..24_000).for_each(|_| w.write_sample(0i16).unwrap());
    w.finalize().unwrap();

    let mut p = Project::new(48_000);
    run(&mut p, r#"{"op": "add_track", "name": "Vox"}"#).unwrap();
    let cmd = Command::AddAudioRegion {
        track: 0,
        file: outside,
        start: PPQ,
        offset: 0,
        length: None,
    };
    apply(&mut p, &dir, cmd).unwrap();
    let r = p.region((0, 0));
    assert_eq!((r.start, r.length), (PPQ, PPQ)); // half a second at 120 bpm is a beat
    assert!(dir.join("Audio Files/take.wav").exists());
    // Sliced, and following the tempo: at 60 it plays twice as long, still a beat.
    run(
        &mut p,
        r#"{"op": "set_region", "track": 0, "region": 0, "flex": "slicing", "follow": true}"#,
    )
    .unwrap();
    run(&mut p, r#"{"op": "set_tempo", "bpm": 60}"#).unwrap();
    let r = p.region((0, 0));
    assert!(matches!(
        r.content,
        Content::Audio {
            flex: Flex::Slicing,
            follow: true,
            stretch: 2.0,
            ..
        }
    ));
    assert_eq!(r.length, PPQ);
    // MIDI has no flex.
    run(
        &mut p,
        r#"{"op": "add_track", "name": "Keys", "instrument": "synth"}"#,
    )
    .unwrap();
    run(
        &mut p,
        r#"{"op": "add_midi_region", "track": 1, "start": 0, "length": 960, "notes": []}"#,
    )
    .unwrap();
    let err = run(
        &mut p,
        r#"{"op": "set_region", "track": 1, "region": 0, "flex": "speed"}"#,
    );
    assert!(err.unwrap_err().contains("audio"));
    std::fs::remove_dir_all(base).unwrap();
}

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
fn every_command_is_described() {
    let samples = [
        "set_tempo",
        "set_time_signature",
        "set_master",
        "add_track",
        "delete_track",
        "set_track",
        "set_instrument",
        "set_insert",
        "add_midi_region",
        "set_notes",
        "add_pattern_region",
        "set_pattern",
        "add_audio_region",
        "move_region",
        "trim_region",
        "split_region",
        "delete_region",
        "flex_region",
        "set_cell",
        "comp",
        "flatten",
        "add_send",
        "set_send",
        "set_output",
        "add_stack",
        "flatten_stack",
        "remove_send",
        "set_aux",
        "set_automation",
        "shape_automation",
        "smooth_automation",
        "set_palette",
        "add_marker",
        "set_marker",
        "delete_marker",
        "set_key",
        "set_cycle",
        "set_eq",
        "set_eq_band",
        "set_comp",
        "set_saturator",
        "edit_notes",
        "set_region",
        "add_alias",
        "unlink",
        "set_input",
        "set_automation_mode",
        "set_plugin_param",
        "set_controls",
        "set_preset",
        "set_sidechain",
        "set_chords",
        "write_chords",
        "write_bass",
        "write_melody",
        "write_drums",
        "set_genre",
        "arrange",
        "copy_bars",
        "transition",
        "set_reference",
        "add_modulator",
        "share_modulator",
        "unshare_modulator",
        "set_modulator",
        "remove_modulator",
        "modulate",
    ];
    for op in samples {
        assert!(
            DESCRIBE.contains(&format!("`{op}`")),
            "{op} isn't in commands.md"
        );
    }
    // And the list above is complete: an unknown op names every real one.
    let err = serde_json::from_str::<Command>(r#"{"op": "nope"}"#)
        .unwrap_err()
        .to_string();
    for op in samples {
        assert!(err.contains(op), "{op} isn't a command");
    }
    assert_eq!(err.matches('`').count() / 2, samples.len() + 1); // + the unknown one
}
