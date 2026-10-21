//! Putting samples into a song: how long a loop lasts at the song's tempo, how far to move a
//! sample into its key, and where a one-shot goes on a sampler, kept off the notes its other
//! zones play. The browser and `pina samples put` both place samples with these.

use crate::{
    music::Key,
    samples::{Analysis, Category, Sample, search},
    song::project::{PPQ, Sampler},
};

/// A key and those that go with it: its relative, and its neighbours round the circle of fifths.
pub fn related(k: Key) -> [Key; 4] {
    let relative = match k.minor {
        true => Key {
            root: (k.root + 3) % 12,
            minor: false,
        },
        false => Key {
            root: (k.root + 9) % 12,
            minor: true,
        },
    };
    [
        k,
        relative,
        Key {
            root: (k.root + 7) % 12,
            ..k
        },
        Key {
            root: (k.root + 5) % 12,
            ..k
        },
    ]
}

/// Semitones to move a sample by to sit in `key`, the nearest way: a key onto it (or its
/// relative), a note onto its tonic and in tune. Drums and unpitched sounds stay.
pub fn transpose_to(a: &Analysis, key: Key) -> f32 {
    let nearest = |s: i32| {
        let s = s.rem_euclid(12);
        if s > 6 { s - 12 } else { s }
    };
    if a.category.is_drum() {
        return 0.0;
    }
    if let Some(k) = &a.key {
        let target = match (k.minor, key.minor) {
            (true, false) => (key.root + 9) % 12,
            (false, true) => (key.root + 3) % 12,
            _ => key.root,
        };
        return nearest(target as i32 - k.tonic as i32) as f32;
    }
    match &a.pitch {
        Some(p) => nearest(key.root as i32 - (p.note % 12) as i32) as f32 - p.cents / 100.0,
        None => 0.0,
    }
}

/// The tempo a loop at `bpm` is heard at in a song at `tempo`: its own, or halved or doubled
/// (half or double time) when that's much nearer. A 170 BPM loop is half time at 85, but one at
/// 120 slows to 84 with its bars still its bars: sped up to fit as half time instead, its chords
/// would come twice as fast.
pub fn feel(bpm: f64, tempo: f64) -> f64 {
    let far = |b: f64| (b / tempo).ln().abs();
    // ponytail: "much" is a quarter nearer, by ear; tune if half-time picks sound wrong.
    [bpm / 2.0, bpm * 2.0]
        .into_iter()
        .find(|&b| far(b) + 1.25f64.ln() < far(bpm))
        .unwrap_or(bpm)
}

/// How long a loop lasts in the song to play in time, in ticks: its beats at the song's tempo
/// (as `feel` hears them).
pub fn loop_ticks(a: &Analysis, tempo: f64) -> Option<u64> {
    let bpm = a.bpm.filter(|b| *b > 20.0)? as f64;
    let bpm = feel(bpm, tempo);
    let beats = a.seconds as f64 * bpm / 60.0;
    // A loop is whole beats: what's left over is the analysis rounding.
    let beats = match (beats - beats.round()).abs() < 0.1 {
        true => beats.round(),
        false => beats,
    };
    Some((beats * PPQ as f64).round().max(1.0) as u64)
}

/// The free notes a tonal zone can spread over: the run of them round `root` if it's an octave
/// or more, else the longest.
fn free_run(sampler: &Sampler, root: u8) -> Option<(u8, u8)> {
    let taken = |n: u16| sampler.zones.iter().any(|z| z.covers(n as u8));
    let mut runs = Vec::new();
    let mut start = None;
    for n in 0..=128u16 {
        match (n < 128 && !taken(n), start) {
            (true, None) => start = Some(n),
            (false, Some(s)) => {
                runs.push((s as u8, (n - 1) as u8));
                start = None;
            }
            _ => {}
        }
    }
    runs.iter()
        .find(|(lo, hi)| (*lo..=*hi).contains(&root) && hi - lo >= 11)
        .or_else(|| runs.iter().max_by_key(|(lo, hi)| hi - lo))
        .copied()
}

/// Puts a sample on `sampler` as the search would play it, kept off the notes its other zones
/// play: a drum on its General MIDI note (or `role`'s, in a kit) or the next free one up, a tonal
/// sound across the free notes round its own (in tune), anything else on the next free note from
/// middle C. A tonal zone a drum lands in makes room, keeping to its own side of it.
pub fn place_zone(sampler: &mut Sampler, s: &Sample, role: Option<Category>) -> Result<(), String> {
    place_zone_in(sampler, s, role, None)
}

/// As `place_zone`, a pitched drum (a kick, a tom, a perc with a note) tuned onto `key` if given.
pub fn place_zone_in(
    sampler: &mut Sampler,
    s: &Sample,
    role: Option<Category>,
    key: Option<Key>,
) -> Result<(), String> {
    let full = || "There's no free note left on that sampler.".to_string();
    let mut z = search::zone(s, None);
    if let Some(r) = role.filter(|r| *r != Category::Other) {
        z.name = r.name().to_string();
    }
    let tuned = key.is_some_and(|k| search::tune_to(&mut z, s, k).is_some());
    match z.high {
        Some(_) => {
            let (low, high) = free_run(sampler, z.root.unwrap_or(z.note)).ok_or_else(full)?;
            (z.note, z.high) = (low, Some(high));
        }
        None => {
            let from = role.and_then(Category::drum_note).unwrap_or(z.note);
            for r in sampler
                .zones
                .iter_mut()
                .filter(|r| r.high.is_some() && r.covers(from))
            {
                let root = r.root.unwrap_or(r.note);
                match root < from {
                    true => r.high = Some(from - 1),
                    false => r.note = from.saturating_add(1).min(r.high.unwrap_or(127)),
                }
            }
            let taken = |n: u8| sampler.zones.iter().any(|z| z.covers(n));
            let note = (from..=127)
                .chain((0..from).rev())
                .find(|&n| !taken(n))
                .ok_or_else(full)?;
            // Tuned on its first note: moved, its root moves with it.
            if tuned && let Some(root) = z.root {
                z.root = Some((root as i32 + note as i32 - z.note as i32).clamp(0, 127) as u8);
            }
            z.note = note;
        }
    }
    sampler.zones.push(z);
    Ok(())
}

#[cfg(test)]
mod tests;
