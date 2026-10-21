//! Regions: moving, trimming, copying, looping, stretching, warping and their playback settings.

use super::*;

#[gpui::test]
fn dragging_a_region_moves_it_by_the_grid_and_undo_puts_it_back(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    // The second drums region (track 3) starts at bar 2; drag it two beats right.
    let from = spot(&ws, cx, BAR + PPQ, 2);
    let to = spot(&ws, cx, BAR + PPQ * 3, 2);
    drag(cx, from, to);
    assert_eq!(selected(&ws, cx), (2, BAR + PPQ * 2, BAR));

    cx.simulate_keystrokes("cmd-z");
    let start = ws.read_with(cx, |ws, _| ws.session.project.tracks[2].regions[1].start);
    assert_eq!(start, BAR);
}

#[gpui::test]
fn regions_move_only_to_tracks_that_can_play_them(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    // Keys onto Lead (both instruments): moves.
    drag(cx, spot(&ws, cx, PPQ, 0), spot(&ws, cx, PPQ, 1));
    assert_eq!(selected(&ws, cx).0, 1);
    // Then onto Drums (audio): stays on Lead.
    drag(cx, spot(&ws, cx, PPQ, 1), spot(&ws, cx, PPQ, 2));
    assert_eq!(selected(&ws, cx).0, 1);
}

#[gpui::test]
fn dragging_a_regions_end_trims_it(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    // The first drums region's right edge (its bottom half: the top loops), one beat left.
    let edge = spot(&ws, cx, BAR, 2) + point(px(-2.), px(12.));
    drag(cx, edge, edge - point(px(48.), px(0.)));
    assert_eq!(selected(&ws, cx), (2, 0, BAR - PPQ));
}

#[gpui::test]
fn split_at_the_playhead(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    // Put the playhead at bar 3 by clicking the ruler above the lanes.
    let ruler = spot(&ws, cx, BAR * 2, 0) - point(px(0.), px(LANE_H / 2.0 + 10.));
    cx.simulate_click(ruler, Modifiers::none());
    until(cx, &ws, |ws| ws.playhead() == BAR * 2);
    // Select the Keys region (a click that moves nothing isn't an edit) and split it.
    cx.simulate_click(spot(&ws, cx, PPQ, 0), Modifiers::none());
    assert!(!ws.read_with(cx, |ws, _| ws.session.edited));
    cx.simulate_keystrokes("cmd-t");
    let lengths = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[0]
            .regions
            .iter()
            .map(|r| (r.start, r.length))
            .collect::<Vec<_>>()
    });
    assert_eq!(lengths, [(0, BAR * 2), (BAR * 2, BAR * 2)]);
}

#[gpui::test]
fn regions_copy_paste_mute_and_repeat_from_their_menus_and_keys(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let count = |cx: &VisualTestContext, t: usize| {
        ws.read_with(cx, |ws, _| ws.session.project.tracks[t].regions.len())
    };
    let (keys, drums) = (count(cx, 0), count(cx, 2));
    // Copy a drums region; paste it on the drums' lane, four bars in.
    click_on(cx, "region-2-1", Modifiers::none(), MouseButton::Right);
    pick(cx, &ws, "Copy");
    let at = spot(&ws, cx, BAR * 4, 2);
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    pick(cx, &ws, "Paste Here");
    assert_eq!(count(cx, 2), drums + 1);
    assert_eq!(selected(&ws, cx), (2, BAR * 4, BAR));
    // ⌃M mutes it; ⌘R repeats it right after itself; ⌘X cuts the repeat.
    cx.simulate_keystrokes("ctrl-m");
    assert!(ws.read_with(cx, |ws, _| ws.session.project.tracks[2].regions[drums].mute));
    cx.simulate_keystrokes("cmd-r");
    assert_eq!(selected(&ws, cx), (2, BAR * 5, BAR));
    cx.simulate_keystrokes("cmd-x");
    assert_eq!(count(cx, 2), drums + 1);
    // Audio won't paste on an instrument track.
    ws.update(cx, |ws, _| ws.select_track(0));
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(count(cx, 0), keys);
    assert!(
        ws.read_with(cx, |ws, _| ws.status.clone())
            .unwrap()
            .contains("audio track")
    );
    // The region's menu mutes it too, and undo puts it back.
    click_on(cx, "region-0-0", Modifiers::none(), MouseButton::Right);
    pick(cx, &ws, "Mute");
    assert!(ws.read_with(cx, |ws, _| ws.session.project.tracks[0].regions[0].mute));
    cx.simulate_keystrokes("cmd-z");
    assert!(!ws.read_with(cx, |ws, _| ws.session.project.tracks[0].regions[0].mute));
}

