//! The command lines that start each agent, with Pina Studio's tools and instructions.

use super::*;

pub(crate) fn instructions(project: &Path) -> String {
    let p = project.display();
    format!(
        "You're working inside Pina Studio, a DAW, on the song open at {p}. The person is watching: \
every edit you make appears in their window as you make it, so work in small steps, and keep \
your words short: a sentence before a group of steps saying what you're doing, and a few lines \
at the end saying what changed. Use Pina Studio's tools, passing \"{p}\" as the project: `describe` \
before your first edit (the commands, and how ticks, levels and notes work), `show` to read the \
song, `edit` to change it, `samples_find`, `samples_kit` and `samples_put` for sounds that fit, \
`analyze` to check levels. The person can pick sounds, instruments and effects for the song \
themselves (`palette`, and \
listed in their message when they change): build with those before choosing your own, each for \
the part they gave it, and with no others when they ask for only these. You can't hear the song: `critique` is your ears (quick after each \
edit, with a render after mixing), trust it and the samples' analysis, and say what you couldn't \
check.\n\nWrite music with Pina Studio's write commands, not note by note: set_chords, then \
write_chords, write_bass, write_melody and write_drums put parts on the chords that are in key, \
voice-led and played like a player would. To make a song that sounds finished: {}",
        crate::agent::produce::WORKFLOW
    )
}

/// Where an agent runs: the song's folder, or, for a song not saved yet (its folder isn't made
/// until it is), the nearest folder that is.
pub(crate) fn workdir(project: &Path) -> PathBuf {
    project
        .ancestors()
        .find(|d| d.is_dir())
        .unwrap_or(Path::new("/"))
        .to_path_buf()
}

/// Claude Code with Pina Studio's tools and nothing else: no shell, no files, no one else's MCP servers
/// or hooks; anything else asked for is refused, not asked about.
pub(crate) fn claude_command(
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
        .args(["--tools", "", "--allowedTools", "mcp__pina"])
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
pub(crate) fn tuning_args(kind: Kind, tuning: &Tuning, resume: Option<&str>) -> Vec<String> {
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

/// `codex exec`, resuming the conversation after its first question: Pina Studio's MCP server in place
/// of any others (its tools let through without asking, as nothing can be asked), its sandbox
/// read-only, nothing asked about.
pub(crate) fn codex_command(
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
pub(crate) fn codex_servers() -> Vec<String> {
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

/// What makes Pina Studio's MCP server Codex's only tools: it, its tools approved (Codex asks before
/// an MCP tool runs, and with no one to ask, refuses), and the person's other servers, plugins,
/// apps, computer and browser use off. Features are turned off by key, which a Codex without them
/// ignores (its `--disable` refuses a name it doesn't know).
pub(crate) fn codex_isolation(exe: &Path, servers: &[String]) -> Vec<String> {
    let mut a = vec![
        "-c".into(),
        format!(
            "mcp_servers.pina={{command={:?},args=[\"mcp\"],default_tools_approval_mode=\"approve\"}}",
            exe.display().to_string()
        ),
    ];
    // By bare name: Codex's `-c` paths don't take quoted ones (a name that needs quotes, rare,
    // stays as the person set it up).
    let bare = |n: &&String| {
        n.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    };
    for name in servers.iter().filter(|n| *n != "pina").filter(bare) {
        a.extend(["-c".into(), format!("mcp_servers.{name}.enabled=false")]);
    }
    for feature in ["plugins", "apps", "computer_use", "browser_use"] {
        a.extend(["-c".into(), format!("features.{feature}=false")]);
    }
    a
}
