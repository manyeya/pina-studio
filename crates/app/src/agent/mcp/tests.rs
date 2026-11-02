use super::*;

#[test]
fn speaks_mcp() {
    let init = answer(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-06-18"}}))
    .unwrap();
    assert_eq!(init["result"]["serverInfo"]["name"], "pina");
    assert_eq!(
        answer(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"})),
        None
    );

    let list = answer(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).unwrap();
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "describe",
            "show",
            "edit",
            "analyze",
            "listen",
            "render",
            "bounce",
            "critique",
            "study",
            "playbook",
            "theory",
            "reference",
            "sounds",
            "mix",
            "learn",
            "master",
            "transport",
            "plugins",
            "params",
            "presets",
            "samples_find",
            "samples_similar",
            "samples_kit",
            "samples_put",
            "palette",
            "samples_library",
            "midi_find",
            "midi_group",
            "present",
            "midi_write",
            "midi_study",
            "reference_study",
            "reference_create",
            "reference_compare"
        ]
    );

    // Build a little project on disk through the tools, then measure it.
    let dir = std::env::temp_dir().join(format!("pina-test-mcp-{}", std::process::id()));
    let tool = |id: u64, name: &str, args: Value| {
        let r = answer(&json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {"name": name, "arguments": args}}))
        .unwrap();
        let text = r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string();
        (text, r["result"]["isError"].as_bool().unwrap())
    };
    let project = dir.to_str().unwrap();
    let (text, err) = tool(
        3,
        "edit",
        json!({"project": project,
        "command": {"op": "add_track", "name": "Keys", "instrument": "synth"}}),
    );
    assert!(!err, "{text}");
    let (_, err) = tool(
        4,
        "edit",
        json!({"project": project, "command": {"op": "add_midi_region",
        "track": 0, "start": 0, "length": 960,
        "notes": [{"start": 0, "length": 960, "pitch": 69, "velocity": 100}]}}),
    );
    assert!(!err);
    // MIDI written like a group, heard against it, and the song left alone.
    let write = |id, name: &str, notes: &str, like: Value| {
        tool(
            id,
            "midi_write",
            json!({"project": project, "name": name, "tempo": 120, "key": "A minor",
                "parts": [{"name": "Stabs", "notes": notes}], "like": like}),
        )
    };
    let stabs = "1.3 A3 1 100\n1.7 C4 1 90\n1.11 E4 1 100\n1.15 A3 1 90";
    let (text, err) = write(30, "One", stabs, json!([]));
    assert!(!err, "{text}");
    let one: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(one["bars"], 1);
    let first = one["file"].as_str().unwrap().to_string();
    assert!(first.ends_with("MIDI/One.mid"), "{first}");
    // The same name doesn't write over it.
    let (text, _) = write(31, "One", stabs, json!([first]));
    let two: Value = serde_json::from_str(&text).unwrap();
    assert!(
        two["file"].as_str().unwrap().ends_with("MIDI/One 2.mid"),
        "{two}"
    );
    assert!(two["likeness"]["score"].as_f64().unwrap() > 0.95, "{two}");
    let (text, err) = tool(
        32,
        "midi_group",
        json!({"files": [first, two["file"]], "candidate": two["file"]}),
    );
    assert!(!err, "{text}");
    let heard: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(heard["shared"]["files"], 2);
    assert_eq!(heard["shared"]["steps"][2], 1.0);
    assert!(
        heard["likeness"]["rhythm"].as_f64().unwrap() > 0.99,
        "{heard}"
    );
    // Its name doesn't make it drums.
    let (text, _) = write(34, "Off Beat Stabs", stabs, json!([]));
    let named: Value = serde_json::from_str(&text).unwrap();
    assert!(
        !named["summary"].as_str().unwrap().contains("drums"),
        "{named}"
    );
    let (text, err) = write(33, "Bad", "not a note", json!([]));
    assert!(err && text.contains("line 1"), "{text}");
    let (text, err) = tool(5, "analyze", json!({"path": project}));
    assert!(!err, "{text}");
    let measured: Value = serde_json::from_str(&text).unwrap();
    assert!(
        measured["mix"]["lufs"].as_f64().unwrap() > -40.0,
        "{measured}"
    );
    assert_eq!(measured["tracks"][0]["name"], "Keys");
    let (text, err) = tool(
        6,
        "edit",
        json!({"project": project, "command": {"op": "delete_track", "track": 5}}),
    );
    assert!(err && text.contains("no track 5"));
    let (text, err) = tool(
        7,
        "transport",
        json!({"project": project, "action": "play"}),
    );
    assert!(err && text.contains("doesn't have"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn playbooks_are_used_only_when_the_person_asks() {
    // Nothing tells an agent to start from one: only to use one when asked.
    for text in [INSTRUCTIONS, produce::WORKFLOW] {
        assert!(text.contains("only when"), "{text}");
        assert!(!text.contains("playbook <genre>") && !text.contains("`playbook` for"));
    }
    let list = answer(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).unwrap();
    let tool = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "playbook")
        .unwrap()
        .clone();
    assert!(
        tool["description"]
            .as_str()
            .unwrap()
            .contains("only when the person asks")
    );
}

#[test]
fn agents_are_told_to_write_the_music_themselves() {
    let prompt = crate::agent::commands::instructions(std::path::Path::new("/song"));
    for text in [INSTRUCTIONS, produce::WORKFLOW, prompt.as_str()] {
        assert!(text.contains("yourself"), "{text}");
        assert!(
            !text.contains("write_") && !text.contains("arrange"),
            "{text}"
        );
    }
}
