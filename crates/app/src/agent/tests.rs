use super::*;

#[test]
fn codex_gets_tempos_tools_approved_and_nothing_else() {
    let home = std::env::temp_dir().join(format!("pina-test-codex-mcp-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join("config.toml"),
        "model = \"x\"\n[mcp_servers.node_repl]\ncommand = \"n\"\n[mcp_servers.node_repl.env]\nA = \"1\"\n[mcp_servers.\"computer-use\"]\ncommand = \"c\"\n[mcp_servers.pina]\ncommand = \"t\"\n",
    )
    .unwrap();
    // Read where Codex keeps its config: the servers it sets up, once each.
    let servers = {
        let _home = CodexHome::set(&home);
        codex_servers()
    };
    assert_eq!(servers, ["node_repl", "computer-use", "pina"]);
    let args = codex_isolation(Path::new("/A/pina"), &servers).join(" ");
    assert!(args.contains(r#"mcp_servers.pina={command="/A/pina",args=["mcp"],default_tools_approval_mode="approve"}"#), "{args}");
    assert!(args.contains("mcp_servers.node_repl.enabled=false"));
    assert!(args.contains("mcp_servers.computer-use.enabled=false"));
    assert!(!args.contains("pina.enabled"));
    assert!(
        args.contains("features.plugins=false") && args.contains("features.computer_use=false")
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// CODEX_HOME pointed somewhere for a test, and back after; one test at a time.
struct CodexHome {
    _held: std::sync::MutexGuard<'static, ()>,
}

static CODEX_HOME: Mutex<()> = Mutex::new(());

impl CodexHome {
    fn set(dir: &Path) -> CodexHome {
        let guard = CODEX_HOME.lock().unwrap_or_else(|e| e.into_inner());
        unsafe { std::env::set_var("CODEX_HOME", dir) };
        CodexHome { _held: guard }
    }
}

impl Drop for CodexHome {
    fn drop(&mut self) {
        unsafe { std::env::remove_var("CODEX_HOME") };
    }
}

#[test]
fn codex_says_its_signed_in_on_either_stream() {
    let ready = |s: &str| codex_status(true, s);
    assert_eq!(
        ready("Logged in using ChatGPT\n"),
        Readiness::Ready("ChatGPT".into())
    );
    assert_eq!(
        ready("WARNING: something\nLogged in using an API key - sk-…\n"),
        Readiness::Ready("an API key - sk-…".into())
    );
    assert_eq!(codex_status(false, "Not logged in\n"), Readiness::SignIn);
    assert_eq!(codex_status(true, ""), Readiness::Ready(String::new()));
    assert_eq!(codex_status(false, "error"), Readiness::SignIn);
}

#[test]
fn models_and_efforts_are_picked_kept_and_passed_on() {
    // The flags: Claude Code's model, effort and the conversation carried on; Codex's.
    let t = Tuning {
        model: Some("opus".into()),
        effort: Some("high".into()),
    };
    let args = tuning_args(Kind::ClaudeCode, &t, Some("s-1"));
    assert_eq!(
        args,
        ["--model", "opus", "--effort", "high", "--resume", "s-1"]
    );
    let args = tuning_args(Kind::Codex, &t, None);
    assert_eq!(
        args,
        ["-m", "opus", "-c", "model_reasoning_effort=\"high\""]
    );
    assert!(tuning_args(Kind::ClaudeCode, &Tuning::default(), None).is_empty());
    // Claude's first line names its conversation, to carry it on in another model.
    let init = json!({"type": "system", "subtype": "init", "model": "m", "session_id": "s-1"});
    assert_eq!(claude_events(&init).1.as_deref(), Some("s-1"));
    assert_eq!(Kind::ClaudeCode.models()[1].id, "opus");
    assert!(
        Kind::ClaudeCode
            .efforts(Some("opus"))
            .contains(&"max".to_string())
    );

    // Codex's from its own list: those it shows, in its order, each with its efforts; its
    // default from its config.
    let home = std::env::temp_dir().join(format!("pina-test-codex-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let list = json!({"models": [
        {"slug": "b", "display_name": "B", "visibility": "list", "priority": 9,
         "supported_reasoning_levels": [{"effort": "low"}, {"effort": "high"}]},
        {"slug": "hidden", "display_name": "H", "visibility": "hide", "priority": 1,
         "supported_reasoning_levels": []},
        {"slug": "a", "display_name": "A", "visibility": "list", "priority": 2,
         "supported_reasoning_levels": [{"effort": "medium"}, {"effort": "xhigh"}]}
    ]});
    std::fs::write(home.join("models_cache.json"), list.to_string()).unwrap();
    std::fs::write(
        home.join("config.toml"),
        "model = \"b\"\n[x]\nmodel = \"z\"\n",
    )
    .unwrap();
    let codex_home = CodexHome::set(&home);
    let names: Vec<String> = Kind::Codex.models().into_iter().map(|m| m.name).collect();
    assert_eq!(names, ["A", "B"]);
    assert_eq!(Kind::Codex.default_model().as_deref(), Some("b"));
    assert_eq!(Kind::Codex.efforts(None), ["low", "high"]);
    assert_eq!(Kind::Codex.efforts(Some("a")), ["medium", "xhigh"]);
    assert_eq!(model_name(Kind::Codex, "a"), "A");
    assert_eq!(effort_name("xhigh"), "Extra high");
    drop(codex_home);
    let _ = std::fs::remove_dir_all(&home);

    // Kept for each agent.
    tune(Kind::Codex, &t);
    assert_eq!(tuning(Kind::Codex), t);
    assert_eq!(tuning(Kind::ClaudeCode), Tuning::default());
    let _ = std::fs::remove_file(tuning_file());
}

#[test]
fn claudes_stream_becomes_events() {
    let line = |s: &str| claude_line(&serde_json::from_str(s).unwrap());
    assert_eq!(
        line(r#"{"type":"system","subtype":"init","model":"claude-opus-5-5"}"#),
        [Event::Started {
            model: "claude-opus-5-5".into()
        }]
    );
    assert_eq!(
        line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Hi"}}}"#
        ),
        [Event::Text { text: "Hi".into() }]
    );
    assert_eq!(
        line(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"t1","name":"mcp__pina__edit","input":{}}}}"#
        ),
        [Event::ToolStart {
            id: "t1".into(),
            name: "mcp__pina__edit".into()
        }]
    );
    assert_eq!(
        line(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":[{"type":"text","text":"no such track"}]}]}}"#
        ),
        [Event::ToolDone {
            id: "t1".into(),
            ok: false,
            output: "no such track".into()
        }]
    );
    assert_eq!(
        line(r#"{"type":"result","subtype":"success","is_error":false,"duration_ms":11449}"#),
        [Event::Done {
            seconds: 11.449,
            stopped: false,
            error: None
        }]
    );
    assert_eq!(
        line(
            r#"{"type":"result","subtype":"error_during_execution","is_error":true,"duration_ms":6000}"#
        ),
        [Event::Done {
            seconds: 6.0,
            stopped: true,
            error: None
        }]
    );
    assert!(line(r#"{"type":"rate_limit_event"}"#).is_empty());
}

#[test]
fn codexs_stream_becomes_the_same_events() {
    let codex = |s: &str| codex_events(&serde_json::from_str(s).unwrap());
    let (e, id) = codex(r#"{"type":"thread.started","thread_id":"01a1"}"#);
    assert_eq!((e.len(), id.as_deref()), (1, Some("01a1")));
    let (e, _) = codex(
        r#"{"type":"item.started","item":{"id":"item_2","type":"mcp_tool_call","server":"pina","tool":"edit","arguments":{"command":{"op":"set_tempo","bpm":101}},"status":"in_progress"}}"#,
    );
    assert!(matches!(&e[1], Event::Tool { name, input, .. }
        if name == "mcp__pina__edit" && input["command"]["bpm"] == 101));
    let (e, _) = codex(
        r#"{"type":"item.completed","item":{"id":"item_2","type":"mcp_tool_call","server":"pina","tool":"edit","status":"failed","error":{"message":"no such track"}}}"#,
    );
    assert_eq!(
        e,
        [Event::ToolDone {
            id: "item_2".into(),
            ok: false,
            output: "no such track".into()
        }]
    );
    let (e, _) = codex(r#"{"type":"turn.failed","error":{"message":"401 Unauthorized"}}"#);
    assert!(matches!(&e[0], Event::Done { error: Some(m), .. } if m == "401 Unauthorized"));
}

#[test]
fn a_song_not_saved_yet_runs_in_the_nearest_folder_there_is() {
    let tmp = std::env::temp_dir();
    assert_eq!(workdir(&tmp.join("pina-not-made-yet/Untitled")), tmp);
    assert_eq!(workdir(&tmp), tmp);
}

#[test]
fn agents_are_known_by_modisas_ids() {
    assert_eq!(Kind::from_id("codex"), Some(Kind::Codex));
    assert_eq!(Kind::from_id("claude-code\n"), Some(Kind::ClaudeCode));
    assert_eq!(Kind::Codex.def().logo, '\u{f5a01}');
    assert_eq!(Kind::from_id("opencode"), None);
}

#[test]
fn tool_calls_read_as_what_they_do() {
    let edit = json!({ "project": "/p", "command": {
        "op": "add_midi_region", "track": 2, "start": 30720, "length": 15360,
        "notes": [{}, {}, {}] } });
    assert_eq!(
        label("mcp__pina__edit", &edit, 3840),
        (
            "Add MIDI region".into(),
            "track 2 · bar 9 · 4 bars · 3 notes".into()
        )
    );
    let set = json!({ "command": { "op": "set_track", "track": 1, "volume_db": -5 } });
    assert_eq!(
        label("mcp__pina__edit", &set, 3840),
        ("Set track 1".into(), "volume db -5".into())
    );
    let put = json!({ "path": "/s/Kick 01.wav", "project": "/p" });
    assert_eq!(
        label("mcp__pina__samples_put", &put, 3840),
        ("Putting in".into(), "Kick 01".into())
    );
}

#[test]
fn an_agents_usual_errors_say_what_happened_and_what_to_do() {
    let codex = |e: &str| Kind::Codex.explain(e).unwrap_or_default();
    // What Codex says when it can't reach OpenAI.
    assert!(codex("workspace routing discovery failed").contains("couldn't reach OpenAI"));
    assert!(codex("Connection failed: error sending request").contains("couldn't reach OpenAI"));
    assert!(codex("unexpected status 401 Unauthorized").contains("codex login"));
    assert!(codex("You've hit your usage limit").contains("usage limit"));
    let claude = Kind::ClaudeCode
        .explain("API Error: 529 Overloaded")
        .unwrap();
    assert!(claude.contains("couldn't reach Anthropic"), "{claude}");
    // Anything else is shown as it was said.
    assert_eq!(Kind::Codex.explain("the song folder is read-only"), None);
}

#[test]
fn an_auxs_limiter_hears_only_what_reaches_it() {
    use pina_engine::song::{
        commands::ChannelRef,
        project::{AuxSend, Content, Instrument, Note, PPQ, Project, Region, Track},
    };
    let synth = |name: &str| Track {
        name: name.into(),
        instrument: Some(Instrument::Synth),
        regions: vec![Region {
            start: 0,
            length: PPQ * 4,
            content: Content::Midi {
                notes: vec![Note {
                    start: 0,
                    length: PPQ * 2,
                    pitch: 69,
                    velocity: 100,
                }],
                controls: Vec::new(),
            },
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut p = Project::new(48_000);
    let bus = p.add_aux();
    let mut sending = synth("sends");
    sending.sends = vec![AuxSend {
        bus,
        level_db: -6.0,
        pre: false,
    }];
    p.tracks = vec![sending, synth("doesn't")];
    p.assign_ids();
    let dir = std::env::temp_dir().join(format!("pina-test-aux-limit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let peak = |p: &Project| {
        let (heard, range) = produce::limit_heard(p, ChannelRef::Aux(0));
        let [l, _] = crate::cli::rendered(&heard, &dir, range).unwrap();
        l.iter().fold(0f32, |m, x| m.max(x.abs()))
    };
    // The send, 6 dB down, and nothing of either track's own output: with no send, silence.
    let sent = peak(&p);
    let mut none = p.clone();
    none.tracks[0].sends.clear();
    assert!(peak(&none) < 1e-6, "a track that doesn't send isn't heard");
    // Against one track's own level: the two of them, alike, through the master, halved.
    let [both, _] = crate::cli::rendered(
        &produce::limit_heard(&none, ChannelRef::Master).0,
        &dir,
        None,
    )
    .unwrap();
    let own = both.iter().fold(0f32, |m, x| m.max(x.abs())) / 2.0;
    assert!(
        (sent / own - 0.5).abs() < 0.05,
        "sent {sent}, a track's own {own}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
