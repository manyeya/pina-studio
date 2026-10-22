//! The agents Pina Studio knows, how each is run, and whether it's ready.

use super::*;

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

/// What `codex login status` said: signed in (and how), or not.
pub(crate) fn codex_status(ok: bool, said: &str) -> Readiness {
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

impl Kind {
    pub fn def(self) -> &'static Def {
        AGENTS.iter().find(|d| d.kind == self).unwrap()
    }

    pub fn from_id(id: &str) -> Option<Kind> {
        AGENTS.iter().find(|d| d.id == id.trim()).map(|d| d.kind)
    }

    /// Where its command is: `PINA_<COMMAND>` (PINA_CLAUDE), else on the PATH or where its
    /// installer puts it (an app opened from the Finder has a short PATH).
    pub fn command(self) -> Option<PathBuf> {
        let bin = self.def().bin;
        if let Some(p) = std::env::var_os(format!("PINA_{}", bin.to_uppercase())) {
            return Some(p.into());
        }
        crate::platform::find_command(bin, &crate::platform::tool_folders())
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
