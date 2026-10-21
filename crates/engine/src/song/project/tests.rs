use super::*;

#[test]
fn a_patterns_lanes_and_the_modulators_on_its_sounds() {
    use crate::dsp::modulate::{
        LfoShape, ModKind, ModRate, ModRoute, ModTarget, Modulator, SoundParam,
    };
    let step = PPQ / 4;
    let row = |cells: Vec<u8>| PatternRow {
        pitch: 36,
        cells,
        ..Default::default()
    };
    // A ratchet of three, a fifth up, held half its time, a quarter of a step late.
    let mut r = row(vec![100, 0, 0, 0]);
    (r.repeat, r.transpose, r.gate, r.nudge) = (vec![3], vec![7], vec![50], vec![25]);
    let notes = pattern_notes(step, &[r], step * 4, 50.0);
    let starts: Vec<i64> = notes.iter().map(|n| n.start).collect();
    let sub = (step / 3) as i64;
    let late = (step / 4) as i64;
    assert_eq!(starts, [late, sub + late, 2 * sub + late]);
    assert!(
        notes
            .iter()
            .all(|n| n.pitch == 43 && n.length == (sub as u64) / 2)
    );
    // Half a chance: some steps of a long run play, some don't, the same each time.
    let mut r = row(vec![100; 64]);
    r.chance = vec![50; 64];
    let played = pattern_notes(step, std::slice::from_ref(&r), step * 64, 50.0);
    assert!((20..44).contains(&played.len()), "{}", played.len());
    assert_eq!(played, pattern_notes(step, &[r], step * 64, 50.0));
    // A square LFO a beat long on the sound's velocity: loud for the first half of each beat.
    let lfo = Modulator {
        kind: ModKind::Lfo {
            shape: LfoShape::Square,
            rate: ModRate::Beats(1.0),
            phase: 0.0,
            unipolar: false,
        },
        routes: vec![ModRoute {
            target: ModTarget::Sound(0, SoundParam::Velocity),
            depth: -40.0,
        }],
    };
    let notes = pattern_played(step, &[row(vec![100; 4])], step * 4, 50.0, 0, &[lfo], 120.0);
    let v: Vec<u8> = notes.iter().map(|n| n.velocity).collect();
    assert_eq!(v, [60, 60, 127, 127]);
}

fn midi(start: u64, length: u64, notes: &[(i64, u8)]) -> Region {
    let notes = notes
        .iter()
        .map(|&(start, pitch)| Note {
            start,
            length: 240,
            pitch,
            velocity: 100,
        })
        .collect();
    Region {
        start,
        length,
        content: Content::Midi {
            notes,
            controls: Vec::new(),
        },
        ..Default::default()
    }
}

#[test]
fn audio_that_follows_the_tempo_keeps_its_beats() {
    let mut p = Project::new(48_000);
    p.tempo = 100.0;
    // A loop at 90 stretched to 100 BPM, following; a recording that doesn't.
    let mut follows = audio(0, PPQ * 4, 0);
    if let Content::Audio {
        stretch, follow, ..
    } = &mut follows.content
    {
        (*stretch, *follow) = (0.9, true);
    }
    p.tracks.push(Track {
        regions: vec![follows, audio(0, PPQ * 4, 0)],
        ..Default::default()
    });
    let stretch = |p: &Project, r: usize| match p.tracks[0].regions[r].content {
        Content::Audio { stretch, .. } => stretch,
        _ => unreachable!(),
    };
    if let Content::Audio { warps, .. } = &mut p.tracks[0].regions[0].content {
        warps.push([1000, 900]);
    }
    p.set_tempo(120.0);
    assert_eq!((stretch(&p, 0), stretch(&p, 1)), (0.75, 1.0));
    // Its warp markers move with it: a beat still lands on its beat.
    let warp = match &p.tracks[0].regions[0].content {
        Content::Audio { warps, .. } => warps[0],
        _ => unreachable!(),
    };
    assert_eq!(warp, [1000, 750]);
    assert_eq!(p.tracks[0].regions[0].length, PPQ * 4);
    // Back where it was, exactly.
    p.set_tempo(100.0);
    assert_eq!(stretch(&p, 0), 0.9);
}

fn audio(start: u64, length: u64, offset: u64) -> Region {
    Region {
        start,
        length,
        content: Content::Audio {
            file: "a.wav".into(),
            offset,
            stretch: 1.0,
            flex: Flex::default(),
            follow: false,
            formants: false,
            warps: Vec::new(),
        },
        ..Default::default()
    }
}

