//! Finding samples: by words, filters, likeness and fit; building kits of what goes together;
//! and a map of the library by sound.
//!
//! Every hit says why it's there, in words, for people and for agents choosing between them:
//! "in C minor", "stretches from 92 BPM", "same pack as Kick_03", "sounds like Snare_12".

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{Category, Kind, Sample, TRAITS, note_name};
use crate::{music::Key, project::Zone};

/// What to look for. Everything is optional: leave it all out for everything.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Query {
    /// Words to look for, as a person would say them: "dark punchy kick", "dusty keys loop in
    /// C minor 95 bpm". Categories, traits, a key and a tempo in them are understood as such;
    /// other words are looked for in names and folders.
    pub text: String,
    pub kind: Option<Kind>,
    /// Any of these.
    pub categories: Vec<Category>,
    /// All of these traits (see `TRAITS`).
    pub traits: Vec<String>,
    /// Fitting this key ("C minor"): melodic samples that clash with it are left out, the rest
    /// ranked by how well they fit, with a transposition where one would help.
    pub key: Option<String>,
    /// With `key`, keys that go with it too: its relative and its neighbours round the circle of
    /// fifths, ranked a little after it.
    pub compatible: bool,
    /// Loops at this tempo, or half or double it, near enough to stretch to it.
    pub bpm: Option<f32>,
    pub tonal: Option<bool>,
    pub min_seconds: Option<f32>,
    pub max_seconds: Option<f32>,
    /// In a folder whose path has this in it (a pack).
    pub folder: Option<String>,
    /// Sounding like this sample, weighing its aspects as `aspects` says.
    pub like: Option<PathBuf>,
    pub aspects: Aspects,
    /// Going with these samples: no key clash, the same tempo, a cohesive sound and pack.
    pub with: Vec<PathBuf>,
    /// The most to give back.
    pub limit: usize,
}

impl Default for Query {
    fn default() -> Self {
        Query {
            text: String::new(),
            kind: None,
            categories: Vec::new(),
            traits: Vec::new(),
            key: None,
            compatible: false,
            bpm: None,
            tonal: None,
            min_seconds: None,
            max_seconds: None,
            folder: None,
            like: None,
            aspects: Aspects::default(),
            with: Vec::new(),
            limit: 50,
        }
    }
}

/// How much each aspect of a sound counts when comparing two, 0 to 1 (as Sononym's dial does).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Aspects {
    /// Its tone colour.
    pub timbre: f32,
    /// Its note or key.
    pub pitch: f32,
    /// How it starts and dies away.
    pub envelope: f32,
    /// Where its energy is.
    pub spectrum: f32,
}

impl Default for Aspects {
    fn default() -> Self {
        Aspects {
            timbre: 1.0,
            pitch: 0.5,
            envelope: 0.5,
            spectrum: 0.5,
        }
    }
}

/// A sample found: its place in the library, how well it matches (0 to 1, higher better), and
/// why, in words.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Hit {
    pub index: usize,
    pub score: f32,
    pub reasons: Vec<String>,
    /// Semitones to move it by to sit in the asked key, for a melodic sample that fits better
    /// moved.
    pub transpose: Option<i32>,
}

/// Words people use for categories, kinds and traits, and what they mean.
const CATEGORY_WORDS: &[(&str, &[Category])] = &[
    ("kick", &[Category::Kick]),
    ("kicks", &[Category::Kick]),
    ("bd", &[Category::Kick]),
    ("snare", &[Category::Snare]),
    ("snares", &[Category::Snare]),
    ("clap", &[Category::Clap]),
    ("claps", &[Category::Clap]),
    ("rim", &[Category::Rim]),
    ("rimshot", &[Category::Rim]),
    ("sidestick", &[Category::Rim]),
    ("snap", &[Category::Snap]),
    ("snaps", &[Category::Snap]),
    ("hat", &[Category::HatClosed, Category::HatOpen]),
    ("hats", &[Category::HatClosed, Category::HatOpen]),
    ("hihat", &[Category::HatClosed, Category::HatOpen]),
    ("hihats", &[Category::HatClosed, Category::HatOpen]),
    ("hi-hat", &[Category::HatClosed, Category::HatOpen]),
    ("ride", &[Category::Ride]),
    ("crash", &[Category::Crash]),
    ("cymbal", &[Category::Crash, Category::Ride]),
    ("tom", &[Category::Tom]),
    ("toms", &[Category::Tom]),
    ("shaker", &[Category::Shaker]),
    ("tamb", &[Category::Tambourine]),
    ("tambourine", &[Category::Tambourine]),
    ("perc", &[Category::Perc]),
    ("percussion", &[Category::Perc]),
    ("beat", &[Category::Drums]),
    ("break", &[Category::Drums]),
    ("drums", &[Category::Drums]),
    ("808", &[Category::Bass808]),
    ("808s", &[Category::Bass808]),
    ("sub", &[Category::Bass808]),
    ("bass", &[Category::Bass, Category::Bass808]),
    ("keys", &[Category::Keys]),
    ("piano", &[Category::Keys]),
    ("rhodes", &[Category::Keys]),
    ("ep", &[Category::Keys]),
    ("organ", &[Category::Keys]),
    ("pad", &[Category::Pad]),
    ("pads", &[Category::Pad]),
    ("lead", &[Category::Lead]),
    ("pluck", &[Category::Pluck]),
    ("bell", &[Category::Bell]),
    ("bells", &[Category::Bell]),
    ("strings", &[Category::Strings]),
    ("string", &[Category::Strings]),
    ("violin", &[Category::Strings]),
    ("brass", &[Category::Brass]),
    ("horn", &[Category::Brass]),
    ("horns", &[Category::Brass]),
    ("trumpet", &[Category::Brass]),
    ("flute", &[Category::Woodwind]),
    ("sax", &[Category::Woodwind]),
    ("guitar", &[Category::Guitar]),
    ("synth", &[Category::Synth]),
    ("chord", &[Category::Chord]),
    ("chords", &[Category::Chord]),
    ("stab", &[Category::Chord]),
    ("stabs", &[Category::Chord]),
    ("vocal", &[Category::Vocal]),
    ("vocals", &[Category::Vocal]),
    ("vox", &[Category::Vocal]),
    ("fx", &[Category::Fx]),
    ("riser", &[Category::Fx]),
    ("sweep", &[Category::Fx]),
    ("impact", &[Category::Fx]),
    ("reverse", &[Category::Reverse]),
    ("vinyl", &[Category::Texture]),
    ("crackle", &[Category::Texture]),
    ("noise", &[Category::Texture]),
    ("texture", &[Category::Texture]),
    ("ambience", &[Category::Texture]),
    ("foley", &[Category::Foley]),
];

/// Everyday words for the traits.
const TRAIT_WORDS: &[(&str, &str)] = &[
    ("hard", "punchy"),
    ("crisp", "snappy"),
    ("tight", "short"),
    ("deep", "sub"),
    ("low", "dark"),
    ("fat", "boomy"),
    ("thick", "warm"),
    ("smooth", "soft"),
    ("mellow", "soft"),
    ("gritty", "distorted"),
    ("dirty", "distorted"),
    ("crunchy", "distorted"),
    ("saturated", "distorted"),
    ("dusty", "lo-fi"),
    ("vintage", "lo-fi"),
    ("tape", "lo-fi"),
    ("lofi", "lo-fi"),
    ("spacey", "wet"),
    ("reverb", "wet"),
    ("verb", "wet"),
    ("ambient", "wet"),
    ("roomy", "wet"),
    ("shiny", "bright"),
    ("sparkly", "airy"),
    ("stereo", "wide"),
];

