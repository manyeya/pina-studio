//! Notes and controllers: when they start and stop, patterns, takes, loops, cells and edits while playing.

use super::*;

#[test]
fn notes_start_on_their_beat_and_play_past_the_region_end() {
    // A beat-long note from beat 2 in a region that ends halfway through it, and a note
    // starting after the region ends, which doesn't play.
    let notes = vec![note(PPQ as i64, PPQ, 69), note(PPQ as i64 * 3, PPQ, 69)];
    let p = project(vec![synth_track("a", notes, PPQ * 3 / 2)]);
    let [l, _] = render(&p, Path::new("."), BEAT * 4);
    assert_eq!(peak(&l[..BEAT + 1]), 0.0);
    assert!(peak(&l[BEAT..BEAT + 1000]) > 0.1);
    assert!(peak(&l[BEAT * 2 - 1000..BEAT * 2]) > 0.1);
    assert_eq!(peak(&l[BEAT * 2 + RELEASE + 10..]), 0.0);
}

#[test]
fn a_pattern_loops_over_its_region() {
    use crate::song::project::PatternRow;
    // Kick on steps 1 and 3 of four sixteenths, over a beat and a half: it comes round again.
    let rows = vec![PatternRow {
        pitch: 36,
        cells: vec![100, 0, 90, 0],
        mute: false,
        ..Default::default()
    }];
    let region = Region {
        start: 0,
        length: PPQ * 3 / 2,
        content: Content::Pattern(crate::song::project::Pattern::new(PPQ / 4, rows, 50.0)),
        ..Default::default()
    };
    let track = Track {
        name: "d".into(),
        instrument: Some(Instrument::Drums),
        regions: vec![region],
        ..Default::default()
    };
    let mut files = AudioFiles::new(Path::new("."), RATE);
    let (arrangement, _) = compile(
        &project(vec![track.clone()]),
        RATE,
        &[(0, 0)],
        &[],
        None,
        &mut files,
    );
    let ons: Vec<u64> = arrangement.plans[0]
        .events
        .iter()
        .filter(|e| e.velocity > 0.0)
        .map(|e| e.at)
        .collect();
    let sixteenth = BEAT as u64 / 4;
    assert_eq!(ons, [0, 2 * sixteenth, 4 * sixteenth]);
    assert!(peak(&render(&project(vec![track]), Path::new("."), BEAT)[0]) > 0.1); // the kit plays it
}

#[test]
fn a_patterns_automation_rows_move_the_track_and_send_controllers() {
    use crate::song::project::{Param, Pattern, PatternRow, RowTarget, value_at};
    // A volume row: quiet on the first step, loud on the third (held between); and the mod
    // wheel on the second step.
    let mut volume = PatternRow::automation(RowTarget::Volume, 4, 0.0);
    volume.cells = vec![100, 0, 100, 0];
    volume.value = vec![
        Param::Volume.to_unit(-24.0),
        0.5,
        Param::Volume.to_unit(0.0),
        0.5,
    ];
    let mut wheel = PatternRow::automation(RowTarget::Cc(1), 4, 0.0);
    wheel.cells = vec![0, 100, 0, 0];
    wheel.value = vec![0.0, 0.75, 0.0, 0.0];
    let region = Region {
        start: 0,
        length: PPQ,
        content: Content::Pattern(Pattern::new(PPQ / 4, vec![volume, wheel], 50.0)),
        ..Default::default()
    };
    let track = Track {
        name: "d".into(),
        instrument: Some(Instrument::Drums),
        regions: vec![region],
        ..Default::default()
    };
    let p = project(vec![track]);
    let mut files = AudioFiles::new(Path::new("."), RATE);
    let (arrangement, _) = compile(&p, RATE, &[(0, 0)], &[], None, &mut files);
    let plan = &arrangement.plans[0];
    let sixteenth = BEAT as f64 / 4.0;
    let db = |at: f64| value_at(&plan.volume, at).unwrap();
    assert!(
        (db(sixteenth * 0.5) + 24.0).abs() < 0.5,
        "{}",
        db(sixteenth * 0.5)
    );
    assert!((db(sixteenth * 1.5) + 24.0).abs() < 0.5); // held to the next step that's on
    assert!(db(sixteenth * 2.5).abs() < 0.5);
    let wheel: Vec<(u64, f32)> = plan
        .controls
        .iter()
        .filter_map(|c| match c.kind {
            crate::plugin::ControlKind::Cc { number: 1, value } => Some((c.at, value)),
            _ => None,
        })
        .collect();
    assert_eq!(wheel.len(), 1);
    assert_eq!(wheel[0].0, BEAT as u64 / 4);
    assert!((wheel[0].1 - 0.75).abs() < 1e-6);
}