fn project(regions: Vec<Region>, instrument: bool) -> Project {
    let track = Track {
        name: "t".into(),
        instrument: instrument.then_some(Instrument::Synth),
        regions,
        ..Default::default()
    };
    Project {
        tracks: vec![track],
        ..Project::new(48_000)
    }
}

#[test]
fn json_round_trips_and_reads_sparse_tracks() {
    let json = r#"{
        "tempo": 120, "sample_rate": 48000,
        "tracks": [
            { "name": "Keys", "instrument": "synth",
              "regions": [{ "start": 0, "length": 3840,
                            "midi": { "notes": [{ "start": 0, "length": 480, "pitch": 60, "velocity": 100 }] } }] },
            { "name": "Drums", "volume_db": -3,
              "regions": [{ "start": 960, "length": 1920, "audio": { "file": "Audio Files/drums.wav" } }] }
        ]
    }"#;
    let p: Project = serde_json::from_str(json).unwrap();
    assert_eq!(p.tracks[0].instrument, Some(Instrument::Synth));
    assert_eq!(p.tracks[1].instrument, None);
    assert!(matches!(
        p.tracks[1].regions[0].content,
        Content::Audio { offset: 0, .. }
    ));
    assert_eq!(p.end(), 3840);
    let again: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(p, again);
}

#[test]
fn a_follower_follows_its_track_after_the_song_is_saved_and_read() {
    let mut p: Project = serde_json::from_str(
        r#"{"tempo": 120, "sample_rate": 48000,
            "tracks": [{"name": "Kick"}, {"name": "Bass"}]}"#,
    )
    .unwrap();
    for _ in 0..5 {
        next_id();
    }
    p.assign_ids();
    let kick = p.tracks[0].id;
    p.tracks[1].modulators.push(Modulator {
        kind: ModKind::follower(kick),
        routes: vec![],
    });
    let saved = serde_json::to_string(&p.portable()).unwrap();
    let mut read: Project = serde_json::from_str(&saved).unwrap();
    read.assign_ids();
    read.resolve_sources();
    assert_eq!(
        read.tracks[1].modulators[0].kind,
        ModKind::follower(read.tracks[0].id)
    );
}

#[test]
fn a_stack_gathers_its_tracks_and_flattening_it_lets_them_go() {
    let mut p: Project = serde_json::from_str(
        r#"{"tempo": 120, "sample_rate": 48000, "tracks": [
            {"name": "Kick"}, {"name": "Bass"}, {"name": "Snare"}, {"name": "Pad"}, {"name": "Hats"}]}"#,
    )
    .unwrap();
    p.assign_ids();
    let (bus, moved) = p.add_stack(&[0, 2, 4]);
    let names: Vec<&str> = p.tracks.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Kick", "Snare", "Hats", "Bass", "Pad"]);
    assert_eq!(moved, [0, 3, 1, 4, 2]);
    p.add_shared(bus, ModKind::mac());
    p.flatten_stack(bus);
    assert!(p.auxes.is_empty());
    assert!(
        p.tracks
            .iter()
            .all(|t| t.output.is_none() && t.modulators.is_empty())
    );
}

#[test]
fn a_sidechain_names_its_track_wherever_the_song_goes() {
    // Ids are a session's own: the drums' here aren't the drums' after it's saved and read
    // elsewhere, so a sidechain is saved as the track's number.
    let mut p: Project = serde_json::from_str(
        r#"{"tempo": 120, "sample_rate": 48000,
            "tracks": [{"name": "Drums"}, {"name": "Bass"}, {"name": "Pad"}]}"#,
    )
    .unwrap();
    for _ in 0..5 {
        next_id(); // as an app that's opened other songs has used ids up
    }
    p.assign_ids();
    let drums = p.tracks[0].id;
    p.tracks[2].eq = Some(Eq {
        sidechain: Some(drums),
        ..Eq::default()
    });
    let saved = serde_json::to_string(&p.portable()).unwrap();
    let mut read: Project = serde_json::from_str(&saved).unwrap();
    read.assign_ids();
    read.resolve_sources();
    assert_ne!(read.tracks[0].id, drums);
    assert_eq!(
        read.tracks[2].eq.as_ref().unwrap().sidechain,
        Some(read.tracks[0].id)
    );
    // A command names it by its number, as commands do every track.
    let dir = std::env::temp_dir();
    let set: crate::song::commands::Command = serde_json::from_value(serde_json::json!({
        "op": "set_eq", "channel": { "track": 1 }, "eq": { "sidechain": 0, "bands": [] }
    }))
    .unwrap();
    crate::song::commands::apply(&mut read, &dir, set).unwrap();
    assert_eq!(
        read.tracks[1].eq.as_ref().unwrap().sidechain,
        Some(read.tracks[0].id)
    );
}

