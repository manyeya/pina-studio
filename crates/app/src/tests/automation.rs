//! Automation: points and lanes, the modes as controls move, shapes and smoothing.

use super::*;

#[gpui::test]
fn automation_points_are_added_dragged_and_deleted(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_keystrokes("a");
    let volume = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            ws.session.project.tracks[1].automation.volume.clone()
        })
    };
    // Click near the top of Lead's automation lane, under its track, at beat 2: a loud point.
    let lane = cx
        .debug_bounds("auto-lane-1")
        .expect("automation lanes show");
    let top = point(spot(&ws, cx, PPQ, 1).x, lane.origin.y + px(8.));
    cx.simulate_mouse_down(top, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_up(top, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let points = volume(cx);
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].tick, PPQ);
    assert!(points[0].value > 0.0, "{}", points[0].value);

    // Drag it down: quieter.
    let lane_top = f32::from(top.y) - 8.;
    let at = point(
        top.x,
        px(lane_top
            + crate::arrange::automation::to_y(
                pina_engine::song::project::Param::Volume,
                points[0].value,
                64.0,
            )),
    );
    drag(cx, at, at + point(px(0.), px(30.)));
    let moved = volume(cx)[0];
    assert!(moved.value < points[0].value - 6.0, "{moved:?}");
    assert_eq!(moved.tick, PPQ);

    // Option-click deletes it.
    let at = point(
        top.x,
        px(lane_top
            + crate::arrange::automation::to_y(
                pina_engine::song::project::Param::Volume,
                moved.value,
                64.0,
            )),
    );
    cx.simulate_click(
        at,
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    );
    assert!(volume(cx).is_empty());
}

#[gpui::test]
fn latch_records_the_fader_while_playing_until_stop(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_keystrokes("a");
    // Keys' automation mode: Off, Read, Touch, Latch.
    click_on(cx, "auto-mode-0", Modifiers::none(), MouseButton::Left);
    click_on(cx, "menu-3", Modifiers::none(), MouseButton::Left);
    let mode = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].automation.mode);
    assert_eq!(mode, pina_engine::song::project::AutoMode::Latch);

    ws.update(cx, |ws, _| ws.player.play());
    until(cx, &ws, |ws| ws.playhead() > PPQ / 4);
    // Pull Keys' fader (in the inspector) down while it plays, and let go.
    let fader = cx.debug_bounds("fader-Track(0)").unwrap();
    let cap = point(fader.center().x, fader.origin.y + px(30.));
    drag(cx, cap, cap + point(px(0.), px(40.)));
    let written = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[0].automation.volume.clone()
    });
    assert!(!written.is_empty());
    let held = written.last().unwrap().value;
    assert!(held < -1.0, "{held}");
    // Latched: it carries on writing that value as the song plays...
    let from = written.last().unwrap().tick;
    until(cx, &ws, |ws| {
        ws.session.project.tracks[0]
            .automation
            .volume
            .last()
            .is_some_and(|p| p.tick > from + PPQ / 2)
    });
    // ...until playback stops.
    ws.update(cx, |ws, cx| ws.stop(cx));
    until(cx, &ws, |ws| ws.writing.is_empty());
    let last = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[0]
            .automation
            .volume
            .last()
            .copied()
    });
    assert_eq!(last.unwrap().value, held);
    // While the automation plays, the fader shows it rather than its own setting.
    let shown = ws.read_with(cx, |ws, _| ws.volume(crate::mix::strip::Channel::Track(0)));
    assert!((shown - held).abs() < 0.01 || shown < 0.0, "{shown}");
}

