//! Claude, working in tempo: Claude Code run headless, with tempo's MCP server for its only
//! tools, its stream of thinking, words and tool calls turned into events. The app's Agent panel
//! and `tempo ask` both run it; `tempo ask` also sends its events to the app that has the
//! project open, so the panel shows that too.

use std::{
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    thread,
};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// What Claude is doing, as it happens.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// What it was asked.
    Asked { text: String },
    /// It's running, as this model.
    Started { model: String },
    /// It's thinking.
    Thinking,
    /// A piece of what it's saying.
    Text { text: String },
    /// It's calling a tool; what for comes with `Tool`.
    ToolStart { id: String, name: String },
    /// A tool call, all of it.
    Tool {
        id: String,
        name: String,
        input: Value,
    },
    /// What a tool answered.
    ToolDone {
        id: String,
        ok: bool,
        output: String,
    },
    /// It's done with what it was asked, was stopped, or went wrong.
    Done {
        seconds: f64,
        #[serde(default)]
        stopped: bool,
        #[serde(default)]
        error: Option<String>,
    },
}

/// Where Claude Code is: `TEMPO_CLAUDE`, else on the PATH or where its installer puts it (an app
/// opened from the Finder has a short PATH).
pub fn claude() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("TEMPO_CLAUDE") {
        return Some(p.into());
    }
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .chain([
            home.join(".local/bin"),
            home.join(".claude/local"),
            "/opt/homebrew/bin".into(),
            "/usr/local/bin".into(),
        ])
        .map(|d| d.join("claude"))
        .find(|p| p.is_file())
}

fn instructions(project: &Path) -> String {
    let p = project.display();
    format!(
        "You're working inside tempo, a DAW, on the song open at {p}. The person is watching: \
every edit you make appears in their window as you make it, so work in small steps, and keep \
your words short: a sentence before a group of steps saying what you're doing, and a few lines \
at the end saying what changed. Use tempo's tools, passing \"{p}\" as the project: `describe` \
before your first edit (the commands, and how ticks, levels and notes work), `show` to read the \
song, `edit` to change it, `samples_find`, `samples_kit` and `samples_put` for sounds that fit, \
`analyze` to check levels. You can't hear the song: trust `analyze` and the samples' analysis, \
and say what you couldn't check."
    )
}

/// A conversation with Claude about one song. Dropping it ends it.
pub struct Agent {
    child: Child,
    stdin: ChildStdin,
    asks: u32,
}

