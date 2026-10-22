//! Measuring each part's own sound: its level, lows, peaks and the kick's pitch.

use super::*;

/// What a track's own sound measured, before anything of the mix's touches it.
pub(crate) struct Source {
    pub(crate) lufs: f32,
    /// Sub, bass, low mid, mid, presence, air: dB under the whole.
    pub(crate) bands: [f32; 6],
    pub(crate) width_db: f32,
    /// Its lowest note's fundamental, for a part played from notes.
    pub(crate) lowest_hz: Option<f32>,
    pub(crate) render: [Vec<f32>; 2],
}

/// A dynamic band hearing another track, its threshold set once the mix is: `band` of
/// `track`'s EQ, hearing `source` between `listens`' two frequencies with its attack and
/// release (`times`), letting go between hits or not, moving by `range`.
pub(crate) struct Hearing {
    pub(crate) track: usize,
    pub(crate) band: usize,
    pub(crate) source: usize,
    pub(crate) listens: [f32; 2],
    pub(crate) times: (f32, f32),
    pub(crate) lets_go: bool,
    pub(crate) range: f32,
}

/// The lowest note a track plays, in Hz.
pub(crate) fn lowest_hz(p: &Project, t: usize) -> Option<f32> {
    let low = p.tracks[t]
        .regions
        .iter()
        .filter_map(|r| match &r.content {
            Content::Midi { notes, .. } => notes.iter().map(|n| n.pitch).min(),
            Content::Pattern { rows, .. } => rows.iter().map(|r| r.pitch).min(),
            _ => None,
        })
        .min()?;
    Some(440.0 * 2f32.powf((low as f32 - 69.0) / 12.0))
}

/// A second-order Butterworth section, high- or low-pass at `hz` (the bilinear transform): what
/// a dynamic band's detector filters with.
pub(crate) fn butterworth(x: &[f32], rate: f32, hz: f32, high: bool) -> Vec<f32> {
    let k = (std::f32::consts::PI * hz.min(rate * 0.45) / rate).tan();
    let q = std::f32::consts::FRAC_1_SQRT_2;
    let n = 1.0 / (1.0 + k / q + k * k);
    let b = match high {
        true => [n, -2.0 * n, n],
        false => [k * k * n, 2.0 * k * k * n, k * k * n],
    };
    let a = [2.0 * (k * k - 1.0) * n, (1.0 - k / q + k * k) * n];
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    x.iter()
        .map(|&v| {
            let y = b[0] * v + b[1] * x1 + b[2] * x2 - a[0] * y1 - a[1] * y2;
            (x2, x1, y2, y1) = (x1, v, y1, y);
            y
        })
        .collect()
}

/// What a dynamic band listening between `lo` and `hi` Hz hears of a track, as its detector
/// does: the louder side, filtered there, followed with its attack and release.
pub(crate) fn heard_level(
    render: &[Vec<f32>; 2],
    rate: f32,
    [lo, hi]: [f32; 2],
    attack_ms: f32,
    release_ms: f32,
) -> Vec<f32> {
    let side = |x: &[f32]| butterworth(&butterworth(x, rate, lo, true), rate, hi, false);
    let (l, r) = (side(&render[0]), side(&render[1]));
    let coef = |ms: f32| 1.0 - (-1.0 / (ms.max(0.05) * 1e-3 * rate)).exp();
    let (up, down) = (coef(attack_ms), coef(release_ms));
    let mut env = 0f32;
    l.iter()
        .zip(&r)
        .map(|(a, b)| {
            let v = a.abs().max(b.abs());
            env += (v - env) * if v > env { up } else { down };
            env
        })
        .collect()
}

/// How hard a track hits between `lo` and `hi` Hz as a dynamic band's detector hears it (with
/// its attack and release), and how far that's fallen by the end of each beat: each beat's
/// peak, and its least over the beat's last quarter; of each, the loudest quarter's least (a
/// sparse kick leaves many beats without one). dBFS.
pub(crate) fn band_peak_db(
    render: &[Vec<f32>; 2],
    rate: f32,
    beat: usize,
    band: [f32; 2],
    times: (f32, f32),
) -> Option<(f32, f32)> {
    let env = heard_level(render, rate, band, times.0, times.1);
    let most = |c: &[f32]| c.iter().fold(0f32, |m, v| m.max(*v));
    let least = |c: &[f32]| c.iter().fold(f32::MAX, |m, v| m.min(*v));
    let (mut peaks, mut ends): (Vec<f32>, Vec<f32>) = env
        .chunks(beat.max(1))
        .filter(|c| c.len() == beat.max(1))
        .map(|c| (most(c), least(&c[c.len() * 3 / 4..])))
        .filter(|(p, _)| *p > 1e-4)
        .unzip();
    if peaks.is_empty() {
        return None;
    }
    peaks.sort_by(f32::total_cmp);
    ends.sort_by(f32::total_cmp);
    let db = |v: f32| 20.0 * v.max(1e-6).log10();
    Some((db(peaks[peaks.len() * 3 / 4]), db(ends[ends.len() * 3 / 4])))
}

/// Where a kick's thump is strongest, 40 to 100 Hz (its fundamental): Goertzel's energy at each
/// frequency over its loudest beats.
pub(crate) fn kick_hz(render: &[Vec<f32>; 2], rate: f32) -> f32 {
    let mono: Vec<f32> = render[0]
        .iter()
        .zip(&render[1])
        .map(|(l, r)| (l + r) * 0.5)
        .collect();
    let energy = |f: f32| {
        let w = 2.0 * std::f32::consts::PI * f / rate;
        let c = 2.0 * w.cos();
        let (mut s1, mut s2) = (0f32, 0f32);
        for &x in mono.iter().step_by(4) {
            let s = x + c * s1 - s2;
            s2 = s1;
            s1 = s;
        }
        s1 * s1 + s2 * s2 - c * s1 * s2
    };
    // Sampled every fourth sample: to Goertzel, every frequency is four times as high.
    (40..=100)
        .step_by(5)
        .map(|f| f as f32)
        .max_by(|a, b| energy(*a * 4.0).total_cmp(&energy(*b * 4.0)))
        .unwrap_or(55.0)
}
