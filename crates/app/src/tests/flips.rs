//! Flips: the Studio Agent's engine (a stand-in here, answering as Claude Code does) turning a
//! region's lyrics into hooks, each shown as it's written, heard, and used.

use super::*;
use crate::{agent::Kind, ui::menu::MenuAction};
use pina_engine::lyrics::{Line, Word};
use pina_engine::song::project::{At, Instrument};

/// A stand-in for Claude Code: it reads the question, then (after `wait` seconds) says `said`
/// in two pieces half a second apart, the second ending its answer; or, `failing`, says that on
/// stderr and ends.
pub(super) fn engine(name: &str, said: &str, wait: f32, failing: Option<&str>) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pina-flips-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let event = |text: &str| {
        serde_json::json!({"type":"stream_event","event":{"type":"content_block_delta","index":1,
            "delta":{"type":"text_delta","text":text}}})
        .to_string()
    };
    let half = said.len() / 2;
    let script = match failing {
        Some(e) => format!("#!/bin/sh\ncat > /dev/null\necho '{e}' >&2\nexit 1\n"),
        None => format!(
            "#!/bin/sh\necho \"$@\" > \"$(dirname \"$0\")/args\"\ncat > \"$(dirname \"$0\")/asked\"\n\
             echo '{{\"type\":\"system\",\"subtype\":\"init\",\"model\":\"claude-test-1\"}}'\n\
             echo '{{\"type\":\"stream_event\",\"event\":{{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{{\"type\":\"thinking\"}}}}}}'\n\
             sleep {wait}\nprintf '%s\\n' '{}'\nsleep 0.5\nprintf '%s\\n' '{}'\n\
             echo '{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"duration_ms\":900}}'\n",
            event(&said[..half]),
            event(&said[half..]),
        ),
    };
    let file = dir.join("claude");
    std::fs::write(&file, script).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    file
}

/// The demo's drums with lyrics heard before (a hook, sung twice), shown in the Audio editor,
/// a tall one; the region's place.
fn with_lyrics<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
) -> (Entity<Workspace>, &'a mut VisualTestContext, At, PathBuf) {
    let _ = test_whisper();
    let dir = demo_copy(name);
    let kept = dir.join("Audio Files/Lyrics/drums · Whisper small.json");
    std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
    let word = |text: &str, start: f32, end: f32| Word {
        text: text.into(),
        start,
        end,
    };
    let line = |start: f32| Line {
        text: "Ain't no holding back".into(),
        start,
        end: start + 1.3,
        sung: 2,
        words: vec![
            word("Ain't", start, start + 0.3),
            word("no", start + 0.3, start + 0.5),
            word("holding", start + 0.5, start + 0.9),
            word("back", start + 0.9, start + 1.3),
        ],
    };
    std::fs::write(
        &kept,
        serde_json::to_string(&[line(0.5), line(2.5)]).unwrap(),
    )
    .unwrap();
    cx.executor().allow_parking();
    let (ws, cx) = open_at(cx, &dir);
    let drums = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        p.tracks.iter().position(|t| t.name == "Drums").unwrap()
    });
    let at = (drums, 0);
    ws.update(cx, |ws, cx| {
        ws.select_track(drums);
        ws.region = Some(at);
        ws.editor_h = 760.0;
        ws.show_bottom(Bottom::Audio, cx);
        ws.hear_lyrics(at, cx);
    });
    until(cx, &ws, |ws| ws.lyrics.is_some());
    cx.run_until_parked();
    (ws, cx, at, dir)
}

/// Waits (drawing as it goes) until `f`, or fails after 10 s.
fn waits(cx: &mut VisualTestContext, ws: &Entity<Workspace>, f: impl Fn(&Workspace) -> bool) {
    until(cx, ws, f);
    cx.run_until_parked();
}

pub(super) const SAID: &str = concat!(
    r#"{"name":"Hold Back","style":"chop","says":"holding back, no","why":"It lands on the one.","bars":1,"pitch":5,"hits":[{"from":2,"to":3,"beat":0,"beats":2},{"from":1,"to":1,"beat":2,"beats":0.5}]}"#,
    "\n",
    r#"{"name":"No No","style":"stutter","says":"no no, aint no holding back","why":"It climbs.","bars":2,"pitch":0,"hits":[{"from":1,"to":1,"beat":0,"beats":0.25},{"from":1,"to":1,"beat":0.25,"beats":0.25},{"from":0,"to":3,"beat":1,"beats":3}]}"#,
    "\n",
);

