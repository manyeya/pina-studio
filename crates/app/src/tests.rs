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

use crate::{
    pianoroll::KEY_H,
    theme::{CONTROL_BAR_H, LANE_H, MARKERS_H, RULER_H, STATUS_H},
    tracks::auto_to_y,
    workspace::*,
};

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
            Content::Midi { notes, .. } => notes.iter().map(|n| (n.start, n.pitch)).collect(),
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
        format: Default::default(),
        path: "/nowhere/Missing.vst3".into(),
        class: "Missing".into(),
        uid: None,
        vendor: "Nobody".into(),
        instrument: false,
        categories: Vec::new(),
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
fn unsaved_edits_are_kept_and_offered_back(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("tempo-test-kept-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("Audio Files")).unwrap();
    for f in ["project.json", "Audio Files/drums.wav"] {
        std::fs::copy(demo().join(f), dir.join(f)).unwrap();
    }
    let (ws, cx) = open_at(cx, &dir);
    let saved = ws.read_with(cx, |ws, _| ws.session.project.tempo);
    let kept = dir.join(".recovery/project.json");
    // Edits, kept; then tempo stops, and the project opens again and asks.
    let edit_and_reopen = |cx: &mut VisualTestContext| {
        ws.update(cx, |ws, cx| {
            ws.keep_edits(); // nothing to keep yet
            assert!(!kept.exists());
            ws.edit(cx, |ws| ws.session.project.tempo = saved + 7.0);
            ws.keep_edits();
            assert!(kept.exists());
            ws.session = Session::open(&dir).unwrap();
            assert_eq!(ws.session.project.tempo, saved);
        });
        cx.update(|window, cx| ws.update(cx, |ws, cx| ws.offer_kept(window, cx)));
        cx.run_until_parked();
        assert!(cx.has_pending_prompt());
    };

    edit_and_reopen(cx);
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    assert!(!dir.join(".recovery").exists());
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.session.project.tempo, saved);
        assert!(!ws.session.edited);
    });

    edit_and_reopen(cx);
    cx.simulate_prompt_answer("Restore");
    cx.run_until_parked();
    ws.update(cx, |ws, cx| {
        assert_eq!(ws.session.project.tempo, saved + 7.0);
        assert!(ws.session.edited);
        assert!(ws.save(cx));
    });
    assert!(!dir.join(".recovery").exists());
    assert_eq!(Session::open(&dir).unwrap().project.tempo, saved + 7.0);
    std::fs::remove_dir_all(dir).unwrap();
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

#[gpui::test]
fn the_editor_is_resized_by_its_top_edge(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_click(spot(&ws, cx, PPQ, 0), Modifiers::none());
    cx.simulate_keystrokes("p");
    let height = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.editor_height());
    assert_eq!(height(cx), crate::theme::EDITOR_H);
    // Up 80: taller by 80, the lanes shorter.
    let lanes = ws.read_with(cx, |ws, _| ws.lanes_bounds.get().size.height);
    let edge = cx.debug_bounds("editor-edge").unwrap().center();
    drag(cx, edge, edge - point(px(0.), px(80.)));
    assert_eq!(height(cx), crate::theme::EDITOR_H + 80.0);
    let shorter = ws.read_with(cx, |ws, _| ws.lanes_bounds.get().size.height);
    assert_eq!(shorter, lanes - px(80.));
    // Not past what leaves the tracks room, nor shorter than its tools.
    let edge = cx.debug_bounds("editor-edge").unwrap().center();
    drag(cx, edge, edge - point(px(0.), px(5000.)));
    let tallest = ws.read_with(cx, |ws, _| {
        ws.viewport_h - CONTROL_BAR_H - STATUS_H - MARKERS_H - RULER_H - LANE_H * 2.0
    });
    assert_eq!(height(cx), tallest);
    let edge = cx.debug_bounds("editor-edge").unwrap().center();
    drag(cx, edge, edge + point(px(0.), px(5000.)));
    assert_eq!(height(cx), crate::theme::EDITOR_MIN_H);
    // Double-click: back to its usual height. None of it is an edit.
    let edge = cx.debug_bounds("editor-edge").unwrap().center();
    double_click(cx, edge);
    assert_eq!(height(cx), crate::theme::EDITOR_H);
    assert!(!ws.read_with(cx, |ws, _| ws.session.edited));
}

