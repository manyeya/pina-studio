//! `tempo mcp`: an MCP server on stdin and stdout, so agents that speak MCP can make music with
//! tempo. Its tools are the command line's, and edits reach the app when it has the project open.

use std::{
    io::{BufRead, Write},
    path::PathBuf,
};

use serde_json::{Value, json};
use tempo_engine::commands::{Command, DESCRIBE};

use crate::{cli, server::Request};

const INSTRUCTIONS: &str = "tempo is a DAW. A project is a folder; call `describe` first for the \
commands and how time and levels work, `show` to see a project, `edit` to change it one command at \
a time, and `analyze` to hear it as numbers (loudness, peaks, balance).";

fn tools() -> Value {
    let project = json!({ "type": "string", "description": "The project folder." });
    json!([
        {
            "name": "describe",
            "description": "The command reference: every edit, with examples, and how ticks, levels and notes work.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "show",
            "description": "A project's tracks, regions, notes, mixer, sends and auxes, as JSON.",
            "inputSchema": { "type": "object", "properties": { "project": project }, "required": ["project"] }
        },
        {
            "name": "edit",
            "description": "Applies one command (see `describe`) to a project, live in the app if it's open there. Makes the project if it doesn't exist.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "command": { "type": "object", "description": "A command, e.g. {\"op\": \"add_track\", \"name\": \"Bass\", \"instrument\": \"synth\"}." }
                },
                "required": ["project", "command"]
            }
        },
        {
            "name": "analyze",
            "description": "Measures a project (the mix, then each track alone) or a WAV file: peak, RMS, LUFS, clipping, and low/mid/high balance.",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string", "description": "A project folder or a .wav file." } },
                "required": ["path"]
            }
        },
        {
            "name": "render",
            "description": "Bounces a project to a WAV file.",
            "inputSchema": {
                "type": "object",
                "properties": { "project": project, "out": { "type": "string", "description": "The .wav to write." } },
                "required": ["project", "out"]
            }
        },
        {
            "name": "transport",
            "description": "Drives the app that has the project open: play, stop, locate (to a tick), save, undo, redo, or launch (a track's Live Loops cell for a scene, or stop it without one).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "action": { "type": "string", "enum": ["play", "stop", "locate", "save", "undo", "redo", "launch"] },
                    "tick": { "type": "integer", "minimum": 0 },
                    "track": { "type": "integer", "minimum": 0 },
                    "scene": { "type": "integer", "minimum": 0 }
                },
                "required": ["project", "action"]
            }
        },
        {
            "name": "plugins",
            "description": "The VST3 plugins installed, for set_instrument and set_insert.",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}

fn path(args: &Value, key: &str) -> Result<PathBuf, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| format!("`{key}` is required"))
}

fn call(name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "describe" => Ok(Value::String(DESCRIBE.to_string())),
        "show" => cli::show(&path(args, "project")?),
        "edit" => {
            let command: Command =
                serde_json::from_value(args.get("command").cloned().unwrap_or_default())
                    .map_err(|e| format!("not a command: {e}"))?;
            cli::edit_with(&path(args, "project")?, command)
        }
        "analyze" => cli::analyze(&path(args, "path")?),
        "render" => cli::render(&path(args, "project")?, &path(args, "out")?),
        "transport" => {
            let request = match args.get("action").and_then(Value::as_str) {
                Some("play") => Request::Play,
                Some("stop") => Request::Stop,
                Some("save") => Request::Save,
                Some("undo") => Request::Undo,
                Some("redo") => Request::Redo,
                Some("launch") => Request::Launch {
                    track: args
                        .get("track")
                        .and_then(Value::as_u64)
                        .ok_or("launch needs a `track`")? as usize,
                    scene: args
                        .get("scene")
                        .and_then(Value::as_u64)
                        .map(|s| s as usize),
                },
                Some("locate") => Request::Locate {
                    tick: args
                        .get("tick")
                        .and_then(Value::as_u64)
                        .ok_or("locate needs a `tick`")?,
                },
                _ => {
                    return Err(
                        "`action` is one of play, stop, locate, save, undo, redo, launch".into(),
                    );
                }
            };
            cli::app(&path(args, "project")?, request)
        }
        "plugins" => cli::plugins(),
        _ => Err(format!("no tool called {name}")),
    }
}

/// Answers one JSON-RPC message; None for notifications.
pub fn answer(msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let params = msg.get("params").cloned().unwrap_or_default();
    let result = match msg.get("method").and_then(Value::as_str).unwrap_or("") {
        "initialize" => json!({
            "protocolVersion": params.get("protocolVersion").cloned().unwrap_or("2025-06-18".into()),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "tempo", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let (text, error) = match call(name, &args) {
                Ok(Value::String(s)) => (s, false),
                Ok(v) => (serde_json::to_string_pretty(&v).unwrap(), false),
                Err(e) => (e, true),
            };
            json!({ "content": [{ "type": "text", "text": text }], "isError": error })
        }
        method => {
            let error = json!({ "code": -32601, "message": format!("no method {method}") });
            return Some(json!({ "jsonrpc": "2.0", "id": id, "error": error }));
        }
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

pub fn serve() -> Result<(), String> {
    let mut out = std::io::stdout();
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => answer(&msg),
            Err(e) => Some(json!({
                "jsonrpc": "2.0", "id": null,
                "error": { "code": -32700, "message": e.to_string() }
            })),
        };
        if let Some(reply) = reply {
            writeln!(out, "{reply}").map_err(|e| e.to_string())?;
            out.flush().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaks_mcp() {
        let init = answer(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-06-18"}}))
        .unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "tempo");
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
                "render",
                "transport",
                "plugins"
            ]
        );

        // Build a little project on disk through the tools, then measure it.
        let dir = std::env::temp_dir().join(format!("tempo-test-mcp-{}", std::process::id()));
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
}