#[gpui::test]
fn the_studio_agent_flips_the_words_into_hooks_shown_as_theyre_written_heard_and_used(
    cx: &mut TestAppContext,
) {
    let (ws, cx, _, dir) = with_lyrics(cx, "flips");
    let claude = engine("found", SAID, 0.3, None);
    ws.update(cx, |ws, _| {
        ws.flips.command = Some(claude.clone());
        ws.flips.kind = Some(Kind::ClaudeCode);
    });
    click_on(cx, "lyrics-tab-flips", Modifiers::none(), MouseButton::Left);
    for what in [
        "flips-engine",
        "flips-tuning",
        "flips-style",
        "flips-vibe",
        "flips-find",
    ] {
        assert!(cx.debug_bounds(what).is_some(), "{what}");
    }
    // Its own model and effort for flips (the Studio Agent's conversation keeps its own).
    ws.update(cx, |ws, cx| {
        ws.choose(MenuAction::FlipModel(Some("haiku".into())), cx);
        ws.choose(MenuAction::FlipEffort(Some("low".into())), cx);
    });
    let (flips, chat) = ws.update(cx, |ws, _| (ws.flip_tuning(), ws.chat.tuning.clone()));
    assert_eq!(
        (flips.model.as_deref(), flips.effort.as_deref()),
        (Some("haiku"), Some("low"))
    );
    assert_ne!(chat.model.as_deref(), Some("haiku"));
    // Asked: it says what it's doing while it works, each flip showing as it's written.
    click_on(cx, "flips-find", Modifiers::none(), MouseButton::Left);
    assert!(
        cx.debug_bounds("flips-finding").is_some(),
        "the engine at work, shown"
    );
    assert!(
        cx.debug_bounds("flips-stop").is_some() && cx.debug_bounds("flips-find").is_none(),
        "Find is Stop while it works"
    );
    waits(cx, &ws, |ws| {
        ws.flips.now.as_ref().is_some_and(|f| !f.ideas.is_empty())
    });
    let still = ws.read_with(cx, |ws, _| ws.flips.now.as_ref().unwrap().asking.is_some());
    assert!(still, "the first flip shows before the answer's done");
    assert!(cx.debug_bounds("flip-0").is_some());
    assert!(cx.debug_bounds("flips-finding").is_some());
    waits(cx, &ws, |ws| {
        ws.flips.now.as_ref().is_some_and(|f| f.asking.is_none())
    });
    assert!(cx.debug_bounds("flips-finding").is_none());
    // Asked with the model and effort picked.
    let args = std::fs::read_to_string(claude.with_file_name("args")).unwrap();
    assert!(
        args.contains("--model haiku") && args.contains("--effort low"),
        "{args}"
    );
    let (ideas, status) = ws.read_with(cx, |ws, _| {
        (
            ws.flips.now.as_ref().unwrap().ideas.clone(),
            ws.status.clone(),
        )
    });
    assert_eq!(ideas.len(), 2);
    assert!(status.unwrap().contains("found 2 flips"));
    // "holding back" (words 2 and 3), up five: in the recording's seconds.
    let h = &ideas[0].hits[0];
    assert_eq!((h.from, h.to, h.words.as_str()), (1.0, 1.8, "holding back"));
    assert_eq!(ideas[0].pitch, 5);
    // Heard: it loops, lit; again, it stops.
    click_on(cx, "flip-hear-0", Modifiers::none(), MouseButton::Left);
    let playing = |cx: &mut VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            ws.flips.now.as_ref().unwrap().playing.map(|p| p.0)
        })
    };
    assert_eq!(playing(cx), Some(0));
    click_on(cx, "flip-hear-0", Modifiers::none(), MouseButton::Left);
    assert_eq!(playing(cx), None);
    // Used: a Sampler track of its own, its chops on it, looping, open to flip further.
    let tracks = ws.read_with(cx, |ws, _| ws.session.project.tracks.len());
    click_on(cx, "flip-use-0", Modifiers::none(), MouseButton::Left);
    ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        assert_eq!(p.tracks.len(), tracks + 1);
        let t = &p.tracks[ws.track];
        assert_eq!(t.name, "Hold Back");
        let Some(Instrument::Sampler(s)) = &t.instrument else {
            panic!("no sampler")
        };
        assert_eq!((s.zones.len(), s.voice.transpose), (2, 5.0));
        assert!(t.regions[0].looped);
        assert_eq!(ws.bottom, Some(Bottom::Sampler));
    });
    // One undo takes it back.
    ws.update(cx, |ws, cx| ws.undo(cx));
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks.len()),
        tracks
    );
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(claude.parent().unwrap());
}

#[gpui::test]
fn an_engine_that_cant_answer_says_why_in_plain_words_and_finding_can_be_stopped(
    cx: &mut TestAppContext,
) {
    let (ws, cx, _, dir) = with_lyrics(cx, "flips-fail");
    let signed_out = engine("signed-out", "", 0.0, Some("Error: 401 Unauthorized"));
    ws.update(cx, |ws, cx| {
        ws.flips.command = Some(signed_out.clone());
        ws.flips.kind = Some(Kind::ClaudeCode);
        ws.find_flips(false, cx);
    });
    waits(cx, &ws, |ws| {
        ws.flips.now.as_ref().is_some_and(|f| f.asking.is_none())
    });
    let error = ws.read_with(cx, |ws, _| ws.flips.now.as_ref().unwrap().error.clone());
    assert!(
        error.unwrap().contains("isn't signed in"),
        "explained, not the raw error"
    );
    assert!(cx.debug_bounds("flips-error").is_some());
    // A slow one, stopped: no error, nothing found, the finding gone.
    let slow = engine("slow", SAID, 30.0, None);
    ws.update(cx, |ws, cx| {
        ws.flips.command = Some(slow.clone());
        ws.find_flips(false, cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("flips-finding").is_some());
    click_on(cx, "flips-stop", Modifiers::none(), MouseButton::Left);
    ws.read_with(cx, |ws, _| {
        let f = ws.flips.now.as_ref().unwrap();
        assert!(f.asking.is_none() && f.error.is_none() && f.ideas.is_empty());
        assert!(ws.status.as_deref().unwrap().starts_with("Stopped"));
    });
    assert!(cx.debug_bounds("flips-finding").is_none());
    let _ = std::fs::remove_dir_all(dir);
    for f in [signed_out, slow] {
        let _ = std::fs::remove_dir_all(f.parent().unwrap());
    }
}
