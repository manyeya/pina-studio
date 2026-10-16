//! Finding samples: by words, filters, likeness and fit; and building kits of what goes
//! together.
// ponytail: a first pass (words and filters); likeness, fit and kits come next.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{Category, Kind, Sample};

/// What to look for. Everything is optional: leave it all out for everything.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Query {
    /// Words to look for in names, folders, categories and traits: "dark punchy kick".
    pub text: String,
    pub kind: Option<Kind>,
    /// Any of these.
    pub categories: Vec<Category>,
    /// All of these traits (see `TRAITS`).
    pub traits: Vec<String>,
    /// In, or fitting, this key ("C minor"): melodic samples whose key or note fits it.
    pub key: Option<String>,
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
    /// Going with these samples: a key that doesn't clash, a cohesive sound.
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

/// A sample found: its place in the library, how well it matches (higher is better, 0 to 1),
/// and why, in words.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Hit {
    pub index: usize,
    pub score: f32,
    pub reasons: Vec<String>,
    /// Semitones to move it by to sit in the asked key, for a melodic sample that's out of it.
    pub transpose: Option<i32>,
}

/// The samples matching `q`, best first.
pub fn find(samples: &[Sample], q: &Query) -> Vec<Hit> {
    let words: Vec<String> = q
        .text
        .to_lowercase()
        .split_whitespace()
        .map(String::from)
        .collect();
    let mut hits: Vec<Hit> = samples
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            let a = &s.analysis;
            q.kind.is_none_or(|k| a.kind == k)
                && (q.categories.is_empty() || q.categories.contains(&a.category))
                && q.traits.iter().all(|t| a.traits.contains(t))
                && q.folder
                    .as_ref()
                    .is_none_or(|f| s.folder.to_lowercase().contains(&f.to_lowercase()))
                && q.min_seconds.is_none_or(|m| a.seconds >= m)
                && q.max_seconds.is_none_or(|m| a.seconds <= m)
        })
        .filter_map(|(index, s)| {
            let haystack = format!(
                "{} {} {} {}",
                s.name,
                s.folder,
                s.analysis.category.name(),
                s.analysis.traits.join(" ")
            )
            .to_lowercase();
            let matched = words
                .iter()
                .filter(|w| haystack.contains(w.as_str()))
                .count();
            (words.is_empty() || matched > 0).then(|| Hit {
                index,
                score: if words.is_empty() {
                    1.0
                } else {
                    matched as f32 / words.len() as f32
                },
                reasons: Vec::new(),
                transpose: None,
            })
        })
        .collect();
    hits.sort_by(|a, b| b.score.total_cmp(&a.score));
    hits.truncate(q.limit);
    hits
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
    /// Alternatives to offer for each role.
    pub alternatives: usize,
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

/// Builds a kit from `seed`: for each role, the best fit that goes with the rest.
pub fn kit(samples: &[Sample], seed: &KitSeed) -> Kit {
    let roles = if seed.roles.is_empty() {
        vec![
            Category::Kick,
            Category::Snare,
            Category::HatClosed,
            Category::HatOpen,
            Category::Perc,
            Category::Bass808,
            Category::Keys,
        ]
    } else {
        seed.roles.clone()
    };
    let slots = roles
        .into_iter()
        .map(|role| {
            let q = Query {
                text: seed.text.clone(),
                categories: vec![role],
                limit: seed.alternatives + 1,
                ..Default::default()
            };
            let mut hits = find(samples, &q).into_iter();
            KitSlot {
                role,
                pick: hits.next(),
                alternatives: hits.collect(),
            }
        })
        .collect();
    Kit { slots }
}

/// Every sample's place on a map of sound, -1 to 1 each way: near ones sound alike.
pub fn map(samples: &[Sample]) -> Vec<[f32; 2]> {
    let n = samples.len().max(1) as f32;
    (0..samples.len())
        .map(|i| [i as f32 / n * 2.0 - 1.0, 0.0])
        .collect()
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
        parts.push(format!(
            "{} {:+.0} cents",
            super::note_name(p.note),
            p.cents
        ));
    }
    if let Some(bpm) = a.bpm {
        parts.push(format!("{bpm:.0} BPM"));
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
