//! The Channel EQ: its window, bands, dynamics and the instance list.

use super::*;

#[gpui::test]
fn the_eq_window_sets_dynamics_character_output_and_records_automation(cx: &mut TestAppContext) {
    use pina_engine::song::project::{
        AutoMode, Bands, Character, Eq, EqBand, EqField, Param, Processing,
    };
    let (ws, cx) = open(cx);
    cx.simulate_resize(gpui::size(px(1500.), px(1000.)));
    ws.update(cx, |ws, cx| {
        ws.session.project.tracks[0].eq = Some(Eq {
            bands: Bands::from_slice(&[EqBand::new(Shape::Bell, 1_000.0, 3.0)]),
            ..Eq::default()
        });
        ws.changed(cx);
        ws.open_eq(Channel::Track(0), cx);
    });
    cx.run_until_parked();
    let eq =
        |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.session.project.tracks[0].eq.unwrap());
    click_on(cx, "eq-band-0", Modifiers::none(), MouseButton::Left);
    // Gain-Q; dynamic, its own settings opened: free between two frequencies, from a sidechain.
    click_on(cx, "eq-gain-q", Modifiers::none(), MouseButton::Left);
    assert!(eq(cx).bands[0].gain_q);
    click_on(cx, "eq-dynamic", Modifiers::none(), MouseButton::Left);
    click_on(cx, "eq-dyn-expand", Modifiers::none(), MouseButton::Left);
    assert!(cx.debug_bounds("eq-dynamics").is_some());
    let d = eq(cx).bands[0].dynamic.unwrap();
    assert!(
        d.threshold_db.is_none() && d.attack_ms.is_none(),
        "its own, to start: {d:?}"
    );
    let thresh = cx.debug_bounds("eq-threshold").unwrap().center();
    drag(cx, thresh, thresh + point(px(0.), px(20.)));
    assert!(eq(cx).bands[0].dynamic.unwrap().threshold_db.is_some());
    click_on(
        cx,
        "eq-auto-threshold",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert!(eq(cx).bands[0].dynamic.unwrap().threshold_db.is_none());
    click_on(cx, "eq-trigger-free", Modifiers::none(), MouseButton::Left);
    assert_eq!(
        eq(cx).bands[0].dynamic.unwrap().trigger,
        Some([500.0, 2_000.0])
    );
    click_on(cx, "eq-sidechain", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Drums");
    let drums = ws.read_with(cx, |ws, _| ws.session.project.tracks[2].id);
    assert_eq!(eq(cx).sidechain, Some(drums));
    assert!(eq(cx).bands[0].dynamic.unwrap().external);
    // Spectral: its tilt off, its density dragged up.
    click_on(cx, "eq-spectral", Modifiers::none(), MouseButton::Left);
    click_on(cx, "eq-spectral-tilt", Modifiers::none(), MouseButton::Left);
    let density = cx.debug_bounds("eq-knob-density").unwrap().center();
    drag(cx, density, density - point(px(0.), px(40.)));
    let d = eq(cx).bands[0].dynamic.unwrap();
    assert!(d.spectral && !d.tilt && d.density > 0.6, "{d:?}");
    // The character, and the output's pan, phase and auto gain.
    click_on(cx, "eq-character", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Warm");
    assert_eq!(eq(cx).character, Character::Warm);
    // Linear phase, high resolution: its latency shown, and no analog character to pick.
    assert!(cx.debug_bounds("eq-latency").is_some(), "spectral's window");
    click_on(cx, "eq-processing", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Linear Phase");
    click_on(cx, "eq-processing", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "High");
    let e = eq(cx);
    assert_eq!((e.processing, e.resolution), (Processing::Linear, 2));
    assert!(cx.debug_bounds("eq-character").is_none());
    click_on(cx, "eq-processing", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Natural Phase");
    assert_eq!(eq(cx).processing, Processing::Natural);
    click_on(cx, "eq-output", Modifiers::none(), MouseButton::Left);
    click_on(cx, "eq-pan-ms", Modifiers::none(), MouseButton::Left);
    click_on(cx, "eq-invert", Modifiers::none(), MouseButton::Left);
    click_on(cx, "eq-auto-gain", Modifiers::none(), MouseButton::Left);
    let pan = cx.debug_bounds("eq-knob-pan").unwrap().center();
    drag(cx, pan, pan - point(px(0.), px(40.)));
    let e = eq(cx);
    assert!(e.pan_ms && e.invert && e.auto_gain && e.pan > 0.1, "{e:?}");
    // Its bands' settings are in the automation lanes' list.
    let items = ws.read_with(cx, |ws, _| ws.auto_params(0, 0));
    let labels: Vec<&String> = items.iter().map(|(l, _)| l).collect();
    assert!(
        labels.iter().any(|l| l.contains("EQ 1 Bell: Gain")),
        "{labels:?}"
    );
    // Latch, playing: a band dragged is recorded.
    ws.update(cx, |ws, cx| {
        ws.session.project.tracks[0].automation.mode = AutoMode::Latch;
        ws.changed(cx);
        ws.player.play();
    });
    until(cx, &ws, |ws| ws.playhead() > PPQ / 2);
    let dot = cx.debug_bounds("eq-band-0").unwrap().center();
    cx.simulate_mouse_down(dot, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        dot - point(px(0.), px(60.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    let held = ws.read_with(cx, |ws, _| ws.playhead());
    until(cx, &ws, |ws| ws.playhead() > held + PPQ / 4);
    cx.simulate_mouse_up(
        dot - point(px(0.), px(60.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    ws.update(cx, |ws, cx| ws.stop(cx));
    until(cx, &ws, |ws| ws.writing.is_empty() && !ws.was_playing);
    let points = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[0]
            .automation
            .points(Param::Eq(0, EqField::Gain))
            .to_vec()
    });
    assert!(!points.is_empty(), "nothing recorded");
    assert!(points.iter().any(|p| p.value > 5.0), "{points:?}");
}

#[gpui::test]
fn the_instance_list_edits_every_eq_in_the_song(cx: &mut TestAppContext) {
    use pina_engine::song::project::{Bands, Eq, EqBand};
    let (ws, cx) = open(cx);
    cx.simulate_resize(gpui::size(px(1500.), px(1000.)));
    ws.update(cx, |ws, cx| {
        for t in 0..3 {
            ws.session.project.tracks[t].eq = Some(Eq {
                bands: Bands::from_slice(&[EqBand::new(Shape::Bell, 1_000.0, 3.0)]),
                ..Eq::default()
            });
        }
        ws.changed(cx);
        ws.open_eq(Channel::Track(0), cx);
    });
    cx.run_until_parked();
    let eq = |cx: &VisualTestContext, t: usize| {
        ws.read_with(cx, |ws, _| ws.session.project.tracks[t].eq.unwrap())
    };
    click_on(cx, "eq-instances", Modifiers::none(), MouseButton::Left);
    assert!(cx.debug_bounds("eq-list").is_some());
    // Zoomed to edit: the second track's band dragged up from its own row; the first's alone.
    click_on(
        cx,
        "eq-instance-zoom-2",
        Modifiers::none(),
        MouseButton::Left,
    );
    let row = cx.debug_bounds("eq-instance-display-1").unwrap();
    let (w, h) = (f32::from(row.size.width), f32::from(row.size.height));
    let dot = point(
        row.origin.x + px(w * (50f32).ln() / (1000f32).ln()),
        row.origin.y + px(h / 2.0 - 3.0 / 18.0 * (h / 2.0 - 18.0)),
    );
    drag(cx, dot, dot - point(px(0.), px(30.)));
    assert!(eq(cx, 1).bands[0].gain_db > 6.0, "{:?}", eq(cx, 1).bands[0]);
    assert_eq!(eq(cx, 0).bands[0].gain_db, 3.0);
    // A click where there's no band makes one there; its bypass, from its row.
    let empty = point(row.origin.x + px(w * 0.25), row.origin.y + px(h * 0.3));
    let third = cx.debug_bounds("eq-instance-display-2").unwrap();
    let empty3 = empty + point(px(0.), third.origin.y - row.origin.y);
    drag(cx, empty3, empty3);
    assert_eq!(eq(cx, 2).bands.len(), 2);
    click_on(
        cx,
        "eq-instance-bypass-1",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert!(eq(cx, 1).bypass);
    cx.simulate_keystrokes("cmd-z");
    assert!(!eq(cx, 1).bypass);
    // Pinned, only those; filtered by name, only those.
    click_on(
        cx,
        "eq-instance-pin-1",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(
        cx,
        "eq-instance-pinned",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert!(cx.debug_bounds("eq-instance-0").is_some());
    assert!(cx.debug_bounds("eq-instance-1").is_none());
    click_on(
        cx,
        "eq-instance-pinned",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(
        cx,
        "eq-instance-filter",
        Modifiers::none(),
        MouseButton::Left,
    );
    cx.simulate_input("dru");
    cx.run_until_parked();
    assert!(cx.debug_bounds("eq-instance-1").is_none());
    // Opened: the window on it, the list gone.
    click_on(
        cx,
        "eq-instance-open-0",
        Modifiers::none(),
        MouseButton::Left,
    );
    let (ch, list) = ws.read_with(cx, |ws, _| {
        let v = ws.eq_view.as_ref().unwrap();
        (v.ch, v.list)
    });
    assert_eq!((ch, list), (Channel::Track(2), false));
}

#[gpui::test]
fn smart_controls_turn_the_channel_eq_on_a_built_in_instrument(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    // Keys plays the built-in synth, so its Smart Controls are its channel's: Low Cut first.
    let knob = cx.debug_bounds("smart-0").unwrap().center();
    drag(cx, knob, knob - point(px(0.), px(60.)));
    let cut = ws.read_with(cx, |ws, _| {
        crate::mix::eq::low_cut(&ws.session.project.tracks[0].eq.unwrap())
    });
    assert!(cut > 40.0, "{cut}");
}

#[gpui::test]
fn the_band_panel_never_hides_the_band_it_is_for(cx: &mut TestAppContext) {
    use pina_engine::song::project::{Bands, Eq, EqBand};
    let (ws, cx) = open(cx);
    cx.simulate_resize(gpui::size(px(1500.), px(1000.)));
    // A band high up: the panel at the bottom, under it. Low down, where the panel would
    // hide it: the panel at the top, over it.
    for (gain, below) in [(9.0, true), (-11.0, false)] {
        ws.update(cx, |ws, cx| {
            ws.session.project.tracks[0].eq = Some(Eq {
                bands: Bands::from_slice(&[EqBand::new(Shape::Bell, 1_000.0, gain)]),
                ..Eq::default()
            });
            ws.changed(cx);
            ws.open_eq(Channel::Track(0), cx);
        });
        cx.run_until_parked();
        click_on(cx, "eq-band-0", Modifiers::none(), MouseButton::Left);
        let dot = cx.debug_bounds("eq-band-0").unwrap();
        let panel = cx.debug_bounds("eq-panel").unwrap();
        match below {
            true => assert!(panel.origin.y > dot.origin.y + dot.size.height),
            false => assert!(panel.origin.y + panel.size.height < dot.origin.y),
        }
    }
}

#[gpui::test]
fn the_eq_window_adds_shapes_moves_and_takes_out_bands(cx: &mut TestAppContext) {
    use pina_engine::song::project::Placement;
    let (ws, cx) = open(cx);
    cx.simulate_resize(gpui::size(px(1500.), px(1000.)));
    ws.update(cx, |ws, cx| ws.open_eq(Channel::Track(0), cx));
    cx.run_until_parked();
    let eq =
        |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.session.project.tracks[0].eq.unwrap());
    let picked = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| ws.eq_view.as_ref().unwrap().picked.len())
    };
    let display = cx.debug_bounds("eq-display").unwrap();
    let at = |x: f32, y: f32| {
        point(
            display.origin.x + display.size.width * x,
            display.origin.y + display.size.height * y,
        )
    };
    // A click with nothing picked makes a band: a cut at the far left. Then, one picked, a
    // click lets it go, and a double-click makes another: a cut at the right, a bell between.
    cx.simulate_click(at(0.02, 0.5), Modifiers::none());
    assert_eq!(eq(cx).bands.len(), 1);
    cx.simulate_click(at(0.5, 0.15), Modifiers::none());
    assert_eq!((eq(cx).bands.len(), picked(cx)), (1, 0));
    double_click(cx, at(0.98, 0.5));
    double_click(cx, at(0.6, 0.3));
    let shapes: Vec<Shape> = eq(cx).bands.iter().map(|b| b.shape).collect();
    assert_eq!(shapes, [Shape::LowCut, Shape::HighCut, Shape::Bell]);
    let bell = eq(cx).bands[2];
    assert!(
        bell.gain_db > 3.0 && (500.0..4_000.0).contains(&bell.freq),
        "{bell:?}"
    );
    // Picked, its panel shows: a low shelf now, 24 dB an octave, on the middle only.
    assert!(cx.debug_bounds("eq-panel").is_some());
    click_on(
        cx,
        "eq-shape-low-shelf",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(cx, "eq-slope", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "24 dB/oct");
    click_on(cx, "eq-place-mid", Modifiers::none(), MouseButton::Left);
    let b = eq(cx).bands[2];
    assert_eq!(
        (b.shape, b.slope, b.placement),
        (Shape::LowShelf, 24.0, Placement::Mid)
    );
    // Its slope dragged: any between; the wheel a step, ⇧ a dB.
    let slope = cx.debug_bounds("eq-slope").unwrap().center();
    drag(cx, slope, slope - point(px(0.), px(15.)));
    let s = eq(cx).bands[2].slope;
    assert!(s > 24.0 && s.fract() != 0.0, "{s}");
    let wheel = |cx: &mut VisualTestContext, shift: bool| {
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: slope,
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(16.))),
            modifiers: Modifiers {
                shift,
                ..Modifiers::none()
            },
            ..Default::default()
        });
        cx.run_until_parked();
    };
    wheel(cx, false);
    assert_eq!(eq(cx).bands[2].slope, 36.0);
    wheel(cx, true);
    assert_eq!(eq(cx).bands[2].slope, 37.0);
    // Dynamic: the ring round its gain knob is its range.
    click_on(cx, "eq-dynamic", Modifiers::none(), MouseButton::Left);
    let ring = cx.debug_bounds("eq-knob-range").unwrap();
    let edge = point(ring.center().x, ring.origin.y + px(3.));
    drag(cx, edge, edge + point(px(0.), px(40.)));
    let d = eq(cx).bands[2].dynamic.expect("dynamic");
    assert!(d.range_db < -8.0, "{d:?}");
    // The frequency knob: dragged up, ⌘-clicked back, typed.
    let freq = cx.debug_bounds("eq-knob-freq").unwrap().center();
    drag(cx, freq, freq - point(px(0.), px(30.)));
    assert!(eq(cx).bands[2].freq > bell.freq * 1.5);
    click_on(cx, "eq-knob-freq", Modifiers::command(), MouseButton::Left);
    assert_eq!(eq(cx).bands[2].freq, 1_000.0);
    // (The panel follows its band about, so the knob's moved.)
    let freq = cx.debug_bounds("eq-knob-freq").unwrap().center();
    double_click(cx, freq);
    cx.simulate_input("2k");
    cx.simulate_keystrokes("enter");
    assert_eq!(eq(cx).bands[2].freq, 2_000.0);
    // Solo, held: heard while it's held.
    let solo = cx.debug_bounds("eq-solo").unwrap().center();
    cx.simulate_mouse_down(solo, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let held = ws.read_with(cx, |ws, _| ws.eq_view.as_ref().unwrap().solo.map(|s| s.id));
    assert_eq!(held, Some(eq(cx).bands[2].id));
    cx.simulate_mouse_up(solo, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(ws.read_with(cx, |ws, _| ws.eq_view.as_ref().unwrap().solo.is_none()));
    // ⌥-click switches a band off; ⌘⌥-click changes its shape.
    click_on(cx, "eq-band-0", Modifiers::alt(), MouseButton::Left);
    assert!(eq(cx).bands[0].off);
    let cmd_alt = Modifiers {
        platform: true,
        alt: true,
        ..Modifiers::none()
    };
    click_on(cx, "eq-band-1", cmd_alt, MouseButton::Left);
    assert_ne!(eq(cx).bands[1].shape, Shape::HighCut);
    // A band's menu: a notch.
    click_on(cx, "eq-band-1", Modifiers::none(), MouseButton::Right);
    pick(cx, &ws, "Notch");
    assert_eq!(eq(cx).bands[1].shape, Shape::Notch);
    // ⌘-click picks a second band; dragging one moves both by the same ratio.
    click_on(cx, "eq-band-0", Modifiers::none(), MouseButton::Left);
    click_on(cx, "eq-band-2", Modifiers::command(), MouseButton::Left);
    assert_eq!(picked(cx), 2);
    let before: Vec<f32> = eq(cx).bands.iter().map(|b| b.freq).collect();
    let dot = cx.debug_bounds("eq-band-2").unwrap().center();
    drag(cx, dot, dot + point(px(60.), px(0.)));
    let after: Vec<f32> = eq(cx).bands.iter().map(|b| b.freq).collect();
    let ratio = after[2] / before[2];
    assert!(
        ratio > 1.2 && (after[0] / before[0] / ratio - 1.0).abs() < 0.05,
        "{before:?} {after:?}"
    );
    assert_eq!(after[1], before[1]);
    // ⌘C, ⌘V: copies of both; ⌘A picks all; Escape none; Delete takes out those picked.
    cx.simulate_keystrokes("cmd-c cmd-v");
    assert_eq!(eq(cx).bands.len(), 5);
    cx.simulate_keystrokes("cmd-a");
    assert_eq!(picked(cx), 5);
    cx.simulate_keystrokes("escape");
    assert_eq!(picked(cx), 0);
    click_on(cx, "eq-band-4", Modifiers::none(), MouseButton::Left);
    cx.simulate_keystrokes("backspace");
    assert_eq!(eq(cx).bands.len(), 4);
    // A box dragged across the background picks the bands in it.
    drag(cx, at(0.001, 0.01), at(0.999, 0.99));
    assert_eq!(picked(cx), 4);
    // Split: a band on both sides becomes one for each.
    // (Pasted bands sit on their originals, so the one clicked may be either: go by its id.)
    click_on(cx, "eq-band-1", Modifiers::none(), MouseButton::Left);
    let id = ws.read_with(cx, |ws, _| ws.eq_view.as_ref().unwrap().picked[0]);
    let was = eq(cx).bands[eq(cx).bands.find(id).unwrap()].placement;
    click_on(cx, "eq-split", Modifiers::none(), MouseButton::Left);
    let i = eq(cx).bands.find(id).unwrap();
    let places = (eq(cx).bands[i].placement, eq(cx).bands[i + 1].placement);
    let want = match was {
        Placement::Mid | Placement::Side => (Placement::Mid, Placement::Side),
        _ => (Placement::Left, Placement::Right),
    };
    assert_eq!(places, want);
    // The analyzer's settings, and the display's range.
    click_on(cx, "eq-analyzer", Modifiers::none(), MouseButton::Left);
    click_on(cx, "eq-pre", Modifiers::none(), MouseButton::Left);
    click_on(
        cx,
        "eq-resolution-maximum",
        Modifiers::none(),
        MouseButton::Left,
    );
    let analysis = ws.read_with(cx, |ws, _| ws.eq_view.as_ref().unwrap().analysis);
    assert!(!analysis.pre && analysis.post && analysis.resolution == 3);
    click_on(cx, "eq-range", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "± 30 dB");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.eq_view.as_ref().unwrap().range),
        30
    );
    // The output, turned up.
    click_on(cx, "eq-output", Modifiers::none(), MouseButton::Left);
    let out = cx.debug_bounds("eq-knob-output").unwrap().center();
    drag(cx, out, out - point(px(0.), px(20.)));
    assert!(eq(cx).output_db > 1.0);
    // A and B: B starts as A; changed, A is still what it was.
    let a = eq(cx);
    click_on(cx, "eq-b", Modifiers::none(), MouseButton::Left);
    assert_eq!(eq(cx), a);
    ws.update(cx, |ws, cx| ws.eq_op(crate::mix::eq::EqOp::Clear, cx));
    assert!(eq(cx).bands.is_empty());
    click_on(cx, "eq-a", Modifiers::none(), MouseButton::Left);
    assert_eq!(eq(cx).bands.len(), a.bands.len());
    // A preset of Pina Studio's.
    click_on(cx, "eq-presets", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Telephone");
    assert_eq!(eq(cx).bands.len(), 3);
    // Moved by its title; full screen and back.
    let from = ws.read_with(cx, |ws, _| ws.eq_view.as_ref().unwrap().at);
    let title = cx.debug_bounds("eq-title").unwrap().center();
    drag(cx, title, title + point(px(-60.), px(30.)));
    let to = ws.read_with(cx, |ws, _| ws.eq_view.as_ref().unwrap().at);
    assert_eq!((to.0 - from.0, to.1 - from.1), (-60.0, 30.0));
    click_on(cx, "eq-full", Modifiers::none(), MouseButton::Left);
    click_on(cx, "eq-full", Modifiers::none(), MouseButton::Left);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.eq_view.as_ref().unwrap().at),
        to
    );
    // Every edit undoes, back to the first band made.
    for _ in 0..80 {
        let e = eq(cx);
        if e.bands.len() == 1 && e.bands[0].shape == Shape::LowCut && !e.bands[0].off {
            break;
        }
        cx.simulate_keystrokes("cmd-z");
    }
    assert_eq!(eq(cx).bands.len(), 1);
    assert_eq!(eq(cx).bands[0].shape, Shape::LowCut);
}
