//! Putting samples into a song: how long a loop lasts at the song's tempo, how far to move a
//! sample into its key, and where a one-shot goes on a sampler, kept off the notes its other
//! zones play. The browser and `pina samples put` both place samples with these.

use crate::{
    music::Key,
    project::{PPQ, Sampler},
    samples::{Analysis, Category, Sample, search},
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
    let full = || "There's no free note left on that sampler.".to_string();
    let mut z = search::zone(s, None);
    if let Some(r) = role.filter(|r| *r != Category::Other) {
        z.name = r.name().to_string();
    }
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
            z.note = (from..=127)
                .chain((0..from).rev())
                .find(|&n| !taken(n))
                .ok_or_else(full)?;
        }
    }
    sampler.zones.push(z);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samples::{KeyGuess, Pitch};

    fn key(s: &str) -> Key {
        Key::parse(s).unwrap()
    }

    #[test]
    fn previews_move_into_a_key_the_nearest_way() {
        let note = |n: u8, cents: f32| Analysis {
            category: Category::Bass808,
            pitch: Some(Pitch {
                note: n,
                cents,
                hz: 0.0,
                confidence: 0.9,
            }),
            ..Default::default()
        };
        // G up to C is 5; F# down to C is 6 (not up 6... either way, within six).
        assert_eq!(transpose_to(&note(31, 0.0), key("C minor")), 5.0);
        assert_eq!(transpose_to(&note(42, 0.0), key("C minor")).abs(), 6.0);
        // A little sharp comes down a little.
        assert!((transpose_to(&note(36, 20.0), key("C major")) + 0.2).abs() < 1e-4);
        // A loop in E minor goes to A minor, or to C major's relative.
        let loop_in = |name: &str, tonic: u8, minor: bool| Analysis {
            category: Category::Keys,
            key: Some(KeyGuess {
                tonic,
                minor,
                name: name.into(),
                confidence: 0.9,
            }),
            ..Default::default()
        };
        assert_eq!(
            transpose_to(&loop_in("E minor", 4, true), key("A minor")),
            5.0
        );
        assert_eq!(
            transpose_to(&loop_in("E minor", 4, true), key("C major")),
            5.0
        );
        // Drums stay.
        let kick = Analysis {
            category: Category::Kick,
            ..note(36, 0.0)
        };
        assert_eq!(transpose_to(&kick, key("D minor")), 0.0);
    }

    #[test]
    fn loops_last_their_beats_at_the_songs_tempo() {
        let at = |bpm: f32, seconds: f32| Analysis {
            bpm: Some(bpm),
            seconds,
            ..Default::default()
        };
        // Four beats at 90: a bar at any tempo.
        assert_eq!(
            loop_ticks(&at(90.0, 4.0 * 60.0 / 90.0), 120.0),
            Some(PPQ * 4)
        );
        // Double time at 180 in a song at 90: still four of the song's beats... of 180's eight.
        assert_eq!(
            loop_ticks(&at(180.0, 8.0 * 60.0 / 180.0), 90.0),
            Some(PPQ * 4)
        );
        // Two bars at 120 in a song at 84: slowed, still two bars (not sped up as half time).
        assert_eq!(
            loop_ticks(&at(120.0, 8.0 * 60.0 / 120.0), 84.0),
            Some(PPQ * 8)
        );
        // At 140 in a song at 84: half time, its two bars one of the song's.
        assert_eq!(
            loop_ticks(&at(140.0, 8.0 * 60.0 / 140.0), 84.0),
            Some(PPQ * 4)
        );
        assert_eq!(loop_ticks(&Analysis::default(), 120.0), None);
    }

    #[test]
    fn related_keys_are_the_relative_and_the_neighbours_by_fifths() {
        let names: Vec<String> = related(key("A minor")).iter().map(|k| k.name()).collect();
        assert_eq!(names, ["A minor", "C major", "E minor", "D minor"]);
    }
}
