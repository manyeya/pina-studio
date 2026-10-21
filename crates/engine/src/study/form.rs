//! The song's form: where it changes (Foote's novelty: a checkerboard kernel slid along the bars'
//! self-similarity, peaking where what's before stops being like what's after), which of its
//! sections are the same section again, and what each one is: intro, verse, pre-chorus, chorus,
//! bridge, outro, or a break.

use serde::{Deserialize, Serialize};

use super::beats::cosine;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Part {
    pub name: String,
    /// The same letter for the same section: A, B, A, B, C, B.
    pub label: char,
    /// Bars, from 0.
    pub from: usize,
    pub to: usize,
    /// How hard it plays against the rest of the song: 0 to 1.
    pub energy: f32,
}

/// Each bar's description: its pitch classes, its timbre, its loudness.
pub struct Bar {
    pub chroma: [f32; 12],
    pub timbre: Vec<f32>,
    pub level: f32,
}

/// How alike every two bars are.
fn similarity(bars: &[Bar]) -> Vec<Vec<f32>> {
    let n = bars.len();
    // Timbre, each coefficient standardized across the song so none drowns the others.
    let dims = bars.first().map_or(0, |b| b.timbre.len());
    let mut mean = vec![0f32; dims];
    let mut sd = vec![0f32; dims];
    for b in bars {
        for (m, t) in mean.iter_mut().zip(&b.timbre) {
            *m += t / n as f32;
        }
    }
    for b in bars {
        for ((v, m), t) in sd.iter_mut().zip(&mean).zip(&b.timbre) {
            *v += (t - m).powi(2) / n as f32;
        }
    }
    let z: Vec<Vec<f32>> = bars
        .iter()
        .map(|b| {
            (0..dims)
                .map(|d| (b.timbre[d] - mean[d]) / sd[d].sqrt().max(1e-6))
                .collect()
        })
        .collect();
    let level_spread = {
        let m = bars.iter().map(|b| b.level).sum::<f32>() / n.max(1) as f32;
        (bars.iter().map(|b| (b.level - m).powi(2)).sum::<f32>() / n.max(1) as f32)
            .sqrt()
            .max(1.0)
    };
    (0..n)
        .map(|i| {
            (0..n)
                .map(|j| {
                    // Harmony as a run of three bars, so the order chords come in counts:
                    // "Am then F" isn't "F then G", though both have an F.
                    let harmony = (-1isize..=1)
                        .map(|d| {
                            let (a, b) = (i as isize + d, j as isize + d);
                            if a < 0 || b < 0 || a >= n as isize || b >= n as isize {
                                cosine(&bars[i].chroma, &bars[j].chroma)
                            } else {
                                cosine(&bars[a as usize].chroma, &bars[b as usize].chroma)
                            }
                        })
                        .sum::<f32>()
                        / 3.0;
                    let timbre = 0.5 + 0.5 * cosine(&z[i], &z[j]);
                    let loud =
                        (-(bars[i].level - bars[j].level).abs() / (2.0 * level_spread)).exp();
                    0.45 * harmony + 0.35 * timbre + 0.2 * loud
                })
                .collect()
        })
        .collect()
}

/// Novelty at each bar line: a Gaussian-tapered checkerboard kernel, `k` bars on each side,
/// summed along the diagonal (alike on each side of the line counts for, alike across it,
/// against).
fn novelty(s: &[Vec<f32>], k: usize) -> Vec<f32> {
    let n = s.len() as isize;
    let k = k as isize;
    (0..n)
        .map(|i| {
            let (mut v, mut w) = (0.0f32, 0.0f32);
            for a in -k..k {
                for b in -k..k {
                    let (x, y) = (i + a, i + b);
                    if x < 0 || y < 0 || x >= n || y >= n {
                        continue;
                    }
                    let (fa, fb) = (a as f32 + 0.5, b as f32 + 0.5);
                    let g = (-(fa * fa + fb * fb) / (k * k) as f32).exp();
                    let sign = if (a < 0) == (b < 0) { 1.0 } else { -1.0 };
                    v += sign * g * s[x as usize][y as usize];
                    w += g;
                }
            }
            if w > 0.0 { v / w } else { 0.0 }
        })
        .collect()
}

