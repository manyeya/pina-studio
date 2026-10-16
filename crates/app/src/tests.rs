//! The window, driven like a person would: mouse drags and shortcuts on the demo project, in a
//! headless window with a silent engine.

use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use gpui::{
    Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point, TestAppContext,
    VisualTestContext, point, px,
};
use tempo_engine::{
    player::Player,
    project::{AuxSend, Content, PPQ},
    scan::PluginInfo,
    session::Session,
};

use crate::{pianoroll::KEY_H, theme::LANE_H, tracks::auto_to_y, workspace::*};

const BAR: u64 = PPQ * 4;

fn demo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/demo")
}

fn open(cx: &mut TestAppContext) -> (Entity<Workspace>, &mut VisualTestContext) {
    open_at(cx, &demo())
}

fn open_at<'a>(
    cx: &'a mut TestAppContext,
    dir: &Path,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    cx.update(bind_keys);
    let session = Session::open(dir).unwrap();
    let player = Player::silent(&session.project, &session.dir);
    let (ws, cx) = cx.add_window_view(|window, cx| Workspace::new(session, player, window, cx));
    cx.run_until_parked();
    (ws, cx)
}

/// The window position of a tick on a track, in the lanes.
fn spot(ws: &Entity<Workspace>, cx: &VisualTestContext, tick: u64, track: usize) -> Point<Pixels> {
    ws.read_with(cx, |ws, _| {
        let b = ws.lanes_bounds.get();
        let x = tick as f32 * ws.px_per_tick() - ws.scroll_x;
        let y = track as f32 * LANE_H + LANE_H / 2.0 - ws.scroll_y;
        point(b.origin.x + px(x), b.origin.y + px(y))
    })
}

fn drag(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

/// The selected region's track, start and length.
fn selected(ws: &Entity<Workspace>, cx: &VisualTestContext) -> (usize, u64, u64) {
    ws.read_with(cx, |ws, _| {
        let at = ws.region.expect("a region is selected");
        let r = ws.session.project.region(at);
        (at.0, r.start, r.length)
    })
}

/// Waits for the engine thread to catch up with something, drawing frames meanwhile.
fn until(cx: &mut VisualTestContext, ws: &Entity<Workspace>, f: impl Fn(&Workspace) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !ws.read_with(cx, |ws, _| f(ws)) {
        assert!(Instant::now() < deadline, "timed out");
        thread::sleep(Duration::from_millis(5));
        ws.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
    }
}

fn double_click(cx: &mut VisualTestContext, at: Point<Pixels>) {
    let modifiers = Modifiers::none();
    let button = MouseButton::Left;
    cx.simulate_event(MouseDownEvent {
        position: at,
        modifiers,
        button,
        click_count: 2,
        first_mouse: false,
    });
    cx.run_until_parked();
    cx.simulate_event(MouseUpEvent {
        position: at,
        modifiers,
        button,
        click_count: 2,
    });
    cx.run_until_parked();
}

/// The window position of a tick and pitch in the piano roll.
fn roll_spot(
    ws: &Entity<Workspace>,
    cx: &VisualTestContext,
    tick: u64,
    pitch: u8,
) -> Point<Pixels> {
    ws.read_with(cx, |ws, _| {
        let x =
            f32::from(ws.roll_bounds.get().origin.x) + tick as f32 * ws.px_per_tick() - ws.scroll_x;
        point(px(x), px(ws.pitch_y(pitch) + KEY_H / 2.0))
    })
}

/// The Keys region's notes, as (start, pitch).
fn keys_notes(ws: &Entity<Workspace>, cx: &VisualTestContext, region: usize) -> Vec<(i64, u8)> {
    ws.read_with(cx, |ws, _| {
        match &ws.session.project.tracks[0].regions[region].content {
            Content::Midi { notes } => notes.iter().map(|n| (n.start, n.pitch)).collect(),
            _ => unreachable!(),
        }
    })
}

#[gpui::test]
fn the_window_lays_out_the_lanes(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let b = ws.read_with(cx, |ws, _| ws.lanes_bounds.get());
    assert!(b.size.width > px(100.) && b.size.height > px(100.), "{b:?}");
}

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
    // The first drums region's right edge, dragged one beat left.
    let edge = spot(&ws, cx, BAR, 2) - point(px(2.), px(0.));
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
fn shortcuts_add_tracks_undo_and_play(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let tracks = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.session.project.tracks.len());
    cx.simulate_keystrokes("cmd-alt-n");
    assert_eq!(tracks(cx), 4);
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(tracks(cx), 3);
    cx.simulate_keystrokes("space");
    until(cx, &ws, |ws| ws.player.playing());
    cx.simulate_keystrokes("space");
    until(cx, &ws, |ws| !ws.player.playing());
}

