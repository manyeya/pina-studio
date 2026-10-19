//! Agents working in tempo, as modisa knows them: Claude Code and Codex, each run
//! headless with tempo's MCP server for its only tools, their streams of thinking, words and
//! tool calls turned into the same events. The app's Agent panel and `tempo ask` both run them;
//! `tempo ask` also sends its events to the app that has the project open, so the panel shows
//! that too.

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

/// The agents tempo runs. Only those whose headless streams and tool hookup are checked against
/// the real thing are here (modisa knows more; they come in the same way). Not opencode: version
/// 2 hands MCP tools to its model inside one code-running `execute` tool, so its steps can't
/// show as tempo's, and its free tier refuses headless runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    #[default]
    ClaudeCode,
    Codex,
}

/// An agent as modisa shows it: its id, name, command, brand colour (none for a monochrome
/// brand) and its logo in modisa's logo font, "Modisa Marks".
pub struct Def {
    pub kind: Kind,
    pub id: &'static str,
    pub name: &'static str,
    pub bin: &'static str,
    pub color: Option<u32>,
    pub logo: char,
    /// How to sign in, when it isn't.
    pub sign_in: &'static str,
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

impl Kind {
    pub fn def(self) -> &'static Def {
        AGENTS.iter().find(|d| d.kind == self).unwrap()
    }

    pub fn from_id(id: &str) -> Option<Kind> {
        AGENTS.iter().find(|d| d.id == id.trim()).map(|d| d.kind)
    }

    /// Where its command is: `TEMPO_<COMMAND>` (TEMPO_CLAUDE), else on the PATH or where its
    /// installer puts it (an app opened from the Finder has a short PATH).
    pub fn command(self) -> Option<PathBuf> {
        let bin = self.def().bin;
        if let Some(p) = std::env::var_os(format!("TEMPO_{}", bin.to_uppercase())) {
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
            .map(|d| d.join(bin))
            .find(|p| p.is_file())
    }

    /// Whether it can work: installed, and signed in. Asks the agent, so takes a moment.
    pub fn readiness(self) -> Readiness {
        let Some(cmd) = self.command() else {
            return Readiness::Missing;
        };
        // What it says, on either stream (Codex says its status on stderr), and whether it
        // ended well.
        let run = |args: &[&str]| {
            Command::new(&cmd)
                .args(args)
                .stdin(Stdio::null())
                .output()
                .ok()
                .map(|o| {
                    let said = String::from_utf8_lossy(&o.stdout).into_owned()
                        + &String::from_utf8_lossy(&o.stderr);
                    (o.status.success(), said)
                })
        };
        match self {
            Kind::ClaudeCode => {
                let status: Value = run(&["auth", "status"])
                    .and_then(|(_, s)| {
                        // The JSON, whatever else is said around it.
                        let (a, b) = (s.find('{')?, s.rfind('}')?);
                        serde_json::from_str(s.get(a..=b)?).ok()
                    })
                    .unwrap_or_default();
                match (status["loggedIn"].as_bool(), status["authMethod"].as_str()) {
                    (Some(false), _) => Readiness::SignIn,
                    (_, Some("claude.ai")) => Readiness::Ready("your Claude plan".into()),
                    (_, Some(m)) => Readiness::Ready(m.replace(['_', '-'], " ")),
                    _ => Readiness::Ready(String::new()),
                }
            }
            Kind::Codex => match run(&["login", "status"]) {
                Some((ok, said)) => codex_status(ok, &said),
                None => Readiness::SignIn,
            },
        }
    }
}

/// What `codex login status` said: signed in (and how), or not.
fn codex_status(ok: bool, said: &str) -> Readiness {
    let line = said
        .lines()
        .map(str::trim)
        .find(|l| l.contains("Logged in") || l.contains("Not logged in"));
    match line {
        Some(l) if l.contains("Not logged in") => Readiness::SignIn,
        Some(l) => Readiness::Ready(
            l.split_once("Logged in using ")
                .map_or("", |(_, how)| how)
                .to_string(),
        ),
        // Said nothing we know: it ending well means it's signed in.
        None if ok => Readiness::Ready(String::new()),
        None => Readiness::SignIn,
    }
}

/// Whether an agent can work here.
#[derive(Clone, Debug, PartialEq)]
pub enum Readiness {
    /// Installed and signed in; on what (a plan, an API key), when it says.
    Ready(String),
    Missing,
    SignIn,
}

/// The agent chosen last, in `~/Library/Application Support/tempo/agent`; Claude Code at first.
/// How an agent runs: the model it uses and how hard it thinks (its effort). None leaves either
/// to the agent's own settings.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Tuning {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

/// A model to pick: its id as the agent takes it, its name, and the effort levels it takes.
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub efforts: Vec<String>,
}