#[test]
fn a_take_folder_plays_its_comp() {
    use crate::song::project::{CompPiece, Take};
    // Two takes, an octave apart; the comp plays the first for a beat, then the second.
    let take = |pitch| Take {
        start: 0,
        content: Content::Midi {
            notes: vec![note(0, PPQ / 2, pitch), note(PPQ as i64, PPQ / 2, pitch)],
            controls: Vec::new(),
        },
    };
    let comp = vec![
        CompPiece { from: 0, take: 0 },
        CompPiece { from: PPQ, take: 1 },
    ];
    let region = Region {
        start: 0,
        length: PPQ * 2,
        content: Content::Takes {
            takes: vec![take(57), take(69)],
            comp,
        },
        ..Default::default()
    };
    let track = Track {
        name: "t".into(),
        instrument: Some(Instrument::Synth),
        regions: vec![region],
        ..Default::default()
    };
    let mut files = AudioFiles::new(Path::new("."), RATE);
    let (arrangement, _) = compile(
        &project(vec![track]),
        RATE,
        &[(0, 0)],
        &[],
        None,
        &mut files,
    );
    let ons: Vec<(u64, u8)> = arrangement.plans[0]
        .events
        .iter()
        .filter(|e| e.velocity > 0.0)
        .map(|e| (e.at, e.pitch))
        .collect();
    assert_eq!(ons, [(0, 57), (BEAT as u64, 69)]);
}

#[test]
fn a_launched_cell_loops_instead_of_the_track() {
    use crate::song::project::Cell;
    // The track's own region is silent; its cell is a beat-long loop with a short note.
    let mut t = synth_track("a", vec![], PPQ * 8);
    let notes = vec![note(0, PPQ / 4, 69)];
    let region = Region {
        start: 0,
        length: PPQ,
        content: Content::Midi {
            notes,
            controls: Vec::new(),
        },
        ..Default::default()
    };
    t.cells.push(Cell { scene: 0, region });
    let (mut engine, _) = Engine::load(&project(vec![t]), Path::new("."), RATE).unwrap();
    engine.play();
    assert_eq!(peak(&run(&mut engine, BEAT)[0]), 0.0);
    // Launched at beat 2: it plays there, rests, and comes round again at beat 3.
    engine.launch(0, Some(0), BEAT as u64 * 2);
    let [l, _] = run(&mut engine, BEAT * 3);
    let beat = |b: usize, from: f32, to: f32| {
        peak(&l[b * BEAT + (BEAT as f32 * from) as usize..b * BEAT + (BEAT as f32 * to) as usize])
    };
    assert_eq!(beat(0, 0.0, 1.0), 0.0); // beat 2 of the song is before the launch
    assert!(beat(1, 0.02, 0.2) > 0.1 && beat(1, 0.5, 0.95) == 0.0);
    assert!(beat(2, 0.02, 0.2) > 0.1);
    // Stopped, the track's own (silent) region plays again.
    engine.launch(0, None, engine.position());
    let [l, _] = run(&mut engine, BEAT);
    assert_eq!(peak(&l[RELEASE + MAX_BLOCK..]), 0.0);
}

