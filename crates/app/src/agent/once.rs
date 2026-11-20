//! One question for an agent, with no tools and no song: what it says, as it streams. For the
//! features that think (flips), apart from the conversation: nothing it says can change the song.

use super::*;

/// A question being answered: `stop` ends it.
#[derive(Clone)]
pub struct Asked {
    child: Arc<Mutex<Option<Child>>>,
    stopping: Arc<AtomicBool>,
}

impl Asked {
    /// Asked of no one: for pictures of an answer coming.
    #[cfg(all(test, unix))]
    pub fn idle() -> Asked {
        Asked {
            child: Arc::default(),
            stopping: Arc::default(),
        }
    }

    pub fn stop(&self) {
        self.stopping.store(true, Ordering::Relaxed);
        if let Some(c) = self.child.lock().unwrap().as_mut() {
            let _ = c.kill();
        }
    }
}

/// Asks `kind` (its command at `cmd`, as `tuning` picks) `prompt`, as `system` says to answer:
/// no tools, no MCP servers, no settings of the person's. Its words, thinking and end come as
/// events; an end without saying it's done says why (what it said on stderr), or that it was
/// stopped.
pub fn ask_once(
    kind: Kind,
    cmd: &Path,
    tuning: &Tuning,
    system: &str,
    prompt: &str,
) -> Result<(Asked, UnboundedReceiver<Event>), String> {
    let mut c = Command::new(cmd);
    c.args(tuning_args(kind, tuning, None));
    let parse: Parse = match kind {
        Kind::ClaudeCode => {
            c.args(["-p", "--verbose", "--include-partial-messages"])
                .args(["--output-format", "stream-json"])
                .args(["--tools", ""])
                .args([
                    "--strict-mcp-config",
                    "--mcp-config",
                    r#"{"mcpServers":{}}"#,
                ])
                .args(["--setting-sources", ""])
                .args(["--system-prompt", system])
                .env_remove("CLAUDECODE")
                .stdin(Stdio::piped());
            claude_events
        }
        Kind::Codex => {
            c.args(["exec", "--json", "--skip-git-repo-check"]).args([
                "-c",
                "sandbox_mode=\"read-only\"",
                "-c",
                "approval_policy=\"never\"",
            ]);
            // None of the person's MCP servers, plugins, apps, computer or browser use.
            for name in codex_servers() {
                c.args(["-c", &format!("mcp_servers.{name}.enabled=false")]);
            }
            for feature in ["plugins", "apps", "computer_use", "browser_use"] {
                c.args(["-c", &format!("features.{feature}=false")]);
            }
            c.arg(format!("{system}\n\n{prompt}")).stdin(Stdio::null());
            codex_events
        }
    };
    let mut child = c
        .current_dir(std::env::temp_dir())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Couldn't start {}: {e}", kind.def().name))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .map_err(|e| format!("Couldn't ask {}: {e}", kind.def().name))?;
    }
    let (tx, rx) = unbounded();
    let (stdout, mut stderr) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    let stopping = Arc::new(AtomicBool::new(false));
    let asked = Asked {
        child: Arc::new(Mutex::new(Some(child))),
        stopping: stopping.clone(),
    };
    let errors = thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let reaped = asked.child.clone();
    thread::spawn(move || {
        let mut done = false;
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            for e in parse(&v).0 {
                done |= matches!(e, Event::Done { .. });
                if tx.unbounded_send(e).is_err() {
                    return;
                }
            }
        }
        let said = errors.join().unwrap_or_default();
        if let Some(mut c) = reaped.lock().unwrap().take() {
            let _ = c.wait();
        }
        if done {
            return;
        }
        let stopped = stopping.load(Ordering::Relaxed);
        let _ = tx.unbounded_send(Event::Done {
            seconds: 0.0,
            stopped,
            error: (!stopped).then(|| {
                said.lines()
                    .rev()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or("It stopped without answering.")
                    .to_string()
            }),
        });
    });
    Ok((asked, rx))
}
