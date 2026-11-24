//! The command lines that start each agent, with Pina Studio's tools and instructions.

use super::*;

pub(crate) fn instructions(project: &Path) -> String {
    let p = project.display();
    format!(
        "You're working inside Pina Studio, a DAW, on the song open at {p}. The person is watching: \
every edit you make appears in their window as you make it, so work in small steps, and keep \
your words short: a sentence before a group of steps saying what you're doing, and a line or two \
at the end saying what it does for the song. Use Pina Studio's tools, passing \"{p}\" as the project: `describe` \
before your first edit (how ticks, levels and notes work, and a line a command; `describe` with \
`commands` for the whole entry of each command you use), `show` to read the \
song, `edit` to change it, `samples_find`, `samples_kit` and `samples_put` for sounds that fit, \
`analyze` to check levels. The person can pick sounds, instruments and effects for the song \
themselves (`palette`, and \
listed in their message when they change): build with those before choosing your own, each for \
the part they gave it, and with no others when they ask for only these. You can't hear the song: `critique` is your ears (quick after each \
edit, with a render after mixing), trust it and the samples' analysis, and say what you couldn't \
check. You can search the web (`WebSearch`, `WebFetch`): research an artist, record or genre the \
person names before writing in its style, and say what you found.\n\nShow rather than tell, with `present`: its cards are what the person looks at. For anything \
of more than a few steps, a `plan` first, sent again as each step finishes. When a real choice is \
theirs, `choices`, then stop and wait; don't ask in words. Levels and loudness as `metrics`, a \
progression as `chords`, a song's form as `sections`, and `next` at the end with what you'd do \
next. Don't repeat a card in words, and don't list what you changed: the panel shows that itself.\n\nWrite the music yourself, note by note, as a producer and a player would. Pina Studio \
keeps no templates, progressions or grooves of its own: the person's playbooks hold theirs. Before \
writing a part, know what it has to fit: the song's key and tempo (each message starts with them), \
the notes every other part plays (`show`), and what its audio loops play as they sound in the song \
(`study` a loop's bounce: by its sound, not its file's name). When the person names a genre, write in \
that genre's own idiom, the way its records are made: its chord qualities, voicings and register, \
how long its chords hold, its rhythms and its sounds. Follow what they have first: `theory` with the \
genre for their own progressions, and `midi_find` then `midi_study` their packs' MIDI in the genre \
for how its parts really go. With nothing of theirs to follow, don't bet on one guess: write a \
small set, two or three short takes of the part that differ in how it plays (held, rhythmic, \
melodic), each with `midi_write` so they can hear it in the panel, offer them with `choices`, and \
put only the one they pick into the song. What the person says a genre is outranks what you know \
or find: when they correct you, keep to it for the rest of the conversation. To make a song that \
sounds finished: {}",
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

/// What Pina Studio's MCP server is called to the agents: its tools are `mcp__pina__…`, which is
/// what's allowed and what the panel reads.
const SERVER: &str = "pina";

/// Claude Code with Pina Studio's tools and the web, and nothing else: no shell, no files, no one
/// else's MCP servers or hooks; anything else asked for is refused, not asked about.
pub(crate) fn claude_command(
    cmd: &Path,
    project: &Path,
    tuning: &Tuning,
    resume: Option<&str>,
) -> Result<Child, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mcp = json!({ "mcpServers": { SERVER: { "command": exe, "args": ["mcp"] } } });
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
        // The web, to research the records and genres the person names; no shell, no files.
        .args([
            "--tools",
            "WebSearch,WebFetch",
            "--allowedTools",
            &format!("mcp__{SERVER},WebSearch,WebFetch"),
        ])
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
            // The web, to research the records and genres the person names.
            "-c",
            "web_search=\"live\"",
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
            "mcp_servers.{SERVER}={{command={:?},args=[\"mcp\"],default_tools_approval_mode=\"approve\"}}",
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