#[gpui::test]
fn the_compact_browser_is_resized_by_its_left_edge(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_keystrokes("f");
    cx.simulate_keystrokes("escape");
    let width = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.panel_width());
    assert_eq!(width(cx), crate::browser::PANEL_W);
    // Left 100: wider by 100, the lanes narrower.
    let lanes = ws.read_with(cx, |ws, _| ws.lanes_bounds.get().size.width);
    let edge = cx.debug_bounds("panel-edge").unwrap().center();
    drag(cx, edge, edge - point(px(100.), px(0.)));
    assert_eq!(width(cx), crate::browser::PANEL_W + 100.0);
    let narrower = ws.read_with(cx, |ws, _| ws.lanes_bounds.get().size.width);
    assert_eq!(narrower, lanes - px(100.));
    // Not past what leaves the lanes room, nor narrower than its controls.
    let edge = cx.debug_bounds("panel-edge").unwrap().center();
    drag(cx, edge, edge - point(px(5000.), px(0.)));
    let widest = ws.read_with(cx, |ws, _| {
        ws.viewport_w - crate::theme::INSPECTOR_W - crate::theme::HEADER_W - 240.0
    });
    assert_eq!(width(cx), widest);
    let edge = cx.debug_bounds("panel-edge").unwrap().center();
    drag(cx, edge, edge + point(px(5000.), px(0.)));
    assert_eq!(width(cx), 260.0);
    // Double-click: back to its usual width. None of it is an edit.
    let edge = cx.debug_bounds("panel-edge").unwrap().center();
    double_click(cx, edge);
    assert_eq!(width(cx), crate::browser::PANEL_W);
    assert!(!ws.read_with(cx, |ws, _| ws.session.edited));
}

