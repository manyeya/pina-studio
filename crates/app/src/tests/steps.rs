//! The step sequencer.

use pina_engine::song::project::{Lane, Pattern, PatternRow, RowTarget};

use super::*;
use crate::editors::steps::{EditMode, Learn, StepsOp};

/// A drum track with a pattern on it, the step sequencer open on it.
fn drums_pattern(cx: &mut TestAppContext) -> (Entity<Workspace>, &mut VisualTestContext, usize) {
    let (ws, cx) = open(cx);
    click_on(cx, "new-track", Modifiers::none(), MouseButton::Left);
    click_on(cx, "adder-Drums", Modifiers::none(), MouseButton::Left);
    click_on(cx, "adder-row-0", Modifiers::none(), MouseButton::Left); // Pina Studio's Drum Kit
    let track = ws.read_with(cx, |ws, _| ws.track);
    cx.simulate_keystrokes("cmd-alt-p");
    ws.update(cx, |ws, cx| {
        ws.editor_h = 620.0;
        cx.notify();
    });
    cx.run_until_parked();
    (ws, cx, track)
}

fn shown(ws: &Entity<Workspace>, cx: &VisualTestContext) -> Pattern {
    ws.read_with(cx, |ws, _| ws.shown_pattern().unwrap().1.clone())
}

fn at(cx: &mut VisualTestContext, name: String) -> Point<Pixels> {
    cx.debug_bounds(Box::leak(name.into_boxed_str()))
        .unwrap_or_else(|| panic!("not on screen"))
        .center()
}

fn with(f: impl FnOnce(&mut Modifiers)) -> Modifiers {
    let mut m = Modifiers::none();
    f(&mut m);
    m
}

#[gpui::test]
fn steps_are_painted_rubbed_out_reset_and_rows_muted_rotated_and_filled(cx: &mut TestAppContext) {
    let (ws, cx, track) = drums_pattern(cx);
    // A second pattern from the same playhead goes after the first, not on it.
    ws.update(cx, |ws, cx| {
        ws.region = None;
        cx.notify();
    });
    cx.simulate_keystrokes("cmd-alt-p");
    let starts = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[track]
            .regions
            .iter()
            .map(|r| r.start)
            .collect::<Vec<_>>()
    });
    assert_eq!(starts, [0, BAR]);
    // Painting the kick row across four steps, then rubbing two out.
    let from = at(cx, "step-0-0".into());
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    for k in 1..=3 {
        let p = at(cx, format!("step-0-{k}"));
        cx.simulate_mouse_move(p, MouseButton::Left, Modifiers::none());
    }
    cx.simulate_mouse_up(from, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(&shown(&ws, cx).rows[0].cells[..5], [100, 100, 100, 100, 0]);
    let (a, b) = (at(cx, "step-0-2".into()), at(cx, "step-0-3".into()));
    drag(cx, a, b);
    assert_eq!(&shown(&ws, cx).rows[0].cells[..5], [100, 100, 0, 0, 0]);
    // ⌥-click: back as usual (off).
    click_on(cx, "step-0-1", with(|m| m.alt = true), MouseButton::Left);
    assert_eq!(&shown(&ws, cx).rows[0].cells[..2], [100, 0]);
    // The hats muted; every step of them on with the row's up button; the kick rotated on a
    // step.
    click_on(cx, "steps-m-2", Modifiers::none(), MouseButton::Left);
    assert!(shown(&ws, cx).rows[2].mute);
    click_on(cx, "steps-up-2", Modifiers::none(), MouseButton::Left);
    assert!(shown(&ws, cx).rows[2].cells.iter().all(|&c| c > 0));
    click_on(
        cx,
        "steps-rotate-right-0",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert_eq!(&shown(&ws, cx).rows[0].cells[..3], [0, 100, 0]);
    // 32 steps: the bar's groove twice.
    click_on(cx, "steps-length", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "32 Steps");
    let p = shown(&ws, cx);
    assert_eq!((p.steps(), p.rows[0].cells.len()), (32, 32));
    assert_eq!(p.rows[0].cells[16..18], p.rows[0].cells[0..2]);
}

#[gpui::test]
fn values_are_dragged_ramped_and_tied_and_rows_open_on_their_subrows(cx: &mut TestAppContext) {
    let (ws, cx, _) = drums_pattern(cx);
    click_on(cx, "step-1-4", Modifiers::none(), MouseButton::Left);
    // Velocity / Value: the snare's step dragged down.
    click_on(cx, "steps-mode-value", Modifiers::none(), MouseButton::Left);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.steps.mode),
        EditMode::Lane(Lane::Velocity)
    );
    let s = at(cx, "step-1-4".into());
    drag(cx, s, point(s.x, s.y + px(60.)));
    let v = shown(&ws, cx).rows[1].cells[4];
    assert!(v > 0 && v < 70, "{v}");
    // ⌥⇧: a ramp from the first kick step (as usual: 100) to the fourth, dragged up.
    let (a, b) = (at(cx, "step-0-0".into()), at(cx, "step-0-3".into()));
    drag_with(
        cx,
        a,
        point(b.x, b.y - px(100.)),
        with(|m| (m.alt, m.shift) = (true, true)),
    );
    let kick = shown(&ws, cx).rows[0].clone();
    let ramp: Vec<u8> = (0..4).map(|i| kick.velocity_at(i)).collect();
    assert!(
        ramp.windows(2).all(|w| w[0] <= w[1]) && ramp[3] > 100,
        "{ramp:?}"
    );
    // ⌘⌥: the hats' first three steps tied.
    let (a, b) = (at(cx, "step-2-0".into()), at(cx, "step-2-2".into()));
    drag_with(cx, a, b, with(|m| (m.platform, m.alt) = (true, true)));
    let hats = shown(&ws, cx).rows[2].clone();
    assert_eq!(
        (hats.tie_at(0), hats.tie_at(1), hats.tie_at(2)),
        (true, true, false)
    );
    assert!((0..3).all(|i| hats.on(i)));
    // The snare's subrows, its chance among them: one drawn low.
    click_on(cx, "steps-open-1", Modifiers::none(), MouseButton::Left);
    let snare = shown(&ws, cx).rows[1].clone();
    assert!(snare.open && snare.subrows.contains(&Lane::Chance));
    let c = at(cx, "step-1-Chance-4".into());
    drag(cx, c, point(c.x, c.y + px(120.)));
    let chance = shown(&ws, cx).rows[1].chance_at(4);
    assert!(chance < 40, "{chance}");
    // A subrow added after the last shows a setting the row hasn't one for.
    let k = snare.subrows.len() - 1;
    ws.update(cx, |ws, cx| ws.steps_op(StepsOp::AddSubrow(1, k), cx));
    let subrows = shown(&ws, cx).rows[1].subrows.clone();
    assert_eq!(subrows.len(), k + 2);
    let mut seen = subrows.clone();
    seen.sort_by_key(|l| format!("{l:?}"));
    seen.dedup();
    assert_eq!(seen.len(), subrows.len());
}

