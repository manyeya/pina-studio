//! The timeline: tools, the cycle, markers, key and time signature, pattern mode, Live Loops and recording.

use super::*;

#[gpui::test]
fn the_tool_menu_and_its_tools_text_glue_solo_mute_zoom_fade_flex(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let region = |cx: &VisualTestContext, at: (usize, usize)| {
        ws.read_with(cx, |ws, _| ws.session.project.region(at).clone())
    };
    let use_tool = |cx: &mut VisualTestContext, name: &str| {
        cx.simulate_keystrokes("t");
        pick(cx, &ws, name);
    };
    // T: the Tool menu; Mute mutes a region with a click.
    use_tool(cx, "Mute");
    assert_eq!(ws.read_with(cx, |ws, _| ws.tool), Tool::Mute);
    click_on(cx, "region-0-0", Modifiers::none(), MouseButton::Left);
    assert!(region(cx, (0, 0)).mute);
    click_on(cx, "region-0-0", Modifiers::none(), MouseButton::Left);
    assert!(!region(cx, (0, 0)).mute);
    // Text: its name, to type.
    use_tool(cx, "Text");
    click_on(cx, "region-0-0", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| ws.naming.is_some()));
    cx.simulate_keystrokes("escape");
    // Cut in two, then glued back: one region, every note.
    let whole = region(cx, (0, 0));
    let notes =
        |r: &pina_engine::song::project::Region| r.content.notes(r.length).map_or(0, |n| n.len());
    ws.update(cx, |ws, cx| ws.split_at((0, 0), BAR * 2, cx));
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks[0].regions.len()),
        2
    );
    use_tool(cx, "Glue");
    click_on(cx, "region-0-0", Modifiers::none(), MouseButton::Left);
    let glued = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].regions.clone());
    assert_eq!(glued.len(), 1);
    assert_eq!(
        (glued[0].start, glued[0].length),
        (whole.start, whole.length)
    );
    assert_eq!(notes(&glued[0]), notes(&whole));
    // Zoom: a click in, ⌥ out; a drag across two bars fills the window with them.
    use_tool(cx, "Zoom");
    let zoom = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.px_per_beat);
    let before = zoom(cx);
    let lane = spot(&ws, cx, BAR * 6, 1);
    drag(cx, lane, lane);
    assert_eq!(zoom(cx), before * 2.0);
    cx.simulate_mouse_down(lane, MouseButton::Left, Modifiers::alt());
    cx.simulate_mouse_up(lane, MouseButton::Left, Modifiers::alt());
    cx.run_until_parked();
    assert_eq!(zoom(cx), before);
    drag(cx, spot(&ws, cx, BAR, 1), spot(&ws, cx, BAR * 3, 1));
    let (fit, scroll, width) = ws.read_with(cx, |ws, _| {
        (
            ws.px_per_beat,
            ws.scroll_x,
            f32::from(ws.lanes_bounds.get().size.width),
        )
    });
    assert!((fit * 8.0 - width).abs() < 1.0, "{fit} {width}");
    assert!((scroll - fit * 4.0).abs() < 1.0);
    ws.update(cx, |ws, cx| {
        (ws.px_per_beat, ws.scroll_x) = (before, 0.0);
        cx.notify();
    });
    cx.run_until_parked();
    // Fade: dragged in from the drums' start, a fade in.
    use_tool(cx, "Fade");
    drag(cx, spot(&ws, cx, PPQ / 4, 2), spot(&ws, cx, PPQ, 2));

    let drums = region(cx, (2, 0));
    assert!(
        drums.fade_in > PPQ / 2 && drums.fade_out == 0,
        "{}",
        drums.fade_in
    );
    // Flex: the lead's end dragged a bar on stretches its notes.
    use_tool(cx, "Flex");
    let lead = region(cx, (1, 0));
    let b = cx.debug_bounds("region-1-0").unwrap();
    let end = point(b.right() - px(2.), b.bottom() - px(6.));
    drag(
        cx,
        end,
        point(
            spot(&ws, cx, lead.start + lead.length + BAR, 1).x - px(2.),
            end.y,
        ),
    );
    assert_eq!(region(cx, (1, 0)).length, lead.length + BAR);
    assert_ne!(region(cx, (1, 0)).content, lead.content);
    // Solo: held, the region plays alone (pattern mode); let go, as it was.
    use_tool(cx, "Solo");
    let at = spot(&ws, cx, PPQ, 0);
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(ws.read_with(cx, |ws, _| ws.pattern_mode));
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(!ws.read_with(cx, |ws, _| ws.pattern_mode));
}