#[gpui::test]
fn a_regions_end_loops_from_its_top_and_time_stretches_with_option(cx: &mut TestAppContext) {
    use pina_engine::song::project::Content;
    let (ws, cx) = open(cx);
    let region = |cx: &VisualTestContext, at: (usize, usize)| {
        ws.read_with(cx, |ws, _| ws.session.project.region(at).clone())
    };
    let keys = region(cx, (0, 0));
    let end = keys.start + keys.length;
    // The top of Keys' end, dragged two bars on: looped to there.
    let tab = cx.debug_bounds("loop-tab-0-0").unwrap().center();
    drag(cx, tab, point(spot(&ws, cx, end + BAR * 2, 0).x, tab.y));
    let looped = region(cx, (0, 0));
    assert_eq!(
        (looped.looped, looped.loop_length),
        (true, Some(keys.length + BAR * 2))
    );
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.loop_end((0, 0))),
        end + BAR * 2
    );
    // The tab is on the last repeat now: back inside the region, no loops.
    let tab = cx.debug_bounds("loop-tab-0-0").unwrap().center();
    drag(cx, tab, point(spot(&ws, cx, end - BAR, 0).x, tab.y));
    assert!(!region(cx, (0, 0)).looped);

    // ⌥-dragging the bottom of Lead's end a bar on: its notes spread over the longer region.
    let lead = region(cx, (1, 0));
    let b = cx.debug_bounds("region-1-0").unwrap();
    let from = point(b.right() - px(2.), b.bottom() - px(6.));
    let to = point(
        spot(&ws, cx, lead.start + lead.length + BAR, 1).x - px(2.),
        from.y,
    );
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::alt());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::alt());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::alt());
    cx.run_until_parked();
    let stretched = region(cx, (1, 0));
    assert_eq!(stretched.length, lead.length + BAR);
    let (Content::Midi { notes: was, .. }, Content::Midi { notes: now, .. }) =
        (&lead.content, &stretched.content)
    else {
        panic!("not MIDI")
    };
    let ratio = stretched.length as f64 / lead.length as f64;
    let last = was.len() - 1;
    assert_eq!(
        now[last].start,
        (was[last].start as f64 * ratio).round() as i64
    );
    // Without ⌥ it trims: its notes stay put.
    let b = cx.debug_bounds("region-1-0").unwrap();
    let from = point(b.right() - px(2.), b.bottom() - px(6.));
    drag(cx, from, from - point(px(48.), px(0.)));
    let Content::Midi { notes, .. } = &region(cx, (1, 0)).content else {
        panic!("not MIDI")
    };
    assert_eq!(notes, now);
    cx.simulate_keystrokes("cmd-z cmd-z");
    assert_eq!(region(cx, (1, 0)), lead);
}

#[gpui::test]
fn option_dragging_an_audio_regions_end_flexes_it(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    // The first drums region's right edge (its bottom half), option-dragged a bar longer.
    let edge = spot(&ws, cx, BAR, 2) + point(px(-2.), px(12.));
    let alt = Modifiers {
        alt: true,
        ..Modifiers::none()
    };
    cx.simulate_mouse_down(edge, MouseButton::Left, alt);
    cx.run_until_parked();
    cx.simulate_mouse_move(edge + point(px(48. * 4.), px(0.)), MouseButton::Left, alt);
    cx.simulate_mouse_up(edge + point(px(48. * 4.), px(0.)), MouseButton::Left, alt);
    cx.run_until_parked();
    let (length, content) = ws.read_with(cx, |ws, _| {
        let r = ws.session.project.region((2, 0));
        (r.length, r.content.clone())
    });
    assert_eq!(length, BAR * 2);
    assert!(
        matches!(content, Content::Audio { stretch: 2.0, .. }),
        "{content:?}"
    );
    // And an ordinary drag of the same edge still trims.
    assert_eq!(ws.read_with(cx, |ws, _| ws.status.clone()), None);
}

