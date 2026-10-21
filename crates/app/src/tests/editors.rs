//! The score, the sampler, the binaural panner and the editor pane itself.

use super::*;

#[gpui::test]
fn the_binaural_panner_places_a_sound_round_the_head(cx: &mut TestAppContext) {
    use crate::editors::binaural::Camera;
    use pina_engine::song::project::{PanMode, Place};
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
    assert_ne!(orbit, crate::editors::binaural::ORBIT);
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
fn the_editor_is_resized_by_its_top_edge(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_click(spot(&ws, cx, PPQ, 0), Modifiers::none());
    cx.simulate_keystrokes("p");
    let height = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.editor_height());
    assert_eq!(height(cx), crate::ui::theme::EDITOR_H);
    // Up 80: taller by 80, the lanes shorter.
    let lanes = ws.read_with(cx, |ws, _| ws.lanes_bounds.get().size.height);
    let edge = cx.debug_bounds("editor-edge").unwrap().center();
    drag(cx, edge, edge - point(px(0.), px(80.)));
    assert_eq!(height(cx), crate::ui::theme::EDITOR_H + 80.0);
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
    assert_eq!(height(cx), crate::ui::theme::EDITOR_MIN_H);
    // Double-click: back to its usual height. None of it is an edit.
    let edge = cx.debug_bounds("editor-edge").unwrap().center();
    double_click(cx, edge);
    assert_eq!(height(cx), crate::ui::theme::EDITOR_H);
    assert!(!ws.read_with(cx, |ws, _| ws.session.edited));
}

#[gpui::test]
fn the_sampler_takes_samples_slices_them_and_its_knobs_and_marks_turn(cx: &mut TestAppContext) {
    use pina_engine::song::project::{FilterMode, Instrument, LoopMode};
    // A song of the test's own, so samples are copied into it, not the demo.
    let dir = std::env::temp_dir().join(format!("pina-test-sampler-{}", std::process::id()));
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
    pina_engine::music::analysis::write_wav(&loop_file, &hits, &hits, rate).unwrap();
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
        ws.slice_zone(crate::editors::sampler::Slicing::Hits(8), cx)
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
