//! Step patterns: lanes, modulators on their sounds, swing, loops, playback modes, ties and skips,
//! rates, repeats, conditions, automation rows, old songs, and what can be done to rows.

use super::*;

const STEP: u64 = PPQ / 4;

fn row(pitch: u8, cells: Vec<u8>) -> PatternRow {
    PatternRow {
        pitch,
        cells,
        ..Default::default()
    }
}

fn pattern(rows: Vec<PatternRow>) -> Pattern {
    Pattern::new(STEP, rows, 50.0)
}

fn starts(notes: &[Note], pitch: u8) -> Vec<i64> {
    notes
        .iter()
        .filter(|n| n.pitch == pitch)
        .map(|n| n.start)
        .collect()
}

#[test]
fn a_patterns_lanes_and_the_modulators_on_its_sounds() {
    use crate::dsp::modulate::{
        LfoShape, ModKind, ModRate, ModRoute, ModTarget, Modulator, SoundParam,
    };
    // A ratchet of three, a fifth up, held half its time, a quarter of a step late.
    let mut r = row(36, vec![100, 0, 0, 0]);
    (r.repeat, r.transpose, r.gate, r.nudge) = (vec![3], vec![7], vec![50], vec![25]);
    let notes = pattern_notes(&pattern(vec![r]), STEP * 4);
    let sub = STEP as f64 / 3.0;
    let late = (STEP / 4) as i64;
    let at: Vec<i64> = notes.iter().map(|n| n.start).collect();
    assert_eq!(
        at,
        [
            late,
            late + sub.round() as i64,
            late + (2.0 * sub).round() as i64
        ]
    );
    assert!(
        notes
            .iter()
            .all(|n| n.pitch == 43 && n.length == (sub / 2.0).round() as u64)
    );
    // Half a chance: some steps of a long run play, some don't, the same each time.
    let mut r = row(36, vec![100; 64]);
    r.chance = vec![50; 64];
    let p = pattern(vec![r]);
    let played = pattern_notes(&p, STEP * 64);
    assert!((20..44).contains(&played.len()), "{}", played.len());
    assert_eq!(played, pattern_notes(&p, STEP * 64));
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
    let p = pattern(vec![row(36, vec![100; 4])]);
    let notes = pattern_played(&p, STEP * 4, 0, &[lfo], 120.0);
    let v: Vec<u8> = notes.iter().map(|n| n.velocity).collect();
    assert_eq!(v, [60, 60, 127, 127]);
}

#[test]
fn swing_makes_every_second_sixteenth_or_eighth_late() {
    let mut p = pattern(vec![row(42, vec![100; 4])]);
    let at = |p: &Pattern| starts(&pattern_notes(p, PPQ), 42);
    assert_eq!(at(&p), [0, 240, 480, 720]);
    // 66%: the second of each pair two thirds of the way through it, a triplet feel.
    p.swing = 66.0;
    assert_eq!(at(&p), [0, 316, 480, 796]);
    // On eighths: the second eighth of each quarter late, the sixteenth after it with it.
    p.swing_eighths = true;
    assert_eq!(at(&p), [0, 240, 633, 873]);
}

#[test]
fn rows_loop_on_their_own_lengths_and_muted_rows_rest() {
    let mut muted = row(38, vec![100; 4]);
    muted.mute = true;
    let p = pattern(vec![
        row(36, vec![100, 0, 0, 0]),
        row(42, vec![80, 0, 0]),
        muted,
    ]);
    // The three-step row loops on its own: its loop end is where it stopped.
    assert_eq!(p.rows[1].cells.len(), 4);
    assert_eq!(p.rows[1].loop_end, Some(3));
    let notes = pattern_notes(&p, STEP * 8);
    let s = STEP as i64;
    assert_eq!(starts(&notes, 36), [0, 4 * s]);
    assert_eq!(starts(&notes, 42), [0, 3 * s, 6 * s]);
    assert!(starts(&notes, 38).is_empty());
}

