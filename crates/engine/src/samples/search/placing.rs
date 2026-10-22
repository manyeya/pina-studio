//! A sample described, and fitted into a sampler zone at the song's key.

use super::*;

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

/// A pitched drum's zone moved onto `key`: a kick to the key's root or fifth, a tom or perc to
/// its scale, by the nearer way and never far (a kick four semitones, the rest two). Says what it
/// did; leaves a drum without a clear note, and anything that isn't a drum zone, as it is.
pub fn tune_to(z: &mut Zone, s: &Sample, key: Key) -> Option<String> {
    let a = &s.analysis;
    let (Some(p), None) = (&a.pitch, z.high) else {
        return None;
    };
    let (targets, reach): (Vec<u8>, f32) = match a.category {
        Category::Kick => (vec![key.root, (key.root + 7) % 12], 4.0),
        Category::Tom | Category::Perc => ((0..12).filter(|&pc| key.contains(pc)).collect(), 2.0),
        _ => return None,
    };
    if a.tonal < 0.3 {
        return None;
    }
    let heard = p.note as f32 + p.cents / 100.0;
    let shift = (-12..=12)
        .map(|d| (heard.round() as i32 + d) as f32 - heard)
        .filter(|d| targets.contains(&(((heard + d).round() as i32).rem_euclid(12) as u8)))
        .min_by(|x, y| x.abs().total_cmp(&y.abs()))?;
    if shift.abs() > reach || shift.abs() < 0.1 {
        return None;
    }
    let whole = shift.round();
    z.root = Some((z.note as i32 - whole as i32).clamp(0, 127) as u8);
    z.tune_cents = ((shift - whole) * 100.0).round();
    let to = Key {
        root: ((heard + shift).round() as i32).rem_euclid(12) as u8,
        minor: false,
    }
    .name();
    let to = to.split_whitespace().next().unwrap_or("");
    Some(format!(
        "tuned {shift:+.1} semitones onto {to}, in {}",
        key.name()
    ))
}
