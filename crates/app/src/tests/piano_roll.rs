//! The piano roll: writing, picking, moving and editing notes.

use super::*;

#[gpui::test]
fn the_piano_roll_adds_moves_and_deletes_notes(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_click(spot(&ws, cx, PPQ, 0), Modifiers::none()); // the Keys region
    cx.simulate_keystrokes("p");
    cx.run_until_parked();
    let before = keys_notes(&ws, cx, 0).len();

    // Double-click empty grid: a beat-long note there.
    let spot_at = roll_spot(&ws, cx, PPQ * 2 + PPQ / 2, 70);
    double_click(cx, spot_at);
    let notes = keys_notes(&ws, cx, 0);
    assert_eq!(notes.len(), before + 1);
    assert_eq!(notes[before], (PPQ as i64 * 2, 70));

    // Drag it a beat later and a semitone up.
    let from = roll_spot(&ws, cx, PPQ * 2 + PPQ / 2, 70);
    drag(cx, from, from + point(px(48.), px(-KEY_H)));
    assert_eq!(keys_notes(&ws, cx, 0)[before], (PPQ as i64 * 3, 71));

    // Delete deletes the selected note, not the region.
    cx.simulate_keystrokes("backspace");
    assert_eq!(keys_notes(&ws, cx, 0).len(), before);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks[0].regions.len()),
        1
    );
}

#[gpui::test]
fn the_piano_roll_picks_moves_copies_and_pastes_notes_together(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_click(spot(&ws, cx, PPQ, 0), Modifiers::none()); // the Keys region
    cx.simulate_keystrokes("p");
    cx.run_until_parked();
    let notes = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            match &ws.session.project.tracks[0].regions[0].content {
                Content::Midi { notes, .. } => notes.clone(),
                _ => unreachable!(),
            }
        })
    };
    let picked = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            let mut p = ws.picked.clone();
            p.sort_unstable();
            p
        })
    };
    let before = notes(cx);
    // ⌘A picks them all; ↑ moves them a semitone, ⇧↓ an octave down; each undoes.
    cx.simulate_keystrokes("cmd-a");
    assert_eq!(picked(cx).len(), before.len());
    cx.simulate_keystrokes("up");
    assert!(
        notes(cx)
            .iter()
            .zip(&before)
            .all(|(a, b)| a.pitch == b.pitch + 1)
    );
    cx.simulate_keystrokes("shift-down");
    assert!(
        notes(cx)
            .iter()
            .zip(&before)
            .all(|(a, b)| a.pitch + 11 == b.pitch)
    );
    cx.simulate_keystrokes("cmd-z cmd-z");
    assert_eq!(notes(cx), before);
    // Escape lets go; a rubber band round the first bar's notes picks them.
    cx.simulate_keystrokes("escape");
    assert!(picked(cx).is_empty());
    let first_bar: Vec<usize> = (0..before.len())
        .filter(|&i| before[i].start < BAR as i64)
        .collect();
    let lo = first_bar.iter().map(|&i| before[i].pitch).min().unwrap();
    let hi = first_bar.iter().map(|&i| before[i].pitch).max().unwrap();
    ws.update(cx, |ws, cx| {
        ws.roll.key_h = 6.0;
        ws.roll_scroll_y = ((127 - hi) as f32 * 6.0 - 12.0).max(0.0);
        cx.notify();
    });
    cx.run_until_parked();
    let (from, to) = ws.read_with(cx, |ws, _| {
        let x = |t: u64| {
            f32::from(ws.roll_bounds.get().origin.x) + t as f32 * ws.px_per_tick() - ws.scroll_x
        };
        (
            point(px(x(0) + 1.0), px(ws.pitch_y(hi + 1) + 2.0)),
            point(px(x(BAR) - 4.0), px(ws.pitch_y(lo - 1) + 2.0)),
        )
    });
    drag(cx, from, to);
    assert_eq!(picked(cx), first_bar);
    // Copied, and pasted at the playhead: bar 3.
    ws.update(cx, |ws, cx| ws.locate_tick(BAR * 2, cx));
    cx.simulate_keystrokes("cmd-c cmd-v");
    let after = notes(cx);
    assert_eq!(after.len(), before.len() + first_bar.len());
    assert!(
        picked(cx)
            .iter()
            .all(|&i| after[i].start >= (BAR * 2) as i64)
    );
    // ⌘D: the pasted notes again, after themselves; ⌫ deletes them.
    cx.simulate_keystrokes("cmd-d");
    assert_eq!(notes(cx).len(), before.len() + 2 * first_bar.len());
    cx.simulate_keystrokes("backspace");
    assert_eq!(notes(cx).len(), before.len() + first_bar.len());
    // ⌥-dragging a note copies it.
    cx.simulate_keystrokes("escape");
    let at = cx.debug_bounds("note-0").unwrap().center();
    drag_with(cx, at, at + point(px(0.), px(-12.)), Modifiers::alt());
    let copied = notes(cx);
    assert_eq!(copied.len(), before.len() + first_bar.len() + 1);
    assert_eq!(copied[0], before[0]);
    assert_eq!(copied.last().unwrap().pitch, before[0].pitch + 2);
}

