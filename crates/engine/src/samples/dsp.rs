//! The signal processing the analysis is built on: spectra, mel bands and cepstra, envelopes
//! and loudness.

use std::sync::Arc;

use rustfft::{Fft, FftPlanner, num_complex::Complex32};

/// Power spectra of Hann-windowed frames of one size.
pub struct Spectra {
    pub size: usize,
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    buf: Vec<Complex32>,
    scratch: Vec<Complex32>,
}

impl Spectra {
    pub fn new(size: usize) -> Self {
        let fft = FftPlanner::new().plan_fft_forward(size);
        let scratch = vec![Complex32::default(); fft.get_inplace_scratch_len()];
        // Scaled so a full-scale sine's bin reads about 1 (power 1/4 per side of the window).
        let norm = 2.0 / size as f32;
        let window = (0..size)
            .map(|i| norm * (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / size as f32).cos()))
            .collect();
        Spectra {
            size,
            fft,
            window,
            buf: vec![Complex32::default(); size],
            scratch,
        }
    }

    /// The power spectrum (`size / 2 + 1` bins) of `x` from `start`: zero outside it.
    pub fn power(&mut self, x: &[f32], start: isize, out: &mut [f32]) {
        for (i, b) in self.buf.iter_mut().enumerate() {
            let at = start + i as isize;
            let v = if at >= 0 && (at as usize) < x.len() {
                x[at as usize] * self.window[i]
            } else {
                0.0
            };
            *b = Complex32::new(v, 0.0);
        }
        self.fft
            .process_with_scratch(&mut self.buf, &mut self.scratch);
        for (o, b) in out.iter_mut().zip(&self.buf) {
            *o = b.norm_sqr();
        }
    }
}

/// A short-time spectrum: frames `hop` apart, each a power spectrum of `size` points.
pub struct Stft {
    pub size: usize,
    pub hop: usize,
    pub rate: f32,
    pub frames: Vec<Vec<f32>>,
}

impl Stft {
    pub fn new(x: &[f32], rate: f32, size: usize, hop: usize) -> Stft {
        let mut spectra = Spectra::new(size);
        let count = x.len().div_ceil(hop).max(1);
        let frames = (0..count)
            .map(|t| {
                let mut p = vec![0.0; size / 2 + 1];
                spectra.power(x, (t * hop) as isize, &mut p);
                p
            })
            .collect();
        Stft {
            size,
            hop,
            rate,
            frames,
        }
    }

    pub fn bin_hz(&self) -> f32 {
        self.rate / self.size as f32
    }

    /// Frames per second.
    pub fn fps(&self) -> f32 {
        self.rate / self.hop as f32
    }
}

/// The analysis frame size for a sample rate: about 46 ms.
pub fn frame_size(rate: f32) -> usize {
    ((rate * 0.046) as usize).next_power_of_two().max(256)
}

/// A mel filterbank: triangles evenly spaced in mel, each of unit area, so white noise reads the
/// same in every band.
pub struct Mel {
    filters: Vec<(usize, Vec<f32>)>,
}

pub fn hz_to_mel(f: f32) -> f32 {
    2595.0 * (1.0 + f / 700.0).log10()
}

pub fn mel_to_hz(m: f32) -> f32 {
    700.0 * (10f32.powf(m / 2595.0) - 1.0)
}

impl Mel {
    pub fn new(bands: usize, size: usize, rate: f32, low: f32, high: f32) -> Mel {
        let high = high.min(rate / 2.0);
        let (lo, hi) = (hz_to_mel(low), hz_to_mel(high));
        let edges: Vec<f32> = (0..bands + 2)
            .map(|i| mel_to_hz(lo + (hi - lo) * i as f32 / (bands + 1) as f32))
            .collect();
        let bin = rate / size as f32;
        let filters = (0..bands)
            .map(|b| {
                let (f0, f1, f2) = (edges[b], edges[b + 1], edges[b + 2]);
                let first = (f0 / bin).ceil() as usize;
                let last = ((f2 / bin).floor() as usize).min(size / 2);
                let area = 2.0 / (f2 - f0).max(bin);
                let mut weights: Vec<f32> = (first..=last.max(first))
                    .map(|k| {
                        let f = k as f32 * bin;
                        let w = if f < f1 {
                            (f - f0) / (f1 - f0)
                        } else {
                            (f2 - f) / (f2 - f1)
                        };
                        w.max(0.0) * area
                    })
                    .collect();
                // A band narrower than a bin still takes its nearest bin.
                if weights.iter().all(|&w| w == 0.0) {
                    weights = vec![area];
                    return (((f1 / bin).round() as usize).min(size / 2), weights);
                }
                (first, weights)
            })
            .collect();
        Mel { filters }
    }

