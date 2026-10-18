//! The beat: its tempo (from how the starts in the song repeat), each beat (followed through the
//! song as Ellis's beat tracker does: as strong a start as there is near where the next beat
//! should be), the bar's length, and where its downbeats are.

use super::features::{Frames, peaks_of};

pub struct Beats {
    pub bpm: f32,
    /// How clearly the song keeps that tempo: 0 to 1.
    pub confidence: f32,
    /// Each beat's time, seconds.
    pub beats: Vec<f32>,
    /// Beats to a bar.
    pub per_bar: usize,
    /// The beat (an index into `beats`) the first full bar starts on.
    pub first: usize,
}

impl Beats {
    /// Each bar's start, seconds.
    pub fn bars(&self) -> Vec<f32> {
        self.beats
            .iter()
            .skip(self.first)
            .step_by(self.per_bar.max(1))
            .copied()
            .collect()
    }
}

/// The tempo the starts repeat at most strongly, as a beat's length in frames, and how clearly.
/// Tempos near 105 BPM are preferred, as listeners do, so a song isn't read at half or double
/// its tempo; and the bar's repetition (four beats) and the eighths count, so a swung song's
/// dotted-eighth pulse (4/3 of the beat) isn't taken for the beat. (Weights measured on 1,200
/// labelled loops with `examples/study_eval`: 87% exact, 91% within an octave.)
fn period(onset: &[f32], rate: f32) -> (f32, f32) {
    let lag = |bpm: f32| 60.0 * rate / bpm;
    let (lo, hi) = (lag(220.0) as usize, lag(50.0) as usize);
    let n = onset.len();
    let energy: f32 = onset.iter().map(|v| v * v).sum::<f32>().max(1e-9);
    let acf = |l: usize| -> f32 {
        (0..n.saturating_sub(l))
            .map(|i| onset[i] * onset[i + l])
            .sum::<f32>()
            / energy
    };
    // At a multiple of a lag, within the rounding a whole number of frames makes there: the
    // peaks are a frame or two wide, and `k` times a rounded lag is up to `k` halves off.
    let at = |l: usize, k: f32| -> f32 {
        let centre = l as f32 * k;
        let w = (k * 0.5).ceil() as isize;
        (-w..=w)
            .map(|d| acf((centre.round() as isize + d).max(1) as usize))
            .fold(f32::MIN, f32::max)
    };
    let mut best = (lo.max(1), f32::MIN, 0.0);
    for l in lo.max(1)..=hi.min(n.saturating_sub(1)) {
        let bpm = 60.0 * rate / l as f32;
        // Its own lag, two beats, the bar (four), and the eighths half a beat splits it into: a
        // beat's pattern repeats at all of them, which a swung song's dotted-eighth pulse (4/3
        // of a beat) or a 1.5-beat syncopation (trap's dotted kicks) doesn't.
        let raw =
            acf(l) + 0.5 * at(l, 2.0).max(0.0) + at(l, 4.0).max(0.0) + 0.9 * at(l, 0.5).max(0.0);
        let octaves = (bpm / 105.0).log2();
        let prior = (-0.5 * (octaves / 0.75).powi(2)).exp();
        if raw * prior > best.1 {
            best = (l, raw * prior, raw);
        }
    }
    // Between frames: the peak of a parabola through it and its neighbours.
    let l = best.0;
    let (a, b, c) = (acf(l.saturating_sub(1)), acf(l), acf(l + 1));
    let shift = if a - 2.0 * b + c != 0.0 {
        0.5 * (a - c) / (a - 2.0 * b + c)
    } else {
        0.0
    };
    (l as f32 + shift.clamp(-0.5, 0.5), best.2.clamp(0.0, 1.0))
}

/// Each beat, as a frame: the path through the song with the strongest starts on it whose steps
/// stay near `period`.
fn track(onset: &[f32], period: f32) -> Vec<usize> {
    let n = onset.len();
    if n == 0 {
        return Vec::new();
    }
    let tight = 100.0;
    let mut score = vec![0f32; n];
    let mut from = vec![usize::MAX; n];
    let (lo, hi) = (
        (period * 0.5).round() as usize,
        (period * 2.0).round() as usize,
    );
    for t in 0..n {
        let mut best = (0.0f32, usize::MAX);
        for back in lo.max(1)..=hi {
            if back > t {
                break;
            }
            let p = t - back;
            let step = (back as f32 / period).ln();
            let s = score[p] - tight * step * step;
            if best.1 == usize::MAX || s > best.0 {
                best = (s, p);
            }
        }
        score[t] = onset[t] + if best.1 == usize::MAX { 0.0 } else { best.0 };
        from[t] = best.1;
    }
    // The end: the best-scoring frame in the last beat.
    let tail = (n as f32 - period).max(0.0) as usize;
    let mut t = (tail..n)
        .max_by(|&a, &b| score[a].total_cmp(&score[b]))
        .unwrap_or(n - 1);
    let mut out = vec![t];
    while from[t] != usize::MAX {
        t = from[t];
        out.push(t);
    }
    out.reverse();
    out
}

