//! Writing with chords: the ones a key holds and those it borrows, progressions from the library,
//! each chord voiced (close, open, spread, or led from the last by the fewest steps) and played
//! (held, strummed, arpeggiated, pulsed, on the off-beats, in a rhythm, broken), a bass under it;
//! and the chords some notes play, read back.

use serde::{Deserialize, Serialize};

use super::{
    Key,
    theory::{Chord, PROGRESSIONS, Progression, name_notes, numeral},
};
use crate::song::project::Note;

/// The key's seven chords, by numeral: triads, or with `sevenths` its seventh chords.
pub fn in_key(key: Key, sevenths: bool) -> Vec<(String, Chord)> {
    let names: [&str; 7] = match (key.minor, sevenths) {
        (false, false) => ["I", "ii", "iii", "IV", "V", "vi", "vii°"],
        (false, true) => ["Imaj7", "ii7", "iii7", "IVmaj7", "V7", "vi7", "viiø"],
        (true, false) => ["i", "ii°", "III", "iv", "v", "VI", "VII"],
        (true, true) => ["i7", "iiø", "IIImaj7", "iv7", "v7", "VImaj7", "VII7"],
    };
    read(key, &names)
}

/// Chords borrowed from the parallel key, as producers reach for them: in major the minor iv,
/// the flat III, VI and VII; in minor the major IV and V, and the Neapolitan.
pub fn borrowed(key: Key) -> Vec<(String, Chord)> {
    match key.minor {
        false => read(key, &["iv", "bIII", "bVI", "bVII", "ii°", "v"]),
        true => read(key, &["IV", "V", "V7", "#vi°", "bII", "I"]),
    }
}

fn read(key: Key, names: &[&str]) -> Vec<(String, Chord)> {
    names
        .iter()
        .filter_map(|n| Some((n.to_string(), numeral(n, &key)?)))
        .collect()
}

/// A progression's chords in `key`, each with its share of a bar: "ii9 | V13 | Imaj9 V7" is a
/// bar each, then two halves.
pub fn progression(p: &Progression, key: Key) -> Vec<(Chord, f32)> {
    p.numerals
        .split('|')
        .flat_map(|bar| {
            let chords: Vec<Chord> = bar
                .split_whitespace()
                .filter_map(|n| numeral(n, &key))
                .collect();
            let share = 1.0 / chords.len().max(1) as f32;
            chords.into_iter().map(move |c| (c, share))
        })
        .collect()
}

/// The library's progressions for a key's mode, those suiting `mood` (any, with None).
pub fn progressions(minor: bool, mood: Option<&str>) -> Vec<&'static Progression> {
    PROGRESSIONS
        .iter()
        .filter(|p| p.minor == minor && mood.is_none_or(|m| p.suits.contains(&m)))
        .collect()
}

/// Every mood and genre the library's progressions suit, most used first.
pub fn moods() -> Vec<&'static str> {
    let mut seen: Vec<(&'static str, usize)> = Vec::new();
    for p in PROGRESSIONS {
        for &s in p.suits {
            match seen.iter_mut().find(|m| m.0 == s) {
                Some(m) => m.1 += 1,
                None => seen.push((s, 1)),
            }
        }
    }
    seen.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    seen.into_iter().map(|m| m.0).collect()
}

/// How a chord's notes are stacked.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Voicing {
    /// As close as they go, from the root up.
    Close,
    /// The second from the top dropped an octave: wider, as a pianist plays it.
    Open,
    /// The root low on its own, the rest close an octave and more above.
    Spread,
    /// Each chord's notes as near the last's as they go: the fewest steps between.
    #[default]
    Led,
}

impl Voicing {
    pub const ALL: [Voicing; 4] = [Voicing::Led, Voicing::Close, Voicing::Open, Voicing::Spread];

    pub fn name(self) -> &'static str {
        match self {
            Voicing::Close => "Close",
            Voicing::Open => "Open",
            Voicing::Spread => "Spread",
            Voicing::Led => "Voice-led",
        }
    }
}

/// A chord's pitches stacked close upward from the first of its notes at or above `low`.
fn close(c: &Chord, low: u8) -> Vec<u8> {
    let classes = c.classes();
    let root = low + (c.root + 12 - low % 12) % 12;
    let mut out = Vec::new();
    let mut at = root;
    for pc in classes {
        while at % 12 != pc {
            at += 1;
        }
        out.push(at);
    }
    out
}

/// Each chord voiced, around `middle` (the note its voicings sit about).
pub fn voice(chords: &[Chord], voicing: Voicing, middle: u8) -> Vec<Vec<u8>> {
    let low = middle.saturating_sub(6);
    let mut last: Option<Vec<u8>> = None;
    chords
        .iter()
        .map(|c| {
            let mut v = close(c, low);
            match voicing {
                Voicing::Close => {}
                Voicing::Open => {
                    if v.len() >= 3 {
                        let k = v.len() - 2;
                        v[k] = v[k].saturating_sub(12);
                        v.sort_unstable();
                    }
                }
                Voicing::Spread => v[0] = v[0].saturating_sub(12),
                Voicing::Led => {
                    // Of its inversions an octave either way, the nearest the last chord.
                    if let Some(prev) = &last {
                        let centre =
                            |v: &[u8]| v.iter().map(|&p| p as f32).sum::<f32>() / v.len() as f32;
                        let target = centre(prev);
                        let mut best = v.clone();
                        let mut inv = v.clone();
                        for _ in 0..v.len() * 2 {
                            // The lowest up an octave: the next inversion.
                            let lo = inv.remove(0);
                            inv.push(lo + 12);
                            for shift in [-12i16, 0] {
                                let moved: Vec<u8> = inv
                                    .iter()
                                    .map(|&p| (p as i16 + shift).clamp(0, 127) as u8)
                                    .collect();
                                if (centre(&moved) - target).abs() < (centre(&best) - target).abs()
                                {
                                    best = moved;
                                }
                            }
                        }
                        v = best;
                    }
                }
            }
            last = Some(v.clone());
            v
        })
        .collect()
}

