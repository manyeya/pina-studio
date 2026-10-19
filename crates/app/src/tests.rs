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
    // Playbooks of the tests' own: never the person's.
    let books = std::env::temp_dir().join(format!("tempo-test-books-{}", std::process::id()));
    unsafe { std::env::set_var("TEMPO_PLAYBOOKS", &books) };
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
        let y = ws.row_top(track) + ws.lane_h(track) / 2.0;
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

/// The window position of a tick and pitch in the piano roll, scrolled into view first if it's
/// off the top or bottom.
fn roll_spot(
    ws: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    tick: u64,
    pitch: u8,
) -> Point<Pixels> {
    let hidden = ws.read_with(cx, |ws, _| {
        let b = ws.roll_bounds.get();
        let (top, y) = (f32::from(b.origin.y), ws.pitch_y(pitch));
        y < top || y + ws.roll.key_h > top + f32::from(b.size.height)
    });
    if hidden {
        ws.update(cx, |ws, cx| {
            ws.roll_scroll_y = ((127 - pitch) as f32 * ws.roll.key_h - 40.0).max(0.0);
            cx.notify();
        });
        cx.run_until_parked();
    }
    // And the tick, scrolled to if it's off either side of the roll.
    let scroll = ws.read_with(cx, |ws, _| {
        let (x, w) = (
            tick as f32 * ws.px_per_tick(),
            f32::from(ws.roll_bounds.get().size.width),
        );
        let shown = ws.scroll_x + 20.0..ws.scroll_x + w - 40.0;
        (!shown.contains(&x)).then(|| (x - 80.0).max(0.0))
    });
    if let Some(to) = scroll {
        ws.update(cx, |ws, cx| {
            ws.scroll_x = to;
            cx.notify();
        });
        cx.run_until_parked();
    }
    ws.read_with(cx, |ws, _| {
        let x =
            f32::from(ws.roll_bounds.get().origin.x) + tick as f32 * ws.px_per_tick() - ws.scroll_x;
        point(px(x), px(ws.pitch_y(pitch) + ws.roll.key_h / 2.0))
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

/// Clicks the open menu's item whose label (without its tick) starts with this.
fn pick(cx: &mut VisualTestContext, ws: &Entity<Workspace>, label: &str) {
    let i = ws
        .read_with(cx, |ws, _| {
            let menu = ws.menu.as_ref()?;
            menu.items
                .iter()
                .position(|(l, _)| l.trim_start_matches("✓ ").starts_with(label))
        })
        .unwrap_or_else(|| panic!("no {label} in the menu"));
    click_on(
        cx,
        Box::leak(format!("menu-{i}").into_boxed_str()),
        Modifiers::none(),
        MouseButton::Left,
    );
}

#[gpui::test]
fn a_tracks_menu_its_pan_modes_its_height_and_the_columns_widths(cx: &mut TestAppContext) {
    use tempo_engine::project::{PanLaw, PanMode};
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
fn the_binaural_panner_places_a_sound_round_the_head(cx: &mut TestAppContext) {
    use crate::binaural::Camera;
    use tempo_engine::project::{PanMode, Place};
    let (ws, cx) = open(cx);
    // Binaural, from the pan knob's menu: the panner opens.
    click_on(cx, "track-pan-1", Modifiers::none(), MouseButton::Right);
    pick(cx, &ws, "Binaural");
    assert!(ws.read_with(cx, |ws, _| ws.panner.map(|p| p.0)) == Some(1));
    let place = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.place_of(1).unwrap());
    // Behind, with a click.
    click_on(cx, "panner-behind", Modifiers::none(), MouseButton::Left);
    assert_eq!(place(cx).azimuth, 180.0);
    click_on(cx, "panner-front", Modifiers::none(), MouseButton::Left);
    // Dragging the space round looks from elsewhere; the sound stays.
    let view = cx.debug_bounds("panner-view").unwrap();
    let corner = view.origin + point(px(20.), px(20.));
    drag(cx, corner, corner + point(px(50.), px(10.)));
    let orbit = ws.read_with(cx, |ws, _| ws.orbit);
    assert_ne!(orbit, crate::binaural::ORBIT);
    assert_eq!(place(cx).azimuth, 0.0);
    // The sound, dragged to a spot 1.5 m to the right of the head on the floor, goes there.
    let camera = Camera::new(orbit);
    // Inside its border.
    let origin = view.origin + point(px(1.), px(1.));
    let on_screen = |p: [f32; 3]| {
        let (x, y, _) = camera.project(p).unwrap();
        origin + point(px(x), px(y))
    };
    let from = on_screen(place(cx).xyz());
    drag(cx, from, on_screen([1.5, 0.0, 0.0]));
    let p = place(cx);
    assert!(
        (p.azimuth - 90.0).abs() < 1.0 && (p.distance - 1.5).abs() < 0.02,
        "{p:?}"
    );
    // ⌘-dragging it up raises it, still to the right.
    let from = on_screen(p.xyz());
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::command());
    cx.simulate_mouse_move(
        from - point(px(0.), px(80.)),
        MouseButton::Left,
        Modifiers::command(),
    );
    cx.simulate_mouse_up(
        from - point(px(0.), px(80.)),
        MouseButton::Left,
        Modifiers::command(),
    );
    cx.run_until_parked();
    let up = place(cx);
    assert!(
        up.elevation > 20.0 && (up.azimuth - 90.0).abs() < 1.0,
        "{up:?}"
    );
    let track = ws.read_with(cx, |ws, _| ws.session.project.tracks[1].clone());
    assert_eq!((track.pan_mode, track.pan), (PanMode::Binaural, 0.5));
    assert_eq!(track.elevation.map(f32::round), Some(up.elevation.round()));
    // The knob's ⌥-click: in front, level, a metre off.
    click_on(cx, "panner-close", Modifiers::none(), MouseButton::Left);
    let knob = cx.debug_bounds("track-pan-1").unwrap();
    let dial = point(knob.left() + px(15.), knob.center().y);
    cx.simulate_mouse_down(dial, MouseButton::Left, Modifiers::alt());
    cx.simulate_mouse_up(dial, MouseButton::Left, Modifiers::alt());
    cx.run_until_parked();
    assert_eq!(
        place(cx),
        Place {
            azimuth: 0.0,
            elevation: 0.0,
            distance: 1.0
        }
    );
}

#[gpui::test]
fn modulators_are_added_mapped_by_dragging_and_their_depth_set(cx: &mut TestAppContext) {
    use tempo_engine::modulate::{ModKind, ModTarget};
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.select_track(0);
        ws.show_bottom(Bottom::Modulation, cx);
    });
    cx.run_until_parked();
    let mods = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| ws.session.project.tracks[0].modulators.clone())
    };
    click_on(cx, "add-mod-LFO", Modifiers::none(), MouseButton::Left);
    assert!(matches!(mods(cx)[0].kind, ModKind::Lfo { .. }));
    // Its ↗ dragged onto the header's pan knob: the pan follows it, by its usual depth.
    let handle = cx.debug_bounds("mod-0-map").unwrap().center();
    let knob = cx.debug_bounds("track-pan-0").unwrap();
    let dial = point(knob.left() + px(12.), knob.center().y);
    drag(cx, handle, dial);
    let routes: Vec<_> = mods(cx)[0]
        .routes
        .iter()
        .map(|r| (r.target, r.depth))
        .collect();
    assert_eq!(routes, [(ModTarget::Pan, 0.8)]);
    // Its route dragged right: deeper (as far as pan goes, 2).
    let route = cx.debug_bounds("mod-0-route-Pan").unwrap().center();
    drag(cx, route, route + point(px(40.), px(0.)));
    let depth = mods(cx)[0].routes[0].depth;
    assert!((depth - 1.3).abs() < 0.01, "{depth}");
    // Picked, dragging the knob it moves sets its depth, not the pan.
    let pan = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].pan);
    click_on(cx, "mod-card-0", Modifiers::none(), MouseButton::Left);
    drag(cx, dial, dial - point(px(0.), px(40.)));
    let after = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].pan);
    assert_eq!(after, pan);
    assert!(
        mods(cx)[0].routes[0].depth > 1.6,
        "{}",
        mods(cx)[0].routes[0].depth
    );
    // Steps: painted in its picture, the first step set low.
    click_on(cx, "add-mod-Steps", Modifiers::none(), MouseButton::Left);
    let pic = cx.debug_bounds("mod-1-visual").unwrap();
    // Clear of its rounded corner: the first step, at the bottom.
    let low = point(pic.left() + px(10.), pic.bottom() - px(1.5));
    drag(cx, low, low);
    let ModKind::Steps { values, .. } = &mods(cx)[1].kind else {
        panic!("not steps")
    };
    assert_eq!(values[0], 0.0);
    // Taken off: the LFO and its route.
    click_on(cx, "mod-0-remove", Modifiers::none(), MouseButton::Left);
    let left = mods(cx);
    assert_eq!(left.len(), 1);
    assert!(matches!(left[0].kind, ModKind::Steps { .. }));
}