/// What a query's words say, understood: categories, kind, traits, key, tempo, and the rest.
#[derive(Default, Debug, PartialEq)]
struct Words {
    categories: Vec<Category>,
    kind: Option<Kind>,
    traits: Vec<String>,
    /// The word each trait was said by ("dusty" for lo-fi), which a name saying counts for.
    said: Vec<String>,
    key: Option<Key>,
    bpm: Option<f32>,
    rest: Vec<String>,
}

/// The categories and traits a search's words name ("punchy kick": Kick, punchy), for its chips
/// to show.
pub fn named(text: &str) -> (Vec<Category>, Vec<String>) {
    let w = understand(text);
    (w.categories, w.traits)
}

/// A search without the words that name `category` or `trait_`: a chip its words lit, clicked
/// off.
pub fn unsay(text: &str, category: Option<Category>, trait_: Option<&str>) -> String {
    let says = |s: &str| {
        let w = understand(s);
        category.is_some_and(|c| w.categories.contains(&c))
            || trait_.is_some_and(|t| w.traits.iter().any(|x| x == t))
    };
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut kept = Vec::new();
    let mut i = 0;
    while i < words.len() {
        // "open hat": two words that name it together, more than the second alone does.
        if let Some(next) = words.get(i + 1)
            && says(&format!("{} {next}", words[i]))
            && !says(words[i])
            && named(&format!("{} {next}", words[i])).0 != named(next).0
        {
            i += 2;
            continue;
        }
        if !says(words[i]) {
            kept.push(words[i]);
        }
        i += 1;
    }
    kept.join(" ")
}

fn understand(text: &str) -> Words {
    let lower = text.to_lowercase().replace(['(', ')', ','], " ");
    let tokens: Vec<&str> = lower.split_whitespace().collect();
    let mut w = Words::default();
    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i];
        let next = tokens.get(i + 1).copied().unwrap_or("");
        // "95 bpm", "95bpm"; "in C minor", "C minor", "Am", "F#m".
        if let Some(n) = t.strip_suffix("bpm").and_then(|n| n.parse::<f32>().ok()) {
            w.bpm = Some(n);
        } else if (next == "bpm") && t.parse::<f32>().is_ok() {
            w.bpm = t.parse().ok();
            i += 1;
        } else if let Some(k) = Key::parse(&format!("{t} {next}"))
            .filter(|_| matches!(next, "minor" | "major" | "min" | "maj"))
        {
            w.key = Some(k);
            i += 1;
        } else if (2..=3).contains(&t.len())
            && t.chars().next().is_some_and(|c| "abcdefg".contains(c))
            && (t.ends_with('m') || t.ends_with('#') || t.ends_with('b'))
            && let Some(k) = Key::parse(t)
            && !CATEGORY_WORDS.iter().any(|(word, _)| *word == t)
        {
            // "Am", "F#m", "Bb": a key. A lone letter is one only after "in" ("in C").
            w.key = Some(k);
        } else if t == "in"
            && next.len() <= 2
            && !matches!(
                tokens.get(i + 2).copied(),
                Some("minor" | "major" | "min" | "maj")
            )
            && let Some(k) = Key::parse(next)
        {
            w.key = Some(k);
            i += 1;
        } else if t == "loop" || t == "loops" {
            w.kind = Some(Kind::Loop);
        } else if matches!(
            t,
            "one-shot" | "oneshot" | "one-shots" | "oneshots" | "hit" | "hits"
        ) {
            w.kind = Some(Kind::OneShot);
        } else if t == "one" && (next == "shot" || next == "shots") {
            w.kind = Some(Kind::OneShot);
            i += 1;
        } else if (t == "open" || t == "closed") && next.starts_with("hat") {
            w.categories.push(if t == "open" {
                Category::HatOpen
            } else {
                Category::HatClosed
            });
            i += 1;
        } else if let Some((_, cats)) = CATEGORY_WORDS.iter().find(|(word, _)| *word == t) {
            w.categories.extend_from_slice(cats);
        } else if let Some((name, _)) = TRAITS.iter().find(|(name, _)| *name == t) {
            w.traits.push(name.to_string());
            w.said.push(t.to_string());
        } else if let Some((_, name)) = TRAIT_WORDS.iter().find(|(word, _)| *word == t) {
            w.traits.push(name.to_string());
            w.said.push(t.to_string());
        } else if !matches!(
            t,
            "in" | "a" | "an" | "the" | "with" | "and" | "of" | "some"
        ) {
            w.rest.push(t.to_string());
        }
        i += 1;
    }
    w
}

/// Pearson correlation of two 12-long profiles.
fn correlate(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    let (ma, mb) = (a.iter().sum::<f32>() / 12.0, b.iter().sum::<f32>() / 12.0);
    let (mut num, mut da, mut db) = (0.0, 0.0, 0.0);
    for i in 0..12 {
        num += (a[i] - ma) * (b[i] - mb);
        da += (a[i] - ma).powi(2);
        db += (b[i] - mb).powi(2);
    }
    if da > 0.0 && db > 0.0 {
        num / (da * db).sqrt()
    } else {
        0.0
    }
}

fn rotate(chroma: &[f32; 12], semitones: i32) -> [f32; 12] {
    std::array::from_fn(|i| chroma[(i as i32 - semitones).rem_euclid(12) as usize])
}

/// What a sample is to a key: whether it has none to clash (drums, noise), plays any note (a
/// one-shot with one pitch, as a sampler plays it), or has notes that fit or don't.
enum Harmony {
    None,
    Tunable(u8),
    Notes([f32; 12]),
}

fn harmony(s: &Sample) -> Harmony {
    let a = &s.analysis;
    if a.category.is_drum()
        || a.tonal < 0.35
        || matches!(a.category, Category::Texture | Category::Foley)
    {
        return Harmony::None;
    }
    let single = matches!(
        a.category,
        Category::Bass808
            | Category::Bass
            | Category::Lead
            | Category::Pluck
            | Category::Bell
            | Category::Synth
    );
    match (&a.pitch, a.kind) {
        // A clearly tonal note is trusted at a lower confidence: a low bass tone's is modest.
        (Some(p), Kind::OneShot)
            if single && (p.confidence > 0.5 || a.tonal >= 0.8 && p.confidence > 0.3) =>
        {
            Harmony::Tunable(p.note)
        }
        _ if a.chroma.iter().sum::<f32>() > 0.0 => Harmony::Notes(a.chroma),
        _ => Harmony::None,
    }
}

