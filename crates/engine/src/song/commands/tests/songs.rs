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
fn a_genre_is_any_word_and_names_a_playbook_only_if_the_person_has_one() {
    let mut p = Project::new(48_000);
    run(&mut p, r#"{"op": "set_genre", "genre": "gqom"}"#).unwrap();
    assert_eq!(p.genre.as_deref(), Some("gqom"));
    // One of theirs (the tests' own), by its name: its id.
    run(&mut p, r#"{"op": "set_genre", "genre": "Lo-fi hip-hop"}"#).unwrap();
    assert_eq!(p.genre.as_deref(), Some("lofi"));
    run(&mut p, r#"{"op": "set_genre"}"#).unwrap();
    assert_eq!(p.genre, None);
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
    // Sliced, and following the tempo: its own tempo is the song's then, 120, so at 60 it
    // plays twice as long, still a beat.
    run(
        &mut p,
        r#"{"op": "set_region", "track": 0, "region": 0, "stretch_mode": "slicing", "follow": true}"#,
    )
    .unwrap();
    run(&mut p, r#"{"op": "set_tempo", "bpm": 60}"#).unwrap();
    let r = p.region((0, 0)).clone();
    assert!(matches!(
        r.content,
        Content::Audio {
            stretch_mode: StretchMode::Slicing,
            follow: crate::song::project::Follow::On,
            tempo: Some(120.0),
            ..
        }
    ));
    assert_eq!(r.length, PPQ);
    let half = p.frame_at(&r.content, PPQ as f64, PPQ as f64 * 1.5);
    assert_eq!(half, Some(12_000));
    // Locked to its beats, at its own tempo of 90, by name.
    run(
        &mut p,
        r#"{"op": "set_region", "track": 0, "region": 0, "follow": "beats", "tempo": 90}"#,
    )
    .unwrap();
    assert!(matches!(
        p.region((0, 0)).content,
        Content::Audio {
            follow: crate::song::project::Follow::Beats,
            tempo: Some(90.0),
            ..
        }
    ));
    // A tempo change, and where it is, and gone again.
    run(&mut p, r#"{"op": "set_tempo_at", "tick": 3840, "bpm": 90}"#).unwrap();
    assert_eq!(p.tempo_at(4000), 90.0);
    run(&mut p, r#"{"op": "delete_tempo", "tick": 3840}"#).unwrap();
    assert!(p.tempos.is_empty());
    // MIDI has no stretch mode.
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
        "set_tempo_at",
        "delete_tempo",
        "set_tempo_mode",
        "add_kit",
        "split_kit",
        "import_midi",
        "move_section",
        "copy_section",
        "swap_sections",
        "delete_section",
        "clear_section",
        "add_section",
        "insert_time",
        "remove_time",
        "analyze_tempo",
        "adapt_tempo",
        "fit_tempo",
        "set_time_signature",
        "set_master",
        "add_track",
        "delete_track",
        "set_track",
        "set_instrument",
        "set_insert",
        "move_effect",
        "add_midi_region",
        "set_notes",
        "add_pattern_region",
        "set_pattern",
        "add_audio_region",
        "move_region",
        "trim_region",
        "split_region",
        "delete_region",
        "stretch_region",
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
        "set_synth",
        "set_reverb",
        "set_delay",
        "set_fx",
        "set_limiter",
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
        "set_genre",
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
    // + the unknown one, and flex_region (stretch_region's old name).
    assert_eq!(err.matches('`').count() / 2, samples.len() + 2);
}

#[test]
fn a_kit_comes_as_a_stack_and_splits_onto_its_pads() {
    let dir = std::env::temp_dir().join(format!("pina-kit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    for name in ["Kick 01", "Snare Tight", "Open Hat", "Closed Hat"] {
        let mut w = hound::WavWriter::create(dir.join(format!("{name}.wav")), spec).unwrap();
        (0..2400).for_each(|_| w.write_sample(1000i16).unwrap());
        w.finalize().unwrap();
    }
    let mut p = Project::new(48_000);
    let pads: Vec<Value> = ["Kick 01", "Snare Tight", "Open Hat", "Closed Hat"]
        .iter()
        .map(|n| json!({ "file": dir.join(format!("{n}.wav")), "steps": [0, 8] }))
        .collect();
    let cmd = json!({ "op": "add_kit", "name": "Trap kit", "pads": pads });
    let made = apply(&mut p, &dir, serde_json::from_value(cmd).unwrap()).unwrap();
    assert_eq!(made["kit"], 0);
    assert_eq!(made["pads"], json!([1, 2, 3, 4]));
    // Each pad on its name's drum note, the hats choking each other, its sound copied in.
    let pads: Vec<_> = (1..5).map(|t| p.tracks[t].pad.unwrap()).collect();
    assert_eq!(
        pads.iter().map(|d| (d.note, d.choke)).collect::<Vec<_>>(),
        [(36, None), (38, None), (46, Some(1)), (42, Some(1))]
    );
    assert!(dir.join("Audio Files/Kick 01.wav").exists());
    let split = apply(
        &mut p,
        &dir,
        serde_json::from_value(json!({ "op": "split_kit", "track": 0 })).unwrap(),
    )
    .unwrap();
    assert_eq!(split["regions"], 4);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_track_takes_the_synth_by_patch_and_settings() {
    use crate::song::project::{
        FilterField, FilterRouting, Instrument, Param, PolyFilter, Warp, filter_param,
    };
    let mut p = Project::new(48_000);
    run(&mut p, r#"{"op": "add_track", "name": "Bass"}"#).unwrap();
    let s = |p: &Project| match &p.tracks[0].instrument {
        Some(Instrument::Poly(s)) => (**s).clone(),
        i => panic!("{i:?}"),
    };
    run(
        &mut p,
        r#"{"op": "set_synth", "track": 0, "patch": "Reese"}"#,
    )
    .unwrap();
    assert_eq!(s(&p).patch, "Reese");
    // Only what's given changes.
    run(&mut p, r#"{"op": "set_synth", "track": 0, "settings": {"params": {"filter1_cutoff": 900}, "filters": ["diode", "none"], "routing": "parallel", "warps": [["sync", "none"], ["none", "none"], ["none", "none"]]}}"#).unwrap();
    let now = s(&p);
    assert_eq!(now.patch, "Reese");
    assert_eq!(now.params[filter_param(0, FilterField::Cutoff)], 900.0);
    assert_eq!(now.filters[0], PolyFilter::Diode);
    assert_eq!(now.routing, FilterRouting::Parallel);
    assert_eq!(now.warps[0][0], Warp::Sync);
    assert_eq!(
        now.tables,
        crate::song::project::poly_patch("Reese").unwrap().tables
    );
    // Its parameters automated in their own units, by id or by name.
    run(&mut p, r#"{"op": "set_automation", "track": 0, "param": "synth", "knob": "filter1_cutoff", "points": [{"tick": 0, "value": 632.5}]}"#).unwrap();
    let i = filter_param(0, FilterField::Cutoff) as u16;
    let points = p.tracks[0].automation.points(Param::Poly(i));
    assert!((points[0].value - 0.5).abs() < 1e-3, "{points:?}");
    let e = run(
        &mut p,
        r#"{"op": "set_automation", "track": 0, "param": "synth", "knob": "wobble", "points": []}"#,
    )
    .unwrap_err();
    assert!(e.contains("wobble"), "{e}");
    // Bad patch names say what there is.
    let e = run(
        &mut p,
        r#"{"op": "set_synth", "track": 0, "patch": "Nope"}"#,
    )
    .unwrap_err();
    assert!(e.contains("Supersaw") && e.contains("Drone"), "{e}");
}

#[test]
fn a_loop_says_how_far_it_runs() {
    let mut p = Project::new(48_000);
    run(
        &mut p,
        r#"{"op": "add_track", "name": "Keys", "instrument": "synth"}"#,
    )
    .unwrap();
    run(
        &mut p,
        r#"{"op": "add_midi_region", "track": 0, "start": 0, "length": 15360, "notes": []}"#,
    )
    .unwrap();
    let alone = run(
        &mut p,
        r#"{"op": "set_region", "track": 0, "region": 0, "loop": true}"#,
    )
    .unwrap();
    assert_eq!(alone["loops_to"], 15360);
    assert!(
        alone["note"].as_str().unwrap().contains("loop_length"),
        "{alone}"
    );
    let far = run(
        &mut p,
        r#"{"op": "set_region", "track": 0, "region": 0, "loop_length": 122880}"#,
    )
    .unwrap();
    assert_eq!(far, json!({"loops_to": 122880, "bars": "1-32"}));
}

#[test]
fn a_fill_dropped_in_a_loop_keeps_the_loop_going() {
    let mut p = Project::new(48_000);
    run(
        &mut p,
        r#"{"op": "add_track", "name": "Keys", "instrument": "synth"}"#,
    )
    .unwrap();
    run(
        &mut p,
        r#"{"op": "add_midi_region", "track": 0, "start": 0, "length": 15360,
            "notes": [{"start": 0, "length": 240, "pitch": 60, "velocity": 100},
                      {"start": 7680, "length": 240, "pitch": 64, "velocity": 100}]}"#,
    )
    .unwrap();
    run(
        &mut p,
        r#"{"op": "set_region", "track": 0, "region": 0, "loop_length": 122880}"#,
    )
    .unwrap();
    run(
        &mut p,
        r#"{"op": "add_midi_region", "track": 0, "start": 61440, "length": 3840,
            "notes": [{"start": 0, "length": 240, "pitch": 72, "velocity": 100}]}"#,
    )
    .unwrap();
    let heard: Vec<(i64, u8)> = crate::music::compose::heard_notes(&p, 0)
        .iter()
        .map(|n| (n.start, n.pitch))
        .collect();
    // The fill in place of its bar, the loop's second half straight after it, then whole loops.
    assert!(heard.contains(&(61440, 72)), "{heard:?}");
    assert!(!heard.contains(&(61440, 60)), "{heard:?}");
    assert!(heard.contains(&(69120, 64)), "{heard:?}");
    assert!(heard.contains(&(76800, 60)), "{heard:?}");
    assert!(heard.contains(&(115200, 64)), "{heard:?}");
    assert_eq!(p.end(), 122880);
}

/// A 4-bar region of notes, on a new track.
fn four_bars(p: &mut Project) {
    run(
        p,
        r#"{"op": "add_track", "name": "Keys", "instrument": "synth"}"#,
    )
    .unwrap();
    run(
        p,
        r#"{"op": "add_midi_region", "track": 0, "start": 0, "length": 15360,
            "notes": [{"start": 0, "length": 960, "pitch": 60, "velocity": 100},
                      {"start": 11520, "length": 960, "pitch": 64, "velocity": 100}]}"#,
    )
    .unwrap();
}

#[test]
fn trimming_a_loop_out_runs_its_loops_and_keeps_its_notes_going() {
    let mut p = Project::new(48_000);
    four_bars(&mut p);
    run(
        &mut p,
        r#"{"op": "set_region", "track": 0, "region": 0, "loop": true}"#,
    )
    .unwrap();
    let out = run(
        &mut p,
        r#"{"op": "trim_region", "track": 0, "region": 0, "end": 122880}"#,
    )
    .unwrap();
    assert_eq!(out, json!({"loops_to": 122880, "bars": "1-32"}));
    let r = &p.tracks[0].regions[0];
    assert_eq!((r.length, r.loop_length), (15360, Some(122880)));
    assert_eq!(p.end(), 122880);
    // Notes in the last pass: bar 29 and its last bar.
    let heard: Vec<i64> = crate::music::compose::heard_notes(&p, 0)
        .iter()
        .map(|n| n.start)
        .collect();
    assert!(
        heard.contains(&107520) && heard.contains(&119040),
        "{heard:?}"
    );
    // Trimmed back in, they stop sooner.
    run(
        &mut p,
        r#"{"op": "trim_region", "track": 0, "region": 0, "end": 61440}"#,
    )
    .unwrap();
    assert_eq!(p.loop_end((0, 0)), 61440);
}

#[test]
fn how_far_a_loop_runs_and_one_pass_given_the_wrong_way_round_are_put_right() {
    let mut p = Project::new(48_000);
    four_bars(&mut p);
    run(
        &mut p,
        r#"{"op": "trim_region", "track": 0, "region": 0, "end": 61440}"#,
    )
    .unwrap();
    let out = run(
        &mut p,
        r#"{"op": "set_region", "track": 0, "region": 0, "loop_length": 15360}"#,
    )
    .unwrap();
    assert_eq!(out, json!({"loops_to": 61440, "bars": "1-16"}));
    assert_eq!(p.tracks[0].regions[0].length, 15360);
}

#[test]
fn a_pass_that_ends_in_silence_says_so() {
    let mut p = Project::new(48_000);
    run(
        &mut p,
        r#"{"op": "add_track", "name": "Keys", "instrument": "synth"}"#,
    )
    .unwrap();
    run(
        &mut p,
        r#"{"op": "add_midi_region", "track": 0, "start": 0, "length": 15360,
            "notes": [{"start": 0, "length": 960, "pitch": 60, "velocity": 100}]}"#,
    )
    .unwrap();
    let out = run(
        &mut p,
        r#"{"op": "set_region", "track": 0, "region": 0, "loop_length": 122880}"#,
    )
    .unwrap();
    assert!(
        out["note"].as_str().unwrap().contains("ends in silence"),
        "{out}"
    );
}
