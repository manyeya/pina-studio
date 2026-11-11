//! Each note's own expression (MPE) in the piano roll: drawn, its points added, moved, bowed,
//! scaled and taken out; bent over the notes, and carried as they move; shapes from the MPE tab;
//! release velocities; and a controller set apart in Settings.

use pina_engine::song::project::{Dim, Note};

use super::*;
use crate::editors::pianoroll::{VELOCITY_H, lane_level, lane_y};

/// The demo with the Lead region's notes in the roll, the editor tall enough for its lanes.
fn lead(cx: &mut TestAppContext) -> (Entity<Workspace>, &mut VisualTestContext) {
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, _| ws.editor_h = 560.0);
    cx.simulate_click(spot(&ws, cx, PPQ, 1), Modifiers::none());
    cx.simulate_keystrokes("p");
    cx.run_until_parked();
    (ws, cx)
}

fn notes(ws: &Entity<Workspace>, cx: &VisualTestContext) -> Vec<Note> {
    ws.read_with(cx, |ws, _| {
        ws.session
            .project
            .region((1, 0))
            .content
            .notes(0)
            .unwrap()
            .to_vec()
    })
}

/// The window x of a tick into the Lead region, and the y in the lane of a value.
fn x_at(ws: &Entity<Workspace>, cx: &mut VisualTestContext, tick: i64, pitch: u8) -> Pixels {
    let start = ws.read_with(cx, |ws, _| ws.session.project.region((1, 0)).start as i64);
    roll_spot(ws, cx, (start + tick).max(0) as u64, pitch).x
}

fn lane_at(ws: &Entity<Workspace>, cx: &VisualTestContext, dim: Dim, v: f32) -> Pixels {
    ws.read_with(cx, |ws, _| {
        px(lane_y(ws.lane_bounds.get(), lane_level(dim, v)))
    })
}

/// The roll's panel scrolled to its end.
fn scroll_panel(cx: &mut VisualTestContext) {
    let tools = cx.debug_bounds("roll-tools").unwrap().center();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: tools,
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-400.))),
        ..Default::default()
    });
    cx.run_until_parked();
}

fn values(n: &Note, dim: Dim) -> Vec<f32> {
    n.expression.get(dim).iter().map(|b| b.value).collect()
}

#[gpui::test]
fn a_notes_own_bend_is_drawn_in_its_lane_thinned_and_rubbed_out(cx: &mut TestAppContext) {
    let (ws, cx) = lead(cx);
    let n = notes(&ws, cx)[0].clone();
    click_on(cx, "lane-Pitch", Modifiers::none(), MouseButton::Left);
    let a = x_at(&ws, cx, n.start + n.length as i64 / 4, n.pitch);
    let b = x_at(&ws, cx, n.start + n.length as i64 * 3 / 4, n.pitch);
    // Across the first note at 7 semitones up: a held line, two points draw it.
    let y = lane_at(&ws, cx, Dim::Pitch, 7.0);
    drag(cx, point(a, y), point(b, y));
    let after = notes(&ws, cx);
    assert_eq!(values(&after[0], Dim::Pitch), [7.0, 7.0]);
    assert!(after[1].expression.is_empty());
    // ⌥ away from the line rubs it out.
    let rest = lane_at(&ws, cx, Dim::Pitch, 0.0);
    drag_with(
        cx,
        point(a - px(4.), rest),
        point(b + px(4.), rest),
        Modifiers::alt(),
    );
    assert!(notes(&ws, cx)[0].expression.pitch.is_empty());
}

#[gpui::test]
fn a_bends_points_are_added_moved_bowed_scaled_and_taken_out(cx: &mut TestAppContext) {
    let (ws, cx) = lead(cx);
    let n = notes(&ws, cx)[0].clone();
    click_on(cx, "lane-Pitch", Modifiers::none(), MouseButton::Left);
    let a = x_at(&ws, cx, n.start + n.length as i64 / 4, n.pitch);
    let b = x_at(&ws, cx, n.start + n.length as i64 * 3 / 4, n.pitch);
    let y = |cx: &VisualTestContext, v| lane_at(&ws, cx, Dim::Pitch, v);
    // A click puts a point; two, two.
    cx.simulate_click(point(a, y(cx, 2.0)), Modifiers::none());
    cx.simulate_click(point(b, y(cx, 4.0)), Modifiers::none());
    assert_eq!(values(&notes(&ws, cx)[0], Dim::Pitch), [2.0, 4.0]);
    // The first dragged up lands on a whole semitone.
    drag(cx, point(a, y(cx, 2.0)), point(a, y(cx, 5.2)));
    assert_eq!(values(&notes(&ws, cx)[0], Dim::Pitch), [5.0, 4.0]);
    // ⌥ on the line between bows it: up, a falling line stays high, then falls.
    let mid = (a + b) / 2.0;
    drag_with(
        cx,
        point(mid, y(cx, 4.5)),
        point(mid, y(cx, 4.5) - px(30.)),
        Modifiers::alt(),
    );
    let bowed = notes(&ws, cx)[0].expression.pitch[0].curve;
    assert!(bowed > 0.5, "{bowed}");
    // ⇧ scales the note's line: up a step, twice as far.
    let rest = y(cx, 0.0);
    drag_with(
        cx,
        point(mid, rest),
        point(mid, rest - px(60.)),
        Modifiers::shift(),
    );
    assert_eq!(values(&notes(&ws, cx)[0], Dim::Pitch), [10.0, 8.0]);
    // A double-click takes a point out.
    double_click(cx, point(b, y(cx, 8.0)));
    assert_eq!(values(&notes(&ws, cx)[0], Dim::Pitch), [10.0]);
    // One undo puts it back.
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(values(&notes(&ws, cx)[0], Dim::Pitch), [10.0, 8.0]);
}

