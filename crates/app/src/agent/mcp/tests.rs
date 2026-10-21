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
            "samples_library"
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