#[gpui::test]
fn settings_hide_editor_tabs_and_their_shortcuts_still_open_them(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| ws.show_bottom(Bottom::PianoRoll, cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("tab-Score").is_some());
    // ⌘, : Score and Live Loops switched off.
    cx.simulate_keystrokes("cmd-,");
    assert_eq!(ws.read_with(cx, |ws, _| ws.view), View::Settings);
    click_on(
        cx,
        "settings-Editor tabs",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(
        cx,
        "setting-tab-score",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(
        cx,
        "setting-tab-live_loops",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(cx, "settings-done", Modifiers::none(), MouseButton::Left);
    assert!(cx.debug_bounds("tab-Score").is_none());
    assert!(cx.debug_bounds("tab-Loops").is_none());
    assert!(cx.debug_bounds("tab-Takes").is_some());
    // N still opens the score, its tab there while it's open.
    cx.simulate_keystrokes("n");
    assert_eq!(ws.read_with(cx, |ws, _| ws.bottom), Some(Bottom::Score));
    assert!(cx.debug_bounds("tab-Score").is_some());
    // The last tab can't go.
    ws.update(cx, |ws, _| {
        ws.settings.hidden = Bottom::ALL[1..]
            .iter()
            .map(|b| b.key().to_string())
            .collect();
    });
    cx.simulate_keystrokes("cmd-,");
    click_on(
        cx,
        "setting-tab-piano_roll",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert!(ws.read_with(cx, |ws, _| ws.settings.shows(Bottom::PianoRoll)));
}

#[gpui::test]
fn settings_change_the_headers_regions_editing_and_stopping(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let flip = |cx: &mut VisualTestContext, section: &'static str, name: &'static str| {
        cx.simulate_keystrokes("cmd-,");
        click_on(cx, section, Modifiers::none(), MouseButton::Left);
        click_on(cx, name, Modifiers::none(), MouseButton::Left);
        click_on(cx, "settings-done", Modifiers::none(), MouseButton::Left);
    };
    // Headers without icons or record buttons; tracks taller.
    assert!(cx.debug_bounds("track-look-0").is_some() && cx.debug_bounds("arm-0").is_some());
    flip(cx, "settings-Track headers", "setting-header_icons");
    flip(cx, "settings-Track headers", "setting-header_record");
    assert!(cx.debug_bounds("track-look-0").is_none() && cx.debug_bounds("arm-0").is_none());
    cx.simulate_keystrokes("cmd-,");
    click_on(
        cx,
        "setting-height-Large",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(cx, "settings-done", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.lane_h(0)), 110.0);
    // No loop zone: the region's end trims, top to bottom.
    assert!(cx.debug_bounds("loop-tab-0-0").is_some());
    flip(cx, "settings-Editing", "setting-loop_zone");
    assert!(cx.debug_bounds("loop-tab-0-0").is_none());
    // A double-click in an empty lane writes nothing.
    flip(cx, "settings-Editing", "setting-double_click_writes");
    let regions = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| ws.session.project.tracks[1].regions.len())
    };
    let before = regions(cx);
    double_click(cx, spot(&ws, cx, BAR * 6, 1));
    assert_eq!(regions(cx), before);
    // Stopping goes back where play started.
    flip(cx, "settings-Transport", "setting-return_on_stop");
    // (The engine runs on its own thread: each move waited for.)
    ws.update(cx, |ws, cx| ws.locate_tick(BAR, cx));
    until(cx, &ws, |ws| ws.playhead() == BAR);
    ws.update(cx, |ws, cx| ws.play_stop(cx));
    until(cx, &ws, |ws| ws.player.playing());
    assert_eq!(ws.read_with(cx, |ws, _| ws.play_from), BAR);
    ws.update(cx, |ws, cx| ws.locate_tick(BAR * 3, cx));
    until(cx, &ws, |ws| ws.playhead() >= BAR * 3);
    ws.update(cx, |ws, cx| ws.play_stop(cx));
    until(cx, &ws, |ws| !ws.player.playing() && ws.playhead() == BAR);
}

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
        |r: &tempo_engine::project::Region| r.content.notes(r.length).map_or(0, |n| n.len());
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
    use tempo_engine::project::Content;
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
    use tempo_engine::project::Instrument;
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
    // Instrument narrows the list to instruments; tempo's synth is first.
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
        ws.roll.stamp = crate::pianoroll::Stamp::Triad;
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
                        tempo_engine::project::ControlValue::Cc { number: 1, .. }
                    )
                })
                .count(),
            _ => 0,
        }
    });
    assert!(moves >= 2, "{moves}");
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
        px(lane_top
            + crate::automation::to_y(tempo_engine::project::Param::Volume, points[0].value, 64.0)),
    );
    drag(cx, at, at + point(px(0.), px(30.)));
    let moved = volume(cx)[0];
    assert!(moved.value < points[0].value - 6.0, "{moved:?}");
    assert_eq!(moved.tick, PPQ);

    // Option-click deletes it.
    let at = point(
        top.x,
        px(lane_top
            + crate::automation::to_y(tempo_engine::project::Param::Volume, moved.value, 64.0)),
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
fn the_step_sequencer_paints_accents_mutes_fills_and_converts(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    click_on(cx, "new-track", Modifiers::none(), MouseButton::Left);
    click_on(cx, "adder-Drums", Modifiers::none(), MouseButton::Left);
    click_on(cx, "adder-row-0", Modifiers::none(), MouseButton::Left); // tempo's Drum Kit
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
        ws.row_op(at, 2, crate::steps::RowOp::Fill(2), cx);
        ws.row_op(at, 4, crate::steps::RowOp::Length(3), cx);
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
    click_on(cx, "adder-row-0", Modifiers::none(), MouseButton::Left); // tempo's Drum Kit
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
    use tempo_engine::modulate::{ModKind, ModTarget, SoundParam};
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
    assert_eq!(ws.read_with(cx, |ws, _| ws.picked.clone()), [2]);
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
    let param = ws.read_with(cx, |ws, _| ws.auto_lanes.get(&id).map(|l| l[0]));
    assert_eq!(param, Some(tempo_engine::project::Param::Pan));
    // Typing didn't reach the shortcuts ("a" hides automation).
    assert!(ws.read_with(cx, |ws, _| ws.show_automation && ws.menu.is_none()));
}

/// A window position in a track's `k`th automation lane: at a tick, at a value.
fn lane_spot(
    ws: &Entity<Workspace>,
    cx: &VisualTestContext,
    tick: u64,
    track: usize,
    k: usize,
    value: f32,
) -> Point<Pixels> {
    let x = spot(ws, cx, tick, track).x;
    ws.read_with(cx, |ws, _| {
        let param = ws.auto_lanes(track)[k];
        let y = f32::from(ws.lanes_bounds.get().origin.y)
            + ws.lane_top(track, k)
            + crate::automation::to_y(param, value, ws.auto_h());
        point(x, px(y))
    })
}

#[gpui::test]
fn controls_show_their_automation_and_moved_while_stopped_move_its_line(cx: &mut TestAppContext) {
    use crate::{automation::Stopped, strip::Channel};
    use tempo_engine::project::{AutoPoint, Param, value_at};
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
    use tempo_engine::project::{AutoMode, AutoPoint, Param, value_at};
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
    use tempo_engine::project::{AutoMode, AutoPoint, value_at};
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
    use tempo_engine::project::{Param, value_at};
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
        use tempo_engine::project::AutoPoint;
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
    use tempo_engine::project::{AutoPoint, Param, value_at};
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
    use tempo_engine::project::{AutoPoint, value_at};
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
    click_on(cx, "pad-4", Modifiers::none(), MouseButton::Left); // the first empty pad
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
        ws.viewport_w - ws.side_w() - ws.header_w - 240.0
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
    let shown = ws.read_with(cx, |ws, _| (ws.view, ws.show_browser));
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
    // F: the Browser beside the tracks; its Sounds, with the search field ready to type in.
    cx.simulate_keystrokes("f");
    let shown = ws.read_with(cx, |ws, _| (ws.view, ws.show_browser));
    assert_eq!(shown, (View::Arrange, true));
    click_on(cx, "scope-Sounds", Modifiers::none(), MouseButton::Left);
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
    let shown = ws.read_with(cx, |ws, _| (ws.view, ws.show_browser));
    assert_eq!(shown, (View::Arrange, true));
    cx.simulate_keystrokes("escape");
    // Escape hands the keyboard back to the window, the frame after.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    cx.simulate_keystrokes("f");
    assert!(!ws.read_with(cx, |ws, _| ws.show_browser));
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
        ws.show_browser = true;
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
    // Sounds, in the same panel: the search comes along, and back again.
    click_on(cx, "scope-Sounds", Modifiers::none(), MouseButton::Left);
    let sounds = ws.read_with(cx, |ws, cx| ws.browser.search.read(cx).text().to_string());
    assert_eq!(sounds, "echo");
    assert!(ws.read_with(cx, |ws, _| ws.browser_open()));
    let field = ws.read_with(cx, |ws, _| ws.browser.search.clone());
    field.update(cx, |i, cx| i.set_text("pad", cx));
    click_on(
        cx,
        "scope-Instruments",
        Modifiers::none(),
        MouseButton::Left,
    );
    let devices = ws.read_with(cx, |ws, cx| ws.catalog.search.read(cx).text().to_string());
    assert_eq!(devices, "pad");
    assert!(!ws.read_with(cx, |ws, _| ws.browser_open()));
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

#[gpui::test]
fn playbook_parts_are_added_edited_and_removed(cx: &mut TestAppContext) {
    // `open_at` gives the tests a playbooks folder of their own.
    let books = std::env::temp_dir().join(format!("tempo-test-books-{}", std::process::id()));
    let dir = demo_copy("playbook-parts");
    let (ws, cx) = open_at(cx, &dir);
    // Tall enough to show the whole page.
    cx.simulate_resize(gpui::size(px(1600.), px(3400.)));
    ws.update(cx, |ws, cx| ws.open_genres(Some("lofi"), cx));
    cx.run_until_parked();
    let parts = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| ws.genres.draft.as_ref().unwrap().parts.clone())
    };
    let saved = || {
        let json = std::fs::read_to_string(books.join("lofi.json")).unwrap();
        serde_json::from_str::<tempo_engine::playbook::Playbook>(&json).unwrap()
    };
    let before = parts(cx).len();

    // Add a percussion part: it shows, and it's saved.
    click_on(cx, "add-part-perc", Modifiers::none(), MouseButton::Left);
    assert_eq!(parts(cx).len(), before + 1);
    assert_eq!(saved().parts.last().unwrap().role, "perc");

    // Type what it sounds like into its field, and Enter: saved.
    let n = before;
    let field: &'static str = Box::leak(format!("part-sound-{n}").into_boxed_str());
    click_on(cx, field, Modifiers::none(), MouseButton::Left);
    cx.simulate_input("shaker, conga");
    cx.simulate_keystrokes("enter");
    assert_eq!(parts(cx)[n].sound, "shaker, conga");
    assert_eq!(saved().parts[n].sound, "shaker, conga");
    // And how to write it, kept when the click goes elsewhere.
    let field: &'static str = Box::leak(format!("part-write-{n}").into_boxed_str());
    click_on(cx, field, Modifiers::none(), MouseButton::Left);
    cx.simulate_input("write_drums afrobeats on a percussion kit");
    click_on(cx, "genre-harmony", Modifiers::none(), MouseButton::Left);
    assert_eq!(
        saved().parts[n].write,
        "write_drums afrobeats on a percussion kit"
    );

    // Remove it: gone from the page, the file, and every section.
    let remove: &'static str = Box::leak(format!("part-remove-{n}").into_boxed_str());
    click_on(cx, remove, Modifiers::none(), MouseButton::Left);
    assert_eq!(parts(cx).len(), before);
    let book = saved();
    assert!(!book.parts.iter().any(|p| p.role == "perc"));
    assert!(
        book.sections
            .iter()
            .all(|s| !s.parts.contains(&"perc".to_string()))
    );
    // tempo's again: the same as it ships.
    assert_eq!(
        tempo_engine::playbook::origin(&book),
        tempo_engine::playbook::Origin::Tempo
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn the_agents_model_and_effort_are_picked_in_its_panel(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_resize(gpui::size(px(1600.), px(1000.)));
    ws.update(cx, |ws, cx| {
        ws.chat.kind = crate::agent::Kind::ClaudeCode;
        ws.chat.tuning = Default::default();
        ws.show_agent = true;
        cx.notify();
    });
    cx.run_until_parked();
    let tuning = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.chat.tuning.clone());
    // By Send: its own model and effort until one's picked.
    assert_eq!(ws.read_with(cx, |ws, _| ws.tuning_label()), "Default model");
    click_on(cx, "agent-tuning", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Opus");
    click_on(cx, "agent-tuning", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Extra high");
    let t = tuning(cx);
    assert_eq!(
        (t.model.as_deref(), t.effort.as_deref()),
        (Some("opus"), Some("xhigh"))
    );
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.tuning_label()),
        "Opus · Extra high"
    );
    // Kept for the next song: what the agent starts with.
    assert_eq!(crate::agent::tuning(crate::agent::Kind::ClaudeCode), t);
    // Back to its own.
    click_on(cx, "agent-tuning", Modifiers::none(), MouseButton::Left);
    let own = ws.read_with(cx, |ws, _| {
        let m = ws.menu.as_ref().unwrap();
        m.items
            .iter()
            .rposition(|(l, _)| l.contains("Its own"))
            .unwrap()
    });
    click_on(
        cx,
        Box::leak(format!("menu-{own}").into_boxed_str()),
        Modifiers::none(),
        MouseButton::Left,
    );
    assert_eq!(tuning(cx).effort, None);
}

#[gpui::test]
fn sounds_are_picked_for_the_agent_given_parts_and_told_it(cx: &mut TestAppContext) {
    let dir = demo_copy("palette");
    let (ws, cx) = open_at(cx, &dir);
    cx.simulate_resize(gpui::size(px(1600.), px(1100.)));
    ws.update(cx, |ws, cx| {
        ws.show_agent = true;
        cx.notify();
    });
    cx.run_until_parked();
    let palette =
        |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.session.project.palette.clone());
    // Nothing picked: a chip in the message box opens the Browser's sounds to pick from.
    click_on(cx, "palette-start", Modifiers::none(), MouseButton::Left);
    let (picking, browser, scope) = ws.read_with(cx, |ws, _| {
        (ws.picking_sounds, ws.show_browser, ws.catalog.scope)
    });
    assert!(picking && browser && scope == crate::catalog::Scope::Sounds);
    assert!(cx.debug_bounds("picking-done").is_some());
    // A sound given (the library's ＋ does this), then back to the agent with Done.
    let drums = dir.join("Audio Files/drums.wav");
    ws.update(cx, |ws, cx| ws.give_agent(drums.clone(), cx));
    click_on(cx, "picking-done", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| ws.show_agent
        && !ws.show_browser
        && !ws.picking_sounds));
    assert_eq!(palette(cx).sounds.len(), 1);
    // In the message box: its part from its tag's menu, "only these".
    click_on(cx, "palette-role-0", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Drum loop");
    assert_eq!(palette(cx).sounds[0].role.as_deref(), Some("drum loop"));
    click_on(cx, "palette-only", Modifiers::none(), MouseButton::Left);
    assert!(palette(cx).only);
    // What the agent is told with the next message: these, for their parts, and only these;
    // then nothing more until they change.
    let note = ws.update(cx, |ws, _| ws.palette_note()).unwrap();
    assert!(
        note.contains("no others") && note.contains("drum loop: \"drums\""),
        "{note}"
    );
    assert!(note.contains(&drums.display().to_string()));
    assert_eq!(ws.update(cx, |ws, _| ws.palette_note()), None);
    // Files dropped from the Finder on the Agent panel are for the agent too.
    let keys = dir.join("Audio Files/keys.wav");
    std::fs::copy(&drums, &keys).unwrap();
    let at = ws.read_with(cx, |ws, _| point(ws.viewport_w - 40.0, 400.0));
    ws.update(cx, |ws, cx| {
        ws.drop_files(std::slice::from_ref(&keys), at, cx)
    });
    assert_eq!(palette(cx).sounds.len(), 2);
    let note = ws.update(cx, |ws, _| ws.palette_note()).unwrap();
    assert!(note.contains("any part: \"keys\""), "{note}");
    // Taken out with its ×; undone, back; given again, taken back.
    click_on(cx, "palette-remove-1", Modifiers::none(), MouseButton::Left);
    assert_eq!(palette(cx).sounds.len(), 1);
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(palette(cx).sounds.len(), 2);
    ws.update(cx, |ws, cx| ws.give_agent(keys.clone(), cx));
    assert!(!palette(cx).has(&keys));
    // Cleared: the agent hears it's on its own again; and it's kept in the song.
    click_on(cx, "palette-clear", Modifiers::none(), MouseButton::Left);
    assert!(palette(cx).is_empty());
    let note = ws.update(cx, |ws, _| ws.palette_note()).unwrap();
    assert!(note.contains("choose your own"));
    cx.simulate_keystrokes("cmd-z");
    ws.update(cx, |ws, cx| ws.save(cx));
    let saved = tempo_engine::project::Project::load(&dir).unwrap();
    assert_eq!(saved.palette.sounds.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui::test]
fn the_agent_panel_knows_the_song_and_its_critic(cx: &mut TestAppContext) {
    use crate::{chat::Item, producer::Stage};
    let dir = demo_copy("agent-producer");
    let (ws, cx) = open_at(cx, &dir);
    cx.simulate_resize(gpui::size(px(1600.), px(1200.)));
    ws.update(cx, |ws, cx| {
        ws.show_agent = true;
        cx.notify();
    });
    cx.run_until_parked();

    // The stages: the demo has a loop, its keys playing chords, but no genre or mix; a new
    // conversation offers what it hasn't had, the genre first.
    let done: Vec<bool> = ws.read_with(cx, |ws, _| {
        crate::producer::STAGES
            .iter()
            .map(|s| s.done(&ws.session.project))
            .collect()
    });
    assert_eq!(done, [false, true, true, false, false, false]);
    let first = ws.read_with(cx, |ws, _| ws.suggestions()[0].0.clone());
    assert_eq!(first, "Pick a genre for this song");

    // The genre picker sets the song's genre, from the playbooks.
    click_on(cx, "agent-genre", Modifiers::none(), MouseButton::Left);
    let items = ws.read_with(cx, |ws, _| {
        ws.menu
            .as_ref()
            .unwrap()
            .items
            .iter()
            .map(|i| i.0.clone())
            .collect::<Vec<_>>()
    });
    assert!(
        items.iter().any(|i| i.contains("Lo-fi"))
            && items.last().unwrap().contains("Edit playbooks")
    );
    let lofi = items.iter().position(|i| i.contains("Lo-fi")).unwrap();
    let name: &'static str = Box::leak(format!("menu-{lofi}").into_boxed_str());
    click_on(cx, name, Modifiers::none(), MouseButton::Left);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.genre.clone()),
        Some("lofi".into())
    );
    // And what each stage asks says the genre.
    let ask = ws.read_with(cx, |ws, _| Stage::Arrange.ask(&ws.session.project));
    assert!(ask.contains("Lo-fi hip-hop"), "{ask}");

    // The critic: the demo plays sine synths, so there's something to improve, with a fix each.
    click_on(cx, "critic-head", Modifiers::none(), MouseButton::Left);
    let notes = ws.read_with(cx, |ws, _| ws.chat.critic.notes.clone());
    assert!(
        notes
            .iter()
            .any(|n| n.level == "improve" && n.what.contains("sine")),
        "{notes:?}"
    );
    assert!(cx.debug_bounds("critic-fix-0").is_some());

    // What's selected goes with a request: the region, by its track and bars.
    ws.update(cx, |ws, _| ws.region = Some((0, 0)));
    let (label, said) = ws.read_with(cx, |ws, _| ws.selection_note().unwrap());
    assert!(label.starts_with("Keys · bar"), "{label}");
    assert!(said.contains("track 0 \"Keys\""), "{said}");

    // Undo this turn: the song as it was when the agent was asked, in one step.
    let before = ws.read_with(cx, |ws, _| ws.session.project.clone());
    ws.update(cx, |ws, cx| {
        ws.chat.before = Some(before.clone());
        ws.edit(cx, |ws| ws.session.project.tracks.remove(1).name.clear());
        ws.chat.items.push(Item::You("make it hit harder".into()));
        ws.chat.items.push(Item::End {
            seconds: 4.0,
            steps: 3,
            stopped: false,
            error: None,
        });
        cx.notify();
    });
    cx.run_until_parked();
    click_on(cx, "undo-turn", Modifiers::none(), MouseButton::Left);
    let tracks = ws.read_with(cx, |ws, _| ws.session.project.tracks.len());
    assert_eq!(tracks, before.tracks.len());
    assert!(cx.debug_bounds("undo-turn").is_none());
    // ⌘Z takes the undo back.
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks.len()),
        before.tracks.len() - 1
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn the_song_map_toggles_parts_and_genres_duplicate_and_go(cx: &mut TestAppContext) {
    use tempo_engine::playbook::{self, Origin};
    let dir = demo_copy("playbook-map");
    let (ws, cx) = open_at(cx, &dir);
    cx.simulate_resize(gpui::size(px(1600.), px(3400.)));
    ws.update(cx, |ws, cx| ws.open_genres(Some("house"), cx));
    cx.run_until_parked();
    let draft = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.genres.draft.clone().unwrap());
    // House's intro is drums alone: a click puts the bass in it, another takes it out.
    assert!(!draft(cx).sections[0].parts.contains(&"bass".to_string()));
    click_on(cx, "cell-bass-0", Modifiers::none(), MouseButton::Left);
    assert!(draft(cx).sections[0].parts.contains(&"bass".to_string()));
    assert_eq!(playbook::origin(&draft(cx)), Origin::Edited);
    click_on(cx, "cell-bass-0", Modifiers::none(), MouseButton::Left);
    assert_eq!(playbook::origin(&draft(cx)), Origin::Tempo);

    // A section picked: its energy and length change; a new one comes and goes.
    click_on(cx, "section-0", Modifiers::none(), MouseButton::Left);
    click_on(cx, "energy-5", Modifiers::none(), MouseButton::Left);
    click_on(cx, "bars-up", Modifiers::none(), MouseButton::Left);
    let s = draft(cx).sections[0].clone();
    assert_eq!((s.energy, s.bars), (5, 20));
    let n = draft(cx).sections.len();
    click_on(cx, "add-section", Modifiers::none(), MouseButton::Left);
    assert_eq!(draft(cx).sections.len(), n + 1);
    click_on(cx, "section-remove", Modifiers::none(), MouseButton::Left);
    assert_eq!(draft(cx).sections.len(), n);

    // Duplicate: a genre of the person's own; Delete takes it away again.
    click_on(cx, "genre-duplicate", Modifiers::none(), MouseButton::Left);
    let copy = draft(cx);
    assert_eq!(
        (copy.name.as_str(), playbook::origin(&copy)),
        ("House 2", Origin::Yours)
    );
    click_on(cx, "genre-reset", Modifiers::none(), MouseButton::Left);
    let gone = ws.read_with(cx, |ws, _| !ws.genres.list.iter().any(|p| p.id == copy.id));
    assert!(gone);
    // Reset to tempo's: house as it ships.
    ws.update(cx, |ws, cx| ws.open_genres(Some("house"), cx));
    click_on(cx, "genre-reset", Modifiers::none(), MouseButton::Left);
    assert_eq!(playbook::origin(&draft(cx)), Origin::Tempo);
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn the_composer_wraps_and_grows_as_its_written_in(cx: &mut TestAppContext) {
    let dir = demo_copy("composer-grows");
    let (ws, cx) = open_at(cx, &dir);
    cx.simulate_resize(gpui::size(px(1600.), px(1000.)));
    ws.update(cx, |ws, cx| {
        ws.show_agent = true;
        cx.notify();
    });
    cx.run_until_parked();
    let tall = |cx: &mut VisualTestContext| {
        cx.run_until_parked();
        f32::from(cx.debug_bounds("agent-input").unwrap().size.height)
    };
    let empty = tall(cx);
    click_on(cx, "agent-input", Modifiers::none(), MouseButton::Left);
    // A long request wraps onto more lines, and the composer grows to show them.
    let long = "make the whole song hit harder, not the bass, everything from the drums to the pads, the hook especially, and keep the swing";
    cx.simulate_input(long);
    let wrapped = tall(cx);

    assert!(wrapped >= empty + 19.0 * 2.0, "{empty} -> {wrapped}");
    // ⇧Enter starts a new line in it; it grows again.
    cx.simulate_keystrokes("shift-enter");
    cx.simulate_input("thanks");
    let more = tall(cx);
    assert!(more >= wrapped + 19.0, "{wrapped} -> {more}");
    let text = ws.read_with(cx, |ws, cx| ws.chat.input.read(cx).text().to_string());
    assert_eq!(text, format!("{long}\nthanks"));
    // But not past eight lines: then it scrolls.
    for _ in 0..12 {
        cx.simulate_keystrokes("shift-enter");
        cx.simulate_input("more");
    }
    let most = tall(cx);
    assert!(most <= empty + 19.0 * 8.0, "{most}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn a_song_studied_shows_its_form_and_becomes_the_mix_reference(cx: &mut TestAppContext) {
    // Studies of the test's own, never the person's.
    let dir = std::env::temp_dir().join(format!("tempo-test-studies-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    unsafe { std::env::set_var("TEMPO_STUDIES", &dir) };
    // Ten seconds at 120 BPM: a kick on every beat over an A minor chord.
    let rate = 44_100u32;
    let l: Vec<f32> = (0..rate as usize * 10)
        .map(|i| {
            let t = i as f32 / rate as f32;
            let beat = t % 0.5;
            let chord: f32 = [220.0f32, 261.63, 329.63]
                .iter()
                .map(|f| (std::f32::consts::TAU * f * t).sin())
                .sum();
            0.12 * chord + 0.6 * (-beat * 30.0).exp() * (std::f32::consts::TAU * 55.0 * beat).sin()
        })
        .collect();
    let song = dir.join("Someone - A Test.wav");
    tempo_engine::analysis::write_wav(&song, &l, &l, rate).unwrap();
    let studied = crate::study_cli::study(&song, false).unwrap();
    assert_eq!(studied.identity.title, "A Test");
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| ws.open_study(cx));
    cx.run_until_parked();
    assert!(ws.read_with(cx, |ws, _| ws.view == View::Study
        && ws.studies.open.as_ref() == Some(&song)));
    assert!(cx.debug_bounds("study-title").is_some());
    // A section picked opens up.
    click_on(cx, "study-section-0", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.studies.section), Some(0));
    assert!(cx.debug_bounds("study-section-detail").is_some());
    // Its mix becomes this song's reference.
    click_on(cx, "study-reference", Modifiers::none(), MouseButton::Left);
    let reference = ws
        .read_with(cx, |ws, _| ws.session.project.reference.clone())
        .unwrap();
    assert_eq!(reference.file, song);
    assert!((reference.lufs - studied.mix.overall.lufs).abs() < 0.01);
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui::test]
fn the_sampler_takes_samples_slices_them_and_its_knobs_and_marks_turn(cx: &mut TestAppContext) {
    use tempo_engine::project::{FilterMode, Instrument, LoopMode};
    // A song of the test's own, so samples are copied into it, not the demo.
    let dir = std::env::temp_dir().join(format!("tempo-test-sampler-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let loop_file = dir.join("Loop.wav");
    let rate = 48_000u32;
    let hits: Vec<f32> = (0..rate as usize)
        .map(|i| {
            let k = i % 12_000;
            (k as f32 * 0.7).sin() * (-(k as f32) / 300.0).exp()
        })
        .collect();
    tempo_engine::analysis::write_wav(&loop_file, &hits, &hits, rate).unwrap();
    let song = dir.join("Song");
    let (ws, cx) = open_at(cx, &song);
    let sampler = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            match ws
                .sampler_track()
                .map(|t| ws.session.project.tracks[t].instrument.clone())
            {
                Some(Some(Instrument::Sampler(s))) => s,
                _ => panic!("no sampler"),
            }
        })
    };
    ws.update(cx, |ws, cx| ws.show_bottom(Bottom::Sampler, cx));
    cx.run_until_parked();
    click_on(cx, "sampler-new", Modifiers::none(), MouseButton::Left);
    ws.update(cx, |ws, cx| {
        ws.add_samples(std::slice::from_ref(&loop_file), cx)
    });
    cx.run_until_parked();
    // One sample: across the keys, copied into the song.
    let s = sampler(cx);
    assert_eq!(s.zones.len(), 1);
    assert_eq!((s.zones[0].note, s.zones[0].high), (0, Some(127)));
    assert!(song.join(&s.zones[0].file).is_file());
    // Its waveform read (in the background), its start mark dragged a quarter in.
    let mut tries = 0;
    while cx.debug_bounds("mark-Start").is_none() && tries < 50 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        ws.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        tries += 1;
    }
    let wave = cx.debug_bounds("sampler-wave").unwrap();
    let mark = cx.debug_bounds("mark-Start").unwrap().center();
    let quarter = f32::from(wave.size.width) * 0.25;
    drag(cx, mark, mark + point(px(quarter), px(0.)));
    let start = sampler(cx).zones[0].start;
    assert!((start - 0.25).abs() < 0.03, "{start}");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(sampler(cx).zones[0].start, 0.0);
    // The attack knob turned up.
    let knob = cx.debug_bounds("sampler-Attack").unwrap().center();
    drag(cx, knob, knob - point(px(0.), px(60.)));
    assert!(
        sampler(cx).voice.attack_ms > 1.0,
        "{}",
        sampler(cx).voice.attack_ms
    );
    // A low-pass filter, and a loop.
    click_on(cx, "filter-LP", Modifiers::none(), MouseButton::Left);
    click_on(cx, "loop-Loop", Modifiers::none(), MouseButton::Left);
    let s = sampler(cx);
    assert_eq!(
        (s.voice.filter, s.zones[0].loop_mode),
        (FilterMode::LowPass, LoopMode::Forward)
    );
    // Sliced at its hits: four of them, on C1 up.
    ws.update(cx, |ws, cx| {
        ws.slice_zone(crate::sampler::Slicing::Hits(8), cx)
    });
    let s = sampler(cx);
    assert_eq!(
        s.zones.iter().map(|z| z.note).collect::<Vec<_>>(),
        [36, 37, 38, 39],
        "{:?}",
        s.zones
            .iter()
            .map(|z| (z.note, z.start))
            .collect::<Vec<_>>()
    );
    for (k, z) in s.zones.iter().enumerate() {
        assert!((z.start - k as f32 * 0.25).abs() < 0.01, "{}", z.start);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Lets a background job and the timers watching it run, until `done`.
fn settle(cx: &mut VisualTestContext, ws: &Entity<Workspace>, done: impl Fn(&Workspace) -> bool) {
    for _ in 0..400 {
        cx.executor().advance_clock(Duration::from_millis(150));
        cx.run_until_parked();
        if ws.read_with(cx, |ws, _| done(ws)) {
            return;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("timed out");
}

#[gpui::test]
fn the_bounce_sheet_writes_stems_in_several_formats(cx: &mut TestAppContext) {
    let song = demo_copy("bounce");
    let (ws, cx) = open_at(cx, &song);
    cx.simulate_keystrokes("cmd-b");
    assert!(ws.read_with(cx, |ws, _| ws.export.is_some()));
    click_on(
        cx,
        "bounce-what-Stems",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(
        cx,
        "bounce-format-MP3",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(
        cx,
        "bounce-format-MIDI",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(cx, "bounce-level-2", Modifiers::none(), MouseButton::Left); // to a loudness
    click_on(cx, "bounce-go", Modifiers::none(), MouseButton::Left);
    settle(cx, &ws, |ws| {
        ws.export
            .as_ref()
            .and_then(|e| e.run.as_ref())
            .is_some_and(|r| r.finished)
    });
    // A file of each format for each track with something on it.
    let name = song.file_name().unwrap().to_string_lossy().to_string();
    for track in ["Keys", "Lead", "Drums"] {
        for ext in ["wav", "mp3", "mid"] {
            let f = song.join("Bounces").join(format!("{name} - {track}.{ext}"));
            assert!(f.is_file(), "{}", f.display());
        }
    }
    // 24-bit: its header says so (bits per sample at byte 34).
    let wav = std::fs::read(song.join("Bounces").join(format!("{name} - Keys.wav"))).unwrap();
    assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 24);
    click_on(cx, "bounce-cancel", Modifiers::none(), MouseButton::Left); // Done
    assert!(ws.read_with(cx, |ws, _| ws.export.is_none()));
    let _ = std::fs::remove_dir_all(&song);
}

#[gpui::test]
fn bounce_in_place_puts_a_track_on_audio_and_mutes_it(cx: &mut TestAppContext) {
    use tempo_engine::project::Content;
    let song = demo_copy("in-place");
    let (ws, cx) = open_at(cx, &song);
    let before = ws.read_with(cx, |ws, _| ws.session.project.tracks.len());
    click_on(cx, "header-0", Modifiers::none(), MouseButton::Right);
    pick(cx, &ws, "Bounce in Place");
    settle(cx, &ws, |ws| ws.session.project.tracks.len() == before + 1);
    let (muted, new) = ws.read_with(cx, |ws, _| {
        (
            ws.session.project.tracks[0].mute,
            ws.session.project.tracks[1].clone(),
        )
    });
    assert!(muted);
    assert!(new.instrument.is_none() && new.name.ends_with("(bounced)"));
    let Content::Audio { file, .. } = &new.regions[0].content else {
        panic!("not audio")
    };
    assert!(song.join(file).is_file());
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(
        ws.read_with(cx, |ws, _| (
            ws.session.project.tracks.len(),
            ws.session.project.tracks[0].mute
        )),
        (before, false)
    );
    let _ = std::fs::remove_dir_all(&song);
}

/// Pages drawn to PNGs, to look at without a window on screen:
/// `TEMPO_SHOT=<dir> TEMPO_SHOT_PAGE=study cargo test -p tempo shots -- --ignored`. The Song
/// Study page shows the studies kept on this Mac, the first or the one whose file name has
/// `TEMPO_SHOT_SONG` in it.
#[test]
#[ignore]
fn shots() {
    let out =
        PathBuf::from(std::env::var("TEMPO_SHOT").expect("TEMPO_SHOT: a folder for the PNGs"));
    std::fs::create_dir_all(&out).unwrap();
    let page = std::env::var("TEMPO_SHOT_PAGE").unwrap_or_else(|_| "study".into());
    let tall: f32 = std::env::var("TEMPO_SHOT_H")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(900.0);
    let mut cx = gpui::HeadlessAppContext::with_platform(
        std::sync::Arc::new(gpui_apple::AppleTextSystem::new()),
        std::sync::Arc::new(()),
        || {
            Ok(Some(Box::new(
                gpui_apple::metal_renderer::MetalHeadlessRenderer::new(),
            )))
        },
    );
    cx.update(|cx| {
        cx.text_system().add_fonts(crate::theme::fonts()).unwrap();
        bind_keys(cx);
    });
    let session = Session::open(&demo()).unwrap();
    let player = Player::silent(&session.project, &session.dir);
    let window = cx
        .open_window(gpui::size(px(1440.), px(tall)), |window, cx| {
            gpui::AppContext::new(cx, |cx| Workspace::new(session, player, window, cx))
        })
        .unwrap();
    cx.run_until_parked();
    let song = std::env::var("TEMPO_SHOT_SONG").ok();
    cx.update(|cx| {
        window
            .update(cx, |ws, window, cx| match page.as_str() {
                "study" => {
                    ws.open_study(cx);
                    if let Some(want) = &song {
                        ws.studies.open = ws
                            .studies
                            .list
                            .iter()
                            .find(|s| s.file.to_string_lossy().contains(want.as_str()))
                            .map(|s| s.file.clone());
                    }
                }
                "study-empty" => {
                    ws.open_study(cx);
                    ws.studies.list.clear();
                    ws.studies.open = None;
                }
                "playbooks" => ws.open_genres(None, cx),
                "roll" => {
                    ws.track = 0;
                    ws.region = Some((0, 0));
                    ws.editor_h = 460.0;
                    ws.show_bottom(Bottom::PianoRoll, cx);
                }
                "roll-pro" => {
                    ws.track = 0;
                    ws.region = Some((0, 0));
                    ws.editor_h = 460.0;
                    ws.show_bottom(Bottom::PianoRoll, cx);
                    ws.picked = vec![0, 1, 2, 3];
                    ws.roll.by_velocity = true;
                    ws.roll.in_key = true;
                    ws.roll.stamp = crate::pianoroll::Stamp::Triad;
                    if let Some(notes) = ws.session.project.notes_mut((0, 0)) {
                        for (i, n) in notes.iter_mut().enumerate() {
                            n.velocity = (40 + i * 6).min(127) as u8;
                        }
                    }
                }
                "sampler" => {
                    use tempo_engine::project::{
                        FilterMode, Instrument, Sampler, SamplerVoice, Track,
                    };
                    let file = PathBuf::from(
                        std::env::var("TEMPO_SHOT_SAMPLE").expect("TEMPO_SHOT_SAMPLE: a loop"),
                    );
                    let (sides, rate) = tempo_engine::engine::read_audio(&file).unwrap();
                    let found = tempo_engine::sampler::onsets(&sides[0], rate as f32);
                    let cut = tempo_engine::sampler::cuts(&found, 8);
                    let zones = tempo_engine::sampler::slices(&file, &cut, 36);
                    let voice = SamplerVoice {
                        filter: FilterMode::LowPass,
                        cutoff_hz: 6000.0,
                        resonance: 0.3,
                        filter_env: 2.0,
                        attack_ms: 3.0,
                        decay_ms: 250.0,
                        sustain: 0.7,
                        release_ms: 180.0,
                        ..Default::default()
                    };
                    ws.session.project.tracks.push(Track {
                        name: "Break".into(),
                        instrument: Some(Instrument::Sampler(Sampler { zones, voice })),
                        ..Default::default()
                    });
                    ws.session.project.assign_ids();
                    ws.select_track(ws.session.project.tracks.len() - 1);
                    ws.sampler_ui.zone = 2;
                    ws.show_bottom(Bottom::Sampler, cx);
                }
                p if p.starts_with("adder") => {
                    // The plugins as last scanned, their presets as last read, the library.
                    let cached: serde_json::Value = std::fs::read(tempo_engine::scan::cache_file())
                        .ok()
                        .and_then(|b| serde_json::from_slice(&b).ok())
                        .unwrap_or_default();
                    let plugins: Vec<PluginInfo> = cached
                        .as_object()
                        .map(|m| {
                            m.values()
                                .flat_map(|c| {
                                    serde_json::from_value::<Vec<PluginInfo>>(c["plugins"].clone())
                                        .unwrap_or_default()
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    ws.plugins = Some(plugins);
                    ws.catalog_plugins();
                    ws.presets_now();
                    // Read, not rescanned: the person's library is only looked at.
                    ws.set_library(tempo_engine::samples::library::Library::load(), cx);
                    ws.open_adder(gpui::point(px(250.), px(110.)), window, cx);
                    if let Some(q) = p.strip_prefix("adder-") {
                        ws.adder
                            .as_ref()
                            .unwrap()
                            .search
                            .clone()
                            .update(cx, |s, cx| s.set_text(q, cx));
                    }
                }
                "bounce" => {
                    ws.open_export(cx);
                    ws.choose(crate::menu::MenuAction::Nothing, cx);
                }
                "bounce-stems" => {
                    ws.export_stem(0, cx);
                    if let Some(e) = ws.export.as_mut() {
                        e.tracks = vec![true; e.tracks.len()];
                        e.formats = vec![
                            tempo_engine::bounce::Format::Wav,
                            tempo_engine::bounce::Format::Mp3 { kbps: 320 },
                            tempo_engine::bounce::Format::Flac,
                        ];
                        e.level = tempo_engine::bounce::Level::Loudness {
                            lufs: -14.0,
                            ceiling: -1.0,
                        };
                    }
                }
                "bounce-done" => {
                    ws.export_stem(0, cx);
                    if let Some(e) = ws.export.as_mut() {
                        e.tracks = vec![true; e.tracks.len()];
                        e.formats = vec![
                            tempo_engine::bounce::Format::Wav,
                            tempo_engine::bounce::Format::Mp3 { kbps: 320 },
                        ];
                        e.level = tempo_engine::bounce::Level::Loudness {
                            lufs: -14.0,
                            ceiling: -1.0,
                        };
                        e.folder = out.join("bounces");
                    }
                    ws.start_export(cx);
                }
                "steps-empty" => {
                    ws.select_track(1);
                    ws.region = Some((1, 0));
                    ws.editor_h = 380.0;
                    ws.show_bottom(Bottom::Steps, cx);
                }
                "steps" | "steps-mod" => {
                    ws.session
                        .project
                        .tracks
                        .push(tempo_engine::project::Track {
                            name: "Kit".into(),
                            instrument: Some(tempo_engine::project::Instrument::Drums),
                            ..Default::default()
                        });
                    ws.session.project.assign_ids();
                    ws.track = ws.session.project.tracks.len() - 1;
                    ws.editor_h = 380.0;
                    ws.new_pattern(cx);
                    // A groove to look at: kick, snare, hats with accents, a three-step perc.
                    let at = ws.pattern().unwrap();
                    if let Content::Pattern { rows, .. } =
                        &mut ws.session.project.tracks[at.0].regions[at.1].content
                    {
                        rows[0].cells = vec![120, 0, 0, 0, 0, 0, 90, 0, 110, 0, 0, 0, 0, 0, 70, 0];
                        rows[1].cells = vec![0, 0, 0, 0, 127, 0, 0, 0, 0, 0, 0, 0, 127, 0, 0, 40];
                        rows[2].cells =
                            (0..16).map(|k| if k % 2 == 0 { 100 } else { 55 }).collect();
                        rows[3].cells = vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 100, 0];
                        rows[4].cells = vec![90, 0, 0];
                        rows[4].mute = false;
                    }
                    ws.steps_row = 2;
                    if page == "steps-mod" {
                        use tempo_engine::modulate::{
                            LfoShape, ModKind, ModRate, ModRoute, ModTarget, Modulator, SoundParam,
                        };
                        if let Content::Pattern { rows, .. } =
                            &mut ws.session.project.tracks[at.0].regions[at.1].content
                        {
                            rows[0].repeat = vec![1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 3, 1];
                            rows[1].chance = vec![
                                100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100,
                                100, 100, 50,
                            ];
                            rows[2].transpose =
                                vec![0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 7, 0];
                        }
                        let t = &mut ws.session.project.tracks[at.0];
                        t.modulators = vec![
                            Modulator {
                                kind: ModKind::Lfo {
                                    shape: LfoShape::Sine,
                                    rate: ModRate::Beats(2.0),
                                    phase: 0.0,
                                    unipolar: false,
                                },
                                routes: vec![ModRoute {
                                    target: ModTarget::Sound(2, SoundParam::Velocity),
                                    depth: -40.0,
                                }],
                            },
                            Modulator {
                                kind: ModKind::Steps {
                                    values: vec![
                                        1.0, 0.2, 0.6, 0.2, 0.9, 0.2, 0.6, 0.35, 1.0, 0.2, 0.6,
                                        0.2, 0.9, 0.4, 0.7, 0.3,
                                    ],
                                    rate: ModRate::Beats(0.25),
                                    glide: 0.0,
                                },
                                routes: vec![ModRoute {
                                    target: ModTarget::Volume,
                                    depth: -12.0,
                                }],
                            },
                        ];
                        ws.editor_h = 470.0;
                    }
                }
                // The tracks with the sample browser beside them and the piano roll under them,
                // no inspector: the columns' edges, to line up.
                "beside" | "beside-agent" | "beside-sounds" => {
                    ws.show_inspector = page != "beside";
                    ws.show_browser = page != "beside-agent";
                    ws.show_agent = page == "beside-agent";
                    if page == "beside-sounds" {
                        ws.catalog.scope = crate::catalog::Scope::Sounds;
                    }
                    ws.track = 1;
                    ws.show_bottom(Bottom::PianoRoll, cx);
                }
                // The agent's model and effort, picked by Send.
                "agent-tuning" => {
                    ws.chat.kind = crate::agent::Kind::ClaudeCode;
                    ws.chat.tuning = crate::agent::Tuning {
                        model: Some("opus".into()),
                        effort: Some("high".into()),
                    };
                    ws.show_agent = true;
                    let at = point(px(1440. - 200.), px(62.));
                    ws.menu = Some(ws.tuning_menu(at));
                    cx.notify();
                }
                // Sounds picked for the agent: in its message box, and the Browser picking them.
                "palette" | "palette-pick" => {
                    use tempo_engine::{project::PaletteSound, samples::Category};
                    if ws.browser.library.samples.is_empty() {
                        ws.browser.library = tempo_engine::samples::library::Library::load();
                        ws.browser.loaded = true;
                        ws.refilter(cx);
                    }
                    let lib = &ws.browser.library.samples;
                    let mut sounds = Vec::new();
                    for want in [
                        Category::Kick,
                        Category::Snare,
                        Category::HatClosed,
                        Category::Bass808,
                        Category::Keys,
                        Category::Pad,
                    ] {
                        if let Some(s) = lib.iter().find(|s| s.analysis.category == want) {
                            sounds.push(PaletteSound {
                                path: s.path.clone(),
                                role: crate::palette::role_for(want).map(str::to_string),
                            });
                        }
                    }
                    if let Some(s) = sounds.get_mut(4) {
                        s.role = None;
                    }
                    ws.session.project.palette.sounds = sounds;
                    ws.session.project.palette.only = true;
                    ws.show_agent = true;
                    if page == "palette-pick" {
                        ws.pick_sounds(cx);
                    }
                    cx.notify();
                }
                "settings-auto" => {
                    ws.settings_section = crate::settings::Section::Automation;
                    ws.open_settings(cx);
                }
                "settings" => {
                    ws.settings.hidden = vec!["score".into(), "takes".into(), "live_loops".into()];
                    ws.open_settings(cx);
                }
                // Modulation: an LFO, steps, an envelope, a follower and random on the keys,
                // mapped to its pan, volume, low cut and presence; values as if playing.
                "modulation" => {
                    use tempo_engine::modulate::{
                        LfoShape, ModKind, ModRate, ModRoute, ModTarget, Modulator,
                    };
                    let drums = ws.session.project.tracks[2].id;
                    let route = |target, depth| ModRoute { target, depth };
                    let t = &mut ws.session.project.tracks[0];
                    t.eq = Some(Default::default());
                    t.modulators = vec![
                        Modulator {
                            kind: ModKind::Lfo {
                                shape: LfoShape::Sine,
                                rate: ModRate::Beats(0.5),
                                phase: 0.0,
                                unipolar: false,
                            },
                            routes: vec![
                                route(ModTarget::Pan, 0.7),
                                route(ModTarget::EqGain(2), 6.0),
                            ],
                        },
                        Modulator {
                            kind: ModKind::steps(),
                            routes: vec![route(ModTarget::LowCut, 2.5)],
                        },
                        Modulator {
                            kind: ModKind::envelope(),
                            routes: vec![],
                        },
                        Modulator {
                            kind: ModKind::follower(drums),
                            routes: vec![route(ModTarget::Volume, -18.0)],
                        },
                        Modulator {
                            kind: ModKind::Random {
                                rate: ModRate::Beats(0.25),
                                smooth: true,
                            },
                            routes: vec![],
                        },
                    ];
                    let id = t.id;
                    ws.mod_values
                        .insert(id, [0.6, 0.85, 0.4, 0.7, -0.3, 0.0, 0.0, 0.0]);
                    for k in 0..5 {
                        let trace = (0..96)
                            .map(|i| match k {
                                3 => (1.0 - (i % 24) as f32 / 24.0).powf(2.0) * 0.9,
                                _ => ((i as f32) * 0.21).sin() * 0.7,
                            })
                            .collect();
                        ws.mod_trace.insert((id, k), trace);
                    }
                    ws.track = 0;
                    ws.mod_edit = Some((0, 0));
                    ws.editor_h = 330.0;
                    ws.show_bottom(Bottom::Modulation, cx);
                }
                // The console headers: a stereo pan, a binaural one, a tall track, a muted region;
                // and each right-click menu.
                // Automation: the keys' volume rising, the lead's pan swinging, in Read and Latch.
                "automation" => {
                    use tempo_engine::project::{AutoMode, AutoPoint, Param};
                    let pt = |tick, value| AutoPoint::new(tick, value);
                    let bent = |tick, value, curve| AutoPoint {
                        curve,
                        ..pt(tick, value)
                    };
                    let step = |tick, value| AutoPoint {
                        step: true,
                        ..pt(tick, value)
                    };
                    let ids: Vec<u64> = ws.session.project.tracks.iter().map(|t| t.id).collect();
                    let tracks = &mut ws.session.project.tracks;
                    // Keys: a swell in, held, a dip; its pan swinging.
                    tracks[0].automation.volume = vec![
                        bent(0, -40.0, 0.6),
                        pt(BAR * 2, -4.0),
                        pt(BAR * 3, -4.0),
                        bent(BAR * 3 + BAR / 2, -14.0, -0.5),
                        pt(BAR * 4, -6.0),
                    ];
                    tracks[0].automation.pan = (0..9)
                        .map(|i| {
                            bent(
                                i * BAR / 2,
                                if i % 2 == 0 { -0.7 } else { 0.7 },
                                if i % 2 == 0 { -0.3 } else { 0.3 },
                            )
                        })
                        .collect();
                    tracks[0].automation.mode = AutoMode::Read;
                    // Lead in Latch; the drums muted in steps, in Touch.
                    tracks[1].automation.volume = vec![pt(0, -6.0), pt(BAR * 4, -6.0)];
                    tracks[1].automation.mode = AutoMode::Latch;
                    tracks[2].automation.mute = vec![
                        step(0, 0.0),
                        step(BAR + BAR * 3 / 4, 1.0),
                        step(BAR * 2, 0.0),
                        step(BAR * 3 + BAR * 3 / 4, 1.0),
                        step(BAR * 4, 0.0),
                    ];
                    tracks[2].automation.mode = AutoMode::Touch;
                    ws.auto_lanes
                        .insert(ids[0], vec![Param::Volume, Param::Pan]);
                    ws.auto_picked = Some(crate::automation::Picked {
                        track: ids[0],
                        param: Param::Volume,
                        points: vec![2, 3],
                    });
                    ws.show_automation = true;
                    let at = ws.session.project.samples(BAR + BAR / 4, ws.player.rate());
                    ws.player.locate(at);
                    cx.notify();
                }
                // The automation lanes' menus: a mode's, the shapes', a point's.
                "auto-modes" | "auto-shapes" | "auto-point" => {
                    use tempo_engine::project::{AutoPoint, Param};
                    let id = ws.session.project.tracks[0].id;
                    ws.session.project.tracks[0].automation.volume = vec![
                        AutoPoint {
                            curve: 0.5,
                            ..AutoPoint::new(0, -30.0)
                        },
                        AutoPoint {
                            ease: true,
                            ..AutoPoint::new(BAR * 2, -3.0)
                        },
                        AutoPoint::new(BAR * 4, -20.0),
                    ];
                    // A wobbly pan, as recorded, its second half smoothed.
                    let a = &mut ws.session.project.tracks[0].automation;
                    a.pan = (0..=64)
                        .map(|i| {
                            let wob = ((i * 7919) % 13) as f32 / 13.0 - 0.5;
                            let v = i as f32 / 40.0 - 0.6 + wob * 0.5;
                            AutoPoint::new(i * PPQ / 4, v.clamp(-1.0, 1.0))
                        })
                        .collect();
                    a.smooth(Param::Pan, BAR * 2, BAR * 4, PPQ / 2);
                    ws.auto_lanes.insert(id, vec![Param::Volume, Param::Pan]);
                    ws.show_automation = true;
                    let at = point(px(900.), px(240.));
                    match page.as_str() {
                        "auto-modes" => ws.mode_menu(0, at, cx),
                        "auto-shapes" => ws.auto_lane_menu(0, 0, at, cx),
                        _ => ws.point_menu(0, 0, 1, at, cx),
                    }
                }
                // The Tool menu open on the Fade tool, the drums faded in and out.
                "tools" => {
                    let drums = &mut ws.session.project.tracks[2];
                    drums.height = Some(110.0);
                    (drums.regions[0].fade_in, drums.regions[0].fade_out) = (PPQ * 2, PPQ / 2);
                    drums.regions[3].fade_out = PPQ * 3;
                    ws.tool = Tool::Fade;
                    ws.status = Some(Tool::Fade.hint().into());
                    ws.tool_menu(point(px(ws.side_w() + 14.), px(96.)), cx);
                }
                "tracks" | "track-menu" | "pan-menu" | "region-menu" | "lane-menu" | "panner" => {
                    use tempo_engine::project::PanMode;
                    let tracks = &mut ws.session.project.tracks;
                    tracks[0].pan = -0.3;
                    tracks[1].mute = page == "tracks";
                    tracks[1].height = (page == "tracks").then_some(44.0);
                    tracks[1].pan_mode = PanMode::Stereo;
                    tracks[1].spread = Some(0.6);
                    tracks[2].pan_mode = PanMode::Binaural;
                    tracks[2].pan = 0.45;
                    tracks[2].height = Some(130.0);
                    tracks[2].regions[1].mute = true;
                    // Levels, as if playing.
                    for (t, now) in [(0, [0.62f32, 0.55]), (1, [0.3, 0.34]), (2, [0.9, 0.85])] {
                        let id = ws.session.project.tracks[t].id;
                        let mut level = crate::workspace::Level::default();
                        (level.now, level.held) = (now, now.map(|l| (l * 1.25).min(1.0)));
                        ws.levels.insert(Some(id), level);
                    }
                    let at = point(px(330.), px(160.));
                    match page.as_str() {
                        "track-menu" => ws.track_menu(0, at, cx),
                        "pan-menu" => ws.pan_menu(1, at, cx),
                        "region-menu" => ws.region_menu((0, 0), point(px(700.), px(160.)), cx),
                        "lane-menu" => ws.lane_menu(0, BAR * 6, point(px(900.), px(160.)), cx),
                        "panner" => {
                            let t = &mut ws.session.project.tracks[2];
                            (t.pan, t.elevation, t.distance) = (0.3, Some(28.0), Some(1.7));
                            ws.levels.insert(Some(t.id), Default::default());
                            ws.open_panner(2, cx);
                        }
                        _ => cx.notify(),
                    }
                }
                _ => {}
            })
            .unwrap()
    });
    cx.run_until_parked();
    // A bounce runs to its end before it's drawn.
    for _ in 0..600 {
        let running = cx.update(|cx| {
            window
                .read(cx)
                .map(|ws| {
                    ws.export
                        .as_ref()
                        .and_then(|e| e.run.as_ref())
                        .is_some_and(|r| !r.finished)
                })
                .unwrap_or(false)
        });
        if !running {
            break;
        }
        cx.advance_clock(Duration::from_millis(150));
        cx.run_until_parked();
        thread::sleep(Duration::from_millis(5));
    }
    let image = cx.capture_screenshot(window.into()).unwrap();
    let file = out.join(format!("{page}.png"));
    image.save(&file).unwrap();
    println!("{}", file.display());
}