#[gpui::test]
fn with_the_pitch_lane_a_note_bends_where_it_is_and_takes_its_bend_along(cx: &mut TestAppContext) {
    let (ws, cx) = lead(cx);
    let n = notes(&ws, cx)[0].clone();
    click_on(cx, "lane-Pitch", Modifiers::none(), MouseButton::Left);
    // From its middle to near its end, two rows up: it bends up a whole tone, and is picked.
    let start = ws.read_with(cx, |ws, _| ws.session.project.region((1, 0)).start as i64);
    let tick = |t: i64| (start + n.start + t) as u64;
    let from = roll_spot(&ws, cx, tick(n.length as i64 / 2), n.pitch);
    let key_h = ws.read_with(cx, |ws, _| ws.roll.key_h);
    let end = roll_spot(&ws, cx, tick(n.length as i64 * 7 / 8), n.pitch);
    drag(cx, from, point(end.x, from.y - px(2.0 * key_h)));
    let bent = notes(&ws, cx)[0].expression.pitch.clone();
    assert_eq!(bent.first().map(|b| b.value), Some(0.0));
    assert_eq!(bent.last().map(|b| b.value), Some(2.0));
    assert_eq!(ws.read_with(cx, |ws, _| ws.picked.clone()), [0]);
    // Moved a beat later, the Velocity lane showing: its bend goes with it.
    click_on(cx, "lane-Vel", Modifiers::none(), MouseButton::Left);
    let at = cx.debug_bounds("note-0").unwrap().center();
    let beat = ws.read_with(cx, |ws, _| PPQ as f32 * ws.px_per_tick());
    drag(cx, at, at + point(px(beat), px(0.)));
    let moved = &notes(&ws, cx)[0];
    assert_eq!(moved.start, n.start + PPQ as i64);
    assert_eq!(moved.expression.pitch, bent);
}

#[gpui::test]
fn the_mpe_tab_shapes_the_picked_notes_and_copies_one_notes_to_others(cx: &mut TestAppContext) {
    use pina_engine::song::project::{Format, Instrument, PluginRef};
    let (ws, cx) = lead(cx);
    click_on(cx, "roll-panel-MPE", Modifiers::none(), MouseButton::Left);
    // Nothing picked: a vibrato on every note, its lane showing.
    click_on(cx, "mpe-Vibrato", Modifiers::none(), MouseButton::Left);
    assert!(
        notes(&ws, cx)
            .iter()
            .all(|n| !n.expression.pitch.is_empty())
    );
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.roll.lane),
        crate::editors::pianoroll::Lane::Pitch
    );
    click_on(cx, "mpe-clear-Pitch", Modifiers::none(), MouseButton::Left);
    assert!(notes(&ws, cx).iter().all(|n| n.expression.is_empty()));
    // The first picked: a swell on it alone.
    ws.update(cx, |ws, cx| {
        ws.picked = vec![0];
        cx.notify();
    });
    scroll_panel(cx);
    click_on(cx, "mpe-Swell", Modifiers::none(), MouseButton::Left);
    let after = notes(&ws, cx);
    assert!(!after[0].expression.press.is_empty());
    assert!(after[1..].iter().all(|n| n.expression.is_empty()));
    // Copied onto the second, stretched to its length.
    click_on(cx, "mpe-Copy", Modifiers::none(), MouseButton::Left);
    ws.update(cx, |ws, cx| {
        ws.picked = vec![1];
        cx.notify();
    });
    click_on(cx, "mpe-Paste", Modifiers::none(), MouseButton::Left);
    let after = notes(&ws, cx);
    assert_eq!(values(&after[1], Dim::Press), values(&after[0], Dim::Press));
    let last = |n: &Note| n.expression.press.last().unwrap().at as f64 / n.length as f64;
    assert!((last(&after[1]) - last(&after[0])).abs() < 0.01);
    // More of the next one.
    click_on(cx, "mpe-Amount-+", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.roll.gesture_amount), 110);
    // An Audio Unit hears none of it until MPE is on: the tab turns it on.
    ws.update(cx, |ws, cx| {
        ws.session.project.tracks[1].instrument = Some(Instrument::Plugin(PluginRef {
            format: Format::Au,
            class: "Keys".into(),
            uid: Some("aumu keys pina".into()),
            ..Default::default()
        }));
        cx.notify();
    });
    cx.run_until_parked();
    click_on(
        cx,
        "mpe-tab-Turn on MPE",
        Modifiers::none(),
        MouseButton::Left,
    );
    let on = ws.read_with(cx, |ws, _| match &ws.session.project.tracks[1].instrument {
        Some(Instrument::Plugin(r)) => r.mpe,
        _ => false,
    });
    assert!(on);
}

