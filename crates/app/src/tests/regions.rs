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
fn an_audio_regions_end_trims_no_further_than_its_file(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let (start, file_end) = ws.read_with(cx, |ws, _| {
        (
            ws.session.project.region((2, 0)).start,
            ws.audio_end((2, 0)).expect("the drums' file loaded"),
        )
    });
    let length =
        |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.session.project.region((2, 0)).length);
    // Its end's bottom half, dragged two bars on: it stops where the drums' audio does.
    let edge = spot(&ws, cx, BAR, 2) + point(px(-2.), px(12.));
    drag(cx, edge, edge + point(px(48. * 8.), px(0.)));
    assert_eq!(length(cx), file_end - start);
    // In by a beat it trims as ever.
    let edge = spot(&ws, cx, file_end, 2) + point(px(-2.), px(12.));
    drag(cx, edge, edge - point(px(48.), px(0.)));
    assert!(length(cx) < file_end - start);
}

#[gpui::test]
fn audio_draws_to_where_it_ends_whatever_rate_the_engine_runs_at(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    // The drums' region a bar longer than its audio: its waveform stops where the audio does.
    let drawn = |cx: &mut VisualTestContext| {
        ws.update(cx, |ws, _| {
            let file_end = ws.audio_end((2, 0)).unwrap();
            let r = &mut ws.session.project.tracks[2].regions[0];
            r.length = file_end - r.start + BAR;
            let (r, ppt) = (r.clone(), ws.px_per_tick());
            let to = ws.inside(&r.content, r.start, r.length).drawn_to().unwrap();
            (to, (file_end - r.start) as f32 * ppt)
        })
    };
    let (to, end) = drawn(cx);
    assert!((to - end).abs() < 1.0, "{to} {end}");
    // An output that can't play the song's 48 kHz runs the engine at 44.1: the same.
    ws.update(cx, |ws, _| {
        let mut slower = ws.session.project.clone();
        slower.sample_rate = 44_100;
        ws.player = Player::silent(&slower, &ws.session.dir);
        ws.player.sync(&ws.session.project).unwrap();
        assert_eq!(ws.player.rate(), 44_100.0);
    });
    let (to, end) = drawn(cx);
    assert!((to - end).abs() < 1.0, "{to} {end}");
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
fn an_audio_regions_stretch_mode_and_tempo_following_are_set_in_its_inspector(
    cx: &mut TestAppContext,
) {
    use pina_engine::song::project::StretchMode;
    let (ws, cx) = open(cx);
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    cx.simulate_click(spot(&ws, cx, PPQ, drums), Modifiers::none());
    click_on(
        cx,
        "region-stretch-mode",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(cx, "menu-1", Modifiers::none(), MouseButton::Left); // Slicing
    click_on(cx, "region-follow", Modifiers::none(), MouseButton::Left);
    click_on(cx, "menu-1", Modifiers::none(), MouseButton::Left); // Follow
    use pina_engine::song::project::Follow;
    let audio = |ws: &Entity<Workspace>, cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            match &ws.session.project.tracks[drums].regions[0].content {
                Content::Audio {
                    stretch_mode,
                    follow,
                    tempo,
                    ..
                } => (*stretch_mode, *follow, *tempo),
                c => panic!("{c:?}"),
            }
        })
    };
    // Following, its own tempo is the song's where it is.
    let song = ws.read_with(cx, |ws, _| ws.session.project.tempo);
    assert_eq!(
        audio(&ws, cx),
        (StretchMode::Slicing, Follow::On, Some(song))
    );
    // The tempo halved: half way through the bar it's played only a quarter of its file's bar.
    let frame = |ws: &Entity<Workspace>, cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            let p = &ws.session.project;
            let r = &p.tracks[drums].regions[0];
            p.frame_at(&r.content, r.start as f64, (r.start + BAR / 2) as f64)
                .unwrap()
        })
    };
    let before = frame(&ws, cx);
    ws.update(cx, |ws, cx| {
        ws.edit(cx, |ws| {
            let p = &mut ws.session.project;
            p.set_tempo(p.tempo / 2.0);
        })
    });
    assert_eq!(frame(&ws, cx), before);
    // Undo takes the tempo back.
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(ws.read_with(cx, |ws, _| ws.session.project.tempo), song);
    // Its own tempo doubled: heard at double time.
    let value = cx.debug_bounds("region-tempo").unwrap();
    let double = point(value.right() + px(12.), value.center().y);
    cx.simulate_click(double, Modifiers::none());
    assert_eq!(audio(&ws, cx).2, Some(song * 2.0));
}