#[gpui::test]
fn the_piano_roll_adds_moves_and_deletes_notes(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_click(spot(&ws, cx, PPQ, 0), Modifiers::none()); // the Keys region
    cx.simulate_keystrokes("p");
    cx.run_until_parked();
    let before = keys_notes(&ws, cx, 0).len();

    // Double-click empty grid: a beat-long note there.
    double_click(cx, roll_spot(&ws, cx, PPQ * 2 + PPQ / 2, 70));
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
        px(lane_top + auto_to_y(tempo_engine::project::Param::Volume, points[0].value)),
    );
    drag(cx, at, at + point(px(0.), px(30.)));
    let moved = volume(cx)[0];
    assert!(moved.value < points[0].value - 6.0, "{moved:?}");
    assert_eq!(moved.tick, PPQ);

    // Option-click deletes it.
    let at = point(
        top.x,
        px(lane_top + auto_to_y(tempo_engine::project::Param::Volume, moved.value)),
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

fn click_on(
    cx: &mut VisualTestContext,
    name: &'static str,
    modifiers: Modifiers,
    button: MouseButton,
) {
    let b = cx
        .debug_bounds(name)
        .unwrap_or_else(|| panic!("no {name} on screen"));
    let at = b.center();
    cx.simulate_mouse_down(at, button, modifiers);
    cx.simulate_mouse_up(at, button, modifiers);
    cx.run_until_parked();
}

#[gpui::test]
fn a_send_to_a_new_aux_and_its_level(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    click_on(cx, "add-send-0", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| ws.menu.is_some()));
    click_on(cx, "menu-0", Modifiers::none(), MouseButton::Left); // the only bus: New Aux
    let (auxes, sends) = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        (p.auxes.len(), p.tracks[0].sends.clone())
    });
    assert_eq!(auxes, 1);
    assert_eq!(
        sends,
        [AuxSend {
            bus: 1,
            level_db: 0.0,
            ..Default::default()
        }]
    );

    // Dragging the send down 20 pixels takes 10 dB off.
    let at = cx.debug_bounds("send-0-0").unwrap().center();
    drag(cx, at, at + point(px(0.), px(20.)));
    let level = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].sends[0].level_db);
    assert_eq!(level, -10.0);
    // The mixer has a strip for the aux.
    assert!(cx.debug_bounds("strip-Aux(0)").is_none());
    cx.simulate_keystrokes("x");
    assert!(cx.debug_bounds("strip-Aux(0)").is_some());
}

#[gpui::test]
fn a_plugin_that_wont_load_is_reported_and_removable(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let missing = PluginInfo {
        path: "/nowhere/Missing.vst3".into(),
        class: "Missing".into(),
        vendor: "Nobody".into(),
        instrument: false,
    };
    ws.update(cx, |ws, _| ws.plugins = Some(vec![missing]));
    // The Keys strip's empty insert slot offers No Plug-in and the effect.
    click_on(
        cx,
        "slot-Track(0)-Insert(0)",
        Modifiers::none(),
        MouseButton::Left,
    );
    // The effect, and the built-in EQ, compressor and saturator: nothing to take out of an empty
    // slot.
    let items = ws.read_with(cx, |ws, _| ws.menu.as_ref().unwrap().items.len());
    assert_eq!(items, 4);
    click_on(cx, "menu-0", Modifiers::none(), MouseButton::Left);
    let (inserts, status) = ws.read_with(cx, |ws, _| {
        (
            ws.session.project.tracks[0].inserts.len(),
            ws.status.clone(),
        )
    });
    assert_eq!(inserts, 1);
    assert!(status.unwrap().contains("Missing.vst3"));

    // Right-click the slot and take it out again: the error goes with it.
    click_on(
        cx,
        "slot-Track(0)-Insert(0)",
        Modifiers::none(),
        MouseButton::Right,
    );
    click_on(cx, "menu-0", Modifiers::none(), MouseButton::Left);
    let (inserts, status) = ws.read_with(cx, |ws, _| {
        (
            ws.session.project.tracks[0].inserts.len(),
            ws.status.clone(),
        )
    });
    assert_eq!((inserts, status), (0, None));
}

