//! How well a sample fits a song: its key and its tempo.

use super::*;

/// Pearson correlation of two 12-long profiles.
pub(crate) fn correlate(a: &[f32; 12], b: &[f32; 12]) -> f32 {
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

pub(crate) fn rotate(chroma: &[f32; 12], semitones: i32) -> [f32; 12] {
    std::array::from_fn(|i| chroma[(i as i32 - semitones).rem_euclid(12) as usize])
}

/// What a sample is to a key: whether it has none to clash (drums, noise), plays any note (a
/// one-shot with one pitch, as a sampler plays it), or has notes that fit or don't.
pub(crate) enum Harmony {
    None,
    Tunable(u8),
    Notes([f32; 12]),
}

pub(crate) fn harmony(s: &Sample) -> Harmony {
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
pub(crate) fn key_fit(s: &Sample, key: Key) -> (f32, Option<i32>, String) {
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
                    match super::super::place::transpose_to(&s.analysis, key).round() as i32 {
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
pub(crate) fn bpm_fit(s: &Sample, bpm: f32) -> (f32, Option<String>) {
    let a = &s.analysis;
    if a.kind == Kind::OneShot {
        return (1.0, None);
    }
    let Some(own) = a.bpm else {
        return (0.5, Some("tempo unknown".into()));
    };
    // Heard as `put` places it: at its own tempo unless half or double time is much nearer.
    let factor = (super::super::place::feel(own as f64, bpm as f64) / own as f64) as f32;
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