#[gpui::test]
fn shift_t_stretches_audio_to_the_tempo_and_back(cx: &mut TestAppContext) {
    use pina_engine::song::project::Follow;
    let (ws, cx) = open(cx);
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    // Its own tempo known, a fifth slower than the song's.
    let (song, was) = ws.update(cx, |ws, cx| {
        let song = ws.session.project.tempo;
        ws.edit(cx, |ws| {
            ws.session.project.set_audio_tempo((drums, 0), song * 0.8)
        });
        (song, ws.session.project.tracks[drums].regions[0].clone())
    });
    cx.simulate_click(spot(&ws, cx, PPQ, drums), Modifiers::none());
    let now = |ws: &Entity<Workspace>, cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            ws.session.project.tracks[drums].regions[0].clone()
        })
    };
    // Stretched to the song: it plays faster, so the same audio takes a fifth less.
    cx.simulate_keystrokes("shift-t");
    let fast = now(&ws, cx);
    assert!(matches!(
        fast.content,
        Content::Audio {
            follow: Follow::On,
            ..
        }
    ));
    let ratio = fast.length as f64 / was.length as f64;
    assert!((ratio - 0.8).abs() < 1e-3, "{ratio} at {song}");
    // Again: back as it was.
    cx.simulate_keystrokes("shift-t");
    assert_eq!(now(&ws, cx), was);
}

#[gpui::test]
fn holding_alt_puts_a_stretch_grip_on_every_region_and_dragging_it_stretches(
    cx: &mut TestAppContext,
) {
    let (ws, cx) = open(cx);
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    let grip: &'static str = format!("stretch-grip-{drums}-0").leak();
    assert!(cx.debug_bounds(grip).is_none());
    cx.simulate_modifiers_change(Modifiers::alt());
    let at = cx
        .debug_bounds(grip)
        .expect("a grip while ⌥ is held")
        .center();
    // Dragged a bar on: its audio plays slower, to fill it.
    let was = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[drums].regions[0].clone()
    });
    let to = point(spot(&ws, cx, was.start + was.length + BAR, drums).x, at.y);
    drag_with(cx, at, to, Modifiers::alt());
    let now = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[drums].regions[0].clone()
    });
    let (Content::Audio { stretch: a, .. }, Content::Audio { stretch: b, .. }) =
        (&was.content, &now.content)
    else {
        unreachable!()
    };
    assert!(b > a && now.length > was.length, "{a} {b}");
    cx.simulate_modifiers_change(Modifiers::none());
    assert!(cx.debug_bounds(grip).is_none());
}

#[gpui::test]
fn a_regions_tempo_being_found_is_said_on_it_till_it_is(cx: &mut TestAppContext) {
    use crate::arrange::tempo::After;
    let (ws, cx) = open(cx);
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    ws.update(cx, |ws, cx| ws.find_tempo((drums, 0), After::Nothing, cx));
    assert_eq!(ws.read_with(cx, |ws, _| ws.listening.len()), 1);
    until(cx, &ws, |ws| ws.listening.is_empty());
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
    let moved = ws.read_with(cx, |ws, _| w.tick(&ws.session.project, from as usize));
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

#[gpui::test]
fn q_on_an_audio_region_puts_its_hits_on_the_grid(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    cx.simulate_click(spot(&ws, cx, PPQ, drums), Modifiers::none());
    let warps = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            let p = &ws.session.project;
            let r = p.region((drums, 0));
            let t =
                pina_engine::song::project::Timing::of(&r.content, p.sample_rate as f64).unwrap();
            let Content::Audio { warps, .. } = &r.content else {
                unreachable!()
            };
            let grid = ws.grid().min(PPQ) as f64;
            let off: Vec<f64> = warps
                .iter()
                .map(|w| {
                    let tick = p.audio_tick(&t, r.start as f64, w[0] as f64);
                    (tick - (tick / grid).round() * grid).abs()
                })
                .collect();
            off
        })
    };
    assert!(warps(cx).is_empty());
    cx.simulate_keystrokes("q");
    let off = warps(cx);
    assert!(!off.is_empty());
    assert!(off.iter().all(|&o| o < 1.0), "{off:?}");
    cx.simulate_keystrokes("cmd-z");
    assert!(warps(cx).is_empty());
}