#[gpui::test]
fn the_automation_menu_narrows_down_as_you_type(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_keystrokes("a");
    click_on(cx, "auto-param-1", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| ws.menu.as_ref().unwrap().searchable));
    cx.simulate_input("pa");
    let shown = ws.read_with(cx, |ws, _| {
        let m = ws.menu.as_ref().unwrap();
        m.shown()
            .into_iter()
            .map(|i| m.items[i].0.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(shown, ["Pan"]);
    cx.simulate_keystrokes("enter");
    let id = ws.read_with(cx, |ws, _| ws.session.project.tracks[1].id);
    let param = ws.read_with(cx, |ws, _| ws.auto_lanes.get(&id).map(|l| l[0]));
    assert_eq!(param, Some(pina_engine::song::project::Param::Pan));
    // Typing didn't reach the shortcuts ("a" hides automation).
    assert!(ws.read_with(cx, |ws, _| ws.show_automation && ws.menu.is_none()));
}

#[gpui::test]
fn controls_show_their_automation_and_moved_while_stopped_move_its_line(cx: &mut TestAppContext) {
    use crate::{arrange::automation::Stopped, mix::strip::Channel};
    use pina_engine::song::project::{AutoPoint, Param, value_at};
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.session.project.tracks[0].automation.volume =
            vec![AutoPoint::new(0, -20.0), AutoPoint::new(BAR, -10.0)];
        ws.changed(cx);
    });
    // The header's fader, the strip's and the readouts all show the automation, not the fader.
    let shown = ws.read_with(cx, |ws, _| {
        (ws.now_value(0, Param::Volume), ws.volume(Channel::Track(0)))
    });
    assert_eq!(shown, (-20.0, -20.0));
    assert!(ws.read_with(cx, |ws, _| ws.auto_mark(0, Param::Volume).is_some()));
    // ⌥-click the header's fader, stopped: 0 dB where the playhead is, the whole line with it.
    click_on(cx, "track-fader-0", Modifiers::alt(), MouseButton::Left);
    let line = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[0].automation.volume.clone()
    });
    assert!(line[0].value.abs() < 0.01, "{line:?}");
    assert!(line[1].value > -10.0 && line[1].value < 6.0, "{line:?}");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.now_value(0, Param::Volume)),
        line[0].value
    );
    // Undone in one.
    cx.simulate_keystrokes("cmd-z");
    let line = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[0].automation.volume.clone()
    });
    assert_eq!(line[0].value, -20.0);
    // Set to put a point instead: the one at the playhead moves, the rest stay.
    ws.update(cx, |ws, _| ws.settings.stopped = Stopped::Point);
    click_on(cx, "track-fader-0", Modifiers::alt(), MouseButton::Left);
    let line = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[0].automation.volume.clone()
    });
    assert_eq!((line[0].value, line[1].value), (0.0, -10.0));
    // Set to last until play: the fader has it, then the automation takes over again.
    ws.update(cx, |ws, _| ws.settings.stopped = Stopped::UntilPlay);
    cx.simulate_keystrokes("cmd-z");
    click_on(cx, "track-fader-0", Modifiers::alt(), MouseButton::Left);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.now_value(0, Param::Volume)),
        0.0
    );
    let line = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[0].automation.volume.clone()
    });
    assert_eq!(value_at(&line, 0.0), Some(-20.0));
    ws.update(cx, |ws, _| ws.player.play());
    until(cx, &ws, |ws| ws.taken.is_empty());
    ws.update(cx, |ws, cx| ws.stop(cx));
    // Mute, by its automation: the button shows it; clicked while stopped, a step there.
    ws.update(cx, |ws, cx| {
        ws.session.project.tracks[2].automation.mute = vec![
            AutoPoint {
                step: true,
                ..AutoPoint::new(0, 0.0)
            },
            AutoPoint {
                step: true,
                ..AutoPoint::new(BAR, 1.0)
            },
        ];
        ws.player.locate(0);
        ws.changed(cx);
    });
    until(cx, &ws, |ws| ws.playhead() == 0);
    assert!(!ws.read_with(cx, |ws, _| ws.muted_now(2)));
    click_on(cx, "mute-2", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| ws.muted_now(2)));
    let mute = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[2].automation.mute.clone()
    });
    assert!(mute[0].step && mute[0].value == 1.0, "{mute:?}");
}

