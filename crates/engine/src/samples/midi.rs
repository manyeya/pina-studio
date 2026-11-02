//! MIDI files in the library, studied as a musician reads them: the whole file's key (from its
//! notes, checked against what its name says), tempo and meter, its chords bar by bar with
//! numerals and the progression they go round, how long it loops for; and each part's role
//! (drums, bass, chords, pad, pluck, keys, lead, arp, fx, from its name's code and how it plays),
//! range, how many notes sound at once, density, where its notes start on a bar's sixteen steps,
//! swing, syncopation, note lengths and legato, dynamics, its melody's shape, and, for drums,
//! each sound's pattern. Found by role, key, tempo and words; brought into a song, played and
//! read by agents for ideas.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    music::{Key, theory::Chord},
    song::{
        midi_file::{self, GM, MidiFile, Part},
        project::{Note, PPQ},
    },
    study::harmony,
};

mod group;
mod kit;
mod places;
pub use group::*;
pub use kit::*;
pub use places::*;

/// Bumped when what a study holds changes, so the library's are made again.
pub const MIDI_VERSION: u32 = 1;

/// A MIDI file in the library: where it is, its name and folder, when it last changed, and what
/// it is.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MidiSample {
    pub path: PathBuf,
    pub name: String,
    pub folder: String,
    pub modified: u64,
    pub size: u64,
    pub study: MidiStudy,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct MidiStudy {
    pub bars: u64,
    pub time_signature: [u64; 2],
    /// Its tempo: the file's, else its name's ("124 BPM"), else unknown.
    pub tempo: Option<f32>,
    pub key: Option<String>,
    /// How well its notes fit the key, -1 to 1; and how it was found: "file", "name", "notes",
    /// "name and notes".
    pub key_fit: f32,
    pub key_from: String,
    /// The part it's mostly for (its first, or its only).
    pub role: String,
    pub parts: Vec<PartStudy>,
    /// Each bar's chord, as names in the key and numerals ("-" where none sounds), and the
    /// progression they go round.
    pub chords: Vec<String>,
    pub numerals: Vec<String>,
    pub progression: String,
    /// How many bars it goes round in (1, 2, 4, 8), or its whole length if it never repeats.
    pub loop_bars: u64,
    pub traits: Vec<String>,
    pub summary: String,
}

impl MidiStudy {
    /// Whether all it plays is drums, with nothing to move into a key. A song's file can start
    /// with its drums and have everything else after them.
    pub fn all_drums(&self) -> bool {
        self.role == "drums" && self.parts.iter().all(|p| p.role == "drums")
    }
}

/// A part, studied.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct PartStudy {
    pub name: String,
    pub role: String,
    pub channel: u8,
    /// General MIDI's instrument for its program, "Drums", or none.
    pub instrument: Option<String>,
    pub notes: usize,
    pub low: String,
    pub high: String,
    /// Notes sounding at once while any does, on average, and at most.
    pub polyphony: f32,
    pub max_polyphony: usize,
    /// Notes a bar.
    pub density: f32,
    /// How often each of a bar's sixteen steps starts a note, the most 1.
    pub steps: Vec<f32>,
    /// 50 straight, up to 75 (every second sixteenth late by half a step).
    pub swing: f32,
    /// The share of its notes starting off the eighths.
    pub syncopation: f32,
    /// Its velocities: softest, hardest, average.
    pub velocity: [u8; 3],
    /// Its notes' average length in sixteenths, and the share held into the next.
    pub length: f32,
    pub legato: f32,
    /// A melody's shape (one note at a time): steps (two semitones or less) and leaps as shares
    /// of its moves, its span in semitones, and which way it goes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contour: Option<Contour>,
    /// Drums: each sound's sixteen steps, how hard (0-9) as digits a bar.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drums: Vec<(String, String)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Contour {
    pub steps: f32,
    pub leaps: f32,
    pub span: u8,
    pub direction: String,
}