#[gpui::test]
fn a_recording_dropped_on_blooms_grow_page_is_grown_toward(cx: &mut TestAppContext) {
    use crate::editors::bloom::{BloomOp, BloomPage};
    use gpui::{ExternalPaths, FileDropEvent};
    use pina_engine::song::project::{Instrument, Track};
    let dir = demo_copy("bloom-grow-drop");
    let file = dir.join("Outside A.wav");
    std::fs::create_dir_all(&dir).unwrap();
    tone(&file, 1.0, 220.0);
    let (ws, cx) = open_at(cx, &dir);
    let t = ws.update(cx, |ws, cx| {
        ws.session.project.tracks.push(Track {
            name: "Bloom".into(),
            instrument: Some(crate::sounds::catalog::new_bloom()),
            ..Default::default()
        });
        ws.session.project.assign_ids();
        let t = ws.session.project.tracks.len() - 1;
        ws.open_bloom(t, cx);
        ws.bloom_op(t, BloomOp::Page(BloomPage::Grow), cx);
        t
    });
    cx.run_until_parked();
    let at = cx.debug_bounds("bloom-grow-stage").unwrap().center();
    cx.simulate_event(FileDropEvent::Entered {
        position: at,
        paths: ExternalPaths(std::iter::once(file.clone()).collect()),
    });
    cx.simulate_event(FileDropEvent::Pending { position: at });
    cx.simulate_event(FileDropEvent::Submit { position: at });
    cx.simulate_event(FileDropEvent::Exited);
    cx.run_until_parked();
    until(cx, &ws, |ws| {
        ws.bloom_view
            .as_ref()
            .is_some_and(|v| v.grow.current().is_some())
    });
    let note = ws.read_with(cx, |ws, _| {
        let g = &ws.bloom_view.as_ref().unwrap().grow;
        g.current().unwrap().recording.note
    });
    assert_eq!(note, 57, "heard at A3");
    // Grown a generation, a find tried: it's the seed, the plant before it kept.
    click_on(cx, "bloom-grow-stage", Modifiers::none(), MouseButton::Left);
    until(cx, &ws, |ws| {
        ws.bloom_view.as_ref().is_some_and(|v| {
            v.grow
                .grown
                .as_ref()
                .is_some_and(|g| g.lock().unwrap().generation >= 1)
        })
    });
    let family = ws.read_with(cx, |ws, _| ws.bloom(t).unwrap().ancestors.len());
    ws.update(cx, |ws, cx| ws.bloom_try_find(t, (0, 0), cx));
    let (b, find) = ws.read_with(cx, |ws, _| {
        let g = ws.bloom_view.as_ref().unwrap().grow.grown.clone().unwrap();
        let find = g.lock().unwrap().strands[0].1[0].genes;
        (ws.bloom(t).unwrap().clone(), find)
    });
    assert_eq!(b.seed(), find);
    assert_eq!(b.ancestors.len(), family + 1);
    // Stopped, it stops.
    click_on(cx, "bloom-grow-stage", Modifiers::none(), MouseButton::Left);
    until(cx, &ws, |ws| {
        ws.bloom_view.as_ref().is_some_and(|v| !v.grow.running())
    });
    // A sample from the Browser dropped on the window is another take; nothing lands on the
    // tracks under it.
    let library = little_library(&dir.join("Library"));
    let index = library.samples.len() - 1;
    let path = library.samples[index].path.clone();
    ws.update(cx, |ws, cx| ws.set_library(library, cx));
    let tracks = ws.read_with(cx, |ws, _| ws.session.project.tracks.len());
    let on = ws.read_with(cx, |ws, _| {
        let v = ws.bloom_view.as_ref().unwrap();
        point(v.at.0 + 300.0, v.at.1 + 300.0)
    });
    ws.update(cx, |ws, cx| {
        let drag = crate::workspace::Drag::Sample {
            index,
            from: on,
            at: on,
            moved: true,
        };
        ws.drop_sample(Some(drag), cx)
    });
    until(cx, &ws, |ws| {
        ws.bloom_view
            .as_ref()
            .is_some_and(|v| v.grow.takes.len() == 2)
    });
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks.len()),
        tracks
    );
    let grow = |cx: &VisualTestContext,
                f: &dyn Fn(&crate::editors::bloom::grow::GrowView) -> bool| {
        ws.read_with(cx, |ws, _| f(&ws.bloom_view.as_ref().unwrap().grow))
    };
    assert!(grow(cx, &|g| g.take == 1 && g.takes[1].file == path));
    // Both grown toward together.
    click_on(cx, "bloom-grow-stage", Modifiers::none(), MouseButton::Left);
    until(cx, &ws, |ws| {
        ws.bloom_view.as_ref().is_some_and(|v| {
            v.grow
                .grown
                .as_ref()
                .is_some_and(|g| g.lock().unwrap().generation >= 1)
        })
    });
    click_on(cx, "bloom-grow-stage", Modifiers::none(), MouseButton::Left);
    until(cx, &ws, |ws| {
        ws.bloom_view.as_ref().is_some_and(|v| !v.grow.running())
    });
    // Each recording can be heard, and the seed tried beside it.
    click_on(
        cx,
        "bloom-take-hear-1",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(cx, "bloom-hear-seed", Modifiers::none(), MouseButton::Left);
    click_on(
        cx,
        "bloom-hear-recording",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert!(
        ws.read_with(cx, |ws, _| ws.status.is_none()),
        "{:?}",
        ws.read_with(cx, |ws, _| ws.status.clone())
    );
    // The first shown again; the second taken out.
    click_on(cx, "bloom-take-0", Modifiers::none(), MouseButton::Left);
    assert!(grow(cx, &|g| g.take == 0));
    click_on(cx, "bloom-take-out-1", Modifiers::none(), MouseButton::Left);
    assert!(grow(cx, &|g| g.takes.len() == 1 && g.take == 0));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(matches!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks[t].instrument.clone()),
        Some(Instrument::Bloom(_))
    ));
}

