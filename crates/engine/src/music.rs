//! Keys and scales, and the edits the piano roll makes to a region's notes in one go: quantize,
//! humanize, legato, chop, arpeggiate, transpose and velocity.

use serde::{Deserialize, Serialize};

use crate::project::Note;

const NAMES_SHARP: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
const NAMES_FLAT: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
];

/// A key: its tonic (0 is C) and whether it's minor (natural minor) or major.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub root: u8,
    pub minor: bool,
}

impl Key {
    /// Every key: the majors, then the minors.
    pub fn all() -> impl Iterator<Item = Key> {
        [false, true]
            .into_iter()
            .flat_map(|minor| (0..12).map(move |root| Key { root, minor }))
    }

    /// Reads "A minor", "Am", "A min", "Eb major", "F#", "Bb minor" and the like.
    pub fn parse(s: &str) -> Option<Key> {
        let s = s.trim();
        let mut chars = s.chars();
        let letter = chars.next()?.to_ascii_uppercase();
        let base = match letter {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => return None,
        };
        let rest = chars.as_str();
        let (shift, rest) = match rest.chars().next() {
            Some('#' | '♯') => (1, &rest[rest.chars().next().unwrap().len_utf8()..]),
            Some('b' | '♭') if !rest.starts_with("ba") => {
                (-1, &rest[rest.chars().next().unwrap().len_utf8()..])
            }
            _ => (0, rest),
        };
        let quality = rest.trim().to_ascii_lowercase();
        let minor = match quality.as_str() {
            "" | "maj" | "major" | "ma" => false,
            "m" | "min" | "minor" | "mi" => true,
            _ => return None,
        };
        let root = (base + shift + 12) % 12;
        Some(Key {
            root: root as u8,
            minor,
        })
    }

    fn tonic(&self) -> &'static str {
        // Spelled as the key signature would: flats for the flat keys.
        let flat = if self.minor {
            [0, 2, 5, 7, 10, 3].contains(&self.root) // C D F G Bb Eb minor
        } else {
            [5, 10, 3, 8, 1].contains(&self.root) // F Bb Eb Ab Db major
        };
        if flat {
            NAMES_FLAT[self.root as usize]
        } else {
            NAMES_SHARP[self.root as usize]
        }
    }

    /// "A minor".
    pub fn name(&self) -> String {
        format!(
            "{} {}",
            self.tonic(),
            if self.minor { "minor" } else { "major" }
        )
    }

    /// "A min", for tight spaces.
    pub fn short(&self) -> String {
        format!(
            "{} {}",
            self.tonic(),
            if self.minor { "min" } else { "maj" }
        )
    }

    /// Whether a pitch is in the key's scale.
    pub fn contains(&self, pitch: u8) -> bool {
        const MAJOR: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
        const MINOR: [u8; 7] = [0, 2, 3, 5, 7, 8, 10];
        let degree = (pitch + 12 - self.root) % 12;
        if self.minor {
            MINOR.contains(&degree)
        } else {
            MAJOR.contains(&degree)
        }
    }

    pub fn is_root(&self, pitch: u8) -> bool {
        pitch % 12 == self.root
    }
}

/// The key that best fits some notes, as (pitch, length in ticks): Krumhansl and Kessler's
/// profiles against how long each pitch class sounds. None without notes.
pub fn detect(notes: impl IntoIterator<Item = (u8, u64)>) -> Option<Key> {
    const MAJOR: [f64; 12] = [
        6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
    ];
    const MINOR: [f64; 12] = [
        6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
    ];
    let mut weight = [0f64; 12];
    for (pitch, length) in notes {
        weight[(pitch % 12) as usize] += length.max(1) as f64;
    }
    if weight.iter().all(|&w| w == 0.0) {
        return None;
    }
    let correlate = |profile: &[f64; 12], root: usize| {
        let mean = |v: &dyn Fn(usize) -> f64| (0..12).map(v).sum::<f64>() / 12.0;
        let (mw, mp) = (mean(&|i| weight[i]), mean(&|i| profile[i]));
        let (mut num, mut dw, mut dp) = (0.0, 0.0, 0.0);
        for i in 0..12 {
            let (w, p) = (weight[(i + root) % 12] - mw, profile[i] - mp);
            num += w * p;
            dw += w * w;
            dp += p * p;
        }
        num / (dw * dp).sqrt().max(1e-12)
    };
    Key::all().max_by(|a, b| {
        let score = |k: &Key| correlate(if k.minor { &MINOR } else { &MAJOR }, k.root as usize);
        score(a).total_cmp(&score(b))
    })
}

