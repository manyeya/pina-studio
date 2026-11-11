use super::*;

fn bend(at: u64, value: f32) -> Bend {
    Bend::new(at, value)
}

#[test]
fn a_curve_holds_before_and_after_its_points_and_runs_along_them_between() {
    let points = [bend(100, 2.0), bend(300, 6.0)];
    assert_eq!(curve_at(&points, 0), Some(2.0));
    assert_eq!(curve_at(&points, 200), Some(4.0));
    assert_eq!(curve_at(&points, 900), Some(6.0));
    assert_eq!(curve_at(&[], 10), None);
    // Bowed, it's slow then fast (or fast then slow), and still ends where it ends.
    let slow = [
        Bend {
            curve: 0.8,
            ..bend(0, 0.0)
        },
        bend(100, 1.0),
    ];
    assert!(curve_at(&slow, 50).unwrap() < 0.3);
    assert_eq!(curve_at(&slow, 100), Some(1.0));
    let fast = [
        Bend {
            curve: -0.8,
            ..bend(0, 0.0)
        },
        bend(100, 1.0),
    ];
    assert!(curve_at(&fast, 50).unwrap() > 0.7);
}

#[test]
fn a_dense_line_thins_to_the_points_that_shape_it() {
    // Up a semitone in a straight line, then held: three points draw it.
    let dense: Vec<Bend> = (0..=40)
        .map(|k| bend(k * 10, (k.min(20) as f32) / 20.0))
        .collect();
    let kept = thin(&dense, 0.03);
    assert_eq!(kept, [bend(0, 0.0), bend(200, 1.0), bend(400, 1.0)]);
    // A wiggle bigger than the tolerance stays.
    let wiggle = [bend(0, 0.0), bend(10, 0.5), bend(20, 0.0)];
    assert_eq!(thin(&wiggle, 0.03).len(), 3);
}

#[test]
fn a_regions_note_moves_go_onto_the_notes_they_belong_to() {
    let note = |start, pitch| Note {
        start,
        length: 480,
        pitch,
        ..Default::default()
    };
    let mut notes = vec![note(0, 60), note(480, 60), note(0, 64)];
    let tuning = |start, pitch, value| ControlPoint {
        start,
        kind: ControlValue::Expression {
            pitch,
            kind: ExpressionKind::Tuning,
            value,
        },
    };
    let mut controls = vec![
        tuning(0, 60, 0.0),
        tuning(240, 60, 2.0),
        tuning(600, 60, -1.0),
        ControlPoint {
            start: 120,
            kind: ControlValue::PolyPressure {
                pitch: 64,
                value: 127,
            },
        },
        // No note of its pitch then: it stays.
        tuning(100, 72, 1.0),
        ControlPoint {
            start: 0,
            kind: ControlValue::Cc {
                number: 1,
                value: 9,
            },
        },
    ];
    adopt_expression(&mut notes, &mut controls);
    assert_eq!(notes[0].expression.pitch, [bend(0, 0.0), bend(240, 2.0)]);
    assert_eq!(notes[1].expression.pitch, [bend(120, -1.0)]);
    assert_eq!(notes[2].expression.press, [bend(120, 1.0)]);
    assert_eq!(controls.len(), 2);
}

#[test]
fn a_note_split_keeps_each_halfs_line() {
    let mut e = NoteExpression {
        pitch: vec![bend(0, 0.0), bend(200, 4.0)],
        ..Default::default()
    };
    let after = e.split(100);
    assert_eq!(e.pitch, [bend(0, 0.0), bend(100, 2.0)]);
    assert_eq!(after.pitch, [bend(0, 2.0), bend(100, 4.0)]);
    assert!(after.slide.is_empty());
}

#[test]
fn a_notes_moves_start_with_it_and_follow_its_lines() {
    let e = NoteExpression {
        pitch: vec![bend(0, 0.0), bend(100, 2.0)],
        press: vec![bend(50, 0.5)],
        ..Default::default()
    };
    let moves = e.moves(60, 480, 25);
    let pitch: Vec<(u64, f32)> = moves
        .iter()
        .filter_map(|(t, k)| match k {
            ControlKind::Expression { value, .. } => Some((*t, *value)),
            _ => None,
        })
        .collect();
    assert_eq!(
        pitch,
        [(0, 0.0), (25, 0.5), (50, 1.0), (75, 1.5), (100, 2.0)]
    );
    // A held pressure: said once, from the start.
    let press: Vec<u64> = moves
        .iter()
        .filter(|(_, k)| matches!(k, ControlKind::PolyPressure { .. }))
        .map(|m| m.0)
        .collect();
    assert_eq!(press, [0]);
    // Nothing after the note's end.
    assert!(e.moves(60, 60, 25).iter().all(|m| m.0 < 60));
}

#[test]
fn a_gesture_writes_its_shape_and_a_glide_goes_to_the_next_note() {
    use crate::music::NoteEdit;
    use crate::song::project::PPQ;
    let note = |start, pitch| Note {
        start,
        length: PPQ,
        pitch,
        ..Default::default()
    };
    let mut notes = vec![note(0, 60), note(PPQ as i64, 64)];
    let express = |gesture| NoteEdit::Express {
        gesture,
        amount: 100,
        rate: 3.0,
    };
    express(Gesture::Glide).apply(&mut notes, &[0]);
    let glide = &notes[0].expression.pitch;
    assert_eq!(glide.first().map(|b| b.value), Some(0.0));
    assert_eq!(glide.last().map(|b| (b.at, b.value)), Some((PPQ, 4.0)));
    // The last note has nothing to glide to.
    express(Gesture::Glide).apply(&mut notes, &[1]);
    assert!(notes[1].expression.pitch.is_empty());
    // A scoop comes up from below to the note within a sixteenth.
    express(Gesture::Scoop).apply(&mut notes, &[1]);
    let scoop = &notes[1].expression.pitch;
    assert!(scoop[0].value < 0.0);
    assert_eq!(curve_at(scoop, PPQ / 4), Some(0.0));
    // A vibrato starts still and keeps within its swing.
    express(Gesture::Vibrato).apply(&mut notes, &[0]);
    let vib = &notes[0].expression.pitch;
    assert_eq!(curve_at(vib, 0), Some(0.0));
    assert!(vib.iter().all(|b| b.value.abs() <= 0.4 + 1e-6));
    assert!(vib.iter().any(|b| b.value > 0.3) && vib.iter().any(|b| b.value < -0.3));
    // Pressure and slide stay in 0 to 1.
    express(Gesture::Tremolo).apply(&mut notes, &[]);
    express(Gesture::Wobble).apply(&mut notes, &[]);
    for n in &notes {
        let all = n.expression.press.iter().chain(&n.expression.slide);
        assert!(all.clone().all(|b| (0.0..=1.0).contains(&b.value)));
        assert!(all.count() > 4);
    }
    // Taken off, one kind or all.
    NoteEdit::Unexpress {
        dim: Some(Dim::Press),
    }
    .apply(&mut notes, &[]);
    assert!(notes.iter().all(|n| n.expression.press.is_empty()));
    assert!(!notes[0].expression.slide.is_empty());
    NoteEdit::Unexpress { dim: None }.apply(&mut notes, &[]);
    assert!(notes.iter().all(|n| n.expression.is_empty()));
}
