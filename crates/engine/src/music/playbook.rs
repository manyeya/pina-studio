//! What a genre is, for an agent that knows the words but can't hear: its tempo and swing, the
//! parts it's made of and the sounds they take, how its songs are laid out, and where its mixes
//! sit. Mix targets are measured from released songs (`analysis::Spectrum`, dB of the total).
//!
//! Playbooks are JSON files: Pina Studio's own (in `playbooks/`, built in) and the person's, in
//! `~/Library/Application Support/Pina Studio/playbooks/`, one a genre. One there with a built-in's
//! id replaces it; a new id adds a genre. The folder starts as a copy of Pina Studio's, to edit.

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
    /// `write_drums` style (`theory::GROOVES`), and the swing it has (50 straight).
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
}

/// Pina Studio's own, by id.
const BUILT_IN: &[(&str, &str)] = &[
    ("lofi", include_str!("../../playbooks/lofi.json")),
    ("boom_bap", include_str!("../../playbooks/boom_bap.json")),
    ("trap", include_str!("../../playbooks/trap.json")),
    ("drill", include_str!("../../playbooks/drill.json")),
    ("rnb", include_str!("../../playbooks/rnb.json")),
    ("neo_soul", include_str!("../../playbooks/neo_soul.json")),
    ("pop", include_str!("../../playbooks/pop.json")),
    ("house", include_str!("../../playbooks/house.json")),
    (
        "deep_house",
        include_str!("../../playbooks/deep_house.json"),
    ),
    ("techno", include_str!("../../playbooks/techno.json")),
    ("afrobeats", include_str!("../../playbooks/afrobeats.json")),
    ("amapiano", include_str!("../../playbooks/amapiano.json")),
    ("dnb", include_str!("../../playbooks/dnb.json")),
    ("reggaeton", include_str!("../../playbooks/reggaeton.json")),
    ("edm", include_str!("../../playbooks/edm.json")),
    ("cinematic", include_str!("../../playbooks/cinematic.json")),
    ("ambient", include_str!("../../playbooks/ambient.json")),
];

const README: &str = "\
These are Pina Studio's genre playbooks, one JSON file a genre: what agents follow to make a song in it,
and what pina critique checks a song against. Edit them, or add a file to add a genre (its id is
what set_genre takes). Delete one to get Pina Studio's own back; delete the folder for all of them.
`pina playbook` says if a file can't be read, and why.

  id, name     what it's called
  tempo        [slowest, fastest] bpm
  minor        whether its keys are mostly minor
  groove       write_drums' style: boom_bap, lofi, trap, drill, house, techno, afrobeats,
               amapiano, rnb, pop, reggaeton, dnb, rock or neo_soul
  swing        50 is straight, up to 75
  harmony      words pina theory finds progressions by (a genre or mood)
  parts        each part: its role (drums, perc, bass, chords, pad, lead, counter, arp, fx,
               vocals, texture), the sound to look for, and how to write it
  sections     the song's layout: name, bars, the parts (roles) playing, energy 1-5
  lufs         a finished master's loudness
  spectrum     a finished mix's sub, bass, low mid, mid, presence and air, in dB of the total
               (pina analyze on a song you love says what it has)
  notes        what makes it sound right, for the agent
";

/// The person's playbooks folder (`PINA_PLAYBOOKS` moves it). None in tests, which use only
/// Pina Studio's own.
pub fn dir() -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    if let Some(d) = std::env::var_os("PINA_PLAYBOOKS") {
        return Some(PathBuf::from(d));
    }
    Some(crate::paths::support().join("playbooks"))
}

/// Every playbook, Pina Studio's with the person's over them; and the files that couldn't be read,
/// saying why.
pub fn load() -> (Vec<Playbook>, Vec<String>) {
    load_from(dir().as_deref())
}

/// As `load`, the person's from `dir`, which is made (a copy of Pina Studio's, to edit) the first
/// time.
// ponytail: a playbook copied into the folder shadows later changes to Pina Studio's own; deleting the
// file brings Pina Studio's back.
pub fn load_from(dir: Option<&Path>) -> (Vec<Playbook>, Vec<String>) {
    let mut all: Vec<Playbook> = BUILT_IN
        .iter()
        .map(|(id, json)| serde_json::from_str(json).unwrap_or_else(|e| panic!("{id}: {e}")))
        .collect();
    let mut problems = Vec::new();
    let Some(dir) = dir else {
        return (all, problems);
    };
    if !dir.exists() && std::fs::create_dir_all(dir).is_ok() {
        for (id, json) in BUILT_IN {
            let _ = std::fs::write(dir.join(format!("{id}.json")), json);
        }
        let _ = std::fs::write(dir.join("README.txt"), README);
    }
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

/// Where a playbook comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Pina Studio's, as it ships.
    Pina,
    /// Pina Studio's, changed by the person.
    Edited,
    /// The person's own genre.
    Yours,
}

/// Pina Studio's own playbook for `id`, as it ships.
pub fn builtin(id: &str) -> Option<Playbook> {
    BUILT_IN
        .iter()
        .find(|(b, _)| *b == id)
        .and_then(|(_, json)| serde_json::from_str(json).ok())
}

pub fn origin(p: &Playbook) -> Origin {
    match builtin(&p.id) {
        None => Origin::Yours,
        Some(b) if b == *p => Origin::Pina,
        Some(_) => Origin::Edited,
    }
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

/// Takes the person's playbook for `id` away: Pina Studio's own comes back, or a genre of theirs is
/// gone.
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

/// The playbook for a genre's id or name ("lo-fi", "UK drill"), the nearest it knows.
pub fn find(genre: &str) -> Option<Playbook> {
    find_in(load().0, genre)
}

fn find_in(all: Vec<Playbook>, genre: &str) -> Option<Playbook> {
    let g = genre.trim().to_lowercase().replace([' ', '-'], "_");
    let alias = match g.as_str() {
        "lo_fi" | "lofi_hip_hop" | "chillhop" => "lofi",
        "hip_hop" | "hiphop" | "boombap" => "boom_bap",
        "uk_drill" | "ny_drill" => "drill",
        "r&b" | "randb" | "r_n_b" => "rnb",
        "soul" | "neosoul" => "neo_soul",
        "afro_house" | "garage" => "deep_house",
        "afrobeat" | "afropop" | "afro" => "afrobeats",
        "piano" | "yanos" => "amapiano",
        "drum_and_bass" | "drum_n_bass" | "jungle" | "liquid" => "dnb",
        "future_bass" | "electronic" | "dance" => "edm",
        "film" | "score" | "orchestral" | "trailer" => "cinematic",
        "chill" | "drone" => "ambient",
        "latin" | "dembow" => "reggaeton",
        other => other,
    };
    all.into_iter()
        .find(|p| p.id == alias || p.name.to_lowercase().replace([' ', '-'], "_") == g)
}

/// Every genre there's a playbook for, by id.
pub fn ids() -> Vec<String> {
    load().0.into_iter().map(|p| p.id).collect()
}

#[cfg(test)]
mod tests;
