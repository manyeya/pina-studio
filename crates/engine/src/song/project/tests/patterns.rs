//! Step patterns: lanes, modulators on their sounds, swing, rows of their own lengths.

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