#[gpui::test]
fn option_drag_copies_a_region_and_option_shift_drag_makes_an_alias(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let regions = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            ws.session.project.tracks[0]
                .regions
                .iter()
                .map(|r| (r.start, r.link))
                .collect::<Vec<_>>()
        })
    };
    let alias = Modifiers {
        alt: true,
        shift: true,
        ..Modifiers::none()
    };
    // Keys' region (bars 1-4) to bar 5, as an alias.
    drag_with(
        cx,
        spot(&ws, cx, PPQ, 0),
        spot(&ws, cx, BAR * 4 + PPQ, 0),
        alias,
    );
    let r = regions(cx);
    assert_eq!(r.len(), 2);
    assert_eq!(r[1].0, BAR * 4);
    assert!(r[0].1.is_some() && r[0].1 == r[1].1, "{r:?}");
    // Editing the alias's notes edits the original's.
    ws.update(cx, |ws, cx| {
        ws.edit(cx, |ws| {
            ws.session.project.notes_mut((0, 1)).unwrap()[0].pitch = 30
        })
    });
    assert_eq!(keys_notes(&ws, cx, 0)[0].1, 30);
    // Undo puts both back.
    cx.simulate_keystrokes("cmd-z");
    assert_ne!(keys_notes(&ws, cx, 0)[0].1, 30);
    assert_eq!(keys_notes(&ws, cx, 0), keys_notes(&ws, cx, 1));

    // Making it unique, from the inspector.
    cx.simulate_click(spot(&ws, cx, BAR * 4 + PPQ, 0), Modifiers::none());
    click_on(cx, "region-unlink", Modifiers::none(), MouseButton::Left);
    assert!(regions(cx).iter().all(|r| r.1.is_none()));

    // Option alone: a plain copy, at bar 9.
    let copy = Modifiers {
        alt: true,
        ..Modifiers::none()
    };
    drag_with(
        cx,
        spot(&ws, cx, PPQ, 0),
        spot(&ws, cx, BAR * 8 + PPQ, 0),
        copy,
    );
    let r = regions(cx);
    assert_eq!((r.len(), r[2].0, r[2].1), (3, BAR * 8, None));
    // An option-click that doesn't drag leaves no copy.
    cx.simulate_click(spot(&ws, cx, PPQ, 0), copy);
    assert_eq!(regions(cx).len(), 3);
}

#[gpui::test]
fn region_playback_settings_loop_transpose_and_quantize_without_touching_notes(
    cx: &mut TestAppContext,
) {
    let (ws, cx) = open(cx);
    cx.simulate_click(spot(&ws, cx, PPQ, 0), Modifiers::none());
    let region = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| ws.session.project.tracks[0].regions[0].clone())
    };
    let notes = keys_notes(&ws, cx, 0);
    click_on(cx, "region-loop", Modifiers::none(), MouseButton::Left);
    click_on(cx, "transpose+1", Modifiers::none(), MouseButton::Left);
    click_on(cx, "transpose+1", Modifiers::none(), MouseButton::Left);
    click_on(cx, "velocity-1", Modifiers::none(), MouseButton::Left);
    click_on(cx, "region-quantize", Modifiers::none(), MouseButton::Left);
    click_on(cx, "menu-2", Modifiers::none(), MouseButton::Left); // 1/8
    let r = region(cx);
    assert!(r.looped);
    assert_eq!(
        (r.transpose, r.velocity, r.quantize),
        (2, -1, Some(PPQ / 2))
    );
    assert_eq!(keys_notes(&ws, cx, 0), notes);
    // Applying them writes them into the notes.
    click_on(cx, "region-apply", Modifiers::none(), MouseButton::Left);
    let r = region(cx);
    assert_eq!((r.transpose, r.velocity, r.quantize), (0, 0, None));
    let applied = keys_notes(&ws, cx, 0);
    assert_eq!(applied[0].1, notes[0].1 + 2);
    assert!(applied.iter().all(|n| n.0 % (PPQ as i64 / 2) == 0));
}

#[gpui::test]
fn an_audio_regions_flex_and_tempo_following_are_set_in_its_inspector(cx: &mut TestAppContext) {
    use pina_engine::song::project::Flex;
    let (ws, cx) = open(cx);
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    cx.simulate_click(spot(&ws, cx, PPQ, drums), Modifiers::none());
    click_on(cx, "region-flex", Modifiers::none(), MouseButton::Left);
    click_on(cx, "menu-1", Modifiers::none(), MouseButton::Left); // Slicing
    click_on(cx, "region-follow", Modifiers::none(), MouseButton::Left);
    let audio = |ws: &Entity<Workspace>, cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            match &ws.session.project.tracks[drums].regions[0].content {
                Content::Audio {
                    flex,
                    follow,
                    stretch,
                    ..
                } => (*flex, *follow, *stretch),
                c => panic!("{c:?}"),
            }
        })
    };
    assert_eq!(audio(&ws, cx), (Flex::Slicing, true, 1.0));
    // The tempo halved: it plays twice as long, its beats on the song's.
    ws.update(cx, |ws, cx| {
        ws.edit(cx, |ws| {
            let p = &mut ws.session.project;
            p.set_tempo(p.tempo / 2.0);
        })
    });
    assert_eq!(audio(&ws, cx), (Flex::Slicing, true, 2.0));
    // Undo takes the tempo and the stretch back together.
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(audio(&ws, cx).2, 1.0);
}

