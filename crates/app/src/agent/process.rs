//! An agent at work: its process, a question asked, interrupted.

use super::*;

/// A conversation with an agent about one song. Dropping it ends it.
pub struct Agent {
    pub(crate) kind: Kind,
    pub(crate) project: PathBuf,
    pub(crate) tx: UnboundedSender<Event>,
    pub(crate) run: Run,
    pub(crate) asks: u32,
    /// The model and effort it runs with.
    pub tuning: Tuning,
}

pub(crate) enum Run {
    /// Claude Code: one process for the whole conversation, asked on its stdin; its
    /// conversation's id, to carry it on in another (another model, say).
    Claude {
        child: Child,
        stdin: ChildStdin,
        session: Arc<Mutex<Option<String>>>,
    },
    /// Codex: a process for each question, the conversation carried on by its id.
    Turns {
        child: Option<Child>,
        session: Arc<Mutex<Option<String>>>,
        stopping: Arc<AtomicBool>,
    },
}

/// What one line of an agent's output says, and the conversation's id if it names it.
pub(crate) type Parse = fn(&Value) -> (Vec<Event>, Option<String>);

impl Agent {
    /// Starts `kind` on `project` with a model and effort, carrying on conversation `resume` if
    /// given; its events come on the receiver.
    pub fn start(
        kind: Kind,
        project: &Path,
        tuning: Tuning,
        resume: Option<String>,
    ) -> Result<(Agent, UnboundedReceiver<Event>), String> {
        let def = kind.def();
        let cmd = kind.command().ok_or(format!(
            "{} isn't installed. Install it, sign in ({}), then try again.",
            def.name, def.sign_in
        ))?;
        let (tx, rx) = unbounded();
        let carried = resume.is_some();
        let run = match kind {
            Kind::ClaudeCode => {
                let mut child = claude_command(&cmd, project, &tuning, resume.as_deref())?;
                let stdin = child.stdin.take().unwrap();
                let session = Arc::new(Mutex::new(resume));
                read(
                    &mut child,
                    tx.clone(),
                    claude_events,
                    Some(session.clone()),
                    None,
                );
                Run::Claude {
                    child,
                    stdin,
                    session,
                }
            }
            _ => Run::Turns {
                child: None,
                session: Arc::new(Mutex::new(resume)),
                stopping: Arc::default(),
            },
        };
        let agent = Agent {
            kind,
            project: project.to_path_buf(),
            tx,
            run,
            // A conversation carried on has had Pina Studio's instructions.
            asks: carried as u32,
            tuning,
        };
        Ok((agent, rx))
    }

    /// The conversation's id, once the agent has said it.
    pub fn session(&self) -> Option<String> {
        match &self.run {
            Run::Claude { session, .. } | Run::Turns { session, .. } => {
                session.lock().unwrap().clone()
            }
        }
    }

    /// Asks it something: a first request, or more about the last.
    pub fn ask(&mut self, text: &str) -> Result<(), String> {
        self.asks += 1;
        let first = self.asks == 1;
        match &mut self.run {
            Run::Claude { stdin, .. } => {
                let line =
                    json!({ "type": "user", "message": { "role": "user", "content": text } });
                writeln!(stdin, "{line}")
                    .and_then(|()| stdin.flush())
                    .map_err(|_| "Claude has stopped. Start a new conversation.".to_string())
            }
            Run::Turns {
                child,
                session,
                stopping,
            } => {
                if let Some(c) = child
                    && c.try_wait().ok().flatten().is_none()
                {
                    return Err("It's still working on the last one.".into());
                }
                // Theirs has no way in for Pina Studio's instructions but the first question.
                let text = match first {
                    true => format!("{}\n\n{text}", instructions(&self.project)),
                    false => text.to_string(),
                };
                let cmd = self.kind.command().ok_or("It's no longer installed.")?;
                let id = session.lock().unwrap().clone();
                let mut c = codex_command(&cmd, &self.project, &self.tuning, id.as_deref(), &text)?;
                stopping.store(false, Ordering::Relaxed);
                read(
                    &mut c,
                    self.tx.clone(),
                    codex_events,
                    Some(session.clone()),
                    Some(stopping.clone()),
                );
                *child = Some(c);
                Ok(())
            }
        }
    }