#[gpui::test]
fn musical_typing_records_a_midi_region(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_click(spot(&ws, cx, PPQ, 0), Modifiers::none()); // select Keys
    // Past the end of the Keys region, so the take is a region of its own.
    let ruler = spot(&ws, cx, BAR * 5, 0) - point(px(0.), px(LANE_H / 2.0 + 10.));
    cx.simulate_click(ruler, Modifiers::none());
    until(cx, &ws, |ws| ws.playhead() == BAR * 5);
    cx.simulate_keystrokes("cmd-k r");
    until(cx, &ws, |ws| ws.player.playing());
    cx.simulate_keystrokes("a"); // C3
    until(
        cx,
        &ws,
        |ws| matches!(&ws.recording, Some(Recording::Midi { held, notes, .. }) if !held.is_empty() || !notes.is_empty()),
    );
    cx.simulate_keystrokes("r");
    let regions = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].regions.len());
    assert_eq!(regions, 2);
    let notes = keys_notes(&ws, cx, 1);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].1, 60);
}

#[gpui::test]
fn recording_over_a_region_makes_takes_to_comp_and_flatten(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_click(spot(&ws, cx, PPQ, 0), Modifiers::none()); // select Keys
    cx.simulate_keystrokes("cmd-k r");
    until(cx, &ws, |ws| ws.player.playing());
    cx.simulate_keystrokes("a");
    until(
        cx,
        &ws,
        |ws| matches!(&ws.recording, Some(Recording::Midi { held, notes, .. }) if !held.is_empty() || !notes.is_empty()),
    );
    cx.simulate_keystrokes("r cmd-k");
    let takes = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            match &ws.session.project.tracks[0].regions[0].content {
                Content::Takes { takes, comp } => Some((takes.len(), comp.clone())),
                _ => None,
            }
        })
    };
    assert_eq!(takes(cx).map(|t| t.0), Some(2)); // the region and the new take

    double_click(cx, spot(&ws, cx, PPQ * 6, 0));
    assert_eq!(ws.read_with(cx, |ws, _| ws.bottom), Some(Bottom::Takes));
    // Swipe across the first take for the second bar.
    let y = cx.debug_bounds("take-0").unwrap().center().y;
    let (from, to) = (spot(&ws, cx, BAR, 0).x, spot(&ws, cx, BAR * 2, 0).x);
    drag(cx, point(from, y), point(to, y));
    let comp = takes(cx).unwrap().1;
    let pieces: Vec<(u64, usize)> = comp.iter().map(|p| (p.from, p.take)).collect();
    assert_eq!(pieces, [(0, 1), (BAR, 0), (BAR * 2, 1)]);

    click_on(cx, "flatten", Modifiers::none(), MouseButton::Left);
    assert_eq!(takes(cx), None);
}