/// How voiced chords are played over their time.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Performance {
    /// Held for all of its time.
    #[default]
    Block,
    /// Held, its notes a moment apart upward.
    Strum,
    /// Its notes one at a time upward, a sixteenth apart.
    Arp,
    /// Struck every eighth.
    Pulse,
    /// Short stabs between the beats.
    Offbeat,
    /// 3 hits in every 8 sixteenths.
    Rhythm,
    /// Root, then the rest, then the root again: a pianist's broken chord.
    Broken,
}

impl Performance {
    pub const ALL: [Performance; 7] = [
        Performance::Block,
        Performance::Strum,
        Performance::Arp,
        Performance::Pulse,
        Performance::Offbeat,
        Performance::Rhythm,
        Performance::Broken,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Performance::Block => "Held",
            Performance::Strum => "Strum",
            Performance::Arp => "Arp",
            Performance::Pulse => "Pulse",
            Performance::Offbeat => "Offbeat",
            Performance::Rhythm => "Rhythm",
            Performance::Broken => "Broken",
        }
    }
}

/// Voiced chords played one after another from 0, each for its `spans` ticks (`beat` ticks a
/// beat); with `bass`, its root two octaves under its voicing's lowest, held.
pub fn perform(
    voiced: &[Vec<u8>],
    spans: &[u64],
    style: Performance,
    beat: u64,
    bass: bool,
) -> Vec<Note> {
    let mut out = Vec::new();
    let mut at = 0i64;
    let sixteenth = (beat / 4).max(1);
    for (v, &span) in voiced.iter().zip(spans) {
        let note = |start: i64, length: u64, pitch: u8, velocity: u8| Note {
            start,
            length: length.max(1),
            pitch,
            velocity,
            ..Default::default()
        };
        let end = at + span as i64;
        let hits = |every: u64, length: u64, first: i64, velocity: u8, out: &mut Vec<Note>| {
            let mut t = at + first;
            while t < end {
                for &p in v {
                    out.push(note(t, length.min((end - t) as u64), p, velocity));
                }
                t += every as i64;
            }
        };
        match style {
            Performance::Block => hits(span, span, 0, 92, &mut out),
            Performance::Pulse => hits(beat / 2, beat / 2 - beat / 16, 0, 88, &mut out),
            Performance::Offbeat => hits(beat, beat / 4, (beat / 2) as i64, 96, &mut out),
            Performance::Strum => {
                for (k, &p) in v.iter().enumerate() {
                    let lag = (k as u64 * beat / 24) as i64;
                    out.push(note(at + lag, span - lag as u64, p, 90 - k as u8 * 4));
                }
            }
            Performance::Arp | Performance::Broken => {
                let order: Vec<u8> = match style {
                    Performance::Broken if v.len() > 1 => {
                        let mut o = vec![v[0]];
                        o.extend(&v[1..]);
                        o.push(v[0] + 12);
                        o
                    }
                    _ => v.clone(),
                };
                let mut t = at;
                let mut k = 0;
                while t < end {
                    out.push(note(
                        t,
                        sixteenth.min((end - t) as u64),
                        order[k % order.len()],
                        86,
                    ));
                    t += sixteenth as i64;
                    k += 1;
                }
            }
            Performance::Rhythm => {
                let pattern = super::generate::euclid(3, 8, 0);
                let mut t = at;
                let mut k = 0;
                while t < end {
                    if pattern[k % pattern.len()] {
                        for &p in v {
                            out.push(note(t, (sixteenth * 2).min((end - t) as u64), p, 94));
                        }
                    }
                    t += sixteenth as i64;
                    k += 1;
                }
            }
        }
        if bass && let Some(&lowest) = v.iter().min() {
            let root = v
                .iter()
                .copied()
                .find(|p| p % 12 == lowest % 12)
                .unwrap_or(lowest);
            out.push(note(at, span, root.saturating_sub(24), 100));
        }
        at = end;
    }
    out
}

/// The chords some notes play, a `step` (ticks) at a time from `from` to `to`, each named from
/// what sounds then (the lowest as its bass): runs of one chord made one, silence left out.
pub fn read_chords(notes: &[Note], (from, to): (i64, i64), step: u64) -> Vec<(i64, i64, Chord)> {
    let step = step.max(1) as i64;
    let mut out: Vec<(i64, i64, Chord)> = Vec::new();
    let mut t = from;
    while t < to {
        let end = (t + step).min(to);
        let sounding: Vec<&Note> = notes
            .iter()
            .filter(|n| n.start < end && n.start + n.length as i64 > t && !n.muted)
            .collect();
        let mut pcs: Vec<(u8, f32)> = Vec::new();
        for n in &sounding {
            let w = (n.start + n.length as i64).min(end) - n.start.max(t);
            match pcs.iter_mut().find(|p| p.0 == n.pitch % 12) {
                Some(p) => p.1 += w as f32,
                None => pcs.push((n.pitch % 12, w as f32)),
            }
        }
        let lowest = sounding.iter().map(|n| n.pitch).min().map(|p| p % 12);
        let named = (pcs.len() >= 2).then(|| name_notes(&pcs, lowest)).flatten();
        if let Some(c) = named {
            match out.last_mut() {
                Some(last) if last.2 == c && last.1 == t => last.1 = end,
                _ => out.push((t, end, c)),
            }
        }
        t = end;
    }
    out
}

#[cfg(test)]
mod tests;