#[gpui::test]
fn agents_edit_the_open_project_through_its_socket(cx: &mut TestAppContext) {
    // A copy of the demo, so a tempo that has the real one open isn't disturbed.
    let dir = std::env::temp_dir().join(format!("tempo-test-socket-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("Audio Files")).unwrap();
    for f in ["project.json", "Audio Files/drums.wav"] {
        std::fs::copy(demo().join(f), dir.join(f)).unwrap();
    }
    cx.executor().allow_parking();
    let (ws, cx) = open_at(cx, &dir);
    let weak = ws.downgrade();
    cx.update(|_, cx| crate::server::serve(weak, &dir, cx));

    // Ask from another thread, as `tempo do` would from another process.
    let ask = |cx: &mut VisualTestContext, command: &'static str| {
        let d = dir.clone();
        let asker = thread::spawn(move || crate::cli::edit(&d, command));
        while !asker.is_finished() {
            cx.run_until_parked();
            thread::sleep(Duration::from_millis(1));
        }
        asker.join().unwrap()
    };
    assert_eq!(
        ask(cx, r#"{"op": "set_tempo", "bpm": 90}"#),
        Ok(serde_json::json!({}))
    );
    assert_eq!(ws.read_with(cx, |ws, _| ws.session.project.tempo), 90.0);
    let err = ask(cx, r#"{"op": "delete_track", "track": 9}"#).unwrap_err();
    assert!(err.contains("no track 9"), "{err}");
    // It's an edit like any other: undo takes it back, and the file wasn't touched.
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(ws.read_with(cx, |ws, _| ws.session.project.tempo), 100.0);
    assert_eq!(
        tempo_engine::project::Project::load(&dir).unwrap().tempo,
        100.0
    );
    // The window shows where the agent was working.
    let (what, track) = ws.read_with(cx, |ws, _| {
        let a = ws.agent.last().unwrap();
        (a.what.clone(), a.track)
    });
    assert_eq!((what.as_str(), track), ("set tempo", None));
    ask(cx, r#"{"op": "set_track", "track": 1, "mute": true}"#).unwrap();
    let a = ws
        .read_with(cx, |ws, _| ws.agent_presence().cloned())
        .unwrap();
    assert_eq!((a.what.as_str(), a.track), ("set track · Lead", Some(1)));
    std::fs::remove_dir_all(&dir).unwrap();
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
fn a_drum_track_and_a_step_pattern(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    click_on(cx, "new-track", Modifiers::none(), MouseButton::Left);
    click_on(cx, "menu-1", Modifiers::none(), MouseButton::Left); // Drum Kit
    let (track, instrument) = ws.read_with(cx, |ws, _| {
        (
            ws.track,
            ws.session.project.tracks[ws.track].instrument.clone(),
        )
    });
    assert_eq!(instrument, Some(tempo_engine::project::Instrument::Drums));

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
    click_on(cx, "add-sound", Modifiers::none(), MouseButton::Left);
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
}

#[gpui::test]
fn option_dragging_an_audio_regions_end_flexes_it(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    // The first drums region's right edge, option-dragged a bar longer.
    let edge = spot(&ws, cx, BAR, 2) - point(px(2.), px(0.));
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
fn the_score_shows_a_regions_notes_to_select_and_delete(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_click(spot(&ws, cx, PPQ, 1), Modifiers::none()); // the Lead region
    cx.simulate_keystrokes("n");
    assert_eq!(ws.read_with(cx, |ws, _| ws.bottom), Some(Bottom::Score));
    let before = ws.read_with(cx, |ws, _| {
        ws.session
            .project
            .region((1, 0))
            .content
            .notes(0)
            .unwrap()
            .len()
    });
    let names: Vec<&'static str> = (0..before)
        .map(|i| &*Box::leak(format!("score-note-{i}").into_boxed_str()))
        .collect();
    let drawn = names
        .iter()
        .filter(|n| cx.debug_bounds(n).is_some())
        .count();
    assert!(drawn > 4, "{drawn} of {before} notes drawn");
    click_on(cx, "score-note-2", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.note), Some(2));
    cx.simulate_keystrokes("backspace");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws
            .session
            .project
            .region((1, 0))
            .content
            .notes(0)
            .unwrap()
            .len()),
        before - 1
    );
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
    // After the Lead region, in bar 6.
    cx.simulate_click(spot(&ws, cx, BAR * 5 + PPQ, 1), Modifiers::none());
    assert_eq!(regions(cx, 1).last(), Some(&(BAR * 5, BAR)));

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
fn the_mixer_shapes_a_channel_with_its_eq_and_compressor(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_keystrokes("x");
    assert_eq!(ws.read_with(cx, |ws, _| ws.view), View::Mixer);
    // The Keys strip's EQ: clicking the thumbnail adds one, shown in the panel.
    click_on(cx, "eq-Track(0)", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| ws.session.project.tracks[0].eq.is_some()));
    // Drag the second band's handle up: a boost.
    let handle = cx.debug_bounds("eq-band-2").unwrap().center();
    drag(cx, handle, handle - point(px(0.), px(20.)));
    let eq = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].eq.unwrap());
    assert!(eq.high_mid.gain_db > 3.0, "{eq:?}");
    // A compressor from the add-effect menu, then its threshold turned down.
    click_on(cx, "add-effect", Modifiers::none(), MouseButton::Left);
    let items = ws.read_with(cx, |ws, _| ws.menu.as_ref().unwrap().items.clone());
    let comp = items
        .iter()
        .position(|(label, _)| label.starts_with("Compressor"))
        .unwrap();
    let name: &'static str = Box::leak(format!("menu-{comp}").into_boxed_str());
    click_on(cx, name, Modifiers::none(), MouseButton::Left);
    let knob = cx.debug_bounds("comp-Threshold").unwrap().center();
    drag(cx, knob, knob + point(px(0.), px(30.)));
    let threshold = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[0].comp.unwrap().threshold_db
    });
    assert!(threshold < -18.0, "{threshold}");
    // Its detector: peaks, or an RMS average.
    click_on(
        cx,
        "comp-detector-RMS",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert!(ws.read_with(cx, |ws, _| ws.session.project.tracks[0].comp.unwrap().rms));
    // All of it undoes as edits.
    cx.simulate_keystrokes("cmd-z cmd-z cmd-z cmd-z cmd-z");
    let (eq, comp) = ws.read_with(cx, |ws, _| {
        (
            ws.session.project.tracks[0].eq,
            ws.session.project.tracks[0].comp,
        )
    });
    assert_eq!((eq, comp), (None, None));
}