#[gpui::test]
fn an_audio_regions_flex_and_tempo_following_are_set_in_its_inspector(cx: &mut TestAppContext) {
    use tempo_engine::project::Flex;
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

/// A copy of the demo in a folder of its own, so what's added to it lands there.
fn demo_copy(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tempo-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Audio Files")).unwrap();
    for f in ["project.json", "Audio Files/drums.wav"] {
        std::fs::copy(demo().join(f), dir.join(f)).unwrap();
    }
    dir
}

/// A mono 44.1 kHz WAV of a decaying tone.
fn tone(path: &Path, seconds: f32, hz: f32) {
    let n = (44_100.0 * seconds) as u32;
    let mut bytes = Vec::new();
    bytes.extend(b"RIFF");
    bytes.extend((36 + n * 2).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(44_100u32.to_le_bytes());
    bytes.extend((44_100u32 * 2).to_le_bytes());
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend((n * 2).to_le_bytes());
    for i in 0..n {
        let t = i as f32 / 44_100.0;
        let v = (t * hz * std::f32::consts::TAU).sin() * (-t * 3.0).exp() * 12_000.0;
        bytes.extend((v as i16).to_le_bytes());
    }
    std::fs::write(path, bytes).unwrap();
}

/// A little library: its samples' files, and what they are (set here rather than analysed, so
/// these tests are about the browser).
fn little_library(dir: &Path) -> tempo_engine::samples::library::Library {
    use tempo_engine::samples::{
        Analysis, Category as C, KeyGuess, Kind, Pitch, Sample, library::Library,
    };
    std::fs::create_dir_all(dir).unwrap();
    let sample = |name: &str, category, kind, seconds: f32, traits: &[&str]| {
        let path = dir.join(format!("{name}.wav"));
        tone(&path, seconds, 110.0);
        Sample {
            path,
            name: name.into(),
            folder: "Pack".into(),
            modified: 0,
            size: 0,
            analysis: Analysis {
                seconds,
                sample_rate: 44_100,
                channels: 1,
                kind,
                category,
                confidence: 0.9,
                traits: traits.iter().map(|t| t.to_string()).collect(),
                waveform: (0..96).map(|i| 255 - i as u8 * 2).collect(),
                loudness: -12.0,
                ..Default::default()
            },
        }
    };
    let mut samples = vec![
        sample("Kick_01", C::Kick, Kind::OneShot, 0.3, &["punchy", "dark"]),
        sample("Kick_02", C::Kick, Kind::OneShot, 0.3, &["soft"]),
        sample("Snare_01", C::Snare, Kind::OneShot, 0.25, &["snappy"]),
        sample("Hat_01", C::HatClosed, Kind::OneShot, 0.1, &["bright"]),
        sample("808_G", C::Bass808, Kind::OneShot, 1.0, &["sub"]),
        // Four beats at 90 BPM.
        sample(
            "Keys_Loop_90",
            C::Keys,
            Kind::Loop,
            4.0 * 60.0 / 90.0,
            &["warm"],
        ),
    ];
    samples[4].analysis.tonal = 0.9;
    samples[4].analysis.pitch = Some(Pitch {
        note: 31,
        cents: 12.0,
        hz: 49.4,
        confidence: 0.9,
    });
    let keys = &mut samples[5].analysis;
    keys.bpm = Some(90.0);
    keys.bars = Some(1.0);
    keys.key = Some(KeyGuess {
        tonic: 0,
        minor: true,
        name: "C minor".into(),
        confidence: 0.8,
    });
    Library {
        folders: vec![dir.to_path_buf()],
        samples,
    }
}

/// The demo (a copy), with the browser's page open on a little library.
fn open_browser<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
) -> (Entity<Workspace>, &'a mut VisualTestContext, PathBuf) {
    let dir = demo_copy(name);
    let library = little_library(&dir.join("Library"));
    let (ws, cx) = open_at(cx, &dir);
    ws.update(cx, |ws, cx| ws.set_library(library, cx));
    cx.simulate_keystrokes("shift-f");
    assert_eq!(ws.read_with(cx, |ws, _| ws.view), View::Samples);
    (ws, cx, dir)
}

/// The names of what the browser found, in order.
fn found(ws: &Entity<Workspace>, cx: &VisualTestContext) -> Vec<String> {
    ws.read_with(cx, |ws, _| {
        let b = &ws.browser;
        b.hits
            .iter()
            .map(|h| b.library.samples[h.index].name.clone())
            .collect()
    })
}

#[gpui::test]
fn the_sample_browser_searches_filters_and_hears(cx: &mut TestAppContext) {
    let (ws, cx, dir) = open_browser(cx, "browser-find");
    assert_eq!(found(&ws, cx).len(), 6);
    // The search field has the keyboard: typing finds as it goes, and doesn't reach the
    // shortcuts ("k" would turn the metronome on).
    cx.simulate_input("punchy kick");
    assert_eq!(found(&ws, cx), ["Kick_01", "Kick_02"]);
    assert!(!ws.read_with(cx, |ws, _| ws.player.metronome()));
    // The words light their chips; clicked, a chip's words come out of the search.
    click_on(cx, "category-Kick", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.browser.text.clone()), "punchy");
    // Escape clears it and hands the keyboard back.
    cx.simulate_keystrokes("escape");
    assert_eq!(found(&ws, cx).len(), 6);

    // A category chip narrows the list, with how many it holds; again, and it's off.
    click_on(cx, "category-Snare", Modifiers::none(), MouseButton::Left);
    assert_eq!(found(&ws, cx), ["Snare_01"]);
    click_on(cx, "category-Snare", Modifiers::none(), MouseButton::Left);
    // One-shots or loops.
    click_on(cx, "kind-Some(Loop)", Modifiers::none(), MouseButton::Left);
    assert_eq!(found(&ws, cx), ["Keys_Loop_90"]);
    click_on(cx, "kind-None", Modifiers::none(), MouseButton::Left);

    // A click selects a sample and hears it; down hears the next.
    click_on(cx, "sample-row-0", Modifiers::none(), MouseButton::Left);
    let first = ws.read_with(cx, |ws, _| ws.browser.hits[0].index);
    let (selected, heard, status) = ws.read_with(cx, |ws, _| {
        let b = &ws.browser;
        (b.selected, b.audition.map(|a| a.0), ws.status.clone())
    });
    assert_eq!((selected, heard, status), (Some(first), Some(first), None));
    cx.simulate_keystrokes("down");
    let second = ws.read_with(cx, |ws, _| ws.browser.hits[1].index);
    assert_eq!(ws.read_with(cx, |ws, _| ws.browser.selected), Some(second));
    // Its details show, with what sounds like it.
    assert!(cx.debug_bounds("add-to-song").is_some());
    assert!(!ws.read_with(cx, |ws, _| ws.browser.similar.is_empty()));

    // Play in Key: the 808 (G1, 12 cents sharp) moves to the song's key's tonic, the nearest
    // way, and in tune.
    ws.update(cx, |ws, cx| {
        ws.set_sample_key(
            crate::browser::KeyFilter::Key(tempo_engine::music::Key {
                root: 0,
                minor: true,
            }),
            cx,
        );
        ws.browser.play_in_key = true;
    });
    let i = ws.read_with(cx, |ws, _| {
        ws.browser
            .library
            .samples
            .iter()
            .position(|s| s.name == "808_G")
            .unwrap()
    });
    let semis = ws.read_with(cx, |ws, _| ws.sample_transpose(i));
    assert!((semis - 4.88).abs() < 0.01, "{semis}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn a_loop_is_previewed_at_the_songs_tempo(cx: &mut TestAppContext) {
    let (ws, cx, dir) = open_browser(cx, "browser-preview");
    let i = ws.read_with(cx, |ws, _| {
        let s = &ws.browser.library.samples;
        s.iter().position(|s| s.name == "Keys_Loop_90").unwrap()
    });
    ws.update(cx, |ws, cx| ws.select_sample(i, true, cx));
    until(cx, &ws, |ws| ws.browser.audition.is_some_and(|a| a.0 == i));
    // From 90 BPM to the song's 100: nine tenths as long, as it'll play in the song.
    let (heard, seconds) = ws.read_with(cx, |ws, _| {
        let s = &ws.browser.library.samples[i];
        (ws.browser.audition.unwrap().2, s.analysis.seconds)
    });
    assert!((heard / seconds - 0.9).abs() < 0.01, "{heard} of {seconds}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn add_to_song_puts_loops_on_audio_tracks_and_one_shots_on_samplers(cx: &mut TestAppContext) {
    use tempo_engine::project::Instrument;
    let (ws, cx, dir) = open_browser(cx, "browser-add");
    let pick = |ws: &Entity<Workspace>, cx: &mut VisualTestContext, name: &str| {
        let i = ws.read_with(cx, |ws, _| {
            let s = &ws.browser.library.samples;
            s.iter().position(|s| s.name == name).unwrap()
        });
        ws.update(cx, |ws, cx| ws.select_sample(i, false, cx));
        cx.run_until_parked();
    };
    // The loop, with Drums (an audio track) selected: a region on it at the playhead, stretched
    // from 90 BPM to the song's 100 so its four beats last a bar.
    ws.update(cx, |ws, _| ws.select_track(2));
    pick(&ws, cx, "Keys_Loop_90");
    let before = ws.read_with(cx, |ws, _| ws.session.project.tracks[2].regions.len());
    click_on(cx, "add-to-song", Modifiers::none(), MouseButton::Left);
    let (regions, region) = ws.read_with(cx, |ws, _| {
        let t = &ws.session.project.tracks[2];
        (t.regions.len(), t.regions.last().unwrap().clone())
    });
    assert_eq!(regions, before + 1);
    assert_eq!((region.start, region.length, region.looped), (0, BAR, true));
    match region.content {
        Content::Audio {
            file,
            stretch,
            follow,
            ..
        } => {
            assert!(dir.join(&file).exists(), "{file:?}");
            assert!((stretch - 0.9).abs() < 0.01, "{stretch}");
            // A loop follows the song's tempo from here on.
            assert!(follow);
        }
        c => panic!("{c:?}"),
    }

    // The 808, with Keys (a synth) selected: a new sampler track after it, played across the
    // keys from G1, in tune.
    ws.update(cx, |ws, _| ws.select_track(0));
    pick(&ws, cx, "808_G");
    click_on(cx, "add-to-song", Modifiers::none(), MouseButton::Left);
    let (track, instrument) = ws.read_with(cx, |ws, _| {
        let t = &ws.session.project.tracks[1];
        (ws.track, t.instrument.clone())
    });
    assert_eq!(track, 1);
    let Some(Instrument::Sampler(s)) = instrument else {
        panic!("{instrument:?}")
    };
    let z = &s.zones[0];
    assert_eq!(
        (z.note, z.high, z.root, z.tune_cents),
        (0, Some(127), Some(31), -12.0)
    );
    // Then a kick onto the same sampler (still selected): its own note, the 808 making room.
    pick(&ws, cx, "Kick_01");
    click_on(cx, "add-to-song", Modifiers::none(), MouseButton::Left);
    let zones = ws.read_with(cx, |ws, _| match &ws.session.project.tracks[1].instrument {
        Some(Instrument::Sampler(s)) => s.zones.clone(),
        _ => unreachable!(),
    });
    assert_eq!(zones.len(), 2);
    assert_eq!(
        (zones[0].note, zones[0].high, zones[1].note),
        (0, Some(35), 36)
    );
    assert_eq!(ws.read_with(cx, |ws, _| ws.session.project.tracks.len()), 4);
    // Undo takes the kick back.
    ws.update(cx, |ws, cx| {
        ws.session.undo();
        ws.changed(cx);
    });
    let zones = ws.read_with(cx, |ws, _| match &ws.session.project.tracks[1].instrument {
        Some(Instrument::Sampler(s)) => s.zones.len(),
        _ => unreachable!(),
    });
    assert_eq!(zones, 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn a_kit_becomes_a_sampler_track_and_samples_drag_onto_tracks(cx: &mut TestAppContext) {
    use tempo_engine::project::Instrument;
    let (ws, cx, dir) = open_browser(cx, "browser-kit");
    cx.simulate_keystrokes("escape");
    click_on(cx, "results-Kit", Modifiers::none(), MouseButton::Left);
    let slots = ws.read_with(cx, |ws, _| {
        ws.browser.kit.iter().filter(|k| k.hit().is_some()).count()
    });
    assert!(slots >= 4, "{slots}");
    // Lock the kick: new picks keep it.
    click_on(cx, "kit-lock-0", Modifiers::none(), MouseButton::Left);
    let kick = ws.read_with(cx, |ws, _| ws.browser.kit[0].hit().unwrap().index);
    click_on(cx, "shuffle-kit", Modifiers::none(), MouseButton::Left);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.browser.kit[0].hit().unwrap().index),
        kick
    );
    let tracks = ws.read_with(cx, |ws, _| ws.session.project.tracks.len());
    click_on(cx, "make-kit", Modifiers::none(), MouseButton::Left);
    let (n, instrument) = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        (p.tracks.len(), p.tracks[ws.track].instrument.clone())
    });
    assert_eq!(n, tracks + 1);
    let Some(Instrument::Sampler(s)) = instrument else {
        panic!("{instrument:?}")
    };
    assert_eq!(s.zones.len(), slots);
    let note = |name: &str| s.zones.iter().find(|z| z.name == name).map(|z| z.note);
    assert_eq!(
        (note("Kick"), note("Snare"), note("Closed Hat")),
        (Some(36), Some(38), Some(42))
    );
    // The 808 across the notes under the drums, in tune.
    let bass = s.zones.iter().find(|z| z.name == "808").unwrap();
    assert_eq!((bass.note, bass.high, bass.root), (0, Some(35), Some(31)));

    // Drag the snare out of the page's list: the tracks come back, the compact browser beside
    // them, to drop it on Drums (an audio track) at bar 2.
    click_on(cx, "results-List", Modifiers::none(), MouseButton::Left);
    click_on(cx, "category-Snare", Modifiers::none(), MouseButton::Left);
    let row = cx.debug_bounds("sample-row-0").unwrap().center();
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    let regions = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| ws.session.project.tracks[drums].regions.len())
    };
    let before = regions(cx);
    let none = Modifiers::none();
    cx.simulate_mouse_down(row, MouseButton::Left, none);
    cx.run_until_parked();
    cx.simulate_mouse_move(row + point(px(0.), px(80.)), MouseButton::Left, none);
    cx.run_until_parked();
    let shown = ws.read_with(cx, |ws, _| (ws.view, ws.show_samples));
    assert_eq!(shown, (View::Arrange, true));
    let to = spot(&ws, cx, BAR + 30, drums);
    cx.simulate_mouse_move(to, MouseButton::Left, none);
    cx.simulate_mouse_up(to, MouseButton::Left, none);
    cx.run_until_parked();
    assert_eq!(regions(cx), before + 1);
    let start = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[drums]
            .regions
            .last()
            .unwrap()
            .start
    });
    assert_eq!(start, BAR);
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn the_compact_browser_hears_finds_alike_and_drags_onto_tracks(cx: &mut TestAppContext) {
    use tempo_engine::project::Instrument;
    let dir = demo_copy("browser-compact");
    let library = little_library(&dir.join("Library"));
    let (ws, cx) = open_at(cx, &dir);
    ws.update(cx, |ws, cx| ws.set_library(library, cx));
    // F: beside the tracks, with the search field ready to type in.
    cx.simulate_keystrokes("f");
    let shown = ws.read_with(cx, |ws, _| (ws.view, ws.show_samples));
    assert_eq!(shown, (View::Arrange, true));
    cx.simulate_input("kick");
    assert_eq!(found(&ws, cx), ["Kick_01", "Kick_02"]);
    cx.simulate_keystrokes("escape");

    // A click hears a sound, down the next.
    click_on(cx, "panel-row-0", Modifiers::none(), MouseButton::Left);
    let first = ws.read_with(cx, |ws, _| ws.browser.hits[0].index);
    let heard = ws.read_with(cx, |ws, _| {
        (ws.browser.selected, ws.browser.audition.map(|a| a.0))
    });
    assert_eq!(heard, (Some(first), Some(first)));
    cx.simulate_keystrokes("down");
    let second = ws.read_with(cx, |ws, _| ws.browser.hits[1].index);
    assert_eq!(ws.read_with(cx, |ws, _| ws.browser.selected), Some(second));
    // More like it: the list holds what sounds like it (not itself); Back, the results again.
    click_on(cx, "panel-like", Modifiers::none(), MouseButton::Left);
    let (of, like) = ws.read_with(cx, |ws, _| {
        let (of, hits) = ws.browser.like.as_ref().unwrap();
        (*of, hits.iter().map(|h| h.index).collect::<Vec<_>>())
    });
    assert_eq!(of, second);
    assert!(!like.is_empty() && !like.contains(&second), "{like:?}");
    click_on(cx, "like-back", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| ws.browser.like.is_none()));

    // A double-click adds the snare: Keys (a synth) selected, on a new sampler track after it.
    click_on(cx, "category-Snare", Modifiers::none(), MouseButton::Left);
    assert_eq!(found(&ws, cx), ["Snare_01"]);
    ws.update(cx, |ws, _| ws.select_track(0));
    let tracks = ws.read_with(cx, |ws, _| ws.session.project.tracks.len());
    let row = cx.debug_bounds("panel-row-0").unwrap().center();
    double_click(cx, row);
    let (n, instrument) = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        (p.tracks.len(), p.tracks[1].instrument.clone())
    });
    assert_eq!(n, tracks + 1);
    let Some(Instrument::Sampler(s)) = instrument else {
        panic!("{instrument:?}")
    };
    assert_eq!(s.zones.len(), 1);

    // A kick dragged from the list onto Drums (an audio track) at bar 2.
    click_on(cx, "category-Snare", Modifiers::none(), MouseButton::Left);
    click_on(cx, "category-Kick", Modifiers::none(), MouseButton::Left);
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    let regions = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| ws.session.project.tracks[drums].regions.clone())
    };
    let before = regions(cx).len();
    let row = cx.debug_bounds("panel-row-0").unwrap().center();
    drag(cx, row, spot(&ws, cx, BAR + 30, drums));
    let after = regions(cx);
    assert_eq!(after.len(), before + 1);
    assert_eq!(after.last().unwrap().start, BAR);

    // ⇧F: the page; its Compact button, beside the tracks again; F hides it.
    cx.simulate_keystrokes("shift-f");
    assert_eq!(ws.read_with(cx, |ws, _| ws.view), View::Samples);
    click_on(cx, "samples-compact", Modifiers::none(), MouseButton::Left);
    let shown = ws.read_with(cx, |ws, _| (ws.view, ws.show_samples));
    assert_eq!(shown, (View::Arrange, true));
    cx.simulate_keystrokes("escape");
    // Escape hands the keyboard back to the window, the frame after.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    cx.simulate_keystrokes("f");
    assert!(!ws.read_with(cx, |ws, _| ws.show_samples));
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn the_library_loads_and_scans_in_the_background(cx: &mut TestAppContext) {
    // The only test that reads a library from disk: one of its own.
    let dir = demo_copy("browser-scan");
    let samples = dir.join("Library");
    std::fs::create_dir_all(samples.join("Pack")).unwrap();
    tone(&samples.join("Pack/Kick_01.wav"), 0.2, 60.0);
    tone(&samples.join("Pack/Snare_01.wav"), 0.2, 200.0);
    // SAFETY: no other test reads TEMPO_LIBRARY.
    unsafe { std::env::set_var("TEMPO_LIBRARY", dir.join("config")) };
    tempo_engine::samples::library::Library {
        folders: vec![samples.canonicalize().unwrap()],
        samples: Vec::new(),
    }
    .save()
    .unwrap();
    let (ws, cx) = open_at(cx, &dir);
    ws.update(cx, |ws, cx| ws.load_library(cx));
    until(cx, &ws, |ws| {
        ws.browser.loaded && ws.browser.scan.is_none() && ws.browser.library.samples.len() == 2
    });
    cx.simulate_keystrokes("f");
    cx.simulate_keystrokes("escape");
    assert_eq!(found(&ws, cx).len(), 2);
    assert!(dir.join("config/samples.json").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn the_sound_map_boxes_in_samples_and_similar_weighs_aspects(cx: &mut TestAppContext) {
    let (ws, cx, dir) = open_browser(cx, "browser-map");
    cx.simulate_keystrokes("escape");
    click_on(cx, "results-Map", Modifiers::none(), MouseButton::Left);
    // A box round the whole map keeps everything; a click on empty space shows all again.
    let map = cx.debug_bounds("sample-map").unwrap();
    let inset = point(px(2.), px(2.));
    drag(cx, map.origin + inset, map.bottom_right() - inset);
    let boxed = ws.read_with(cx, |ws, _| ws.browser.area.as_ref().map(|a| a.len()));
    assert_eq!(boxed, Some(6));
    assert_eq!(found(&ws, cx).len(), 6);
    // A small box in a corner holds nothing: nothing found, with a way back.
    drag(cx, map.origin + inset, map.origin + point(px(12.), px(12.)));
    assert_eq!(found(&ws, cx).len(), 0);
    click_on(cx, "clear-area", Modifiers::none(), MouseButton::Left);
    assert_eq!(found(&ws, cx).len(), 6);

    // Similar: a slider sets how much timbre counts.
    click_on(cx, "results-List", Modifiers::none(), MouseButton::Left);
    click_on(cx, "sample-row-0", Modifiers::none(), MouseButton::Left);
    // The sliders are down the details column: scroll to them.
    let column = cx.debug_bounds("add-to-song").unwrap().center();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: column,
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-2000.))),
        ..Default::default()
    });
    cx.run_until_parked();
    let slider = cx.debug_bounds("aspect-0").unwrap();
    let left = point(slider.origin.x + px(1.), slider.center().y);
    drag(cx, left, left);
    let timbre = ws.read_with(cx, |ws, _| ws.browser.aspects.timbre);
    assert!(timbre < 0.1, "{timbre}");
    assert!(!ws.read_with(cx, |ws, _| ws.session.edited));
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn the_agent_panel_shows_claude_working_as_it_goes(cx: &mut TestAppContext) {
    use crate::chat::{Item, Step};
    // A stand-in for Claude Code: it takes the question and answers as Claude does, an edit and
    // a sentence, then waits to be let go.
    let dir = std::env::temp_dir().join(format!("tempo-test-claude-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let claude = dir.join("claude");
    std::fs::write(
        &claude,
        r#"#!/bin/sh
read question
cat <<'OUT'
{"type":"system","subtype":"init","model":"claude-test-1"}
{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking"}}}
{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"t1","name":"mcp__tempo__edit","input":{}}}}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"mcp__tempo__edit","input":{"project":"p","command":{"op":"set_tempo","bpm":101}}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"{}"}]}]}}
{"type":"stream_event","event":{"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"Set the tempo "}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"to 101."}}}
{"type":"result","subtype":"success","is_error":false,"duration_ms":1200}
OUT
cat > /dev/null
"#,
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    // SAFETY: no other test reads TEMPO_CLAUDE.
    unsafe { std::env::set_var("TEMPO_CLAUDE", &claude) };
    cx.executor().allow_parking();
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| ws.ask_agent("Make it 101".into(), cx));
    assert!(ws.read_with(cx, |ws, _| ws.chat.busy));
    let deadline = Instant::now() + Duration::from_secs(5);
    while ws.read_with(cx, |ws, _| ws.chat.busy) && Instant::now() < deadline {
        cx.run_until_parked();
        thread::sleep(Duration::from_millis(5));
    }
    let (items, model, busy) = ws.read_with(cx, |ws, _| {
        (ws.chat.items.clone(), ws.chat.model.clone(), ws.chat.busy)
    });
    assert!(!busy);
    assert_eq!(model.as_deref(), Some("claude-test-1"));
    assert_eq!(items.len(), 4, "{items:?}");
    assert_eq!(items[0], Item::You("Make it 101".into()));
    assert!(
        matches!(&items[1], Item::Tool { step: Step::Done, input, .. }
        if input["command"]["bpm"] == 101)
    );
    assert_eq!(items[2], Item::Words("Set the tempo to 101.".into()));
    assert!(matches!(
        items[3],
        Item::End {
            steps: 1,
            error: None,
            ..
        }
    ));
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn an_agent_that_isnt_signed_in_says_how_rather_than_starting(cx: &mut TestAppContext) {
    use crate::{
        agent::{Kind, Readiness},
        chat::Item,
    };
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.chat.ready = vec![(Kind::Codex, Readiness::SignIn)];
        ws.chat.kind = Kind::Codex;
        ws.ask_agent("Make it 101".into(), cx);
        assert!(!ws.chat.busy);
        assert!(
            matches!(ws.chat.items.last(), Some(Item::End { error: Some(e), .. })
            if e.contains("codex login"))
        );
    });
}