/// Claude Code's effort levels (its `--effort`).
const CLAUDE_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

impl Kind {
    /// The models it offers: Claude Code's by their aliases (each the latest of its line), Codex's
    /// as its own model list has them (the ones it lists, in its order).
    pub fn models(self) -> Vec<Model> {
        match self {
            Kind::ClaudeCode => [
                ("fable", "Fable"),
                ("opus", "Opus"),
                ("sonnet", "Sonnet"),
                ("haiku", "Haiku"),
            ]
            .into_iter()
            .map(|(id, name)| Model {
                id: id.into(),
                name: name.into(),
                efforts: CLAUDE_EFFORTS.map(String::from).to_vec(),
            })
            .collect(),
            Kind::Codex => codex_models(),
        }
    }

    /// The effort levels it takes with a model (or, with none picked, its default's).
    pub fn efforts(self, model: Option<&str>) -> Vec<String> {
        let models = self.models();
        let id = model.map(str::to_string).or_else(|| self.default_model());
        match id.and_then(|id| models.into_iter().find(|m| m.id == id)) {
            Some(m) => m.efforts,
            None => match self {
                Kind::ClaudeCode => CLAUDE_EFFORTS.map(String::from).to_vec(),
                Kind::Codex => ["low", "medium", "high"].map(String::from).to_vec(),
            },
        }
    }

    /// The model it uses when none is picked, if its settings say (Codex's config does).
    pub fn default_model(self) -> Option<String> {
        match self {
            Kind::ClaudeCode => None,
            Kind::Codex => {
                let config = std::fs::read_to_string(codex_home().join("config.toml")).ok()?;
                // The top level's `model = "…"`, before any [table].
                config
                    .lines()
                    .take_while(|l| !l.trim_start().starts_with('['))
                    .find_map(|l| {
                        let (k, v) = l.split_once('=')?;
                        (k.trim() == "model").then(|| v.trim().trim_matches('"').to_string())
                    })
            }
        }
    }
}

/// A model's name for people: its own, or (not one listed) its id.
pub fn model_name(kind: Kind, id: &str) -> String {
    kind.models()
        .into_iter()
        .find(|m| m.id == id)
        .map_or_else(|| id.to_string(), |m| m.name)
}

/// An effort level's name for people.
pub fn effort_name(effort: &str) -> String {
    match effort {
        "xhigh" => "Extra high".into(),
        "max" => "Max".into(),
        e => {
            let mut c = e.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        }
    }
}

fn codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".codex")
        })
}

