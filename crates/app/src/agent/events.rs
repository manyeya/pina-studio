//! What an agent says, read as it streams: its words, thinking and steps.

use super::*;

/// The events in one line of Claude Code's stream-json output.
pub(crate) fn claude_events(v: &Value) -> (Vec<Event>, Option<String>) {
    let id = (v["type"] == "system" && v["subtype"] == "init")
        .then(|| v["session_id"].as_str().map(String::from))
        .flatten();
    (claude_line(v), id)
}

pub fn claude_line(v: &Value) -> Vec<Event> {
    let s = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let blocks = |v: &Value| {
        v.pointer("/message/content")
            .and_then(Value::as_array)
            .cloned()
    };
    match v["type"].as_str() {
        Some("system") if v["subtype"] == "init" => vec![Event::Started {
            model: s(&v["model"]),
        }],
        Some("stream_event") => {
            let e = &v["event"];
            match (e["type"].as_str(), e["content_block"]["type"].as_str()) {
                (Some("content_block_start"), Some("thinking")) => vec![Event::Thinking],
                (Some("content_block_start"), Some("tool_use")) => vec![Event::ToolStart {
                    id: s(&e["content_block"]["id"]),
                    name: s(&e["content_block"]["name"]),
                }],
                (Some("content_block_delta"), _) if e["delta"]["type"] == "text_delta" => {
                    vec![Event::Text {
                        text: s(&e["delta"]["text"]),
                    }]
                }
                _ => Vec::new(),
            }
        }
        Some("assistant") => blocks(v)
            .unwrap_or_default()
            .iter()
            .filter(|b| b["type"] == "tool_use")
            .map(|b| Event::Tool {
                id: s(&b["id"]),
                name: s(&b["name"]),
                input: b["input"].clone(),
            })
            .collect(),
        Some("user") => blocks(v)
            .unwrap_or_default()
            .iter()
            .filter(|b| b["type"] == "tool_result")
            .map(|b| Event::ToolDone {
                id: s(&b["tool_use_id"]),
                ok: !b["is_error"].as_bool().unwrap_or(false),
                output: match &b["content"] {
                    Value::Array(parts) => parts.iter().map(|p| s(&p["text"])).collect(),
                    other => s(other),
                },
            })
            .collect(),
        Some("result") => {
            // An interrupted answer ends "during execution".
            let stopped = v["subtype"] == "error_during_execution";
            vec![Event::Done {
                seconds: v["duration_ms"].as_f64().unwrap_or(0.0) / 1000.0,
                stopped,
                error: match (v["subtype"].as_str(), v["is_error"].as_bool()) {
                    (Some("success"), Some(false) | None) => None,
                    _ if stopped => None,
                    _ => Some(s(&v["result"]))
                        .filter(|r| !r.is_empty())
                        .or(Some(s(&v["subtype"]))),
                },
            }]
        }
        _ => Vec::new(),
    }
}

