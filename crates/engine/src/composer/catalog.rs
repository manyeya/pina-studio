//! The composer models that have been run with Pina Studio, and where their makers host them:
//! downloaded from there when the person asks (Pina Studio ships none). Each says what its host
//! says of its licence; which to use, and on what terms, is the person's to decide. Their files
//! are pinned to the commit they were run at, so each is known to the byte.

use std::{
    path::{Path, PathBuf},
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

/// `k` downloaded into `dir`, `token` sent where its makers ask the person to sign in. A model
/// whose terms haven't been accepted says so, and where.
pub fn download(
    k: &Known,
    dir: &Path,
    token: Option<&str>,
    progress: impl FnMut(u64),
    stop: &AtomicBool,
) -> Result<PathBuf, String> {
    let sign_in = || {
        format!(
            "{}'s makers ask you to sign in first: accept their terms at {}, then give Pina Studio a Hugging Face token",
            k.name, k.page
        )
    };
    if k.gated && token.is_none() {
        return Err(sign_in());
    }
    let to = k.file_in(dir);
    if let Some(d) = to.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let header = token.map(|t| format!("Authorization: Bearer {t}"));
    crate::separate::catalog::fetch_as(k.url, k.bytes, &to, header.as_deref(), progress, stop)
        .map_err(|e| match e.contains(" 401") || e.contains(" 403") {
            true => sign_in(),
            false => e,
        })?;
    Ok(to)
}