#[test]
fn ticks_and_samples() {
    let p = Project::new(48_000);
    assert_eq!(p.samples(PPQ, 48_000.0), 24_000); // a beat at 120 bpm is half a second
    assert_eq!(p.ticks(24_000), PPQ);
}

#[test]
fn splitting_midi_keeps_notes_with_the_half_they_start_in() {
    let mut p = project(
        vec![midi(960, 1920, &[(0, 60), (900, 62), (960, 64)])],
        true,
    );
    assert_eq!(p.split_region((0, 0), 960), None); // at the region's start: nothing to split
    let at = p.split_region((0, 0), 1920).unwrap();
    let (a, b) = (p.region((0, 0)), p.region(at));
    assert_eq!(
        (a.start, a.length, b.start, b.length),
        (960, 960, 1920, 960)
    );
    let pitches = |r: &Region| match &r.content {
        Content::Midi { notes, .. } => notes.iter().map(|n| (n.start, n.pitch)).collect::<Vec<_>>(),
        _ => unreachable!(),
    };
    assert_eq!(pitches(a), [(0, 60), (900, 62)]);
    assert_eq!(pitches(b), [(0, 64)]);
}

#[test]
fn splitting_midi_keeps_each_controller_where_it_stood() {
    let cc = |start, value| ControlPoint {
        start,
        kind: ControlValue::Cc { number: 1, value },
    };
    let bend = |start, value| ControlPoint {
        start,
        kind: ControlValue::PitchBend { value },
    };
    let mut p = Project::new(48_000);
    p.tracks.push(Track {
        regions: vec![Region {
            start: 0,
            length: 4 * PPQ,
            content: Content::Midi {
                notes: vec![],
                controls: vec![
                    cc(0, 10),
                    bend(100, 500),
                    cc(PPQ as i64, 90),
                    cc(3 * PPQ as i64, 127),
                ],
            },
            ..Default::default()
        }],
        ..Default::default()
    });
    let second = p.split_region((0, 0), 2 * PPQ).unwrap();
    let controls = |at| match &p.region(at).content {
        Content::Midi { controls, .. } => controls.clone(),
        _ => unreachable!(),
    };
    assert_eq!(
        controls((0, 0)),
        [cc(0, 10), bend(100, 500), cc(PPQ as i64, 90)]
    );
    // The second half starts with the wheel at 90 and the bend where it was.
    let mut start = controls(second);
    start.sort_by_key(|c| (c.start, matches!(c.kind, ControlValue::PitchBend { .. })));
    assert_eq!(start, [cc(0, 90), bend(0, 500), cc(PPQ as i64, 127)]);
}

#[test]
fn splitting_audio_moves_the_offset() {
    let mut p = project(vec![audio(0, 1920, 100)], false);
    let at = p.split_region((0, 0), 960).unwrap();
    assert!(matches!(
        p.region(at).content,
        Content::Audio { offset: 24_100, .. }
    ));
}

#[test]
fn trimming_the_start_keeps_content_in_place() {
    let mut p = project(vec![midi(960, 1920, &[(0, 60), (480, 62)])], true);
    p.trim_start((0, 0), 1440);
    let r = p.region((0, 0));
    assert_eq!((r.start, r.length), (1440, 1440));
    let Content::Midi { notes, .. } = &r.content else {
        unreachable!()
    };
    assert_eq!((notes[0].start, notes[1].start), (-480, 0));
    p.trim_start((0, 0), 960); // back again: the hidden note returns
    let Content::Midi { notes, .. } = &p.region((0, 0)).content else {
        unreachable!()
    };
    assert_eq!(notes[0].start, 0);

    // Audio can't start before its file.
    let mut p = project(vec![audio(1920, 960, 12_000)], false);
    p.trim_start((0, 0), 0);
    let r = p.region((0, 0));
    assert_eq!((r.start, r.length), (1440, 1440));
    assert!(matches!(r.content, Content::Audio { offset: 0, .. }));
}