pub fn beats(f: &Frames) -> Beats {
    let onset = peaks_of(&f.onset, (f.onset_rate * 0.4) as usize);
    let (p, confidence) = period(&onset, f.onset_rate);
    let frames = track(&onset, p);
    let mut beats: Vec<f32> = frames.iter().map(|&t| t as f32 / f.onset_rate).collect();
    // Off-beat hats (house, disco) can pull the beat half a beat late: kicks land on the beat in
    // nearly every song, so if they're stronger between the beats found, the beats move there.
    let strength = |series: &[f32], t: f32| -> f32 {
        let i = f.at_onset(t);
        let w = (f.onset_rate * 0.04) as usize;
        series[i.saturating_sub(w)..(i + w + 1).min(series.len())]
            .iter()
            .copied()
            .fold(0.0, f32::max)
    };
    let kicks = peaks_of(&f.kick, (f.onset_rate * 0.4) as usize);
    let on: f32 = beats.iter().map(|&t| strength(&kicks, t)).sum();
    let between: f32 = beats
        .windows(2)
        .map(|w| strength(&kicks, 0.5 * (w[0] + w[1])))
        .sum();
    if between > on * 1.3 {
        beats = beats.windows(2).map(|w| 0.5 * (w[0] + w[1])).collect();
    }
    // The tempo from the beats found: over spans of sixteen beats (so a frame's rounding is a
    // sixteenth of what it would be beat to beat), the median.
    let span = 16.min(beats.len().saturating_sub(1)).max(1);
    let mut gaps: Vec<f32> = beats
        .windows(span + 1)
        .map(|w| (w[span] - w[0]) / span as f32)
        .collect();
    gaps.sort_by(f32::total_cmp);
    let bpm = gaps
        .get(gaps.len() / 2)
        .map_or(60.0 * f.onset_rate / p, |g| 60.0 / g);
    // Beats to a bar: 3 if the kicks repeat every three beats more than every four.
    let at = |series: &[f32], t: f32| -> f32 {
        let i = f.at_onset(t);
        let w = (f.onset_rate * 0.05) as usize;
        series[i.saturating_sub(w)..(i + w + 1).min(series.len())]
            .iter()
            .copied()
            .fold(0.0, f32::max)
    };
    let kick: Vec<f32> = beats.iter().map(|&t| at(&f.kick, t)).collect();
    let low = |lag: usize| -> f32 {
        let n = kick.len().saturating_sub(lag);
        if n == 0 {
            return 0.0;
        }
        (0..n).map(|i| kick[i] * kick[i + lag]).sum::<f32>() / n as f32
    };
    let per_bar = if low(3) > low(4) * 1.25 && low(6) > low(4) {
        3
    } else {
        4
    };
    // Downbeats: the beat in the bar where kicks land, and harmony changes, most.
    let change: Vec<f32> = (0..beats.len())
        .map(|i| {
            if i == 0 {
                return 0.0;
            }
            let (a, b) = (f.at(beats[i - 1]), f.at(beats[i]));
            let c = (b..f.at(beats.get(i + 1).copied().unwrap_or(beats[i] + 0.5))).fold(
                [0f32; 12],
                |mut acc, k| {
                    if let Some(x) = f.chroma.get(k) {
                        (0..12).for_each(|j| acc[j] += x[j]);
                    }
                    acc
                },
            );
            let d = (a..b).fold([0f32; 12], |mut acc, k| {
                if let Some(x) = f.chroma.get(k) {
                    (0..12).for_each(|j| acc[j] += x[j]);
                }
                acc
            });
            1.0 - cosine(&c, &d)
        })
        .collect();
    let kick_max = kick.iter().copied().fold(1e-9, f32::max);
    let change_max = change.iter().copied().fold(1e-9, f32::max);
    let first = (0..per_bar)
        .max_by(|&a, &b| {
            let s = |phase: usize| -> f32 {
                (phase..beats.len())
                    .step_by(per_bar)
                    .map(|i| kick[i] / kick_max + 1.5 * change[i] / change_max)
                    .sum()
            };
            s(a).total_cmp(&s(b))
        })
        .unwrap_or(0);
    Beats {
        bpm,
        confidence,
        beats,
        per_bar,
        first,
    }
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na > 0.0 && nb > 0.0 {
        dot / (na * nb)
    } else {
        0.0
    }
}