#[gpui::test]
fn audio_opens_in_its_editor_to_trim_fade_reverse_and_become_a_sampler(cx: &mut TestAppContext) {
    let dir = demo_copy("audio-editor");
    let (ws, cx) = open_at(cx, &dir);
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    let at = (drums, 0);
    let region =
        |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.session.project.region(at).clone());
    let file_and_offset = |cx: &VisualTestContext| match region(cx).content {
        Content::Audio { file, offset, .. } => (file, offset),
        c => panic!("{c:?}"),
    };
    // Double-clicked, it opens in the audio editor, its file read.
    double_click(cx, spot(&ws, cx, PPQ, drums));
    assert_eq!(ws.read_with(cx, |ws, _| ws.bottom), Some(Bottom::Audio));
    until(cx, &ws, |ws| ws.audio_end(at).is_some());
    cx.run_until_parked();
    // Its start dragged in: it starts later, what it plays where it was.
    let wave = cx.debug_bounds("audio-wave").unwrap();
    let was = (region(cx).start, file_and_offset(cx).1);
    let mark = cx.debug_bounds("clip-mark-Start").unwrap().center();
    drag(cx, mark, mark + point(wave.size.width * 0.05, px(0.)));
    let now = (region(cx).start, file_and_offset(cx).1);
    assert!(now.0 > was.0 && now.1 > was.1, "{was:?} {now:?}");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!((region(cx).start, file_and_offset(cx).1), was);
    // Its gain up from its field; a fade in from its handle.
    let gain = cx.debug_bounds("clip-Gain").unwrap().center();
    drag(cx, gain, gain - point(px(0.), px(30.)));
    assert!(region(cx).gain_db > 0.5, "{}", region(cx).gain_db);
    let fade = cx.debug_bounds("clip-mark-FadeIn").unwrap().center();
    drag(cx, fade, fade + point(wave.size.width * 0.03, px(0.)));
    assert!(region(cx).fade_in > 0);
    // Reversed: a reversed copy beside it, the region on the same part of it; again, back.
    let (file, offset) = file_and_offset(cx);
    click_on(cx, "clip-reverse", Modifiers::none(), MouseButton::Left);
    until(cx, &ws, |ws| match &ws.session.project.region(at).content {
        Content::Audio { file: f, .. } => *f != file,
        _ => false,
    });
    let (reversed, _) = file_and_offset(cx);
    assert!(dir.join(&reversed).is_file(), "{reversed:?}");
    click_on(cx, "clip-reverse", Modifiers::none(), MouseButton::Left);
    until(cx, &ws, |ws| match &ws.session.project.region(at).content {
        Content::Audio { file: f, .. } => *f == file,
        _ => false,
    });
    let back = file_and_offset(cx).1;
    assert!(back.abs_diff(offset) <= 1, "{offset} {back}");
    // Sliced to a Sampler track after it, from its bar's menu: at its hits or on the grid. Every
    // quarter note, its bar is four slices, each played where it was; the audio muted, the
    // Sampler open on it.
    click_on(cx, "clip-slice", Modifiers::none(), MouseButton::Left);
    let choices = ws.read_with(cx, |ws, _| {
        ws.menu
            .as_ref()
            .map(|m| m.items.iter().map(|i| i.0.clone()).collect::<Vec<_>>())
    });
    let choices = choices.expect("how to slice it");
    assert_eq!(choices[0], "At its hits");
    let quarters = choices.iter().position(|c| c == "Every 1/4 note").unwrap();
    ws.update(cx, |ws, cx| {
        let action = ws.menu.take().unwrap().items[quarters].1.clone();
        ws.choose(action, cx);
    });
    let (zones, notes, muted, bottom) = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        let zones = match &p.tracks[drums + 1].instrument {
            Some(pina_engine::song::project::Instrument::Sampler(s)) => s.zones.len(),
            _ => 0,
        };
        let notes = match &p.tracks[drums + 1].regions[0].content {
            Content::Midi { notes, .. } => notes.iter().map(|n| n.start).collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        (zones, notes, p.region(at).mute, ws.bottom)
    });
    let q = PPQ as i64;
    assert_eq!((zones, notes), (4, vec![0, q, q * 2, q * 3]));
    assert!(muted && bottom == Some(Bottom::Sampler));
    let _ = std::fs::remove_dir_all(dir);
}

