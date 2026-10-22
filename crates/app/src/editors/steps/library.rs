//! The patterns the browser offers: Pina Studio's own (drum grooves, bass lines, melodies,
//! experiments, templates) and those saved in the user's folders.

use std::path::{Path, PathBuf};

use pina_engine::{
    music::theory,
    song::project::{Lane, PPQ, Pattern, PatternRow, PlayMode},
};

/// A pattern in the browser: one of Pina Studio's, by folder and name; or a file of the user's.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    Factory(&'static str, String),
    User(PathBuf),
}

/// The browser's top folders.
pub const FOLDERS: [&str; 6] = [
    "User",
    "Drums",
    "Bass",
    "Melodic",
    "Experimental",
    "Templates",
];

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

/// A name from an id: "boom_bap" → "Boom Bap".
fn title(id: &str) -> String {
    match id {
        "rnb" => return "R&B".into(),
        "dnb" => return "Drum & Bass".into(),
        "lofi" => return "Lo-Fi".into(),
        _ => {}
    }
    id.split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A row of a written-out pattern: its note, then its steps (x 100, X 127, o 70, g 40, . off),
/// and each step's note up or down (the same length, or empty).
fn row(pitch: u8, steps: &str, notes: &[i8]) -> PatternRow {
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
    if !notes.is_empty() {
        r.melodic = true;
        r.transpose = (0..r.cells.len()).map(|i| notes[i % notes.len()]).collect();
        r.octave = r.transpose.iter().map(|_| 0).collect();
        for (i, t) in r.transpose.clone().iter().enumerate() {
            // Notes past an octave go there by octave.
            if *t > 12 || *t < -12 {
                r.transpose[i] = t % 12;
                r.octave[i] = t / 12;
            }
        }
    }
    r.tidy_lanes();
    r
}

fn pattern(rows: Vec<PatternRow>) -> Pattern {
    Pattern::new(PPQ / 4, rows, 50.0)
}

/// The patterns of one of Pina Studio's folders, by name.
pub fn factory(folder: &str) -> Vec<(String, Pattern)> {
    const KICK: u8 = 36;
    const SNARE: u8 = 38;
    const CLAP: u8 = 39;
    const HAT: u8 = 42;
    const OPEN: u8 = 46;
    const BASS: u8 = 36;
    const KEYS: u8 = 60;
    match folder {
        "Drums" => theory::GROOVES
            .iter()
            .map(|g| {
                let (per_bar, rows) = theory::drums(g, 2, 0, 1);
                let rows = rows
                    .into_iter()
                    .filter(|(_, cells)| cells.iter().any(|&c| c > 0))
                    .map(|(pitch, cells)| {
                        let mut r = PatternRow::note(pitch, cells.len());
                        r.cells = cells;
                        r
                    })
                    .collect();
                let mut p = Pattern::new(PPQ * 4 / per_bar as u64, rows, g.swing);
                p.set_length(p.steps().min(64));
                (title(g.id), p)
            })
            .collect(),
        "Bass" => vec![
            (
                "Root Pulse".into(),
                pattern(vec![row(BASS, "x.x.x.x.x.x.x.x.", &[0])]),
            ),
            (
                "Octave Bounce".into(),
                pattern(vec![row(BASS, "x.xxx.xxx.xxx.xx", &[0, 0, 12, 0])]),
            ),
            (
                "Disco Octaves".into(),
                pattern(vec![row(BASS, "xxxxxxxxxxxxxxxx", &[0, 12])]),
            ),
            (
                "Syncopated Funk".into(),
                pattern(vec![row(
                    BASS,
                    "X..x..x.x.o..x.o",
                    &[0, 0, 0, 0, 0, 0, 7, 0, 10, 0, 12, 0, 0, 10, 0, 7],
                )]),
            ),
            ("Walking".into(), {
                let mut p = pattern(vec![row(BASS, "xxxxxxxx", &[0, 4, 7, 9, 12, 9, 7, 4])]);
                p.step = PPQ;
                p
            }),
            ("Acid Line".into(), {
                let mut r = row(
                    BASS,
                    "x.xxx.x.xx.x.xxx",
                    &[0, 0, 12, 0, 3, 0, 0, 0, 15, 12, 0, 7, 0, 0, 12, 10],
                );
                r.tie = vec![
                    false, false, false, true, false, false, false, false, true, false, false,
                    false, false, false, true, false,
                ];
                let mut p = pattern(vec![r]);
                p.legato = true;
                p
            }),
            (
                "Reggaeton Bass".into(),
                pattern(vec![row(
                    BASS,
                    "x..x..x.x..x..x.",
                    &[0, 0, 0, 0, 0, 0, 5, 0, 7, 0, 0, 7, 0, 0, 5, 0],
                )]),
            ),
        ],
        "Melodic" => vec![
            (
                "Arp Up".into(),
                pattern(vec![row(KEYS, "xxxxxxxxxxxxxxxx", &[0, 4, 7, 12])]),
            ),
            (
                "Arp Down".into(),
                pattern(vec![row(KEYS, "xxxxxxxxxxxxxxxx", &[12, 7, 4, 0])]),
            ),
            (
                "Broken Chord".into(),
                pattern(vec![row(
                    KEYS,
                    "x.xxx.xxx.xxx.xx",
                    &[0, 0, 7, 4, 12, 0, 7, 4],
                )]),
            ),
            (
                "Pentatonic Run".into(),
                pattern(vec![row(
                    KEYS,
                    "xxxxxxxxxxxxxxxx",
                    &[0, 2, 4, 7, 9, 12, 9, 7],
                )]),
            ),
            (
                "Fifths Sequence".into(),
                pattern(vec![row(
                    KEYS,
                    "x.x.x.x.x.x.x.x.",
                    &[0, 0, 7, 0, 5, 0, 12, 0, 3, 0, 10, 0, 7, 0, 5, 0],
                )]),
            ),
            (
                "Pulse Chords".into(),
                pattern(vec![
                    row(KEYS + 7, "x..x..x...x..x..", &[]),
                    row(KEYS + 4, "x..x..x...x..x..", &[]),
                    row(KEYS, "x..x..x...x..x..", &[]),
                ]),
            ),
            (
                "Offbeat Stabs".into(),
                pattern(vec![
                    row(KEYS + 10, "..x...x...x...x.", &[]),
                    row(KEYS + 7, "..x...x...x...x.", &[]),
                    row(KEYS + 3, "..x...x...x...x.", &[]),
                ]),
            ),
        ],
        "Experimental" => vec![
            ("Polyrhythm 3:4:5".into(), {
                let mut p = pattern(vec![
                    row(KICK, "x...............", &[]),
                    row(SNARE, "x..", &[]),
                    row(HAT, "x....", &[]),
                ]);
                p.set_length(16);
                (p.rows[0].loop_end, p.rows[1].loop_end, p.rows[2].loop_end) =
                    (Some(4), Some(3), Some(5));
                p
            }),
            ("Ping-Pong Arp".into(), {
                let mut p = pattern(vec![row(KEYS, "xxxxxxxx", &[0, 3, 7, 10, 12, 15, 19, 24])]);
                p.mode = PlayMode::PingPong;
                p
            }),
            ("Wandering Melody".into(), {
                let mut p = pattern(vec![row(
                    KEYS,
                    "xxxxxxxxxxxxxxxx",
                    &[0, 2, 3, 5, 7, 8, 10, 12],
                )]);
                p.mode = PlayMode::Wander;
                p
            }),
            ("Random Hats".into(), {
                let mut r = row(HAT, "xxxxxxxxxxxxxxxx", &[]);
                r.chance = vec![70; 16];
                r.mode = Some(PlayMode::Random);
                pattern(vec![row(KICK, "x...x...x...x...", &[]), r])
            }),
            ("Ratchet Rolls".into(), {
                let mut r = row(HAT, "x.x.x.x.x.x.x.xx", &[]);
                r.repeat = vec![1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 3, 1, 4, 2];
                r.ramp = vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -40, 0, 60, 0];
                pattern(vec![
                    row(KICK, "x.....x...x.....", &[]),
                    row(CLAP, "....x.......x...", &[]),
                    r,
                ])
            }),
            ("Shuffled Steps".into(), {
                let mut p = pattern(vec![row(KEYS, "xxxxxxxx", &[0, 4, 7, 11, 12, 11, 7, 4])]);
                p.mode = PlayMode::Shuffle;
                p
            }),
            ("Every Other Fill".into(), {
                let mut s = row(SNARE, "....x.......x.xx", &[]);
                s.condition = vec![pina_engine::song::project::Condition::Always; 16];
                s.every = vec![[1, 1]; 16];
                for i in [13, 14, 15] {
                    s.condition[i] = pina_engine::song::project::Condition::Recurrence;
                    s.every[i] = [2, 2];
                }
                pattern(vec![
                    row(KICK, "x.......x.x.....", &[]),
                    s,
                    row(OPEN, "..x...x...x...x.", &[]),
                ])
            }),
        ],
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

/// A pattern read from the user's file, or one of Pina Studio's.
pub fn read(entry: &Entry) -> Option<(String, Pattern, bool)> {
    match entry {
        Entry::Factory(folder, name) => {
            let p = factory(folder).into_iter().find(|(n, _)| n == name)?.1;
            Some((name.clone(), p, *folder == "Templates"))
        }
        Entry::User(path) => {
            let s: Saved = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
            let mut p = s.pattern;
            p.upgrade();
            Some((s.name, p, s.template))
        }
    }
}
