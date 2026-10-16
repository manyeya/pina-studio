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
            "description": "The plugins installed (VST3, CLAP, VST2 and Audio Units), each as set_instrument and set_insert take it: format, path, class and (for some formats) uid.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "params",
            "description": "A plugin's parameters (id, name, value 0-1 and as the plugin shows it, default, unit, steps and each step's name), for set_plugin_param and plugin automation, which take a parameter by id or name and a value 0-1 or as text (\"850 Hz\").",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "channel": { "description": "{\"track\": i}, {\"aux\": i} or \"master\"." },
                    "plugin": { "description": "\"instrument\" or {\"insert\": i}." }
                },
                "required": ["project", "channel", "plugin"]
            }
        },
        {
            "name": "presets",
            "description": "A plugin's presets (its own programs, and preset files of its format), for set_preset; or, with `save`, saves its settings as a preset file of that name and says where.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "channel": { "description": "{\"track\": i}, {\"aux\": i} or \"master\"." },
                    "plugin": { "description": "\"instrument\" or {\"insert\": i}." },
                    "save": { "type": "string", "description": "A name to save its settings as." }
                },
                "required": ["project", "channel", "plugin"]
            }
        },
        {
            "name": "samples_find",
            "description": "Finds samples in the library (analysed for kind, category, pitch, key, tempo, envelope, spectrum and traits), best first, each with why it matched and, for a one-shot, a sampler zone to play it in tune. `query` is words (\"dusty keys loop in C minor 95 bpm\", \"punchy dark kick\") or a query object: text, kind (one_shot|loop), categories, traits, key, bpm, tonal, min_seconds, max_seconds, folder, like (a path: sounds like it), aspects ({timbre, pitch, envelope, spectrum} weights for like), with (paths it should go with), limit. `project` fills the key and tempo from a song.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "description": "Words, or a query object." },
                    "project": project
                },
                "required": ["query"]
            }
        },
        {
            "name": "samples_similar",
            "description": "The samples most like one, weighing timbre, pitch, envelope and spectrum as `aspects` says (each 0-1).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "aspects": { "type": "object" },
                    "limit": { "type": "integer", "minimum": 1 }
                },
                "required": ["path"]
            }
        },
        {
            "name": "samples_kit",
            "description": "Builds a kit that goes together (kick, snare, clap, hats, perc, 808, keys by default): each pick fits the key and tempo and the picks before it (no clashes, one sound, one pack where it can), with alternatives. `seed` is words or {key, bpm, text, traits, like, folder, roles, alternatives, keep}; `project` fills the key and tempo from a song.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "seed": { "description": "Words, or a kit seed object." },
                    "project": project
                }
            }
        },
        {
            "name": "samples_library",
            "description": "The sample library: `status` (its folders and what's in them), `scan` (analyse what's new or changed), `add` or `remove` a folder (`path`), or `info` on a sample (`path`).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["status", "scan", "add", "remove", "info"] },
                    "path": { "type": "string" }
                },
                "required": ["action"]
            }
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
        "params" => {
            let parse = |key: &str| args.get(key).cloned().unwrap_or_default();
            let channel =
                serde_json::from_value(parse("channel")).map_err(|e| format!("`channel`: {e}"))?;
            let plugin =
                serde_json::from_value(parse("plugin")).map_err(|e| format!("`plugin`: {e}"))?;
            cli::params(&path(args, "project")?, channel, plugin)
        }
        "samples_find" => {
            let q = match args.get("query") {
                Some(Value::String(words)) => tempo_engine::samples::search::Query {
                    text: words.clone(),
                    ..Default::default()
                },
                Some(v) => {
                    serde_json::from_value(v.clone()).map_err(|e| format!("`query`: {e}"))?
                }
                None => return Err("`query` is required".into()),
            };
            let project = args
                .get("project")
                .and_then(Value::as_str)
                .map(PathBuf::from);
            crate::samples_cli::find(q, project.as_deref())
        }
        "samples_similar" => {
            let aspects = match args.get("aspects") {
                Some(v) => {
                    serde_json::from_value(v.clone()).map_err(|e| format!("`aspects`: {e}"))?
                }
                None => Default::default(),
            };
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;
            crate::samples_cli::similar(&path(args, "path")?, aspects, limit)
        }
        "samples_kit" => {
            let seed = match args.get("seed") {
                Some(Value::String(words)) => tempo_engine::samples::search::KitSeed {
                    text: words.clone(),
                    ..Default::default()
                },
                Some(v) => serde_json::from_value(v.clone()).map_err(|e| format!("`seed`: {e}"))?,
                None => Default::default(),
            };
            let project = args
                .get("project")
                .and_then(Value::as_str)
                .map(PathBuf::from);
            crate::samples_cli::kit(seed, project.as_deref())
        }
        "samples_library" => match args.get("action").and_then(Value::as_str) {
            Some("status") => crate::samples_cli::status(),
            Some("scan") => crate::samples_cli::scan(),
            Some("add") => crate::samples_cli::add_folder(&path(args, "path")?),
            Some("remove") => crate::samples_cli::remove_folder(&path(args, "path")?),
            Some("info") => crate::samples_cli::info(&path(args, "path")?),
            _ => Err("`action` is one of status, scan, add, remove, info".into()),
        },
        "presets" => {
            let parse = |key: &str| args.get(key).cloned().unwrap_or_default();
            let channel =
                serde_json::from_value(parse("channel")).map_err(|e| format!("`channel`: {e}"))?;
            let plugin =
                serde_json::from_value(parse("plugin")).map_err(|e| format!("`plugin`: {e}"))?;
            let project = path(args, "project")?;
            match args.get("save").and_then(Value::as_str) {
                Some(name) => cli::save_preset(&project, channel, plugin, name),
                None => cli::presets(&project, channel, plugin),
            }
        }
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
                "plugins",
                "params",
                "presets",
                "samples_find",
                "samples_similar",
                "samples_kit",
                "samples_library"
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