#[gpui::test]
fn release_velocities_are_drawn_in_their_lane_and_stepped_in_the_notes_panel(
    cx: &mut TestAppContext,
) {
    let (ws, cx) = lead(cx);
    click_on(cx, "lane-Rel", Modifiers::none(), MouseButton::Left);
    let lane = cx.debug_bounds("roll-lane").unwrap();
    let all = notes(&ws, cx);
    let (first, last) = (all[0].start, all.iter().map(|n| n.start).max().unwrap());
    let a = x_at(&ws, cx, first, all[0].pitch);
    let b = x_at(&ws, cx, last, all[0].pitch) + px(4.);
    // A quarter of the way up: every note let go at 32.
    let y = lane.origin.y + lane.size.height - px(5.0 + VELOCITY_H / 4.0);
    drag(cx, point(a, y), point(b, y));
    assert!(
        notes(&ws, cx).iter().all(|n| n.release == 32),
        "{:?}",
        notes(&ws, cx).iter().map(|n| n.release).collect::<Vec<_>>()
    );
    // The Notes tab steps it for every picked note.
    click_on(cx, "roll-panel-Notes", Modifiers::none(), MouseButton::Left);
    cx.simulate_keystrokes("cmd-a");
    click_on(cx, "note-Release-up", Modifiers::none(), MouseButton::Left);
    assert!(notes(&ws, cx).iter().all(|n| n.release == 33));
}

#[gpui::test]
fn an_mpe_controller_is_set_apart_in_settings(cx: &mut TestAppContext) {
    use pina_engine::audio::midi::InputSetup;
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, _| ws.player.list_midi_input("MPE Controller"));
    cx.simulate_keystrokes("cmd-,");
    click_on(cx, "settings-MIDI", Modifiers::none(), MouseButton::Left);
    // Its notes bend 24 semitones, here and next time.
    click_on(
        cx,
        "setting-midi-bend-24",
        Modifiers::none(),
        MouseButton::Left,
    );
    let read = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            (
                ws.player.midi_inputs()[0].setup,
                ws.settings
                    .midi_inputs
                    .first()
                    .map(|i| (i.name.clone(), i.setup)),
            )
        })
    };
    let set = InputSetup {
        mpe: true,
        bend: 24,
    };
    assert_eq!(read(cx), (set, Some(("MPE Controller".to_string(), set))));
    // MPE off: a plain keyboard, no bend to choose.
    click_on(cx, "setting-midi-mpe", Modifiers::none(), MouseButton::Left);
    assert!(!read(cx).0.mpe);
    assert!(cx.debug_bounds("setting-midi-bend-24").is_none());
}

#[gpui::test]
fn a_click_on_one_of_the_picked_notes_picks_it_alone_for_a_shape(cx: &mut TestAppContext) {
    let (ws, cx) = lead(cx);
    let picked = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.picked.clone());
    click_on(cx, "roll-panel-MPE", Modifiers::none(), MouseButton::Left);
    // Every note picked, then a click on one: that one alone.
    cx.simulate_keystrokes("cmd-a");
    let at = cx.debug_bounds("note-5").unwrap().center();
    cx.simulate_click(at, Modifiers::none());
    assert_eq!(picked(cx), [5]);
    click_on(cx, "mpe-Fall", Modifiers::none(), MouseButton::Left);
    let fell: Vec<usize> = notes(&ws, cx)
        .iter()
        .enumerate()
        .filter(|(_, n)| !n.expression.pitch.is_empty())
        .map(|(i, _)| i)
        .collect();
    assert_eq!(fell, [5]);
    // The same with the Pitch lane showing (where a note's middle bends it), and ⇧ still adds.
    cx.simulate_keystrokes("cmd-a");
    let at = cx.debug_bounds("note-2").unwrap().center();
    cx.simulate_click(at, Modifiers::none());
    assert_eq!(picked(cx), [2]);
    let at = cx.debug_bounds("note-7").unwrap().center();
    cx.simulate_click(at, Modifiers::shift());
    assert_eq!(picked(cx), [2, 7]);
    // A drag still moves them together.
    let before = notes(&ws, cx);
    click_on(cx, "lane-Vel", Modifiers::none(), MouseButton::Left);
    let at = cx.debug_bounds("note-2").unwrap().center();
    let beat = ws.read_with(cx, |ws, _| PPQ as f32 * ws.px_per_tick());
    drag(cx, at, at + point(px(beat), px(0.)));
    let after = notes(&ws, cx);
    assert_eq!(after[7].start, before[7].start + PPQ as i64);
    assert_eq!(picked(cx), [2, 7]);
}
