//! The Studio Agent: conversations kept and carried on, copying, what it shows, and what changed.
//! Clicks and drags as a person would make them; a stand-in for Claude Code where it's asked.

use super::*;
use crate::agent::chat::{ChatPage, Item, Step};
use serde_json::json;

fn open_agent<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
) -> (Entity<Workspace>, &'a mut VisualTestContext, PathBuf) {
    let dir = demo_copy(name);
    let (ws, cx) = open_at(cx, &dir);
    ws.update(cx, |ws, cx| {
        ws.show_agent = true;
        ws.show_browser = false;
        cx.notify();
    });
    cx.run_until_parked();
    (ws, cx, dir)
}

fn click(cx: &mut VisualTestContext, name: &'static str) {
    click_on(cx, name, Modifiers::none(), MouseButton::Left);
    cx.run_until_parked();
}

/// A conversation with something in it, kept.
fn talk(ws: &Entity<Workspace>, cx: &mut VisualTestContext, asked: &str, said: &str) {
    ws.update(cx, |ws, cx| {
        ws.chat.items = vec![Item::You(asked.into()), Item::Words(said.into())];
        ws.keep_chat();
        cx.notify();
    });
    cx.run_until_parked();
}

fn card(id: &str, input: serde_json::Value) -> Item {
    Item::Tool {
        id: id.into(),
        name: "mcp__pina__present".into(),
        input,
        step: Step::Done,
    }
}

#[gpui::test]
fn a_new_conversation_leaves_the_last_to_come_back_to(cx: &mut TestAppContext) {
    let (ws, cx, dir) = open_agent(cx, "agent-history");
    talk(&ws, cx, "Write a dark bass line", "Done: four bars.");
    // New: the panel's empty, and what was said is still there.
    click(cx, "new-chat");
    ws.read_with(cx, |ws, _| {
        assert!(ws.chat.items.is_empty() && ws.chat.id.is_none());
        assert_eq!(ws.chat.past.len(), 1);
    });
    // On the page a new one starts on, and in History.
    assert!(cx.debug_bounds("past-0").is_some(), "offered straight away");
    talk(&ws, cx, "Add a drum fill", "Added it.");
    click(cx, "new-chat");
    click(cx, "agent-tab-history");
    let titles = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            let past = &ws.chat.past;
            past.iter().map(|p| p.title.clone()).collect::<Vec<_>>()
        })
    };
    assert_eq!(titles(cx), ["Add a drum fill", "Write a dark bass line"]);

    // Opened, it's all back, and it's what's showing.
    click(cx, "past-1");
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.chat.page, ChatPage::Chat);
        assert_eq!(ws.chat.items[1], Item::Words("Done: four bars.".into()));
        assert_eq!(ws.chat.title, "Write a dark bass line");
    });

    // A song read afresh finds them on disk.
    let list = crate::agent::chat::list(&dir);
    assert_eq!(list.len(), 2);
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn history_finds_what_was_said_and_conversations_rename_and_delete(cx: &mut TestAppContext) {
    let (ws, cx, dir) = open_agent(cx, "agent-find");
    talk(&ws, cx, "Mix the drums", "Tightened the kick.");
    click(cx, "new-chat");
    talk(&ws, cx, "Write a hook", "A rising melody.");
    click(cx, "new-chat");
    click(cx, "agent-tab-history");
    assert!(cx.debug_bounds("past-1").is_some());
    // By what was said, not only its title.
    ws.update(cx, |ws, cx| {
        let find = ws.chat.find.clone();
        find.update(cx, |i, cx| i.set_text("kick", cx));
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("past-0").is_none(),
        "the hook doesn't say it"
    );
    assert!(cx.debug_bounds("past-1").is_some());
    ws.update(cx, |ws, cx| {
        let find = ws.chat.find.clone();
        find.update(cx, |i, cx| i.set_text("", cx));
    });
    cx.run_until_parked();

    // Rename, from its menu.
    click_on(cx, "past-0", Modifiers::none(), MouseButton::Right);
    cx.run_until_parked();
    pick(cx, &ws, "Rename");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("The hook");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.chat.past[0].title.clone()),
        "The hook"
    );
    let on_disk = crate::agent::chat::list(&dir);
    assert!(
        on_disk.iter().any(|p| p.title == "The hook"),
        "and it stays"
    );

    // Delete, from its menu: gone from the folder too.
    click_on(cx, "past-0", Modifiers::none(), MouseButton::Right);
    cx.run_until_parked();
    pick(cx, &ws, "Delete");
    cx.run_until_parked();
    assert_eq!(ws.read_with(cx, |ws, _| ws.chat.past.len()), 1);
    assert_eq!(crate::agent::chat::list(&dir).len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
fn stand_in(name: &str) -> (PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    // A folder a test: tests run side by side, and each reads back what it asked.
    let dir = std::env::temp_dir().join(format!("pina-stand-in-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (claude, log) = (dir.join("claude"), dir.join("asked.log"));
    std::fs::write(
        &claude,
        format!(
            r#"#!/bin/sh
read question
args=$(printf '%s ' "$@" | tr '\n' ' ')
echo "$args :: $question" >> "{log}"
case "$*" in
  *"--resume gone"*) echo "No conversation found with session ID: gone" >&2; exit 1;;
esac
cat <<'OUT'
{{"type":"system","subtype":"init","model":"claude-test-1","session_id":"kept-1"}}
{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"Carried on."}}}}}}
{{"type":"result","subtype":"success","is_error":false,"duration_ms":10}}
OUT
cat > /dev/null
"#,
            log = log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    (claude, log)
}