#[test]
fn a_row_runs_through_its_loop_its_own_way() {
    let mut r = row(60, vec![100; 4]);
    // Each step a note of its own, to see the order: 60, 61, 62, 63.
    r.transpose = vec![0, 1, 2, 3];
    let order = |mode: PlayMode, steps: u64| {
        let mut p = pattern(vec![r.clone()]);
        p.rows[0].mode = Some(mode);
        pattern_notes(&p, STEP * steps)
            .iter()
            .map(|n| n.pitch - 60)
            .collect::<Vec<_>>()
    };
    assert_eq!(order(PlayMode::Forward, 6), [0, 1, 2, 3, 0, 1]);
    assert_eq!(order(PlayMode::Backward, 6), [3, 2, 1, 0, 3, 2]);
    assert_eq!(
        order(PlayMode::PingPong, 10),
        [0, 1, 2, 3, 3, 2, 1, 0, 0, 1]
    );
    assert_eq!(order(PlayMode::Pendulum, 8), [0, 1, 2, 3, 2, 1, 0, 1]);
    // Shuffle: every step once each time round.
    let mut shuffled = order(PlayMode::Shuffle, 8);
    let (a, b) = shuffled.split_at_mut(4);
    a.sort();
    b.sort();
    assert_eq!(shuffled, [0, 1, 2, 3, 0, 1, 2, 3]);
    // Wander: a step on or back each time.
    let w = order(PlayMode::Wander, 16);
    assert!(
        w.windows(2).all(|p| (p[0] as i32 - p[1] as i32).abs() == 1),
        "{w:?}"
    );
    // Random stays in the loop, and is the same each time.
    let rnd = order(PlayMode::Random, 16);
    assert!(rnd.iter().all(|&x| x < 4));
    assert_eq!(rnd, order(PlayMode::Random, 16));
    // The loop: steps 2 to 3 only.
    let mut p = pattern(vec![r.clone()]);
    (p.rows[0].loop_start, p.rows[0].loop_end) = (1, Some(3));
    let pitches: Vec<u8> = pattern_notes(&p, STEP * 4)
        .iter()
        .map(|n| n.pitch - 60)
        .collect();
    assert_eq!(pitches, [1, 2, 1, 2]);
}

#[test]
fn ties_hold_through_steps_skips_are_jumped_and_steps_have_their_own_rates() {
    // A tie from the first step through the second: one note, two steps and half the third.
    let mut r = row(48, vec![100, 0, 0, 100]);
    r.tie = vec![true, true, false, false];
    r.gate = vec![100, 100, 50, 100];
    let notes = pattern_notes(&pattern(vec![r]), STEP * 4);
    assert_eq!(notes.len(), 2);
    assert_eq!((notes[0].start, notes[0].length), (0, 2 * STEP + STEP / 2));
    assert_eq!(notes[1].start, 3 * STEP as i64);
    // Skipping the second step: the row is three steps long.
    let mut r = row(36, vec![100, 0, 100, 100]);
    r.skip = vec![false, true, false, false];
    let s = STEP as i64;
    let notes = pattern_notes(&pattern(vec![r]), STEP * 6);
    assert_eq!(starts(&notes, 36), [0, s, 2 * s, 3 * s, 4 * s, 5 * s]);
    // A step at an eighth: the steps after it come a sixteenth later.
    let mut r = row(36, vec![100; 4]);
    r.rates = vec![0, PPQ / 2, 0, 0];
    assert_eq!(
        starts(&pattern_notes(&pattern(vec![r]), STEP * 5), 36),
        [0, s, 3 * s, 4 * s]
    );
    // A row at its own rate: eighths against the pattern's sixteenths.
    let mut r = row(36, vec![100; 4]);
    r.rate = Some(PPQ / 2);
    assert_eq!(
        starts(&pattern_notes(&pattern(vec![r]), STEP * 4), 36),
        [0, 2 * s]
    );
    // Legato: a full gate runs a tick on.
    let mut p = pattern(vec![row(60, vec![100, 100])]);
    p.legato = true;
    assert_eq!(pattern_notes(&p, STEP * 2)[0].length, STEP + 1);
}

