//! What a genre is, for an agent that knows the words but can't hear: its tempo and swing, the
//! parts it's made of and the sounds they take, how its songs are laid out, and where its mixes
//! sit. Mix targets are measured from released songs (`analysis::Spectrum`, dB of the total).
//!
//! Playbooks are the person's own JSON files, one a genre, in
//! `~/Library/Application Support/Pina Studio/playbooks/` (made by hand, on the Playbooks page,
//! or from a song study). Pina Studio has none of its own: nothing follows one unless the
//! person asks for it, by name or by setting the song's genre to one.

pub mod moves;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A part a genre's songs have.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Part {
    /// "drums", "perc", "bass", "chords", "pad", "lead", "counter", "arp", "fx", "vocals",
    /// "texture".
    pub role: String,
    /// What to look for: words for samples and instruments' presets.
    pub sound: String,
    /// How to write it with Pina Studio's commands.
    pub write: String,
}

/// A section of a song: how long, which parts play, and how hard (1 to 5).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub name: String,
    pub bars: u64,
    pub parts: Vec<String>,
    pub energy: u8,
    /// How loud it was measured, LUFS, in a book made from a song.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lufs: Option<f32>,
}

/// A bar of drums as a song plays them: a character a step, as `theory::GROOVES` are written.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Beat {
    pub kick: String,
    pub snare: String,
    pub hats: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Playbook {
    pub id: String,
    pub name: String,
    /// The tempos it's made at, bpm.
    pub tempo: (f64, f64),
    /// Its keys are mostly minor.
    pub minor: bool,
    /// Its drum groove (one of `theory::GROOVES`), and the swing it has (50 straight).
    pub groove: String,
    pub swing: f32,
    /// Words `theory` finds progressions for it by.
    pub harmony: String,
    pub parts: Vec<Part>,
    pub sections: Vec<Section>,
    /// Integrated loudness of a finished master, LUFS.
    pub lufs: f32,
    /// Sub, bass, low mid, mid, presence and air of a finished mix, dB of the total.
    pub spectrum: [f32; 6],
    /// What makes it sound like itself, and the mistakes that make it sound amateur.
    pub notes: Vec<String>,
    /// The song's own chords, a bar each, and as numerals, in a book made from a song.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chords: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub numerals: Vec<String>,
    /// The song's own drums.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beat: Option<Beat>,
    /// The file it was made from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PathBuf>,
}

/// The person's playbooks folder (`PINA_PLAYBOOKS` moves it). In tests, a few of their own.
pub fn dir() -> Option<PathBuf> {
    if cfg!(test) {
        return Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/music/playbook/fixtures"));
    }
    if let Some(d) = std::env::var_os("PINA_PLAYBOOKS") {
        return Some(PathBuf::from(d));
    }
    Some(crate::paths::support().join("playbooks"))
}

/// The person's playbooks; and the files that couldn't be read, saying why.
pub fn load() -> (Vec<Playbook>, Vec<String>) {
    load_from(dir().as_deref())
}

/// As `load`, from `dir` (none if it isn't there).
pub fn load_from(dir: Option<&Path>) -> (Vec<Playbook>, Vec<String>) {
    let (mut all, mut problems): (Vec<Playbook>, _) = (Vec::new(), Vec::new());
    let Some(dir) = dir else {
        return (all, problems);
    };
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    for f in files {
        let read = std::fs::read_to_string(&f)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<Playbook>(&s).map_err(|e| e.to_string()))
            .and_then(|p| check(&p).map(|_| p));
        match read {
            Ok(p) => match all.iter_mut().find(|b| b.id == p.id) {
                Some(b) => *b = p,
                None => all.push(p),
            },
            Err(e) => problems.push(format!("{}: {e}", f.display())),
        }
    }
    (all, problems)
}

/// What makes a playbook unusable, said plainly.
fn check(p: &Playbook) -> Result<(), String> {
    if p.id.trim().is_empty() {
        return Err("no id".into());
    }
    if !crate::music::theory::GROOVES
        .iter()
        .any(|g| g.id == p.groove)
    {
        let ids: Vec<&str> = crate::music::theory::GROOVES.iter().map(|g| g.id).collect();
        return Err(format!(
            "groove {:?} isn't one of {}",
            p.groove,
            ids.join(", ")
        ));
    }
    if p.tempo.0 > p.tempo.1 || p.tempo.0 < 20.0 {
        return Err("tempo is [slowest, fastest], from 20 bpm".into());
    }
    if let Some(s) = p
        .sections
        .iter()
        .find(|s| s.bars == 0 || !(1..=5).contains(&s.energy))
    {
        return Err(format!(
            "section {:?}: bars at least 1, energy 1 to 5",
            s.name
        ));
    }
    Ok(())
}

/// Writes a playbook to the person's folder, as `<id>.json`, whole or not at all.
pub fn save(p: &Playbook) -> Result<PathBuf, String> {
    check(p)?;
    let dir = dir().ok_or("no playbooks folder")?;
    save_in(&dir, p)
}

fn save_in(dir: &Path, p: &Playbook) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!("{}.json", p.id));
    let part = file.with_extension("json.part");
    let json = serde_json::to_string_pretty(p).map_err(|e| e.to_string())? + "\n";
    std::fs::write(&part, json).map_err(|e| e.to_string())?;
    std::fs::rename(&part, &file).map_err(|e| e.to_string())?;
    Ok(file)
}

/// Deletes the person's playbook for `id`.
pub fn remove(id: &str) -> Result<(), String> {
    let dir = dir().ok_or("no playbooks folder")?;
    match std::fs::remove_file(dir.join(format!("{id}.json"))) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
        _ => Ok(()),
    }
}

/// An id for a new genre called `name`: its words joined, kept clear of the ones there are.
pub fn new_id(name: &str, taken: &[String]) -> String {
    let base: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    let base = if base.is_empty() {
        "genre".to_string()
    } else {
        base
    };
    let mut id = base.clone();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base}_{n}");
        n += 1;
    }
    id
}

/// The person's playbook with this id or name ("Like That Night"), if they have one.
pub fn find(genre: &str) -> Option<Playbook> {
    find_in(load().0, genre)
}

fn find_in(all: Vec<Playbook>, genre: &str) -> Option<Playbook> {
    let g = genre.trim().to_lowercase().replace([' ', '-'], "_");
    all.into_iter()
        .find(|p| p.id == g || p.name.to_lowercase().replace([' ', '-'], "_") == g)
}

/// Every genre the person has a playbook for, by id.
pub fn ids() -> Vec<String> {
    load().0.into_iter().map(|p| p.id).collect()
}

#[cfg(test)]
mod tests;
