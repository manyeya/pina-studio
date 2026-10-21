//! Tracks: the lanes, headers and their menus, picking tracks, stacks, renaming, the New Track palette.

use super::*;

#[gpui::test]
fn the_window_lays_out_the_lanes(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let b = ws.read_with(cx, |ws, _| ws.lanes_bounds.get());
    assert!(b.size.width > px(100.) && b.size.height > px(100.), "{b:?}");
}

#[gpui::test]
fn a_tracks_menu_its_pan_modes_its_height_and_the_columns_widths(cx: &mut TestAppContext) {
    use pina_engine::song::project::{PanLaw, PanMode};
    let (ws, cx) = open(cx);
    // The pan knob's menu: Stereo Pan, then the song's pan law.
    click_on(cx, "track-pan-1", Modifiers::none(), MouseButton::Right);
    pick(cx, &ws, "Stereo Pan");
    click_on(cx, "track-pan-1", Modifiers::none(), MouseButton::Right);
    pick(cx, &ws, "−3 dB");
    let (mode, law) = ws.read_with(cx, |ws, _| {
        (
            ws.session.project.tracks[1].pan_mode,
            ws.session.project.pan_law,
        )
    });
    assert_eq!((mode, law), (PanMode::Stereo, PanLaw::Minus3));
    // ⌘-dragging a stereo pan's knob down narrows it.
    let knob = cx.debug_bounds("track-pan-1").unwrap().center();
    let down = knob + point(px(0.), px(50.));
    cx.simulate_mouse_down(knob, MouseButton::Left, Modifiers::command());
    cx.simulate_mouse_move(down, MouseButton::Left, Modifiers::command());
    cx.simulate_mouse_up(down, MouseButton::Left, Modifiers::command());
    cx.run_until_parked();
    let spread = ws.read_with(cx, |ws, _| ws.session.project.tracks[1].spread);
    assert!(spread.is_some_and(|s| (s - 0.5).abs() < 0.01), "{spread:?}");

    // The header's menu: duplicating, then a height.
    let (n, name, regions) = ws.read_with(cx, |ws, _| {
        let t = &ws.session.project.tracks;
        (t.len(), t[1].name.clone(), t[1].regions.len())
    });
    click_on(cx, "header-1", Modifiers::none(), MouseButton::Right);
    pick(cx, &ws, "Duplicate Track");
    let copy = ws.read_with(cx, |ws, _| ws.session.project.tracks[2].clone());
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks.len()),
        n + 1
    );
    assert_eq!(
        (copy.name, copy.regions.len()),
        (format!("{name} 2"), regions)
    );
    click_on(cx, "header-0", Modifiers::none(), MouseButton::Right);
    pick(cx, &ws, "Height: Large");
    assert_eq!(ws.read_with(cx, |ws, _| ws.lane_h(0)), 120.0);
    // A header's bottom edge, dragged: its lane's height; the rows under it move down.
    let edge = cx.debug_bounds("track-edge-1").unwrap().center();
    let below = ws.read_with(cx, |ws, _| ws.row_top(2));
    drag(cx, edge, edge + point(px(0.), px(30.)));
    let (h, top) = ws.read_with(cx, |ws, _| (ws.lane_h(1), ws.row_top(2)));
    assert_eq!((h, top), (LANE_H + 30.0, below + 30.0));

    // The headers column and the side panel, wider by their right edges.
    let (header_w, side_w) = ws.read_with(cx, |ws, _| (ws.header_w, ws.side_w()));
    let edge = cx.debug_bounds("headers-edge").unwrap().center();
    drag(cx, edge, edge + point(px(50.), px(0.)));
    let edge = cx.debug_bounds("side-edge").unwrap().center();
    drag(cx, edge, edge + point(px(40.), px(0.)));
    let widths = ws.read_with(cx, |ws, _| (ws.header_w, ws.side_w()));
    assert_eq!(widths, (header_w + 50.0, side_w + 40.0));
}