    pub fn bands(&self) -> usize {
        self.filters.len()
    }

    /// The mel bands' power from a power spectrum.
    pub fn apply(&self, power: &[f32], out: &mut [f32]) {
        for ((first, weights), o) in self.filters.iter().zip(out.iter_mut()) {
            *o = weights
                .iter()
                .zip(&power[*first..])
                .map(|(w, p)| w * p)
                .sum();
        }
    }
}

/// An orthonormal DCT-II of `x`, its first `n` coefficients: cepstra from log mel bands.
pub fn dct(x: &[f32], n: usize) -> Vec<f32> {
    let len = x.len() as f32;
    (0..n)
        .map(|k| {
            let scale = if k == 0 {
                (1.0 / len).sqrt()
            } else {
                (2.0 / len).sqrt()
            };
            scale
                * x.iter()
                    .enumerate()
                    .map(|(i, v)| {
                        v * (std::f32::consts::PI * k as f32 * (i as f32 + 0.5) / len).cos()
                    })
                    .sum::<f32>()
        })
        .collect()
}

pub fn db(power: f32) -> f32 {
    10.0 * power.max(1e-20).log10()
}

pub fn amp_db(amp: f32) -> f32 {
    20.0 * amp.max(1e-10).log10()
}

/// Loudness (LUFS): BS.1770's K-weighted power over both channels (a mono sample counts as
/// playing in both). For a short sound, the loudest 400 ms (EBU's maximum momentary loudness);
/// otherwise gated integrated loudness.
pub fn loudness(channels: &[Vec<f32>], rate: f64, short: bool) -> f32 {
    let [shelf, highpass] = crate::music::analysis::k_weighting(rate);
    let len = channels.first().map_or(0, Vec::len);
    let mut power = vec![0f64; len];
    for c in channels {
        let wide: Vec<f64> = c.iter().map(|&v| v as f64).collect();
        let weighted = highpass.run(&shelf.run(&wide));
        let gain = if channels.len() == 1 { 2.0 } else { 1.0 };
        for (p, w) in power.iter_mut().zip(weighted) {
            *p += gain * w * w;
        }
    }
    let block = (0.4 * rate) as usize;
    let lufs = |p: f64| (-0.691 + 10.0 * p.max(1e-20).log10()) as f32;
    if len == 0 {
        return -70.0;
    }
    // Running sums make each block's mean power one subtraction.
    let mut sum = vec![0f64; len + 1];
    for (i, p) in power.iter().enumerate() {
        sum[i + 1] = sum[i] + p;
    }
    let mean = |start: usize| (sum[(start + block).min(len)] - sum[start]) / block as f64;
    let result = if short || len < block {
        let step = (0.01 * rate) as usize;
        let last = len.saturating_sub(block);
        (0..=last / step.max(1))
            .map(|i| mean(i * step))
            .chain(std::iter::once(mean(last)))
            .fold(0.0, f64::max)
    } else {
        let step = (0.1 * rate) as usize;
        let blocks: Vec<f64> = (0..=(len - block) / step).map(|i| mean(i * step)).collect();
        let gated = |gate: f32| {
            let kept: Vec<f64> = blocks.iter().copied().filter(|&p| lufs(p) > gate).collect();
            (!kept.is_empty()).then(|| kept.iter().sum::<f64>() / kept.len() as f64)
        };
        match gated(-70.0) {
            Some(ungated) => gated(lufs(ungated) - 10.0).unwrap_or(ungated),
            None => 0.0,
        }
    };
    lufs(result).max(-70.0)
}

