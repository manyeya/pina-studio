//! Music itself: theory, writing parts (`compose`) and songs (`arrange`), the critic, genre
//! playbooks, how tracks look by what they play, and analysis of audio. Here: keys and scales,
//! and the edits the piano roll makes to a region's notes in one go (quantize, humanize, legato,
//! chop, arpeggiate, transpose and velocity).

pub mod analysis;
pub mod arrange;
pub mod compose;
pub mod critique;
pub mod look;
pub mod playbook;
pub mod theory;

use serde::{Deserialize, Serialize};

use crate::song::project::Note;

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

    /// The scale's seven steps, in semitones above the tonic.
    fn scale(&self) -> [u8; 7] {
        match self.minor {
            true => [0, 2, 3, 5, 7, 8, 10],
            false => [0, 2, 4, 5, 7, 9, 11],
        }
    }

    /// Where a pitch goes when the music moves from this key to `to`: the tonic moves the nearest
    /// way (up to a fifth up or a tritone down), and each step of the scale lands on the same step
    /// of the new one, so changing between major and minor changes the mode, not just the pitch.
    /// Notes outside the scale keep their place beside the step under them.
    pub fn move_pitch(&self, pitch: u8, to: &Key) -> u8 {
        let shift = (to.root as i32 - self.root as i32).rem_euclid(12);
        let shift = if shift >= 6 { shift - 12 } else { shift };
        let rel = (pitch as i32 - self.root as i32).rem_euclid(12);
        let (from, onto) = (self.scale(), to.scale());
        let degree = (0..7).rev().find(|&i| from[i] as i32 <= rel).unwrap();
        let moved = onto[degree] as i32 + rel - from[degree] as i32;
        (pitch as i32 - rel + shift + moved).clamp(0, 127) as u8
    }

    /// The key signature: sharps (positive) or flats (negative), as the tonic is spelled.
    pub fn signature(&self) -> i8 {
        // The relative major's place on the circle of fifths, from C.
        let major = (self.root + if self.minor { 3 } else { 0 }) % 12;
        let fifths = (major as i8 * 7).rem_euclid(12); // 0 C, 1 G, 2 D ... 11 F
        let sharps = if fifths > 6 { fifths - 12 } else { fifths };
        // Spelled with flats (as `tonic` does), a key with six sharps would be six flats.
        match (sharps, self.tonic().contains('b')) {
            (6, true) => -6,
            (s, _) => s,
        }
    }

    /// A pitch as this key's signature spells it: its letter (C is 0, B 6), octave (middle C's
    /// is 4), and how far it's raised or lowered from the letter (1 a sharp, -1 a flat).
    pub fn spell(&self, pitch: u8) -> (i32, i32, i8) {
        const NATURAL: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
        let sig = self.signature();
        let pc = pitch % 12;
        let octave = pitch as i32 / 12 - 1;
        // The letter the signature alters to make this pitch, if it does.
        let altered = |letter: usize| {
            const SHARPS: [usize; 7] = [3, 0, 4, 1, 5, 2, 6]; // F C G D A E B
            const FLATS: [usize; 7] = [6, 2, 5, 1, 4, 0, 3]; // B E A D G C F
            match sig {
                s if s > 0 => SHARPS[..s as usize].contains(&letter).then_some(1),
                s if s < 0 => FLATS[..(-s) as usize].contains(&letter).then_some(-1),
                _ => None,
            }
        };
        // In the key: spelled as its signature has it (E# and Cb included).
        for (letter, &natural) in NATURAL.iter().enumerate() {
            if let Some(alter) = altered(letter) {
                let sounding = (natural as i8 + alter).rem_euclid(12) as u8;
                if sounding == pc {
                    let octave = octave + (natural as i32 + alter as i32 - pc as i32) / -12;
                    return (letter as i32, octave, alter);
                }
            }
        }
        if let Some(letter) = NATURAL.iter().position(|&n| n == pc) {
            return (letter as i32, octave, 0);
        }
        // A black key outside the signature: flats in flat keys, else sharps.
        let (letter, alter) = match sig < 0 {
            true => (NATURAL.iter().position(|&n| n == pc + 1).unwrap(), -1),
            false => (NATURAL.iter().position(|&n| n + 1 == pc).unwrap(), 1),
        };
        (letter as i32, octave, alter)
    }

    /// What the signature already does to a letter: 1 sharpens it, -1 flattens it.
    pub fn signature_on(&self, letter: i32) -> i8 {
        const SHARPS: [i32; 7] = [3, 0, 4, 1, 5, 2, 6];
        const FLATS: [i32; 7] = [6, 2, 5, 1, 4, 0, 3];
        match self.signature() {
            s if s > 0 && SHARPS[..s as usize].contains(&letter) => 1,
            s if s < 0 && FLATS[..(-s) as usize].contains(&letter) => -1,
            _ => 0,
        }
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
    /// Chords spread as a guitar strums them: each note `ticks` after the one below.
    Strum {
        ticks: u64,
    },
    /// Played backwards: each note ends where its mirror started.
    Reverse,
    /// Upside down: each pitch mirrored about the middle of the lowest and highest.
    Flip,
    /// Notes of a pitch that touch or overlap made one.
    Glue,
    /// Velocities scaled about their middle by `percent` (100 as they are, 0 all the same), then
    /// moved by `by`.
    Dynamics {
        percent: u32,
        #[serde(default)]
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
    /// Applies the edit to a region's notes: those in `only`, or with it empty, all of them.
    pub fn apply(&self, notes: &mut Vec<Note>, only: &[usize]) {
        let mut chosen: Vec<usize> = match only.is_empty() {
            true => (0..notes.len()).collect(),
            false => only.iter().copied().filter(|&i| i < notes.len()).collect(),
        };
        chosen.sort_unstable();
        chosen.dedup();
        if chosen.is_empty() {
            return;
        }
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
                // Each chosen note held to the next note to start, chosen or not.
                let mut starts: Vec<i64> = notes.iter().map(|n| n.start).collect();
                starts.sort_unstable();
                starts.dedup();
                for &i in &chosen {
                    let n = &mut notes[i];
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
            NoteEdit::Arpeggiate { grid } | NoteEdit::Strum { ticks: grid } => {
                let strum = matches!(self, NoteEdit::Strum { .. });
                let grid = grid.max(1);
                let mut order = chosen.clone();
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
                        // A strum lets each string ring to the chord's end; an arpeggio's notes
                        // keep a step at least.
                        n.length = match strum {
                            true => n.length.saturating_sub(shift).max(1),
                            false => n.length.saturating_sub(shift).max(grid),
                        };
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
            NoteEdit::Reverse => {
                let from = chosen.iter().map(|&i| notes[i].start).min().unwrap_or(0);
                let to = chosen
                    .iter()
                    .map(|&i| notes[i].start + notes[i].length as i64)
                    .max()
                    .unwrap_or(0);
                for &i in &chosen {
                    let n = &mut notes[i];
                    n.start = from + to - (n.start + n.length as i64);
                }
            }
            NoteEdit::Flip => {
                let lo = chosen
                    .iter()
                    .map(|&i| notes[i].pitch as i32)
                    .min()
                    .unwrap_or(0);
                let hi = chosen
                    .iter()
                    .map(|&i| notes[i].pitch as i32)
                    .max()
                    .unwrap_or(0);
                for &i in &chosen {
                    notes[i].pitch = (lo + hi - notes[i].pitch as i32).clamp(0, 127) as u8;
                }
            }
            NoteEdit::Glue => {
                let mut order = chosen.clone();
                order.sort_by_key(|&i| (notes[i].pitch, notes[i].start));
                let mut gone = vec![false; notes.len()];
                let mut k = 0;
                while k < order.len() {
                    let first = order[k];
                    let mut end = notes[first].start + notes[first].length as i64;
                    let mut j = k + 1;
                    while j < order.len()
                        && notes[order[j]].pitch == notes[first].pitch
                        && notes[order[j]].start <= end
                    {
                        let n = notes[order[j]];
                        end = end.max(n.start + n.length as i64);
                        gone[order[j]] = true;
                        j += 1;
                    }
                    notes[first].length = (end - notes[first].start) as u64;
                    k = j;
                }
                let mut i = 0;
                notes.retain(|_| {
                    i += 1;
                    !gone[i - 1]
                });
            }
            NoteEdit::Dynamics { percent, by } => {
                let vs: Vec<f32> = chosen.iter().map(|&i| notes[i].velocity as f32).collect();
                let mid = (vs.iter().copied().fold(f32::MAX, f32::min)
                    + vs.iter().copied().fold(f32::MIN, f32::max))
                    / 2.0;
                for &i in &chosen {
                    let v =
                        mid + (notes[i].velocity as f32 - mid) * percent as f32 / 100.0 + by as f32;
                    notes[i].velocity = v.round().clamp(1.0, 127.0) as u8;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
