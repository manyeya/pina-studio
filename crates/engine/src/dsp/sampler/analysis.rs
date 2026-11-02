//! What a sample is made of: where its hits start (to slice it there), and the note it sounds.

use std::path::Path;

use crate::song::project::Zone;

/// Where a sound's hits start, as shares of its length, each with how hard it starts: rises in
/// the energy of its edges (the difference between samples, so a hat's tick counts as much as a
/// kick's thud), each the biggest within 60 ms, over the sound's typical rise.
pub fn onsets(mono: &[f32], rate: f32) -> Vec<(f32, f32)> {
    const SIZE: usize = 1024;
    const HOP: usize = 256;
    let n = mono.len();
    if n < SIZE * 2 {
        return vec![(0.0, 1.0)];
    }
    let energy: Vec<f32> = (0..(n - SIZE) / HOP)
        .map(|f| {
            let a = f * HOP;
            let e: f32 = mono[a + 1..a + SIZE]
                .iter()
                .zip(&mono[a..a + SIZE - 1])
                .map(|(x, y)| (x - y) * (x - y))
                .sum();
            (e + 1e-9).ln()
        })
        .collect();
    let flux: Vec<f32> = std::iter::once(0.0)
        .chain(energy.windows(2).map(|w| (w[1] - w[0]).max(0.0)))
        .collect();
    let mean = flux.iter().sum::<f32>() / flux.len().max(1) as f32;
    let sd =
        (flux.iter().map(|f| (f - mean).powi(2)).sum::<f32>() / flux.len().max(1) as f32).sqrt();
    let gap = ((0.06 * rate) as usize / HOP).max(1);
    let mut out: Vec<(f32, f32)> = vec![(0.0, flux.iter().copied().fold(0.0, f32::max).max(1.0))];
    for i in 1..flux.len() {
        let (a, b) = (i.saturating_sub(gap), (i + gap + 1).min(flux.len()));
        if flux[i] > mean + sd && flux[a..b].iter().all(|&v| v <= flux[i]) {
            // The new energy came in at the frame's end; the hit starts where the sound first
            // rises toward its peak there (a few samples before, to keep its attack whole).
            let centre = i * HOP + SIZE - HOP;
            let (a, b) = (centre.saturating_sub(HOP), (centre + HOP).min(n));
            let peak = mono[a..b].iter().fold(0f32, |m, x| m.max(x.abs()));
            let rise = mono[a..b]
                .iter()
                .position(|x| x.abs() > 0.2 * peak)
                .unwrap_or(0);
            let at = (a + rise).saturating_sub(16) as f32 / n as f32;
            if at - out.last().map_or(-1.0, |o| o.0) > 0.06 * rate / n as f32 {
                out.push((at, flux[i]));
            }
        }
    }
    out
}

/// The strongest `most` of a sound's onsets, in order: where to cut it into slices (its start
/// always one).
pub fn cuts(onsets: &[(f32, f32)], most: usize) -> Vec<f32> {
    let mut by_strength: Vec<(f32, f32)> = onsets.iter().copied().skip(1).collect();
    by_strength.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut cuts: Vec<f32> = std::iter::once(0.0)
        .chain(
            by_strength
                .into_iter()
                .take(most.saturating_sub(1))
                .map(|o| o.0),
        )
        .collect();
    cuts.sort_by(f32::total_cmp);
    cuts
}

/// A sound cut into one-shot zones at `cuts` (shares of its length), on notes upward from
/// `first`, as a slicer does.
pub fn slices(file: &Path, cuts: &[f32], first: u8) -> Vec<Zone> {
    cuts.iter()
        .enumerate()
        .filter(|(k, _)| first as usize + k <= 127)
        .map(|(k, &start)| Zone {
            file: file.to_path_buf(),
            name: format!("Slice {}", k + 1),
            note: first + k as u8,
            start,
            end: cuts.get(k + 1).copied().filter(|&e| e < 1.0),
            ..Zone::default()
        })
        .collect()
}