#[gpui::test]
fn an_audio_region_is_separated_into_stems_from_a_sheet_picking_the_model_and_stems(
    cx: &mut TestAppContext,
) {
    use crate::workspace::context::RegionOp;
    let models = test_models();
    let dir = demo_copy("stems");
    let (ws, cx) = open_at(cx, &dir);
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    let at = (drums, 0);
    let tracks = ws.read_with(cx, |ws, _| ws.session.project.tracks.len());
    let items = ws.read_with(cx, |ws, _| ws.stems_items(at));
    if !pina_engine::separate::available() {
        assert_eq!(items[0].0, "Separate Stems (needs Apple silicon)");
        return;
    }
    // The region's menu opens the sheet, the model there is picked, every stem kept, the
    // region to be muted.
    assert_eq!(items[0].0, "Separate Stems…");
    ws.update(cx, |ws, cx| ws.region_op(at, RegionOp::Stems, cx));
    cx.run_until_parked();
    let (model, without, mute) = ws.read_with(cx, |ws, _| {
        let s = ws.stems_sheet.as_ref().unwrap();
        (s.model.clone(), s.without.clone(), s.mute)
    });
    assert_eq!(
        (model.as_deref(), without.len(), mute),
        (Some("Test Splitter"), 0, true)
    );
    assert!(
        cx.debug_bounds("stems-model").is_some(),
        "the model, a dropdown"
    );
    // Its stems made before by this model are kept: no model runs, they go straight in. The
    // rest left out.
    let made = dir.join("Audio Files/Stems/drums · Test Splitter");
    std::fs::create_dir_all(&made).unwrap();
    for stem in ["kick", "rest"] {
        std::fs::copy(
            dir.join("Audio Files/drums.wav"),
            made.join(format!("{stem}.wav")),
        )
        .unwrap();
    }
    click_on(cx, "stems-keep-rest", Modifiers::none(), MouseButton::Left);
    click_on(cx, "stems-go", Modifiers::none(), MouseButton::Left);
    until(cx, &ws, |ws| ws.separating.is_none());
    cx.run_until_parked();
    let (names, muted, stack, sheet) = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        let names: Vec<String> = p.tracks[drums + 1..]
            .iter()
            .map(|t| t.name.clone())
            .collect();
        let stack = p.stack_of(drums + 1).map(|a| a.name.clone());
        (names, p.region(at).mute, stack, ws.stems_sheet.is_some())
    });
    assert_eq!(names.first().map(String::as_str), Some("kick"));
    assert!(!names.iter().any(|n| n == "rest"), "{names:?}");
    assert!(muted, "the region muted");
    assert_eq!(stack.as_deref(), Some("Drums Stems"));
    assert!(!sheet, "the sheet put away once they're in");
    // One undo: as it was.
    ws.update(cx, |ws, cx| {
        ws.session.undo();
        ws.changed(cx);
    });
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks.len()),
        tracks
    );
    let _ = std::fs::remove_dir_all(dir);
    let _ = models;
}

