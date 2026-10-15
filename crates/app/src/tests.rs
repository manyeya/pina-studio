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
    project::{AuxSend, BEATS_PER_BAR, Content, PPQ},
    scan::PluginInfo,
    session::Session,
};

use crate::{pianoroll::KEY_H, theme::LANE_H, tracks::auto_to_y, workspace::*};

const BAR: u64 = PPQ * BEATS_PER_BAR;

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
            Content::Audio { .. } => unreachable!(),
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
fn mute_from_the_track_header(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    // M sits 46 px in from the header's right edge, which is the lanes' left edge.
    let m = spot(&ws, cx, 0, 1) - point(px(47.), px(0.));
    cx.simulate_click(m, Modifiers::none());
    assert!(ws.read_with(cx, |ws, _| ws.session.project.tracks[1].mute));
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
    // Click near the top of Lead's lane at beat 2: a loud point.
    let top = spot(&ws, cx, PPQ, 1) - point(px(0.), px(LANE_H / 2.0 - 8.));
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
            level_db: 0.0
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
    let items = ws.read_with(cx, |ws, _| ws.menu.as_ref().unwrap().items.len());
    assert_eq!(items, 2);
    click_on(cx, "menu-1", Modifiers::none(), MouseButton::Left);
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
    std::fs::remove_dir_all(&dir).unwrap();
}
