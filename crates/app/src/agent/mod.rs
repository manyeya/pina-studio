//! Agents working in Pina Studio, as modisa knows them: Claude Code and Codex, each run
//! headless with Pina Studio's MCP server for its only tools, their streams of thinking, words and
//! tool calls turned into the same events. The app's Agent panel and `pina ask` both run them;
//! `pina ask` also sends its events to the app that has the project open, so the panel shows
//! that too.

mod commands;
mod events;
mod kinds;
mod process;
mod tuning;

pub(crate) use commands::*;
pub use events::*;
pub use kinds::*;
pub use process::*;
pub use tuning::*;

pub mod chat;
pub mod listen;
pub mod mcp;
pub mod mixing;
pub mod produce;
pub mod producer;
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// What an agent is doing, as it happens.
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

/// The agents Pina Studio runs. Only those whose headless streams and tool hookup are checked against
/// the real thing are here (modisa knows more; they come in the same way). Not opencode: version
/// 2 hands MCP tools to its model inside one code-running `execute` tool, so its steps can't
/// show as Pina Studio's, and its free tier refuses headless runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    #[default]
    ClaudeCode,
    Codex,
}

pub const AGENTS: [Def; 2] = [
    Def {
        kind: Kind::ClaudeCode,
        id: "claude-code",
        name: "Claude Code",
        bin: "claude",
        color: Some(0xd97757),
        logo: '\u{f5a00}',
        sign_in: "claude auth login",
    },
    Def {
        kind: Kind::Codex,
        id: "codex",
        name: "Codex",
        bin: "codex",
        color: None,
        logo: '\u{f5a01}',
        sign_in: "codex login",
    },
];

/// The logo font, with the logos from U+F5A00 in modisa's order.
pub const MARKS: &str = "Modisa Marks";

/// Claude Code's effort levels (its `--effort`).
const CLAUDE_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

#[cfg(test)]
mod tests;
