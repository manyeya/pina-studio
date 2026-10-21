//! A sample's rhythm: where its hits are (onsets, from how fast its mel spectrum rises), and,
//! for a loop, its tempo: the beat period its onsets repeat at, an octave chosen with a gentle
//! preference for 70-160 BPM, made exact when the loop is a whole number of beats long.

/// Onset strength per frame: how much louder the mel bands get from the frame before (dB,
/// rises only, averaged over the bands). The first frame rises from silence.
pub fn onset_strength(log_mel: &[Vec<f32>], floor: f32) -> Vec<f32> {
    let mut previous: Option<&Vec<f32>> = None;
    log_mel
        .iter()
        .map(|frame| {
            let rise: f32 = frame
                .iter()
                .enumerate()
                .map(|(m, &v)| (v - previous.map_or(floor, |p| p[m])).max(0.0))
                .sum();
            previous = Some(frame);
            rise / frame.len().max(1) as f32
        })
        .collect()
}

/// The frames where a hit starts: peaks of the onset strength that stand out from what's around
/// them, at least 50 ms apart.
pub fn onsets(strength: &[f32], fps: f32) -> Vec<usize> {
    let top = strength.iter().fold(0f32, |m, &v| m.max(v));
    if top <= 0.0 {
        return Vec::new();
    }
    let gap = (0.05 * fps).ceil() as usize;
    let around = (0.1 * fps).ceil() as usize;
    let mean_span = (0.4 * fps).ceil() as usize;
    let mut found: Vec<usize> = Vec::new();
    for t in 0..strength.len() {
        let v = strength[t];
        let lo = t.saturating_sub(around);
        let hi = (t + around + 1).min(strength.len());
        if strength[lo..hi].iter().any(|&w| w > v) {
            continue;
        }
        let lo = t.saturating_sub(mean_span);
        let hi = (t + mean_span / 4 + 1).min(strength.len());
        let mean = strength[lo..hi].iter().sum::<f32>() / (hi - lo) as f32;
        if v < mean + 0.1 * top || v < 0.12 * top {
            continue;
        }
        if found.last().is_some_and(|&last| t - last < gap) {
            continue;
        }
        found.push(t);
    }
    found
}

/// A loop's tempo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tempo {
    pub bpm: f32,
    /// 0 to 1: how strongly its onsets repeat at the beat.
    pub strength: f32,
    /// How many beats long it is, when it's a whole number of beats at this tempo.
    pub beats: Option<u32>,
}

/// The onset strength readied for finding a period: what stands above its surroundings (a
/// moving average over half a second taken off), smoothed over about 50 ms so hits a frame
/// early or late still line up.
fn pulses(strength: &[f32], fps: f32) -> Vec<f32> {
    let n = strength.len();
    let half = ((0.25 * fps) as usize).max(1);
    let mut sum = vec![0f32; n + 1];
    for (i, v) in strength.iter().enumerate() {
        sum[i + 1] = sum[i] + v;
    }
    let peaks: Vec<f32> = (0..n)
        .map(|i| {
            let (a, b) = (i.saturating_sub(half), (i + half + 1).min(n));
            (strength[i] - (sum[b] - sum[a]) / (b - a) as f32).max(0.0)
        })
        .collect();
    let radius = ((0.025 * fps).round() as usize).max(1);
    let kernel: Vec<f32> = (0..=2 * radius)
        .map(|k| {
            let x = (k as f32 - radius as f32) / (radius as f32 + 1.0);
            0.5 + 0.5 * (std::f32::consts::PI * x).cos()
        })
        .collect();
    // Circular, as the autocorrelation is.
    (0..n)
        .map(|i| {
            kernel
                .iter()
                .enumerate()
                .map(|(k, w)| w * peaks[(i + n + k - radius) % n])
                .sum()
        })
        .collect()
}

/// The circular autocorrelation of `x` (loops wrap around), lags 0 to `max`, normalised so lag 0
/// is 1.
fn autocorrelation(x: &[f32], max: usize) -> Vec<f32> {
    let n = x.len();
    let mean = x.iter().sum::<f32>() / n as f32;
    let centred: Vec<f32> = x.iter().map(|v| v - mean).collect();
    let zero: f32 = centred.iter().map(|v| v * v).sum();
    if zero <= 0.0 {
        return vec![0.0; max + 1];
    }
    (0..=max)
        .map(|lag| {
            (0..n)
                .map(|t| centred[t] * centred[(t + lag) % n])
                .sum::<f32>()
                / zero
        })
        .collect()
}