#[test]
fn automation_bends_steps_and_takes_shapes() {
    let p = |tick, value| AutoPoint::new(tick, value);
    // Straight, bent slow-then-fast, and a step.
    let line = [p(0, 0.0), p(100, 1.0)];
    assert_eq!(value_at(&line, 50.0), Some(0.5));
    let slow = [
        AutoPoint {
            curve: 0.8,
            ..p(0, 0.0)
        },
        p(100, 1.0),
    ];
    let fast = [
        AutoPoint {
            curve: -0.8,
            ..p(0, 0.0)
        },
        p(100, 1.0),
    ];
    assert!(value_at(&slow, 50.0).unwrap() < 0.2);
    assert!(value_at(&fast, 50.0).unwrap() > 0.8);
    assert_eq!(value_at(&slow, 100.0), Some(1.0));
    let step = [
        AutoPoint {
            step: true,
            ..p(0, 0.0)
        },
        p(100, 1.0),
    ];
    assert_eq!(value_at(&step, 99.0), Some(0.0));
    assert_eq!(value_at(&step, 100.0), Some(1.0));

    // A sine over a bar, a cycle a beat, drawn into a line that was flat at -6 dB: it stays
    // -6 either side, and swings between -12 and 0 inside.
    let mut a = Automation {
        volume: vec![p(0, -6.0), p(PPQ * 8, -6.0)],
        ..Default::default()
    };
    a.shape(
        Param::Volume,
        AutoShape::Sine,
        PPQ * 2,
        PPQ * 6,
        PPQ,
        -12.0,
        0.0,
    );
    let v = |t: u64| value_at(&a.volume, t as f64).unwrap();
    assert_eq!(v(PPQ), -6.0);
    assert_eq!(v(PPQ * 7), -6.0);
    assert!((v(PPQ * 2 + PPQ / 4) - 0.0).abs() < 1e-4);
    assert!((v(PPQ * 2 + PPQ * 3 / 4) + 12.0).abs() < 1e-4);
    assert!(
        (v(PPQ * 2 + PPQ / 8) + 1.8).abs() < 0.8,
        "{}",
        v(PPQ * 2 + PPQ / 8)
    );
    // A square steps; mute is always steps.
    a.shape(Param::Mute, AutoShape::Square, 0, PPQ * 2, PPQ, 0.0, 1.0);
    assert_eq!(value_at(&a.mute, (PPQ / 2 - 1) as f64), Some(1.0));
    assert_eq!(value_at(&a.mute, (PPQ / 2) as f64), Some(0.0));
    assert!(a.mute.iter().all(|p| p.step));

    // Thinning takes out what changes nothing, and keeps the shape.
    let mut a = Automation {
        pan: (0..=100)
            .map(|i| p(i * 10, (i as f32 / 100.0).min(0.5)))
            .collect(),
        ..Default::default()
    };
    a.thin(Param::Pan, 0.002);
    assert_eq!(a.pan.len(), 3, "{:?}", a.pan);
    assert_eq!(value_at(&a.pan, 250.0), Some(0.25));

    // Lifted out and put down elsewhere: the same line there.
    let mut a = Automation {
        volume: vec![p(0, -20.0), p(PPQ * 4, 0.0)],
        pan: vec![p(PPQ, -1.0), p(PPQ * 3, 1.0)],
        ..Default::default()
    };
    let clip = a.take(PPQ, PPQ * 3);
    assert_eq!(clip.length, PPQ * 2);
    a.put(PPQ * 8, &clip);
    for t in [0, PPQ / 2, PPQ, PPQ * 3 / 2, PPQ * 2 - 1] {
        let (was, now) = (PPQ + t, PPQ * 8 + t);
        assert!(
            (value_at(&a.volume, was as f64).unwrap() - value_at(&a.volume, now as f64).unwrap())
                .abs()
                < 0.01
        );
        assert!(
            (value_at(&a.pan, was as f64).unwrap() - value_at(&a.pan, now as f64).unwrap()).abs()
                < 0.01
        );
    }
    // Trimmed: all of it up, never past the top.
    a.offset(Param::Volume, 10.0);
    assert_eq!(a.volume[0].value, -10.0);
    assert!(a.volume.iter().all(|p| p.value <= 6.0));
    // Erased: the line either side holds.
    a.erase(PPQ * 8, PPQ * 10);
    assert!(
        a.pan
            .iter()
            .all(|p| !(PPQ * 8 + 1..PPQ * 10).contains(&p.tick))
    );
}