/// How well a sample fits a key, 0 to 1, the transposition that would fit it better (if one
/// would), and why.
fn key_fit(s: &Sample, key: Key) -> (f32, Option<i32>, String) {
    match harmony(s) {
        Harmony::None => (1.0, None, "no key to clash".into()),
        Harmony::Tunable(note) => (
            1.0,
            None,
            format!("plays any note (root {})", note_name(note)),
        ),
        Harmony::Notes(chroma) => {
            // How much of its sound is on the key's scale: a chord of the relative major is all
            // on it, a chord a tritone off hardly any.
            let total: f32 = chroma.iter().sum();
            let fit = |shift: i32| {
                let moved = rotate(&chroma, shift);
                (0..12u8)
                    .filter(|&pc| key.contains(pc))
                    .map(|pc| moved[pc as usize])
                    .sum::<f32>()
                    / total
            };
            let here = fit(0);
            // A loop moves two semitones at most before it sounds moved; a hit can go further.
            let reach = match s.analysis.kind {
                Kind::Loop => 2,
                Kind::OneShot => 6,
            };
            let (best, best_fit) = (-reach..=reach)
                .map(|t| (t, fit(t)))
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap();
            let score = |share: f32| ((share - 0.5) / 0.42).clamp(0.0, 1.0);
            let named = s.analysis.key.as_ref().map(|k| k.name.clone());
            let by_notes = if best != 0 && best_fit - here > 0.08 {
                let why = match &named {
                    Some(k) => format!("in {k}: {best:+} semitones fits {}", key.name()),
                    None => format!("{best:+} semitones fits {}", key.name()),
                };
                (score(here).max(score(best_fit) * 0.85), Some(best), why)
            } else {
                let why = match &named {
                    Some(k) if *k == key.name() => format!("in {k}"),
                    Some(k) => format!("in {k}, which fits {}", key.name()),
                    None => format!("its notes fit {}", key.name()),
                };
                (score(here), None, why)
            };
            // The key analysis is sure of: in the key (or its relative) it fits as it is, ahead
            // of anything moved into it, however busy its notes; a step or two off, moved.
            let by_key = s
                .analysis
                .key
                .as_ref()
                .filter(|k| k.confidence >= 0.3)
                .and_then(|k| {
                    let sure = 0.9 + 0.1 * k.confidence.min(1.0);
                    match super::place::transpose_to(&s.analysis, key).round() as i32 {
                        0 if k.name == key.name() => Some((sure, None, format!("in {}", k.name))),
                        0 => Some((
                            sure,
                            None,
                            format!("in {}, which fits {}", k.name, key.name()),
                        )),
                        t if t.abs() <= reach => Some((
                            sure * 0.85,
                            Some(t),
                            format!("in {}: {t:+} semitones fits {}", k.name, key.name()),
                        )),
                        _ => None,
                    }
                });
            match by_key {
                Some(k) if k.0 > by_notes.0 => k,
                _ => by_notes,
            }
        }
    }
}

