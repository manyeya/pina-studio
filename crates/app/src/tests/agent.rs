//! The agents: the Agent panel, the socket they edit through, and what they're given.

use super::*;

#[gpui::test]
fn agents_edit_the_open_project_through_its_socket(cx: &mut TestAppContext) {
    // A copy of the demo, so a Pina Studio that has the real one open isn't disturbed.
    let dir = std::env::temp_dir().join(format!("pina-test-socket-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("Audio Files")).unwrap();
    for f in ["project.json", "Audio Files/drums.wav"] {
        std::fs::copy(demo().join(f), dir.join(f)).unwrap();
    }
    cx.executor().allow_parking();
    let (ws, cx) = open_at(cx, &dir);
    let weak = ws.downgrade();
    cx.update(|_, cx| crate::cli::server::serve(weak, &dir, cx));

    // Ask from another thread, as `pina do` would from another process.
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
        pina_engine::song::project::Project::load(&dir)
            .unwrap()
            .tempo,
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
fn the_agent_panel_shows_claude_working_as_it_goes(cx: &mut TestAppContext) {
    use crate::agent::chat::{Item, Step};
    // A stand-in for Claude Code: it takes the question and answers as Claude does, an edit and
    // a sentence, then waits to be let go.
    let dir = std::env::temp_dir().join(format!("pina-test-claude-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let claude = dir.join("claude");
    std::fs::write(
        &claude,
        r#"#!/bin/sh
read question
cat <<'OUT'
{"type":"system","subtype":"init","model":"claude-test-1"}
{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking"}}}
{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"t1","name":"mcp__pina__edit","input":{}}}}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"mcp__pina__edit","input":{"project":"p","command":{"op":"set_tempo","bpm":101}}}]}}
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
    // SAFETY: no other test reads PINA_CLAUDE.
    unsafe { std::env::set_var("PINA_CLAUDE", &claude) };
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
        agent::chat::Item,
        agent::{Kind, Readiness},
    };
    let (ws, cx) = open(cx);
    // Not signed in when last asked: it's asked again first, then (still not) says how.
    ws.update(cx, |ws, cx| {
        ws.chat.ready = vec![(Kind::Codex, Readiness::SignIn)];
        ws.chat.kind = Kind::Codex;
        ws.ask_agent("Make it 101".into(), cx);
    });
    cx.run_until_parked();
    ws.read_with(cx, |ws, _| {
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
    click_on(
        cx,
        "palette-start-sounds",
        Modifiers::none(),
        MouseButton::Left,
    );
    let (picking, browser, scope) =
        ws.read_with(cx, |ws, _| (ws.picking, ws.show_browser, ws.catalog.scope));
    assert!(picking && browser && scope == crate::sounds::catalog::Scope::Sounds);
    assert!(cx.debug_bounds("picking-done").is_some());
    // A sound given (the library's ＋ does this), then back to the agent with Done.
    let drums = dir.join("Audio Files/drums.wav");
    ws.update(cx, |ws, cx| ws.give_agent(drums.clone(), cx));
    click_on(cx, "picking-done", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| ws.show_agent && !ws.show_browser && !ws.picking));
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
    // Instruments and effects too: the Browser's instruments, opened from the ＋'s menu, each
    // with its own ＋; given a part, and told with how to put it in.
    click_on(cx, "palette-more", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Instruments…");
    assert!(ws.read_with(cx, |ws, _| ws.picking
        && ws.catalog.scope == crate::sounds::catalog::Scope::Instruments));
    click_on(
        cx,
        "give-agent-own:synth",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert_eq!(palette(cx).devices.len(), 1);
    assert!(palette(cx).devices[0].instrument);
    click_on(cx, "picking-done", Modifiers::none(), MouseButton::Left);
    click_on(
        cx,
        "palette-device-role-0",
        Modifiers::none(),
        MouseButton::Left,
    );
    pick(cx, &ws, "Pad");
    let note = ws.update(cx, |ws, _| ws.palette_note()).unwrap();
    assert!(
        note.contains("Instruments")
            && note.contains(r#"pad: "Sine Synth" — Pina Studio's own: "instrument": "synth""#),
        "{note}"
    );
    click_on(
        cx,
        "palette-device-remove-0",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert!(palette(cx).devices.is_empty());
    // Cleared: the agent hears it's on its own again; and it's kept in the song.
    click_on(cx, "palette-clear", Modifiers::none(), MouseButton::Left);
    assert!(palette(cx).is_empty());
    let note = ws.update(cx, |ws, _| ws.palette_note()).unwrap();
    assert!(note.contains("choose your own"));
    cx.simulate_keystrokes("cmd-z");
    ws.update(cx, |ws, cx| ws.save(cx));
    let saved = pina_engine::song::project::Project::load(&dir).unwrap();
    assert_eq!(saved.palette.sounds.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui::test]
fn the_agent_panel_knows_the_song_and_its_critic(cx: &mut TestAppContext) {
    use crate::{agent::chat::Item, agent::producer::Stage};
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
        crate::agent::producer::STAGES
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
    use pina_engine::music::playbook::{self, Origin};
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
    assert_eq!(playbook::origin(&draft(cx)), Origin::Pina);

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
    // Reset to Pina Studio's: house as it ships.
    ws.update(cx, |ws, cx| ws.open_genres(Some("house"), cx));
    click_on(cx, "genre-reset", Modifiers::none(), MouseButton::Left);
    assert_eq!(playbook::origin(&draft(cx)), Origin::Pina);
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