#[test]
fn smoothing_rounds_corners_and_wobbles_off_and_leaves_its_ends() {
    let p = |tick, value| AutoPoint::new(tick, value);
    // An S eases out and in: slow at both ends, halfway at the middle.
    let s = [
        AutoPoint {
            ease: true,
            ..p(0, 0.0)
        },
        p(100, 1.0),
    ];
    assert!(value_at(&s, 10.0).unwrap() < 0.05);
    assert_eq!(value_at(&s, 50.0), Some(0.5));
    assert!(value_at(&s, 90.0).unwrap() > 0.95);

    // A pan recorded with a wobble every sixteenth over two bars, flat either side.
    let wobble: Vec<AutoPoint> = (0..=32)
        .map(|i| p(PPQ * 2 + i * PPQ / 4, if i % 2 == 0 { -0.2 } else { 0.2 }))
        .collect();
    let mut a = Automation {
        pan: [vec![p(0, -0.2)], wobble, vec![p(PPQ * 16, -0.2)]].concat(),
        ..Default::default()
    };
    let before = a.pan.clone();
    a.smooth(Param::Pan, PPQ * 2, PPQ * 10, PPQ / 2);
    let v = |a: &Automation, t: u64| value_at(&a.pan, t as f64).unwrap();
    // The wobble's gone (it swung 0.4), the ends are where they were, outside untouched.
    let swing = (PPQ * 3..PPQ * 9)
        .step_by(30)
        .map(|t| v(&a, t))
        .fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
    assert!(swing.1 - swing.0 < 0.05, "{swing:?}");
    assert_eq!(v(&a, PPQ * 2), -0.2);
    assert!((v(&a, PPQ * 10) - value_at(&before, (PPQ * 10) as f64).unwrap()).abs() < 1e-3);
    assert_eq!(
        v(&a, PPQ * 12),
        v(
            &Automation {
                pan: before.clone(),
                ..Default::default()
            },
            PPQ * 12
        )
    );
    assert!(a.pan.len() < before.len(), "{}", a.pan.len());

    // A sharp peak in the volume, rounded off: lower at the top, still rising to it; the
    // silence at its ends stays silence.
    let mut a = Automation {
        volume: vec![p(0, -144.0), p(PPQ * 4, 0.0), p(PPQ * 8, -144.0)],
        ..Default::default()
    };
    a.smooth(Param::Volume, 0, PPQ * 8, PPQ);
    let peak = value_at(&a.volume, (PPQ * 4) as f64).unwrap();
    assert!(peak < -0.5 && peak > -30.0, "{peak}");
    let rise = |t: u64| value_at(&a.volume, t as f64).unwrap();
    assert!(rise(PPQ * 2) < rise(PPQ * 3) && rise(PPQ * 3) < rise(PPQ * 4));
    assert_eq!(value_at(&a.volume, 0.0), Some(-144.0));
    assert_eq!(value_at(&a.volume, (PPQ * 8) as f64), Some(-144.0));
    // Mute's steps stay steps.
    let mut a = Automation {
        mute: vec![
            AutoPoint {
                step: true,
                ..p(0, 0.0)
            },
            AutoPoint {
                step: true,
                ..p(PPQ, 1.0)
            },
        ],
        ..Default::default()
    };
    let was = a.mute.clone();
    a.smooth(Param::Mute, 0, PPQ, PPQ);
    assert_eq!(a.mute, was);
}

#[test]
fn automation_points_stay_in_order_and_interpolate() {
    let mut a = Automation::default();
    assert_eq!(value_at(a.points(Param::Volume), 0.0), None);
    a.add(Param::Volume, AutoPoint::new(960, -12.0));
    a.add(Param::Volume, AutoPoint::new(0, 0.0));
    assert_eq!(a.add(Param::Volume, AutoPoint::new(480, -6.0)), 1);
    let v = a.points(Param::Volume);
    assert_eq!(value_at(v, 240.0), Some(-3.0));
    assert_eq!(value_at(v, 5000.0), Some(-12.0));
    a.set(Param::Volume, 1, 2000, 0.0); // can't pass the point after it
    assert_eq!(a.points(Param::Volume)[1].tick, 960);
    assert!(a.points(Param::Pan).is_empty());
}