#[test]
fn controllers_reach_the_instrument_as_they_stand_at_each_pass() {
    use crate::song::project::{ControlPoint, ControlValue::Cc};
    // A beat-long region from beat 2, looping: the mod wheel at 10 from before it starts,
    // then 100 halfway through.
    let controls = vec![
        ControlPoint {
            start: -100,
            kind: Cc {
                number: 1,
                value: 10,
            },
        },
        ControlPoint {
            start: PPQ as i64 / 2,
            kind: Cc {
                number: 1,
                value: 100,
            },
        },
    ];
    let region = Region {
        start: PPQ,
        length: PPQ,
        looped: true,
        content: Content::Midi {
            notes: vec![],
            controls,
        },
        ..Default::default()
    };
    let t = Track {
        name: "p".into(),
        instrument: Some(Instrument::Plugin(PluginRef::default())),
        regions: vec![
            region,
            Region {
                start: PPQ * 3,
                length: PPQ,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let probe = Probe::default();
    let mut engine = probed(&project(vec![t]), &probe);
    engine.play();
    run(&mut engine, BEAT * 4);
    let wheel = |v: u8| ControlKind::Cc {
        number: 1,
        value: v as f32 / 127.0,
    };
    let half = BEAT as u64 / 2;
    let (b1, b2) = (BEAT as u64, BEAT as u64 * 2);
    // Each pass of the loop starts where the mod wheel stood before it, and moves halfway.
    assert_eq!(
        *probe.heard.lock().unwrap(),
        [
            (b1, wheel(10)),
            (b1 + half, wheel(100)),
            (b2, wheel(10)),
            (b2 + half, wheel(100))
        ]
    );
}

#[test]
fn stopping_releases_held_notes() {
    let p = project(vec![synth_track("a", vec![note(0, PPQ * 8, 69)], PPQ * 8)]);
    let (mut engine, _) = Engine::load(&p, Path::new("."), RATE).unwrap();
    engine.play();
    assert!(peak(&run(&mut engine, MAX_BLOCK * 10)[0]) > 0.1);
    engine.stop();
    run(&mut engine, RELEASE + MAX_BLOCK);
    assert_eq!(peak(&run(&mut engine, MAX_BLOCK)[0]), 0.0);
    assert_eq!(engine.position(), (MAX_BLOCK * 10) as u64);
}

#[test]
fn stopping_releases_overlapping_notes_of_one_pitch() {
    // Two notes on A, the second starting while the first still sounds: two voices.
    let notes = vec![note(0, PPQ * 8, 69), note(PPQ as i64, PPQ * 8, 69)];
    let p = project(vec![synth_track("a", notes, PPQ * 8)]);
    let (mut engine, _) = Engine::load(&p, Path::new("."), RATE).unwrap();
    engine.play();
    run(&mut engine, BEAT * 2);
    engine.stop();
    run(&mut engine, RELEASE + MAX_BLOCK);
    assert_eq!(peak(&run(&mut engine, MAX_BLOCK)[0]), 0.0);
}

#[test]
fn an_edit_while_playing_lets_go_of_notes_the_song_no_longer_has() {
    let (mut tx, rx) = rtrb::RingBuffer::new(16);
    let (gtx, _garbage) = rtrb::RingBuffer::new(16);
    let (_live, live_rx) = rtrb::RingBuffer::new(16);
    let (played_tx, _played) = rtrb::RingBuffer::new(16);
    let link = Link {
        rx,
        garbage: gtx,
        shared: Arc::new(Shared::new()),
        live: live_rx,
        played: played_tx,
    };
    let mut engine = Engine::live(RATE, link);
    // A long A, and a long C that the edit keeps.
    let notes = vec![note(0, PPQ * 8, 69), note(0, PPQ * 8, 60)];
    let p = project(vec![synth_track("a", notes, PPQ * 8)]);
    let lane = make_lane(
        Some(&Instrument::Synth),
        &[],
        RATE,
        &mut Modules::new(),
        Path::new("."),
    )
    .unwrap()
    .0;
    let mut files = AudioFiles::new(Path::new("."), RATE);
    let arrange = |p: &Project, files: &mut AudioFiles| {
        Box::new(compile(p, RATE, &[(3, 0)], &[], None, files).0)
    };
    for m in [
        Msg::Insert(3, Box::new(lane)),
        Msg::Arrange(arrange(&p, &mut files)),
        Msg::Play,
    ] {
        assert!(tx.push(m).is_ok());
    }
    run(&mut engine, BEAT);
    // The A goes (as a key change or a deleted note would take it); the C stays.
    let mut edited = p.clone();
    if let Content::Midi { notes, .. } = &mut edited.tracks[0].regions[0].content {
        notes.retain(|n| n.pitch == 60);
    }
    assert!(tx.push(Msg::Arrange(arrange(&edited, &mut files))).is_ok());
    run(&mut engine, RELEASE + MAX_BLOCK);
    let [l, _] = run(&mut engine, BEAT / 2);
    // Only the C rings on: its 261.6 Hz, and no 440 Hz.
    let level = |f: f64| {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &x) in l.iter().enumerate() {
            let w = 2.0 * std::f64::consts::PI * f * i as f64 / RATE;
            re += x as f64 * w.cos();
            im += x as f64 * w.sin();
        }
        (re * re + im * im).sqrt() * 2.0 / l.len() as f64
    };
    let (c, a) = (level(261.63), level(440.0));
    assert!(c > 0.05 && a < c / 20.0, "C {c}, A {a}"); // what's left of A is the C's spill
}

#[test]
fn a_looped_region_plays_until_the_next_region() {
    let mut t = synth_track("a", vec![note(0, 240, 60)], PPQ);
    t.regions[0].looped = true;
    t.regions.push(Region {
        start: PPQ * 3,
        length: PPQ,
        content: Content::Midi {
            notes: vec![note(0, 240, 72)],
            controls: Vec::new(),
        },
        ..Default::default()
    });
    let b = BEAT as u64;
    assert_eq!(
        note_ons(&project(vec![t])),
        [
            (0, 60, 127),
            (b, 60, 127),
            (2 * b, 60, 127),
            (3 * b, 72, 127)
        ]
    );
}

#[test]
fn loops_dragged_out_stop_where_they_were_dragged_and_stretching_spreads_notes() {
    // Looped to three beats, with the next region at five: three plays.
    let mut t = synth_track("a", vec![note(0, 240, 60)], PPQ);
    t.regions[0].looped = true;
    t.regions[0].loop_length = Some(PPQ * 3);
    t.regions.push(Region {
        start: PPQ * 5,
        length: PPQ,
        content: Content::Midi {
            notes: vec![note(0, 240, 72)],
            controls: Vec::new(),
        },
        ..Default::default()
    });
    let b = BEAT as u64;
    let mut p = project(vec![t]);
    assert_eq!(
        note_ons(&p),
        [
            (0, 60, 127),
            (b, 60, 127),
            (2 * b, 60, 127),
            (5 * b, 72, 127)
        ]
    );
    // Trimming its start keeps where its loops stop.
    p.trim_start((0, 0), PPQ / 2);
    assert_eq!(p.loop_end((0, 0)), PPQ * 3);
    // Stretched to twice as long: the note starts and lasts twice as late and long.
    p.tracks[0].regions[1].start = PPQ * 5;
    p.flex((0, 1), PPQ * 2).unwrap();
    let r = &p.tracks[0].regions[1];
    let Content::Midi { notes, .. } = &r.content else {
        unreachable!()
    };
    assert_eq!(
        (r.length, notes[0].start, notes[0].length),
        (PPQ * 2, 0, 480)
    );
}

#[test]
fn region_playback_settings_change_what_plays_not_the_notes() {
    let mut t = synth_track("a", vec![note(100, 240, 60), note(350, 240, 64)], PPQ);
    let r = &mut t.regions[0];
    r.transpose = 12;
    r.velocity = -27;
    r.quantize = Some(240);
    let p = project(vec![t]);
    let sixteenth = BEAT as u64 / 4;
    assert_eq!(note_ons(&p), [(0, 72, 100), (sixteenth, 76, 100)]);
    let Content::Midi { notes, .. } = &p.tracks[0].regions[0].content else {
        unreachable!()
    };
    assert_eq!((notes[0].start, notes[0].pitch), (100, 60));
}