#[gpui::test]
fn a_headers_fader_and_pan_drag_finely_type_and_reset(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let track = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            let t = &ws.session.project.tracks[0];
            (t.volume_db, t.pan)
        })
    };
    let close = |a: f32, b: f32| (a - b).abs() < 0.011;
    ws.update(cx, |ws, _| ws.session.project.tracks[0].pan = 0.0);
    // Dragged right 30 px, the knob turns right 30; then 50 more with ⇧, only 10.
    let knob = cx.debug_bounds("track-pan-0").unwrap().center();
    cx.simulate_mouse_down(knob, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        knob + point(px(30.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    assert!(close(track(cx).1, 0.3), "{:?}", track(cx));
    cx.simulate_mouse_move(
        knob + point(px(80.), px(0.)),
        MouseButton::Left,
        Modifiers::shift(),
    );
    cx.simulate_mouse_up(
        knob + point(px(80.), px(0.)),
        MouseButton::Left,
        Modifiers::shift(),
    );
    cx.run_until_parked();
    assert!(close(track(cx).1, 0.4), "{:?}", track(cx));
    // Dragged back left past the middle, it catches there first.
    cx.simulate_mouse_down(knob, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        knob - point(px(60.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    assert_eq!(track(cx).1, 0.0);
    cx.simulate_mouse_up(
        knob - point(px(60.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    // Typed: a level and a pan.
    let db = cx.debug_bounds("track-db-0").unwrap().center();
    double_click(cx, db);
    cx.simulate_input("-12");
    cx.simulate_keystrokes("enter");
    let knob = cx.debug_bounds("track-pan-0").unwrap().center();
    double_click(cx, knob);
    cx.simulate_input("L25");
    cx.simulate_keystrokes("enter");
    assert_eq!(track(cx), (-12.0, -0.25));
    // Nonsense is said so, and changes nothing.
    double_click(cx, db);
    cx.simulate_input("loud");
    cx.simulate_keystrokes("enter");
    assert_eq!(track(cx), (-12.0, -0.25));
    assert!(
        ws.read_with(cx, |ws, _| ws.status.clone())
            .unwrap()
            .contains("isn't a level")
    );
    // ⌥-click: 0 dB, and the middle.
    click_on(cx, "track-fader-0", Modifiers::alt(), MouseButton::Left);
    click_on(cx, "track-pan-0", Modifiers::alt(), MouseButton::Left);
    assert_eq!(track(cx), (0.0, 0.0));
}

#[gpui::test]
fn tracks_picked_with_cmd_and_shift_go_into_one_stack(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let picked = |cx: &mut VisualTestContext| ws.read_with(cx, |ws, _| ws.picked_tracks());
    let shift = Modifiers {
        shift: true,
        ..Modifiers::none()
    };
    // Its header's top corner, clear of its knobs and buttons.
    let header = |cx: &mut VisualTestContext, i: usize, m: Modifiers, button: MouseButton| {
        let name = ["header-0", "header-1", "header-2"][i];
        let at = cx.debug_bounds(name).unwrap().origin + point(px(6.), px(3.));
        cx.simulate_mouse_down(at, button, m);
        cx.simulate_mouse_up(at, button, m);
        cx.run_until_parked();
    };
    header(cx, 0, Modifiers::none(), MouseButton::Left);
    header(cx, 2, Modifiers::command(), MouseButton::Left);
    assert_eq!(picked(cx), [0, 2]);
    // ⇧: everything from the selected track (the last ⌘-clicked) to it.
    header(cx, 0, shift, MouseButton::Left);
    assert_eq!(picked(cx), [0, 1, 2]);
    header(cx, 1, Modifiers::command(), MouseButton::Left);
    assert_eq!(picked(cx), [0, 2]);
    // The menu of a picked track offers to stack them all; ⇧⌘D does it.
    header(cx, 2, Modifiers::none(), MouseButton::Right);
    assert_eq!(picked(cx), [0, 2]);
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("cmd-shift-d");
    let outputs = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        let stack = p.auxes.iter().find(|a| a.stack).map(|a| a.bus);
        (stack, p.tracks.iter().map(|t| t.output).collect::<Vec<_>>())
    });
    let stack = outputs.0.expect("a stack");
    // Gathered under the first: the drums moved up beside the keys.
    assert_eq!(outputs.1, [Some(stack), Some(stack), None]);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks[1].name.clone()),
        "Drums"
    );
    // A plain click picks one again.
    header(cx, 1, Modifiers::none(), MouseButton::Left);
    assert_eq!(picked(cx), [1]);
}

#[gpui::test]
fn a_stack_has_a_row_of_its_own_folds_its_tracks_away_and_flattens(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| ws.make_stack(&[0, 2], cx));
    cx.run_until_parked();
    // Its row above its tracks, and its channel picked.
    let row = cx.debug_bounds("stack-0").expect("the stack's row");
    let keys = cx.debug_bounds("header-0").unwrap();
    assert!(row.origin.y < keys.origin.y);
    assert!(ws.read_with(cx, |ws, _| ws.channel == Channel::Aux(0)));
    // Folded: its tracks' rows go, and the lead comes up under it.
    let lead = cx.debug_bounds("header-2").unwrap().origin.y;
    click_on(cx, "stack-fold-0", Modifiers::none(), MouseButton::Left);
    assert!(cx.debug_bounds("header-0").is_none() && cx.debug_bounds("header-1").is_none());
    assert!(cx.debug_bounds("header-2").unwrap().origin.y < lead);
    click_on(cx, "stack-fold-0", Modifiers::none(), MouseButton::Left);
    assert!(cx.debug_bounds("header-1").is_some());
    // Flattened from its menu: the tracks play into the master again.
    click_on(cx, "stack-0", Modifiers::none(), MouseButton::Right);
    pick(cx, &ws, "Flatten Stack");
    ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        assert!(p.auxes.iter().all(|a| !a.stack));
        assert!(p.tracks.iter().all(|t| t.output.is_none()));
    });
    assert!(cx.debug_bounds("stack-0").is_none());
}

#[gpui::test]
fn mute_solo_and_arm_from_the_track_header(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    click_on(cx, "mute-1", Modifiers::none(), MouseButton::Left);
    click_on(cx, "solo-2", Modifiers::none(), MouseButton::Left);
    let (mute, solo) = ws.read_with(cx, |ws, _| {
        let t = &ws.session.project.tracks;
        (t[1].mute, t[2].solo)
    });
    assert!(mute && solo);
    // The selected track is armed until another is: R arms that one instead, whatever's
    // selected; R on the armed one disarms it, and recording then says to arm one.
    assert_eq!(ws.read_with(cx, |ws, _| ws.armed()), Some(0));
    click_on(cx, "arm-2", Modifiers::none(), MouseButton::Left);
    let (armed, selected) = ws.read_with(cx, |ws, _| (ws.armed(), ws.track));
    assert_eq!((armed, selected), (Some(2), 0));
    click_on(cx, "arm-2", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.armed()), None);
    cx.simulate_keystrokes("r");
    let (recording, status) = ws.read_with(cx, |ws, _| (ws.recording.is_some(), ws.status.clone()));
    assert!(!recording && status.unwrap().contains("Arm a track"));
}