#[gpui::test]
fn live_loops_take_a_region_and_launch_it(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_click(spot(&ws, cx, PPQ, 1), Modifiers::none()); // select the Lead region
    cx.simulate_keystrokes("l");
    assert_eq!(ws.read_with(cx, |ws, _| ws.bottom), Some(Bottom::Loops));
    click_on(cx, "cell-1-2", Modifiers::none(), MouseButton::Left); // copy it into scene 3
    let cells = ws.read_with(cx, |ws, _| ws.session.project.tracks[1].cells.clone());
    assert_eq!(
        (cells.len(), cells[0].scene, cells[0].region.start),
        (1, 2, 0)
    );

    // Launching it starts playback; the scene button launches it too, and clicking it stops it.
    click_on(cx, "cell-1-2", Modifiers::none(), MouseButton::Left);
    until(cx, &ws, |ws| ws.player.playing());
    let id = ws.read_with(cx, |ws, _| ws.session.project.tracks[1].id);
    assert_eq!(ws.read_with(cx, |ws, _| ws.live[&id].0), Some(2));
    click_on(cx, "cell-1-2", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.live[&id].0), None);
    click_on(cx, "scene-2", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.live[&id].0), Some(2));

    click_on(cx, "cell-1-2", Modifiers::none(), MouseButton::Right); // empty it
    assert!(ws.read_with(cx, |ws, _| ws.session.project.tracks[1].cells.is_empty()));
}

#[gpui::test]
fn the_cycle_turns_on_and_is_swept_out_in_the_ruler(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let cycle = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.session.project.cycle);
    // The transport's button, with nothing selected: four bars from the playhead's bar.
    click_on(cx, "cycle", Modifiers::none(), MouseButton::Left);
    let c = cycle(cx).unwrap();
    assert_eq!((c.start, c.end, c.off), (0, BAR * 4, false));
    click_on(cx, "cycle", Modifiers::none(), MouseButton::Left);
    assert!(cycle(cx).unwrap().off);
    // Sweeping the strip along the ruler's top sets it, from bar 2 to bar 4.
    let ruler = cx.debug_bounds("ruler").unwrap();
    let y = ruler.origin.y + px(4.);
    drag(
        cx,
        point(spot(&ws, cx, BAR, 0).x, y),
        point(spot(&ws, cx, BAR * 3, 0).x, y),
    );
    let c = cycle(cx).unwrap();
    assert_eq!((c.start, c.end, c.off), (BAR, BAR * 3, false));
    cx.simulate_keystrokes("cmd-z");
    assert!(cycle(cx).unwrap().off);
}

#[gpui::test]
fn markers_are_added_moved_named_and_deleted(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let markers = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            ws.session
                .project
                .markers
                .iter()
                .map(|m| (m.tick, m.name.clone()))
                .collect::<Vec<_>>()
        })
    };
    let row = cx.debug_bounds("markers").unwrap();
    double_click(cx, point(spot(&ws, cx, BAR + PPQ, 0).x, row.center().y));
    assert_eq!(markers(cx), [(BAR, "Intro".to_string())]);
    // A new marker opens for naming, its name selected: typing replaces it.
    assert!(ws.read_with(cx, |ws, _| ws.naming.is_some()));
    cx.simulate_input("Cold open");
    cx.simulate_keystrokes("enter");
    assert_eq!(markers(cx), [(BAR, "Cold open".to_string())]);
    assert!(ws.read_with(cx, |ws, _| ws.naming.is_none()));
    // Escape keeps the name it had.
    let at = cx.debug_bounds("marker-0").unwrap().center();
    double_click(cx, at);
    cx.simulate_input("Nope");
    cx.simulate_keystrokes("escape");
    assert_eq!(markers(cx)[0].1, "Cold open");
    cx.run_until_parked();
    // Dragged a bar on.
    let at = cx.debug_bounds("marker-0").unwrap().center();
    drag(cx, at, at + point(px(48. * 4.), px(0.)));
    assert_eq!(markers(cx)[0].0, BAR * 2);
    // Right-click: rename, a name, or delete.
    click_on(cx, "marker-0", Modifiers::none(), MouseButton::Right);
    click_on(cx, "menu-4", Modifiers::none(), MouseButton::Left); // Chorus
    assert_eq!(markers(cx)[0].1, "Chorus");
    click_on(cx, "marker-0", Modifiers::none(), MouseButton::Right);
    let last = ws.read_with(cx, |ws, _| ws.menu.as_ref().unwrap().items.len() - 1);
    let name: &'static str = Box::leak(format!("menu-{last}").into_boxed_str());
    click_on(cx, name, Modifiers::none(), MouseButton::Left);
    assert!(markers(cx).is_empty());
}

