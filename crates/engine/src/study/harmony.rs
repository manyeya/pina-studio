//! The harmony: the key (the pitch classes the song dwells on, against Temperley's key
//! profiles), the chord on each beat (its pitch classes against each chord's, the bass weighing
//! for its root, smoothed by a hidden Markov model so a chord holds until the music moves), each
//! chord as a numeral in the key, and the loops of chords a stretch of the song goes round.

use serde::{Deserialize, Serialize};

use super::beats::cosine;
use crate::{
    music::Key,
    music::theory::{Chord, Quality},
};

/// Temperley's major and minor key profiles (from the Kostka-Payne corpus), tonic first.
const MAJOR: [f32; 12] = [
    0.748, 0.060, 0.488, 0.082, 0.670, 0.460, 0.096, 0.715, 0.104, 0.366, 0.057, 0.400,
];
const MINOR: [f32; 12] = [
    0.712, 0.084, 0.474, 0.618, 0.049, 0.460, 0.105, 0.747, 0.404, 0.067, 0.133, 0.330,
];

/// Every key, scored by how well the pitch classes fit it (Pearson's r against its profile) and
/// how much of the bass sits on its tonic (bass lines come home); best first, each with its r.
pub fn keys(chroma: &[f32; 12], bass: &[f32; 12]) -> Vec<(Key, f32, f32)> {
    let bass_sum: f32 = bass.iter().sum::<f32>().max(1e-9);
    let mut out = Vec::with_capacity(24);
    for tonic in 0..12u8 {
        for (profile, minor) in [(&MAJOR, false), (&MINOR, true)] {
            let rotated: [f32; 12] =
                std::array::from_fn(|i| profile[(i + 12 - tonic as usize) % 12]);
            let fit = crate::samples::tonal::correlate(chroma, &rotated);
            let home = bass[tonic as usize] / bass_sum;
            out.push((Key { root: tonic, minor }, fit + 0.6 * home, fit));
        }
    }
    out.sort_by(|a, b| b.1.total_cmp(&a.1));
    out
}

/// The key pitch classes fit best, and how well (Pearson's r, -1 to 1).
pub fn key_of(chroma: &[f32; 12], bass: &[f32; 12]) -> (Key, f32) {
    let k = keys(chroma, bass);
    (k[0].0, k[0].2)
}

/// How much a song makes `k`'s tonic chord home: the share of beats it holds (its root with the
/// other third, half), the share of four-bar phrases that start on it (a loop starts on its
/// home: "Am F C G" is A minor, "F G C Am" isn't), and starting and ending on it.
fn home(k: &Key, beats: &[Option<Chord>], phrases: &[usize]) -> f32 {
    let is = |c: &Chord| match (c.root == k.root, c.quality.minor() == k.minor) {
        (true, true) => 1.0,
        (true, false) => 0.5,
        _ => 0.0,
    };
    let chords: Vec<&Chord> = beats.iter().flatten().collect();
    if chords.is_empty() {
        return 0.0;
    }
    let held = chords.iter().map(|c| is(c)).sum::<f32>() / chords.len() as f32;
    let starts: Vec<f32> = phrases
        .iter()
        .filter_map(|&i| beats.get(i)?.as_ref())
        .map(is)
        .collect();
    let starts = if starts.is_empty() {
        0.0
    } else {
        starts.iter().sum::<f32>() / starts.len() as f32
    };
    held + 0.5 * starts + 0.08 * (is(chords[0]) + is(chords[chords.len() - 1]))
}

/// Of the keys the notes fit nearly as well (a key, its relative, the keys a fifth either side
/// share most of its notes), the one the chords make most home: the notes say the scale, the
/// chords say where home is. `phrases`: the beats four-bar phrases start on.
pub fn by_chords(
    scored: &[(Key, f32, f32)],
    beats: &[Option<Chord>],
    phrases: &[usize],
) -> (Key, f32) {
    let top = scored[0].1;
    let best = scored
        .iter()
        .take(4)
        .filter(|(_, score, _)| *score >= top - 0.25)
        .max_by(|a, b| {
            (a.1 + 1.2 * home(&a.0, beats, phrases))
                .total_cmp(&(b.1 + 1.2 * home(&b.0, beats, phrases)))
        })
        .map_or(scored[0].0, |k| k.0);
    // A key and its relative share every note: the chords alone say which.
    let other = match best.minor {
        true => Key {
            root: (best.root + 3) % 12,
            minor: false,
        },
        false => Key {
            root: (best.root + 9) % 12,
            minor: true,
        },
    };
    let key = if home(&other, beats, phrases) > home(&best, beats, phrases) {
        other
    } else {
        best
    };
    (key, scored.iter().find(|k| k.0 == key).map_or(0.0, |k| k.2))
}

