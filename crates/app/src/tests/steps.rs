//! The step sequencer.

use super::*;

#[gpui::test]
fn the_step_sequencer_paints_accents_mutes_fills_and_converts(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    click_on(cx, "new-track", Modifiers::none(), MouseButton::Left);
    click_on(cx, "adder-Drums", Modifiers::none(), MouseButton::Left);
    click_on(cx, "adder-row-0", Modifiers::none(), MouseButton::Left); // Pina Studio's Drum Kit
    let track = ws.read_with(cx, |ws, _| ws.track);
    // Two patterns from the same playhead: the second goes after the first, not on it.
    cx.simulate_keystrokes("cmd-alt-p");
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
    let rows = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            match &ws.session.project.region(ws.pattern().unwrap()).content {
                Content::Pattern { rows, .. } => rows.clone(),
                _ => panic!(),
            }
        })
    };
    // Painting the kick row across four steps, then rubbing two out.
    let from = cx.debug_bounds("step-0-0").unwrap().center();
    let to = cx.debug_bounds("step-0-3").unwrap().center();
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    for k in 1..=3 {
        let at = cx
            .debug_bounds(Box::leak(format!("step-0-{k}").into_boxed_str()))
            .unwrap()
            .center();
        cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
    }
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(&rows(cx)[0].cells[..5], [100, 100, 100, 100, 0]);
    let a = cx.debug_bounds("step-0-2").unwrap().center();
    let b = cx.debug_bounds("step-0-3").unwrap().center();
    drag(cx, a, b);
    assert_eq!(&rows(cx)[0].cells[..5], [100, 100, 0, 0, 0]);
    // ⌥: an accent, and ⌥ again takes it off.
    let alt = Modifiers {
        alt: true,
        ..Modifiers::none()
    };
    click_on(cx, "step-1-4", alt, MouseButton::Left);
    assert_eq!(rows(cx)[1].cells[4], 127);
    click_on(cx, "step-1-4", alt, MouseButton::Left);
    assert_eq!(rows(cx)[1].cells[4], 100);
    // The hat row muted; filled every 2 steps from its menu; the clap three steps long.
    click_on(cx, "row-mute-2", Modifiers::none(), MouseButton::Left);
    assert!(rows(cx)[2].mute);
    ws.update(cx, |ws, cx| {
        let at = ws.pattern().unwrap();
        ws.row_op(at, 2, crate::editors::steps::RowOp::Fill(2), cx);
        ws.row_op(at, 4, crate::editors::steps::RowOp::Length(3), cx);
    });
    let r = rows(cx);
    assert_eq!(&r[2].cells[..4], [100, 0, 100, 0]);
    assert_eq!(r[4].cells.len(), 3);
    // The kick's velocities drawn low in the lane (the kick picked by clicking it).
    click_on(cx, "step-0-1", Modifiers::none(), MouseButton::Left); // off
    click_on(cx, "step-0-1", Modifiers::none(), MouseButton::Left); // on again, kick picked
    let lane = cx.debug_bounds("steps-lane").unwrap();
    let bar = cx.debug_bounds("step-velocity-0").unwrap().center();
    let low = point(bar.x, lane.origin.y + lane.size.height - px(12.));
    drag(cx, low, low);
    let kick = rows(cx)[0].cells[0];
    assert!(kick > 0 && kick < 40, "{kick}");
    // 32 steps: the bar's groove twice.
    click_on(cx, "steps-32", Modifiers::none(), MouseButton::Left);
    let r = rows(cx);
    assert_eq!(r[0].cells.len(), 32);
    assert_eq!(r[0].cells[16..18], r[0].cells[0..2]);
    // To MIDI: the piano roll, with the notes it played.
    click_on(cx, "pattern-to-midi", Modifiers::none(), MouseButton::Left);
    let (bottom, notes) = ws.read_with(cx, |ws, _| {
        let at = ws.region.unwrap();
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
    // And back: a MIDI region made a pattern.
    let at = ws.read_with(cx, |ws, _| ws.region.unwrap());
    ws.update(cx, |ws, cx| ws.midi_to_pattern(at, cx));
    assert!(ws.read_with(cx, |ws, _| matches!(
        ws.session.project.region(at).content,
        Content::Pattern { .. }
    )));
}

#[gpui::test]
fn a_drum_track_and_a_step_pattern(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    click_on(cx, "new-track", Modifiers::none(), MouseButton::Left);
    click_on(cx, "adder-Drums", Modifiers::none(), MouseButton::Left);
    click_on(cx, "adder-row-0", Modifiers::none(), MouseButton::Left); // Pina Studio's Drum Kit
    let (track, instrument) = ws.read_with(cx, |ws, _| {
        (
            ws.track,
            ws.session.project.tracks[ws.track].instrument.clone(),
        )
    });
    assert_eq!(
        instrument,
        Some(pina_engine::song::project::Instrument::Drums)
    );

    cx.simulate_keystrokes("cmd-alt-p");
    assert_eq!(ws.read_with(cx, |ws, _| ws.bottom), Some(Bottom::Steps));
    click_on(cx, "step-0-0", Modifiers::none(), MouseButton::Left); // kick on 1
    click_on(cx, "step-0-8", Modifiers::none(), MouseButton::Left); // and 3
    click_on(
        cx,
        "step-1-4",
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
        MouseButton::Left,
    ); // snare accent
    click_on(cx, "pad-5", Modifiers::none(), MouseButton::Left); // the first empty pad: + a sound
    click_on(cx, "menu-9", Modifiers::none(), MouseButton::Left); // Crash
    let rows = ws.read_with(cx, |ws, _| {
        match &ws.session.project.tracks[track].regions[0].content {
            Content::Pattern { rows, .. } => rows.clone(),
            _ => panic!("not a pattern"),
        }
    });
    assert_eq!(
        (rows[0].pitch, rows[0].cells[0], rows[0].cells[8]),
        (36, 100, 100)
    );
    assert_eq!((rows[1].pitch, rows[1].cells[4]), (38, 127));
    assert_eq!((rows.len(), rows[5].pitch), (6, 49));

    // A pad picks its sound; the bar under the grid does things to it: every 4, then Euclid's
    // three hits over sixteen.
    let row = |cx: &VisualTestContext, r: usize| {
        ws.read_with(cx, |ws, _| {
            match &ws.session.project.tracks[track].regions[0].content {
                Content::Pattern { rows, .. } => rows[r].cells.clone(),
                _ => panic!("not a pattern"),
            }
        })
    };
    click_on(cx, "pad-2", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.steps_row), 2);
    click_on(cx, "row-op-fill-4", Modifiers::none(), MouseButton::Left);
    assert_eq!(&row(cx, 2)[..8], [100, 0, 0, 0, 100, 0, 0, 0]);
    click_on(cx, "row-op-euclid", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "3 hits");
    let hits: Vec<usize> = (0..16).filter(|&k| row(cx, 2)[k] > 0).collect();
    assert_eq!(hits, [0, 6, 11]);

    // Each step's own settings, in the hats' lane: the first step's chance drawn low, its pitch
    // high.
    let pattern = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            match &ws.session.project.tracks[track].regions[0].content {
                Content::Pattern { rows, .. } => rows[2].clone(),
                _ => panic!("not a pattern"),
            }
        })
    };
    let draw = |cx: &mut VisualTestContext, lane: &'static str, high: bool| {
        click_on(cx, lane, Modifiers::none(), MouseButton::Left);
        let bar = cx.debug_bounds("step-velocity-0").unwrap();
        let y = if high {
            bar.top() + px(3.)
        } else {
            bar.bottom() - px(3.)
        };
        let at = point(bar.center().x, y);
        drag(cx, at, at);
    };
    draw(cx, "lane-Chance", false);
    draw(cx, "lane-Pitch", true);
    let hats = pattern(cx);
    assert!(hats.chance_at(0) <= 10, "{}", hats.chance_at(0));
    assert!(hats.transpose_at(0) >= 10, "{}", hats.transpose_at(0));
    assert_eq!(hats.chance_at(6), 100); // the rest as usual

    // An LFO from the strip above the grid, dragged onto the hats' velocity lane.
    click_on(cx, "lane-Velocity", Modifiers::none(), MouseButton::Left);
    click_on(cx, "steps-add-LFO", Modifiers::none(), MouseButton::Left);
    let chip = cx.debug_bounds("steps-mod-0").unwrap().center();
    let lane = cx.debug_bounds("steps-lane").unwrap().center();
    drag(cx, chip, lane);
    let routes = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[track].modulators[0]
            .routes
            .iter()
            .map(|r| r.target)
            .collect::<Vec<_>>()
    });
    use pina_engine::dsp::modulate::{ModKind, ModTarget, SoundParam};
    assert_eq!(routes, [ModTarget::Sound(2, SoundParam::Velocity)]);

    // A parameter row: mapped to the volume from its menu, its first step painted low.
    click_on(cx, "add-param", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Volume");
    let (kind, target) = ws.read_with(cx, |ws, _| {
        let m = &ws.session.project.tracks[track].modulators[1];
        (m.kind.clone(), m.routes.first().map(|r| r.target))
    });
    assert!(matches!(kind, ModKind::Steps { .. }));
    assert_eq!(target, Some(ModTarget::Volume));
    // (A taller editor, so the row's in sight.)
    ws.update(cx, |ws, cx| {
        ws.editor_h = 620.0;
        cx.notify();
    });
    cx.run_until_parked();
    let cells = cx.debug_bounds("param-cells-1").unwrap();
    let low = point(cells.left() + px(4.), cells.bottom() - px(1.));
    drag(cx, low, low);
    let first = ws.read_with(cx, |ws, _| {
        match &ws.session.project.tracks[track].modulators[1].kind {
            ModKind::Steps { values, .. } => values[0],
            _ => 1.0,
        }
    });
    assert!(first < 0.1, "{first}");
}

#[gpui::test]
fn a_pattern_on_a_synth_has_rows_of_notes_in_the_key(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| ws.new_pattern(cx));
    let rows = ws.read_with(cx, |ws, _| {
        let at = ws.region.unwrap();
        match &ws.session.project.region(at).content {
            Content::Pattern { rows, .. } => rows.iter().map(|r| r.pitch).collect::<Vec<_>>(),
            _ => unreachable!(),
        }
    });
    let key = ws
        .read_with(cx, |ws, _| ws.session.project.song_key())
        .unwrap();
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().all(|&p| key.contains(p)), "{rows:?}");
    cx.run_until_parked();
    click_on(cx, "pad-4", Modifiers::none(), MouseButton::Left); // the first empty pad
    let first = ws.read_with(cx, |ws, _| ws.menu.as_ref().unwrap().items[0].0.clone());
    assert!(!first.contains("Kick"), "{first}");
}