/// How well a loop fits a tempo (directly, or at half or double), 0 to 1, and why. One-shots
/// fit any.
fn bpm_fit(s: &Sample, bpm: f32) -> (f32, Option<String>) {
    let a = &s.analysis;
    if a.kind == Kind::OneShot {
        return (1.0, None);
    }
    let Some(own) = a.bpm else {
        return (0.5, Some("tempo unknown".into()));
    };
    // Heard as `put` places it: at its own tempo unless half or double time is much nearer.
    let factor = (super::place::feel(own as f64, bpm as f64) / own as f64) as f32;
    let off = ((own * factor - bpm) / bpm).abs();
    let how = match factor {
        2.0 => " (double time)",
        0.5 => " (half time)",
        _ => "",
    };
    match off {
        o if o <= 0.01 => (1.0, Some(format!("at {own:.0} BPM{how}"))),
        o if o <= 0.06 => (0.85, Some(format!("stretches from {own:.0} BPM{how}"))),
        o if o <= 0.12 => (
            0.6,
            Some(format!("stretches from {own:.0} BPM{how}, audibly")),
        ),
        _ => (0.15, Some(format!("at {own:.0} BPM, far from {bpm:.0}"))),
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let (mut dot, mut na, mut nb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na > 0.0 && nb > 0.0 {
        dot / (na * nb).sqrt()
    } else {
        0.0
    }
}

/// How alike two samples are, 0 to 1, by the aspects' weights.
pub fn likeness(a: &Sample, b: &Sample, w: &Aspects) -> f32 {
    let (x, y) = (&a.analysis, &b.analysis);
    let timbre =
        match x.embedding.iter().any(|v| *v != 0.0) && y.embedding.iter().any(|v| *v != 0.0) {
            true => cosine(&x.embedding, &y.embedding).max(0.0),
            false => 0.5,
        } * if x.category == y.category { 1.0 } else { 0.85 };
    let pitch = match (&x.pitch, &y.pitch) {
        (Some(p), Some(q)) => {
            let d = ((p.note as f32 + p.cents / 100.0) - (q.note as f32 + q.cents / 100.0)).abs();
            (1.0 - d / 12.0).max(0.0)
        }
        _ if x.tonal > 0.35 && y.tonal > 0.35 => (correlate(&x.chroma, &y.chroma) + 1.0) / 2.0,
        _ => 1.0 - (x.tonal - y.tonal).abs(),
    };
    let ln = |v: f32| (v.max(1.0)).ln();
    let (e, f) = (&x.envelope, &y.envelope);
    let envelope = (-((ln(e.attack_ms) - ln(f.attack_ms)).abs() * 0.5
        + (ln(e.decay_ms) - ln(f.decay_ms)).abs() * 0.7
        + (ln(e.tail_ms) - ln(f.tail_ms)).abs() * 0.4
        + (e.sustain - f.sustain).abs()))
    .exp();
    let (s, t) = (&x.spectrum, &y.spectrum);
    let bands: f32 = s
        .bands_db
        .iter()
        .zip(&t.bands_db)
        .map(|(p, q)| (p.max(-60.0) - q.max(-60.0)).abs())
        .sum::<f32>()
        / 6.0;
    let spectrum = (-((s.centroid_hz.max(20.0) / t.centroid_hz.max(20.0))
        .log2()
        .abs()
        * 0.8
        + bands / 12.0
        + (s.flatness - t.flatness).abs() * 2.0))
        .exp();
    let total = w.timbre + w.pitch + w.envelope + w.spectrum;
    if total <= 0.0 {
        return 0.0;
    }
    (timbre * w.timbre + pitch * w.pitch + envelope * w.envelope + spectrum * w.spectrum) / total
}

/// How well a sample goes with another in a song, 0 to 1, and why: no key clash, tempos that
/// meet, a cohesive sound (loudness, colour, the same pack).
fn goes_with(s: &Sample, other: &Sample) -> (f32, Vec<String>) {
    let mut reasons = Vec::new();
    let mut score = 0.6;
    if s.folder == other.folder && !s.folder.is_empty() {
        score += 0.2;
        reasons.push(format!("same pack as {}", other.name));
    } else if pack(&s.folder) == pack(&other.folder) && !pack(&s.folder).is_empty() {
        score += 0.12;
        reasons.push(format!("same pack as {}", other.name));
    }
    if let Some(k) = &other.analysis.key
        && let Some(key) = Key::parse(&k.name)
        && let Harmony::Notes(_) = harmony(s)
    {
        let (fit, _, _) = key_fit(s, key);
        score *= 0.5 + 0.5 * fit;
        if fit < 0.4 {
            reasons.push(format!("clashes with {}", other.name));
        }
    }
    if let (Some(a), Some(b), Kind::Loop, Kind::Loop) = (
        s.analysis.bpm,
        other.analysis.bpm,
        s.analysis.kind,
        other.analysis.kind,
    ) {
        let (fit, _) = bpm_fit(s, b);
        score *= 0.6 + 0.4 * fit;
        if (a - b).abs() < 1.0 {
            reasons.push(format!("same tempo as {}", other.name));
        }
    }
    // A cohesive sound: alike in loudness and lo-fi, wet or distorted character.
    let loud = (-(s.analysis.loudness - other.analysis.loudness).abs() / 12.0).exp();
    let shared = ["lo-fi", "distorted", "wet", "dry", "warm", "bright", "dark"]
        .iter()
        .filter(|t| {
            s.analysis.traits.iter().any(|x| x == *t)
                && other.analysis.traits.iter().any(|x| x == *t)
        })
        .count();
    score += 0.1 * loud + 0.05 * shared.min(2) as f32;
    (score.min(1.0), reasons)
}

/// Whether a sample's name or folder says `w`, word by word ("b" is pack B, not every b):
/// "kick" finds Kick_03, and "jazzy", "funky", "orchestral" find Jazz, Funk and Orchestra.
fn mentions(s: &Sample, w: &str) -> bool {
    format!("{} {}", s.name, s.folder)
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .any(|t| t == w || (w.len() >= 3 && t.starts_with(w)) || (t.len() >= 4 && w.starts_with(t)))
}

/// A folder's first part: the pack, usually.
fn pack(folder: &str) -> &str {
    folder.split(['/', '\\']).next().unwrap_or("")
}

/// The samples matching `q`, best first.
pub fn find(samples: &[Sample], q: &Query) -> Vec<Hit> {
    let words = understand(&q.text);
    let categories: Vec<Category> = q
        .categories
        .iter()
        .chain(&words.categories)
        .copied()
        .collect();
    // Traits picked keep to them; traits in words rank, since how a sound comes across is
    // judged, not measured.
    let traits: Vec<&String> = q.traits.iter().chain(&words.traits).collect();
    let kind = q.kind.or(words.kind);
    let key = q.key.as_deref().and_then(Key::parse).or(words.key);
    let bpm = q.bpm.or(words.bpm);
    let like = q
        .like
        .as_ref()
        .and_then(|p| samples.iter().find(|s| &s.path == p));
    let with: Vec<&Sample> = q
        .with
        .iter()
        .filter_map(|p| samples.iter().find(|s| &s.path == p))
        .collect();
    let mut hits: Vec<Hit> = samples
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            let a = &s.analysis;
            kind.is_none_or(|k| a.kind == k)
                && (categories.is_empty() || categories.contains(&a.category))
                && q.traits.iter().all(|t| a.traits.contains(t))
                && q.folder
                    .as_ref()
                    .is_none_or(|f| s.folder.to_lowercase().contains(&f.to_lowercase()))
                && q.min_seconds.is_none_or(|m| a.seconds >= m)
                && q.max_seconds.is_none_or(|m| a.seconds <= m)
                && q.tonal.is_none_or(|t| (a.tonal >= 0.5) == t)
                && like.is_none_or(|l| l.path != s.path)
                && !with.iter().any(|w| w.path == s.path)
        })
        .filter_map(|(index, s)| {
            let a = &s.analysis;
            let mut reasons = Vec::new();
            let mut parts: Vec<(f32, f32)> = Vec::new(); // (score, weight)
            // Words left over: names and folders. Every word, as a search box means it.
            if !words.rest.is_empty() {
                if !words.rest.iter().all(|w| mentions(s, w)) {
                    return None;
                }
                parts.push((1.0, 1.0));
            }
            if !categories.is_empty() {
                parts.push((0.6 + 0.4 * a.confidence, 0.5));
            }
            let mut transpose = None;
            if let Some(key) = key {
                let (fit, t, why) = match q.compatible {
                    // The best of the key and those that go with it, its own a little ahead.
                    true => super::place::related(key)
                        .into_iter()
                        .enumerate()
                        .map(|(i, k)| {
                            let (fit, t, why) = key_fit(s, k);
                            (if i == 0 { fit } else { fit * 0.92 }, t, why)
                        })
                        .max_by(|a, b| a.0.total_cmp(&b.0))
                        .unwrap(),
                    false => key_fit(s, key),
                };
                // Strictly in the key, or loosely with those that go with it.
                if fit < if q.compatible { 0.4 } else { 0.6 } {
                    return None;
                }
                transpose = t;
                // A drum's lack of a key goes without saying.
                if !matches!(harmony(s), Harmony::None) {
                    reasons.push(why);
                }
                parts.push((fit, 1.5));
            }
            if let Some(bpm) = bpm {
                let (fit, why) = bpm_fit(s, bpm);
                if fit < 0.5 {
                    return None;
                }
                reasons.extend(why);
                parts.push((fit, 1.5));
            }
            if let Some(l) = like {
                let alike = likeness(s, l, &q.aspects);
                reasons.push(format!("sounds {:.0}% like {}", alike * 100.0, l.name));
                parts.push((alike, 3.0));
            }
            if !with.is_empty() {
                let mut total = 0.0;
                let mut mates = Vec::new();
                for w in &with {
                    let (fit, why) = goes_with(s, w);
                    total += fit;
                    for r in why {
                        // Pack-mates are told once: "same pack as Kick_01, Snare_02 and 2 more".
                        match r.strip_prefix("same pack as ") {
                            Some(name) => mates.push(name.to_string()),
                            None if !reasons.contains(&r) => reasons.push(r),
                            None => {}
                        }
                    }
                }
                if !mates.is_empty() {
                    let named = mates.iter().take(2).cloned().collect::<Vec<_>>().join(", ");
                    reasons.push(match mates.len() {
                        1 | 2 => format!("same pack as {named}"),
                        n => format!("same pack as {named} and {} more", n - 2),
                    });
                }
                parts.push((total / with.len() as f32, 2.0));
            }
            // A trait in words is there by ear, or by name: Dusty Kick 01 is dusty.
            let picked = q.traits.len();
            let has: Vec<&&String> = traits
                .iter()
                .enumerate()
                .filter(|&(i, t)| {
                    a.traits.contains(t) || i >= picked && mentions(s, &words.said[i - picked])
                })
                .map(|(_, t)| t)
                .collect();
            if !words.traits.is_empty() {
                parts.push((has.len() as f32 / traits.len() as f32, 1.0));
            }
            for t in has {
                reasons.push((*t).clone());
            }
            let weight: f32 = parts.iter().map(|p| p.1).sum();
            let score = match weight > 0.0 {
                true => parts.iter().map(|p| p.0 * p.1).sum::<f32>() / weight,
                false => 0.5 + 0.5 * a.confidence,
            };
            Some(Hit {
                index,
                score,
                reasons,
                transpose,
            })
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| samples[a.index].name.cmp(&samples[b.index].name))
    });
    hits.truncate(q.limit.max(1));
    hits
}

/// The samples most like `path`, by the aspects' weights, of the same kind.
pub fn similar(
    samples: &[Sample],
    path: &std::path::Path,
    aspects: Aspects,
    limit: usize,
) -> Vec<Hit> {
    let Some(of) = samples.iter().find(|s| s.path == path) else {
        return Vec::new();
    };
    find(
        samples,
        &Query {
            kind: Some(of.analysis.kind),
            like: Some(path.to_path_buf()),
            aspects,
            limit,
            ..Default::default()
        },
    )
}

