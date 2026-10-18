//! The groove: the kick, snare and hats on a 16-step grid (each bar's, and the bar it plays
//! most), its swing (where the off-beats really land), how syncopated it is, and the groove of
//! tempo's that's nearest.

use serde::{Deserialize, Serialize};

use super::features::{Frames, peaks_of};
use crate::theory::{self, Drum};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Groove {
    /// Kick, snare, hats: a character a step, as tempo's grooves are written (X accent, x hit,
    /// o soft, g ghost, . rest).
    pub kick: String,
    pub snare: String,
    pub hats: String,
    /// 50 straight, 66 a triplet feel; on the 8ths or the 16ths, whichever its hats play.
    pub swing: f32,
    pub swing_on: String,
    /// The share of kick and snare hits off the beat's 8ths: 0 square, 0.5 and up busy.
    pub syncopation: f32,
    /// The nearest of tempo's grooves, and how near (0 to 1).
    pub nearest: String,
    pub likeness: f32,
}

/// Each drum's strength on each 16th of each bar: the strongest start near the step.
fn grid(series: &[f32], f: &Frames, bars: &[f32], per_bar: usize, bar_len: f32) -> Vec<[f32; 16]> {
    let width = (f.onset_rate * bar_len / 16.0 * 0.4) as usize;
    let steps = (per_bar * 4).min(16);
    bars.iter()
        .map(|&b| {
            std::array::from_fn(|k| {
                if k >= steps {
                    return 0.0;
                }
                let t = b + bar_len * k as f32 / steps as f32;
                let i = f.at_onset(t);
                series[i.saturating_sub(width)..(i + width + 1).min(series.len())]
                    .iter()
                    .copied()
                    .fold(0.0, f32::max)
            })
        })
        .collect()
}

/// Strengths as characters, against the song's own strong hits.
fn chars(cells: &[f32; 16], top: f32) -> String {
    cells
        .iter()
        .map(|&v| {
            let r = if top > 0.0 { v / top } else { 0.0 };
            match r {
                r if r > 0.85 => 'X',
                r if r > 0.55 => 'x',
                r if r > 0.38 => 'o',
                r if r > 0.25 => 'g',
                _ => '.',
            }
        })
        .collect()
}

fn quantile(mut v: Vec<f32>, q: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f32::total_cmp);
    v[((v.len() - 1) as f32 * q) as usize]
}