fn take(start: i64, pitch: u8) -> Take {
    let notes = vec![Note {
        start: 0,
        length: 240,
        pitch,
        velocity: 100,
    }];
    Take {
        start,
        content: Content::Midi {
            notes,
            controls: Vec::new(),
        },
    }
}

fn folder(p: &Project, at: At) -> (Vec<i64>, Vec<(u64, u64, usize)>) {
    let r = p.region(at);
    let Content::Takes { takes, comp } = &r.content else {
        panic!("not a folder")
    };
    (
        takes.iter().map(|t| t.start).collect(),
        comp_segments(comp, takes.len(), r.length),
    )
}

#[test]
fn recording_over_a_region_makes_a_take_folder() {
    let mut p = project(vec![midi(960, 1920, &[(0, 60)])], true);
    let notes = vec![Note {
        start: 0,
        length: 240,
        pitch: 64,
        velocity: 90,
    }];
    let at = p.add_take(
        0,
        1440,
        3360,
        Content::Midi {
            notes,
            controls: Vec::new(),
        },
    );
    assert_eq!(at, (0, 0));
    let r = p.region(at);
    assert_eq!((r.start, r.length), (960, 2400)); // grown to the end of the new take
    assert_eq!(folder(&p, at), (vec![0, 480], vec![(0, 2400, 1)])); // the newest plays
    // Recording somewhere else makes a plain region.
    assert_eq!(
        p.add_take(
            0,
            7680,
            8640,
            Content::Midi {
                notes: vec![],
                controls: Vec::new()
            }
        ),
        (0, 1)
    );
}

#[test]
fn comping_overlays_ranges_and_merges_neighbours() {
    let takes = vec![take(0, 60), take(0, 62), take(0, 64)];
    let region = Region {
        start: 0,
        length: 3840,
        content: Content::Takes {
            takes,
            comp: vec![],
        },
        ..Default::default()
    };
    let mut p = project(vec![region], true);
    p.comp((0, 0), 960, 1920, 0).unwrap();
    assert_eq!(
        folder(&p, (0, 0)).1,
        [(0, 960, 2), (960, 1920, 0), (1920, 3840, 2)]
    );
    p.comp((0, 0), 1920, 2880, 0).unwrap();
    assert_eq!(
        folder(&p, (0, 0)).1,
        [(0, 960, 2), (960, 2880, 0), (2880, 3840, 2)]
    );
    p.comp((0, 0), 0, 3840, 1).unwrap();
    assert_eq!(folder(&p, (0, 0)).1, [(0, 3840, 1)]);
    assert!(p.comp((0, 0), 0, 960, 7).is_err());
}

#[test]
fn take_folders_split_trim_and_flatten() {
    // Take 1 starts at 480 with a note 600 in (at 1080), where the comp plays take 1.
    let mut late = take(480, 62);
    let Content::Midi { notes, .. } = &mut late.content else {
        unreachable!()
    };
    notes[0].start = 600;
    let takes = vec![take(0, 60), late];
    let comp = vec![
        CompPiece { from: 0, take: 0 },
        CompPiece { from: 960, take: 1 },
    ];
    let region = Region {
        start: 0,
        length: 1920,
        content: Content::Takes { takes, comp },
        ..Default::default()
    };
    let mut p = project(vec![region], true);
    let at = p.split_region((0, 0), 1440).unwrap();
    assert_eq!(folder(&p, (0, 0)).1, [(0, 960, 0), (960, 1440, 1)]);
    assert_eq!(folder(&p, at), (vec![-1440, -960], vec![(0, 480, 1)]));

    p.trim_start((0, 0), 480); // takes stay put on the timeline
    assert_eq!(
        folder(&p, (0, 0)),
        (vec![-480, 0], vec![(0, 480, 0), (480, 960, 1)])
    );

    // Flattening keeps what the comp plays: take 1's note (1080, now 600 after the trim), not
    // take 0's (now -480, before the region).
    let flat = p.flatten((0, 0));
    let Content::Midi { notes, .. } = &p.region(flat[0]).content else {
        panic!()
    };
    let kept: Vec<(i64, u8)> = notes.iter().map(|n| (n.start, n.pitch)).collect();
    assert_eq!(kept, [(600, 62)]);
}

