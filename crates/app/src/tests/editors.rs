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
    use crate::{editors::poly::PolyPage, ui::menu::MenuAction};
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

    // The sounds: the library a page of its own, narrowed to a part and by typing, liked,
    // picked and loaded.
    click(cx, "poly-patch");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.poly_view.as_ref().unwrap().page),
        PolyPage::Sounds
    );
    click(cx, "poly-lib-part-bass");
    assert!(cx.debug_bounds("poly-sound-Reese").is_some());
    assert!(cx.debug_bounds("poly-sound-Supersaw").is_none());
    click(cx, "poly-lib-part-all");
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
    click(cx, "poly-lib-load");
    assert_eq!(poly(cx).patch, "Acid");
    assert_eq!(poly(cx).macros[0], "Bright");
    ws.update(cx, |ws, cx| ws.undo(cx));
    assert_eq!(poly(cx).patch, "Warm Pad");

    // Saving: changed, the bar says so; saved as a sound of its own (the factory's stays),
    // then over it; compared with how it was saved, and taken back to it.
    use crate::editors::poly::library::Library;
    let changed = |cx: &mut VisualTestContext| cx.debug_bounds("poly-patch-changed").is_some();
    // Changed since it was loaded, by all the above (and put back after).
    assert!(changed(cx));
    let before = poly(cx);
    click(cx, "poly-sound-Acid");
    click(cx, "poly-lib-load");
    assert!(!changed(cx));
    ws.update(cx, |ws, cx| {
        ws.edit(cx, |ws| ws.poly_mut(t).unwrap().params[0] += 0.1);
    });
    cx.run_until_parked();
    assert!(changed(cx));
    click(cx, "poly-save");
    // Typed into its name: words, not notes (the computer's keys play the synth otherwise).
    let octave = ws.read_with(cx, |ws, _| ws.typing);
    cx.simulate_input("wax");
    cx.run_until_parked();
    let typed = ws.read_with(cx, |ws, cx| {
        let Some(crate::editors::poly::saving::Dialog::Save(f)) =
            ws.poly_view.as_ref().unwrap().lib.dialog.as_ref()
        else {
            panic!("Save's dialog");
        };
        f.name.read(cx).text().to_string()
    });
    assert!(typed.ends_with("wax"), "{typed}");
    assert!(
        ws.read_with(cx, |ws, _| ws.typed.is_empty()),
        "no notes played"
    );
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.typing),
        octave,
        "x didn't move the octave"
    );
    ws.update(cx, |ws, cx| {
        let v = ws.poly_view.as_mut().unwrap();
        let Some(crate::editors::poly::saving::Dialog::Save(f)) = v.lib.dialog.as_ref() else {
            panic!("Save's dialog");
        };
        f.name.update(cx, |i, cx| i.set_text("My Pad", cx));
    });
    click(cx, "poly-save-word-glassy");
    click(cx, "poly-save-part-pad");
    click(cx, "poly-save-confirm");
    assert_eq!(poly(cx).patch, "/My Pad");
    assert!(!changed(cx));
    let dir = ws.read_with(cx, |ws, _| ws.sounds_dir.clone());
    let read = Library::read(&dir);
    let mine = read.find("/My Pad").expect("on disk");
    assert_eq!(
        (mine.sound.part.as_str(), mine.sound.words.clone()),
        ("Pad", vec!["Glassy".to_string()])
    );
    assert!(read.find("Warm Pad").is_some_and(|e| e.factory()));
    ws.update(cx, |ws, cx| {
        ws.edit(cx, |ws| ws.poly_mut(t).unwrap().params[0] += 0.1);
    });
    cx.run_until_parked();
    let now = poly(cx).params[0];
    click(cx, "poly-compare");
    let heard = ws.read_with(cx, |ws, _| {
        match &ws.engine_project().tracks[t].instrument {
            Some(pina_engine::song::project::Instrument::Poly(p)) => p.params[0],
            _ => 0.0,
        }
    });
    assert!(
        (heard - (now - 0.1)).abs() < 1e-5,
        "compared: heard as saved"
    );
    click(cx, "poly-compare");
    click(cx, "poly-save");
    assert!(!changed(cx), "saved over its own, no dialog");
    assert!(
        (Library::read(&dir)
            .find("/My Pad")
            .unwrap()
            .sound
            .poly
            .params[0]
            - now)
            .abs()
            < 1e-5
    );
    ws.update(cx, |ws, cx| {
        ws.edit(cx, |ws| ws.poly_mut(t).unwrap().params[0] += 0.2);
    });
    cx.run_until_parked();
    click(cx, "poly-undo-all");
    assert!((poly(cx).params[0] - now).abs() < 1e-5);

    // A pack made for it, the sound moved in, the pack shared as one file and brought back
    // in as a pack of its own.
    ws.update(cx, |ws, cx| ws.open_pack_dialog(None, None, cx));
    ws.update(cx, |ws, cx| {
        let v = ws.poly_view.as_mut().unwrap();
        let Some(crate::editors::poly::saving::Dialog::Pack(f)) = v.lib.dialog.as_ref() else {
            panic!("the pack's dialog");
        };
        f.name.update(cx, |i, cx| i.set_text("Night Drive", cx));
    });
    click(cx, "poly-pack-confirm");
    use crate::editors::poly::sounds::SoundOp;
    ws.update(cx, |ws, cx| {
        ws.sound_op(
            t,
            SoundOp::Move("/My Pad".into(), Some("Night Drive".into())),
            cx,
        )
    });
    assert_eq!(poly(cx).patch, "Night Drive/My Pad");
    assert!(cx.debug_bounds("poly-pack-share").is_some());
    // The agent: the synth's sound saved into the pack through the open app, then a sound
    // loaded back onto the track, as Save and Load do.
    let saved = ws.update(cx, |ws, cx| {
        ws.agent_sounds(
            &serde_json::json!({ "op": "save", "track": t, "name": "Agent Pad", "part": "Pad",
                "words": ["Wide"], "pack": "Night Drive" }),
            cx,
        )
    });
    assert_eq!(saved.unwrap()["saved"], "Night Drive/Agent Pad");
    assert_eq!(poly(cx).patch, "Night Drive/Agent Pad");
    let loaded = ws.update(cx, |ws, cx| {
        ws.agent_sounds(
            &serde_json::json!({ "op": "load", "track": t, "key": "Acid" }),
            cx,
        )
    });
    assert!(loaded.is_ok(), "{loaded:?}");
    assert_eq!(poly(cx).patch, "Acid");
    ws.update(cx, |ws, cx| ws.undo(cx));
    let file = ws.update(cx, |ws, _| ws.library().export("Night Drive").unwrap());
    let path = dir.join("shared.pinapack");
    std::fs::write(&path, file).unwrap();
    ws.update(cx, |ws, cx| {
        ws.import_files(std::slice::from_ref(&path), cx)
    });
    let read = Library::read(&dir);
    assert!(read.pack("Night Drive 2").is_some_and(|p| p.imported));
    assert!(read.find("Night Drive 2/My Pad").is_some());
    let _ = std::fs::remove_dir_all(&dir);
    click(cx, "poly-page-synth");
    ws.update(cx, |ws, cx| {
        ws.edit(cx, |ws| *ws.poly_mut(t).unwrap() = before.clone());
    });
    cx.run_until_parked();

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

    // The Effects page is its own rack: the patch's chorus and hall, in the order they run.
    click(cx, "poly-page-effects");
    let names = |cx: &VisualTestContext| -> Vec<String> {
        poly(cx)
            .fx
            .iter()
            .map(pina_engine::song::project::rack_name)
            .collect()
    };
    assert_eq!(names(cx), ["Chorus", "Hall Reverb"]);
    // One switched off; the hall picked: the stage is the mixer's reverb editor, on the
    // synth's own hall, its mix turned down by its knob.
    click(cx, "poly-fx-power-0");
    assert!(poly(cx).fx[0].bypassed());
    click(cx, "poly-fx-1");
    assert!(cx.debug_bounds("reverb-display").is_some(), "its picture");
    assert!(
        !ws.read_with(cx, |ws, _| ws.fx_window_open()),
        "not a window of its own"
    );
    let table = crate::mix::strip::knobs(poly(cx).fx[1].kind());
    let mix = table.iter().position(|k| k.name == "Mix").unwrap();
    let was = (table[mix].pos)(&poly(cx).fx[1]);
    pull(cx, "fx-knob-mix", (0.0, 40.0));
    assert!((table[mix].pos)(&poly(cx).fx[1]) < was);
    // The whole rack passed by and back, each effect keeping its own switch.
    click(cx, "poly-rack-on");
    assert!(poly(cx).rack_off && poly(cx).fx[0].bypassed() && !poly(cx).fx[1].bypassed());
    click(cx, "poly-rack-on");
    assert!(!poly(cx).rack_off);
    // The browser: searched by what it sounds like; pointed at, an offer's heard in the sound
    // without going in; clicked, a tube drive goes in last and opens. Moved earlier, it's in
    // the middle.
    click(cx, "poly-add-effect");
    assert!(cx.debug_bounds("poly-offer-Tube").is_some());
    let hover = bounds(cx, "poly-offer-Ensemble").center();
    cx.simulate_mouse_move(hover, None, Modifiers::none());
    let heard = ws.read_with(cx, |ws, _| ws.rack_preview.map(|(_, u)| u.kind()));
    assert_eq!(heard, Some(pina_engine::song::project::Fx::Motion));
    assert_eq!(names(cx), ["Chorus", "Hall Reverb"], "only heard");
    click(cx, "poly-offer-Tube");
    assert!(ws.read_with(cx, |ws, _| ws.rack_preview.is_none()));
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.settings.recent_effects.clone()),
        ["Tube"]
    );
    assert_eq!(names(cx), ["Chorus", "Hall Reverb", "Warm Tube Drive"]);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.poly_view.as_ref().unwrap().fx_open),
        2
    );
    click(cx, "poly-fx-earlier");
    assert_eq!(names(cx), ["Chorus", "Warm Tube Drive", "Hall Reverb"]);
    // Dragged along the chain past the hall, the chorus goes last; undone, it's back.
    let from = bounds(cx, "poly-fx-0").center();
    let to = bounds(cx, "poly-fx-2").center();
    drag(cx, from, to + point(px(30.), px(0.)));
    assert_eq!(names(cx), ["Warm Tube Drive", "Hall Reverb", "Chorus"]);
    ws.update(cx, |ws, cx| ws.undo(cx));
    cx.run_until_parked();
    assert_eq!(names(cx), ["Chorus", "Warm Tube Drive", "Hall Reverb"]);
    // An LFO and a lane on the hall's mix follow it when the drive's taken out.
    ws.update(cx, |ws, cx| {
        ws.poly_mut(t)
            .unwrap()
            .routes
            .push(pina_engine::song::project::PolyRoute {
                source: PolySource::Lfo(0),
                dest: PolyDest::Rack(2, mix as u8),
                depth: 0.2,
                by: None,
            });
        ws.session.project.tracks[t]
            .automation
            .points_mut(Param::Rack(2, mix as u8))
            .push(pina_engine::song::project::AutoPoint::new(0, 0.5));
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.param_name(t, Param::Rack(2, mix as u8))),
        "Synth-0: Hall Reverb · Mix"
    );
    click(cx, "poly-fx-1");
    click(cx, "poly-fx-remove");
    assert_eq!(names(cx), ["Chorus", "Hall Reverb"]);
    assert!(
        poly(cx)
            .routes
            .iter()
            .any(|r| r.dest == PolyDest::Rack(1, mix as u8))
    );
    let lanes = ws.read_with(cx, |ws, _| ws.session.project.tracks[t].automation.params());
    assert!(lanes.contains(&Param::Rack(1, mix as u8)), "{lanes:?}");
    // The track's own effects come after, and are a click away.
    assert!(cx.debug_bounds("poly-open-mixer").is_some());
    // An EQ of its own is the whole EQ on the stage: a click on it makes a band there.
    ws.update(cx, |ws, cx| {
        ws.poly_op(t, crate::editors::poly::PolyOp::AddFx("EQ"), cx)
    });
    assert_eq!(names(cx), ["Chorus", "Hall Reverb", "EQ"]);
    cx.run_until_parked();
    let display = cx.debug_bounds("eq-display").expect("the EQ's display");
    let at = point(
        display.origin.x + display.size.width * 0.5,
        display.origin.y + display.size.height * 0.3,
    );
    cx.simulate_click(at, Modifiers::none());
    let bands = match poly(cx).fx[2] {
        pina_engine::song::project::Unit::Eq(e) => e.bands.len(),
        _ => 0,
    };
    assert_eq!(bands, 1);
    assert!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks[t].eq.is_none()),
        "not the track's"
    );

    // An LFO's rate: any length on the grid, any typed, or free as fast as it was.
    ws.update(cx, |ws, cx| {
        let v = ws.poly_view.as_mut().unwrap();
        v.page = PolyPage::Synth;
        v.source = PolySource::Lfo(0);
        cx.notify();
    });
    cx.run_until_parked();
    click(cx, "poly-lfo-sync");
    click(cx, "poly-rate-2-1");
    assert!(
        (poly(cx).lfos[0].beats - 0.25 * 2.0 / 3.0).abs() < 1e-6,
        "1/16 T"
    );
    click(cx, "poly-rate-typed");
    ws.update(cx, |ws, cx| ws.finish_naming(Some("3/16".into()), cx));
    assert_eq!(poly(cx).lfos[0].beats, 0.75);
    click(cx, "poly-rate-free");
    let rate = crate::editors::poly::lfo_rate_param(0);
    let tempo = ws.read_with(cx, |ws, _| ws.session.project.tempo) as f32;
    assert_eq!(poly(cx).lfos[0].beats, 0.0);
    assert!(
        (poly(cx).params[rate] - tempo / 60.0 / 0.75).abs() < 1e-3,
        "as fast as it was"
    );
    click(cx, "poly-rate-in-time");
    assert_eq!(
        poly(cx).lfos[0].beats,
        0.75,
        "back on the grid where it was"
    );

    // Racks kept: this one saved by name, then a factory sound's put in its place.
    ws.update(cx, |ws, cx| {
        ws.poly_view.as_mut().unwrap().page = PolyPage::Effects;
        cx.notify();
    });
    cx.run_until_parked();
    let kept: Vec<String> = names(cx);
    click(cx, "poly-rack-save");
    ws.update(cx, |ws, cx| ws.finish_naming(Some("Wide Space".into()), cx));
    let racks = ws.update(cx, |ws, _| ws.library().racks());
    let mine = racks
        .iter()
        .position(|r| r.0 == "Wide Space")
        .expect("kept");
    let pad = racks
        .iter()
        .position(|r| r.0 == "Evolving Pad's")
        .expect("the factory's");
    ws.update(cx, |ws, cx| {
        ws.choose(
            MenuAction::Poly(t, crate::editors::poly::PolyOp::LoadRack(pad)),
            cx,
        )
    });
    assert_eq!(names(cx), ["Ensemble", "Hall Reverb"]);
    assert!(
        !poly(cx)
            .routes
            .iter()
            .any(|r| matches!(r.dest, PolyDest::Rack(..))),
        "the old rack's rides went with it"
    );
    ws.update(cx, |ws, cx| {
        ws.choose(
            MenuAction::Poly(t, crate::editors::poly::PolyOp::LoadRack(mine)),
            cx,
        )
    });
    assert_eq!(names(cx), kept);
    let _ = std::fs::remove_dir_all(ws.read_with(cx, |ws, _| ws.sounds_dir.clone()));

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
        ws.choose(
            MenuAction::Poly(
                t,
                PolyOp::Sound(crate::editors::poly::sounds::SoundOp::Load("Reese".into())),
            ),
            cx,
        );
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
    // Its corner makes it smaller or bigger as a whole: its layout never changes, so what's in
    // it stays where it was in it; no smaller than its least scale; its close button and the
    // sound's arrows always seen.
    let share = |cx: &mut VisualTestContext, sel: &str| {
        let from = bounds(cx, "poly-patch").origin.x;
        let close = bounds(cx, "poly-close").center();
        let on = bounds(cx, sel).center();
        f32::from(on.x - from) / f32::from(close.x - from)
    };
    let placed = share(cx, "poly-patch-next");
    let grip = bounds(cx, "poly-resize").center();
    drag(cx, grip, grip - point(px(120.), px(60.)));
    let s = ws.read_with(cx, |ws, _| ws.poly_scale());
    assert!(s < 1.0, "{s}");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.poly_size()),
        (1380.0, 860.0),
        "its layout's"
    );
    let moved = share(cx, "poly-patch-next");
    assert!(
        (moved - placed).abs() < 0.01,
        "nothing moved inside it: {placed} {moved}"
    );
    let grip = bounds(cx, "poly-resize").center();
    drag(cx, grip, grip - point(px(2000.), px(2000.)));
    assert_eq!(ws.read_with(cx, |ws, _| ws.poly_scale()), 0.7);
    for sel in ["poly-close", "poly-patch-back", "poly-patch-next"] {
        let b = bounds(cx, sel);
        assert!(b.size.width > px(14.) && b.size.height > px(14.), "{sel}");
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

#[gpui::test]
fn bloom_grows_from_its_window_and_everything_in_it_works(cx: &mut TestAppContext) {
    use crate::editors::bloom::{BloomOp, BloomPage};
    use pina_engine::song::project::*;
    let (ws, cx) = open(cx);
    let t = ws.update(cx, |ws, cx| {
        ws.session.project.tracks.push(Track {
            name: "Bloom".into(),
            instrument: Some(crate::sounds::catalog::new_bloom()),
            ..Default::default()
        });
        ws.session.project.assign_ids();
        let t = ws.session.project.tracks.len() - 1;
        ws.select_track(t);
        cx.notify();
        t
    });
    cx.run_until_parked();
    // Its slot on the channel opens it.
    let slot: &'static str = Box::leak(format!("slot-Track({t})-Instrument").into_boxed_str());
    click_on(cx, slot, Modifiers::none(), MouseButton::Left);
    assert!(
        ws.read_with(cx, |ws, _| ws.bloom_view.is_some()),
        "the window opens"
    );
    let bloom = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.bloom(t).unwrap().clone());
    // Settle the garden where it's drawn: every branch as long as it is.
    ws.update(cx, |ws, cx| {
        let b = ws.bloom(t).unwrap().clone();
        if let Some(v) = ws.bloom_view.as_mut() {
            v.shown = std::array::from_fn(|k| b.reach(k, 0.0));
            v.speed = [0.0; BRANCHES];
        }
        cx.notify();
    });
    cx.run_until_parked();
    // A branch dragged out by its flower grows, bends, and is picked; its note plays meanwhile.
    let garden = cx.debug_bounds("bloom-garden").unwrap();
    let half = f32::from(garden.size.width) / 2.0;
    let center = garden.center();
    let b = bloom(cx);
    let k = 4;
    let tip = crate::editors::bloom::garden_tip(&b, k, b.reach(k, 0.0));
    let at = |x: f32, y: f32| center + point(px(x * half), px(y * half));
    cx.simulate_mouse_down(at(tip.0, tip.1), MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(
        ws.read_with(cx, |ws, _| ws
            .bloom_view
            .as_ref()
            .unwrap()
            .audition
            .is_some()),
        "it plays"
    );
    // Out to the full length, a little round clockwise.
    let a = crate::editors::bloom::garden_angle(k) + 0.2;
    let far = 0.8;
    cx.simulate_mouse_move(
        at(far * a.cos(), far * a.sin()),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        at(far * a.cos(), far * a.sin()),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    let b = bloom(cx);
    assert!(
        b.params[branch_length(k)] > 0.95,
        "{}",
        b.params[branch_length(k)]
    );
    assert!(
        b.params[branch_bend(k)] > 0.3,
        "{}",
        b.params[branch_bend(k)]
    );
    assert_eq!(b.chosen as usize, k);
    assert!(
        ws.read_with(cx, |ws, _| ws
            .bloom_view
            .as_ref()
            .unwrap()
            .audition
            .is_none()),
        "let go"
    );
    // The growth knob, as any knob: dragged up, it grows.
    let knob = cx.debug_bounds("bloom-knob-growth").unwrap().center();
    drag(cx, knob, knob - point(px(0.), px(60.)));
    assert!(bloom(cx).params[BLOOM_GROWTH] > 0.3);
    // Its lane is named for it.
    let name = ws.read_with(cx, |ws, _| {
        ws.param_name(t, Param::Poly(BLOOM_GROWTH as u16))
    });
    assert_eq!(name, "Growth");
    let gene = ws.read_with(cx, |ws, _| {
        ws.param_name(t, Param::Poly(gene_param(32) as u16))
    });
    assert_eq!(gene, "Cutoff");
    // Replanted, the branch is the seed and the old seed is family.
    let tip_genes = bloom(cx).branch_genes(k, bloom(cx).reach(k, 0.0));
    click_on(cx, "bloom-replant", Modifiers::none(), MouseButton::Left);
    let b = bloom(cx);
    assert_eq!(b.seed(), tip_genes);
    let family = b.ancestors.len();
    assert!(family >= 1);
    // Going back swaps it again.
    click_on(
        cx,
        Box::leak(format!("bloom-ancestor-{}", family - 1).into_boxed_str()),
        Modifiers::none(),
        MouseButton::Left,
    );
    assert_ne!(bloom(cx).seed(), tip_genes);
    // Planting a reese: picked from every kind, a new seed drawn as one.
    click_on(
        cx,
        "bloom-kind-picker",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert!(ws.read_with(cx, |ws, _| ws.bloom_view.as_ref().unwrap().picking));
    click_on(cx, "bloom-kind-reese", Modifiers::none(), MouseButton::Left);
    assert!(!ws.read_with(cx, |ws, _| ws.bloom_view.as_ref().unwrap().picking));
    click_on(cx, "bloom-plant", Modifiers::none(), MouseButton::Left);
    assert_eq!(bloom(cx).kind, SeedKind::Reese);
    // Crossing: the picked branch with another, by clicking it.
    click_on(cx, "bloom-cross", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| ws.bloom_view.as_ref().unwrap().crossing));
    let before = bloom(cx).seed();
    ws.update(cx, |ws, cx| ws.bloom_op(t, BloomOp::CrossWith(9), cx));
    assert_ne!(bloom(cx).seed(), before);
    assert!(!ws.read_with(cx, |ws, _| ws.bloom_view.as_ref().unwrap().crossing));
    // The keys: Layer, a branch put in it.
    click_on(cx, "bloom-keys-layer", Modifiers::none(), MouseButton::Left);
    assert_eq!(bloom(cx).keys, BloomKeys::Layer);
    // The Strand: a gene locked, a branch steered.
    click_on(
        cx,
        "bloom-page-strand",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(
        cx,
        "bloom-lock-cutoff",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert!(bloom(cx).locked(32));
    click_on(cx, "bloom-steer-true", Modifiers::none(), MouseButton::Left);
    let dial = cx.debug_bounds("bloom-steer-knob-attack").unwrap().center();
    drag(cx, dial, dial - point(px(0.), px(40.)));
    let b = bloom(cx);
    assert!(
        b.edits.iter().any(|e| e.branch == b.chosen && e.gene == 0),
        "steered"
    );
    // Sounds: filtered and planted.
    click_on(
        cx,
        "bloom-page-sounds",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(
        cx,
        "bloom-filter-bell",
        Modifiers::none(),
        MouseButton::Left,
    );
    let shown = ws.read_with(cx, |ws, _| ws.bloom_view.as_ref().unwrap().sounds.shown());
    assert!(!shown.is_empty() && shown.iter().all(|(_, s)| s.kind == SeedKind::Bell));
    let first = shown[0].0;
    click_on(
        cx,
        Box::leak(format!("bloom-sound-{first}").into_boxed_str()),
        Modifiers::none(),
        MouseButton::Left,
    );
    assert_eq!(bloom(cx).sound, shown[0].1.name);
    // Its frames draw it alone: the app under it isn't drawn again while it moves.
    let drawn = crate::workspace::MAIN_RENDERS.with(|n| n.get());
    let view = ws.read_with(cx, |ws, _| ws.bloom_window_view().unwrap());
    for _ in 0..5 {
        view.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
    }
    assert_eq!(crate::workspace::MAIN_RENDERS.with(|n| n.get()), drawn);
    // A change to the song draws both.
    ws.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    assert!(crate::workspace::MAIN_RENDERS.with(|n| n.get()) > drawn);
    // Undo takes it back; ⌘W closes the window.
    ws.update(cx, |ws, cx| ws.undo(cx));
    assert_ne!(bloom(cx).sound, shown[0].1.name);
    ws.update(cx, |ws, cx| {
        ws.bloom_op(t, BloomOp::Page(BloomPage::Garden), cx)
    });
    ws.update(cx, |ws, cx| ws.close_bloom(cx));
    assert!(ws.read_with(cx, |ws, _| ws.bloom_view.is_none()));
}