/// Every MIDI file under a folder, hidden ones skipped.
pub fn midi_files(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for path in entries.flatten().map(|e| e.path()) {
            let hidden = path
                .file_name()
                .is_none_or(|n| n.to_string_lossy().starts_with('.'));
            if hidden {
                continue;
            }
            if path.is_dir() {
                walk(&path, out);
            } else if midi_file::is_midi(&path) {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort();
    out
}

/// What a file's name says of its key ("PML BS Kila - G#m", "Bass_Am_124") and tempo ("124 BPM",
/// "_124_").
pub fn from_name(name: &str) -> (Option<Key>, Option<f32>) {
    let words: Vec<&str> = name
        .split([' ', '_', '-', '(', ')', '.'])
        .filter(|w| !w.is_empty())
        .collect();
    let key = words.iter().enumerate().rev().find_map(|(i, w)| {
        // "G#m", "Am", "F#", or "F# minor" across two words.
        let joined = match words.get(i + 1) {
            Some(n) if matches!(n.to_lowercase().as_str(), "minor" | "major" | "min" | "maj") => {
                format!("{w} {n}")
            }
            _ => w.to_string(),
        };
        let first = joined.chars().next()?;
        (first.is_ascii_uppercase() && joined.len() <= 9)
            .then(|| Key::parse(&joined))
            .flatten()
    });
    let tempo = words.iter().enumerate().find_map(|(i, w)| {
        let lower = w.to_lowercase();
        let digits = lower.trim_end_matches("bpm");
        let v: f32 = digits.parse().ok()?;
        let said = lower.ends_with("bpm")
            || words
                .get(i + 1)
                .is_some_and(|n| n.eq_ignore_ascii_case("bpm"));
        ((60.0..=200.0).contains(&v) && (said || digits.len() == 3)).then_some(v)
    });
    (key, tempo)
}

/// A part's role from its name's code, as sample packs name them ("PML BS …", "Lead_…").
fn role_from_name(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let has = |list: &[&str]| words.iter().any(|w| list.contains(w));
    [
        (
            "drums",
            &[
                "drm", "drum", "drums", "perc", "kick", "hat", "hats", "snare", "clap", "tom",
                "toms", "shaker", "beat", "groove",
            ][..],
        ),
        ("bass", &["bs", "bass", "sub", "808", "log"][..]),
        ("pad", &["pd", "pad", "pads", "atmos", "strings", "str"][..]),
        ("pluck", &["pl", "pluck", "plucks"][..]),
        (
            "lead",
            &["ld", "lead", "leads", "melody", "mel", "hook"][..],
        ),
        (
            "keys",
            &["ky", "keys", "key", "piano", "rhodes", "organ", "ep"][..],
        ),
        ("arp", &["arp", "arps", "arpeggio", "seq"][..]),
        ("chords", &["ch", "chord", "chords", "stab", "stabs"][..]),
        ("fx", &["fx", "sfx", "noise", "riser", "sweep"][..]),
        ("vocals", &["vox", "vocal", "vocals", "voice"][..]),
    ]
    .into_iter()
    .find(|(_, list)| has(list))
    .map(|(r, _)| r)
}

/// General MIDI's drum sounds, by note, as a drummer names them.
fn drum_name(pitch: u8) -> &'static str {
    match pitch {
        35 | 36 => "kick",
        37 => "rim",
        38 | 40 => "snare",
        39 => "clap",
        42 | 44 => "closed hat",
        46 => "open hat",
        41 | 43 | 45 | 47 | 48 | 50 => "tom",
        49 | 52 | 55 | 57 => "crash",
        51 | 53 | 59 => "ride",
        54 => "tambourine",
        56 => "cowbell",
        69 | 70 => "shaker",
        _ => "perc",
    }
}

/// A part, studied on bars of `bar` ticks.
fn study_part(p: &Part, bar: u64, bars: u64, file_name: &str) -> PartStudy {
    let notes = &p.notes;
    let step = (bar / 16).max(1);
    // How many sound at once: at each start, those still sounding.
    let at_once: Vec<usize> = notes
        .iter()
        .map(|n| {
            notes
                .iter()
                .filter(|m| m.start <= n.start && m.start + m.length as i64 > n.start)
                .count()
        })
        .collect();
    let polyphony = at_once.iter().sum::<usize>() as f32 / at_once.len().max(1) as f32;
    let max_polyphony = at_once.iter().copied().max().unwrap_or(0);
    let mut steps = [0f32; 16];
    let (mut off_eighth, mut late, mut odd) = (0usize, 0f32, 0usize);
    let mut starts: Vec<i64> = notes.iter().map(|n| n.start).collect();
    starts.dedup();
    for &s in &starts {
        let in_bar = s.rem_euclid(bar as i64) as u64;
        let k = ((in_bar as f32 / step as f32).round() as usize) % 16;
        steps[k] += 1.0;
        if k % 2 == 1 {
            off_eighth += 1;
            odd += 1;
            // How late it is against the straight step, as a share of a step.
            late += (in_bar as f32 - k as f32 * step as f32) / step as f32;
        }
    }
    let top = steps.iter().copied().fold(0f32, f32::max).max(1.0);
    let swing = match odd {
        n if n >= 4 => (50.0 + 50.0 * (late / n as f32).clamp(0.0, 0.5)).round(),
        _ => 50.0,
    };
    let vel: Vec<u8> = notes.iter().map(|n| n.velocity).collect();
    let mean_vel = vel.iter().map(|&v| v as f32).sum::<f32>() / vel.len().max(1) as f32;
    let length = notes
        .iter()
        .map(|n| n.length as f32 / step as f32)
        .sum::<f32>()
        / notes.len().max(1) as f32;
    // Held into the next note's start (one note at a time).
    let tops = melody(notes);
    let legato = match tops.len() {
        0 | 1 => 0.0,
        n => {
            tops.windows(2)
                .filter(|w| w[0].start + w[0].length as i64 >= w[1].start)
                .count() as f32
                / (n - 1) as f32
        }
    };
    let drums =
        p.is_drums() || role_from_name(file_name) == Some("drums") && looks_like_drums(notes);
    let role = match (drums, role_from_name(&p.name).or(role_from_name(file_name))) {
        (true, _) => "drums",
        (false, Some(r)) if r != "drums" => r,
        _ => played_role(notes, polyphony, length, bar),
    };
    let contour = (!drums && polyphony < 1.6 && tops.len() >= 4).then(|| {
        let moves: Vec<i32> = tops
            .windows(2)
            .map(|w| w[1].pitch as i32 - w[0].pitch as i32)
            .filter(|m| *m != 0)
            .collect();
        let n = moves.len().max(1) as f32;
        let (lo, hi) = tops
            .iter()
            .fold((127u8, 0u8), |a, n| (a.0.min(n.pitch), a.1.max(n.pitch)));
        let half = tops.len() / 2;
        let mean =
            |s: &[Note]| s.iter().map(|n| n.pitch as f32).sum::<f32>() / s.len().max(1) as f32;
        let (first, second) = (mean(&tops[..half]), mean(&tops[half..]));
        let peak = tops.iter().position(|n| n.pitch == hi).unwrap_or(0) as f32 / tops.len() as f32;
        Contour {
            steps: moves.iter().filter(|m| m.abs() <= 2).count() as f32 / n,
            leaps: moves.iter().filter(|m| m.abs() > 2).count() as f32 / n,
            span: hi.saturating_sub(lo),
            direction: match (second - first, peak) {
                (_, p) if (0.3..0.7).contains(&p) && hi as f32 > first.max(second) + 3.0 => {
                    "arches up and back"
                }
                (d, _) if d > 2.0 => "climbs",
                (d, _) if d < -2.0 => "falls",
                _ => "stays round one place",
            }
            .to_string(),
        }
    });
    let drum_rows = match drums {
        true => {
            let mut rows: Vec<(String, [f32; 16])> = Vec::new();
            for n in notes {
                let name = drum_name(n.pitch);
                let k =
                    ((n.start.rem_euclid(bar as i64) as f32 / step as f32).round() as usize) % 16;
                let row = match rows.iter().position(|r| r.0 == name) {
                    Some(i) => i,
                    None => {
                        rows.push((name.to_string(), [0.0; 16]));
                        rows.len() - 1
                    }
                };
                rows[row].1[k] = rows[row].1[k].max(n.velocity as f32 / 127.0);
            }
            rows.into_iter().map(|(n, r)| (n, digits(&r))).collect()
        }
        false => Vec::new(),
    };
    let (lo, hi) = notes
        .iter()
        .fold((127u8, 0u8), |a, n| (a.0.min(n.pitch), a.1.max(n.pitch)));
    PartStudy {
        name: p.name.clone(),
        role: role.into(),
        channel: p.channel,
        instrument: match (p.is_drums(), p.program) {
            (true, _) => Some("Drums".into()),
            (false, Some(prog)) => Some(GM[prog as usize].into()),
            _ => None,
        },
        notes: notes.len(),
        low: crate::samples::note_name(lo),
        high: crate::samples::note_name(hi),
        polyphony: (polyphony * 10.0).round() / 10.0,
        max_polyphony,
        density: (notes.len() as f32 / bars.max(1) as f32 * 10.0).round() / 10.0,
        steps: steps
            .iter()
            .map(|v| (v / top * 100.0).round() / 100.0)
            .collect(),
        swing,
        syncopation: (off_eighth as f32 / starts.len().max(1) as f32 * 100.0).round() / 100.0,
        velocity: [
            vel.iter().copied().min().unwrap_or(0),
            vel.iter().copied().max().unwrap_or(0),
            mean_vel.round() as u8,
        ],
        length: (length * 10.0).round() / 10.0,
        legato: (legato * 100.0).round() / 100.0,
        contour,
        drums: drum_rows,
    }
}

/// Notes in the drum map's range, mostly short and on few pitches: a beat written on a channel
/// other than the tenth.
fn looks_like_drums(notes: &[Note]) -> bool {
    let in_map = notes
        .iter()
        .filter(|n| (35..=81).contains(&n.pitch))
        .count();
    in_map * 10 >= notes.len() * 9
}

/// A part's role from how it plays: low and one at a time a bass; several at once chords (held
/// long, a pad); quick even notes a pattern repeats an arp; else a lead.
fn played_role(notes: &[Note], polyphony: f32, length: f32, bar: u64) -> &'static str {
    let mut pitches: Vec<u8> = notes.iter().map(|n| n.pitch).collect();
    pitches.sort();
    let median = pitches.get(pitches.len() / 2).copied().unwrap_or(60);
    if polyphony >= 2.2 {
        return if length >= 12.0 { "pad" } else { "chords" };
    }
    if median < 52 {
        return "bass";
    }
    let quick = length <= 2.0
        && notes.len() as f32
            >= 8.0 * (notes.last().map_or(0, |n| n.start) as f32 / bar as f32).max(1.0);
    if quick { "arp" } else { "lead" }
}

