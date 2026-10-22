//! What a search says: categories and traits read from its words.

use super::*;

/// Words people use for categories, kinds and traits, and what they mean.
pub(crate) const CATEGORY_WORDS: &[(&str, &[Category])] = &[
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
pub(crate) const TRAIT_WORDS: &[(&str, &str)] = &[
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
pub(crate) struct Words {
    pub(crate) categories: Vec<Category>,
    pub(crate) kind: Option<Kind>,
    pub(crate) traits: Vec<String>,
    /// The word each trait was said by ("dusty" for lo-fi), which a name saying counts for.
    pub(crate) said: Vec<String>,
    pub(crate) key: Option<Key>,
    pub(crate) bpm: Option<f32>,
    pub(crate) rest: Vec<String>,
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

pub(crate) fn understand(text: &str) -> Words {
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

/// Whether a sample's name or folder says `w`, word by word ("b" is pack B, not every b):
/// "kick" finds Kick_03, and "jazzy", "funky", "orchestral" find Jazz, Funk and Orchestra.
pub(crate) fn mentions(s: &Sample, w: &str) -> bool {
    format!("{} {}", s.name, s.folder)
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .any(|t| t == w || (w.len() >= 3 && t.starts_with(w)) || (t.len() >= 4 && w.starts_with(t)))
}