/// A chord found on a stretch of beats.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ChordAt {
    pub start: f32,
    pub end: f32,
    /// Its symbol ("Am7"), or "N" where nothing's tonal.
    pub chord: String,
    /// As a numeral in the song's key ("vi7"), if it is a chord.
    pub numeral: String,
}

const QUALITIES: [Quality; 5] = [
    Quality::Maj,
    Quality::Min,
    Quality::Dom7,
    Quality::Maj7,
    Quality::Min7,
];

/// A chord's pitch classes as a template: its root strongest, its third and fifth, its seventh
/// lighter.
fn template(root: u8, q: Quality) -> [f32; 12] {
    let mut t = [0f32; 12];
    for (k, &iv) in q.intervals().iter().enumerate() {
        let w = match k {
            0 => 1.0,
            1 | 2 => 0.85,
            _ => 0.6,
        };
        t[(root as usize + iv as usize) % 12] += w;
    }
    t
}

/// The chord on each beat (`chroma` and `bass` are each beat's), as states: `None` for no chord.
pub fn chords(chroma: &[[f32; 12]], bass: &[[f32; 12]]) -> Vec<Option<Chord>> {
    let states: Vec<Chord> = (0..12u8)
        .flat_map(|root| {
            QUALITIES.iter().map(move |&q| Chord {
                root,
                quality: q,
                bass: None,
            })
        })
        .collect();
    let templates: Vec<[f32; 12]> = states.iter().map(|c| template(c.root, c.quality)).collect();
    let n = chroma.len();
    let s = states.len() + 1; // and no chord
    // How well each beat fits each chord: its pitch classes against the chord's, and its bass on
    // the chord's root. Sevenths need their seventh: a triad is preferred unless the seventh's
    // there.
    let fit = |i: usize, k: usize| -> f32 {
        if k == states.len() {
            return 0.35;
        }
        let c = &states[k];
        let total: f32 = chroma[i].iter().sum();
        if total <= 1e-6 {
            return 0.0;
        }
        let shape = cosine(&chroma[i], &templates[k]);
        let bsum: f32 = bass[i].iter().sum::<f32>().max(1e-9);
        let root = bass[i][c.root as usize] / bsum;
        let extra = if c.quality.intervals().len() > 3 {
            -0.04
        } else {
            0.0
        };
        shape + 0.35 * root + extra
    };
    // Viterbi: staying on a chord costs nothing, moving costs a little.
    let stay = 0.0f32;
    let mv = -0.18f32;
    let mut score = vec![0f32; s];
    let mut back = vec![vec![0u16; s]; n];
    for (k, v) in score.iter_mut().enumerate() {
        *v = fit(0, k);
    }
    for (i, from_here) in back.iter_mut().enumerate().skip(1) {
        let (best_k, best) = score
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(k, v)| (k, *v))
            .unwrap_or((0, 0.0));
        let mut next = vec![0f32; s];
        for k in 0..s {
            let (from, base) = if score[k] + stay >= best + mv {
                (k, score[k] + stay)
            } else {
                (best_k, best + mv)
            };
            next[k] = base + fit(i, k);
            from_here[k] = from as u16;
        }
        score = next;
    }
    let mut k = score
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map_or(0, |(k, _)| k);
    let mut out = vec![None; n];
    for i in (0..n).rev() {
        out[i] = states.get(k).copied();
        k = back[i][k] as usize;
    }
    out
}