/// What a kit is built from: a key and tempo to fit, words and traits for its sound, a sample
/// it should sound like, a pack to keep to, and the roles it needs (a hip-hop kit by default).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct KitSeed {
    pub key: Option<String>,
    pub bpm: Option<f32>,
    pub text: String,
    pub traits: Vec<String>,
    pub like: Option<PathBuf>,
    pub folder: Option<String>,
    pub roles: Vec<Category>,
    /// Alternatives to offer for each role (4 if not said).
    pub alternatives: usize,
    /// Samples to keep (a role they fill isn't picked again), as when some are locked.
    pub keep: Vec<PathBuf>,
}

/// A kit: for each role, its pick and what else would do.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Kit {
    pub slots: Vec<KitSlot>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct KitSlot {
    pub role: Category,
    pub pick: Option<Hit>,
    pub alternatives: Vec<Hit>,
}

/// The roles of a kit when none are asked for: what a beat needs.
pub const KIT_ROLES: [Category; 8] = [
    Category::Kick,
    Category::Snare,
    Category::Clap,
    Category::HatClosed,
    Category::HatOpen,
    Category::Perc,
    Category::Bass808,
    Category::Keys,
];

/// Builds a kit from `seed`: each role in turn, the best fit for the words, key and tempo that
/// also goes with what's been picked so far (no clashes, one sound), so it holds together.
pub fn kit(samples: &[Sample], seed: &KitSeed) -> Kit {
    let roles = match seed.roles.is_empty() {
        true => KIT_ROLES.to_vec(),
        false => seed.roles.clone(),
    };
    let alternatives = match seed.alternatives {
        0 => 4,
        n => n,
    };
    let words = understand(&seed.text);
    // The seed's words, less any category in them (each role is its own). How they say it should
    // sound ranks, as in a search; traits picked are kept to.
    let rest = words.rest.join(" ");
    let feel = words.said.join(" ");
    let traits = seed.traits.clone();
    let key = seed.key.clone().or(words.key.map(|k| k.name()));
    let bpm = seed.bpm.or(words.bpm);
    let mut chosen: Vec<PathBuf> = seed.keep.clone();
    chosen.extend(seed.like.iter().cloned());
    let slots = roles
        .into_iter()
        .map(|role| {
            // A kept sample in this role stays.
            if let Some((index, _)) = samples
                .iter()
                .enumerate()
                .find(|(_, s)| seed.keep.contains(&s.path) && s.analysis.category == role)
            {
                return KitSlot {
                    role,
                    pick: Some(Hit {
                        index,
                        score: 1.0,
                        reasons: vec!["kept".into()],
                        transpose: None,
                    }),
                    alternatives: Vec::new(),
                };
            }
            let ask = |rest: &str, traits: Vec<String>, folder: Option<String>| Query {
                text: format!("{rest} {feel}"),
                categories: vec![role],
                traits,
                key: key.clone(),
                bpm,
                folder,
                with: chosen.clone(),
                limit: alternatives + 1,
                ..Default::default()
            };
            // Loosen what was asked until something's there: the pack, the words names are
            // looked for by (a "jazzy" kit's kick needn't be called Jazz), then the traits.
            let mut hits = find(samples, &ask(&rest, traits.clone(), seed.folder.clone()));
            if hits.is_empty() && seed.folder.is_some() {
                hits = find(samples, &ask(&rest, traits.clone(), None));
            }
            if hits.is_empty() && !rest.is_empty() {
                hits = find(samples, &ask("", traits.clone(), None));
            }
            if hits.is_empty() && !traits.is_empty() {
                hits = find(samples, &ask("", Vec::new(), None));
            }
            let mut hits = hits.into_iter();
            let pick = hits.next();
            if let Some(p) = &pick {
                chosen.push(samples[p.index].path.clone());
            }
            KitSlot {
                role,
                pick,
                alternatives: hits.collect(),
            }
        })
        .collect();
    Kit { slots }
}

/// Every sample's place on a map of sound, -1 to 1 each way: near ones sound alike. The two
/// directions its timbre varies most in (principal components of the embeddings), spread out by
/// rank so a crowd doesn't sit on one spot.
pub fn map(samples: &[Sample]) -> Vec<[f32; 2]> {
    let n = samples.len();
    let dims = samples.first().map_or(0, |s| s.analysis.embedding.len());
    let usable = dims > 0
        && samples.iter().all(|s| {
            s.analysis.embedding.len() == dims && s.analysis.embedding.iter().any(|v| *v != 0.0)
        });
    if n < 3 || !usable {
        // Without embeddings: categories round a circle, each a little cloud.
        return samples
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let c = Category::ALL
                    .iter()
                    .position(|c| *c == s.analysis.category)
                    .unwrap_or(0);
                let angle = c as f32 / Category::ALL.len() as f32 * std::f32::consts::TAU;
                let jitter = ((i * 7919) % 1000) as f32 / 1000.0 * 0.25;
                [angle.cos() * (0.6 + jitter), angle.sin() * (0.6 + jitter)]
            })
            .collect();
    }
    let mean: Vec<f32> = (0..dims)
        .map(|d| samples.iter().map(|s| s.analysis.embedding[d]).sum::<f32>() / n as f32)
        .collect();
    let centred: Vec<Vec<f32>> = samples
        .iter()
        .map(|s| {
            s.analysis
                .embedding
                .iter()
                .zip(&mean)
                .map(|(v, m)| v - m)
                .collect()
        })
        .collect();
    // The covariance's top two eigenvectors, by power iteration (the second with the first
    // taken out).
    let cov: Vec<Vec<f32>> = (0..dims)
        .map(|i| {
            (0..dims)
                .map(|j| centred.iter().map(|v| v[i] * v[j]).sum::<f32>() / n as f32)
                .collect()
        })
        .collect();
    let power = |against: Option<&Vec<f32>>| {
        let mut v: Vec<f32> = (0..dims).map(|i| 1.0 + i as f32 * 0.01).collect();
        for _ in 0..100 {
            let mut next: Vec<f32> = (0..dims)
                .map(|i| (0..dims).map(|j| cov[i][j] * v[j]).sum())
                .collect();
            if let Some(u) = against {
                let along: f32 = next.iter().zip(u).map(|(a, b)| a * b).sum();
                next.iter_mut().zip(u).for_each(|(a, b)| *a -= along * b);
            }
            let norm = next.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
            v = next.into_iter().map(|x| x / norm).collect();
        }
        v
    };
    let first = power(None);
    let second = power(Some(&first));
    let project = |u: &Vec<f32>| -> Vec<f32> {
        centred
            .iter()
            .map(|v| v.iter().zip(u).map(|(a, b)| a * b).sum())
            .collect()
    };
    let start: Vec<[f32; 2]> = project(&first)
        .into_iter()
        .zip(project(&second))
        .map(|(x, y)| [x, y])
        .collect();
    let vectors: Vec<&[f32]> = samples.iter().map(|s| &s.analysis.embedding[..]).collect();
    let placed = tsne(&vectors, &start);
    // -1 to 1, by the 98th percentile out from the middle, so a stray sound doesn't squash
    // the rest.
    let mid = |k: usize| {
        let mut v: Vec<f32> = placed.iter().map(|p| p[k]).collect();
        v.sort_by(f32::total_cmp);
        v[n / 2]
    };
    let (mx, my) = (mid(0), mid(1));
    let mut reach: Vec<f32> = placed
        .iter()
        .map(|p| (p[0] - mx).abs().max((p[1] - my).abs()))
        .collect();
    reach.sort_by(f32::total_cmp);
    let scale = reach[(n * 98 / 100).min(n - 1)].max(1e-6);
    placed
        .iter()
        .map(|p| {
            [
                ((p[0] - mx) / scale).clamp(-1.0, 1.0),
                ((p[1] - my) / scale).clamp(-1.0, 1.0),
            ]
        })
        .collect()
}