#[test]
fn repeats_spread_and_ramp_and_steps_play_on_their_passes() {
    // Four repeats, louder through them.
    let mut r = row(38, vec![80, 0, 0, 0]);
    (r.repeat, r.ramp) = (vec![4], vec![50]);
    let notes = pattern_notes(&pattern(vec![r]), STEP * 4);
    let v: Vec<u8> = notes.iter().map(|n| n.velocity).collect();
    assert_eq!(v.len(), 4);
    assert!(v.windows(2).all(|w| w[0] < w[1]) && v[3] == 120, "{v:?}");
    // Distributed: the first repeat longest.
    let mut r = row(38, vec![80, 0, 0, 0]);
    (r.repeat, r.spread) = (vec![3], vec![100]);
    let n = pattern_notes(&pattern(vec![r]), STEP * 4);
    assert!(n[1].start - n[0].start > n[2].start - n[1].start);
    // The second pass of every two; the first pass only; the last only.
    let mut r = row(36, vec![100, 100, 100, 100]);
    r.condition = vec![
        Condition::Recurrence,
        Condition::FirstLoop,
        Condition::LastLoop,
        Condition::Always,
    ];
    r.every = vec![[2, 2], [1, 1], [1, 1], [1, 1]];
    let s = STEP as i64;
    let notes = pattern_notes(&pattern(vec![r]), STEP * 12);
    assert_eq!(starts(&notes, 36), [s, 3 * s, 4 * s, 7 * s, 10 * s, 11 * s]);
}

#[test]
fn soloed_rows_play_alone_and_automation_rows_set_their_parameters() {
    let mut kick = row(36, vec![100; 2]);
    kick.solo = true;
    let mut cc = PatternRow::automation(RowTarget::Cc(74), 2, 0.25);
    cc.cells = vec![100, 100];
    cc.value = vec![0.25, 0.75];
    cc.solo = true;
    let p = pattern(vec![kick, row(38, vec![100; 2]), cc.clone()]);
    let notes = pattern_notes(&p, STEP * 2);
    assert!(notes.iter().all(|n| n.pitch == 36) && notes.len() == 2);
    let auto = pattern_automation(&p, STEP * 2, 0);
    assert_eq!(auto.len(), 1);
    let (target, points) = &auto[0];
    assert_eq!(*target, RowTarget::Cc(74));
    assert_eq!(
        points
            .iter()
            .map(|p| (p.tick, p.value, p.step))
            .collect::<Vec<_>>(),
        [(0, 0.25, true), (STEP, 0.75, true)]
    );
    // Sliding, not held.
    let mut p = pattern(vec![cc]);
    p.rows[0].slide = true;
    assert!(
        pattern_automation(&p, STEP * 2, 0)[0]
            .1
            .iter()
            .all(|p| !p.step)
    );
    // An automation row's steps play no notes.
    assert!(pattern_notes(&p, STEP * 2).is_empty());
}

