//! Smart Slice in one click: it hears what's in a recording and slices it that way; its slices
//! heard, narrowed and put on a Sampler; what it heard put right; and flipped as a beat.

use super::*;
use crate::{agent::Kind, editors::audio::SliceFilter, ui::menu::MenuAction};
use pina_engine::{smart::InIt, song::project::Instrument};

/// The demo with its drums region in the Audio editor, Smart Sliced (one click): its folder.
fn sliced<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
) -> (Entity<Workspace>, &'a mut VisualTestContext, PathBuf) {
    // The tests' models (none with a voice in it) and Whisper stand-in, never the person's.
    let _ = (test_models(), test_whisper());
    cx.executor().allow_parking();
    let dir = demo_copy(name);
    let (ws, cx) = open_at(cx, &dir);
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    ws.update(cx, |ws, cx| {
        ws.select_track(drums);
        ws.region = Some((drums, 0));
        ws.editor_h = 760.0;
        ws.show_bottom(Bottom::Audio, cx);
    });
    cx.run_until_parked();
    // One click: no questions; it works (a short file, done at once) and shows its slices.
    click_on(cx, "clip-smart", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| ws.beat_wait.is_some() || ws.beat.is_some()));
    assert!(ws.read_with(cx, |ws, _| ws.menu.is_none()), "nothing asked");
    until(cx, &ws, |ws| ws.beat.is_some());
    cx.run_until_parked();
    (ws, cx, dir)
}

#[gpui::test]
fn one_click_hears_a_beat_slices_it_to_hear_narrow_and_put_on_a_sampler(cx: &mut TestAppContext) {
    let (ws, cx, dir) = sliced(cx, "smart");
    let (slices, heard, guessed) = ws.read_with(cx, |ws, _| {
        let b = ws.beat.as_ref().unwrap();
        (b.slices.clone(), b.heard, b.guessed)
    });
    // No model to listen with: the sample analysis hears drums, a beat; a guess, said so.
    assert_eq!((heard, guessed), (InIt::Beat, true));
    assert!(slices.len() >= 4, "{slices:?}");
    assert!(slices.windows(2).all(|w| w[0].start < w[1].start));
    for what in [
        "lyrics-tab-lines",
        "lyrics-tab-flips",
        "smart-heard-as",
        "slices-filter",
        "slices-to-sampler",
        "slice-0",
    ] {
        assert!(cx.debug_bounds(what).is_some(), "{what}");
    }
    assert!(
        cx.debug_bounds("lyrics-tab-words").is_none(),
        "no voice, no words"
    );
    // Heard: lit; again, stopped.
    click_on(cx, "slice-hear-0", Modifiers::none(), MouseButton::Left);
    let playing = |cx: &mut VisualTestContext| {
        ws.read_with(cx, |ws, _| ws.beat.as_ref().unwrap().playing.map(|p| p.0))
    };
    assert_eq!(playing(cx), Some(0));
    click_on(cx, "slice-hear-0", Modifiers::none(), MouseButton::Left);
    assert_eq!(playing(cx), None);
    // Narrowed to the drums: those go on the Sampler, a pad each, named.
    ws.update(cx, |ws, cx| {
        ws.choose(MenuAction::SliceFilter(Some(SliceFilter::Drums)), cx)
    });
    let drums: Vec<String> = slices
        .iter()
        .filter(|s| s.drum())
        .map(|s| s.name.clone())
        .collect();
    assert!(!drums.is_empty());
    let tracks = ws.read_with(cx, |ws, _| ws.session.project.tracks.len());
    click_on(
        cx,
        "slices-to-sampler",
        Modifiers::none(),
        MouseButton::Left,
    );
    ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        assert_eq!(p.tracks.len(), tracks + 1);
        let t = &p.tracks[ws.track];
        assert_eq!(t.name, "drums Drums");
        let Some(Instrument::Sampler(s)) = &t.instrument else {
            panic!("no sampler")
        };
        let names: Vec<String> = s.zones.iter().map(|z| z.name.clone()).collect();
        assert_eq!(names, drums);
        assert_eq!(ws.bottom, Some(Bottom::Sampler));
    });
    ws.update(cx, |ws, cx| ws.undo(cx));
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks.len()),
        tracks
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[gpui::test]
fn what_it_heard_is_put_right_and_a_voice_is_cut_into_vocal_chops(cx: &mut TestAppContext) {
    let (ws, cx, dir) = sliced(cx, "smart-voice");
    // Put right: it's a voice alone. Sliced again as one, never as instruments; kept so.
    ws.update(cx, |ws, cx| {
        ws.choose(MenuAction::SliceHeardAs(InIt::Voice), cx)
    });
    until(cx, &ws, |ws| {
        ws.beat.as_ref().is_some_and(|b| b.heard == InIt::Voice)
    });
    cx.run_until_parked();
    let slices = ws.read_with(cx, |ws, _| ws.beat.as_ref().unwrap().slices.clone());
    assert!(!slices.is_empty());
    assert!(
        slices
            .iter()
            .all(|s| s.voice && s.name.starts_with("Vocal")),
        "{slices:?}"
    );
    let file = std::path::Path::new("Audio Files/drums.wav");
    assert_eq!(
        pina_engine::smart::kept(&dir, file).heard,
        Some(InIt::Voice)
    );
    // A voice: its words a tab away.
    assert!(cx.debug_bounds("lyrics-tab-words").is_some());
    click_on(cx, "lyrics-tab-words", Modifiers::none(), MouseButton::Left);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.flips.tab),
        crate::editors::audio::LyricsTab::Words
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[gpui::test]
fn a_beats_slices_are_flipped_as_on_an_mpc_by_the_studio_agent(cx: &mut TestAppContext) {
    let (ws, cx, dir) = sliced(cx, "smart-flips");
    let claude = super::flips::engine("beat", super::flips::SAID, 0.1, None);
    ws.update(cx, |ws, _| {
        ws.flips.command = Some(claude.clone());
        ws.flips.kind = Some(Kind::ClaudeCode);
    });
    click_on(cx, "lyrics-tab-flips", Modifiers::none(), MouseButton::Left);
    click_on(cx, "flips-find", Modifiers::none(), MouseButton::Left);
    until(cx, &ws, |ws| {
        ws.flips.now.as_ref().is_some_and(|f| f.asking.is_none())
    });
    cx.run_until_parked();
    // Asked as a beat: its slices by bar, named; the MPC's ways.
    let asked = std::fs::read_to_string(claude.with_file_name("asked")).unwrap();
    assert!(
        asked.contains("chopped at its hits") && asked.contains("Bar 1: 0 "),
        "{asked}"
    );
    assert!(asked.contains("MPC flip"));
    let (ideas, heard) = ws.read_with(cx, |ws, _| {
        let f = ws.flips.now.as_ref().unwrap();
        (f.ideas.clone(), f.heard.clone())
    });
    assert_eq!(ideas.len(), 2);
    assert_eq!(heard, std::path::PathBuf::from("Audio Files/drums.wav"));
    click_on(cx, "flip-use-0", Modifiers::none(), MouseButton::Left);
    ws.read_with(cx, |ws, _| {
        let t = &ws.session.project.tracks[ws.track];
        let Some(Instrument::Sampler(s)) = &t.instrument else {
            panic!("no sampler")
        };
        assert!(s.zones.iter().all(|z| z.file == heard));
    });
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(claude.parent().unwrap());
}
