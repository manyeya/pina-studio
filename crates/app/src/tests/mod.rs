//! The window, driven like a person would: mouse drags and shortcuts on the demo project, in a
//! headless window with a silent engine.

mod agent;
mod automation;
mod editors;
mod eq;
mod mix;
mod modulation;
mod pages;
mod piano_roll;
mod regions;
#[cfg(target_os = "macos")]
mod shots;
mod sounds;
mod steps;
mod timeline;
mod tracks;

use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use gpui::{
    Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point, TestAppContext,
    VisualTestContext, point, px,
};
use pina_engine::{
    audio::player::Player,
    plugin::scan::PluginInfo,
    song::project::{AuxSend, Content, PPQ, Shape},
    song::session::Session,
};

use crate::{
    editors::pianoroll::KEY_H,
    mix::strip::Channel,
    ui::theme::{CONTROL_BAR_H, LANE_H, MARKERS_H, RULER_H, STATUS_H},
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
    let books = std::env::temp_dir().join(format!("pina-test-books-{}", std::process::id()));
    unsafe { std::env::set_var("PINA_PLAYBOOKS", &books) };
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

/// Waits for the engine thread to catch up with something, drawing frames meanwhile. Room for a
/// slow machine: on Windows' coarser timer, playing runs behind real time.
fn until(cx: &mut VisualTestContext, ws: &Entity<Workspace>, f: impl Fn(&Workspace) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
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
            + crate::arrange::automation::to_y(param, value, ws.auto_h());
        point(x, px(y))
    })
}

/// A copy of the demo in a folder of its own, so what's added to it lands there.
fn demo_copy(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pina-test-{name}-{}", std::process::id()));
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
fn little_library(dir: &Path) -> pina_engine::samples::library::Library {
    use pina_engine::samples::{
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