/// Where the sections start, in bars: novelty's peaks, at least `min` bars apart, near a
/// multiple of four bars from the first where there's one close.
fn boundaries(nov: &[f32], min: usize) -> Vec<usize> {
    let n = nov.len();
    if n == 0 {
        return vec![0];
    }
    // A change is the strongest within three bars each way, and well over the song's typical
    // (its median, taken from inside it: its edges always look new). A median, so one dramatic
    // change (an intro dropping out) doesn't hide the subtler ones (a verse into a chorus).
    let mut inner: Vec<f32> = nov
        .iter()
        .copied()
        .skip(2)
        .take(n.saturating_sub(4))
        .collect();
    inner.sort_by(f32::total_cmp);
    let typical = inner.get(inner.len() / 2).copied().unwrap_or(0.0);
    let mut peaks: Vec<(usize, f32)> = (1..n)
        .filter(|&i| {
            // The first and last bars always look new: they're not compared with.
            let (a, b) = (
                i.saturating_sub(3).max(2.min(i)),
                (i + 4).min(n.saturating_sub(2)).max(i + 1),
            );
            nov[a..b].iter().all(|&v| v <= nov[i]) && nov[i] > 1.5 * typical
        })
        .map(|i| (i, nov[i]))
        .collect();
    // The strongest first, each keeping clear of those already kept.
    peaks.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut kept: Vec<usize> = vec![0];
    for (i, _) in peaks {
        // Phrases come in fours: a change between phrase lines is taken to the nearer line
        // where the song changes more (a section often shares its first bars with the last).
        let i = match i % 4 {
            0 => i,
            r => {
                let (down, up) = (i - r, i - r + 4);
                match (down > 0, up < n) {
                    (true, true) if r == 1 || (r == 2 && nov[down] >= nov[up]) => down,
                    (_, true) => up,
                    (true, false) => down,
                    _ => i,
                }
            }
        };
        if kept.iter().all(|&k| k.abs_diff(i) >= min) && i + min / 2 <= n {
            kept.push(i);
        }
    }
    kept.sort_unstable();
    // Sections run 8 to 16 bars, rarely past 24: one longer is split at its phrase line (eight
    // bars clear of its ends) where the song changes most, while that's over the typical.
    loop {
        let split = kept.iter().enumerate().find_map(|(k, &a)| {
            let b = kept.get(k + 1).copied().unwrap_or(n);
            (b - a > 24)
                .then(|| {
                    (a + 8..=b - 8)
                        .filter(|i| i % 4 == 0)
                        .max_by(|&x, &y| nov[x].total_cmp(&nov[y]))
                })
                .flatten()
                .filter(|&i| nov[i] > typical)
        });
        match split {
            Some(i) => {
                kept.push(i);
                kept.sort_unstable();
            }
            None => break kept,
        }
    }
}