/// The pitch a sound holds (Hz), if it holds one: the median of what the YIN method hears in
/// a few windows of its loudest, steadiest part (after its attack).
pub fn pitch(mono: &[f32], rate: f32) -> Option<f32> {
    const WINDOW: usize = 2048;
    if mono.len() < WINDOW * 2 {
        return None;
    }
    let rms =
        |a: usize| (mono[a..a + WINDOW].iter().map(|x| x * x).sum::<f32>() / WINDOW as f32).sqrt();
    let starts: Vec<usize> = (0..mono.len() - WINDOW * 2).step_by(WINDOW / 2).collect();
    let loudest = starts.iter().map(|&a| rms(a)).fold(0f32, f32::max);
    if loudest < 1e-4 {
        return None;
    }
    // Past the attack: windows a little after the first loud one, while it stays loud.
    let mut found: Vec<f32> = starts
        .iter()
        .copied()
        .skip_while(|&a| rms(a) < 0.5 * loudest)
        .skip(1)
        .filter(|&a| rms(a) > 0.25 * loudest)
        .take(8)
        .filter_map(|a| yin(&mono[a..a + WINDOW * 2], rate))
        .collect();
    if found.is_empty() {
        return None;
    }
    found.sort_by(f32::total_cmp);
    Some(found[found.len() / 2])
}

/// One window's pitch by YIN (de Cheveigné and Kawahara): the shortest lag whose cumulative
/// mean normalized difference dips under a threshold, refined between lags; 40 Hz to 2 kHz.
fn yin(x: &[f32], rate: f32) -> Option<f32> {
    let hi = ((rate / 40.0) as usize).min(x.len() / 2 - 1);
    let (mut d, mut cmnd) = (vec![0f32; hi + 2], vec![1f32; hi + 2]);
    yin_in(x, rate, (40.0, 2_000.0), &mut d, &mut cmnd)
}

/// YIN between `lo` and `hi` Hz, in the scratch it's given, for the audio thread: each at least
/// the longest lag and two long.
pub(crate) fn yin_in(
    x: &[f32],
    rate: f32,
    (lo, hi): (f32, f32),
    d: &mut [f32],
    cmnd: &mut [f32],
) -> Option<f32> {
    let w = x.len() / 2;
    let (lo, hi) = (
        (rate / hi) as usize,
        ((rate / lo) as usize).min(w - 1).min(d.len() - 2),
    );
    for (tau, slot) in d.iter_mut().enumerate().take(hi + 2).skip(1) {
        *slot = (0..w).map(|j| (x[j] - x[j + tau]).powi(2)).sum();
    }
    cmnd[..hi + 2].fill(1.0);
    let mut sum = 0f32;
    for tau in 1..hi + 2 {
        sum += d[tau];
        cmnd[tau] = if sum > 0.0 {
            d[tau] * tau as f32 / sum
        } else {
            1.0
        };
    }
    let mut tau = (lo.max(2)..=hi).find(|&t| cmnd[t] < 0.15)?;
    while tau < hi && cmnd[tau + 1] < cmnd[tau] {
        tau += 1;
    }
    let (a, b, c) = (cmnd[tau - 1], cmnd[tau], cmnd[tau + 1]);
    let shift = match a - 2.0 * b + c {
        den if den.abs() > 1e-9 => 0.5 * (a - c) / den,
        _ => 0.0,
    };
    Some(rate / (tau as f32 + shift.clamp(-1.0, 1.0)))
}

/// The note a pitch is nearest, and how many cents it's off it (+ sharp).
pub fn note_of(hz: f32) -> (u8, f32) {
    let midi = 69.0 + 12.0 * (hz / 440.0).log2();
    let note = midi.round().clamp(0.0, 127.0);
    (note as u8, (midi - note) * 100.0)
}
