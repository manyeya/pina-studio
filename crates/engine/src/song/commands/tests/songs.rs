//! A song built by commands as an agent would, mistakes, every command described, audio copied in.

use super::*;

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