#[gpui::test]
fn the_agent_panel_takes_typing_spaces_and_all(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_keystrokes("cmd-l");
    cx.run_until_parked();
    assert!(ws.read_with(cx, |ws, _| ws.show_agent));
    // Clicked, the message box takes the keyboard; a space is a space, not play.
    let input = ws.read_with(cx, |ws, _| ws.chat.input.clone());
    let at = cx
        .debug_bounds("agent-input")
        .expect("the message box is drawn")
        .center();
    cx.simulate_click(at, Modifiers::none());
    cx.simulate_input("make it 101");
    cx.run_until_parked();
    assert_eq!(
        input.read_with(cx, |i, _| i.text().to_string()),
        "make it 101"
    );
    assert!(!ws.read_with(cx, |ws, _| ws.player.playing()));
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
    assert_eq!(before.look.family, tempo_engine::look::Family::Drums);
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
        tempo_engine::look::Family::Bass
    );
    cx.simulate_keystrokes("cmd-z cmd-z");
    cx.run_until_parked();
    assert_eq!(ws.read_with(cx, |ws, _| ws.shown(2)), before);
}

#[gpui::test]
fn the_browser_finds_by_words_and_puts_devices_on_tracks(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.side = Side::Browser;
        ws.plugins = Some(vec![PluginInfo {
            format: tempo_engine::project::Format::Au,
            path: "".into(),
            class: "AUDelay".into(),
            uid: Some("aufx dely appl".into()),
            vendor: "Apple".into(),
            instrument: false,
            categories: Vec::new(),
        }]);
        ws.catalog_plugins();
        ws.track = 0;
        cx.notify();
    });
    cx.run_until_parked();
    // "glue" means a compressor: typed, picked, Enter puts it on the selected track.
    let search = cx.debug_bounds("browser-search").unwrap().center();
    cx.simulate_click(search, Modifiers::none());
    cx.simulate_input("glue");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(ws.read_with(cx, |ws, _| ws.session.project.tracks[0].comp.is_some()));
    // "echo" finds the delay; dragged onto the Lead track, it goes in its inserts.
    let input = ws.read_with(cx, |ws, _| ws.catalog.search.clone());
    input.update(cx, |i, cx| i.set_text("echo", cx));
    cx.run_until_parked();
    let row = cx
        .debug_bounds("device-plugin:apple/audelay")
        .expect("the delay is found")
        .center();
    let lead = spot(&ws, cx, 960, 1);
    cx.simulate_mouse_down(row, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(lead, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(lead, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let inserts = ws.read_with(cx, |ws, _| ws.session.project.tracks[1].inserts.clone());
    assert_eq!(inserts.first().map(|p| p.class.as_str()), Some("AUDelay"));
    // Both are remembered as used lately.
    let recent = ws.read_with(cx, |ws, _| ws.catalog.recent.clone());
    assert_eq!(recent, ["plugin:apple/audelay", "own:comp"]);
}

#[gpui::test]
fn songs_start_from_a_sheet_and_open_in_this_window(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("tempo-test-window-songs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let other = crate::projects::create(
        &root,
        "Other",
        96.0,
        tempo_engine::music::Key::parse("D minor"),
        [3, 4],
        crate::projects::Template::Band,
    )
    .unwrap();
    let (ws, cx) = open(cx);
    // Open another song here: its tracks, tempo and key; this one's agent and picker gone.
    cx.update(|window, cx| ws.update(cx, |ws, cx| ws.switch_to(other.clone(), window, cx)));
    cx.run_until_parked();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.session.dir, other);
        let p = &ws.session.project;
        assert_eq!(
            (p.tempo, p.key.as_deref(), p.tracks.len()),
            (96.0, Some("D minor"), 5)
        );
        assert_eq!(ws.view, View::Arrange);
        // A band's tracks look like what they are.
        let icons: Vec<&str> = (0..5).map(|i| ws.shown(i).look.icon).collect();
        assert_eq!(icons, ["drums", "bass", "keys", "guitar", "vocals"]);
    });
    // ⌘N: the sheet. A name, a template, Create: the song is made where songs are kept (here a
    // scratch folder) and opens.
    // SAFETY: no other test reads TEMPO_SONGS.
    unsafe { std::env::set_var("TEMPO_SONGS", &root) };
    cx.simulate_keystrokes("cmd-n");
    cx.run_until_parked();
    let name = ws
        .read_with(cx, |ws, _| ws.new_song.as_ref().map(|s| s.name.clone()))
        .expect("the sheet");
    name.update(cx, |i, cx| i.set_text("Rain", cx));
    click_on(cx, "template-Lo-fi", Modifiers::none(), MouseButton::Left);
    click_on(cx, "new-song-create", Modifiers::none(), MouseButton::Left);
    ws.read_with(cx, |ws, _| {
        assert!(ws.new_song.is_none());
        assert_eq!(ws.session.dir, root.join("Rain"));
        let names: Vec<&str> = ws
            .session
            .project
            .tracks
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(names, ["Vinyl", "Drums", "Keys", "Bass"]);
    });
    std::fs::remove_dir_all(root).unwrap();
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
fn the_inspectors_strips_line_up_row_for_row(cx: &mut TestAppContext) {
    use tempo_engine::project::Eq;
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

#[gpui::test]
fn writing_notes_starts_with_a_double_click_anywhere(cx: &mut TestAppContext) {
    use tempo_engine::project::Instrument;
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
    double_click(cx, roll_spot(&ws, cx, BAR * 7 + 10, 64));
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
    double_click(cx, roll_spot(&ws, cx, 10, 60));
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
        ws.session.project.cycle = Some(tempo_engine::project::Cycle {
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

#[gpui::test]
fn the_browser_names_new_tracks_and_puts_effects_on_them(cx: &mut TestAppContext) {
    use crate::catalog::Target;
    let dir = demo_copy("browser-new-track");
    let (ws, cx) = open_at(cx, &dir);
    let device = |cx: &VisualTestContext, name: &str| {
        let name = name.to_string();
        ws.read_with(cx, move |ws, _| {
            ws.catalog
                .devices
                .iter()
                .position(|d| d.name == name)
                .unwrap()
        })
    };
    // An instrument on a new track, from the first track: after it, named for it, selected.
    ws.update(cx, |ws, _| ws.select_track(0));
    let synth = device(cx, "Sine Synth");
    ws.update(cx, |ws, cx| {
        ws.use_device(synth, Target::NewTrack, None, cx)
    });
    let (name, selected, status) = ws.read_with(cx, |ws, _| {
        (
            ws.session.project.tracks[1].name.clone(),
            ws.track,
            ws.status.clone().unwrap_or_default(),
        )
    });
    assert_eq!((name.as_str(), selected), ("Sine Synth", 1));
    assert!(status.contains("double-click"), "{status}");
    // An effect on a new track: an audio track with the effect on it.
    let eq = device(cx, "Channel EQ");
    ws.update(cx, |ws, cx| ws.use_device(eq, Target::NewTrack, None, cx));
    let t = ws.read_with(cx, |ws, _| ws.track);
    let (audio, has_eq) = ws.read_with(cx, |ws, _| {
        let tr = &ws.session.project.tracks[t];
        (tr.instrument.is_none(), tr.eq.is_some())
    });
    assert!(audio && has_eq);
    std::fs::remove_dir_all(dir).unwrap();
}