#[gpui::test]
fn the_new_track_palette_adds_beats_samplers_and_instruments(cx: &mut TestAppContext) {
    use pina_engine::song::project::Instrument;
    let (ws, cx) = open(cx);
    let last = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            let t = &ws.session.project.tracks[ws.track];
            (t.instrument.clone(), t.regions.len(), ws.bottom)
        })
    };
    // Beat: a drum kit with a pattern, in the step sequencer.
    click_on(cx, "new-track", Modifiers::none(), MouseButton::Left);
    click_on(cx, "adder-Beat", Modifiers::none(), MouseButton::Left);
    let (instrument, regions, bottom) = last(cx);
    assert_eq!(
        (instrument, regions, bottom),
        (Some(Instrument::Drums), 1, Some(Bottom::Steps))
    );
    // Sampler: an empty one, in its editor.
    click_on(cx, "new-track", Modifiers::none(), MouseButton::Left);
    click_on(cx, "adder-Sampler", Modifiers::none(), MouseButton::Left);
    let (instrument, _, bottom) = last(cx);
    assert!(matches!(instrument, Some(Instrument::Sampler(_))) && bottom == Some(Bottom::Sampler));
    // Instrument narrows the list to instruments; Pina Studio's synth is first.
    click_on(cx, "new-track", Modifiers::none(), MouseButton::Left);
    click_on(cx, "adder-Instrument", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| ws.adder.is_some()));
    click_on(cx, "adder-row-0", Modifiers::none(), MouseButton::Left);
    assert_eq!(last(cx).0, Some(Instrument::Synth));
    assert!(ws.read_with(cx, |ws, _| ws.adder.is_none()));
}