/// t-SNE (van der Maaten and Hinton, 2008), as XO and Atlas lay out their sound maps: near
/// sounds stay near, so alike sounds gather in clusters rather than smearing along a line. From
/// `start` (the first two principal components, so it comes out the same every time), with a
/// perplexity of 30, early exaggeration, and momentum. Past 1,500 sounds it lays out 1,500 of
/// them evenly picked, and puts each other sound among its five nearest.
// ponytail: exact, O(n²) a step on up to 1,500 points (a second or two, off the UI thread);
// Barnes-Hut if libraries outgrow the landmarks.
fn tsne(vectors: &[&[f32]], start: &[[f32; 2]]) -> Vec<[f32; 2]> {
    const LANDMARKS: usize = 1500;
    const PERPLEXITY: f32 = 30.0;
    const STEPS: usize = 750;
    let n = vectors.len();
    let dist = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f32>();
    let marks: Vec<usize> = match n > LANDMARKS {
        true => (0..LANDMARKS).map(|i| i * n / LANDMARKS).collect(),
        false => (0..n).collect(),
    };
    let m = marks.len();
    // Affinities: each point's neighbours weighted so their perplexity is 30, then made
    // symmetric.
    let d: Vec<Vec<f32>> = marks
        .iter()
        .map(|&i| {
            marks
                .iter()
                .map(|&j| dist(vectors[i], vectors[j]))
                .collect()
        })
        .collect();
    let target = PERPLEXITY.min((m - 1) as f32 / 3.0).max(2.0).ln();
    let mut p = vec![vec![0f32; m]; m];
    for i in 0..m {
        let (mut lo, mut hi, mut beta) = (0f32, f32::INFINITY, 1f32);
        for _ in 0..64 {
            let mut sum = 0.0;
            let mut weighted = 0.0;
            for j in 0..m {
                if j != i {
                    let w = (-d[i][j] * beta).exp();
                    p[i][j] = w;
                    sum += w;
                    weighted += w * d[i][j];
                }
            }
            let sum = sum.max(1e-12);
            // The distribution's entropy, against the perplexity's.
            let entropy = sum.ln() + beta * weighted / sum;
            for v in p[i].iter_mut() {
                *v /= sum;
            }
            if (entropy - target).abs() < 1e-4 {
                break;
            }
            if entropy > target {
                lo = beta;
                beta = if hi.is_finite() {
                    (beta + hi) / 2.0
                } else {
                    beta * 2.0
                };
            } else {
                hi = beta;
                beta = (beta + lo) / 2.0;
            }
        }
    }
    let mut sym = vec![vec![0f32; m]; m];
    for i in 0..m {
        for j in 0..m {
            sym[i][j] = ((p[i][j] + p[j][i]) / (2.0 * m as f32)).max(1e-12);
        }
    }
    // From the principal components, small.
    let spread = (marks.iter().map(|&i| start[i][0].powi(2)).sum::<f32>() / m as f32)
        .sqrt()
        .max(1e-9);
    let mut y: Vec<[f32; 2]> = marks
        .iter()
        .map(|&i| [start[i][0] / spread * 1e-2, start[i][1] / spread * 1e-2])
        .collect();
    let mut velocity = vec![[0f32; 2]; m];
    let rate = (m as f32 / 12.0).max(50.0);
    for step in 0..STEPS {
        let exaggeration = if step < 250 { 12.0 } else { 1.0 };
        let momentum = if step < 250 { 0.5 } else { 0.8 };
        // Q's normaliser: every pair's Student-t kernel.
        let mut z = 0.0;
        for i in 0..m {
            for j in i + 1..m {
                let dx = y[i][0] - y[j][0];
                let dy = y[i][1] - y[j][1];
                z += 2.0 / (1.0 + dx * dx + dy * dy);
            }
        }
        let z = z.max(1e-12);
        for i in 0..m {
            let mut g = [0f32; 2];
            for j in 0..m {
                if j == i {
                    continue;
                }
                let dx = y[i][0] - y[j][0];
                let dy = y[i][1] - y[j][1];
                let k = 1.0 / (1.0 + dx * dx + dy * dy);
                let pull = (exaggeration * sym[i][j] - k / z) * k;
                g[0] += 4.0 * pull * dx;
                g[1] += 4.0 * pull * dy;
            }
            for a in 0..2 {
                velocity[i][a] = momentum * velocity[i][a] - rate * g[a];
            }
        }
        for i in 0..m {
            y[i][0] += velocity[i][0];
            y[i][1] += velocity[i][1];
        }
    }
    if m == n {
        return y;
    }
    // The rest among their five nearest landmarks, the nearer counting more.
    let mut placed = vec![[0f32; 2]; n];
    for (k, &i) in marks.iter().enumerate() {
        placed[i] = y[k];
    }
    let is_mark: std::collections::HashSet<usize> = marks.iter().copied().collect();
    for i in (0..n).filter(|i| !is_mark.contains(i)) {
        let mut near: Vec<(f32, usize)> = (0..m)
            .map(|k| (dist(vectors[i], vectors[marks[k]]), k))
            .collect();
        near.select_nth_unstable_by(4.min(m - 1), |a, b| a.0.total_cmp(&b.0));
        let (mut sum, mut at) = (0.0, [0f32; 2]);
        for &(d, k) in near.iter().take(5) {
            let w = 1.0 / (d.sqrt() + 1e-3);
            sum += w;
            at[0] += w * y[k][0];
            at[1] += w * y[k][1];
        }
        placed[i] = [at[0] / sum, at[1] / sum];
    }
    placed
}

/// A sample in a line, for agents and tooltips: what it is, its note or key, tempo, length,
/// traits and pack.
pub fn describe(s: &Sample) -> String {
    let a = &s.analysis;
    let mut parts = vec![format!(
        "{} {}",
        a.category.name(),
        match a.kind {
            Kind::OneShot => "one-shot",
            Kind::Loop => "loop",
        }
    )];
    if let Some(k) = &a.key {
        parts.push(format!("in {}", k.name));
    } else if let Some(p) = &a.pitch {
        parts.push(format!("{} {:+.0} cents", note_name(p.note), p.cents));
    }
    if let Some(bpm) = a.bpm {
        match a.bars {
            Some(bars) => parts.push(format!("{bpm:.0} BPM, {bars:.0} bars")),
            None => parts.push(format!("{bpm:.0} BPM")),
        }
    }
    parts.push(format!("{:.2} s", a.seconds));
    if !a.traits.is_empty() {
        parts.push(a.traits.join(", "));
    }
    if !s.folder.is_empty() {
        parts.push(format!("from {}", s.folder));
    }
    parts.join(" · ")
}