#[gpui::test]
fn rows_loop_rate_and_play_their_own_way_and_are_added_and_copied(cx: &mut TestAppContext) {
    let (ws, cx, _) = drums_pattern(cx);
    // Loop: a click nearer the end of the hats' loop moves the end there, nearer the start the
    // start.
    ws.update(cx, |ws, cx| ws.steps_op(StepsOp::Mode(EditMode::Loop), cx));
    cx.run_until_parked();
    click_on(cx, "step-2-11", Modifiers::none(), MouseButton::Left);
    assert_eq!(shown(&ws, cx).rows[2].loop_range(16), (0, 12));
    click_on(cx, "step-2-3", Modifiers::none(), MouseButton::Left);
    assert_eq!(shown(&ws, cx).rows[2].loop_range(16), (3, 12));
    // The pattern at eighths; the snare at its own sixteenth triplets; the kick backwards.
    click_on(
        cx,
        "steps-pattern-rate",
        Modifiers::none(),
        MouseButton::Left,
    );
    pick(cx, &ws, "/8");
    assert_eq!(shown(&ws, cx).step, PPQ / 2);
    click_on(cx, "steps-rate-1", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "16T");
    assert_eq!(shown(&ws, cx).rows[1].rate, Some(PPQ / 6));
    click_on(cx, "steps-play-0", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Backward");
    assert_eq!(
        shown(&ws, cx).rows[0].mode,
        Some(pina_engine::song::project::PlayMode::Backward)
    );
    // A crash from Add Row's notes, and an automation row of the track's volume.
    let rows = shown(&ws, cx).rows.len();
    click_on(cx, "steps-add-row", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Notes");
    pick(cx, &ws, "Crash");
    let p = shown(&ws, cx);
    assert_eq!((p.rows.len(), p.rows[rows].pitch), (rows + 1, 49));
    click_on(cx, "steps-add-row", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Automation");
    pick(cx, &ws, "Volume");
    let p = shown(&ws, cx);
    assert_eq!(p.rows[rows + 1].target, Some(RowTarget::Volume));
    // The kick copied to a new row: the same steps.
    ws.update(cx, |ws, cx| {
        ws.steps_op(StepsOp::CopyRow(0), cx);
        ws.steps_op(StepsOp::PasteToNewRow, cx);
    });
    let p = shown(&ws, cx);
    assert_eq!(p.rows.last().unwrap().cells, p.rows[0].cells);
}

#[gpui::test]
fn notes_played_learn_rows_and_go_in_step_by_step(cx: &mut TestAppContext) {
    let (ws, cx, _) = drums_pattern(cx);
    // MIDI in: the kick on the cursor's step, then a chord on the next; a note with no row
    // makes one.
    ws.update(cx, |ws, cx| {
        ws.steps.midi_in = true;
        ws.steps.cursor = Some((0, 0));
        ws.steps_input(&[(0, 36, 90)], cx);
        ws.steps_input(&[(0, 36, 0)], cx);
        ws.steps_input(&[(0, 36, 80), (0, 50, 70)], cx);
        ws.steps_input(&[(0, 36, 0), (0, 50, 0)], cx);
    });
    let p = shown(&ws, cx);
    assert_eq!(&p.rows[0].cells[..3], [90, 80, 0]);
    let tom = p
        .rows
        .iter()
        .position(|r| r.pitch == 50)
        .expect("a row for 50");
    assert!(p.rows[tom].on(1));
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.steps.cursor.map(|c| c.1)),
        Some(2)
    );
    // Learn (Assign): the picked row becomes the note played.
    ws.update(cx, |ws, cx| {
        ws.steps.midi_in = false;
        ws.steps.row = 4;
        ws.steps_op(StepsOp::Learn(Some(Learn::Assign)), cx);
        ws.steps_input(&[(0, 56, 100)], cx);
    });
    assert_eq!(shown(&ws, cx).rows[4].pitch, 56);
    assert!(ws.read_with(cx, |ws, _| ws.steps.learn.is_none()));
}

#[gpui::test]
fn shortcuts_edit_modes_rows_and_the_pattern(cx: &mut TestAppContext) {
    let (ws, cx, _) = drums_pattern(cx);
    click_on(cx, "step-0-0", Modifiers::none(), MouseButton::Left);
    cx.simulate_keystrokes("ctrl-alt-g");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.steps.mode),
        EditMode::Lane(Lane::Gate)
    );
    cx.simulate_keystrokes("ctrl-alt-s");
    assert_eq!(ws.read_with(cx, |ws, _| ws.steps.mode), EditMode::OnOff);
    // ⌘D: the kick again on the next sound; ⌥↑ everything a semitone up.
    let rows = shown(&ws, cx).rows.len();
    ws.update(cx, |ws, _| ws.steps.row = 0);
    cx.simulate_keystrokes("cmd-d");
    let p = shown(&ws, cx);
    assert_eq!(p.rows.len(), rows + 1);
    assert_eq!(p.rows[1].cells, p.rows[0].cells);
    let pitches: Vec<u8> = p.rows.iter().map(|r| r.pitch).collect();
    cx.simulate_keystrokes("alt-up");
    let up: Vec<u8> = shown(&ws, cx).rows.iter().map(|r| r.pitch).collect();
    assert_eq!(up, pitches.iter().map(|p| p + 1).collect::<Vec<_>>());
    // Converted to MIDI (the piano roll, with what it played), and back.
    let at = ws.read_with(cx, |ws, _| ws.pattern().unwrap());
    ws.update(cx, |ws, cx| ws.pattern_to_midi(at, cx));
    let (bottom, notes) = ws.read_with(cx, |ws, _| {
        (
            ws.bottom,
            ws.session
                .project
                .region(at)
                .content
                .notes(0)
                .map(|n| n.len()),
        )
    });
    assert_eq!(bottom, Some(Bottom::PianoRoll));
    assert!(notes.unwrap() > 0);
    ws.update(cx, |ws, cx| ws.midi_to_pattern(at, cx));
    assert!(ws.read_with(cx, |ws, _| matches!(
        ws.session.project.region(at).content,
        Content::Pattern(_)
    )));
}

#[gpui::test]
fn a_pattern_on_a_synth_has_rows_of_notes_in_the_key(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| ws.new_pattern(cx));
    cx.run_until_parked();
    let rows = shown(&ws, cx)
        .rows
        .iter()
        .map(|r| r.pitch)
        .collect::<Vec<_>>();
    let key = ws
        .read_with(cx, |ws, _| ws.session.project.song_key())
        .unwrap();
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().all(|&p| key.contains(p)), "{rows:?}");
    click_on(cx, "steps-add-row", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Notes");
    let first = ws.read_with(cx, |ws, _| ws.menu.as_ref().unwrap().items[0].0.clone());
    assert!(!first.contains("Kick"), "{first}");
    // Melodic: a row of its own notes.
    ws.update(cx, |ws, cx| {
        ws.menu = None;
        cx.notify();
    });
    click_on(cx, "steps-add-row", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Melodic");
    let last: PatternRow = shown(&ws, cx).rows.last().unwrap().clone();
    assert!(last.melodic);
}