#[gpui::test]
fn the_pencil_draws_the_scissors_cut_and_the_eraser_erases(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let regions = |cx: &VisualTestContext, t: usize| {
        ws.read_with(cx, |ws, _| {
            ws.session.project.tracks[t]
                .regions
                .iter()
                .map(|r| (r.start, r.length))
                .collect::<Vec<_>>()
        })
    };
    click_on(cx, "tool-Pencil", Modifiers::none(), MouseButton::Left);
    // After the Lead region, in bar 6: four bars from there.
    cx.simulate_click(spot(&ws, cx, BAR * 5 + PPQ, 1), Modifiers::none());
    assert_eq!(regions(cx, 1).last(), Some(&(BAR * 5, BAR * 4)));

    click_on(cx, "tool-Scissors", Modifiers::none(), MouseButton::Left);
    cx.simulate_click(spot(&ws, cx, BAR * 2, 0), Modifiers::none());
    assert_eq!(regions(cx, 0), [(0, BAR * 2), (BAR * 2, BAR * 2)]);

    click_on(cx, "tool-Eraser", Modifiers::none(), MouseButton::Left);
    cx.simulate_click(spot(&ws, cx, BAR * 3, 0), Modifiers::none());
    assert_eq!(regions(cx, 0), [(0, BAR * 2)]);
}

#[gpui::test]
fn snap_follows_the_menu(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    assert_eq!(ws.read_with(cx, |ws, _| ws.grid()), PPQ); // at the default zoom
    click_on(cx, "snap", Modifiers::none(), MouseButton::Left);
    click_on(cx, "menu-4", Modifiers::none(), MouseButton::Left); // 1/16
    assert_eq!(ws.read_with(cx, |ws, _| ws.grid()), PPQ / 4);
}

#[gpui::test]
fn pattern_mode_loops_the_selected_region_alone(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    click_on(cx, "pattern-mode", Modifiers::none(), MouseButton::Left);
    assert!(!ws.read_with(cx, |ws, _| ws.pattern_mode)); // nothing selected
    cx.simulate_click(spot(&ws, cx, PPQ, 1), Modifiers::none());
    click_on(cx, "pattern-mode", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| ws.pattern_mode));
    // It plays, looping in the region, and the project itself has no cycle.
    cx.simulate_keystrokes("space");
    until(cx, &ws, |ws| ws.player.playing());
    assert!(ws.read_with(cx, |ws, _| ws.session.project.cycle.is_none()));
    until(cx, &ws, |ws| ws.playhead() > 0);
    cx.simulate_keystrokes("space");
}

#[gpui::test]
fn choosing_a_key_moves_the_music_into_it_or_just_names_it(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let key = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| ws.session.project.song_key().map(|k| k.name()))
    };
    let lowest = |cx: &VisualTestContext| keys_notes(&ws, cx, 0).iter().map(|n| n.1).min();
    assert_eq!(key(cx).as_deref(), Some("C major")); // the demo's chords
    let c = lowest(cx).unwrap();
    // A major, from the menu: the chords move down a minor third into it.
    click_on(cx, "key", Modifiers::none(), MouseButton::Left);
    click_on(cx, "menu-10", Modifiers::none(), MouseButton::Left);
    assert_eq!(key(cx).as_deref(), Some("A major"));
    assert_eq!(lowest(cx), Some(c - 3));
    // Worked out from the notes again, they say A major now.
    click_on(cx, "key", Modifiers::none(), MouseButton::Left);
    click_on(cx, "menu-0", Modifiers::none(), MouseButton::Left);
    assert_eq!(key(cx).as_deref(), Some("A major"));
    // With moving turned off (the menu's last row; it stays open), a key only names it.
    click_on(cx, "key", Modifiers::none(), MouseButton::Left);
    click_on(cx, "menu-tail", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| !ws.key_moves_notes && ws.menu.is_some()));
    click_on(cx, "menu-1", Modifiers::none(), MouseButton::Left); // C major
    assert_eq!(
        (key(cx).as_deref(), lowest(cx)),
        (Some("C major"), Some(c - 3))
    );
    // Each was one edit.
    cx.simulate_keystrokes("cmd-z cmd-z cmd-z");
    assert_eq!((key(cx).as_deref(), lowest(cx)), (Some("C major"), Some(c)));
}

