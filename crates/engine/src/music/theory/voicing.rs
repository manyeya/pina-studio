//! Voicing chords: laid out in a range, each moving as little as it can from the last.

use super::*;

/// A chord and when it plays, in ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub start: u64,
    pub end: u64,
    pub chord: Chord,
}

/// A progression written out: bars split by "|", each bar's chords sharing it equally; chords as
/// symbols or numerals. Repeated to fill `bars` bars from `start`.
pub fn lay_out(
    text: &str,
    key: &Key,
    start: u64,
    bars: u64,
    bar: u64,
) -> Result<Vec<Span>, String> {
    let groups: Vec<Vec<Chord>> = text
        .split('|')
        .map(|g| {
            g.split_whitespace()
                .map(|c| read_chord(c, key).ok_or(format!("{c:?} isn't a chord Pina Studio reads")))
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<_, _>>()?;
    let groups: Vec<Vec<Chord>> = groups.into_iter().filter(|g| !g.is_empty()).collect();
    if groups.is_empty() {
        return Err("no chords".into());
    }
    let mut out: Vec<Span> = Vec::new();
    for b in 0..bars {
        let g = &groups[b as usize % groups.len()];
        let each = bar / g.len() as u64;
        for (i, c) in g.iter().enumerate() {
            let s = start + b * bar + i as u64 * each;
            let e = if i + 1 == g.len() {
                start + (b + 1) * bar
            } else {
                s + each
            };
            match out.last_mut() {
                // The same chord again carries on.
                Some(last) if last.chord == *c && last.end == s => last.end = e,
                _ => out.push(Span {
                    start: s,
                    end: e,
                    chord: *c,
                }),
            }
        }
    }
    Ok(out)
}

/// How a chord is spread over the keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Voicing {
    /// Its notes close together.
    Close,
    /// Its root low, the rest close above: a pianist's left hand and right.
    #[default]
    Open,
    /// Without its root, for when a bass plays it.
    Rootless,
    /// Root, third and fifth only.
    Triad,
    /// Spread over two octaves, for pads and strings.
    Spread,
}

/// Every way to place `classes` (in order, lowest first) as pitches within `lo..=hi`.
pub(crate) fn placements(classes: &[u8], lo: u8, hi: u8) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for r in 0..classes.len() {
        let order: Vec<u8> = classes[r..].iter().chain(&classes[..r]).copied().collect();
        for octave in 0..11u8 {
            let mut v = Vec::new();
            let mut at = octave as i32 * 12 + order[0] as i32;
            v.push(at);
            for &pc in &order[1..] {
                let mut p = (at / 12) * 12 + pc as i32;
                while p <= at {
                    p += 12;
                }
                v.push(p);
                at = p;
            }
            if v.iter().all(|&p| p >= lo as i32 && p <= hi as i32) {
                out.push(v.into_iter().map(|p| p as u8).collect());
            }
        }
    }
    out
}

/// How far one voicing moves to another: each voice's step, by their places.
pub(crate) fn movement(a: &[u8], b: &[u8]) -> u32 {
    let n = a.len().min(b.len());
    let mut cost: u32 = (0..n)
        .map(|i| (a[i] as i32 - b[i] as i32).unsigned_abs())
        .sum();
    cost += (a.len().abs_diff(b.len()) as u32) * 6;
    cost
}

/// A chord voiced in `register` (lowest and highest pitch for its upper notes), led from `prev`
/// by the smallest moves; with none before, centred.
pub fn voice(chord: &Chord, prev: Option<&[u8]>, voicing: Voicing, register: (u8, u8)) -> Vec<u8> {
    let iv = chord.quality.intervals();
    let pcs = |ivs: &[u8]| -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        for i in ivs {
            let pc = (chord.root + i) % 12;
            if !out.contains(&pc) {
                out.push(pc);
            }
        }
        out
    };
    // The upper notes: the colours first; the fifth goes when there are many.
    let mut upper: Vec<u8> = match voicing {
        Voicing::Triad => pcs(&iv[..3.min(iv.len())]),
        _ => pcs(&iv[1..]),
    };
    if matches!(voicing, Voicing::Triad | Voicing::Close) {
        upper = pcs(iv);
        if voicing == Voicing::Triad {
            upper = pcs(&iv[..3.min(iv.len())]);
        }
    }
    let fifth = (chord.root + 7) % 12;
    if upper.len() > 4 && upper.contains(&fifth) {
        upper.retain(|&p| p != fifth);
    }
    upper.truncate(5);
    let (lo, hi) = register;
    let centre = (lo as i32 + hi as i32) / 2;
    let candidates = placements(&upper, lo, hi);
    let previous_upper: Option<Vec<u8>> = prev.map(|p| {
        let mut p = p.to_vec();
        p.sort_unstable();
        // The upper notes of what came before (its low root left out).
        if p.len() > upper.len() {
            p.split_off(p.len() - upper.len())
        } else {
            p
        }
    });
    let pick = candidates
        .into_iter()
        .min_by_key(|v| match &previous_upper {
            Some(p) => {
                movement(v, p) * 4
                    + (v.iter().map(|&x| x as i32).sum::<i32>() / v.len() as i32 - centre)
                        .unsigned_abs()
            }
            None => {
                (v.iter().map(|&x| x as i32).sum::<i32>() / v.len() as i32 - centre).unsigned_abs()
                    * 4
            }
        })
        .unwrap_or_else(|| {
            upper
                .iter()
                .map(|&pc| lo + (pc + 12 - lo % 12) % 12)
                .collect()
        });
    let mut notes = pick;
    match voicing {
        Voicing::Open => {
            // The root (or slash bass) an octave or so under, in the left hand's place.
            let bass = chord.bass.unwrap_or(chord.root);
            let low = lo.saturating_sub(12).max(36);
            let mut b = low - low % 12 + bass;
            if b < low {
                b += 12;
            }
            while b + 3 > notes[0] {
                b -= 12;
            }
            notes.insert(0, b);
        }
        Voicing::Spread => {
            // Root low, fifth, then the rest an octave up.
            let low = lo.saturating_sub(12).max(36);
            let mut root = low - low % 12 + chord.root;
            if root < low {
                root += 12;
            }
            let mut spread = vec![root, root + 7];
            spread.extend(
                notes
                    .iter()
                    .map(|&n| if n < root + 12 { n + 12 } else { n }),
            );
            spread.sort_unstable();
            spread.dedup();
            notes = spread.into_iter().filter(|&n| n <= 108).collect();
        }
        _ => {}
    }
    notes
}