fn at(ac: &[f32], lag: f32) -> f32 {
    let i = lag.floor() as usize;
    if i + 1 >= ac.len() {
        return 0.0;
    }
    let f = lag - i as f32;
    ac[i] * (1.0 - f) + ac[i + 1] * f
}

/// The beat's multiples a tempo is judged by, and their weights: the beat, half bar, bar, and
/// the eighth.
const COMB: [(f32, f32); 4] = [(1.0, 0.5), (2.0, 0.7), (4.0, 1.0), (0.5, 0.2)];

/// How much a beat period (frames) shows in the autocorrelation: at the beat, and at the half bar
/// and bar.
fn comb(ac: &[f32], period: f32) -> f32 {
    let mut score = 0.0;
    let mut weight = 0.0;
    for (k, w) in COMB {
        let lag = period * k;
        if lag + 1.0 < ac.len() as f32 {
            score += w * at(ac, lag);
            weight += w;
        }
    }
    if weight > 0.0 { score / weight } else { 0.0 }
}

/// How well a whole-bar tempo has to fit, against the best tempo of any length: loops are cut
/// to bars so often that one fitting a quarter as well is still the likelier.
const WHOLE: f32 = 0.25;

/// The tempo preference: a log-normal centred on 110 BPM, an octave and a bit wide.
fn prior(bpm: f32) -> f32 {
    let octaves = (bpm / 110.0).log2();
    (-0.5 * (octaves / 0.75).powi(2)).exp()
}

/// The tempo of an onset strength track `seconds` long at `fps` frames a second, if it has one.
/// `hint` is a tempo from its name: when the onsets bear it out (or half or double it), it's
/// taken.
pub fn tempo(strength: &[f32], fps: f32, seconds: f32, hint: Option<f32>) -> Option<Tempo> {
    if seconds < 1.0 || strength.len() < 16 {
        return None;
    }
    let (slowest, fastest) = (45.0, 240.0);
    // Up to its whole length: a loop wrapped round matches itself there, which favours tempos
    // it holds a whole number of bars of.
    let max_lag = ((60.0 / slowest * fps * 4.0) as usize).min(strength.len() - 1);
    let ac = autocorrelation(&pulses(strength, fps), max_lag);
    let period = |bpm: f32| 60.0 * fps / bpm;
    // The best beat on a coarse grid, then refined: the beat's comb, gently preferring 110 BPM.
    let mut best = (0.0f32, f32::MIN);
    let mut bpm = slowest;
    while bpm <= fastest {
        if period(bpm) * 0.5 < max_lag as f32 {
            let score = comb(&ac, period(bpm)).max(0.0) * prior(bpm);
            if score > best.1 {
                best = (bpm, score);
            }
        }
        bpm *= 1.005;
    }
    if best.1 <= 0.0 {
        // No pulse at all (a held chord, a pad): only its length to go on, a whole number of
        // bars at the likeliest tempo.
        let (bpm, n) = (2..=64u32)
            .step_by(2)
            .map(|n| (60.0 * n as f32 / seconds, n))
            .filter(|(bpm, _)| (55.0..=200.0).contains(bpm))
            .max_by(|a, b| {
                let score = |(bpm, n): (f32, u32)| prior(bpm) * if n % 4 == 0 { 1.0 } else { 0.5 };
                score(*a).total_cmp(&score(*b))
            })?;
        let bpm = match hint {
            Some(named)
                if [0.5, 1.0, 2.0]
                    .iter()
                    .any(|k| ((bpm * k - named) / named).abs() < 0.05) =>
            {
                named
            }
            _ => bpm,
        };
        return Some(Tempo {
            bpm,
            strength: 0.0,
            beats: whole_beats(seconds, bpm).or(Some(n)),
        });
    }
    // Several bar-length multiples pin the period down.
    let fine = |bpm: f32| -> f32 {
        let p = period(bpm);
        let count = ((max_lag as f32 / p) as usize).clamp(1, 16);
        (1..=count).map(|k| at(&ac, p * k as f32)).sum::<f32>() / count as f32
    };
    let mut refined = best.0;
    let mut top = f32::MIN;
    let mut bpm = best.0 * 0.97;
    while bpm <= best.0 * 1.03 {
        let s = fine(bpm);
        if s > top {
            (top, refined) = (s, bpm);
        }
        bpm += 0.02;
    }
    let strength_of = |bpm: f32| at(&ac, period(bpm)).clamp(0.0, 1.0);
    // A name's tempo, when the audio is near it or half or double it.
    if let Some(named) = hint {
        let agrees = [0.5, 1.0, 2.0, 2.0 / 3.0, 1.5]
            .iter()
            .any(|k| ((refined * k - named) / named).abs() < 0.05);
        if agrees {
            return Some(Tempo {
                bpm: named,
                strength: strength_of(named),
                beats: whole_beats(seconds, named),
            });
        }
    }
    // A loop cut to whole bars says its tempo exactly: of the tempos it could be (2, 4, 8, 16
    // beats long...), the one its onsets bear out best, if they bear it out about as well as
    // any tempo.
    let mut whole: Option<(f32, f32, u32)> = None;
    for n in (2..=64u32).step_by(2) {
        let bpm = 60.0 * n as f32 / seconds;
        if !(55.0..=200.0).contains(&bpm) || period(bpm) * 0.5 >= max_lag as f32 {
            continue;
        }
        // Loops are most often 1, 2, 4 or 8 bars long; other whole bars less, half bars least.
        let bars = match n {
            4 | 8 | 16 | 32 => 1.0,
            n if n % 4 == 0 => 0.75,
            _ => 0.5,
        };
        let score = comb(&ac, period(bpm)).max(0.0) * prior(bpm) * bars;
        if whole.is_none_or(|w| score > w.1) {
            whole = Some((bpm, score, n));
        }
    }
    if let Some((bpm, score, n)) = whole
        && score >= WHOLE * best.1
    {
        return Some(Tempo {
            bpm,
            strength: strength_of(bpm),
            beats: Some(n),
        });
    }
    let beats = seconds * refined / 60.0;
    for unit in [4.0, 2.0, 1.0] {
        let n = (beats / unit).round() * unit;
        if n >= 1.0 && ((n - beats) / beats).abs() < 0.025 {
            let exact = 60.0 * n / seconds;
            return Some(Tempo {
                bpm: exact,
                strength: strength_of(exact),
                beats: Some(n as u32),
            });
        }
    }
    let rounded = if (refined - refined.round()).abs() < 0.35 {
        refined.round()
    } else {
        (refined * 10.0).round() / 10.0
    };
    Some(Tempo {
        bpm: rounded,
        strength: strength_of(rounded),
        beats: None,
    })
}