#[gpui::test]
fn shortcuts_add_tracks_undo_and_play(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let tracks = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.session.project.tracks.len());
    // ⌥⌘N: the New Track palette, its search typed in; Enter adds what's picked.
    cx.simulate_keystrokes("cmd-alt-n");
    assert!(ws.read_with(cx, |ws, _| ws.adder.is_some()));
    cx.simulate_input("audio");
    cx.simulate_keystrokes("enter");
    assert_eq!(tracks(cx), 4);
    let (instrument, open) = ws.read_with(cx, |ws, _| {
        (
            ws.session.project.tracks[ws.track].instrument.clone(),
            ws.adder.is_some(),
        )
    });
    assert!(instrument.is_none() && !open);
    // Arrows move down the list; Escape closes it.
    cx.simulate_keystrokes("cmd-alt-n down down escape");
    assert!(ws.read_with(cx, |ws, _| ws.adder.is_none()));
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(tracks(cx), 3);
    cx.simulate_keystrokes("space");
    until(cx, &ws, |ws| ws.player.playing());
    cx.simulate_keystrokes("space");
    until(cx, &ws, |ws| !ws.player.playing());
}

#[gpui::test]
fn a_track_stack_and_routing_another_track_into_it(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_click(spot(&ws, cx, PPQ, 1), Modifiers::none()); // select Lead
    cx.simulate_keystrokes("cmd-shift-d");
    let (stack, output) = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        (
            (p.auxes[0].stack, p.auxes[0].name.clone()),
            p.tracks[1].output,
        )
    });
    assert_eq!(stack, (true, "Stack 1".to_string()));
    assert_eq!(output, Some(1));

    // Keys joins it from its strip's output menu.
    cx.simulate_click(spot(&ws, cx, PPQ, 0), Modifiers::none());
    click_on(cx, "output-0", Modifiers::none(), MouseButton::Left);
    click_on(cx, "menu-1", Modifiers::none(), MouseButton::Left); // after Stereo Out: the stack
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks[0].output),
        Some(1)
    );
    cx.simulate_keystrokes("cmd-z cmd-z");
    let p = ws.read_with(cx, |ws, _| ws.session.project.clone());
    assert!(p.auxes.is_empty() && p.tracks.iter().all(|t| t.output.is_none()));
}

