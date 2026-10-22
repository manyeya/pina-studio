//! Finding samples: by words, filters, likeness and fit; building kits of what goes together;
//! and a map of the library by sound.
//!
//! Every hit says why it's there, in words, for people and for agents choosing between them:
//! "in C minor", "stretches from 92 BPM", "same pack as Kick_03", "sounds like Snare_12".

mod fit;
mod kits;
mod likeness;
mod placing;
mod sound_map;
mod words;

pub(crate) use fit::*;
pub use kits::*;
pub use likeness::*;
pub use placing::*;
pub use sound_map::*;
pub use words::*;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{Category, Kind, Sample, TRAITS, note_name};
use crate::{music::Key, song::project::Zone};

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
            // A drum good at being its drum: short closed hats, kicks without a tail.
            if let Some((good, why)) = profile(a) {
                parts.push((good, 2.0));
                reasons.extend(why.map(str::to_string));
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
    // The same sound twice (a pack's copy in another folder) is said once.
    let mut seen = std::collections::HashSet::new();
    hits.retain(|h| {
        let f = samples[h.index].analysis.fingerprint;
        f == 0 || seen.insert(f)
    });
    hits.truncate(q.limit.max(1));
    hits
}

#[cfg(test)]
mod tests;
