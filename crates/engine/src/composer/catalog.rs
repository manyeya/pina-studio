//! The composer models that have been run with Pina Studio, and where their makers host them:
//! downloaded from there when the person asks (Pina Studio ships none). Each says what its host
//! says of its licence; which to use, and on what terms, is the person's to decide. Their files
//! are pinned to the commit they were run at, so each is known to the byte.

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

/// A kind of model: what installs it and serves it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    MidiGpt,
}

impl Family {
    pub fn name(self) -> &'static str {
        match self {
            Family::MidiGpt => "MIDI-GPT",
        }
    }

    /// Its folder in the composers' folder: its Python, shared by its models.
    pub fn id(self) -> &'static str {
        match self {
            Family::MidiGpt => "midigpt",
        }
    }

    /// The Python package that serves it, at the version run with Pina Studio.
    pub fn package(self) -> &'static str {
        match self {
            Family::MidiGpt => "midigpt[http]==0.4.2",
        }
    }

    /// Its server, as its package installs it.
    pub fn server(self) -> &'static str {
        match self {
            Family::MidiGpt => "midigpt-http",
        }
    }
}

#[derive(Debug)]
pub struct Known {
    pub id: &'static str,
    pub name: &'static str,
    pub family: Family,
    pub maker: &'static str,
    /// What it's good at, plainly.
    pub about: &'static str,
    pub url: &'static str,
    pub bytes: u64,
    /// What its host says of its licence, and where to read it.
    pub licence: &'static str,
    pub licence_url: &'static str,
    /// Its makers ask the person to sign in and accept their terms first, at `page`.
    pub gated: bool,
    pub page: &'static str,
    /// How many bars it hears at once (its windows), and whether it places notes off the grid.
    pub bars: &'static [u32],
    pub microtiming: bool,
}

const MIDIGPT_LICENCE: &str =
    "Metacreation Non-Commercial Research License: non-commercial research only (the code is MIT)";
const MIDIGPT_LICENCE_URL: &str =
    "https://huggingface.co/Metacreation/MIDI-GPT/blob/main/LICENSE-WEIGHTS.md";
const MIDIGPT_PAGE: &str = "https://huggingface.co/Metacreation/MIDI-GPT";

pub const KNOWN: &[Known] = &[
    Known {
        id: "midigpt-yellow-small",
        name: "MIDI-GPT Yellow Small",
        family: Family::MidiGpt,
        maker: "Metacreation Lab",
        about: "The quickest: hears four or eight bars at a time. Drums steered by density, other parts by polyphony and note length.",
        url: "https://huggingface.co/Metacreation/MIDI-GPT/resolve/9a21a3976ce4fbd78e607171c679d7b85f8b8625/yellow_small-final.safetensors",
        bytes: 82_515_744,
        licence: MIDIGPT_LICENCE,
        licence_url: MIDIGPT_LICENCE_URL,
        gated: true,
        page: MIDIGPT_PAGE,
        bars: &[4, 8],
        microtiming: false,
    },
    Known {
        id: "midigpt-yellow-medium",
        name: "MIDI-GPT Yellow Medium",
        family: Family::MidiGpt,
        maker: "Metacreation Lab",
        about: "Yellow, larger: the same controls, more musical, slower.",
        url: "https://huggingface.co/Metacreation/MIDI-GPT/resolve/9a21a3976ce4fbd78e607171c679d7b85f8b8625/yellow_medium-final.safetensors",
        bytes: 350_508_072,
        licence: MIDIGPT_LICENCE,
        licence_url: MIDIGPT_LICENCE_URL,
        gated: true,
        page: MIDIGPT_PAGE,
        bars: &[4, 8],
        microtiming: false,
    },
    Known {
        id: "midigpt-prism-medium",
        name: "MIDI-GPT Prism Medium",
        family: Family::MidiGpt,
        maker: "Metacreation Lab",
        about: "Hears up to sixteen bars, and is steered by key, pitch range, note length and density, bar by bar.",
        url: "https://huggingface.co/Metacreation/MIDI-GPT/resolve/9a21a3976ce4fbd78e607171c679d7b85f8b8625/prism_medium-final.safetensors",
        bytes: 358_006_400,
        licence: MIDIGPT_LICENCE,
        licence_url: MIDIGPT_LICENCE_URL,
        gated: true,
        page: MIDIGPT_PAGE,
        bars: &[4, 8, 12, 16],
        microtiming: false,
    },
    Known {
        id: "midigpt-expressive-medium",
        name: "MIDI-GPT Expressive Medium",
        family: Family::MidiGpt,
        maker: "Metacreation Lab",
        about: "Prism's controls, and played: notes off the grid and 128 velocities, for parts that feel performed.",
        url: "https://huggingface.co/Metacreation/MIDI-GPT/resolve/9a21a3976ce4fbd78e607171c679d7b85f8b8625/expressive_medium-final.safetensors",
        bytes: 358_750_104,
        licence: MIDIGPT_LICENCE,
        licence_url: MIDIGPT_LICENCE_URL,
        gated: true,
        page: MIDIGPT_PAGE,
        bars: &[4, 8, 12, 16],
        microtiming: true,
    },
];

