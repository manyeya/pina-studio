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
    // The filter's point pressed (the filter comes on, a low pass), and a loop.
    click_on(
        cx,
        "sampler-filter-curve",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(cx, "loop-Loop", Modifiers::none(), MouseButton::Left);
    let s = sampler(cx);
    assert_eq!(
        (s.voice.filter, s.zones[0].loop_mode),
        (FilterMode::LowPass, LoopMode::Forward)
    );
    // The amp envelope's attack dragged out from its point; Grains, frozen; a route.
    let env = cx.debug_bounds("sampler-env-Amp").unwrap();
    let from = point(env.origin.x + px(7.), env.origin.y + px(7.));
    drag(cx, from, from + point(env.size.width * 0.2, px(0.)));
    assert!(
        sampler(cx).voice.attack_ms > 10.0,
        "{}",
        sampler(cx).voice.attack_ms
    );
    click_on(
        cx,
        "sampler-mode-grains",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(cx, "sampler-freeze", Modifiers::none(), MouseButton::Left);
    click_on(
        cx,
        "sampler-tab-matrix",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(cx, "route-add", Modifiers::none(), MouseButton::Left);
    let v = sampler(cx).voice;
    assert_eq!(
        (v.mode, v.freeze, v.matrix.iter().count()),
        (pina_engine::song::project::SampleMode::Grains, true, 1)
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

#[gpui::test]
fn the_synth_has_its_own_window_and_everything_in_it_works(cx: &mut TestAppContext) {
    use crate::{editors::poly::PolyPage, mix::strip::Channel, ui::menu::MenuAction};
    use pina_engine::song::project::{
        EnvField, FilterField, Instrument, LfoWave, OscField, Param, PolyDest, PolyFilter,
        PolySource, Reverb, Track, Unit, Warp, env_param, filter_param, osc_param, poly_patch,
    };
    let (ws, cx) = open(cx);
    let t = ws.update(cx, |ws, cx| {
        ws.session.project.tracks.push(Track {
            name: "Synth".into(),
            instrument: poly_patch("Warm Pad").map(|p| Instrument::Poly(Box::new(p))),
            more: vec![Unit::Reverb(Reverb::default())],
            ..Default::default()
        });
        ws.session.project.assign_ids();
        let t = ws.session.project.tracks.len() - 1;
        ws.select_track(t);
        cx.notify();
        t
    });
    cx.run_until_parked();
    // Clicking its slot on the channel opens it, not the list of plugins.
    let slot: &'static str = Box::leak(format!("slot-Track({t})-Instrument").into_boxed_str());
    click_on(cx, slot, Modifiers::none(), MouseButton::Left);
    assert!(
        ws.read_with(cx, |ws, _| ws.poly_view.is_some()),
        "the window opens"
    );
    assert!(
        ws.read_with(cx, |ws, _| ws.plugin_page.mini.is_none()),
        "not the picker"
    );
    assert!(
        ws.read_with(cx, |ws, _| ws.typing.is_some()),
        "it plays from the computer's keys"
    );
    let poly = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.poly(t).unwrap().clone());
    // Selectors outlive the test's lookups: their names are made as it goes.
    let named = |sel: &str| -> &'static str { Box::leak(sel.to_string().into_boxed_str()) };
    let bounds = |cx: &mut VisualTestContext, sel: &str| {
        let sel = named(sel);
        cx.debug_bounds(sel).unwrap_or_else(|| panic!("{sel}"))
    };
    let pull = |cx: &mut VisualTestContext, sel: &str, by: (f32, f32)| {
        let at = bounds(cx, sel).center();
        drag(cx, at, at + point(px(by.0), px(by.1)));
    };
    let click = |cx: &mut VisualTestContext, sel: &str| {
        click_on(cx, named(sel), Modifiers::none(), MouseButton::Left);
    };
    assert!(cx.debug_bounds("poly-wave-0").is_some());

    // A long slider, a knob, the filters' curve, the envelope's picture.
    let (position, detune, cutoff, sustain) = (
        osc_param(0, OscField::Position),
        osc_param(0, OscField::Detune),
        filter_param(0, FilterField::Cutoff),
        env_param(0, EnvField::Sustain),
    );
    let was = poly(cx);
    pull(cx, "poly-slider-osc1_position", (40.0, 0.0));
    pull(cx, "poly-knob-osc1_detune", (0.0, -30.0));
    pull(cx, "poly-filter-curve", (100.0, 0.0));
    click(cx, "poly-source-env(0)");
    pull(cx, "poly-env-plot", (0.0, 30.0));
    let now = poly(cx);
    assert!(now.params[position] > was.params[position]);
    assert!(now.params[detune] > was.params[detune]);
    assert!(now.params[cutoff] > was.params[cutoff]);
    assert!(now.params[sustain] < was.params[sustain]);

    // Choices from their menus, a patch, the filter's model and a warp; the routing in a row.
    ws.update(cx, |ws, cx| {
        use crate::editors::poly::PolyOp;
        ws.choose(
            MenuAction::Poly(t, PolyOp::Filter(1, PolyFilter::Diode)),
            cx,
        );
        ws.choose(MenuAction::Poly(t, PolyOp::Warp(0, 1, Warp::Sync)), cx);
    });
    assert_eq!(poly(cx).filters[1], PolyFilter::Diode);
    assert_eq!(poly(cx).warps[0][1], Warp::Sync);
    click(cx, "poly-routing-parallel");
    assert_eq!(
        poly(cx).routing,
        pina_engine::song::project::FilterRouting::Parallel
    );
    // A table picked from its pictures; the sub opened and given a saw.
    click(cx, "poly-table-0");
    click(cx, "poly-table-pick-Growl");
    assert_eq!(poly(cx).tables[0], "Growl");
    assert!(ws.read_with(cx, |ws, _| ws.poly_view.as_ref().unwrap().tables.is_none()));
    click(cx, "poly-pick-3");
    click(cx, "poly-sub-saw");
    assert_eq!(poly(cx).sub_wave, 2);
    click(cx, "poly-pick-0");

    // An envelope's chip dropped on a knob: a route, and the knob shows it. Once only.
    let routes = poly(cx).routes.len();
    let key = filter_param(0, FilterField::KeyTrack);
    let knob = bounds(cx, "poly-knob-filter1_key_track").center();
    let chip = bounds(cx, "poly-source-env(2)").center();
    drag(cx, chip, knob);
    drag(cx, chip, knob);
    let now = poly(cx);
    assert_eq!(now.routes.len(), routes + 1);
    let r = now.routes.last().unwrap();
    assert_eq!(r.source, PolySource::Env(2));
    assert_eq!(r.dest, PolyDest::Param(key as u16));
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.mod_rings(t, pina_engine::dsp::modulate::ModTarget::Poly(key as u16))
                .len(),
            1
        );
        assert_eq!(ws.poly_view.as_ref().unwrap().source, PolySource::Env(2));
        assert!(
            ws.poly_view.as_ref().unwrap().cord.is_none(),
            "the cord goes"
        );
    });
    // A macro's name is a handle too.
    let spread = filter_param(0, FilterField::Spread);
    let knob = bounds(cx, "poly-knob-filter1_spread").center();
    let name = bounds(cx, "poly-macro-name-0").center();
    drag(cx, name, knob);
    let r = *poly(cx).routes.last().unwrap();
    assert_eq!(
        (r.source, r.dest),
        (PolySource::Macro(0), PolyDest::Param(spread as u16))
    );

    // Its keys play the synth while held, its bend wheel springs back.
    let keys = bounds(cx, "poly-keys");
    let low = keys.origin + point(px(4.), keys.size.height - px(6.));
    cx.simulate_mouse_down(low, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.poly_view.as_ref().unwrap().held.clone()),
        vec![36]
    );
    cx.simulate_mouse_move(
        low + point(px(30.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        low + point(px(30.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(ws.read_with(cx, |ws, _| ws.poly_view.as_ref().unwrap().held.is_empty()));
    let bend = bounds(cx, "poly-bend");
    cx.simulate_mouse_down(bend.center(), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        bend.center() - point(px(0.), px(20.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(ws.read_with(cx, |ws, _| ws.poly_view.as_ref().unwrap().bend > 0.2));
    cx.simulate_mouse_up(bend.center(), MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.poly_view.as_ref().unwrap().bend),
        0.0
    );

    // An LFO's picture drawn on: its wave becomes points, and it's where the pointer went.
    click(cx, "poly-source-lfo(1)");
    let plot = bounds(cx, "poly-lfo-plot");
    drag(
        cx,
        plot.center(),
        plot.center() - point(px(0.), plot.size.height * 0.4),
    );
    let LfoWave::Drawn(points) = poly(cx).lfos[1].wave.clone() else {
        panic!("{:?}", poly(cx).lfos[1].wave);
    };
    assert!(points[32].1 > 0.8 || points[31].1 > 0.8, "{points:?}");

    // The sounds: narrowed to a part and by typing, liked, picked.
    click(cx, "poly-patch");
    click(cx, "poly-part-bass");
    assert!(cx.debug_bounds("poly-sound-Reese").is_some());
    assert!(cx.debug_bounds("poly-sound-Supersaw").is_none());
    click(cx, "poly-part-all");
    let search = bounds(cx, "poly-sound-search").center();
    cx.simulate_click(search, Modifiers::none());
    cx.simulate_input("acid");
    cx.run_until_parked();
    assert!(cx.debug_bounds("poly-sound-Reese").is_none());
    click(cx, "poly-like-Acid");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.settings.liked_sounds.clone()),
        vec!["Acid".to_string()]
    );
    click(cx, "poly-sound-Acid");
    assert_eq!(poly(cx).patch, "Acid");
    assert_eq!(poly(cx).macros[0], "Bright");
    ws.update(cx, |ws, cx| ws.undo(cx));
    assert_eq!(poly(cx).patch, "Warm Pad");
    click(cx, "poly-sounds-close");

    // The matrix: a route's depth dragged, then taken off.
    ws.update(cx, |ws, cx| {
        ws.poly_view.as_mut().unwrap().page = PolyPage::Matrix;
        cx.notify();
    });
    cx.run_until_parked();
    // Narrowed to the envelopes' routes, the one dropped above is in sight.
    click(cx, "poly-routes-envelopes");
    let last = poly(cx).routes.len() - 1;
    assert_eq!(poly(cx).routes[last].source, PolySource::Macro(0));
    assert!(
        cx.debug_bounds(named(&format!("poly-route-depth-{last}")))
            .is_none()
    );
    let at = poly(cx)
        .routes
        .iter()
        .position(|r| r.source == PolySource::Env(2))
        .unwrap();
    let depth = poly(cx).routes[at].depth;
    pull(cx, &format!("poly-route-depth-{at}"), (40.0, 0.0));
    assert!(poly(cx).routes[at].depth > depth);
    click(cx, &format!("poly-route-remove-{at}"));
    assert_eq!(poly(cx).routes.len(), routes + 1);

    // The Effects page is the track's chain: its knob is the same knob, its switch the same.
    click(cx, "poly-page-effects");
    click(cx, "poly-effect-power-0");
    let bypassed = ws.read_with(cx, |ws, _| ws.more(Channel::Track(t))[0].bypassed());
    assert!(bypassed);

    // Every parameter's in the automation list, by its part, a macro by its name.
    ws.read_with(cx, |ws, _| {
        let items = ws.auto_params(t, 0);
        let names: Vec<&str> = items.iter().map(|i| i.0.as_str()).collect();
        for want in [
            "Oscillator B",
            "B Position",
            "Filter 2 Drive",
            "LFO 8 Rate",
            "Motion",
        ] {
            assert!(names.contains(&want), "{want}");
        }
        assert_eq!(ws.param_name(t, Param::Poly(position as u16)), "A Position");
    });
    // A patch from its menu: all of it at once, and undone with one step.
    ws.update(cx, |ws, cx| {
        use crate::editors::poly::PolyOp;
        ws.choose(MenuAction::Poly(t, PolyOp::Patch("Reese")), cx);
    });
    assert_eq!(poly(cx).patch, "Reese");
    ws.update(cx, |ws, cx| ws.undo(cx));
    assert_ne!(poly(cx).patch, "Reese");
    // A synth made before patches had macros is offered them, and keeps its sound till they move.
    ws.update(cx, |ws, cx| {
        let p = ws.poly_mut(t).unwrap();
        p.routes
            .retain(|r| !matches!(r.source, PolySource::Macro(_)));
        p.macros = Default::default();
        ws.poly_view.as_mut().unwrap().page = PolyPage::Synth;
        cx.notify();
    });
    cx.run_until_parked();
    click(cx, "poly-give-macros");
    assert_eq!(poly(cx).macros[1], "Bite");
    assert!(
        poly(cx)
            .routes
            .iter()
            .any(|r| r.source == PolySource::Macro(7))
    );
    assert!(cx.debug_bounds("poly-give-macros").is_none());
    // Its corner makes it smaller or bigger, never under what its columns need; its close
    // button and the patch's arrows are drawn where they can be seen.
    let was = ws.read_with(cx, |ws, _| ws.poly_size());
    let grip = bounds(cx, "poly-resize").center();
    drag(cx, grip, grip - point(px(120.), px(60.)));
    let now = ws.read_with(cx, |ws, _| ws.poly_size());
    assert!(now.0 < was.0 && now.1 < was.1, "{was:?} {now:?}");
    let grip = bounds(cx, "poly-resize").center();
    drag(cx, grip, grip - point(px(2000.), px(2000.)));
    assert_eq!(ws.read_with(cx, |ws, _| ws.poly_size()), (1180.0, 720.0));
    for sel in ["poly-close", "poly-patch-back", "poly-patch-next"] {
        let b = bounds(cx, sel);
        assert!(b.size.width > px(20.) && b.size.height > px(20.), "{sel}");
    }
    // Closing it closes only it, and typing goes back off.
    click(cx, "poly-close");
    assert!(ws.read_with(cx, |ws, _| ws.poly_view.is_none()));
    assert!(ws.read_with(cx, |ws, _| ws.typing.is_none()));
}

#[gpui::test]
fn the_synth_opens_as_its_put_on_a_track_and_from_the_mixers_card(cx: &mut TestAppContext) {
    use crate::{sounds::catalog::Target as Place, ui::menu::MenuAction};
    use pina_engine::song::project::{Instrument, poly_patch};
    let (ws, cx) = open(cx);
    // Picked for a track from its slot's menu.
    ws.update(cx, |ws, cx| {
        let instrument = Instrument::Poly(Box::new(poly_patch("Init").unwrap()));
        ws.choose(
            MenuAction::Instrument {
                track: 0,
                instrument,
            },
            cx,
        );
    });
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.poly_view.as_ref().map(|v| v.track)),
        Some(0)
    );
    // Picked from the Browser, on a track of its own.
    ws.update(cx, |ws, cx| {
        ws.poly_view = None;
        let synth = ws
            .catalog
            .devices
            .iter()
            .position(|d| d.key == "own:poly")
            .unwrap();
        ws.use_device(synth, Place::NewTrack, None, cx);
    });
    let (view, track) = ws.read_with(cx, |ws, _| {
        (ws.poly_view.as_ref().map(|v| v.track), ws.track)
    });
    assert_eq!(view, Some(track));
    assert!(ws.read_with(cx, |ws, _| ws.poly(track).is_some()));
}

#[gpui::test]
fn the_rolls_bar_keeps_whats_used_less_in_more(cx: &mut TestAppContext) {
    use crate::editors::pianoroll::Stamp;
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| ws.show_bottom(Bottom::PianoRoll, cx));
    cx.run_until_parked();
    // What's reached for all the time stays out; More doesn't get pushed off the end.
    for always in ["roll-scale", "roll-in-key", "roll-quantize", "editor-more"] {
        assert!(cx.debug_bounds(always).is_some(), "{always}");
    }
    for gone in ["roll-stamp", "roll-ghost", "roll-colours", "roll-fit"] {
        assert!(cx.debug_bounds(gone).is_none(), "{gone}");
    }
    let (more, close) = (
        cx.debug_bounds("editor-more").unwrap(),
        cx.debug_bounds("close-editor").unwrap(),
    );
    assert!(more.right() <= close.origin.x, "{more:?} {close:?}");
    let more_menu = |cx: &mut VisualTestContext| {
        click_on(cx, "editor-more", Modifiers::none(), MouseButton::Left);
        cx.run_until_parked();
    };

    // A switch, ticked once it's on.
    more_menu(cx);
    pick(cx, &ws, "Colour by velocity");
    assert!(ws.read_with(cx, |ws, _| ws.roll.by_velocity));
    more_menu(cx);
    let ticked = ws.read_with(cx, |ws, _| {
        let menu = ws.menu.as_ref().unwrap();
        menu.items.iter().any(|(l, _)| l == "✓ Colour by velocity")
    });
    assert!(ticked);
    cx.simulate_keystrokes("escape");

    // Chords and ghost notes open their own lists, and More says what they're on.
    more_menu(cx);
    pick(cx, &ws, "Chords");
    let stamp = ws.read_with(cx, |ws, _| {
        let name = Stamp::ALL[1].name();
        assert!(ws.menu.is_some(), "the list of chords");
        name
    });
    pick(cx, &ws, stamp);
    assert_eq!(ws.read_with(cx, |ws, _| ws.roll.stamp), Stamp::ALL[1]);
    more_menu(cx);
    let said = ws.read_with(cx, |ws, _| ws.menu.as_ref().unwrap().items[0].0.clone());
    assert!(said.contains(stamp), "{said}");
    pick(cx, &ws, "Ghost notes");
    let track = ws.read_with(cx, |ws, _| ws.menu.as_ref().unwrap().items[1].0.clone());
    pick(cx, &ws, &track);
    assert!(ws.read_with(cx, |ws, _| ws.roll.ghost.is_some()));

    more_menu(cx);
    pick(cx, &ws, "Fit notes");
    assert!(ws.read_with(cx, |ws, _| ws.menu.is_none()));
}

#[gpui::test]
fn the_samplers_mono_and_legato_are_in_more(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.session.project.tracks[0].instrument = Some(
            pina_engine::song::project::Instrument::Sampler(Default::default()),
        );
        ws.select_track(0);
        ws.show_bottom(Bottom::Sampler, cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("sampler-mono").is_none());
    let voice = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.sampler_of(0).unwrap().voice);
    assert!(!voice(cx).mono);
    click_on(cx, "editor-more", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Legato");
    // Legato plays one voice, so it brings Mono with it.
    assert!(voice(cx).legato && voice(cx).mono);
    click_on(cx, "editor-more", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Mono");
    assert!(!voice(cx).mono);
}