#[gpui::test]
fn pattern_mode_plays_whichever_region_is_selected_at_once(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    // The first drum loop (bar 1), as a pattern, playing.
    cx.simulate_click(spot(&ws, cx, PPQ, 2), Modifiers::none());
    click_on(cx, "pattern-mode", Modifiers::none(), MouseButton::Left);
    cx.simulate_keystrokes("space");
    until(cx, &ws, |ws| ws.player.playing() && ws.playhead() > 0);
    // Selecting the third (bar 3) plays it now, from the same place in its loop.
    cx.simulate_click(spot(&ws, cx, BAR * 2 + PPQ, 2), Modifiers::none());
    until(cx, &ws, |ws| (BAR * 2..BAR * 3).contains(&ws.playhead()));
    let looped = ws.read_with(cx, |ws, _| ws.pattern.map(|p| p.0));
    assert_eq!(looped, ws.read_with(cx, |ws, _| ws.region));
    cx.simulate_keystrokes("space");
}

#[gpui::test]
fn the_time_signature_menu_sets_the_bar(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    click_on(cx, "time-signature", Modifiers::none(), MouseButton::Left);
    click_on(cx, "menu-6", Modifiers::none(), MouseButton::Left); // 6/8
    let (sig, bar) = ws.read_with(cx, |ws, _| {
        (ws.session.project.time_signature, ws.session.project.bar())
    });
    assert_eq!((sig, bar), ([6, 8], PPQ * 3));
    ws.update(cx, |ws, _| ws.snap = Snap::Bar);
    assert_eq!(ws.read_with(cx, |ws, _| ws.grid()), PPQ * 3);
}

#[gpui::test]
fn the_cycle_is_swept_in_the_rulers_top_half_and_c_follows_the_selection(cx: &mut TestAppContext) {
    let dir = demo_copy("cycle-sweep");
    let (ws, cx) = open_at(cx, &dir);
    // Pattern mode on (a region selected), then a stretch swept in the ruler's top half: the
    // song plays that stretch, not the pattern.
    ws.update(cx, |ws, cx| {
        ws.region = Some((0, 0));
        ws.set_pattern_mode(true, cx);
    });
    let ruler = cx.debug_bounds("ruler").unwrap();
    let y = ruler.origin.y + px(RULER_H / 2.0 - 2.0);
    let x = |tick: u64| spot(&ws, cx, tick, 0).x;
    drag(cx, point(x(BAR), y), point(x(BAR * 3), y));
    let (cycle, pattern) = ws.read_with(cx, |ws, _| (ws.session.project.cycle, ws.pattern_mode));
    let c = cycle.unwrap();
    assert_eq!(
        (c.start, c.end, c.off, pattern),
        (BAR, BAR * 3, false, false)
    );
    // C with a region selected that the cycle isn't round: the cycle moves to it.
    let r = ws.read_with(cx, |ws, _| {
        let r = &ws.session.project.tracks[0].regions[0];
        (r.start, r.start + r.length)
    });
    ws.update(cx, |ws, cx| {
        ws.region = Some((0, 0));
        ws.toggle_cycle(cx);
    });
    let c = ws.read_with(cx, |ws, _| ws.session.project.cycle.unwrap());
    assert_eq!((c.start, c.end, c.off), (r.0, r.1, false));
    // C again, the cycle already round it: off.
    ws.update(cx, |ws, cx| ws.toggle_cycle(cx));
    assert!(ws.read_with(cx, |ws, _| ws.session.project.cycle.unwrap().off));
    std::fs::remove_dir_all(dir).unwrap();
}
