//! How alike two samples sound, and what goes with what.

use super::*;

pub(crate) fn cosine(a: &[f32], b: &[f32]) -> f32 {
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
pub(crate) fn goes_with(s: &Sample, other: &Sample) -> (f32, Vec<String>) {
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

/// A folder's first part: the pack, usually.
pub(crate) fn pack(folder: &str) -> &str {
    folder.split(['/', '\\']).next().unwrap_or("")
}

/// How good a one-shot is at being its kind of drum, 0 to 1, and what's off: a closed hat that
/// rings on, a kick with a long tail, a snare that booms, an 808 too short or toneless to play a
/// bassline. By how long it sounds (to 20 dB under its peak) and its character.
pub(crate) fn profile(a: &super::super::Analysis) -> Option<(f32, Option<&'static str>)> {
    if a.kind != Kind::OneShot {
        return None;
    }
    let has = |t: &str| a.traits.iter().any(|x| x == t);
    let d = a.envelope.decay_ms;
    let (mut q, mut why) = (1.0f32, None);
    // What it has that its kind wants: a kick's punch and sub, an 808's sub.
    let mut bonus = 1.0f32;
    let mut off = |by: f32, w: &'static str| {
        q *= by;
        why.get_or_insert(w);
    };
    match a.category {
        // A hat's crack is over at once; what tells a tight one from a washy one is its tail.
        Category::HatClosed => {
            let tail = a.envelope.tail_ms.max(d);
            match tail {
                t if t <= 300.0 => {}
                t if t <= 600.0 => off(0.8, "a little long for a closed hat"),
                _ => off(0.5, "a reverb tail: it rings on, long for a closed hat"),
            }
            if has("wet") {
                off(0.8, "reverb on it");
            }
            if has("sub") || has("boomy") {
                off(0.7, "low end in a hat");
            }
        }
        Category::HatOpen => match d {
            d if d < 120.0 => off(0.6, "short for an open hat"),
            d if d > 1500.0 => off(0.75, "rings very long"),
            _ => {}
        },
        Category::Kick => {
            match d {
                d if d <= 400.0 => {}
                d if d <= 700.0 => off(0.85, "a long tail for a kick"),
                _ => off(0.6, "a long tail for a kick"),
            }
            if has("noisy") {
                off(0.8, "noisy for a kick");
            }
            if has("wet") {
                off(0.85, "reverb on it");
            }
            bonus = 0.85 + 0.075 * has("punchy") as u8 as f32 + 0.075 * has("sub") as u8 as f32;
        }
        Category::Snare | Category::Clap => {
            match d {
                d if d <= 350.0 => {}
                d if d <= 700.0 => off(0.85, "a long tail"),
                _ => off(0.65, "a long tail"),
            }
            if has("boomy") || has("sub") {
                off(0.85, "boomy");
            }
        }
        Category::Rim | Category::Snap if d > 250.0 => off(0.75, "long for a rim or snap"),
        Category::Perc | Category::Shaker | Category::Tambourine if d > 600.0 => {
            off(0.8, "rings on")
        }
        Category::Bass808 => {
            if a.tonal < 0.4 || a.pitch.is_none() {
                off(0.6, "no clear note: it won't play a bassline in tune");
            }
            if d < 400.0 {
                off(0.65, "too short to hold a note");
            }
            bonus = if has("sub") { 1.0 } else { 0.85 };
        }
        _ => return None,
    }
    if a.loudness < -40.0 {
        off(0.75, "very quiet");
    }
    Some(((q * bonus).min(1.0), why))
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