/// One note at a time: the highest sounding at each start.
fn melody(notes: &[Note]) -> Vec<Note> {
    let mut out: Vec<Note> = Vec::new();
    for n in notes {
        match out.last_mut() {
            Some(last) if last.start == n.start => {
                if n.pitch > last.pitch {
                    *last = *n;
                }
            }
            _ => out.push(*n),
        }
    }
    out
}

/// Sixteen steps as digits 0-9, the strongest 9, in fours: "9002 0090 9020 0090".
fn digits(r: &[f32; 16]) -> String {
    let top = r.iter().copied().fold(0f32, f32::max).max(1e-6);
    r.chunks(4)
        .map(|c| {
            c.iter()
                .map(|v| char::from(b'0' + (v / top * 9.0).round().clamp(0.0, 9.0) as u8))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A MIDI file studied: its key (from its notes, checked against its name's), tempo, meter,
/// chords and progression, how long it loops, each part's role and how it plays, and what it
/// is in a sentence.
pub fn study(file: &MidiFile, name: &str) -> MidiStudy {
    let time_signature = file.time_signature.unwrap_or([4, 4]);
    let bar = (PPQ * 4 * time_signature[0] / time_signature[1].max(1)).max(1);
    let bars = file.end().div_ceil(bar).max(1);
    let (named_key, named_tempo) = from_name(name);
    let parts: Vec<PartStudy> = file
        .parts
        .iter()
        .map(|p| study_part(p, bar, bars, name))
        .collect();
    let pitched: Vec<&Part> = file
        .parts
        .iter()
        .zip(&parts)
        .filter(|(_, s)| s.role != "drums")
        .map(|(p, _)| p)
        .collect();
    // The notes' pitch classes, by how long each sounds; and the lowest's.
    let mut chroma = [0f32; 12];
    let mut lows = [0f32; 12];
    for p in &pitched {
        for n in &p.notes {
            chroma[n.pitch as usize % 12] += n.length as f32;
        }
        for n in melody_low(&p.notes) {
            lows[n.pitch as usize % 12] += n.length as f32;
        }
    }
    let scored = match chroma.iter().sum::<f32>() > 0.0 {
        true => harmony::keys(&chroma, &lows),
        false => Vec::new(),
    };
    let fit_of = |k: &Key| scored.iter().find(|s| s.0 == *k).map_or(-1.0, |s| s.2);
    // Relatives share their notes: where both fit about as well, the one whose home note the
    // music starts on (its first bar's lowest note) is the key.
    let opening = pitched
        .iter()
        .flat_map(|p| p.notes.iter())
        .filter(|n| n.start < bar as i64)
        .min_by_key(|n| n.pitch)
        .map(|n| n.pitch % 12);
    let best = scored.first().map(|s| {
        let relative = Key {
            root: if s.0.minor {
                (s.0.root + 3) % 12
            } else {
                (s.0.root + 9) % 12
            },
            minor: !s.0.minor,
        };
        let score_of = |k: &Key| scored.iter().find(|x| x.0 == *k).map_or(-1.0, |x| x.1);
        match opening {
            Some(r) if r == relative.root && score_of(&relative) >= s.1 - 0.1 => {
                (relative, fit_of(&relative))
            }
            _ => (s.0, s.2),
        }
    });
    // A pack labels its files' keys: the name's stands unless the notes plainly leave it (a bass
    // line or a pad's voicing leans on other degrees, so the notes' best guess is often a
    // relative or a fifth away).
    // Its name over its key signature, which is often the C major software writes by default.
    let (key, key_fit, key_from) = match (file.key, named_key, best) {
        (_, Some(k), Some(_)) if fit_of(&k) > 0.0 => (Some(k), fit_of(&k), "name and notes"),
        (Some(k), _, _) => (Some(k), fit_of(&k), "file"),
        (None, Some(k), None) => (Some(k), 0.0, "name"),
        (None, _, Some((k, f))) if f > 0.3 => (Some(k), f, "notes"),
        (None, Some(k), _) => (Some(k), fit_of(&k), "name"),
        _ => (None, 0.0, ""),
    };
    // Each bar's chord, from every pitched note sounding in it, the lowest its bass. One note at
    // a time names no chord: there the bar's main note (the one held longest) is a root, its
    // chord the key's own on it.
    let chordal = parts
        .iter()
        .any(|p| p.role != "drums" && p.max_polyphony >= 3);
    let mut chords = Vec::new();
    let mut numerals = Vec::new();
    for b in 0..bars {
        let (a, z) = ((b * bar) as i64, ((b + 1) * bar) as i64);
        let mut pcs = [0f32; 12];
        let mut lowest: Option<u8> = None;
        for p in &pitched {
            for n in &p.notes {
                let held = (n.start + n.length as i64).min(z) - n.start.max(a);
                if held > 0 {
                    pcs[n.pitch as usize % 12] += held as f32;
                    lowest = Some(lowest.map_or(n.pitch, |l| l.min(n.pitch)));
                }
            }
        }
        let weights: Vec<(u8, f32)> = pcs
            .iter()
            .enumerate()
            .filter(|(_, w)| **w > 0.0)
            .map(|(k, w)| (k as u8, *w))
            .collect();
        let named = match (chordal, key) {
            (true, _) | (false, None) => {
                crate::music::theory::name_notes(&weights, lowest.map(|l| l % 12))
            }
            (false, Some(k)) => weights
                .iter()
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|&(root, _)| diatonic(&k, root)),
        };
        match named {
            Some(c) => {
                chords.push(c.name_in(key.as_ref()));
                numerals.push(key.map_or(String::new(), |k| harmony::numeral(&c, &k)));
            }
            None => {
                chords.push("-".into());
                numerals.push("-".into());
            }
        }
    }
    let looped = harmony::loop_of(&chords);
    let progression = match key {
        Some(k) => looped
            .iter()
            .map(|c| Chord::parse(c).map_or(c.clone(), |c| harmony::numeral(&c, &k)))
            .collect::<Vec<_>>()
            .join(" | "),
        None => looped.join(" | "),
    };
    let loop_bars = loop_length(file, bar, bars);
    let role = parts.first().map_or("", |p| p.role.as_str()).to_string();
    let main = parts.first();
    let mut traits = Vec::new();
    if let Some(m) = main {
        if m.syncopation >= 0.35 {
            traits.push("syncopated");
        }
        if m.swing >= 55.0 {
            traits.push("swung");
        }
        if m.role != "drums" {
            traits.push(if m.legato >= 0.5 {
                "legato"
            } else if m.length <= 1.5 {
                "staccato"
            } else {
                "detached"
            });
            traits.push(if m.max_polyphony >= 3 {
                "chordal"
            } else {
                "one note at a time"
            });
        }
        traits.push(match m.density / time_signature[0] as f32 {
            d if d >= 3.0 => "busy",
            d if d <= 0.75 => "sparse",
            _ => "moderate",
        });
    }
    let tempo = file.tempo.map(|t| t as f32).or(named_tempo);
    // "An 8-bar", "an 11-bar", else "a".
    let a = |n: u64| match n.to_string().starts_with('8') || n == 11 || n == 18 {
        true => "an",
        false => "a",
    };
    let summary = format!(
        "{}{} {bars}-bar {role}{}{}{}{}",
        a(bars)[..1].to_uppercase(),
        &a(bars)[1..],
        key.map_or(String::new(), |k| format!(" in {}", k.name())),
        tempo.map_or(String::new(), |t| format!(", {t:.0} BPM")),
        match loop_bars < bars {
            true => format!(", {} {loop_bars}-bar loop", a(loop_bars)),
            false => String::new(),
        },
        match (progression.is_empty(), traits.is_empty()) {
            (false, _) if role != "drums" => format!(": {progression}; {}", traits.join(", ")),
            _ => format!(": {}", traits.join(", ")),
        },
    );
    MidiStudy {
        bars,
        time_signature,
        tempo,
        key: key.map(|k| k.name()),
        key_fit: (key_fit * 100.0).round() / 100.0,
        key_from: key_from.into(),
        role,
        parts,
        chords,
        numerals,
        progression,
        loop_bars,
        traits: traits.into_iter().map(str::to_string).collect(),
        summary,
    }
}

/// The key's own chord on a root: a third and fifth from its scale (a triad: major, minor or
/// diminished; a root off the scale, major).
fn diatonic(k: &Key, root: u8) -> Chord {
    use crate::music::theory::Quality;
    let has = |i: u8| k.contains((root + i) % 12);
    let quality = match (has(3), has(4), has(6), has(7)) {
        (true, _, true, false) => Quality::Dim,
        (true, false, _, _) => Quality::Min,
        _ => Quality::Maj,
    };
    Chord {
        root,
        quality,
        bass: None,
    }
}

/// The lowest note sounding at each start.
fn melody_low(notes: &[Note]) -> Vec<Note> {
    let mut out: Vec<Note> = Vec::new();
    for n in notes {
        match out.last_mut() {
            Some(last) if last.start == n.start => {
                if n.pitch < last.pitch {
                    *last = *n;
                }
            }
            _ => out.push(*n),
        }
    }
    out
}

/// How many bars it goes round in: the shortest of 1, 2, 4 and 8 whose every repeat starts the
/// same notes at the same places (nine in ten alike), else its length.
fn loop_length(file: &MidiFile, bar: u64, bars: u64) -> u64 {
    let events: Vec<(u64, u8)> = file
        .parts
        .iter()
        .flat_map(|p| p.notes.iter().map(|n| (n.start.max(0) as u64, n.pitch)))
        .collect();
    for len in [1u64, 2, 4, 8] {
        if len >= bars || !bars.is_multiple_of(len) {
            continue;
        }
        let span = len * bar;
        let first: std::collections::HashSet<(u64, u8)> =
            events.iter().filter(|e| e.0 < span).copied().collect();
        if first.is_empty() {
            continue;
        }
        let alike = events
            .iter()
            .filter(|e| first.contains(&(e.0 % span, e.1)))
            .count();
        if alike * 10 >= events.len() * 9 {
            return len;
        }
    }
    bars
}

/// A MIDI file read and studied, for the library.
pub fn study_file(
    path: &Path,
    root: &Path,
    modified: u64,
    size: u64,
) -> Result<MidiSample, String> {
    let file = midi_file::open(path)?;
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(MidiSample {
        study: study(&file, &name),
        path: path.to_path_buf(),
        folder: path
            .parent()
            .and_then(|p| p.strip_prefix(root).ok())
            .map(|p| {
                p.components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .unwrap_or_default(),
        name,
        modified,
        size,
    })
}

/// What to look for among the library's MIDI files: words (in names and folders), a role, a key
/// (others are found too, as they'd be moved into it), a tempo, and how many.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct MidiQuery {
    #[serde(default)]
    pub words: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub tempo: Option<f32>,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// A MIDI file found: which, how well it fits, and why; and the semitones that move it into the
/// key asked for.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MidiHit {
    pub index: usize,
    pub score: f32,
    pub why: Vec<String>,
    pub transpose: i32,
}

/// The semitones (-6 to 5) that move a key's tonic onto another's, minor to minor and major to
/// major (a relative's counts as the same notes).
pub fn transpose_to(from: &Key, to: &Key) -> i32 {
    let target = match (from.minor, to.minor) {
        (true, false) => (to.root + 9) % 12,
        (false, true) => (to.root + 3) % 12,
        _ => to.root,
    };
    let d = (target as i32 - from.root as i32).rem_euclid(12);
    if d > 5 { d - 12 } else { d }
}

/// How far a tempo is from another, as a share of it, at its own, half or double.
fn tempo_off(own: f32, tempo: f32) -> f32 {
    let off = |o: f32| (o - tempo).abs() / tempo.max(1.0);
    off(own).min(off(own * 2.0)).min(off(own / 2.0))
}

/// How far off a tempo can be and still feel right, as for loops.
const NEAR_TEMPO: f32 = 0.12;

/// Whether a file fits a song in `key` at `tempo`. Drums fit any key. Otherwise it's in the key's
/// notes as it is (in it, or its relative); or, if it'll be `moved` into the key, it's at least
/// minor for a minor song: moved, it keeps its mood. A file whose key can't be told doesn't fit
/// one, but one whose tempo can't be told fits any: it plays at the song's anyway.
pub fn fits(m: &MidiSample, key: Option<Key>, tempo: Option<f32>, moved: bool) -> bool {
    let own = m.study.key.as_deref().and_then(Key::parse);
    let in_key = m.study.all_drums()
        || key.is_none_or(|k| {
            own.is_some_and(|o| transpose_to(&o, &k) == 0 || (moved && o.minor == k.minor))
        });
    let near = |t: f32| {
        m.study
            .tempo
            .is_none_or(|own| tempo_off(own, t) <= NEAR_TEMPO)
    };
    in_key && tempo.is_none_or(near)
}

/// The library's MIDI files that fit a query, best first.
pub fn find(midi: &[MidiSample], q: &MidiQuery) -> Vec<MidiHit> {
    let words: Vec<String> = q
        .words
        .to_lowercase()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let want_key = q.key.as_deref().and_then(Key::parse);
    let mut hits: Vec<MidiHit> = midi
        .iter()
        .enumerate()
        .filter_map(|(index, m)| {
            let mut score = 0f32;
            let mut why = Vec::new();
            let hay = format!("{} {} {}", m.name, m.folder, m.study.summary).to_lowercase();
            for w in &words {
                if hay.contains(w.as_str()) {
                    score += 1.0;
                    why.push(format!("says {w}"));
                } else if role_from_name(w).is_some_and(|r| r == m.study.role) {
                    score += 1.5;
                    why.push(format!("a {}", m.study.role));
                }
            }
            if let Some(r) = &q.role {
                match m.study.parts.iter().any(|p| &p.role == r) {
                    true => {
                        score += 3.0;
                        why.push(format!("a {r}"));
                    }
                    false => return None,
                }
            }
            let mut transpose = 0;
            if let (Some(k), Some(have)) = (want_key, m.study.key.as_deref().and_then(Key::parse)) {
                transpose = transpose_to(&have, &k);
                let same_kind = have.minor == k.minor
                    || (have.minor != k.minor && transpose_to(&have, &k) == 0);
                score += match (transpose, same_kind) {
                    (0, true) => {
                        why.push(format!("in {}", k.name()));
                        2.0
                    }
                    (t, _) => {
                        why.push(format!("in {} (moved {t:+} semitones)", have.name()));
                        1.0 - t.abs() as f32 * 0.05
                    }
                };
            }
            if let (Some(t), Some(have)) = (q.tempo, m.study.tempo) {
                let near = (have - t)
                    .abs()
                    .min((have * 2.0 - t).abs())
                    .min((have / 2.0 - t).abs());
                score += (1.0 - near / 20.0).max(0.0);
                if near < 3.0 {
                    why.push(format!("{have:.0} BPM"));
                }
            }
            // A key or tempo ranks: a file with neither to go on is last, not left out.
            (words.is_empty() && q.role.is_none() || score > 0.0).then_some(MidiHit {
                index,
                score,
                why,
                transpose,
            })
        })
        .collect();
    hits.sort_by(|a, b| b.score.total_cmp(&a.score));
    hits.truncate(q.limit.unwrap_or(24));
    hits
}

/// Puts the parts the study plays as drums on the drum channel, so a pack's drum file on
/// channel 1 comes in on drums and is never moved into a key.
pub fn mark_drums(file: &mut MidiFile, study: &MidiStudy) {
    for (part, s) in file.parts.iter_mut().zip(&study.parts) {
        if s.role == "drums" {
            part.channel = midi_file::DRUMS;
        }
    }
}

/// A part's notes as text, to read: "1.1 G#4 2 96", its bar and step (from 1), name, sixteenths
/// long and velocity, one a line.
pub fn notes_text(part: &Part, bar: u64) -> String {
    let step = (bar / 16).max(1);
    part.notes
        .iter()
        .map(|n| {
            let s = n.start.max(0) as u64;
            format!(
                "{}.{} {} {} {}",
                s / bar + 1,
                (s % bar) / step + 1,
                crate::samples::note_name(n.pitch),
                (n.length as f32 / step as f32).round().max(1.0),
                n.velocity
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Notes as `notes_text` writes them, a line each ("1.3 G#4 2 96": bar.step from 1 in sixteenths,
/// its name or number, how many sixteenths, velocity). A step can be fractional ("1.3.5") and the
/// velocity left out; blank lines and anything after a # at a line's end are passed over.
pub fn parse_notes(text: &str, bar: u64) -> Result<Vec<Note>, String> {
    let step = (bar / 16).max(1) as f64;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let bad = |what: &str| format!("line {}: {what} in {line:?}", i + 1);
        let mut words = line.split_whitespace();
        let at = words.next().ok_or_else(|| bad("no place"))?;
        let (b, s) = at.split_once('.').ok_or_else(|| bad("no bar.step"))?;
        let b: u64 = b.parse().map_err(|_| bad("no bar"))?;
        let s: f64 = s.parse().map_err(|_| bad("no step"))?;
        if b == 0 || s < 1.0 {
            return Err(bad("bars and steps count from 1"));
        }
        let name = words.next().ok_or_else(|| bad("no note"))?;
        let pitch = pitch_of(name).ok_or_else(|| bad("no such note"))?;
        let length: f64 = match words.next() {
            Some(w) => w.parse().map_err(|_| bad("no length"))?,
            None => 1.0,
        };
        let velocity: u8 = match words.next() {
            Some(w) if !w.starts_with('#') => w.parse().map_err(|_| bad("no velocity"))?,
            _ => 100,
        };
        out.push(Note {
            start: ((b - 1) as f64 * bar as f64 + (s - 1.0) * step).round() as i64,
            length: (length * step).round().max(1.0) as u64,
            pitch,
            velocity: velocity.clamp(1, 127),
        });
    }
    out.sort_by_key(|n| (n.start, n.pitch));
    Ok(out)
}

/// A note by its name ("C#4", "Db4", "c4") or number.
fn pitch_of(name: &str) -> Option<u8> {
    if let Ok(n) = name.parse::<u8>() {
        return (n < 128).then_some(n);
    }
    let split = name.find(|c: char| c.is_ascii_digit() || c == '-')?;
    let (letter, octave) = name.split_at(split);
    let octave: i32 = octave.parse().ok()?;
    let mut chars = letter.chars();
    let base = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let shift: i32 = chars
        .map(|c| match c {
            '#' => Some(1),
            'b' => Some(-1),
            _ => None,
        })
        .sum::<Option<i32>>()?;
    u8::try_from((octave + 1) * 12 + base + shift)
        .ok()
        .filter(|p| *p < 128)
}

#[cfg(test)]
mod tests;