#[test]
fn regions_only_move_to_tracks_that_can_play_them() {
    let mut p = project(vec![midi(0, 960, &[])], true);
    p.add_track(
        1,
        Track {
            name: "audio".into(),
            ..Default::default()
        },
    );
    p.add_track(
        2,
        Track {
            name: "keys".into(),
            instrument: Some(Instrument::Synth),
            ..Default::default()
        },
    );
    assert_eq!(p.move_region((0, 0), 1, 480), (0, 0));
    assert_eq!(p.move_region((0, 0), 2, 480), (2, 0));
    assert_eq!(p.region((2, 0)).start, 480);
}

#[test]
fn aliases_share_their_notes_until_unlinked() {
    let mut p = project(vec![midi(0, 960, &[(0, 60)])], true);
    let alias = p.alias((0, 0), 0, 3840).unwrap();
    assert!(p.region(alias).link.is_some());
    assert_eq!(p.region(alias).link, p.region((0, 0)).link);
    assert_eq!(p.region(alias).start, 3840);
    let mut links = p.link_contents();
    // Editing the alias edits the original, and the other way round.
    p.notes_mut(alias).unwrap()[0].pitch = 64;
    p.sync_links(&mut links);
    assert_eq!(p.notes_mut((0, 0)).unwrap()[0].pitch, 64);
    p.notes_mut((0, 0)).unwrap()[0].pitch = 67;
    p.sync_links(&mut links);
    assert_eq!(p.notes_mut(alias).unwrap()[0].pitch, 67);
    // Its place and playback settings are its own.
    p.tracks[0].regions[1].transpose = 12;
    p.sync_links(&mut links);
    assert_eq!(p.region((0, 0)).transpose, 0);
    // Splitting makes regions of their own; one left alone isn't an alias.
    p.split_region(alias, 3840 + 480).unwrap();
    p.sync_links(&mut links);
    assert!(p.tracks[0].regions.iter().all(|r| r.link.is_none()));
    // Audio has no aliases.
    let mut p = project(vec![audio(0, 960, 0)], false);
    assert!(p.alias((0, 0), 0, 960).is_err());
}

#[test]
fn recording_automation_replaces_the_line_it_passes() {
    let mut a = Automation::default();
    for tick in [0, 3840] {
        a.add(Param::Volume, AutoPoint::new(tick, -6.0));
    }
    a.write(Param::Volume, None, 960, 0.0);
    a.write(Param::Volume, Some(960), 1440, -3.0);
    a.write(Param::Volume, Some(1440), 1920, -3.0);
    a.write(Param::Volume, Some(1920), 2400, -3.0); // held still: the last point moves on
    let points: Vec<(u64, f32)> = a.volume.iter().map(|p| (p.tick, p.value)).collect();
    assert_eq!(
        points,
        [
            (0, -6.0),
            (959, -6.0), // the line as it was, up to where recording began
            (960, 0.0),
            (1440, -3.0),
            (2400, -3.0),
            (3840, -6.0)
        ]
    );
    // Plugin parameters record the same way, in lanes of their own.
    let cutoff = Param::Plugin(Which::Instrument, 7);
    a.write(cutoff, None, 0, 0.25);
    a.name(cutoff, "Cutoff");
    assert_eq!(a.points(cutoff).len(), 1);
    assert_eq!(a.plugins[0].name, "Cutoff");
    assert_eq!(a.params(), [Param::Volume, cutoff]);
}

#[test]
fn a_slots_automation_goes_with_its_plugin() {
    let plugin = |class: &str| PluginRef {
        path: "/p.vst3".into(),
        class: class.into(),
        ..Default::default()
    };
    let mut t = Track {
        inserts: vec![plugin("a"), plugin("b")],
        ..Default::default()
    };
    for i in 0..2 {
        t.automation.add(
            Param::Plugin(Which::Insert(i), 1),
            AutoPoint::new(0, i as f32),
        );
    }
    t.set_insert(0, None).unwrap();
    assert_eq!(t.automation.plugins.len(), 1);
    assert_eq!(t.automation.plugins[0].plugin, Which::Insert(0)); // b's, moved up
    assert_eq!(t.automation.plugins[0].points[0].value, 1.0);
    // The same plugin with other settings keeps it; another plugin doesn't.
    t.set_insert(0, Some(plugin("b"))).unwrap();
    assert_eq!(t.automation.plugins.len(), 1);
    t.set_insert(0, Some(plugin("c"))).unwrap();
    assert!(t.automation.plugins.is_empty());
}

