//! The pages: settings, songs, playbooks, song study, and edits kept for recovery.

use super::*;

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
fn unsaved_edits_are_kept_and_offered_back(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("pina-test-kept-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("Audio Files")).unwrap();
    for f in ["project.json", "Audio Files/drums.wav"] {
        std::fs::copy(demo().join(f), dir.join(f)).unwrap();
    }
    let (ws, cx) = open_at(cx, &dir);
    let saved = ws.read_with(cx, |ws, _| ws.session.project.tempo);
    let kept = dir.join(".recovery/project.json");
    // Edits, kept; then Pina Studio stops, and the project opens again and asks.
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
fn songs_start_from_the_songs_page_open_here_and_go_to_the_trash(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("pina-test-window-songs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let other = crate::pages::projects::create(
        &root,
        "Other",
        96.0,
        pina_engine::music::Key::parse("D minor"),
        [3, 4],
        crate::pages::projects::Template::Band,
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
    // ⌘N: the Songs page, a new song's name ready to type. A name, then a template's click:
    // the song is made where songs are kept (here a scratch folder) and opens.
    // SAFETY: no other test reads PINA_SONGS.
    unsafe { std::env::set_var("PINA_SONGS", &root) };
    cx.simulate_keystrokes("cmd-n");
    cx.run_until_parked();
    assert_eq!(ws.read_with(cx, |ws, _| ws.view), View::Songs);
    let name = ws
        .read_with(cx, |ws, _| ws.home.as_ref().map(|h| h.name.clone()))
        .expect("the Songs page");
    name.update(cx, |i, cx| i.set_text("Rain", cx));
    click_on(cx, "template-Lo-fi", Modifiers::none(), MouseButton::Left);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.session.dir, root.join("Rain"));
        assert_eq!(ws.view, View::Arrange);
        let names: Vec<&str> = ws
            .session
            .project
            .tracks
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(names, ["Vinyl", "Drums", "Keys", "Bass"]);
    });
    // The song open, to the Trash (asked first): it closes, and the window is the Songs page
    // alone, nothing to play.
    ws.update(cx, |ws, cx| {
        ws.song_pending = Some(crate::ui::menu::MenuAction::TrashSong(root.join("Rain")));
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Move to Trash");
    cx.run_until_parked();
    assert!(!root.join("Rain").exists());
    assert!(ws.read_with(cx, |ws, _| ws.home_alone()));
    assert!(cx.debug_bounds("play").is_none());
    // Two picked with ⌘-click, Delete: both to the Trash, asked once.
    let third = crate::pages::projects::create(
        &root,
        "Third",
        100.0,
        None,
        [4, 4],
        crate::pages::projects::Template::Empty,
    )
    .unwrap();
    ws.update(cx, |ws, cx| {
        ws.songs = crate::pages::projects::list(&root, &[]);
        cx.notify();
    });
    cx.run_until_parked();
    click_on(cx, "song-Other", Modifiers::command(), MouseButton::Left);
    click_on(cx, "song-Third", Modifiers::command(), MouseButton::Left);
    let picks = ws.read_with(cx, |ws, _| ws.home.as_ref().unwrap().picks.len());
    assert_eq!(picks, 2);
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    cx.simulate_prompt_answer("Move to Trash");
    cx.run_until_parked();
    assert!(!other.exists() && !third.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[gpui::test]
fn with_no_song_open_tempo_starts_on_the_songs_page(cx: &mut TestAppContext) {
    // A song not made yet, as Pina Studio opens with none named: the Songs page is the whole window,
    // a template a click away. Close Song from a song comes back here.
    let scratch = std::env::temp_dir().join(format!("pina-test-scratch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let (ws, cx) = open_at(cx, &scratch.join("Untitled"));
    assert!(ws.read_with(cx, |ws, _| ws.home_alone()));
    assert!(cx.debug_bounds("play").is_none());
    assert!(cx.debug_bounds("template-Beat").is_some());
    assert!(cx.debug_bounds("new-song-name").is_some());
    // ⌘O has no song to go back to.
    cx.simulate_keystrokes("cmd-o");
    assert!(ws.read_with(cx, |ws, _| ws.home_alone()));
}

#[gpui::test]
fn playbook_parts_are_added_edited_and_removed(cx: &mut TestAppContext) {
    // `open_at` gives the tests a playbooks folder of their own.
    let books = std::env::temp_dir().join(format!("pina-test-books-{}", std::process::id()));
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
        serde_json::from_str::<pina_engine::music::playbook::Playbook>(&json).unwrap()
    };
    let before = parts(cx).len();

    // Add a percussion part: it shows, and it's saved.
    click_on(cx, "genre-tab-parts", Modifiers::none(), MouseButton::Left);
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
    cx.simulate_input("congas and bongos on a percussion kit, written by hand");
    click_on(cx, "genre-tab-feel", Modifiers::none(), MouseButton::Left);
    assert_eq!(
        saved().parts[n].write,
        "congas and bongos on a percussion kit, written by hand"
    );

    // Remove it: gone from the page, the file, and every section.
    click_on(cx, "genre-tab-parts", Modifiers::none(), MouseButton::Left);
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
    // As it was.
    assert_eq!(book, playbook("lofi"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn a_book_shows_its_moves_on_its_lanes_and_finds_its_way_among_many(cx: &mut TestAppContext) {
    use crate::pages::genres::{BookTab, Shelf};
    let dir = demo_copy("playbook-lanes");
    let (ws, cx) = open_at(cx, &dir);
    ws.update(cx, |ws, cx| ws.open_genres(Some("house"), cx));
    cx.run_until_parked();
    // A move picked lights the bars it's about.
    assert_eq!(ws.read_with(cx, |ws, _| ws.genres.tab), BookTab::Moves);
    assert!(cx.debug_bounds("genre-lanes").is_some());
    let chorus = ws.read_with(cx, |ws, _| {
        let book = ws.genres.draft.as_ref().unwrap();
        pina_engine::music::playbook::moves::moves(book)
            .iter()
            .position(|m| m.at.is_some())
            .unwrap()
    });
    let sel: &'static str = Box::leak(format!("move-{chorus}").into_boxed_str());
    click_on(cx, sel, Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.genres.picked), chorus);
    assert!(cx.debug_bounds("genre-lit").is_some());
    // Chords and groove, and hearing the book: rendered, then playing, then stopped.
    click_on(cx, "genre-tab-chords", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.genres.tab), BookTab::Sound);
    click_on(cx, "genre-play", Modifiers::none(), MouseButton::Left);
    until(cx, &ws, |ws| ws.genres.heard.is_some());
    click_on(cx, "genre-play", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| ws.genres.heard.is_none()));
    // The rail narrows to what's asked for, however many books there are.
    click_on(
        cx,
        "genres-only-major",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert_eq!(ws.read_with(cx, |ws, _| ws.genres.shelf), Shelf::Major);
    assert!(cx.debug_bounds("genre-house").is_none(), "house is minor");
    click_on(cx, "genres-only-all", Modifiers::none(), MouseButton::Left);
    assert!(cx.debug_bounds("genre-house").is_some());
    ws.update(cx, |ws, cx| {
        let search = ws.genres.search.clone().unwrap();
        search.update(cx, |input, cx| input.set_text("lo-fi", cx));
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("genre-house").is_none());
    assert!(cx.debug_bounds("genre-lofi").is_some());
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn a_song_studied_shows_its_form_and_becomes_the_mix_reference(cx: &mut TestAppContext) {
    // Studies of the tests' own, never the person's.
    let dir = test_studies();
    // A minute at 120 BPM: a kick on every beat, over A minor and then, louder, over F major.
    let rate = 44_100u32;
    let l: Vec<f32> = (0..rate as usize * 60)
        .map(|i| {
            let t = i as f32 / rate as f32;
            let beat = t % 0.5;
            let (notes, level) = match t < 30.0 {
                true => ([220.0f32, 261.63, 329.63], 0.05),
                false => ([174.61, 220.0, 261.63], 0.14),
            };
            let chord: f32 = notes
                .iter()
                .map(|f| (std::f32::consts::TAU * f * t).sin())
                .sum();
            level * chord + 0.5 * (-beat * 30.0).exp() * (std::f32::consts::TAU * 55.0 * beat).sin()
        })
        .collect();
    let song = dir.join("Someone - A Test.wav");
    pina_engine::music::analysis::write_wav(&song, &l, &l, rate).unwrap();
    let studied = crate::cli::study::study(&song, false).unwrap();
    assert_eq!(studied.identity.title, "A Test");
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| ws.open_study(cx));
    cx.run_until_parked();
    assert!(ws.read_with(cx, |ws, _| ws.view == View::Study
        && ws.studies.open.as_ref() == Some(&song)));
    assert!(cx.debug_bounds("study-title").is_some());
    // A section picked opens up.
    click_on(cx, "study-section-0", Modifiers::none(), MouseButton::Left);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.studies.pick),
        crate::pages::study::Pick::Section(0)
    );
    assert!(cx.debug_bounds("study-section-detail").is_some());
    // Its mix becomes this song's reference.
    click_on(cx, "study-reference", Modifiers::none(), MouseButton::Left);
    let reference = ws
        .read_with(cx, |ws, _| ws.session.project.reference.clone())
        .unwrap();
    assert_eq!(reference.file, song);
    assert!((reference.lufs - studied.mix.overall.lufs).abs() < 0.01);
}

#[gpui::test]
fn a_loop_shows_only_what_it_has_and_plays_from_where_its_clicked(cx: &mut TestAppContext) {
    use crate::pages::study::{Dock, Only, Pick};
    test_studies();
    let dir = std::env::temp_dir().join(format!("pina-study-lanes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Four bars of a chord at 120, named as packs name their loops.
    let rate = 44_100u32;
    let l: Vec<f32> = (0..rate as usize * 8)
        .map(|i| {
            let t = i as f32 / rate as f32;
            let swell = (-(t % 2.0) * 1.5).exp();
            let chord = match (t / 4.0) as u32 % 2 {
                0 => [220.0f32, 261.63, 329.63],
                _ => [174.61, 220.0, 261.63],
            };
            chord
                .iter()
                .map(|f| 0.12 * swell * (std::f32::consts::TAU * f * t).sin())
                .sum()
        })
        .collect();
    let song = dir.join("Test Piano Loop Amin (120 BPM).wav");
    pina_engine::music::analysis::write_wav(&song, &l, &l, rate).unwrap();
    let studied = crate::cli::study::study(&song, false).unwrap();
    assert_eq!(studied.nature.name, "Piano loop");
    assert_eq!(studied.tempo.bpm, 120.0);
    assert_eq!(studied.key.name, "A minor");
    assert!(studied.groove.is_none(), "no drums in a piano");
    assert!(studied.genres.is_empty(), "a loop isn't near a song's book");
    assert_eq!(studied.instruments.len(), 1);

    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.open_study(cx);
        ws.studies.open = Some(song.clone());
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("study-absent").is_some());
    assert!(cx.debug_bounds("study-tab-groove").is_none());
    assert!(cx.debug_bounds("study-tab-playbooks").is_none());
    assert!(cx.debug_bounds("study-spectrogram").is_some());

    // A chord picked reads out at the side and moves the playhead.
    click_on(cx, "study-chord-1", Modifiers::none(), MouseButton::Left);
    let (pick, at) = ws.read_with(cx, |ws, _| (ws.studies.pick, ws.studies.at));
    assert_eq!(pick, Pick::Chord(1));
    assert!(at > 0.0);
    // Play starts there, and stops where it got to.
    click_on(cx, "study-play", Modifiers::none(), MouseButton::Left);
    until(cx, &ws, |ws| ws.studies.playing.is_some());
    click_on(cx, "study-play", Modifiers::none(), MouseButton::Left);
    let (playing, stopped) = ws.read_with(cx, |ws, _| (ws.studies.playing, ws.studies.at));
    assert!(playing.is_none() && stopped >= at);
    // A fact says where it comes from.
    click_on(cx, "study-fact-tempo", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.studies.pick), Pick::Tempo);
    // The dock opens what's in it.
    click_on(cx, "study-tab-mix", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.studies.dock), Dock::Mix);
    // The rail narrows to what's asked for.
    click_on(cx, "study-only-songs", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.studies.only), Only::Songs);
    assert!(cx.debug_bounds("study-0").is_none(), "a loop isn't a song");
}

#[gpui::test]
fn settings_choose_the_buffer_and_the_cpu_number_holds_still(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_keystrokes("cmd-,");
    click_on(cx, "settings-Audio", Modifiers::none(), MouseButton::Left);
    cx.run_until_parked();
    // Each size the output can do is a click away, and Auto hands it back to the system.
    assert!(ws.read_with(cx, |ws, _| !ws.buffers.is_empty()));
    let size = ws.read_with(cx, |ws, _| ws.buffers[0]);
    let chip = format!("setting-buffer-{size}").leak();
    click_on(cx, chip, Modifiers::none(), MouseButton::Left);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.settings.audio.buffer),
        Some(size)
    );
    click_on(
        cx,
        "setting-buffer-Auto",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert_eq!(ws.read_with(cx, |ws, _| ws.settings.audio.buffer), None);
    // The system's output is always there to go back to.
    assert!(cx.debug_bounds("setting-output-System output").is_some());
    click_on(
        cx,
        "setting-output-System output",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.settings.audio.device.clone()),
        None
    );
}