#[gpui::test]
fn a_hit_dragged_in_an_audio_region_makes_a_warp_marker(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    cx.simulate_click(spot(&ws, cx, PPQ, drums), Modifiers::none());
    let at = (drums, 0);
    let hits = ws.read_with(cx, |ws, _| ws.warping(at).map(|w| w.hits.to_vec()));
    let hits = hits.expect("the drums are loaded");
    assert!(hits.len() >= 3, "{hits:?}");
    let from = hits[1] as u64;
    let name: &'static str = format!("warp-{}-0-{from}", drums).leak();
    // A sixteenth later: a marker there, the hits either side pinned where they were.
    let handle = cx
        .debug_bounds(name)
        .expect("a handle for the hit")
        .center();
    let sixteenth = ws.read_with(cx, |ws, _| ws.px_per_tick() * (PPQ / 4) as f32);
    drag(cx, handle, handle + point(px(sixteenth), px(0.)));
    let warps = |ws: &Entity<Workspace>, cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| match &ws.session.project.region(at).content {
            Content::Audio { warps, .. } => warps.clone(),
            c => panic!("{c:?}"),
        })
    };
    let made = warps(&ws, cx);
    assert_eq!(made.len(), 3, "{made:?}");
    assert_eq!(
        made.iter().map(|m| m[0]).collect::<Vec<_>>(),
        [hits[0] as u64, from, hits[2] as u64]
    );
    let w = ws.read_with(cx, |ws, _| ws.warping(at).unwrap());
    let moved = w.tick(from as usize);
    assert!(
        moved > 0.0 && (moved / (PPQ / 4) as f64).fract().abs() < 1e-6,
        "{moved}"
    );
    // A double-click takes the marker out; undo puts it back.
    let handle = cx.debug_bounds(name).unwrap().center();
    double_click(cx, handle);
    assert_eq!(warps(&ws, cx).len(), 2);
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(warps(&ws, cx).len(), 3);
}