/// The song's sections, from its bars.
pub fn form(bars: &[Bar]) -> Vec<Part> {
    let n = bars.len();
    if n == 0 {
        return Vec::new();
    }
    if n < 8 {
        return vec![Part {
            name: "Loop".into(),
            label: 'A',
            from: 0,
            to: n,
            energy: 1.0,
        }];
    }
    let s = similarity(bars);
    // Two widths: eight bars for the song's turns (wider than a chord loop, so the loop's own
    // changes average out), four for short sections (a 4-bar intro).
    let (wide, narrow) = (novelty(&s, 8), novelty(&s, 4));
    let nov: Vec<f32> = wide.iter().zip(&narrow).map(|(a, b)| a + 0.5 * b).collect();
    let starts = boundaries(&nov, 4);
    let spans: Vec<(usize, usize)> = starts
        .iter()
        .enumerate()
        .map(|(i, &a)| (a, starts.get(i + 1).copied().unwrap_or(n)))
        .collect();
    // How alike two sections are: their bars against each other's, lined up from their starts
    // (a section repeated runs the same way), else on average.
    let alike = |a: (usize, usize), b: (usize, usize)| -> f32 {
        let len = (a.1 - a.0).min(b.1 - b.0);
        let lined = (0..len).map(|k| s[a.0 + k][b.0 + k]).sum::<f32>() / len.max(1) as f32;
        let all: f32 = s[a.0..a.1]
            .iter()
            .map(|row| row[b.0..b.1].iter().sum::<f32>())
            .sum();
        let all = all / ((a.1 - a.0) * (b.1 - b.0)).max(1) as f32;
        lined.max(all)
    };
    // Labels: a section like one before it takes its letter. Like, against the song's own
    // spread: how alike two of its bars typically are (the median), and how alike its most alike
    // bars are (the top 3%, a part played again). A song played on the same instruments
    // throughout is alike everywhere; its sections' differences are what's over that.
    let (typical, close) = {
        let mut all: Vec<f32> = s
            .iter()
            .enumerate()
            .flat_map(|(i, row)| {
                row.iter()
                    .enumerate()
                    .filter(move |(j, _)| *j != i)
                    .map(|(_, v)| *v)
            })
            .collect();
        all.sort_by(f32::total_cmp);
        let q = |p: f32| {
            all.get(((all.len().max(1) - 1) as f32 * p) as usize)
                .copied()
                .unwrap_or(0.0)
        };
        (q(0.5), q(0.97))
    };
    let mut labels: Vec<char> = Vec::new();
    let mut next = b'A';
    for (i, &a) in spans.iter().enumerate() {
        let best = (0..i)
            .map(|j| {
                (
                    j,
                    (alike(a, spans[j]) - typical) / (close - typical).max(1e-3),
                )
            })
            .max_by(|x, y| x.1.total_cmp(&y.1));
        match best {
            Some((j, r)) if r > 0.75 => labels.push(labels[j]),
            _ => {
                labels.push(next as char);
                next = next.saturating_add(1);
            }
        }
    }
    // Neighbours that turn out to be the same section are one.
    let mut merged: Vec<((usize, usize), char)> = Vec::new();
    for (&span, &l) in spans.iter().zip(&labels) {
        match merged.last_mut() {
            Some((last, ll)) if *ll == l => last.1 = span.1,
            _ => merged.push((span, l)),
        }
    }
    let spans: Vec<(usize, usize)> = merged.iter().map(|m| m.0).collect();
    let labels: Vec<char> = merged.iter().map(|m| m.1).collect();
    // Energy: loudness, against the song's loudest and quietest sections.
    let level: Vec<f32> = spans
        .iter()
        .map(|&(a, b)| bars[a..b].iter().map(|x| x.level).sum::<f32>() / (b - a) as f32)
        .collect();
    let (lo, hi) = level
        .iter()
        .fold((f32::MAX, f32::MIN), |(l, h), &v| (l.min(v), h.max(v)));
    let energy: Vec<f32> = level
        .iter()
        .map(|&v| if hi > lo { (v - lo) / (hi - lo) } else { 1.0 })
        .collect();
    let names = name(&spans, &labels, &energy, n);
    spans
        .iter()
        .zip(labels)
        .zip(names)
        .zip(energy)
        .map(|((((from, to), label), name), energy)| Part {
            name,
            label,
            from: *from,
            to: *to,
            energy,
        })
        .collect()
}

/// What each section is, by its letter: the chorus is the loudest letter that comes back; the
/// verse, the other letter that comes back most; what leads straight into each chorus and is
/// short, the pre-chorus; a letter heard once, late, between choruses, the bridge; a quiet
/// first, the intro; a quiet or new last, the outro.
pub fn name(spans: &[(usize, usize)], labels: &[char], energy: &[f32], bars: usize) -> Vec<String> {
    name_with(spans, labels, energy, bars, None)
}