impl Known {
    pub fn named(id: &str) -> Option<&'static Known> {
        KNOWN.iter().find(|k| k.id == id)
    }

    /// Its file in `dir`.
    pub fn file_in(&self, dir: &Path) -> PathBuf {
        dir.join(self.id).join("model.safetensors")
    }
}

/// `k` downloaded into `dir`, `token` sent where its makers ask the person to sign in. When the
/// host won't give it, why, as the host says it, and what to do.
pub fn download(
    k: &Known,
    dir: &Path,
    token: Option<&str>,
    progress: impl FnMut(u64),
    stop: &AtomicBool,
) -> Result<PathBuf, String> {
    if k.gated && token.is_none() {
        return Err(format!(
            "{}'s makers ask you to sign in first: accept their terms at {}, then give Pina Studio a Hugging Face token",
            k.name, k.page
        ));
    }
    if let Some(t) = token {
        allowed(k, t)?;
    }
    let to = k.file_in(dir);
    if let Some(d) = to.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let header = token.map(|t| format!("Authorization: Bearer {t}"));
    let got =
        crate::separate::catalog::fetch_as(k.url, k.bytes, &to, header.as_deref(), progress, stop);
    if got.is_err()
        && let Some(d) = to.parent()
    {
        // Nothing came down: no empty folder left to look like a model.
        let _ = std::fs::remove_dir(d);
    }
    got.map(|_| to)
}

/// Whether the host gives `k` to whoever `token` is: its first byte asked for. Refused, why,
/// in the host's terms: the token not taken, the account not given access yet, or a token
/// that may not read gated models.
fn allowed(k: &Known, token: &str) -> Result<(), String> {
    let ask = |url: &str, range: bool| {
        let mut c = Command::new("curl");
        c.args([
            "-sS",
            "-L",
            "-A",
            "PinaStudio",
            "--max-time",
            "30",
            "-o",
            "-",
        ])
        .args(["-w", "\n%{http_code}", "-H"])
        .arg(format!("Authorization: Bearer {token}"));
        if range {
            c.args(["-r", "0-0"]);
        }
        let out = c
            .arg(url)
            .output()
            .map_err(|e| format!("Couldn't start curl: {e}"))?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let (body, code) = text.rsplit_once('\n').unwrap_or(("", &text));
        Ok::<_, String>((body.to_string(), code.trim().to_string()))
    };
    let (body, code) = ask(k.url, true)?;
    match code.as_str() {
        "200" | "206" => Ok(()),
        "401" => Err(
            "Hugging Face didn't take the token: it may have been mistyped or deleted. Make a new one at huggingface.co/settings/tokens (Read is enough) and paste it in Settings › Composers"
                .into(),
        ),
        "403" => {
            // A fine-grained token may only read gated models when it's been allowed to.
            let fine = ask("https://huggingface.co/api/whoami-v2", false)
                .ok()
                .and_then(|(b, _)| serde_json::from_str::<serde_json::Value>(&b).ok())
                .is_some_and(|w| w["auth"]["accessToken"]["role"] == "fineGrained");
            let token_too = match fine {
                true => " Your token is fine-grained: in its settings on Hugging Face, tick “Read access to contents of all public gated repos you can access” (or make a Read token instead).",
                false => "",
            };
            Err(match body.contains("authorized list") || body.contains("ask for access") {
                true => format!(
                    "Hugging Face hasn't given your account {} yet: open {} signed in to Hugging Face and agree to its terms (access comes straight away), then Download again.{token_too}",
                    k.name, k.page
                ),
                false => format!("Hugging Face refused the download: {}{token_too}", body.trim()),
            })
        }
        "" | "000" => Err("Couldn't reach Hugging Face: check the connection".into()),
        c => Err(format!("Hugging Face answered {c}: {}", body.trim())),
    }
}