    /// Stops what it's doing; the conversation can go on.
    pub fn interrupt(&mut self) {
        match &mut self.run {
            Run::Claude { stdin, .. } => {
                let line = json!({
                    "type": "control_request",
                    "request_id": format!("stop-{}", self.asks),
                    "request": { "subtype": "interrupt" },
                });
                let _ = writeln!(stdin, "{line}").and_then(|()| stdin.flush());
            }
            Run::Turns {
                child, stopping, ..
            } => {
                stopping.store(true, Ordering::Relaxed);
                if let Some(c) = child {
                    let _ = c.kill();
                }
            }
        }
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        let child = match &mut self.run {
            Run::Claude { child, .. } => Some(child),
            Run::Turns { child, .. } => child.as_mut(),
        };
        if let Some(c) = child {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// `pina ask`: an agent does what's asked to the song, saying what it's doing as it goes. The app
/// that has the song open shows it all in its Agent panel, and the edits as they're made.
pub fn ask(kind: Kind, dir: &Path, words: &str) -> Result<(), String> {
    use std::io::IsTerminal;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let dir = dir
        .canonicalize()
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    let bar = crate::cli::project(&dir).map_or(0, |p| p.bar());
    let def = kind.def();
    match kind.readiness() {
        Readiness::Missing => return Err(format!("{} isn't installed.", def.name)),
        Readiness::SignIn => {
            return Err(format!("{} isn't signed in: run {}", def.name, def.sign_in));
        }
        Readiness::Ready(_) => {}
    }
    let (mut agent, events) = Agent::start(kind, &dir, tuning(kind), None)?;
    let mut app = crate::cli::server::Link::to(&dir);
    let mut tell = |e: &Event| {
        if let Some(link) = app.as_mut()
            && link
                .send(&crate::cli::server::Request::Agent { event: e.clone() })
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
            Event::Started { model } => {
                let who = if model.is_empty() {
                    kind.def().name
                } else {
                    &model
                };
                line(format!("{dim}{who} is working on {song}…{off}"))
            }
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

/// Turns an agent's output into events until it ends, keeping the conversation's id when it
/// names it; then, if it ended without saying it was done, says why.
pub(crate) fn read(
    child: &mut Child,
    tx: UnboundedSender<Event>,
    parse: Parse,
    session: Option<Arc<Mutex<Option<String>>>>,
    stopping: Option<Arc<AtomicBool>>,
) {
    let (stdout, mut stderr) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    let errors = thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    thread::spawn(move || {
        let mut done = false;
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let (events, id) = parse(&v);
            if let (Some(id), Some(s)) = (id, &session) {
                *s.lock().unwrap() = Some(id);
            }
            for e in events {
                done = matches!(e, Event::Done { .. });
                if tx.unbounded_send(e).is_err() {
                    return;
                }
            }
        }
        let said = errors.join().unwrap_or_default();
        // Codex's runs (a process a question) can be stopped; Claude's lasts the conversation.
        let turns = stopping.is_some();
        let stopped = stopping.is_some_and(|s| s.load(Ordering::Relaxed));
        let error = match turns {
            // A process for each question ends when it's answered, or stopped.
            true if done => return,
            true => None,
            // Claude's lasts the conversation: its ending is news.
            false if done && said.trim().is_empty() => return,
            false => Some(
                said.lines()
                    .rev()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or("Claude stopped.")
                    .to_string(),
            ),
        };
        let _ = tx.unbounded_send(Event::Done {
            seconds: 0.0,
            stopped,
            error: error.filter(|_| !stopped),
        });
    });
}