/// `name`, with the chorus's letter known (from the lyrics a song repeats).
pub fn name_with(
    spans: &[(usize, usize)],
    labels: &[char],
    energy: &[f32],
    bars: usize,
    chorus_is: Option<char>,
) -> Vec<String> {
    let n = spans.len();
    let mut letters: Vec<char> = labels.to_vec();
    letters.sort_unstable();
    letters.dedup();
    let count = |l: char| labels.iter().filter(|&&x| x == l).count();
    let mean_energy = |l: char| {
        let e: Vec<f32> = (0..n)
            .filter(|&i| labels[i] == l)
            .map(|i| energy[i])
            .collect();
        e.iter().sum::<f32>() / e.len().max(1) as f32
    };
    let repeated: Vec<char> = letters.iter().copied().filter(|&l| count(l) >= 2).collect();
    let chorus = chorus_is.or_else(|| {
        repeated.iter().copied().max_by(|&a, &b| {
            (mean_energy(a) + 0.05 * count(a) as f32)
                .total_cmp(&(mean_energy(b) + 0.05 * count(b) as f32))
        })
    });
    // A letter heard only at the song's edges (an intro come back as its outro) isn't a verse.
    let inside = |l: char| (0..n).any(|i| labels[i] == l && i > 0 && i + 1 < n);
    let bars_of = |l: char| {
        (0..n)
            .filter(|&i| labels[i] == l)
            .map(|i| spans[i].1 - spans[i].0)
            .sum::<usize>()
    };
    let verse = repeated
        .iter()
        .copied()
        .filter(|&l| Some(l) != chorus && inside(l))
        .max_by(|&a, &b| (count(a), bars_of(a)).cmp(&(count(b), bars_of(b))));
    let mut names: Vec<String> = vec![String::new(); n];
    // Nothing comes back (a loop that builds and drops, as dance music does): the sections are
    // named by their energy instead.
    if chorus.is_none() {
        // Only where the middle has highs and lows: a song as loud all through is just its
        // sections, by letter.
        let middle = &energy[1.min(n)..n.saturating_sub(1).max(1.min(n))];
        let (lo, hi) = middle
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, h), &e| (l.min(e), h.max(e)));
        if hi - lo < 0.3 {
            for (i, name) in names.iter_mut().enumerate() {
                *name = match i {
                    0 if n > 1 => "Intro".into(),
                    i if i + 1 == n && n > 1 => "Outro".into(),
                    i => format!("Section {}", labels[i]),
                };
            }
            return numbered(names);
        }
        for i in 0..n {
            let rises_into = |j: usize| {
                energy
                    .get(j)
                    .is_some_and(|&e| e >= 0.7 && e > energy[i] + 0.15)
            };
            names[i] = match i {
                0 if n > 1 => "Intro".into(),
                i if i + 1 == n && n > 1 => "Outro".into(),
                i if energy[i] >= 0.7 => "Drop".into(),
                i if energy[i] < 0.35 => "Breakdown".into(),
                i if rises_into(i + 1) => "Build".into(),
                _ => "Groove".into(),
            };
        }
        return numbered(names);
    }
    let first_chorus = (0..n).find(|&i| Some(labels[i]) == chorus).unwrap_or(n);
    let last_chorus = (0..n)
        .rev()
        .find(|&i| Some(labels[i]) == chorus)
        .unwrap_or(0);
    // One bridge a song: a second section heard once is an interlude, or a break if quiet.
    let mut bridged = false;
    for i in 0..n {
        let l = labels[i];
        let len = spans[i].1 - spans[i].0;
        let once = count(l) == 1;
        let leads_into_chorus = chorus.is_some() && labels.get(i + 1).copied() == chorus;
        let main = Some(l) == chorus || Some(l) == verse;
        names[i] = if Some(l) == chorus {
            "Chorus".into()
        } else if Some(l) == verse {
            "Verse".into()
        } else if i == 0 && !main {
            "Intro".into()
        } else if i + 1 == n && !main {
            "Outro".into()
        } else if once && !bridged && i > first_chorus && i < last_chorus {
            // Heard once, between choruses: the bridge.
            bridged = true;
            "Bridge".into()
        } else if leads_into_chorus && len <= 8 {
            "Pre-chorus".into()
        } else if once && !bridged && spans[i].0 > bars / 2 {
            bridged = true;
            "Bridge".into()
        } else if energy[i] < 0.35 {
            "Break".into()
        } else if once {
            "Interlude".into()
        } else {
            format!("Section {l}")
        };
    }
    // A pre-chorus is its letter everywhere it leads into a chorus.
    let pre: Vec<char> = (0..n)
        .filter(|&i| names[i] == "Pre-chorus")
        .map(|i| labels[i])
        .collect();
    for i in 0..n {
        if pre.contains(&labels[i]) && names[i].starts_with("Section") {
            names[i] = "Pre-chorus".into();
        }
    }
    numbered(names)
}

/// Names numbered when they come back: Verse 2, Chorus 3.
fn numbered(mut names: Vec<String>) -> Vec<String> {
    let mut seen: Vec<(String, usize)> = Vec::new();
    for name in names.iter_mut() {
        let base = name.clone();
        let k = match seen.iter_mut().find(|(b, _)| *b == base) {
            Some(e) => {
                e.1 += 1;
                e.1
            }
            None => {
                seen.push((base.clone(), 1));
                1
            }
        };
        if k > 1 {
            *name = format!("{base} {k}");
        }
    }
    names
}

#[cfg(test)]
mod tests;