#[gpui::test]
fn read_hands_over_while_held_and_trim_moves_the_line_while_playing(cx: &mut TestAppContext) {
    use pina_engine::song::project::{AutoMode, AutoPoint, Param, value_at};
    let (ws, cx) = open(cx);
    let id = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].id);
    ws.update(cx, |ws, cx| {
        ws.session.project.tracks[0].automation.volume =
            vec![AutoPoint::new(0, -20.0), AutoPoint::new(BAR * 4, -20.0)];
        ws.changed(cx);
        ws.player.play();
    });
    until(cx, &ws, |ws| ws.playhead() > PPQ / 4);
    // Read: held, the fader has it (and the engine hears it); let go, the automation again.
    let fader = cx.debug_bounds("track-fader-0").unwrap().center();
    cx.simulate_mouse_down(fader, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        fader + point(px(30.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    let (taken, now) = ws.read_with(cx, |ws, _| {
        (
            ws.taken.contains_key(&(id, Param::Volume)),
            ws.now_value(0, Param::Volume),
        )
    });
    assert!(taken && now > -20.0, "{now}");
    cx.simulate_mouse_up(
        fader + point(px(30.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    let (taken, now) = ws.read_with(cx, |ws, _| {
        (ws.taken.is_empty(), ws.now_value(0, Param::Volume))
    });
    assert!(taken && now == -20.0, "{now}");
    let line = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[0].automation.volume.clone()
    });
    assert_eq!(line.len(), 2);

    // Trim: held, the line from here on is up by as much, ahead too; let go, it comes back down.
    ws.update(cx, |ws, _| {
        ws.session.project.tracks[0].automation.mode = AutoMode::Trim
    });
    let grabbed = ws.read_with(cx, |ws, _| ws.playhead());
    cx.simulate_mouse_down(fader, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        fader + point(px(20.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    let at = |cx: &VisualTestContext, tick: u64| {
        ws.read_with(cx, |ws, _| {
            value_at(&ws.session.project.tracks[0].automation.volume, tick as f64).unwrap()
        })
    };
    assert!(at(cx, BAR * 3) > -15.0, "{}", at(cx, BAR * 3));
    assert!((at(cx, grabbed.saturating_sub(10)) + 20.0).abs() < 0.5);
    until(cx, &ws, |ws| ws.playhead() > grabbed + PPQ / 4);
    cx.simulate_mouse_up(
        fader + point(px(20.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    let released = ws.read_with(cx, |ws, _| ws.playhead());
    assert!(at(cx, (grabbed + released) / 2) > -15.0);
    assert_eq!(at(cx, BAR * 3), -20.0);

    // Touch: what's moved is recorded while held; let go, back to the line in a moment.
    ws.update(cx, |ws, _| {
        ws.session.project.tracks[0].automation.mode = AutoMode::Touch
    });
    let grabbed = ws.read_with(cx, |ws, _| ws.playhead());
    cx.simulate_mouse_down(fader, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        fader + point(px(25.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    until(cx, &ws, |ws| ws.playhead() > grabbed + PPQ / 4);
    cx.simulate_mouse_up(
        fader + point(px(25.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    let released = ws.read_with(cx, |ws, _| ws.playhead());
    assert!(at(cx, (grabbed + released) / 2) > -15.0);
    assert_eq!(at(cx, released + PPQ), -20.0);
    ws.update(cx, |ws, cx| ws.stop(cx));
}

#[gpui::test]
fn write_records_every_automated_control_from_play_then_turns_to_latch(cx: &mut TestAppContext) {
    use pina_engine::song::project::{AutoMode, AutoPoint, value_at};
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        let a = &mut ws.session.project.tracks[0].automation;
        a.volume = vec![AutoPoint::new(0, -30.0), AutoPoint::new(BAR * 4, 0.0)];
        a.mode = AutoMode::Write;
        ws.changed(cx);
        ws.player.play();
    });
    until(cx, &ws, |ws| ws.playhead() > PPQ * 2);
    ws.update(cx, |ws, cx| ws.stop(cx));
    until(cx, &ws, |ws| ws.writing.is_empty() && !ws.was_playing);
    let a = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].automation.clone());
    assert_eq!(a.mode, AutoMode::Latch);
    // Held where it started for the pass (the ramp gone), the ramp after; thinned to a few.
    let held = value_at(&a.volume, PPQ as f64).unwrap();
    assert!((held + 30.0).abs() < 0.5, "{held}");
    let later = value_at(&a.volume, (PPQ * 3 / 2) as f64).unwrap();
    assert!((later - held).abs() < 0.05, "{held} {later}");
    assert_eq!(value_at(&a.volume, (BAR * 4) as f64), Some(0.0));
    assert!(a.volume.len() <= 5, "{:?}", a.volume);
}

#[gpui::test]
fn automation_lanes_draw_select_copy_bend_and_take_shapes(cx: &mut TestAppContext) {
    use pina_engine::song::project::{Param, value_at};
    let (ws, cx) = open(cx);
    cx.simulate_keystrokes("a");
    let points = |cx: &VisualTestContext, param: Param| {
        ws.read_with(cx, |ws, _| {
            ws.session.project.tracks[0]
                .automation
                .points(param)
                .to_vec()
        })
    };
    // A lane more: pan, under the volume.
    click_on(cx, "auto-add-0", Modifiers::none(), MouseButton::Left);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.auto_lanes(0)),
        [Param::Volume, Param::Pan]
    );
    // A sine over bar 2, from the volume lane's menu.
    let at = lane_spot(&ws, cx, BAR + PPQ, 0, 0, -6.0);
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    pick(cx, &ws, "Sine");
    let sine = points(cx, Param::Volume);
    assert!(
        sine.iter().all(|p| p.tick >= BAR && p.tick <= BAR * 2),
        "{sine:?}"
    );
    assert!(sine.iter().any(|p| p.curve != 0.0));
    // A marquee over its first half selects those; delete takes them out.
    let (a, b) = (
        lane_spot(&ws, cx, BAR - PPQ / 2, 0, 0, 6.0),
        lane_spot(&ws, cx, BAR + PPQ * 2 - PPQ / 8, 0, 0, -144.0),
    );
    drag(cx, a, b);
    let picked = ws.read_with(cx, |ws, _| ws.picked_points(0, Param::Volume));
    let inside = sine.iter().filter(|p| p.tick < BAR + PPQ * 2).count();
    assert_eq!(picked.len(), inside, "{picked:?}");
    cx.simulate_keystrokes("backspace");
    assert_eq!(points(cx, Param::Volume).len(), sine.len() - inside);
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(points(cx, Param::Volume), sine);
    // Copied, then pasted at bar 4 from the lane's menu: the same shape there.
    drag(cx, a, b);
    cx.simulate_keystrokes("cmd-c");
    let at = lane_spot(&ws, cx, BAR * 3, 0, 0, -6.0);
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    pick(cx, &ws, "Paste");
    let line = points(cx, Param::Volume);
    for t in [0, PPQ / 4, PPQ / 2, PPQ] {
        let (was, now) = (
            value_at(&sine, (BAR + t) as f64).unwrap(),
            value_at(&line, (BAR * 3 + t) as f64).unwrap(),
        );
        assert!((was - now).abs() < 0.05, "{t}: {was} {now}");
    }
    // The pencil draws the pan lane across a bar.
    ws.update(cx, |ws, _| ws.tool = Tool::Pencil);
    drag(
        cx,
        lane_spot(&ws, cx, 0, 0, 1, -1.0),
        lane_spot(&ws, cx, BAR, 0, 1, 1.0),
    );
    let pan = points(cx, Param::Pan);
    assert!(
        pan.len() >= 2 && pan[0].value < -0.9 && pan.last().unwrap().value > 0.9,
        "{pan:?}"
    );
    ws.update(cx, |ws, _| ws.tool = Tool::Pointer);
    // The line between its two ends dragged down: both ends with it.
    ws.update(cx, |ws, cx| {
        use pina_engine::song::project::AutoPoint;
        *ws.session.project.tracks[0]
            .automation
            .points_mut(Param::Pan) = vec![AutoPoint::new(0, -0.5), AutoPoint::new(BAR, 0.5)];
        ws.changed(cx);
    });
    let on = lane_spot(&ws, cx, BAR / 4, 0, 1, -0.25);
    drag(cx, on, on + point(px(0.), px(10.)));
    let pan = points(cx, Param::Pan);
    assert!(pan[0].value > -0.5 && pan[1].value > 0.5, "{pan:?}");
    // Its bend handle pulled down bends it: slow, then fast.
    let handle = cx.debug_bounds("auto-bend-0-1-0").unwrap().center();
    drag(cx, handle, handle + point(px(0.), px(20.)));
    let pan = points(cx, Param::Pan);
    assert!(pan[0].curve.abs() > 0.2, "{pan:?}");
    // Mode and lanes from the header; a second lane goes again.
    click_on(cx, "auto-remove-0-1", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.auto_lanes(0)), [Param::Volume]);
}

#[gpui::test]
fn automation_smooths_from_its_menus_and_takes_s_curves(cx: &mut TestAppContext) {
    use pina_engine::song::project::{AutoPoint, Param, value_at};
    let (ws, cx) = open(cx);
    cx.simulate_keystrokes("a");
    // The keys' pan, recorded with a wobble every sixteenth through bar 2.
    ws.update(cx, |ws, cx| {
        let id = ws.session.project.tracks[0].id;
        ws.auto_lanes.insert(id, vec![Param::Pan]);
        let wobble = (0..=16)
            .map(|i| AutoPoint::new(BAR + i * PPQ / 4, if i % 2 == 0 { -0.3 } else { 0.3 }));
        ws.session.project.tracks[0].automation.pan = std::iter::once(AutoPoint::new(0, -0.3))
            .chain(wobble)
            .chain([AutoPoint::new(BAR * 4, -0.3)])
            .collect();
        ws.changed(cx);
    });
    let pan = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            ws.session.project.tracks[0].automation.pan.clone()
        })
    };
    let swing = |line: &[AutoPoint]| {
        (BAR + PPQ..BAR * 2 - PPQ)
            .step_by(20)
            .map(|t| value_at(line, t as f64).unwrap())
            .fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)))
    };
    let (lo, hi) = swing(&pan(cx));
    assert!(hi - lo > 0.5);
    // Right-click the lane: Smooth Lane.
    let at = lane_spot(&ws, cx, BAR * 3, 0, 0, 0.9);
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    pick(cx, &ws, "Smooth Lane");
    let smooth = pan(cx);
    let (lo, hi) = swing(&smooth);
    assert!(hi - lo < 0.1, "{lo} {hi}");
    assert_eq!(smooth.first().unwrap().value, -0.3);
    assert_eq!(smooth.last().unwrap(), &AutoPoint::new(BAR * 4, -0.3));
    // One undo puts it back.
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(pan(cx).len(), 19);
    // A point's line as an S, from its menu.
    ws.update(cx, |ws, cx| {
        ws.session.project.tracks[0].automation.pan =
            vec![AutoPoint::new(0, -1.0), AutoPoint::new(BAR, 1.0)];
        ws.changed(cx);
    });
    click_on(
        cx,
        "auto-point-0-0-0",
        Modifiers::none(),
        MouseButton::Right,
    );
    pick(cx, &ws, "S-Curve");
    let s = pan(cx);
    assert!(s[0].ease);
    assert!(value_at(&s, (BAR / 8) as f64).unwrap() < -0.85);
    assert_eq!(value_at(&s, (BAR / 2) as f64), Some(0.0));
}

#[gpui::test]
fn automation_moves_with_regions_when_set(cx: &mut TestAppContext) {
    use pina_engine::song::project::{AutoPoint, value_at};
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.settings.carry_automation = true;
        let a = &mut ws.session.project.tracks[1].automation;
        a.volume = vec![AutoPoint::new(0, -20.0), AutoPoint::new(BAR, 0.0)];
        ws.changed(cx);
    });
    let lead = ws.read_with(cx, |ws, _| ws.session.project.tracks[1].regions[0].clone());
    drag(cx, spot(&ws, cx, PPQ, 1), spot(&ws, cx, PPQ + BAR, 1));
    let moved = ws.read_with(cx, |ws, _| ws.session.project.tracks[1].regions[0].start);
    assert_eq!(moved, lead.start + BAR);
    let line = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[1].automation.volume.clone()
    });
    assert_eq!(value_at(&line, BAR as f64), Some(-20.0));
    assert!((value_at(&line, (BAR + BAR / 2) as f64).unwrap() + 10.0).abs() < 0.1);
}