#[gpui::test]
fn the_pencil_writes_chords_in_key_paints_on_and_the_lane_draws(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_click(spot(&ws, cx, PPQ, 0), Modifiers::none()); // the Keys region
    cx.simulate_keystrokes("p");
    cx.run_until_parked();
    ws.update(cx, |ws, cx| {
        ws.session.project.key = Some("C major".into());
        ws.roll.stamp = crate::editors::pianoroll::Stamp::Triad;
        ws.roll.length = PPQ;
        ws.tool = Tool::Pencil;
        cx.notify();
    });
    let count = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            ws.session
                .project
                .region((0, 0))
                .content
                .notes(0)
                .unwrap()
                .len()
        })
    };
    let before = count(cx);
    // A triad on D in C major is D minor; dragged on two beats, two more of them.
    let start = roll_spot(&ws, cx, BAR * 2 + 10, 62);
    let beat = ws.read_with(cx, |ws, _| ws.px_per_tick() * PPQ as f32);
    drag(cx, start, start + point(px(beat * 2.5), px(0.)));
    let written: Vec<(i64, u8)> = ws.read_with(cx, |ws, _| {
        let notes = ws
            .session
            .project
            .region((0, 0))
            .content
            .notes(0)
            .unwrap()
            .to_vec();
        notes[before..].iter().map(|n| (n.start, n.pitch)).collect()
    });
    let b2 = (BAR * 2) as i64;
    let p = PPQ as i64;
    assert_eq!(
        written,
        [
            (b2, 62),
            (b2, 65),
            (b2, 69),
            (b2 + p, 62),
            (b2 + p, 65),
            (b2 + p, 69),
            (b2 + 2 * p, 62),
            (b2 + 2 * p, 65),
            (b2 + 2 * p, 69)
        ]
    );
    // Drawn across the lane near its top, the picked (written) notes go loud.
    let lane = cx.debug_bounds("roll-lane").unwrap();
    let x = |t: u64| {
        ws.read_with(cx, |ws, _| {
            f32::from(ws.roll_bounds.get().origin.x) + t as f32 * ws.px_per_tick() - ws.scroll_x
        })
    };
    let (x0, x1) = (x(BAR * 2) - 2.0, x(BAR * 2 + 3 * PPQ));
    let top = f32::from(lane.origin.y) + 10.0;
    drag(cx, point(px(x0), px(top)), point(px(x1), px(top)));
    let loud = ws.read_with(cx, |ws, _| {
        ws.session.project.region((0, 0)).content.notes(0).unwrap()[before..]
            .iter()
            .all(|n| n.velocity > 115)
    });
    assert!(loud);
    // The mod wheel's lane: drawn, it holds moves of controller 1.
    click_on(cx, "lane-Mod", Modifiers::none(), MouseButton::Left);
    let mid = f32::from(lane.origin.y) + f32::from(lane.size.height) / 2.0;
    drag(cx, point(px(x0 + 4.0), px(mid)), point(px(x1), px(top)));
    let moves = ws.read_with(cx, |ws, _| {
        match &ws.session.project.region((0, 0)).content {
            Content::Midi { controls, .. } => controls
                .iter()
                .filter(|c| {
                    matches!(
                        c.kind,
                        pina_engine::song::project::ControlValue::Cc { number: 1, .. }
                    )
                })
                .count(),
            _ => 0,
        }
    });
    assert!(moves >= 2, "{moves}");
}