#[gpui::test]
fn tracks_regions_and_buses_are_renamed_where_their_names_show(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let at = cx.debug_bounds("track-name-1").unwrap().center();
    double_click(cx, at);
    cx.simulate_input("Hook Synth");
    cx.simulate_keystrokes("enter");
    let name = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| ws.session.project.tracks[1].name.clone())
    };
    assert_eq!(name(cx), "Hook Synth");
    // Typing doesn't reach the shortcuts: "x" would have opened the mixer.
    assert_eq!(ws.read_with(cx, |ws, _| ws.view), View::Arrange);
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(name(cx), "Lead");

    // A region's name, from the inspector; the lane shows it.
    cx.simulate_click(spot(&ws, cx, PPQ, 0), Modifiers::none());
    click_on(cx, "region-name", Modifiers::none(), MouseButton::Left);
    cx.simulate_input("Verse keys");
    cx.simulate_keystrokes("enter");
    let region = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[0].regions[0].name.clone()
    });
    assert_eq!(region, "Verse keys");

    // A bus, in the mixer: double-click its name plate. Clicking away keeps what's typed.
    ws.update(cx, |ws, cx| {
        ws.choose(
            crate::ui::menu::MenuAction::Send {
                track: 0,
                bus: None,
            },
            cx,
        );
        ws.show_channel(crate::mix::strip::Channel::Aux(0), cx);
    });
    cx.run_until_parked();
    let plate = cx.debug_bounds("plate-Aux(0)").unwrap().center();
    double_click(cx, plate);
    cx.simulate_input("Reverb");
    cx.simulate_click(
        spot(&ws, cx, 0, 0) - point(px(400.), px(0.)),
        Modifiers::none(),
    );
    cx.run_until_parked();
    let bus = ws.read_with(cx, |ws, _| ws.session.project.auxes[0].name.clone());
    assert_eq!(bus, "Reverb");
}

#[gpui::test]
fn a_tracks_icon_opens_a_picker_for_its_colour_and_icon(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let tile = cx
        .debug_bounds("track-look-2")
        .expect("track 3's icon")
        .center();
    cx.simulate_click(tile, Modifiers::none());
    cx.run_until_parked();
    assert!(ws.read_with(cx, |ws, _| ws.picker.as_ref().is_some_and(|p| p.track == 2)));
    // The demo's Drums track is drums, by its name.
    let before = ws.read_with(cx, |ws, _| ws.shown(2));
    assert_eq!(before.look.family, pina_engine::music::look::Family::Drums);
    // A swatch colours it, an icon marks it: each one edit, undone in turn.
    let swatch = cx.debug_bounds("swatch-1-9").unwrap().center();
    cx.simulate_click(swatch, Modifiers::none());
    let icon = cx.debug_bounds("icon-808").unwrap().center();
    cx.simulate_click(icon, Modifiers::none());
    cx.run_until_parked();
    let t = ws.read_with(cx, |ws, _| ws.session.project.tracks[2].clone());
    assert!(t.color.is_some());
    assert_eq!(t.icon.as_deref(), Some("808"));
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.shown(2).look.family),
        pina_engine::music::look::Family::Bass
    );
    cx.simulate_keystrokes("cmd-z cmd-z");
    cx.run_until_parked();
    assert_eq!(ws.read_with(cx, |ws, _| ws.shown(2)), before);
}

#[gpui::test]
fn the_inspectors_strips_line_up_row_for_row(cx: &mut TestAppContext) {
    use pina_engine::song::project::Eq;
    let dir = demo_copy("strips-line-up");
    let (ws, cx) = open_at(cx, &dir);
    let lined_up = |cx: &mut VisualTestContext, t: usize| {
        ws.update(cx, |ws, cx| {
            ws.select_track(t);
            cx.notify();
        });
        cx.run_until_parked();
        let named = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
        let track = cx.debug_bounds(named(format!("fader-Track({t})"))).unwrap();
        let master = cx.debug_bounds("fader-Master").unwrap();
        let (a, b) = (
            cx.debug_bounds(named(format!("strip-Track({t})"))).unwrap(),
            cx.debug_bounds("strip-Master").unwrap(),
        );
        (
            track.origin.y == master.origin.y,
            a.size.height == b.size.height,
        )
    };
    // An audio track (its input, a send slot and a pan knob) beside the master (none of them).
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    assert_eq!(lined_up(cx, drums), (true, true));
    // With an EQ, a plugin-less compressor and two sends, still.
    ws.update(cx, |ws, _| {
        let t = &mut ws.session.project.tracks[drums];
        t.eq = Some(Eq::default());
        t.sends = vec![
            AuxSend {
                bus: 1,
                level_db: -6.0,
                pre: false
            };
            2
        ];
    });
    assert_eq!(lined_up(cx, drums), (true, true));
    std::fs::remove_dir_all(dir).unwrap();
}