#[test]
fn time_signatures_set_the_bar() {
    let mut p = Project::new(48_000);
    assert_eq!((p.beat(), p.bar()), (PPQ, 4 * PPQ));
    p.time_signature = [6, 8];
    assert_eq!(
        (p.beat(), p.bar(), p.beats_per_bar()),
        (PPQ / 2, 3 * PPQ, 6)
    );
    p.time_signature = [3, 4];
    assert_eq!(p.bar(), 3 * PPQ);
    let json = serde_json::to_string(&p).unwrap();
    assert!(json.contains("\"time_signature\":[3,4]"));
    assert!(
        !serde_json::to_string(&Project::new(48_000))
            .unwrap()
            .contains("time_signature")
    );
}

#[test]
fn changing_the_key_can_move_the_music_into_it() {
    let mut p = project(vec![midi(0, 960, &[(0, 60), (240, 64), (480, 67)])], true);
    p.add_track(
        1,
        Track {
            instrument: Some(Instrument::Drums),
            regions: vec![midi(0, 960, &[(0, 36)])],
            ..Default::default()
        },
    );
    let key = |s: &str| crate::music::Key::parse(s);
    let pitches = |p: &Project, t: usize| match &p.tracks[t].regions[0].content {
        Content::Midi { notes, .. } => notes.iter().map(|n| n.pitch).collect::<Vec<_>>(),
        _ => unreachable!(),
    };
    assert_eq!(p.song_key(), key("C major")); // a C major chord
    // Just naming it: nothing moves.
    let mut named = p.clone();
    named.set_key(key("D major"), false);
    assert_eq!(pitches(&named, 0), [60, 64, 67]);
    // Moving into D major: a D major chord; into C minor: C minor's. Drums stay.
    let mut d = p.clone();
    d.set_key(key("D major"), true);
    assert_eq!(
        (pitches(&d, 0), d.key.as_deref()),
        (vec![62, 66, 69], Some("D major"))
    );
    assert_eq!(pitches(&d, 1), [36]);
    p.set_key(key("C minor"), true);
    assert_eq!(pitches(&p, 0), [60, 63, 67]);
    // Back to working it out: nothing moves.
    p.set_key(None, true);
    assert_eq!((pitches(&p, 0), p.key.clone()), (vec![60, 63, 67], None));
}

#[test]
fn swing_makes_every_second_step_late() {
    let rows = [PatternRow {
        pitch: 42,
        cells: vec![100; 4],
        mute: false,
        ..Default::default()
    }];
    let starts = |swing| {
        pattern_notes(PPQ / 4, &rows, PPQ, swing)
            .iter()
            .map(|n| n.start)
            .collect::<Vec<_>>()
    };
    assert_eq!(starts(50.0), [0, 240, 480, 720]);
    // 66%: the second of each pair two thirds of the way through it, a triplet feel.
    assert_eq!(starts(66.0), [0, 316, 480, 796]);
}

#[test]
fn rows_loop_on_their_own_lengths_muted_rows_rest_and_midi_becomes_a_pattern() {
    let step = PPQ / 4;
    let row = |pitch: u8, cells: Vec<u8>, mute: bool| PatternRow {
        pitch,
        cells,
        mute,
        ..Default::default()
    };
    let rows = vec![
        row(36, vec![100, 0, 0, 0], false),
        row(42, vec![80, 0, 0], false),
        row(38, vec![100; 4], true),
    ];
    let notes = pattern_notes(step, &rows, step * 8, 50.0);
    let at = |p: u8| {
        notes
            .iter()
            .filter(|n| n.pitch == p)
            .map(|n| n.start)
            .collect::<Vec<_>>()
    };
    let s = step as i64;
    assert_eq!(at(36), [0, 4 * s]);
    assert_eq!(at(42), [0, 3 * s, 6 * s]);
    assert!(at(38).is_empty());
    // A beat of MIDI: kick on 1 and 3 (a little late), a hat between; highest row first.
    let n = |start: i64, pitch: u8, velocity: u8| Note {
        start,
        length: 100,
        pitch,
        velocity,
    };
    let rows = to_pattern(
        &[n(0, 36, 110), n(2 * s + 20, 36, 90), n(s, 42, 70)],
        step,
        4,
    );
    assert_eq!(
        rows.iter()
            .map(|r| (r.pitch, r.cells.clone()))
            .collect::<Vec<_>>(),
        [(42, vec![0, 70, 0, 0]), (36, vec![110, 0, 90, 0])]
    );
}