#[gpui::test]
fn smart_controls_turn_the_channel_eq_on_a_built_in_instrument(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    // Keys plays the built-in synth, so its Smart Controls are its channel's: Low Cut first.
    let knob = cx.debug_bounds("smart-0").unwrap().center();
    drag(cx, knob, knob - point(px(0.), px(60.)));
    let cut = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].eq.unwrap().low_cut);
    assert!(cut > 40.0, "{cut}");
}

#[gpui::test]
fn velocities_drag_and_the_roll_tools_edit_notes(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
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
    // Legato holds each note to the next.
    click_on(cx, "roll-Legato", Modifiers::none(), MouseButton::Left);
    let n = notes(cx);
    assert_eq!(n[0].start + n[0].length as i64, n[1].start);
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
fn the_key_is_worked_out_or_chosen(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let key = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| ws.session.project.song_key().map(|k| k.name()))
    };
    assert_eq!(key(cx).as_deref(), Some("C major")); // the demo's chords
    click_on(cx, "key", Modifiers::none(), MouseButton::Left);
    click_on(cx, "menu-10", Modifiers::none(), MouseButton::Left); // after the notes, C major on
    assert_eq!(key(cx).as_deref(), Some("A major"));
    click_on(cx, "key", Modifiers::none(), MouseButton::Left);
    click_on(cx, "menu-0", Modifiers::none(), MouseButton::Left); // back to the notes
    assert_eq!(key(cx).as_deref(), Some("C major"));
}

fn drag_with(
    cx: &mut VisualTestContext,
    from: Point<Pixels>,
    to: Point<Pixels>,
    modifiers: Modifiers,
) {
    cx.simulate_mouse_down(from, MouseButton::Left, modifiers);
    cx.run_until_parked();
    cx.simulate_mouse_move(to, MouseButton::Left, modifiers);
    cx.simulate_mouse_up(to, MouseButton::Left, modifiers);
    cx.run_until_parked();
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
            crate::menu::MenuAction::Send {
                track: 0,
                bus: None,
            },
            cx,
        );
        ws.show_channel(crate::strip::Channel::Aux(0), cx);
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
fn latch_records_the_fader_while_playing_until_stop(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_keystrokes("a");
    // Keys' automation mode: Off, Read, Touch, Latch.
    click_on(cx, "auto-mode-0", Modifiers::none(), MouseButton::Left);
    click_on(cx, "menu-3", Modifiers::none(), MouseButton::Left);
    let mode = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].automation.mode);
    assert_eq!(mode, tempo_engine::project::AutoMode::Latch);

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
    let shown = ws.read_with(cx, |ws, _| ws.volume(crate::strip::Channel::Track(0)));
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
    let param = ws.read_with(cx, |ws, _| ws.params.get(&id).copied());
    assert_eq!(param, Some(tempo_engine::project::Param::Pan));
    // Typing didn't reach the shortcuts ("a" hides automation).
    assert!(ws.read_with(cx, |ws, _| ws.show_automation && ws.menu.is_none()));
}

