//! Sounds: the sample browser, the library and its map, and the Browser's devices.

use super::*;

#[gpui::test]
fn the_compact_browser_is_resized_by_its_left_edge(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_keystrokes("f");
    cx.simulate_keystrokes("escape");
    let width = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.panel_width());
    assert_eq!(width(cx), crate::sounds::browser::PANEL_W);
    // Left 100: wider by 100, the lanes narrower.
    let lanes = ws.read_with(cx, |ws, _| ws.lanes_bounds.get().size.width);
    let edge = cx.debug_bounds("panel-edge").unwrap().center();
    drag(cx, edge, edge - point(px(100.), px(0.)));
    assert_eq!(width(cx), crate::sounds::browser::PANEL_W + 100.0);
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
    assert_eq!(width(cx), crate::sounds::browser::PANEL_W);
    assert!(!ws.read_with(cx, |ws, _| ws.session.edited));
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
            crate::sounds::browser::KeyFilter::Key(pina_engine::music::Key {
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
    use pina_engine::song::project::Instrument;
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
    use pina_engine::song::project::Instrument;
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
    use pina_engine::song::project::Instrument;
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
    // SAFETY: no other test reads PINA_LIBRARY.
    unsafe { std::env::set_var("PINA_LIBRARY", dir.join("config")) };
    pina_engine::samples::library::Library {
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

#[cfg(target_os = "macos")]
#[gpui::test]
fn the_browser_finds_by_words_and_puts_devices_on_tracks(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.show_browser = true;
        ws.plugins = Some(vec![PluginInfo {
            format: pina_engine::song::project::Format::Au,
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
fn the_browser_names_new_tracks_and_puts_effects_on_them(cx: &mut TestAppContext) {
    use crate::sounds::catalog::Target;
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