/// A chord as a numeral in `key`: its degree (♭ and ♯ where it's off the scale), upper case for
/// major, lower for minor, and its seventh.
pub fn numeral(c: &Chord, key: &Key) -> String {
    let iv = (c.root + 12 - key.root) % 12;
    let names_major = [
        "I", "bII", "II", "bIII", "III", "IV", "#IV", "V", "bVI", "VI", "bVII", "VII",
    ];
    let names_minor = [
        "I", "bII", "II", "III", "#III", "IV", "#IV", "V", "VI", "#VI", "VII", "#VII",
    ];
    let base = if key.minor { names_minor } else { names_major }[iv as usize];
    let minor = c.quality.minor();
    let base = if minor {
        base.to_lowercase()
    } else {
        base.to_string()
    };
    let suffix = match c.quality {
        Quality::Dom7 => "7",
        Quality::Maj7 => "maj7",
        Quality::Min7 => "7",
        Quality::Dim => "°",
        Quality::HalfDim => "ø7",
        _ => "",
    };
    format!("{base}{suffix}")
}

/// The loop a run of bars goes round: the shortest of 1, 2, 4 or 8 bars that the bars repeat
/// at, as its chords, bar by bar (each bar's chord the one held longest in it).
pub fn loop_of(bars: &[String]) -> Vec<String> {
    if bars.is_empty() {
        return Vec::new();
    }
    let mut best = (bars.len().min(8), -1.0f32);
    for l in [1usize, 2, 4, 8] {
        if l > bars.len() {
            break;
        }
        let pairs = bars.len().saturating_sub(l);
        let same = (0..pairs).filter(|&i| bars[i] == bars[i + l]).count();
        // Longer loops explain more by chance: a short one that holds nearly as well wins.
        let held = if pairs == 0 {
            1.0
        } else {
            same as f32 / pairs as f32
        } - 0.04 * l as f32;
        if held > best.1 + 0.02 {
            best = (l, held);
        }
    }
    // The loop as its most common turn: each place's most frequent chord.
    (0..best.0)
        .map(|p| {
            let mut counts: Vec<(&String, usize)> = Vec::new();
            for b in bars.iter().skip(p).step_by(best.0) {
                match counts.iter_mut().find(|(c, _)| *c == b) {
                    Some(e) => e.1 += 1,
                    None => counts.push((b, 1)),
                }
            }
            counts
                .into_iter()
                .max_by_key(|e| e.1)
                .map(|e| e.0.clone())
                .unwrap_or_default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chroma(pcs: &[u8]) -> [f32; 12] {
        let mut c = [0.05f32; 12];
        for &p in pcs {
            c[p as usize] = 1.0;
        }
        c
    }

    #[test]
    fn keys_and_chords_are_read_from_pitch_classes() {
        // A minor's scale, its tonic in the bass.
        let (k, fit) = key_of(&chroma(&[9, 11, 0, 2, 4, 5, 7]), &chroma(&[9]));
        assert_eq!(k.name(), "A minor");
        assert!(fit > 0.5);
        // Am, Am, F, F, C, C, G, G: each read, and held across its beats.
        let beats: Vec<[f32; 12]> = [
            [9, 0, 4],
            [9, 0, 4],
            [5, 9, 0],
            [5, 9, 0],
            [0, 4, 7],
            [0, 4, 7],
            [7, 11, 2],
            [7, 11, 2],
        ]
        .iter()
        .map(|c| chroma(c))
        .collect();
        let bass: Vec<[f32; 12]> = [9, 9, 5, 5, 0, 0, 7, 7]
            .iter()
            .map(|&b| chroma(&[b]))
            .collect();
        let found: Vec<String> = chords(&beats, &bass)
            .iter()
            .map(|c| c.map_or("N".into(), |c| c.name()))
            .collect();
        assert_eq!(found, ["Am", "Am", "F", "F", "C", "C", "G", "G"]);
        let am = Key::parse("A minor").unwrap();
        assert_eq!(numeral(&Chord::parse("F").unwrap(), &am), "VI");
        assert_eq!(numeral(&Chord::parse("Dm7").unwrap(), &am), "iv7");
        assert_eq!(numeral(&Chord::parse("E7").unwrap(), &am), "V7");
    }

    #[test]
    fn loops_are_found_in_bars() {
        let bars: Vec<String> = [
            "Am", "F", "C", "G", "Am", "F", "C", "G", "Am", "F", "C", "G",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(loop_of(&bars), ["Am", "F", "C", "G"]);
        let two: Vec<String> = ["Dm", "G", "Dm", "G", "Dm", "G"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(loop_of(&two), ["Dm", "G"]);
    }
}