#[gpui::test]
fn audio_dragged_in_from_the_finder_lands_on_the_timeline(cx: &mut TestAppContext) {
    use gpui::{ExternalPaths, FileDropEvent};
    let dir = demo_copy("finder-drop");
    let outside = dir.join("Outside");
    std::fs::create_dir_all(&outside).unwrap();
    let (a, b) = (outside.join("Vox Take.wav"), outside.join("Shaker.wav"));
    tone(&a, 0.5, 220.0);
    tone(&b, 0.5, 880.0);
    let (ws, cx) = open_at(cx, &dir);
    let drop = |cx: &mut VisualTestContext, files: &[&Path], at: Point<Pixels>| {
        let paths = ExternalPaths(files.iter().map(|f| f.to_path_buf()).collect());
        cx.simulate_event(FileDropEvent::Entered {
            position: at,
            paths,
        });
        cx.simulate_event(FileDropEvent::Pending { position: at });
        cx.simulate_event(FileDropEvent::Submit { position: at });
        cx.simulate_event(FileDropEvent::Exited);
        cx.run_until_parked();
    };
    let tracks =
        |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.session.project.tracks.clone());
    let drums = tracks(cx).iter().position(|t| t.name == "Drums").unwrap();
    let before = tracks(cx);

    // On an audio track: a region there, at the bar under the pointer, its file copied in.
    drop(cx, &[&a], spot(&ws, cx, BAR + 30, drums));
    let now = tracks(cx);
    let r = now[drums].regions.last().unwrap();
    assert_eq!(
        (now[drums].regions.len(), r.start),
        (before[drums].regions.len() + 1, BAR)
    );
    let Content::Audio { file, .. } = &r.content else {
        panic!()
    };
    assert!(dir.join(file).exists() && file.starts_with("Audio Files"));

    // On an instrument track: a new audio track under it, named for the file. Two files, two.
    let keys = now.iter().position(|t| t.instrument.is_some()).unwrap();
    drop(cx, &[&a, &b], spot(&ws, cx, 2 * BAR, keys));
    let now = tracks(cx);
    assert_eq!(now.len(), before.len() + 2);
    let names: Vec<&str> = now[keys + 1..keys + 3]
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(names, ["Vox Take", "Shaker"]);
    assert!(now[keys + 1].instrument.is_none() && now[keys + 2].regions[0].start == 2 * BAR);

    // Under the tracks: a new one at the end. One undo takes a drop back.
    let under = spot(&ws, cx, 0, now.len() + 1);
    drop(cx, &[&b], under);
    assert_eq!(tracks(cx).len(), before.len() + 3);
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(tracks(cx).len(), before.len() + 2);

    // Not audio: nothing, and it says what it takes. (Dropped after a key press, which gpui's
    // own drops miss.)
    let text = outside.join("notes.txt");
    std::fs::write(&text, "x").unwrap();
    let at = spot(&ws, cx, 0, drums);
    drop(cx, &[&text], at);
    assert_eq!(tracks(cx).len(), before.len() + 2);
    let status = ws
        .read_with(cx, |ws, _| ws.status.clone())
        .unwrap_or_default();
    assert!(status.contains("WAV"), "{status}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn writing_notes_starts_with_a_double_click_anywhere(cx: &mut TestAppContext) {
    use pina_engine::song::project::Instrument;
    let dir = demo_copy("write-notes");
    let (ws, cx) = open_at(cx, &dir);
    // A new instrument track, empty: it says how to start.
    ws.update(cx, |ws, cx| ws.new_track(Some(Instrument::Synth), cx));
    cx.run_until_parked();
    let t = ws.read_with(cx, |ws, _| ws.track);
    assert!(ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[t].name.starts_with("Synth")
    }));

    // A double-click on its lane: a four-bar region from that bar, the piano roll open on it.
    double_click(cx, spot(&ws, cx, BAR * 2 + 100, t));
    let (regions, bottom, region) = ws.read_with(cx, |ws, _| {
        let r = &ws.session.project.tracks[t].regions;
        (
            r.iter().map(|r| (r.start, r.length)).collect::<Vec<_>>(),
            ws.bottom,
            ws.region,
        )
    });
    assert_eq!(regions, [(BAR * 2, BAR * 4)]);
    assert_eq!((bottom, region), (Some(Bottom::PianoRoll), Some((t, 0))));

    // In the roll, a note past the region's end grows it to that bar's end.
    let spot_at = roll_spot(&ws, cx, BAR * 7 + 10, 64);
    double_click(cx, spot_at);
    let (length, notes) = ws.read_with(cx, |ws, _| {
        let r = &ws.session.project.tracks[t].regions[0];
        let Content::Midi { notes, .. } = &r.content else {
            panic!()
        };
        (
            r.length,
            notes.iter().map(|n| (n.start, n.pitch)).collect::<Vec<_>>(),
        )
    });
    assert_eq!(length, BAR * 6);
    assert_eq!(notes, [((BAR * 5) as i64, 64)]);

    // With nothing selected, the roll still writes: a note before the region makes a new one
    // round it (kept clear of the first), in the same step.
    ws.update(cx, |ws, cx| {
        ws.region = None;
        cx.notify();
    });
    cx.run_until_parked();
    let undo_to = ws.read_with(cx, |ws, _| ws.session.project.tracks[t].regions.len());
    let spot_at = roll_spot(&ws, cx, 10, 60);
    double_click(cx, spot_at);
    let regions = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[t]
            .regions
            .iter()
            .map(|r| (r.start, r.length))
            .collect::<Vec<_>>()
    });
    assert_eq!(regions, [(BAR * 2, BAR * 6), (0, BAR * 2)]);
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks[t].regions.len()),
        undo_to
    );

    // Inside a cycle that's on, a new region takes the cycle's stretch.
    ws.update(cx, |ws, cx| {
        ws.session.project.cycle = Some(pina_engine::song::project::Cycle {
            start: 0,
            end: BAR,
            off: false,
        });
        ws.bottom = None;
        cx.notify();
    });
    cx.run_until_parked();
    double_click(cx, spot(&ws, cx, BAR / 2, t));
    let last = ws.read_with(cx, |ws, _| {
        let r = ws.session.project.tracks[t].regions.last().unwrap();
        (r.start, r.length)
    });
    assert_eq!(last, (0, BAR));
    std::fs::remove_dir_all(dir).unwrap();
}