#[gpui::test]
fn an_audio_regions_lyrics_show_beside_it_and_a_line_becomes_a_loop_on_the_bars(
    cx: &mut TestAppContext,
) {
    use pina_engine::lyrics::Line;
    let whisper = test_whisper();
    let dir = demo_copy("lyrics");
    // Heard before, by Whisper small: kept beside the song's audio, read back.
    let line = |text: &str, start: f32, end: f32, sung: usize| Line {
        text: text.into(),
        start,
        end,
        sung,
        words: Vec::new(),
    };
    let kept = dir.join("Audio Files/Lyrics/drums · Whisper small.json");
    std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
    let lines = vec![
        line("Ain't no holding back", 0.5, 1.8, 2),
        line("Because you are all I see", 2.0, 3.0, 1),
    ];
    std::fs::write(&kept, serde_json::to_string(&lines).unwrap()).unwrap();
    let (ws, cx) = open_at(cx, &dir);
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    let at = (drums, 0);
    ws.update(cx, |ws, cx| {
        ws.select_track(drums);
        ws.region = Some(at);
        ws.show_bottom(Bottom::Audio, cx);
        ws.hear_lyrics(at, cx);
    });
    until(cx, &ws, |ws| ws.lyrics.is_some());
    cx.run_until_parked();
    let heard = ws.read_with(cx, |ws, _| {
        ws.lyrics.as_ref().map(|l| (l.model, l.lines.clone()))
    });
    assert_eq!(heard, Some(("Whisper small", lines)));
    // A line as a loop: on the bars, on the track's lyrics track, ready to loop.
    let tracks = ws.read_with(cx, |ws, _| ws.session.project.tracks.len());
    assert!(
        cx.debug_bounds("lyrics-loop-hooks").is_some(),
        "a hook to loop"
    );
    // The hook's card loops it, as its line in the list below does.
    assert!(cx.debug_bounds("lyric-loop-0").is_some());
    click_on(cx, "hook-loop-0", Modifiers::none(), MouseButton::Left);
    let (name, region) = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        let t = p.tracks.iter().find(|t| t.name == "Drums Lyrics").unwrap();
        (t.name.clone(), t.regions[0].clone())
    });
    assert_eq!(name, "Drums Lyrics");
    assert_eq!(region.name, "Ain't no holding back");
    assert!(region.looped);
    let bar = ws.read_with(cx, |ws, _| ws.session.project.bar());
    assert_eq!(region.start % bar, 0, "on a bar line");
    // One undo: as it was.
    ws.update(cx, |ws, cx| {
        ws.session.undo();
        ws.changed(cx);
    });
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks.len()),
        tracks
    );
    let _ = std::fs::remove_dir_all(dir);
    let _ = whisper;
}

#[gpui::test]
fn a_line_of_lyrics_dragged_onto_the_tracks_becomes_a_loop_where_its_let_go(
    cx: &mut TestAppContext,
) {
    use pina_engine::lyrics::{Line, Word};
    let _ = test_whisper();
    let dir = demo_copy("lyrics-drag");
    let kept = dir.join("Audio Files/Lyrics/drums · Whisper small.json");
    std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
    let word = |text: &str, start: f32, end: f32| Word {
        text: text.into(),
        start,
        end,
    };
    let lines = vec![Line {
        text: "Ain't no holding back".into(),
        start: 0.5,
        end: 1.8,
        sung: 1,
        words: vec![
            word("Ain't", 0.5, 0.8),
            word("no", 0.8, 1.0),
            word("holding", 1.0, 1.4),
            word("back", 1.4, 1.8),
        ],
    }];
    std::fs::write(&kept, serde_json::to_string(&lines).unwrap()).unwrap();
    let (ws, cx) = open_at(cx, &dir);
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    let at = (drums, 0);
    ws.update(cx, |ws, cx| {
        ws.select_track(drums);
        ws.region = Some(at);
        ws.show_bottom(Bottom::Audio, cx);
        ws.hear_lyrics(at, cx);
    });
    until(cx, &ws, |ws| ws.lyrics.is_some());
    cx.run_until_parked();
    // Its words are on the waveform, where they're sung.
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.lyrics.as_ref().unwrap().lines[0].words.len()),
        4
    );
    // Dragged onto the first track (an instrument's): a loop on Drums Lyrics, at the bar there.
    let line = cx.debug_bounds("lyric-line-0").unwrap().center();
    let to = spot(&ws, cx, PPQ * 8 + PPQ / 2, 0);
    drag(cx, line, to);
    let (name, start) = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        let t = p.tracks.iter().find(|t| t.name == "Drums Lyrics").unwrap();
        (t.regions[0].name.clone(), t.regions[0].start)
    });
    assert_eq!(name, "Ain't no holding back");
    assert_eq!(start, PPQ * 8, "on the bar where it was let go");
    assert!(ws.read_with(cx, |ws, _| ws.drag.is_none()));
    let _ = std::fs::remove_dir_all(dir);
}

