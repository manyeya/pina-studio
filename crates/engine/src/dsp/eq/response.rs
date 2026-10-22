//! What an EQ does to the sound, frequency by frequency, and bands fitted to a curve.

use super::*;

/// What sections do at `freq` (Hz), in dB.
pub fn response_db(sections: &[Section], freq: f64, rate: f64) -> f64 {
    let s = half_sin2(freq.min(0.5 * rate), rate);
    let m: f64 = sections.iter().map(|c| c.mag2(s)).product();
    10.0 * m.max(1e-30).log10()
}

/// What a band does at `freq`, dB (as if it were on, and on the whole sound).
pub fn band_response(band: &EqBand, scale: f32, freq: f64, rate: f64) -> f64 {
    response_db(&band_sections(band, scale, rate), freq, rate)
}

/// What a whole EQ does at `freq`, in dB, to the middle of the sound: its bands that are on and
/// work on all of it or its middle, and its output level.
pub fn eq_response(eq: &Eq, freq: f64, rate: f64) -> f64 {
    let bands: f64 = eq
        .bands
        .iter()
        .filter(|b| !b.off && matches!(b.placement, Placement::Stereo | Placement::Mid))
        .map(|b| band_response(b, eq.scale, freq, rate))
        .sum();
    bands + eq.output_db as f64
}

/// A sound's spectrum for the analyzer: its level (dB, a full-scale sine 0) at each of `freqs`
/// (rising), the loudest FFT bin between it and its neighbours, or between bins where they're
/// sparse. Hann windowed.
pub fn spectrum_db(samples: &[f32], rate: f64, freqs: &[f64]) -> Vec<f32> {
    use rustfft::{FftPlanner, num_complex::Complex};
    thread_local! {
        static PLANNER: std::cell::RefCell<FftPlanner<f32>> = std::cell::RefCell::new(FftPlanner::new());
    }
    let n = samples.len();
    if n < 4 || freqs.is_empty() {
        return vec![-120.0; freqs.len()];
    }
    let fft = PLANNER.with(|p| p.borrow_mut().plan_fft_forward(n));
    let mut sum = 0.0;
    let mut buf: Vec<Complex<f32>> = samples
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos();
            sum += w;
            Complex::new(x * w, 0.0)
        })
        .collect();
    fft.process(&mut buf);
    let mag: Vec<f32> = buf[..n / 2].iter().map(|c| c.norm() * 2.0 / sum).collect();
    let bin = rate / n as f64;
    let at = |f: f64| {
        let k = f / bin;
        let i = (k.floor() as usize).min(mag.len() - 2);
        let t = (k - i as f64).clamp(0.0, 1.0) as f32;
        mag[i] * (1.0 - t) + mag[i + 1] * t
    };
    (0..freqs.len())
        .map(|c| {
            let f = freqs[c];
            let lo = if c > 0 { (freqs[c - 1] * f).sqrt() } else { f };
            let hi = freqs.get(c + 1).map_or(f, |&g| (g * f).sqrt());
            let (a, b) = ((lo / bin).ceil() as usize, (hi / bin).floor() as usize);
            let m = match a <= b && b < mag.len() {
                true => mag[a..=b].iter().fold(0f32, |m, &x| m.max(x)),
                false => at(f),
            };
            20.0 * m.max(1e-7).log10()
        })
        .collect()
}

/// A sound's average spectrum (dB at each of `freqs`): the mean power of up to 400 windows of
/// `len` samples spread across it. For matching one sound to another.
pub fn average_spectrum(samples: &[f32], rate: f64, freqs: &[f64], len: usize) -> Vec<f32> {
    if samples.len() < len {
        return spectrum_db(samples, rate, freqs);
    }
    let windows = ((samples.len() - len) / (len / 2) + 1).min(400);
    let step = (samples.len() - len) / windows.max(2).saturating_sub(1).max(1);
    let mut power = vec![0f64; freqs.len()];
    for w in 0..windows {
        let at = (w * step).min(samples.len() - len);
        for (p, db) in power
            .iter_mut()
            .zip(spectrum_db(&samples[at..at + len], rate, freqs))
        {
            *p += 10f64.powf(db as f64 / 10.0);
        }
    }
    power
        .iter()
        .map(|p| (10.0 * (p / windows as f64).max(1e-14).log10()) as f32)
        .collect()
}

/// A curve smoothed over a third of an octave either way (`freqs` spaced evenly in octaves).
pub fn smooth_third(curve: &[f32], freqs: &[f64]) -> Vec<f32> {
    let n = curve.len();
    if n < 3 {
        return curve.to_vec();
    }
    let per_octave = (n - 1) as f64 / (freqs[n - 1] / freqs[0]).log2();
    let k = ((per_octave / 3.0) as usize).max(1);
    (0..n)
        .map(|i| {
            let (a, b) = (i.saturating_sub(k), (i + k).min(n - 1));
            curve[a..=b].iter().sum::<f32>() / (b - a + 1) as f32
        })
        .collect()
}

/// Bells that make a curve (dB at each of `freqs`, the difference one sound needs to sound like
/// another): the biggest stretch of it first, as wide as it goes, then what's left, up to `count`
/// bands (or until what's left is under half a dB). Each bell at most 12 dB.
pub fn fit_bands(curve: &[f32], freqs: &[f64], count: usize, rate: f64) -> Vec<EqBand> {
    let mut left = curve.to_vec();
    let mut out = Vec::new();
    let usable = |f: f64| (25.0..=18_000.0).contains(&f);
    for _ in 0..count {
        let Some(i) = (0..left.len())
            .filter(|&i| usable(freqs[i]))
            .max_by(|&a, &b| left[a].abs().total_cmp(&left[b].abs()))
        else {
            break;
        };
        let g = left[i].clamp(-12.0, 12.0);
        if g.abs() < 0.5 {
            break;
        }
        // As wide as it stays over half of it, the same way.
        let over = |j: usize| left[j].signum() == g.signum() && left[j].abs() >= g.abs() / 2.0;
        let (mut lo, mut hi) = (i, i);
        while lo > 0 && over(lo - 1) {
            lo -= 1;
        }
        while hi + 1 < left.len() && over(hi + 1) {
            hi += 1;
        }
        let octaves = (freqs[hi] / freqs[lo]).log2().max(0.2);
        let r = 2f64.powf(octaves);
        let q = (r.sqrt() / (r - 1.0)).clamp(0.3, 8.0) as f32;
        let mut band = EqBand::new(
            Shape::Bell,
            freqs[i].round() as f32,
            (g * 10.0).round() / 10.0,
        );
        band.q = (q * 100.0).round() / 100.0;
        let sections = band_sections(&band, 1.0, rate);
        for (j, v) in left.iter_mut().enumerate() {
            *v -= response_db(&sections, freqs[j], rate) as f32;
        }
        out.push(band);
    }
    out
}