/// Codex's models, from the list it keeps (`models_cache.json`): those it shows, in its order.
fn codex_models() -> Vec<Model> {
    let Ok(text) = std::fs::read_to_string(codex_home().join("models_cache.json")) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let mut models: Vec<(i64, Model)> = v["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["visibility"] == "list")
        .filter_map(|m| {
            let id = m["slug"].as_str()?.to_string();
            let efforts = m["supported_reasoning_levels"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|l| l["effort"].as_str().map(String::from))
                .collect();
            Some((
                m["priority"].as_i64().unwrap_or(i64::MAX),
                Model {
                    name: m["display_name"].as_str().unwrap_or(&id).to_string(),
                    id,
                    efforts,
                },
            ))
        })
        .collect();
    models.sort_by_key(|m| m.0);
    models.into_iter().map(|m| m.1).collect()
}

/// The model and effort last picked for an agent, kept for every song.
pub fn tuning(kind: Kind) -> Tuning {
    let all: serde_json::Map<String, Value> = std::fs::read_to_string(tuning_file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    all.get(kind.def().id)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

pub fn tune(kind: Kind, tuning: &Tuning) {
    let file = tuning_file();
    let mut all: serde_json::Map<String, Value> = std::fs::read_to_string(&file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    all.insert(kind.def().id.into(), json!(tuning));
    let _ = std::fs::create_dir_all(file.parent().unwrap());
    let _ = std::fs::write(file, Value::Object(all).to_string());
}

fn tuning_file() -> PathBuf {
    match cfg!(test) {
        // Tests never touch the person's.
        true => std::env::temp_dir().join(format!(
            "tempo-test-tuning-{}-{:?}.json",
            std::process::id(),
            thread::current().id()
        )),
        false => choice_file().with_file_name("agent-tuning.json"),
    }
}

pub fn chosen() -> Kind {
    std::fs::read_to_string(choice_file())
        .ok()
        .and_then(|s| Kind::from_id(&s))
        .unwrap_or_default()
}

pub fn choose(kind: Kind) {
    let file = choice_file();
    let _ = std::fs::create_dir_all(file.parent().unwrap());
    let _ = std::fs::write(file, kind.def().id);
}

fn choice_file() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        .join("Library/Application Support/tempo/agent")
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
`analyze` to check levels. The person can pick sounds, instruments and effects for the song \
themselves (`palette`, and \
listed in their message when they change): build with those before choosing your own, each for \
the part they gave it, and with no others when they ask for only these. You can't hear the song: `critique` is your ears (quick after each \
edit, with a render after mixing), trust it and the samples' analysis, and say what you couldn't \
check.\n\nWrite music with tempo's write commands, not note by note: set_chords, then \
write_chords, write_bass, write_melody and write_drums put parts on the chords that are in key, \
voice-led and played like a player would. To make a song that sounds finished: {}",
        crate::produce::WORKFLOW
    )
}

/// A conversation with an agent about one song. Dropping it ends it.
pub struct Agent {
    kind: Kind,
    project: PathBuf,
    tx: UnboundedSender<Event>,
    run: Run,
    asks: u32,
    /// The model and effort it runs with.
    pub tuning: Tuning,
}

enum Run {
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
type Parse = fn(&Value) -> (Vec<Event>, Option<String>);

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
            // A conversation carried on has had tempo's instructions.
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
                // Theirs has no way in for tempo's instructions but the first question.
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

/// Where an agent runs: the song's folder, or, for a song not saved yet (its folder isn't made
/// until it is), the nearest folder that is.
fn workdir(project: &Path) -> PathBuf {
    project
        .ancestors()
        .find(|d| d.is_dir())
        .unwrap_or(Path::new("/"))
        .to_path_buf()
}

/// Claude Code with tempo's tools and nothing else: no shell, no files, no one else's MCP servers
/// or hooks; anything else asked for is refused, not asked about.
fn claude_command(
    cmd: &Path,
    project: &Path,
    tuning: &Tuning,
    resume: Option<&str>,
) -> Result<Child, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mcp = json!({ "mcpServers": { "tempo": { "command": exe, "args": ["mcp"] } } });
    Command::new(cmd)
        .args(tuning_args(Kind::ClaudeCode, tuning, resume))
        .args(["-p", "--verbose", "--include-partial-messages"])
        .args([
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
        ])
        .args(["--strict-mcp-config", "--mcp-config", &mcp.to_string()])
        .args(["--tools", "", "--allowedTools", "mcp__tempo"])
        .args(["--permission-prompts", "none", "--setting-sources", ""])
        .args(["--append-system-prompt", &instructions(project)])
        .current_dir(workdir(project))
        .env_remove("CLAUDECODE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Couldn't start {}: {e}", cmd.display()))
}

/// The flags that pick an agent's model and effort, and (Claude Code's) carry on a conversation.
fn tuning_args(kind: Kind, tuning: &Tuning, resume: Option<&str>) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    match kind {
        Kind::ClaudeCode => {
            if let Some(m) = &tuning.model {
                a.extend(["--model".into(), m.clone()]);
            }
            if let Some(e) = &tuning.effort {
                a.extend(["--effort".into(), e.clone()]);
            }
            if let Some(id) = resume {
                a.extend(["--resume".into(), id.into()]);
            }
        }
        Kind::Codex => {
            if let Some(m) = &tuning.model {
                a.extend(["-m".into(), m.clone()]);
            }
            if let Some(e) = &tuning.effort {
                a.extend(["-c".into(), format!("model_reasoning_effort={e:?}")]);
            }
        }
    }
    a
}

/// `codex exec`, resuming the conversation after its first question: tempo's MCP server in place
/// of any others (its tools let through without asking, as nothing can be asked), its sandbox
/// read-only, nothing asked about.
fn codex_command(
    cmd: &Path,
    project: &Path,
    tuning: &Tuning,
    session: Option<&str>,
    text: &str,
) -> Result<Child, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut c = Command::new(cmd);
    c.arg("exec");
    if session.is_some() {
        c.arg("resume");
    }
    c.args(["--json", "--skip-git-repo-check"])
        .args([
            "-c",
            "sandbox_mode=\"read-only\"",
            "-c",
            "approval_policy=\"never\"",
        ])
        .args(codex_isolation(&exe, &codex_servers()));
    c.args(tuning_args(Kind::Codex, tuning, None));
    if let Some(id) = session {
        c.arg(id);
    }
    c.arg(text)
        .current_dir(workdir(project))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Couldn't start {}: {e}", cmd.display()))
}