#[gpui::test]
fn the_saturator_goes_on_from_the_browser_and_turns_in_the_mixer(cx: &mut TestAppContext) {
    use crate::{
        menu::MenuAction,
        strip::{Builtin, Channel},
    };
    let (ws, cx) = open(cx);
    let add = MenuAction::AddBuiltin {
        channel: Channel::Track(0),
        builtin: Builtin::Sat,
    };
    ws.update(cx, |ws, cx| ws.choose(add.clone(), cx));
    let sat = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.session.project.tracks[0].sat);
    assert_eq!(sat(cx), Some(tempo_engine::project::Saturator::default()));
    // Asking again shows it, in the mixer, without another edit.
    let edits = ws.read_with(cx, |ws, _| ws.session.project.clone());
    ws.update(cx, |ws, cx| ws.choose(add, cx));
    assert_eq!(ws.read_with(cx, |ws, _| ws.session.project.clone()), edits);
    assert_eq!(ws.read_with(cx, |ws, _| ws.view), View::Mixer);
    cx.run_until_parked();
    // Drive up.
    let knob = cx.debug_bounds("sat-Drive").unwrap().center();
    drag(cx, knob, knob - point(px(0.), px(40.)));
    assert!(sat(cx).unwrap().drive_db > 6.0);
    // Bypassed from its header.
    click_on(cx, "bypass-Sat", Modifiers::none(), MouseButton::Left);
    assert!(sat(cx).unwrap().bypass);
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
fn sends_switch_between_pre_and_post_fader(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.choose(
            crate::menu::MenuAction::Send {
                track: 0,
                bus: None,
            },
            cx,
        );
        ws.show_channel(crate::strip::Channel::Track(0), cx);
    });
    cx.run_until_parked();
    click_on(cx, "send-pre-0", Modifiers::none(), MouseButton::Left);
    let pre = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| ws.session.project.tracks[0].sends[0].pre)
    };
    assert!(pre(cx));
    click_on(cx, "send-pre-0", Modifiers::none(), MouseButton::Left);
    assert!(!pre(cx));
}

#[gpui::test]
fn the_master_takes_plugins_and_audio_tracks_pick_an_input(cx: &mut TestAppContext) {
    use crate::{menu::MenuAction, strip::Channel};
    let (ws, cx) = open(cx);
    let plugin = tempo_engine::project::PluginRef {
        path: "/nonexistent/Missing.vst3".into(),
        class: "Missing".into(),
        ..Default::default()
    };
    let insert = MenuAction::Insert {
        channel: Channel::Master,
        index: 0,
        plugin: Some(plugin),
    };
    ws.update(cx, |ws, cx| ws.choose(insert, cx));
    let (inserts, status) = ws.read_with(cx, |ws, _| {
        (ws.session.project.master_inserts.len(), ws.status.clone())
    });
    assert_eq!(inserts, 1);
    assert!(status.unwrap().contains("Missing.vst3"));

    // Drums is an audio track: its strip's input opens a menu of inputs, and one can be picked.
    ws.update(cx, |ws, cx| ws.show_channel(Channel::Track(2), cx));
    cx.run_until_parked();
    click_on(cx, "input-2", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| ws.menu.is_some()));
    let input = tempo_engine::project::Input {
        channel: 1,
        stereo: false,
    };
    ws.update(cx, |ws, cx| {
        ws.choose(
            MenuAction::Input {
                track: 2,
                input: Some(input),
            },
            cx,
        )
    });
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks[2].input),
        Some(input)
    );
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
    click_on(cx, "add-sound", Modifiers::none(), MouseButton::Left);
    let first = ws.read_with(cx, |ws, _| ws.menu.as_ref().unwrap().items[0].0.clone());
    assert!(!first.contains("Kick"), "{first}");
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