/// The events in one line of `codex exec --json`: its items (what it says, thinks and runs, and
/// its MCP tool calls) as they start and finish, and how the turn ended.
pub(crate) fn codex_events(v: &Value) -> (Vec<Event>, Option<String>) {
    let s = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let item = &v["item"];
    let id = s(&item["id"]);
    // An MCP tool's name as Claude Code says it, so it reads the same.
    let name = || match item["type"].as_str() {
        Some("mcp_tool_call") => format!("mcp__{}__{}", s(&item["server"]), s(&item["tool"])),
        _ => "shell".to_string(),
    };
    let events = match (v["type"].as_str(), item["type"].as_str()) {
        (Some("thread.started"), _) => {
            return (
                vec![Event::Started {
                    model: String::new(),
                }],
                Some(s(&v["thread_id"])),
            );
        }
        (Some("item.started"), Some("reasoning")) => vec![Event::Thinking],
        (Some("item.started"), Some("mcp_tool_call" | "command_execution")) => vec![
            Event::ToolStart {
                id: id.clone(),
                name: name(),
            },
            Event::Tool {
                id,
                name: name(),
                input: match item["type"].as_str() {
                    Some("mcp_tool_call") => item["arguments"].clone(),
                    _ => json!({ "command": item["command"] }),
                },
            },
        ],
        (Some("item.completed"), Some("agent_message")) => vec![Event::Text {
            text: format!("{}\n\n", s(&item["text"])),
        }],
        (Some("item.completed"), Some("mcp_tool_call")) => vec![Event::ToolDone {
            id,
            ok: item["status"] == "completed" && item["error"].is_null(),
            output: match item["error"]["message"].as_str() {
                Some(e) => e.to_string(),
                None => item["result"]["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|c| s(&c["text"]))
                    .collect(),
            },
        }],
        (Some("item.completed"), Some("command_execution")) => vec![Event::ToolDone {
            id,
            ok: item["exit_code"].as_i64() == Some(0),
            output: s(&item["aggregated_output"]),
        }],
        (Some("turn.completed"), _) => vec![Event::Done {
            seconds: 0.0,
            stopped: false,
            error: None,
        }],
        (Some("turn.failed"), _) => vec![Event::Done {
            seconds: 0.0,
            stopped: false,
            error: Some(s(&v["error"]["message"])),
        }],
        _ => Vec::new(),
    };
    (events, None)
}

/// A tool call in words: what it's doing, and to what. `bar` is the song's bar in ticks, for
/// saying where.
pub fn label(name: &str, input: &Value, bar: u64) -> (String, String) {
    let tool = name.strip_prefix("mcp__pina__").unwrap_or(name);
    let s = |k: &str| input[k].as_str().unwrap_or_default().to_string();
    let file = |k: &str| {
        Path::new(&s(k))
            .file_stem()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let (what, detail) = match tool {
        "describe" => ("Reading the manual", String::new()),
        "show" => ("Reading the song", String::new()),
        "edit" => return edit_label(&input["command"], bar),
        "analyze" => ("Listening", file("path")),
        "render" => ("Bouncing", file("out")),
        "samples_find" => ("Finding samples", words(&input["query"])),
        "samples_similar" => ("Finding sounds like", file("path")),
        "samples_kit" => ("Building a kit", words(&input["seed"])),
        "samples_put" => ("Putting in", file("path")),
        "samples_library" => ("Looking through the library", String::new()),
        "transport" => ("Transport", s("action")),
        "plugins" => ("Looking at plugins", String::new()),
        "params" => ("Reading a plugin", String::new()),
        "presets" => ("Reading presets", String::new()),
        "critique" => (
            "Listening critically",
            if input["quick"].as_bool() == Some(true) {
                "the notes".into()
            } else {
                "a render".into()
            },
        ),
        "playbook" => ("Reading the playbook", s("genre")),
        "theory" => (
            "Finding chords",
            format!("{} {}", s("key"), s("words")).trim().to_string(),
        ),
        "reference" => ("Measuring a reference", file("file")),
        "sounds" => (
            "Finding sounds",
            format!("{} {}", s("role"), s("words")).trim().to_string(),
        ),
        "mix" => (
            "Mixing",
            input["track"]
                .as_u64()
                .map_or("every part".into(), |t| format!("track {t}")),
        ),
        "master" => (
            "Mastering",
            input["lufs"]
                .as_f64()
                .map_or(String::new(), |l| format!("{l} LUFS")),
        ),
        other => return (other.replace('_', " "), String::new()),
    };
    (what.to_string(), detail)
}

/// A search's or kit's words, whether said as words or as an object with `text`.
pub(crate) fn words(q: &Value) -> String {
    match q {
        Value::String(q) => q.clone(),
        q => q["text"].as_str().unwrap_or_default().to_string(),
    }
}

/// An edit in words: "Set tempo", "bpm 101"; "Add midi region", "track 3 · bar 9 · 24 notes".
pub(crate) fn edit_label(c: &Value, bar: u64) -> (String, String) {
    let op = c["op"].as_str().unwrap_or("edit").replace('_', " ");
    let mut what = op.clone();
    if let Some(first) = what.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    // What it acts on first, then where, then the rest.
    const FIRST: [&str; 9] = [
        "track", "region", "channel", "aux", "name", "start", "tick", "at", "length",
    ];
    let rank = |k: &str| FIRST.iter().position(|f| *f == k).unwrap_or(FIRST.len());
    let mut fields: Vec<(&String, &Value)> = c
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(k, _)| *k != "op")
        .collect();
    fields.sort_by_key(|(k, _)| rank(k));
    // "Set track" and track 1: "Set track 1".
    if let Some((k, Value::Number(n))) = fields.first()
        && op.ends_with(k.as_str())
    {
        what = format!("{what} {n}");
        fields.remove(0);
    }
    let parts: Vec<String> = fields
        .into_iter()
        .filter_map(|(k, v)| match v {
            Value::Number(n) if matches!(k.as_str(), "start" | "tick" | "at") && bar > 0 => {
                n.as_u64().map(|t| format!("bar {}", t / bar + 1))
            }
            Value::Number(n) if k == "length" && bar > 0 => n.as_u64().map(|t| {
                let bars = t as f64 / bar as f64;
                format!("{} bar{}", trim(bars), if bars == 1.0 { "" } else { "s" })
            }),
            Value::Number(n) => Some(format!("{} {n}", k.replace('_', " "))),
            Value::String(t) => Some(format!("{} {t}", k.replace('_', " "))),
            Value::Bool(b) => Some(format!(
                "{} {}",
                k.replace('_', " "),
                if *b { "on" } else { "off" }
            )),
            Value::Array(a) => Some(format!("{} {}", a.len(), k.replace('_', " "))),
            Value::Object(o) if o.len() == 1 => {
                let (k2, v2) = o.iter().next().unwrap();
                Some(format!(
                    "{k2} {}",
                    v2.as_str().map_or(v2.to_string(), str::to_string)
                ))
            }
            _ => None,
        })
        .take(4)
        .collect();
    (what, parts.join(" · "))
}

pub(crate) fn trim(x: f64) -> String {
    let s = format!("{x:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