/// Takes out DC and anything under a few hertz (a one-pole high-pass at 5 Hz): some samples
/// carry an offset, which would otherwise read as their loudest, lowest sound.
pub fn dc_block(x: &[f32], rate: f32) -> Vec<f32> {
    let r = 1.0 - std::f32::consts::TAU * 5.0 / rate;
    // Started from its average, so a steady offset is gone from the first sample.
    let mean = x.iter().sum::<f32>() / x.len().max(1) as f32;
    let (mut last_in, mut last_out) = (mean, 0.0);
    x.iter()
        .map(|&v| {
            let y = v - last_in + r * last_out;
            (last_in, last_out) = (v, y);
            y
        })
        .collect()
}

/// A level envelope: the peak magnitude in each `step`-sample block.
pub fn peak_envelope(x: &[f32], step: usize) -> Vec<f32> {
    x.chunks(step.max(1))
        .map(|c| c.iter().fold(0f32, |m, v| m.max(v.abs())))
        .collect()
}

/// The largest value within `radius` of each point.
pub fn running_max(x: &[f32], radius: usize) -> Vec<f32> {
    // ponytail: straight, O(n * radius); a monotonic deque if radius grows past ~50.
    (0..x.len())
        .map(|i| {
            x[i.saturating_sub(radius)..(i + radius + 1).min(x.len())]
                .iter()
                .fold(0f32, |m, &v| m.max(v))
        })
        .collect()
}

/// The median of some values (0 for none).
pub fn median(values: &mut [f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// Where the peak near `i` really is, from a parabola through it and its neighbours: an offset
/// from -0.5 to 0.5.
pub fn parabolic(a: f32, b: f32, c: f32) -> f32 {
    let d = a - 2.0 * b + c;
    if d.abs() < 1e-12 {
        0.0
    } else {
        (0.5 * (a - c) / d).clamp(-0.5, 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sine_lands_in_its_bin_at_about_full_power() {
        let rate = 44_100.0;
        let size = 2048;
        let k = 100;
        let f = k as f32 * rate / size as f32;
        let x: Vec<f32> = (0..size)
            .map(|i| (std::f32::consts::TAU * f * i as f32 / rate).sin())
            .collect();
        let mut s = Spectra::new(size);
        let mut p = vec![0.0; size / 2 + 1];
        s.power(&x, 0, &mut p);
        let top = (0..p.len()).max_by(|&a, &b| p[a].total_cmp(&p[b])).unwrap();
        assert_eq!(top, k);
        // A Hann window's peak gain is half; scaled by 2/N that's 1/2 in amplitude, 1/4 in power.
        assert!((p[k] - 0.25).abs() < 0.01, "{}", p[k]);
    }

    #[test]
    fn white_noise_reads_level_across_mel_bands() {
        let size = 2048;
        let mel = Mel::new(40, size, 44_100.0, 20.0, 16_000.0);
        let flat = vec![1.0; size / 2 + 1];
        let mut out = vec![0.0; 40];
        mel.apply(&flat, &mut out);
        let bin = 44_100.0 / size as f32;
        // Unit-area triangles over a flat density of 1 per bin: each band reads 1 / bin.
        for &o in &out[8..] {
            assert!((o * bin - 1.0).abs() < 0.15, "{out:?}");
        }
    }

    #[test]
    fn loudness_matches_bs1770_for_a_sine() {
        // A 0 dBFS 997 Hz sine in one channel of two reads -3.01 LUFS.
        let rate = 48_000.0;
        let tone: Vec<f32> = (0..(rate as usize * 3))
            .map(|i| (std::f64::consts::TAU * 997.0 * i as f64 / rate).sin() as f32)
            .collect();
        let silence = vec![0.0; tone.len()];
        let l = loudness(&[tone.clone(), silence], rate, false);
        assert!((l + 3.01).abs() < 0.05, "{l}");
        // The loudest moment of a short burst: the same tone for 400 ms reads the same.
        let burst: Vec<f32> = tone[..(0.4 * rate) as usize].to_vec();
        let l = loudness(&[burst.clone(), vec![0.0; burst.len()]], rate, true);
        assert!((l + 3.01).abs() < 0.1, "{l}");
    }

    #[test]
    fn dct_of_a_constant_is_all_in_the_first_coefficient() {
        let c = dct(&[2.0; 8], 4);
        assert!((c[0] - 2.0 * 8f32.sqrt()).abs() < 1e-4);
        assert!(c[1..].iter().all(|v| v.abs() < 1e-4));
    }
}