#[cfg(unix)]
fn settle_agent(ws: &Entity<Workspace>, cx: &mut VisualTestContext) {
    // Room for a machine busy with the other tests: it's done as soon as the answer's in.
    let deadline = Instant::now() + Duration::from_secs(20);
    while ws.read_with(cx, |ws, _| ws.chat.busy) && Instant::now() < deadline {
        cx.run_until_parked();
        thread::sleep(Duration::from_millis(5));
    }
    cx.run_until_parked();
}

#[cfg(unix)]
#[gpui::test]
fn an_old_conversation_carries_on_and_survives_the_agent_forgetting_it(cx: &mut TestAppContext) {
    let (claude, log) = stand_in("carry");
    let asked = |what: &str| {
        let all = std::fs::read_to_string(&log).unwrap_or_default();
        all.lines().find(|l| l.contains(what)).map(str::to_string)
    };
    cx.executor().allow_parking();
    let (ws, cx, dir) = open_agent(cx, "agent-carry");
    ws.update(cx, |ws, _| {
        ws.chat.command = Some(claude.clone());
    });
    // A conversation the agent remembers: it's carried on under its own id.
    ws.update(cx, |ws, cx| {
        ws.chat.items = vec![
            Item::You("Write a verse".into()),
            Item::Words("Wrote eight bars in A minor.".into()),
        ];
        ws.chat.session = Some("remembered".into());
        ws.keep_chat();
        cx.notify();
    });
    click(cx, "new-chat");
    click(cx, "past-0");
    ws.update(cx, |ws, cx| ws.ask_agent("Now a chorus please".into(), cx));
    settle_agent(&ws, cx);
    let line = asked("Now a chorus please").expect("it was asked");
    assert!(line.contains("--resume remembered"), "{line}");
    assert!(!line.contains("What was said"), "it remembers: {line}");

    // One it's lost: asked again, told what was said, and the person sees one question.
    ws.update(cx, |ws, cx| {
        ws.chat.items = vec![
            Item::You("Write a bridge".into()),
            Item::Words("Wrote a bridge in C major.".into()),
        ];
        ws.chat.id = None;
        ws.chat.agent_gone();
        ws.chat.session = Some("gone".into());
        ws.keep_chat();
        cx.notify();
    });
    click(cx, "new-chat");
    click(cx, "past-0");
    ws.update(cx, |ws, cx| ws.ask_agent("Make it longer".into(), cx));
    settle_agent(&ws, cx);
    let line = std::fs::read_to_string(&log)
        .unwrap()
        .lines()
        .rfind(|l| l.contains("Make it longer"))
        .map(str::to_string)
        .unwrap();
    assert!(!line.contains("--resume"), "{line}");
    assert!(line.contains("Wrote a bridge in C major"), "told: {line}");
    ws.read_with(cx, |ws, _| {
        let questions = ws
            .chat
            .items
            .iter()
            .filter(|i| matches!(i, Item::You(t) if t == "Make it longer"))
            .count();
        assert_eq!(questions, 1);
        assert!(
            ws.chat
                .items
                .iter()
                .any(|i| *i == Item::Words("Carried on.".into()))
        );
        assert!(
            !ws.chat
                .items
                .iter()
                .any(|i| matches!(i, Item::End { error: Some(_), .. }))
        );
        assert_eq!(
            ws.chat.session.as_deref(),
            Some("kept-1"),
            "the new id is kept"
        );
    });
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn text_can_be_selected_and_copied_three_ways(cx: &mut TestAppContext) {
    let (ws, cx, dir) = open_agent(cx, "agent-copy");
    ws.update(cx, |ws, cx| {
        ws.chat.items = vec![
            Item::You("Tighten the low end".into()),
            Item::Words("The **kick** and bass clash.\n\nI cut the bass at 60 Hz.".into()),
            Item::End {
                seconds: 3.0,
                steps: 1,
                stopped: false,
                error: None,
            },
        ];
        ws.keep_chat();
        cx.notify();
    });
    cx.run_until_parked();
    let pasted = |cx: &mut VisualTestContext| {
        cx.read_from_clipboard()
            .and_then(|c| c.text())
            .unwrap_or_default()
    };
    // The whole answer, from the end of the turn: its button shows with the pointer there.
    let hover = |cx: &mut VisualTestContext, name: &'static str| {
        let at = cx.debug_bounds(name).unwrap().center();
        cx.simulate_mouse_move(at, None, Modifiers::none());
        cx.run_until_parked();
    };
    hover(cx, "end-2");
    click(cx, "copy-answer-2");
    assert!(pasted(cx).contains("I cut the bass at 60 Hz."));
    // What was asked.
    hover(cx, "you-0");
    click(cx, "copy-you-0");
    assert_eq!(pasted(cx), "Tighten the low end");
    // The whole conversation, from the header's menu.
    click(cx, "agent-more");
    pick(cx, &ws, "Copy conversation");
    cx.run_until_parked();
    let all = pasted(cx);
    assert!(
        all.contains("Tighten the low end") && all.contains("60 Hz"),
        "{all}"
    );

    // Dragged across the words, ⌘C takes what's selected, over paragraphs, with its marks off.
    let (from, to) = ws.read_with(cx, |ws, _| {
        let shown = ws.chat.shown.borrow();
        let a = shown[1].bounds();
        let b = shown[2].bounds();
        (
            a.origin + point(px(1.), px(2.)),
            b.origin + point(b.size.width, b.size.height - px(1.)),
        )
    });
    drag(cx, from, to);
    let picked = ws
        .read_with(cx, |ws, _| ws.chat.picked_text())
        .expect("selected");
    assert!(
        picked.starts_with("The kick") && picked.contains("\n\nI cut"),
        "{picked}"
    );
    cx.simulate_keystrokes("cmd-c");
    cx.run_until_parked();
    assert_eq!(pasted(cx), picked);
    // A click lets it go.
    cx.simulate_click(from, Modifiers::none());
    cx.run_until_parked();
    assert!(ws.read_with(cx, |ws, _| ws.chat.picked_text()).is_none());
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn the_genre_and_the_songs_facts_never_run_over_each_other(cx: &mut TestAppContext) {
    let (ws, cx, dir) = open_agent(cx, "agent-facts");
    ws.update(cx, |ws, cx| {
        let p = &mut ws.session.project;
        p.genre = Some("progressive melodic deep house".into());
        p.tempo = 126.0;
        ws.panel_w = 0.0;
        cx.notify();
    });
    cx.run_until_parked();
    let (genre, facts) = (
        cx.debug_bounds("agent-genre").unwrap(),
        cx.debug_bounds("agent-facts").unwrap(),
    );
    assert!(genre.right() <= facts.origin.x, "{genre:?} {facts:?}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn a_plan_ticks_along_where_it_is(cx: &mut TestAppContext) {
    use crate::agent::Event;
    let (ws, cx, dir) = open_agent(cx, "agent-plan");
    let plan = |state: &str| {
        json!({
            "kind": "plan", "id": "main", "title": "Building the drop",
            "steps": [{ "text": "Drums", "state": state }, { "text": "Bass", "state": "todo" }],
        })
    };
    ws.update(cx, |ws, cx| {
        ws.chat.apply(Event::Asked {
            text: "Build the drop".into(),
        });
        for (id, state) in [("a", "doing"), ("b", "done")] {
            ws.chat.apply(Event::ToolStart {
                id: id.into(),
                name: "mcp__pina__present".into(),
            });
            ws.chat.apply(Event::Tool {
                id: id.into(),
                name: "mcp__pina__present".into(),
                input: plan(state),
            });
            ws.chat.apply(Event::Text {
                text: "and ".into(),
            });
        }
        cx.notify();
    });
    cx.run_until_parked();
    ws.read_with(cx, |ws, _| {
        let cards: Vec<_> = ws
            .chat
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Tool { input, .. } => Some(input),
                _ => None,
            })
            .collect();
        assert_eq!(cards.len(), 1, "one plan, not a plan a step");
        assert_eq!(cards[0]["steps"][0]["state"], "done");
    });
    assert!(cx.debug_bounds("card-plan-1").is_some());
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn a_choice_is_made_by_clicking_and_only_once(cx: &mut TestAppContext) {
    use crate::agent::{Kind, Readiness};
    let (ws, cx, dir) = open_agent(cx, "agent-choice");
    let choices = json!({
        "kind": "choices", "title": "Which direction?",
        "options": [
            { "label": "Darker", "detail": "Minor, slower", "ask": "Go darker" },
            { "label": "Brighter" },
        ],
    });
    ws.update(cx, |ws, cx| {
        // Nothing to start: the question's still what's being tested.
        ws.chat.ready = vec![(Kind::ClaudeCode, Readiness::SignIn)];
        ws.chat.items = vec![Item::You("Make a song".into()), card("c", choices)];
        cx.notify();
    });
    cx.run_until_parked();
    click(cx, "choice-1-0");
    ws.read_with(cx, |ws, _| {
        assert!(
            ws.chat
                .items
                .iter()
                .any(|i| *i == Item::You("Go darker".into()))
        );
    });
    // Answered, it can't be answered again.
    let count = ws.read_with(cx, |ws, _| ws.chat.items.len());
    click(cx, "choice-1-1");
    assert_eq!(ws.read_with(cx, |ws, _| ws.chat.items.len()), count);
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn cards_show_what_was_measured_and_go_where_they_point(cx: &mut TestAppContext) {
    let (ws, cx, dir) = open_agent(cx, "agent-cards");
    ws.update(cx, |ws, cx| {
        ws.chat.items = vec![
            Item::You("How's it sound?".into()),
            card(
                "m",
                json!({ "kind": "metrics", "metrics": [
                    { "label": "Loudness", "value": "-9.2 LUFS", "target": "-10", "state": "warn" },
                ]}),
            ),
            card(
                "c",
                json!({ "kind": "chords", "key": "A minor", "chords": [
                    { "name": "Am", "numeral": "i" }, { "name": "F", "numeral": "VI" },
                ]}),
            ),
            card(
                "s",
                json!({ "kind": "sections", "sections": [
                    { "name": "Intro", "bars": 4, "energy": 0.3 },
                    { "name": "Drop", "bars": 8, "energy": 1.0 },
                ]}),
            ),
            // One it doesn't know is left out, not a crash.
            card("x", json!({ "kind": "hologram" })),
            card(
                "n",
                json!({ "kind": "next", "options": [{ "label": "Add a bridge" }] }),
            ),
        ];
        cx.notify();
    });
    cx.run_until_parked();
    for shown in [
        "card-metrics-1",
        "card-chords-2",
        "card-sections-3",
        "next-5-0",
    ] {
        assert!(cx.debug_bounds(shown).is_some(), "{shown}");
    }
    // A section goes to where it starts: the second, after the first's four bars.
    click(cx, "section-3-1");
    let bar = ws.read_with(cx, |ws, _| ws.session.project.bar());
    assert_eq!(ws.read_with(cx, |ws, _| ws.playhead()), bar * 4);
    // Chords can be heard.
    click(cx, "chords-hear-2");
    settle(cx, &ws, |ws| ws.browser.midi_playing.is_some());
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn a_turn_says_what_it_changed_and_can_be_taken_back(cx: &mut TestAppContext) {
    use crate::agent::Event;
    let (ws, cx, dir) = open_agent(cx, "agent-changes");
    ws.update(cx, |ws, cx| {
        ws.chat.apply(Event::Asked {
            text: "Faster".into(),
        });
        ws.chat.before = Some(ws.session.project.clone());
        ws.session.project.tempo += 20.0;
        ws.heard_claude(
            Event::Done {
                seconds: 2.0,
                stopped: false,
                error: None,
            },
            cx,
        );
    });
    cx.run_until_parked();
    let changes = ws.read_with(cx, |ws, _| {
        ws.chat
            .items
            .iter()
            .position(|i| matches!(i, Item::Changes { .. }))
    });
    let n = changes.expect("a card");
    assert!(
        cx.debug_bounds(Box::leak(format!("changes-{n}").into_boxed_str()))
            .is_some()
    );
    let tempo = ws.read_with(cx, |ws, _| ws.session.project.tempo);
    click(cx, "undo-turn");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tempo),
        tempo - 20.0
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn a_run_of_steps_folds_to_a_line_and_opens(cx: &mut TestAppContext) {
    let (ws, cx, dir) = open_agent(cx, "agent-steps");
    let tool = |id: &str, op: &str, step: Step| Item::Tool {
        id: id.into(),
        name: "mcp__pina__edit".into(),
        input: json!({ "command": { "op": op } }),
        step,
    };
    ws.update(cx, |ws, cx| {
        ws.chat.items = vec![
            Item::You("Do it".into()),
            tool("1", "add_track", Step::Done),
            tool("2", "set_tempo", Step::Failed("no".into())),
        ];
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("activity-1").is_some());
    assert!(ws.read_with(cx, |ws, _| !ws.chat.opened.contains(&1)));
    click(cx, "activity-1");
    assert!(ws.read_with(cx, |ws, _| ws.chat.opened.contains(&1)));
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[gpui::test]
fn a_move_is_asked_of_the_studio_agent_in_a_click(cx: &mut TestAppContext) {
    let (claude, log) = stand_in("move");
    cx.executor().allow_parking();
    let (ws, cx, dir) = open_agent(cx, "agent-move");
    ws.update(cx, |ws, cx| {
        ws.chat.command = Some(claude.clone());
        ws.open_genres(Some("house"), cx);
    });
    cx.run_until_parked();
    click(cx, "move-do");
    assert!(ws.read_with(cx, |ws, _| ws.view == View::Arrange && ws.show_agent));
    settle_agent(&ws, cx);
    let asked = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(asked.contains("Using my House playbook:"), "{asked}");
    std::fs::remove_dir_all(dir).unwrap();
}
