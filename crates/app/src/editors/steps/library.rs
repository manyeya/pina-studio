//! The patterns the browser offers: the user's own, saved in their folders; the beat of each of
//! their playbooks; and empty templates to start from (rows set up, no steps on). Pina Studio
//! keeps no grooves or lines of its own.

use std::path::{Path, PathBuf};

use pina_engine::{
    music::playbook::{self, Playbook},
    song::project::{Lane, PPQ, Pattern, PatternRow},
};

/// A pattern in the browser: an empty template of Pina Studio's, by folder and name; a
/// playbook's beat, by its id; or a file of the user's.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    Factory(&'static str, String),
    Book(String),
    User(PathBuf),
}

/// The browser's top folders.
pub const FOLDERS: [&str; 3] = ["User", "Playbooks", "Templates"];

/// Where the user's patterns and templates are kept.
pub fn user_dir() -> PathBuf {
    pina_engine::paths::support().join("Patterns")
}

/// A pattern file: its name and the pattern, with whether it's a template (its settings and
/// rows, no steps on).
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Saved {
    pub name: String,
    #[serde(default)]
    pub template: bool,
    pub pattern: Pattern,
}

/// A row of a beat: its note, then its steps (X 127, x 100, o 70, g 40, . off).
fn row(pitch: u8, steps: &str) -> PatternRow {
    let cells: Vec<u8> = steps
        .chars()
        .map(|c| match c {
            'X' => 127,
            'x' => 100,
            'o' => 70,
            'g' => 40,
            _ => 0,
        })
        .collect();
    let mut r = PatternRow::note(pitch, cells.len());
    r.cells = cells;
    r
}

fn pattern(rows: Vec<PatternRow>) -> Pattern {
    Pattern::new(PPQ / 4, rows, 50.0)
}

/// A playbook's beat as a pattern: its kick, snare and hats, at its swing; none if it has no beat.
pub fn beat(p: &Playbook) -> Option<Pattern> {
    let b = p.beat.as_ref()?;
    let rows: Vec<PatternRow> = [(36, &b.kick), (38, &b.snare), (42, &b.hats)]
        .into_iter()
        .filter(|(_, steps)| !steps.is_empty())
        .map(|(pitch, steps)| row(pitch, steps))
        .collect();
    (!rows.is_empty()).then(|| Pattern::new(PPQ / 4, rows, p.swing))
}

/// The empty templates of one of Pina Studio's folders, by name.
pub fn factory(folder: &str) -> Vec<(String, Pattern)> {
    const KICK: u8 = 36;
    const SNARE: u8 = 38;
    const CLAP: u8 = 39;
    const HAT: u8 = 42;
    const OPEN: u8 = 46;
    const BASS: u8 = 36;
    const KEYS: u8 = 60;
    match folder {
        "Templates" => vec![
            (
                "Drum Kit".into(),
                pattern(
                    [KICK, SNARE, CLAP, HAT, OPEN]
                        .map(|p| PatternRow::note(p, 16))
                        .to_vec(),
                ),
            ),
            (
                "Drum Kit with Toms".into(),
                pattern(
                    [KICK, SNARE, CLAP, HAT, OPEN, 41, 45, 48, 49]
                        .map(|p| PatternRow::note(p, 16))
                        .to_vec(),
                ),
            ),
            ("Melodic".into(), {
                let mut r = PatternRow::note(KEYS, 16);
                r.melodic = true;
                r.open = true;
                r.subrows = vec![Lane::Note, Lane::Octave];
                pattern(vec![r])
            }),
            ("Bass".into(), {
                let mut r = PatternRow::note(BASS, 16);
                r.melodic = true;
                r.open = true;
                r.subrows = vec![Lane::Note, Lane::Octave, Lane::Tie];
                pattern(vec![r])
            }),
            (
                "Chord Rows".into(),
                pattern(
                    [KEYS + 11, KEYS + 7, KEYS + 4, KEYS]
                        .map(|p| PatternRow::note(p, 16))
                        .to_vec(),
                ),
            ),
        ],
        _ => Vec::new(),
    }
}

/// The user's folders under one, and the patterns in it.
pub fn user_folder(dir: &Path) -> (Vec<String>, Vec<(String, PathBuf)>) {
    let mut folders = Vec::new();
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let path = e.path();
            let name = path
                .file_stem()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            if path.is_dir() {
                folders.push(name);
            } else if path.extension().is_some_and(|x| x == "json") {
                files.push((name, path));
            }
        }
    }
    folders.sort();
    files.sort();
    (folders, files)
}

/// A pattern read from the user's file, a playbook's beat, or one of Pina Studio's templates.
pub fn read(entry: &Entry) -> Option<(String, Pattern, bool)> {
    match entry {
        Entry::Factory(folder, name) => {
            let p = factory(folder).into_iter().find(|(n, _)| n == name)?.1;
            Some((name.clone(), p, true))
        }
        Entry::Book(id) => {
            let p = playbook::find(id)?;
            Some((p.name.clone(), beat(&p)?, false))
        }
        Entry::User(path) => {
            let s: Saved = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
            let mut p = s.pattern;
            p.upgrade();
            Some((s.name, p, s.template))
        }
    }
}