#[gpui::test]
fn a_line_of_lyrics_chopped_is_a_sampler_with_a_chop_a_word_open_to_flip(cx: &mut TestAppContext) {
    use pina_engine::lyrics::{Line, Word};
    use pina_engine::song::project::Instrument;
    let _ = test_whisper();
    let dir = demo_copy("lyrics-chop");
    let kept = dir.join("Audio Files/Lyrics/drums · Whisper small.json");
    std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
    let word = |text: &str, start: f32, end: f32| Word {
        text: text.into(),
        start,
        end,
    };
    let line = |start: f32| Line {
        text: "Ain't no holding back".into(),
        start,
        end: start + 1.3,
        sung: 2,
        words: vec![
            word("Ain't", start, start + 0.3),
            word("no", start + 0.3, start + 0.5),
            word("holding", start + 0.5, start + 0.9),
            word("back", start + 0.9, start + 1.3),
        ],
    };
    std::fs::write(
        &kept,
        serde_json::to_string(&[line(0.5), line(2.5)]).unwrap(),
    )
    .unwrap();
    let (ws, cx) = open_at(cx, &dir);
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    let at = (drums, 0);
    ws.update(cx, |ws, cx| {
        ws.select_track(drums);
        ws.region = Some(at);
        ws.show_bottom(Bottom::Audio, cx);
        ws.hear_lyrics(at, cx);
    });
    until(cx, &ws, |ws| ws.lyrics.is_some());
    cx.run_until_parked();
    // Sung twice: one hook card, with its wave, and both lines below it.
    assert!(cx.debug_bounds("lyric-hook-0").is_some());
    assert!(
        cx.debug_bounds("lyric-hook-1").is_none(),
        "a hook shows once"
    );
    assert!(cx.debug_bounds("lyric-line-1").is_some());
    let tracks = ws.read_with(cx, |ws, _| ws.session.project.tracks.len());
    click_on(cx, "hook-chop-0", Modifiers::none(), MouseButton::Left);
    // The new track is picked: a Sampler with a chop a word, its notes playing them in order.
    let (zones, notes, bottom) = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        let t = ws.track;
        let Some(Instrument::Sampler(s)) = &p.tracks[t].instrument else {
            panic!("no sampler on the picked track")
        };
        (s.zones.len(), p.chop_order((t, 0)), ws.bottom)
    });
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks.len()),
        tracks + 1
    );
    assert_eq!(zones, 4, "a chop a word");
    assert_eq!(notes, [0, 1, 2, 3]);
    assert_eq!(bottom, Some(Bottom::Sampler), "open in the Sampler to flip");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_agent_separates_a_region_hears_its_lines_and_makes_one_a_loop() {
    use crate::agent::mcp::call;
    use serde_json::json;
    let _ = (test_models(), test_whisper());
    let dir = demo_copy("agent-stems");
    let p = pina_engine::song::project::Project::load(&dir).unwrap();
    let drums = p.tracks.iter().position(|t| t.name == "Drums").unwrap();
    // The stand-in splitter's stems, made before: kept, so no model runs.
    let made = dir.join("Audio Files/Stems/drums · Test Splitter");
    std::fs::create_dir_all(&made).unwrap();
    for stem in ["kick", "rest"] {
        std::fs::copy(
            dir.join("Audio Files/drums.wav"),
            made.join(format!("{stem}.wav")),
        )
        .unwrap();
    }
    let models = call("stem_models", &json!({})).unwrap();
    assert_eq!(models["models"][0]["name"], "Test Splitter");
    let stems = call(
        "stems",
        &json!({"project": dir, "track": drums, "region": 0}),
    )
    .unwrap();
    assert_eq!(stems["stems"][0]["name"], "kick");
    let p = pina_engine::song::project::Project::load(&dir).unwrap();
    assert_eq!(p.tracks[drums + 1].name, "kick");
    assert!(p.tracks[drums].regions[0].mute);
    // Its lines, heard before by Whisper small: read back.
    let kept = dir.join("Audio Files/Lyrics/drums · Whisper small.json");
    std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
    std::fs::write(
        &kept,
        r#"[{"text": "Ain't no holding back", "start": 0.5, "end": 1.8, "sung": 2}]"#,
    )
    .unwrap();
    let heard = call(
        "lyrics",
        &json!({"project": dir, "track": drums, "region": 0}),
    )
    .unwrap();
    assert_eq!(heard["lines"][0]["text"], "Ain't no holding back");
    assert_eq!(heard["lines"][0]["sung"], 2);
    assert_eq!(heard["from_vocals_stem"], false);
    // The line as a loop, as the agent makes it.
    let looped = call(
        "edit",
        &json!({"project": dir, "command": {
            "op": "lyric_loop", "track": drums, "region": 0, "from": 0.5, "to": 1.8,
            "name": "Ain't no holding back"
        }}),
    )
    .unwrap();
    assert_eq!(looped["bars"], 1);
    let p = pina_engine::song::project::Project::load(&dir).unwrap();
    assert!(p.tracks.iter().any(|t| {
        t.name == "Drums Lyrics"
            && t.regions
                .first()
                .is_some_and(|r| r.name == "Ain't no holding back")
    }));
    let _ = std::fs::remove_dir_all(dir);
}