#[test]
fn an_old_pattern_plays_as_it_did_and_midi_becomes_a_pattern() {
    // Saved before steps had gates of their own: 75% of a step, its short row looping.
    let old: Pattern =
        serde_json::from_str(r#"{"step": 240, "rows": [{"pitch": 60, "cells": [100, 0, 0]}]}"#)
            .unwrap();
    assert_eq!(old.version, 0);
    let mut p = old.clone();
    p.upgrade();
    assert_eq!(pattern_notes(&p, STEP)[0].length, STEP * 3 / 4);
    assert_eq!((p.rows[0].loop_end, p.version), (None, 1));
    // Made now: the whole step, and it stays so when saved and opened.
    let new = pattern(vec![row(60, vec![100])]);
    let mut opened: Pattern = serde_json::from_str(&serde_json::to_string(&new).unwrap()).unwrap();
    opened.upgrade();
    assert_eq!(pattern_notes(&opened, STEP)[0].length, STEP);
    // A beat of MIDI: kick on 1 and 3 (a little late), a hat between; highest row first. The
    // late kick keeps its offset; a long note is tied on.
    let s = STEP as i64;
    let n = |start: i64, length: u64, pitch: u8, velocity: u8| Note {
        start,
        length,
        pitch,
        velocity,
    };
    let rows = to_pattern(
        &[
            n(0, 100, 36, 110),
            n(2 * s + 24, 100, 36, 90),
            n(s, 2 * STEP, 42, 70),
        ],
        STEP,
        4,
    );
    assert_eq!(
        rows.iter()
            .map(|r| (r.pitch, r.cells.clone()))
            .collect::<Vec<_>>(),
        [(42, vec![0, 70, 0, 0]), (36, vec![110, 0, 90, 0])]
    );
    assert_eq!(rows[1].nudge_at(2), 10);
    assert!(rows[0].tie_at(1) && rows[0].gate_at(2) == 100);
    let back = pattern_notes(&pattern(rows), STEP * 4);
    assert!(back.iter().any(|n| n.pitch == 42 && n.length == 2 * STEP));
}

#[test]
fn rows_rotate_step_up_and_down_randomize_clear_ramp_and_paste() {
    let mut r = row(36, vec![100, 0, 0, 0]);
    r.gate = vec![50, 100, 100, 100];
    // Rotated with everything: the step and its gate go together.
    r.rotate(4, 1, None);
    assert_eq!((r.cells.clone(), r.gate_at(1)), (vec![0, 100, 0, 0], 50));
    // Rotating the gates alone leaves the rhythm.
    r.rotate(4, 1, Some(Lane::Gate));
    assert_eq!((r.cells.clone(), r.gate_at(2)), (vec![0, 100, 0, 0], 50));
    // Up a notch: every velocity; on/off: every step on; tie: all but the last.
    r.step_all(4, Some(Lane::Velocity), true);
    assert_eq!(r.velocity_at(1), 101);
    r.step_all(4, None, true);
    assert!(r.cells.iter().all(|&c| c > 0));
    r.step_all(4, Some(Lane::Tie), true);
    assert_eq!(r.tie, [true, true, true, false]);
    // A step turned off keeps its velocity, and comes back with it.
    r.set(Lane::Velocity, 0, 33.0);
    r.set_on(0, false);
    assert_eq!((r.cells[0], r.velocity_at(0)), (0, 33));
    r.set_on(0, true);
    assert_eq!(r.cells[0], 33);
    // A ramp from 20 to 80 over four steps.
    r.set(Lane::Velocity, 0, 20.0);
    r.set(Lane::Velocity, 3, 80.0);
    r.ramp(Lane::Velocity, 0, 3);
    assert_eq!(r.cells, [20, 40, 60, 80]);
    // Randomized within 1 to 10 repeats.
    let how = Randomize {
        within: true,
        low: 1.0,
        high: 10.0,
        ..Default::default()
    };
    r.randomize(4, Some(Lane::Repeat), &how, 7);
    assert!((0..4).all(|i| (1..=10).contains(&r.repeat_at(i))));
    // Cleared: the repeats as usual; then everything off.
    r.clear_lane(Some(Lane::Repeat));
    assert!(r.repeat.is_empty());
    r.clear();
    assert!(r.lane_is_clear(None) && r.tie.is_empty());
    // A rhythm pasted from one drum to another keeps the other's sound.
    let mut snare = row(38, vec![0; 4]);
    paste_row(&mut snare, &row(36, vec![100, 0, 100, 0]));
    assert_eq!(
        (snare.pitch, snare.cells.clone()),
        (38, vec![100, 0, 100, 0])
    );
}

#[test]
fn a_pattern_grows_by_repeating_transposes_and_keeps_to_its_scale() {
    let mut p = pattern(vec![
        row(36, vec![100, 0, 0, 0]),
        row(61, vec![0, 100, 0, 0]),
    ]);
    p.set_length(8);
    assert_eq!(p.rows[0].cells, [100, 0, 0, 0, 100, 0, 0, 0]);
    p.set_length(2);
    assert_eq!(p.rows[0].cells, [100, 0]);
    // In C major, C♯ is out until conformed.
    p.scale = Some(Scale::Major);
    assert!(p.out_of_scale());
    p.conform();
    assert!(!p.out_of_scale());
    assert_eq!(p.rows[1].pitch, 60);
    // D major: everything up a tone.
    p.set_key(2, true);
    assert_eq!((p.rows[0].pitch, p.key), (38, 2));
    // Mono: one sound a step.
    let mut p = pattern(vec![row(36, vec![100]), row(38, vec![100])]);
    p.mono(0, 1);
    assert_eq!((p.rows[0].cells[0], p.rows[1].cells[0]), (0, 100));
}