/// The groove over the bars that have drums in them (`loud` bars, a mask).
pub fn groove(f: &Frames, beats: &[f32], bars: &[f32], per_bar: usize, loud: &[bool]) -> Groove {
    let bar_len = {
        let mut gaps: Vec<f32> = bars.windows(2).map(|w| w[1] - w[0]).collect();
        gaps.sort_by(f32::total_cmp);
        gaps.get(gaps.len() / 2).copied().unwrap_or(2.0)
    };
    let width = (f.onset_rate * 0.2) as usize;
    let (kick, snare, hats) = (
        peaks_of(&f.kick, width),
        peaks_of(&f.snare, width),
        peaks_of(&f.hats, width),
    );
    // A snare or clap is loud in the mids and less on top; a hat the other way round. Each
    // against its own strong hits, a start strong on top too is a hat's, not a snare's.
    let norm = |v: &[f32]| {
        let top = quantile(v.to_vec(), 0.97).max(1e-9);
        v.iter().map(|x| x / top).collect::<Vec<f32>>()
    };
    let (sn, hn) = (norm(&snare), norm(&hats));
    let snare: Vec<f32> = sn
        .iter()
        .zip(&hn)
        .map(|(s, h)| (s - 0.8 * h).max(0.0))
        .collect();
    let use_bar = |i: usize| loud.get(i).copied().unwrap_or(true);
    let pattern = |series: &[f32]| -> String {
        let cells = grid(series, f, bars, per_bar, bar_len);
        let all: Vec<f32> = cells
            .iter()
            .enumerate()
            .filter(|(i, _)| use_bar(*i))
            .flat_map(|(_, c)| c.iter().copied())
            .collect();
        let (floor, top) = (quantile(all.clone(), 0.25), quantile(all, 0.97));
        // Each step's typical strength: its median across the bars, over the drum's floor (what
        // any step picks up: a bass line in the kick's band, a reverb's tail), against its
        // strongest step's, so the steps it plays stand out from those it doesn't.
        let typical: [f32; 16] = std::array::from_fn(|k| {
            quantile(
                cells
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| use_bar(*i))
                    .map(|(_, c)| c[k])
                    .collect(),
                0.5,
            )
        });
        let over = typical.map(|v| (v - floor).max(0.0));
        let most = over.iter().copied().fold(0.0, f32::max);
        // A drum barely there (its strongest step near the floor) is drawn as rests.
        let scale = if top - floor > 1e-9 && most > 0.25 * (top - floor) {
            most
        } else {
            f32::MAX
        };
        chars(&over, scale)
    };
    let (k, s, h) = (pattern(&kick), pattern(&snare), pattern(&hats));
    // Swing: where the off-beat 8th (and 16th) really lands, from the hats, as a share of the
    // pair it splits.
    let lean = |series: &[f32], parts: f32| -> Option<f32> {
        let mut found = Vec::new();
        for w in beats.windows(2) {
            let beat = w[1] - w[0];
            for p in 0..(parts as usize / 2) {
                let pair = beat / (parts / 2.0);
                let start = w[0] + pair * p as f32;
                // The second of the pair, where it lands between 40% and 80% of the pair.
                let (a, b) = (
                    f.at_onset(start + pair * 0.4),
                    f.at_onset(start + pair * 0.8),
                );
                if b <= a {
                    continue;
                }
                let (i, v) = (a..b)
                    .map(|i| (i, series[i]))
                    .max_by(|x, y| x.1.total_cmp(&y.1))
                    .unwrap();
                if v > 0.0 {
                    let t = i as f32 / f.onset_rate;
                    found.push(((t - start) / pair).clamp(0.4, 0.8));
                }
            }
        }
        (found.len() >= 8).then(|| quantile(found, 0.5))
    };
    let busy16 = h.chars().skip(1).step_by(2).filter(|&c| c != '.').count() >= 4;
    let (swing, on) = match busy16 {
        true => (lean(&hats, 4.0), "16ths"),
        false => (lean(&hats, 2.0).or(lean(&snare, 2.0)), "8ths"),
    };
    let swing = swing.map_or(50.0, |r| (r * 100.0).clamp(50.0, 75.0));
    // Syncopation: kick and snare off the 8ths.
    let hits = |p: &str| {
        p.chars()
            .enumerate()
            .filter(|(_, c)| *c != '.' && *c != 'g')
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
    };
    let ks: Vec<usize> = hits(&k).into_iter().chain(hits(&s)).collect();
    let syncopation = if ks.is_empty() {
        0.0
    } else {
        ks.iter().filter(|&&i| i % 2 == 1).count() as f32 / ks.len() as f32
    };
    let (nearest, likeness) = nearest(&k, &s, &h);
    Groove {
        kick: k,
        snare: s,
        hats: h,
        swing,
        swing_on: on.into(),
        syncopation,
        nearest,
        likeness,
    }
}

/// The groove of tempo's whose kick, snare (or clap) and hats are most like these: hits shared
/// against hits in either.
fn nearest(kick: &str, snare: &str, hats: &str) -> (String, f32) {
    let on = |p: &str| -> Vec<bool> { p.chars().map(|c| c != '.' && c != 'g').collect() };
    let jaccard = |a: &[bool], b: &[bool]| -> f32 {
        let both = a.iter().zip(b).filter(|(x, y)| **x && **y).count() as f32;
        let either = a.iter().zip(b).filter(|(x, y)| **x || **y).count() as f32;
        if either == 0.0 { 1.0 } else { both / either }
    };
    let mine = [on(kick), on(snare), on(hats)];
    theory::GROOVES
        .iter()
        .map(|g| {
            let row = |drums: &[Drum]| -> Vec<bool> {
                let mut out = vec![false; 16];
                for (d, bars) in g.rows {
                    if drums.contains(d) {
                        for (i, c) in bars[0].chars().enumerate().take(16) {
                            out[i] |= c != '.' && c != 'g';
                        }
                    }
                }
                out
            };
            let theirs = [
                row(&[Drum::Kick]),
                row(&[Drum::Snare, Drum::Clap, Drum::Rim]),
                row(&[Drum::HatClosed, Drum::HatOpen, Drum::Shaker]),
            ];
            let score = 0.45 * jaccard(&mine[0], &theirs[0])
                + 0.35 * jaccard(&mine[1], &theirs[1])
                + 0.2 * jaccard(&mine[2], &theirs[2]);
            (g.id.to_string(), score)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or(("boom_bap".into(), 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grooves_are_named_by_their_nearest() {
        let (id, score) = nearest("X...X...X...X...", "....x.......x...", "..x...x...x...x.");
        assert_eq!(id, "house");
        assert!(score > 0.8);
        let (id, _) = nearest("x.....x...x.....", "........X.......", "x.x.x.x.x.x.x.x.");
        assert_eq!(id, "trap");
    }
}