#[gpui::test]
fn velocities_drag_and_the_roll_tools_edit_notes(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    // The editor dragged taller, its whole panel in sight.
    ws.update(cx, |ws, _| ws.editor_h = 460.0);
    cx.simulate_click(spot(&ws, cx, PPQ, 1), Modifiers::none()); // the Lead region
    cx.simulate_keystrokes("p");
    cx.run_until_parked();
    let notes = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            ws.session
                .project
                .region((1, 0))
                .content
                .notes(0)
                .unwrap()
                .to_vec()
        })
    };
    let before = notes(cx)[0].velocity;
    let dot = cx.debug_bounds("velocity-0").unwrap().center();
    drag(cx, dot, dot + point(px(0.), px(10.)));
    let after = notes(cx)[0].velocity;
    assert!(after < before, "{before} {after}");
    // Legato, from the Transform tab, holds each note to the next.
    click_on(
        cx,
        "roll-panel-Transform",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(cx, "roll-Legato", Modifiers::none(), MouseButton::Left);
    let n = notes(cx);
    assert_eq!(n[0].start + n[0].length as i64, n[1].start);
}

#[gpui::test]
fn the_notes_panel_changes_every_picked_note_and_mutes_them(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    // The editor dragged taller, its whole panel in sight.
    ws.update(cx, |ws, _| ws.editor_h = 460.0);
    cx.simulate_click(spot(&ws, cx, PPQ, 0), Modifiers::none()); // the Keys region
    cx.simulate_keystrokes("p");
    cx.run_until_parked();
    let notes = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            ws.session
                .project
                .region((0, 0))
                .content
                .notes(0)
                .unwrap()
                .to_vec()
        })
    };
    let picked = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            let mut p = ws.picked.clone();
            p.sort_unstable();
            p
        })
    };
    let before = notes(cx);
    // The top note of each chord, then the rest.
    click_on(cx, "roll-pick-Top", Modifiers::none(), MouseButton::Left);
    let tops = picked(cx);
    assert!(!tops.is_empty() && tops.len() < before.len());
    for &i in &tops {
        let n = &before[i];
        assert!(
            before
                .iter()
                .all(|o| o.start != n.start || o.pitch <= n.pitch)
        );
    }
    click_on(cx, "roll-pick-Invert", Modifiers::none(), MouseButton::Left);
    assert_eq!(picked(cx).len(), before.len() - tops.len());

    // A step up in velocity, for every picked note; one undo takes it back.
    cx.simulate_keystrokes("cmd-a");
    click_on(cx, "note-Velocity-up", Modifiers::none(), MouseButton::Left);
    let up = notes(cx);
    assert!(
        up.iter()
            .zip(&before)
            .all(|(a, b)| a.velocity == (b.velocity + 1).min(127))
    );
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(notes(cx), before);

    // Chance dragged down, for all of them at once.
    let field = cx.debug_bounds("note-field-Chance").unwrap().center();
    drag(cx, field, field + point(px(0.), px(30.)));
    let less = notes(cx);
    assert!(
        less.iter()
            .all(|n| n.chance < 100 && n.chance == less[0].chance),
        "{:?}",
        less[0]
    );

    // 0 mutes them, and again plays them.
    cx.simulate_keystrokes("0");
    assert!(notes(cx).iter().all(|n| n.muted));
    cx.simulate_keystrokes("0");
    assert!(notes(cx).iter().all(|n| !n.muted));
}