/// How many beats `seconds` is at `bpm`, when it's close to a whole number.
pub fn whole_beats(seconds: f32, bpm: f32) -> Option<u32> {
    let beats = seconds * bpm / 60.0;
    let n = beats.round();
    (n >= 1.0 && (beats - n).abs() < 0.1).then_some(n as u32)
}

/// A tempo a name gives: "95BPM", "120 bpm", "Loop_140bpm". Numbers alone ("(95)", "[128]") are
/// weaker: `true` with them means the audio has to agree.
pub fn named_tempo(hint: &str) -> Option<(f32, bool)> {
    let name = hint.rsplit('/').next().unwrap_or(hint);
    let bytes = hint.as_bytes();
    // "bpm" with a number just before it, nearest the file's name first.
    let mut found = None;
    let mut from = 0;
    while let Some(i) = hint[from..].find("bpm") {
        let end = from + i;
        let mut start = end;
        while start > 0 && (bytes[start - 1] == b' ' || bytes[start - 1] == b'_') && end - start < 2
        {
            start -= 1;
        }
        let digits_end = start;
        while start > 0 && (bytes[start - 1].is_ascii_digit() || bytes[start - 1] == b'.') {
            start -= 1;
        }
        if let Ok(v) = hint[start..digits_end].parse::<f32>()
            && (40.0..=300.0).contains(&v)
        {
            found = Some(v);
        }
        from = end + 3;
    }
    if let Some(v) = found {
        return Some((v, false));
    }
    // A bracketed number in the file's name or its folder: "(95)", "[128]", "01(100)".
    let parent = hint.rsplit('/').nth(1).unwrap_or("");
    for part in [name, parent] {
        for (open, close) in [('(', ')'), ('[', ']')] {
            let mut rest = part;
            while let Some(i) = rest.find(open) {
                rest = &rest[i + 1..];
                let Some(j) = rest.find(close) else { break };
                if let Ok(v) = rest[..j].trim().parse::<f32>()
                    && (50.0..=250.0).contains(&v)
                {
                    return Some((v, true));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests;