#[gpui::test]
fn lyrics_say_what_theyre_waiting_for_and_the_hooks_loop_at_once(cx: &mut TestAppContext) {
    use crate::editors::audio::LyricsWait;
    use pina_engine::lyrics::Line;
    let _ = test_whisper();
    let dir = demo_copy("lyrics-states");
    let (ws, cx) = open_at(cx, &dir);
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    let at = (drums, 0);
    let file = ws.read_with(cx, |ws, _| match &ws.session.project.region(at).content {
        Content::Audio { file, .. } => file.clone(),
        c => panic!("{c:?}"),
    });
    // Waiting for Whisper: the panel offers it, to download.
    ws.update(cx, |ws, cx| {
        ws.select_track(drums);
        ws.region = Some(at);
        ws.show_bottom(Bottom::Audio, cx);
        ws.lyrics_wait = Some(LyricsWait {
            file: file.clone(),
            at,
            hearing: None,
            failed: None,
        });
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("lyrics-get-whisper").is_some());
    // Heard: two hooks, one sung twice, loop at once, one after the other, on the bars.
    let line = |text: &str, start: f32, end: f32, sung: usize| Line {
        text: text.into(),
        start,
        end,
        sung,
        words: Vec::new(),
    };
    ws.update(cx, |ws, cx| {
        ws.lyrics_wait = None;
        ws.lyrics = Some(crate::editors::audio::Lyrics::shown(
            file.clone(),
            "Whisper small",
            vec![
                line("Euphoria", 0.1, 0.6, 2),
                line("Set me free", 0.7, 1.2, 1),
                line("Euphoria", 1.3, 1.8, 2),
                line("Oh oh", 1.9, 2.3, 2),
            ],
        ));
        cx.notify();
    });
    cx.run_until_parked();
    click_on(
        cx,
        "lyrics-loop-hooks",
        Modifiers::none(),
        MouseButton::Left,
    );
    let loops = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        let t = p.tracks.iter().find(|t| t.name == "Drums Lyrics").unwrap();
        t.regions
            .iter()
            .map(|r| (r.name.clone(), r.start))
            .collect::<Vec<_>>()
    });
    let bar = ws.read_with(cx, |ws, _| ws.session.project.bar());
    assert_eq!(
        loops,
        [("Euphoria".to_string(), 0), ("Oh oh".to_string(), bar)]
    );
    let _ = std::fs::remove_dir_all(dir);
}