/// An edit to a region's notes, made in one go.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NoteEdit {
    /// Moves note starts toward the grid (ticks): all the way at strength 1. Swing (50 is
    /// straight, 66 a triplet feel) pushes every other grid step later.
    Quantize {
        grid: u64,
        #[serde(default = "full")]
        strength: f32,
        #[serde(default = "straight")]
        swing: f32,
    },
    /// Nudges starts by up to `timing` ticks and velocities by up to `velocity`, at random
    /// (the same every time for a seed).
    Humanize {
        #[serde(default = "nudge")]
        timing: u64,
        #[serde(default = "loudness")]
        velocity: u8,
        #[serde(default)]
        seed: u64,
    },
    /// Each note held until the next one starts.
    Legato,
    /// Notes longer than the grid cut into grid-long notes.
    Chop {
        grid: u64,
    },
    /// Chords played upward, a grid step apart.
    Arpeggiate {
        grid: u64,
    },
    Transpose {
        semitones: i32,
    },
    Velocity {
        by: i32,
    },
}

fn full() -> f32 {
    1.0
}

fn straight() -> f32 {
    50.0
}

fn nudge() -> u64 {
    20
}

fn loudness() -> u8 {
    8
}

impl NoteEdit {
    /// Applies the edit to a region's notes; with `only`, to that note alone (where an edit on one
    /// note makes sense: not legato or arpeggiate).
    pub fn apply(&self, notes: &mut Vec<Note>, only: Option<usize>) {
        let all = 0..notes.len();
        let chosen: Vec<usize> = match only {
            Some(i) if i < notes.len() => vec![i],
            Some(_) => return,
            None => all.collect(),
        };
        match *self {
            NoteEdit::Quantize {
                grid,
                strength,
                swing,
            } => {
                let grid = grid.max(1) as f64;
                let strength = strength.clamp(0.0, 1.0) as f64;
                let late = ((swing.clamp(50.0, 75.0) as f64) / 50.0 - 1.0) * grid;
                for &i in &chosen {
                    let start = notes[i].start as f64;
                    let step = (start / grid).round();
                    let swung = if step as i64 % 2 == 1 { late } else { 0.0 };
                    let target = step * grid + swung;
                    notes[i].start = (start + (target - start) * strength).round() as i64;
                }
            }
            NoteEdit::Humanize {
                timing,
                velocity,
                seed,
            } => {
                let mut state = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407)
                    | 1;
                let mut next = |span: i64| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    if span == 0 {
                        0
                    } else {
                        (state % (2 * span as u64 + 1)) as i64 - span
                    }
                };
                for &i in &chosen {
                    let n = &mut notes[i];
                    n.start = (n.start + next(timing as i64)).max(0);
                    n.velocity = (n.velocity as i64 + next(velocity as i64)).clamp(1, 127) as u8;
                }
            }
            NoteEdit::Legato => {
                let mut starts: Vec<i64> = notes.iter().map(|n| n.start).collect();
                starts.sort_unstable();
                starts.dedup();
                for n in notes.iter_mut() {
                    let at = starts.partition_point(|&s| s <= n.start);
                    if let Some(&next) = starts.get(at) {
                        n.length = (next - n.start) as u64;
                    }
                }
            }
            NoteEdit::Chop { grid } => {
                let grid = grid.max(1);
                let mut out = Vec::new();
                for (i, n) in notes.iter().enumerate() {
                    if !chosen.contains(&i) || n.length <= grid {
                        out.push(*n);
                        continue;
                    }
                    let mut at = 0;
                    while at < n.length {
                        out.push(Note {
                            start: n.start + at as i64,
                            length: grid.min(n.length - at),
                            ..*n
                        });
                        at += grid;
                    }
                }
                *notes = out;
            }
            NoteEdit::Arpeggiate { grid } => {
                let grid = grid.max(1);
                let mut order: Vec<usize> = (0..notes.len()).collect();
                order.sort_by_key(|&i| (notes[i].start, notes[i].pitch));
                let mut k = 0;
                while k < order.len() {
                    let start = notes[order[k]].start;
                    let chord: Vec<usize> = order[k..]
                        .iter()
                        .copied()
                        .take_while(|&i| notes[i].start == start)
                        .collect();
                    for (step, &i) in chord.iter().enumerate().skip(1) {
                        let shift = step as u64 * grid;
                        let n = &mut notes[i];
                        n.start += shift as i64;
                        n.length = n.length.saturating_sub(shift).max(grid);
                    }
                    k += chord.len();
                }
            }
            NoteEdit::Transpose { semitones } => {
                for &i in &chosen {
                    let p = notes[i].pitch as i32 + semitones;
                    notes[i].pitch = p.clamp(0, 127) as u8;
                }
            }
            NoteEdit::Velocity { by } => {
                for &i in &chosen {
                    let v = notes[i].velocity as i32 + by;
                    notes[i].velocity = v.clamp(1, 127) as u8;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::PPQ;

    fn n(start: i64, length: u64, pitch: u8) -> Note {
        Note {
            start,
            length,
            pitch,
            velocity: 100,
        }
    }

    #[test]
    fn keys_read_and_write() {
        let am = Key {
            root: 9,
            minor: true,
        };
        for s in ["A minor", "Am", "a min", "A Minor"] {
            assert_eq!(Key::parse(s), Some(am), "{s}");
        }
        assert_eq!(Key::parse("Eb major").unwrap().name(), "Eb major");
        assert_eq!(Key::parse("F#").unwrap().name(), "F# major");
        assert_eq!(Key::parse("Bbm").unwrap().short(), "Bb min");
        assert_eq!(Key::parse("H"), None);
        assert!(am.contains(72) && am.contains(69) && !am.contains(73));
        assert_eq!(Key::all().count(), 24);
    }

    #[test]
    fn detects_the_key_of_some_notes() {
        // An A minor arpeggio and scale, leaning on A, C and E.
        let notes = [
            (69, 960),
            (72, 480),
            (76, 480),
            (71, 240),
            (74, 240),
            (77, 240),
            (79, 240),
            (69, 960),
            (64, 480),
        ];
        assert_eq!(detect(notes), Key::parse("A minor"));
        // A C major scale, starting and ending on C.
        let c = [60, 62, 64, 65, 67, 69, 71, 72, 60, 67, 64];
        assert_eq!(detect(c.map(|p| (p, 480))), Key::parse("C major"));
        assert_eq!(detect([]), None);
    }

    #[test]
    fn quantize_with_strength_and_swing() {
        let g = PPQ as i64 / 4;
        let mut notes = vec![n(g + 30, 100, 60), n(2 * g - 50, 100, 62)];
        NoteEdit::Quantize {
            grid: g as u64,
            strength: 1.0,
            swing: 50.0,
        }
        .apply(&mut notes, None);
        assert_eq!((notes[0].start, notes[1].start), (g, 2 * g));

        let mut notes = vec![n(g + 40, 100, 60)];
        NoteEdit::Quantize {
            grid: g as u64,
            strength: 0.5,
            swing: 50.0,
        }
        .apply(&mut notes, None);
        assert_eq!(notes[0].start, g + 20);

        // 66% swing: the odd step lands about a third of a step late.
        let mut notes = vec![n(g, 100, 60), n(2 * g, 100, 60)];
        NoteEdit::Quantize {
            grid: g as u64,
            strength: 1.0,
            swing: 66.0,
        }
        .apply(&mut notes, None);
        assert_eq!(notes[0].start, g + (g as f64 * 0.32).round() as i64);
        assert_eq!(notes[1].start, 2 * g);
    }

    #[test]
    fn legato_chop_and_arpeggiate() {
        let mut notes = vec![n(0, 100, 60), n(480, 100, 62), n(960, 100, 64)];
        NoteEdit::Legato.apply(&mut notes, None);
        assert_eq!(
            notes.iter().map(|n| n.length).collect::<Vec<_>>(),
            [480, 480, 100]
        );

        let mut notes = vec![n(0, 1000, 60)];
        NoteEdit::Chop { grid: 480 }.apply(&mut notes, None);
        assert_eq!(
            notes
                .iter()
                .map(|n| (n.start, n.length))
                .collect::<Vec<_>>(),
            [(0, 480), (480, 480), (960, 40)]
        );

        let mut notes = vec![n(0, 960, 64), n(0, 960, 60), n(0, 960, 67)];
        NoteEdit::Arpeggiate { grid: 240 }.apply(&mut notes, None);
        let mut by_pitch: Vec<(u8, i64)> = notes.iter().map(|n| (n.pitch, n.start)).collect();
        by_pitch.sort();
        assert_eq!(by_pitch, [(60, 0), (64, 240), (67, 480)]);
    }

    #[test]
    fn humanize_stays_close_and_repeats_for_a_seed() {
        let edit = NoteEdit::Humanize {
            timing: 20,
            velocity: 8,
            seed: 7,
        };
        let base: Vec<Note> = (0..32).map(|i| n(1000 + i * 480, 100, 60)).collect();
        let (mut a, mut b) = (base.clone(), base.clone());
        edit.apply(&mut a, None);
        edit.apply(&mut b, None);
        assert_eq!(a, b);
        assert_ne!(a, base);
        for (x, y) in a.iter().zip(&base) {
            assert!((x.start - y.start).abs() <= 20);
            assert!((x.velocity as i32 - 100).abs() <= 8);
        }
    }

    #[test]
    fn one_note_only() {
        let mut notes = vec![n(0, 100, 60), n(480, 100, 62)];
        NoteEdit::Transpose { semitones: 12 }.apply(&mut notes, Some(1));
        assert_eq!((notes[0].pitch, notes[1].pitch), (60, 74));
        NoteEdit::Velocity { by: 50 }.apply(&mut notes, None);
        assert_eq!(notes[0].velocity, 127);
    }
}