/// The MCP servers the person's Codex config sets up, by name.
fn codex_servers() -> Vec<String> {
    let config = std::fs::read_to_string(codex_home().join("config.toml")).unwrap_or_default();
    let mut names: Vec<String> = Vec::new();
    for line in config.lines().map(str::trim) {
        let Some(rest) = line
            .strip_prefix("[mcp_servers.")
            .and_then(|r| r.strip_suffix(']'))
        else {
            continue;
        };
        // `[mcp_servers.name]`, `[mcp_servers."a name"]`, `[mcp_servers.name.env]`.
        let name = match rest.strip_prefix('"') {
            Some(quoted) => quoted.split('"').next().unwrap_or_default(),
            None => rest.split('.').next().unwrap_or_default(),
        };
        if !name.is_empty() && !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
    }
    names
}

/// What makes tempo's MCP server Codex's only tools: it, its tools approved (Codex asks before
/// an MCP tool runs, and with no one to ask, refuses), and the person's other servers, plugins,
/// apps, computer and browser use off. Features are turned off by key, which a Codex without them
/// ignores (its `--disable` refuses a name it doesn't know).
fn codex_isolation(exe: &Path, servers: &[String]) -> Vec<String> {
    let mut a = vec![
        "-c".into(),
        format!(
            "mcp_servers.tempo={{command={:?},args=[\"mcp\"],default_tools_approval_mode=\"approve\"}}",
            exe.display().to_string()
        ),
    ];
    // By bare name: Codex's `-c` paths don't take quoted ones (a name that needs quotes, rare,
    // stays as the person set it up).
    let bare = |n: &&String| {
        n.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    };
    for name in servers.iter().filter(|n| *n != "tempo").filter(bare) {
        a.extend(["-c".into(), format!("mcp_servers.{name}.enabled=false")]);
    }
    for feature in ["plugins", "apps", "computer_use", "browser_use"] {
        a.extend(["-c".into(), format!("features.{feature}=false")]);
    }
    a
}

/// `tempo ask`: an agent does what's asked to the song, saying what it's doing as it goes. The app
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
fn read(
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

/// The events in one line of Claude Code's stream-json output.
fn claude_events(v: &Value) -> (Vec<Event>, Option<String>) {
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
fn codex_events(v: &Value) -> (Vec<Event>, Option<String>) {
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
    fn codex_gets_tempos_tools_approved_and_nothing_else() {
        let home =
            std::env::temp_dir().join(format!("tempo-test-codex-mcp-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            home.join("config.toml"),
            "model = \"x\"\n[mcp_servers.node_repl]\ncommand = \"n\"\n[mcp_servers.node_repl.env]\nA = \"1\"\n[mcp_servers.\"computer-use\"]\ncommand = \"c\"\n[mcp_servers.tempo]\ncommand = \"t\"\n",
        )
        .unwrap();
        // Read where Codex keeps its config: the servers it sets up, once each.
        let servers = {
            let _home = CodexHome::set(&home);
            codex_servers()
        };
        assert_eq!(servers, ["node_repl", "computer-use", "tempo"]);
        let args = codex_isolation(Path::new("/A/tempo"), &servers).join(" ");
        assert!(args.contains(r#"mcp_servers.tempo={command="/A/tempo",args=["mcp"],default_tools_approval_mode="approve"}"#), "{args}");
        assert!(args.contains("mcp_servers.node_repl.enabled=false"));
        assert!(args.contains("mcp_servers.computer-use.enabled=false"));
        assert!(!args.contains("tempo.enabled"));
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
        let home = std::env::temp_dir().join(format!("tempo-test-codex-{}", std::process::id()));
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
    fn codexs_stream_becomes_the_same_events() {
        let codex = |s: &str| codex_events(&serde_json::from_str(s).unwrap());
        let (e, id) = codex(r#"{"type":"thread.started","thread_id":"01a1"}"#);
        assert_eq!((e.len(), id.as_deref()), (1, Some("01a1")));
        let (e, _) = codex(
            r#"{"type":"item.started","item":{"id":"item_2","type":"mcp_tool_call","server":"tempo","tool":"edit","arguments":{"command":{"op":"set_tempo","bpm":101}},"status":"in_progress"}}"#,
        );
        assert!(matches!(&e[1], Event::Tool { name, input, .. }
            if name == "mcp__tempo__edit" && input["command"]["bpm"] == 101));
        let (e, _) = codex(
            r#"{"type":"item.completed","item":{"id":"item_2","type":"mcp_tool_call","server":"tempo","tool":"edit","status":"failed","error":{"message":"no such track"}}}"#,
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
        assert_eq!(workdir(&tmp.join("tempo-not-made-yet/Untitled")), tmp);
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