/// How a sampler would play a sample: a drum on its General MIDI note, a pitched one-shot across
/// the keyboard from its own note (in tune: its cents taken back out), anything else on middle
/// C. `note` puts a drum or a hit elsewhere.
pub fn zone(s: &Sample, note: Option<u8>) -> Zone {
    let a = &s.analysis;
    let base = Zone {
        file: s.path.clone(),
        name: s.name.clone(),
        ..Zone::default()
    };
    match (a.category.drum_note(), &a.pitch, harmony(s)) {
        (Some(drum), _, _) => Zone {
            note: note.unwrap_or(drum),
            // Hats share a choke group, so a closed one cuts an open one.
            choke: matches!(a.category, Category::HatClosed | Category::HatOpen).then_some(1),
            ..base
        },
        (None, Some(p), Harmony::Tunable(_)) => Zone {
            note: 0,
            high: Some(127),
            root: Some(p.note),
            tune_cents: -p.cents,
            gate: true,
            release_ms: Some(80.0),
            ..base
        },
        _ => Zone {
            note: note.unwrap_or(60),
            ..base
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samples::{Analysis, KeyGuess, Pitch};

    fn sample(name: &str, folder: &str, a: Analysis) -> Sample {
        Sample {
            path: PathBuf::from(format!("/lib/{folder}/{name}.wav")),
            name: name.into(),
            folder: folder.into(),
            modified: 0,
            size: 0,
            analysis: a,
        }
    }

    fn drum(category: Category, traits: &[&str], embedding: [f32; 2]) -> Analysis {
        Analysis {
            category,
            confidence: 0.9,
            traits: traits.iter().map(|t| t.to_string()).collect(),
            embedding: vec![embedding[0], embedding[1]],
            seconds: 0.4,
            ..Default::default()
        }
    }

    /// The chroma of a key's triad, a little of every note under it.
    fn chroma_of(key: &str) -> [f32; 12] {
        let k = Key::parse(key).unwrap();
        let third = if k.minor { 3 } else { 4 };
        let mut c = [0.02; 12];
        for step in [0, third, 7] {
            c[(k.root as usize + step) % 12] += 0.3;
        }
        c
    }

    fn melodic(key: &str, kind: Kind, bpm: Option<f32>) -> Analysis {
        Analysis {
            category: Category::Keys,
            kind,
            bpm,
            tonal: 0.9,
            confidence: 0.8,
            chroma: chroma_of(key),
            key: Some(KeyGuess {
                tonic: Key::parse(key).unwrap().root,
                minor: Key::parse(key).unwrap().minor,
                name: Key::parse(key).unwrap().name(),
                confidence: 0.8,
            }),
            embedding: vec![0.5, 0.5],
            ..Default::default()
        }
    }

    fn library() -> Vec<Sample> {
        vec![
            sample(
                "Kick_01",
                "Pack A/Drums",
                drum(Category::Kick, &["punchy", "dark"], [1.0, 0.0]),
            ),
            sample(
                "Kick_02",
                "Pack B/Drums",
                drum(Category::Kick, &["soft"], [0.9, 0.2]),
            ),
            sample(
                "Snare_01",
                "Pack A/Drums",
                drum(Category::Snare, &["snappy"], [0.0, 1.0]),
            ),
            sample(
                "Snare_02",
                "Pack B/Drums",
                drum(Category::Snare, &["snappy", "lo-fi"], [0.1, 1.0]),
            ),
            sample(
                "Hat_01",
                "Pack A/Drums",
                drum(Category::HatClosed, &["bright"], [-1.0, 0.0]),
            ),
            sample(
                "Keys_Cm",
                "Pack A/Loops",
                melodic("C minor", Kind::Loop, Some(95.0)),
            ),
            sample(
                "Keys_Dm",
                "Pack A/Loops",
                melodic("D minor", Kind::Loop, Some(140.0)),
            ),
            sample(
                "Keys_Fs",
                "Pack B/Loops",
                melodic("F# major", Kind::Loop, Some(96.0)),
            ),
            sample(
                "Sub_01",
                "Pack A/Drums",
                Analysis {
                    category: Category::Bass808,
                    tonal: 0.9,
                    confidence: 0.9,
                    pitch: Some(Pitch {
                        note: 31,
                        cents: 40.0,
                        hz: 50.0,
                        confidence: 0.9,
                    }),
                    embedding: vec![0.7, -0.7],
                    ..Default::default()
                },
            ),
        ]
    }

    fn names(lib: &[Sample], hits: &[Hit]) -> Vec<String> {
        hits.iter().map(|h| lib[h.index].name.clone()).collect()
    }

    #[test]
    fn words_say_categories_traits_keys_and_tempos() {
        let w = understand("Dusty Keys loop in C minor 95 bpm");
        assert_eq!(w.categories, [Category::Keys]);
        assert_eq!(w.kind, Some(Kind::Loop));
        assert_eq!(w.traits, ["lo-fi"]);
        assert_eq!(w.said, ["dusty"]);
        assert_eq!(w.key, Key::parse("C minor"));
        assert_eq!(w.bpm, Some(95.0));
        let w = understand("open hat Am 120bpm vinyl");
        assert_eq!(w.categories, [Category::HatOpen, Category::Texture]);
        assert_eq!(w.key, Key::parse("A minor"));
        assert_eq!(w.bpm, Some(120.0));
        assert_eq!(understand("kick").key, None);
    }

    #[test]
    fn chips_lit_by_words_turn_off_by_taking_the_words_out() {
        assert_eq!(
            named("dusty punchy kick"),
            (
                vec![Category::Kick],
                vec!["lo-fi".to_string(), "punchy".to_string()]
            )
        );
        assert_eq!(
            unsay("dusty punchy kick", Some(Category::Kick), None),
            "dusty punchy"
        );
        assert_eq!(
            unsay("dusty punchy kick", None, Some("lo-fi")),
            "punchy kick"
        );
        assert_eq!(
            unsay("open hat loop", Some(Category::HatOpen), None),
            "loop"
        );
    }

    #[test]
    fn a_search_in_words_finds_what_it_says() {
        let lib = library();
        let q = |text: &str| Query {
            text: text.into(),
            ..Default::default()
        };
        // How it sounds ranks: the punchy kick, then the other.
        assert_eq!(
            names(&lib, &find(&lib, &q("punchy kick"))),
            ["Kick_01", "Kick_02"]
        );
        assert_eq!(names(&lib, &find(&lib, &q("snare pack b"))), ["Snare_02"]);
        // A word with an ending finds the name it's made from, as "jazzy" finds Jazz.
        assert_eq!(names(&lib, &find(&lib, &q("snare packed b"))), ["Snare_02"]);
        // A word for a trait counts in a name too: Kick_02, called dusty, is dusty.
        let mut named = lib.clone();
        named[1].name = "Dusty Kick 02".into();
        assert_eq!(
            names(&named, &find(&named, &q("dusty kick"))),
            ["Dusty Kick 02", "Kick_01"]
        );
        // A key and tempo: the C minor loop at 95 fits as it is; F# major at 96 fits two
        // semitones up (A flat major is C minor's sixth), so it comes after, saying so; D minor at
        // 140 is too far in tempo.
        let hits = find(&lib, &q("keys loop C minor 95 bpm"));
        assert_eq!(names(&lib, &hits), ["Keys_Cm", "Keys_Fs"]);
        assert!(
            hits[0].reasons.iter().any(|r| r.contains("in C minor")),
            "{:?}",
            hits[0].reasons
        );
        assert_eq!((hits[0].transpose, hits[1].transpose), (None, Some(2)));
    }

    #[test]
    fn a_loop_found_in_the_key_ranks_above_one_moved_into_it() {
        // A busy riff, its notes all over, that analysis is sure is in C minor; and D minor
        // chords, which fit two semitones down.
        let mut riff = melodic("C minor", Kind::Loop, Some(95.0));
        riff.chroma = chroma_of("C minor").map(|c| c + 0.1);
        let lib = vec![
            sample("Riff_Cm", "A", riff),
            sample("Keys_Dm", "A", melodic("D minor", Kind::Loop, Some(95.0))),
        ];
        let hits = find(
            &lib,
            &Query {
                text: "keys loop in C minor".into(),
                ..Default::default()
            },
        );
        assert_eq!(names(&lib, &hits), ["Riff_Cm", "Keys_Dm"]);
        assert_eq!((hits[0].transpose, hits[1].transpose), (None, Some(-2)));
    }

    #[test]
    fn words_for_character_rank_rather_than_rule_out() {
        let mut dusty = melodic("C minor", Kind::Loop, Some(95.0));
        dusty.traits = vec!["lo-fi".into()];
        let lib = vec![
            sample(
                "Keys_Clean",
                "A",
                melodic("C minor", Kind::Loop, Some(95.0)),
            ),
            sample("Keys_Dusty", "A", dusty),
        ];
        let q = |text: &str, traits: Vec<String>| Query {
            text: text.into(),
            traits,
            ..Default::default()
        };
        let hits = find(&lib, &q("dusty keys loop", Vec::new()));
        assert_eq!(names(&lib, &hits), ["Keys_Dusty", "Keys_Clean"]);
        // A trait picked as a filter keeps to it.
        let hits = find(&lib, &q("keys loop", vec!["lo-fi".into()]));
        assert_eq!(names(&lib, &hits), ["Keys_Dusty"]);
    }

    #[test]
    fn out_of_key_loops_say_how_far_to_move() {
        let lib = library();
        let hits = find(
            &lib,
            &Query {
                key: Some("C minor".into()),
                categories: vec![Category::Keys],
                ..Default::default()
            },
        );
        // D minor fits C minor two semitones down.
        let dm = hits
            .iter()
            .find(|h| lib[h.index].name == "Keys_Dm")
            .expect("Keys_Dm");
        assert_eq!(dm.transpose, Some(-2));
        // Drums and the 808 have nothing to clash.
        let all = find(
            &lib,
            &Query {
                key: Some("F# major".into()),
                ..Default::default()
            },
        );
        assert!(names(&lib, &all).contains(&"Sub_01".to_string()));
        assert!(names(&lib, &all).contains(&"Kick_01".to_string()));
    }

    #[test]
    fn likeness_finds_neighbours_and_kits_hold_together() {
        let lib = library();
        let near = similar(&lib, &lib[0].path, Aspects::default(), 3);
        assert_eq!(lib[near[0].index].name, "Kick_02");
        // A kit from pack A, in C minor at 95: everything from pack A that fits.
        let k = kit(
            &lib,
            &KitSeed {
                key: Some("C minor".into()),
                bpm: Some(95.0),
                text: "punchy".into(),
                roles: vec![
                    Category::Kick,
                    Category::Snare,
                    Category::Bass808,
                    Category::Keys,
                ],
                ..Default::default()
            },
        );
        let picks: Vec<String> = k
            .slots
            .iter()
            .map(|s| {
                s.pick
                    .as_ref()
                    .map_or("-".into(), |p| lib[p.index].name.clone())
            })
            .collect();
        assert_eq!(picks, ["Kick_01", "Snare_01", "Sub_01", "Keys_Cm"]);
        // Words nothing in a role is called don't leave it empty; how they say it sounds ranks.
        let jazzy = kit(
            &lib,
            &KitSeed {
                text: "jazzy punchy".into(),
                roles: vec![Category::Kick],
                ..Default::default()
            },
        );
        let kick = jazzy.slots[0]
            .pick
            .as_ref()
            .map(|p| lib[p.index].name.as_str());
        assert_eq!(kick, Some("Kick_01"));
        let snare = k.slots[1].pick.as_ref().unwrap();
        assert!(
            snare.reasons.iter().any(|r| r.contains("same pack")),
            "{:?}",
            snare.reasons
        );
    }

    #[test]
    fn a_pitched_one_shot_plays_in_tune_across_the_keys() {
        let lib = library();
        let z = zone(&lib[8], None);
        assert_eq!(
            (z.root, z.high, z.tune_cents, z.gate),
            (Some(31), Some(127), -40.0, true)
        );
        let k = zone(&lib[0], None);
        assert_eq!((k.note, k.high), (36, None));
        assert_eq!(zone(&lib[4], None).choke, Some(1));
        // A clean low tone the pitch tracker is only half sure of still plays in tune.
        let mut low = lib[8].clone();
        low.analysis.tonal = 0.99;
        low.analysis.pitch.as_mut().unwrap().confidence = 0.45;
        assert_eq!(zone(&low, None).root, Some(31));
        low.analysis.tonal = 0.6;
        assert_eq!(zone(&low, None).root, None);
    }

    #[test]
    fn the_map_gathers_alike_sounds_into_clusters() {
        // Three kinds of sound, twenty each, near their own kind in the embedding.
        let mut lib = Vec::new();
        for (c, centre) in [[1.0f32, 0.0], [-0.5, 0.86], [-0.5, -0.86]]
            .iter()
            .enumerate()
        {
            for i in 0..20 {
                let wobble = ((i * 37 + c * 11) % 17) as f32 / 17.0 * 0.3 - 0.15;
                let mut e = vec![0.0; 8];
                e[0] = centre[0] + wobble;
                e[1] = centre[1] - wobble * 0.5;
                e[2 + c] = 0.5;
                lib.push(sample(
                    &format!("S{c}_{i}"),
                    "A",
                    Analysis {
                        embedding: e,
                        ..drum(Category::Kick, &[], [0.0, 0.0])
                    },
                ));
            }
        }
        let m = map(&lib);
        let d =
            |a: usize, b: usize| ((m[a][0] - m[b][0]).powi(2) + (m[a][1] - m[b][1]).powi(2)).sqrt();
        let mean = |pairs: &mut dyn Iterator<Item = (usize, usize)>| {
            let v: Vec<f32> = pairs.map(|(a, b)| d(a, b)).collect();
            v.iter().sum::<f32>() / v.len() as f32
        };
        let within = mean(&mut (0..60).flat_map(|a| {
            (0..60)
                .filter(move |b| a / 20 == b / 20 && a != *b)
                .map(move |b| (a, b))
        }));
        let between = mean(&mut (0..60).flat_map(|a| {
            (0..60)
                .filter(move |b| a / 20 != b / 20)
                .map(move |b| (a, b))
        }));
        assert!(between > within * 3.0, "within {within}, between {between}");
    }

    #[test]
    fn the_map_spreads_samples_by_sound() {
        let lib = library();
        let m = map(&lib);
        assert_eq!(m.len(), lib.len());
        assert!(m.iter().all(|p| p[0].abs() <= 1.0 && p[1].abs() <= 1.0));
        // The two kicks sit nearer each other than the kick and the hat.
        let d =
            |a: usize, b: usize| ((m[a][0] - m[b][0]).powi(2) + (m[a][1] - m[b][1]).powi(2)).sqrt();
        assert!(d(0, 1) < d(0, 4), "{m:?}");
    }
}