impl Agent {
    /// Starts Claude on `project`; its events come on the receiver.
    pub fn start(project: &Path) -> Result<(Agent, UnboundedReceiver<Event>), String> {
        let claude = claude().ok_or(
            "Claude Code isn't installed. Get it at claude.com/claude-code, sign in, then try again.",
        )?;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mcp = json!({ "mcpServers": { "tempo": { "command": exe, "args": ["mcp"] } } });
        let mut child = Command::new(&claude)
            .args(["-p", "--verbose", "--include-partial-messages"])
            .args([
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
            ])
            // tempo's tools and nothing else: no shell, no files, no one else's MCP servers or
            // hooks; anything else asked for is refused, not asked about.
            .args(["--strict-mcp-config", "--mcp-config", &mcp.to_string()])
            .args(["--tools", "", "--allowedTools", "mcp__tempo"])
            .args(["--permission-prompts", "none", "--setting-sources", ""])
            .args(["--append-system-prompt", &instructions(project)])
            .current_dir(project)
            .env_remove("CLAUDECODE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("{}: {e}", claude.display()))?;
        let stdin = child.stdin.take().unwrap();
        let (stdout, mut stderr) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
        let (tx, rx) = unbounded();
        let errors = thread::spawn(move || {
            let mut s = String::new();
            let _ = stderr.read_to_string(&mut s);
            s
        });
        thread::spawn(move || read(stdout, errors, tx));
        Ok((
            Agent {
                child,
                stdin,
                asks: 0,
            },
            rx,
        ))
    }

    /// Asks it something: a first request, or more about the last.
    pub fn ask(&mut self, text: &str) -> Result<(), String> {
        self.asks += 1;
        let line = json!({ "type": "user", "message": { "role": "user", "content": text } });
        writeln!(self.stdin, "{line}")
            .and_then(|()| self.stdin.flush())
            .map_err(|_| "Claude has stopped. Start a new conversation.".to_string())
    }

    /// Stops what it's doing; the conversation can go on.
    pub fn interrupt(&mut self) {
        let line = json!({
            "type": "control_request",
            "request_id": format!("stop-{}", self.asks),
            "request": { "subtype": "interrupt" },
        });
        let _ = writeln!(self.stdin, "{line}").and_then(|()| self.stdin.flush());
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `tempo ask`: Claude does what's asked to the song, saying what it's doing as it goes. The app
/// that has the song open shows it all in its Agent panel, and the edits as they're made.
pub fn ask(dir: &Path, words: &str) -> Result<(), String> {
    use std::io::IsTerminal;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let dir = dir
        .canonicalize()
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    let bar = crate::cli::project(&dir).map_or(0, |p| p.bar());
    let (mut agent, events) = Agent::start(&dir)?;
    let mut app = crate::server::Link::to(&dir);
    let mut tell = |e: &Event| {
        if let Some(link) = app.as_mut()
            && link
                .send(&crate::server::Request::Agent { event: e.clone() })
                .is_none()
        {
            app = None;
        }
    };
    let tty = std::io::stdout().is_terminal();
    let ansi = |code: &'static str| if tty { code } else { "" };
    let (dim, green, red, off) = (
        ansi("\x1b[2m"),
        ansi("\x1b[32m"),
        ansi("\x1b[31m"),
        ansi("\x1b[0m"),
    );
    let song = dir
        .file_name()
        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
    agent.ask(words)?;
    tell(&Event::Asked {
        text: words.to_string(),
    });
    let (mut steps, mut mid_line) = (0, false);
    let mut out = std::io::stdout();
    for e in futures::executor::block_on_stream(events) {
        tell(&e);
        let mut line = |s: String| {
            if std::mem::take(&mut mid_line) {
                println!();
            }
            println!("{s}");
        };
        match e {
            Event::Started { model } => line(format!("{dim}{model} is working on {song}…{off}")),
            Event::Text { text } => {
                print!("{text}");
                let _ = out.flush();
                mid_line = !text.ends_with('\n');
            }
            Event::Tool { name, input, .. } => {
                steps += 1;
                let (what, detail) = label(&name, &input, bar);
                line(format!("  {dim}→{off} {what} {dim}{detail}{off}"));
            }
            Event::ToolDone {
                ok: false, output, ..
            } => line(format!(
                "    {red}✗ {}{off}",
                output.lines().next().unwrap_or("")
            )),
            Event::Done {
                seconds,
                stopped,
                error,
            } => {
                let s = if steps == 1 { "" } else { "s" };
                line(match (error, stopped) {
                    (Some(e), _) => format!("{red}{e}{off}"),
                    (None, true) => {
                        format!("{dim}stopped after {seconds:.0}s · {steps} step{s}{off}")
                    }
                    (None, false) => {
                        format!("{green}✓{off} {dim}done in {seconds:.0}s · {steps} step{s}{off}")
                    }
                });
                break;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Turns Claude's output into events until it ends, then says why if it ended early.
fn read(stdout: impl Read, errors: thread::JoinHandle<String>, tx: UnboundedSender<Event>) {
    let mut busy = false;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        for e in events(&v) {
            busy = !matches!(e, Event::Done { .. });
            if tx.unbounded_send(e).is_err() {
                return;
            }
        }
    }
    let said = errors.join().unwrap_or_default();
    let why = said
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("Claude stopped.");
    if busy || !said.trim().is_empty() {
        let _ = tx.unbounded_send(Event::Done {
            seconds: 0.0,
            stopped: false,
            error: Some(why.trim().to_string()),
        });
    }
}

/// The events in one line of Claude Code's stream-json output.
pub fn events(v: &Value) -> Vec<Event> {
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

/// A tool call in words: what it's doing, and to what. `bar` is the song's bar in ticks, for
/// saying where.
pub fn label(name: &str, input: &Value, bar: u64) -> (String, String) {
    let tool = name.strip_prefix("mcp__tempo__").unwrap_or(name);
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
        other => return (other.replace('_', " "), String::new()),
    };
    (what.to_string(), detail)
}

/// A search's or kit's words, whether said as words or as an object with `text`.
fn words(q: &Value) -> String {
    match q {
        Value::String(q) => q.clone(),
        q => q["text"].as_str().unwrap_or_default().to_string(),
    }
}

/// An edit in words: "Set tempo", "bpm 101"; "Add midi region", "track 3 · bar 9 · 24 notes".
fn edit_label(c: &Value, bar: u64) -> (String, String) {
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

fn trim(x: f64) -> String {
    let s = format!("{x:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claudes_stream_becomes_events() {
        let line = |s: &str| events(&serde_json::from_str(s).unwrap());
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
                r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"t1","name":"mcp__tempo__edit","input":{}}}}"#
            ),
            [Event::ToolStart {
                id: "t1".into(),
                name: "mcp__tempo__edit".into()
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
    fn tool_calls_read_as_what_they_do() {
        let edit = json!({ "project": "/p", "command": {
            "op": "add_midi_region", "track": 2, "start": 30720, "length": 15360,
            "notes": [{}, {}, {}] } });
        assert_eq!(
            label("mcp__tempo__edit", &edit, 3840),
            (
                "Add midi region".into(),
                "track 2 · bar 9 · 4 bars · 3 notes".into()
            )
        );
        let set = json!({ "command": { "op": "set_track", "track": 1, "volume_db": -5 } });
        assert_eq!(
            label("mcp__tempo__edit", &set, 3840),
            ("Set track 1".into(), "volume db -5".into())
        );
        let put = json!({ "path": "/s/Kick 01.wav", "project": "/p" });
        assert_eq!(
            label("mcp__tempo__samples_put", &put, 3840),
            ("Putting in".into(), "Kick 01".into())
        );
    }
}