#[gpui::test]
fn the_roll_writes_rhythms_and_chords_and_names_what_it_hears(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, _| ws.editor_h = 560.0);
    cx.simulate_click(spot(&ws, cx, PPQ, 0), Modifiers::none()); // the Keys region
    cx.simulate_keystrokes("p");
    cx.run_until_parked();
    let notes = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            ws.session
                .project
                .region((0, 0))
                .content
                .notes(0)
                .unwrap()
                .to_vec()
        })
    };
    let before = notes(cx);
    let (length, grid) = ws.read_with(cx, |ws, _| {
        (ws.session.project.region((0, 0)).length, ws.grid())
    });
    // 3 hits in every 8 steps, over the region, in place of its notes.
    click_on(
        cx,
        "roll-panel-Generate",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(
        cx,
        "write-Write rhythm",
        Modifiers::none(),
        MouseButton::Left,
    );
    let hits = notes(cx);
    assert_eq!(hits.len() as u64, length / grid / 8 * 3);
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(notes(cx), before);

    // C, then A minor, played as picked: written, and read back over the ruler.
    click_on(
        cx,
        "roll-panel-Chords",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(cx, "harmony-chord-0", Modifiers::none(), MouseButton::Left);
    click_on(cx, "harmony-chord-5", Modifiers::none(), MouseButton::Left);
    click_on(
        cx,
        "write-Write the chords",
        Modifiers::none(),
        MouseButton::Left,
    );
    let chords = notes(cx);
    assert_eq!(chords.len(), 6);
    let named = ws.read_with(cx, |ws, _| {
        pina_engine::music::harmony::read_chords(&chords, (0, length as i64), PPQ)
            .iter()
            .map(|c| c.2.name_in(ws.session.project.song_key().as_ref()))
            .collect::<Vec<_>>()
    });
    assert_eq!(named, ["C", "Am"]);
    assert!(
        cx.debug_bounds("roll-chord-1").is_some(),
        "the ruler names them"
    );
}

#[gpui::test]
fn the_piano_roll_ruler_moves_the_playhead(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_click(spot(&ws, cx, PPQ, 0), Modifiers::none());
    cx.simulate_keystrokes("p");
    let ruler = cx.debug_bounds("roll-ruler").unwrap();
    let x = ws.read_with(cx, |ws, _| {
        f32::from(ws.roll_bounds.get().origin.x) + (BAR * 2) as f32 * ws.px_per_tick() - ws.scroll_x
    });
    cx.simulate_click(point(px(x + 2.), ruler.center().y), Modifiers::none());
    until(cx, &ws, |ws| ws.playhead() == BAR * 2);
}

#[gpui::test]
fn generate_and_chords_show_one_thing_at_a_time_and_write_from_their_foot(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, _| ws.editor_h = 560.0);
    cx.simulate_click(spot(&ws, cx, PPQ, 0), Modifiers::none()); // the Keys region
    cx.simulate_keystrokes("p");
    cx.run_until_parked();
    let notes = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            ws.session
                .project
                .region((0, 0))
                .content
                .notes(0)
                .unwrap()
                .to_vec()
        })
    };
    let writing = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.roll_writing.clone());
    let click = |cx: &mut VisualTestContext, name: &'static str| {
        click_on(cx, name, Modifiers::none(), MouseButton::Left)
    };
    // Generate: a known rhythm picked by name; then a melody, and a shape, each its own writer.
    click(cx, "roll-panel-Generate");
    click(cx, "write-rhythm-Cinquillo");
    assert_eq!((writing(cx).pulses, writing(cx).steps), (5, 8));
    assert!(
        cx.debug_bounds("write-shape-Arch").is_none(),
        "one writer at a time"
    );
    click(cx, "write-tab-Melody");
    click(cx, "write-Grow a melody");
    let grown = notes(cx);
    let (low, high) = (writing(cx).low, writing(cx).high);
    assert!(!grown.is_empty() && grown.iter().all(|n| (low..=high).contains(&n.pitch)));
    click(cx, "write-tab-Shape");
    click(cx, "write-shape-Rise");
    click(cx, "write-add");
    click(cx, "write-Write the shape");
    let rose = notes(cx);
    assert!(rose.len() > grown.len(), "added to what was there");
    // Chords: a progression from a playbook (the house book's own), heard first (it isn't taken),
    // then taken; played as an arpeggio from its menu, a bass under it, written.
    click(cx, "roll-panel-Chords");
    click(cx, "write-tab-Progressions");
    click(cx, "harmony-books");
    pick(cx, &ws, "House");
    assert_eq!(writing(cx).book.as_deref(), Some("house"));
    click(cx, "harmony-hear-0");
    assert!(writing(cx).chords.is_empty());
    click(cx, "harmony-progression-0");
    let taken = writing(cx).chords.len();
    assert!(taken >= 3);
    click(cx, "harmony-play");
    pick(cx, &ws, "Arp");
    assert_eq!(
        writing(cx).performance,
        pina_engine::music::harmony::Performance::Arp
    );
    click(cx, "harmony-bass");
    click(cx, "write-replace");
    click(cx, "write-Write the chords");
    let written = notes(cx);
    assert!(written.len() > taken * 4, "arpeggiated, with a bass");
    // Taken out one at a time, or all.
    click(cx, "harmony-picked-0");
    assert_eq!(writing(cx).chords.len(), taken - 1);
    click(cx, "harmony-clear");
    assert!(writing(cx).chords.is_empty());
}
