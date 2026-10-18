//! What a song is made of, frame by frame (about 23 ms each): how much starts there (overall, and
//! in the kick's, snare's and hats' bands), its pitch classes (all of it, and its bass alone),
//! its timbre, and how loud it is. Everything the rest of the study reads.

use crate::samples::dsp::Spectra;

/// The rate frames are taken at, near 22 kHz (a song's top octave says little about its form).
const WORK_RATE: f32 = 22_050.0;
const SIZE: usize = 4096;
const HOP: usize = 512;
const BANDS: usize = 40;
pub const TIMBRE: usize = 12;

pub struct Frames {
    /// Frames a second (of the pitch classes, timbre and loudness).
    pub rate: f32,
    /// Onset frames a second: finer (about 6 ms), so tempo and beats are exact.
    pub onset_rate: f32,
    /// Something starting, everywhere: spectral flux on log band energies.
    pub onset: Vec<f32>,
    /// Starts in the kick's band (under 150 Hz), the snare's (1-4 kHz, where its noise is) and
    /// the hats' (over 6 kHz).
    pub kick: Vec<f32>,
    pub snare: Vec<f32>,
    pub hats: Vec<f32>,
    /// Pitch classes (C first) of the tonal peaks between 55 Hz and 4 kHz, and under 250 Hz.
    pub chroma: Vec<[f32; 12]>,
    pub bass: Vec<[f32; 12]>,
    /// The spectrum's shape (cepstral coefficients, loudness left out).
    pub timbre: Vec<[f32; TIMBRE]>,
    /// Loudness, dB.
    pub level: Vec<f32>,
    /// How far the song's notes sit off A440, in cents (-50 to 50).
    pub tuning: f32,
}

impl Frames {
    pub fn len(&self) -> usize {
        self.level.len()
    }

    pub fn is_empty(&self) -> bool {
        self.level.is_empty()
    }

    /// The frame a time falls in.
    pub fn at(&self, seconds: f32) -> usize {
        ((seconds * self.rate).max(0.0) as usize).min(self.len().saturating_sub(1))
    }

    /// The onset frame a time falls in.
    pub fn at_onset(&self, seconds: f32) -> usize {
        ((seconds * self.onset_rate).max(0.0) as usize).min(self.onset.len().saturating_sub(1))
    }
}

const ONSET_SIZE: usize = 1024;
const ONSET_HOP: usize = 128;
const ONSET_BANDS: usize = 24;

/// Starts, finely: spectral flux on log band energies of short frames, everywhere and in the
/// kick's, snare's and hats' bands.
fn onsets(x: &[f32], sr: f32) -> [Vec<f32>; 4] {
    let bin = sr / ONSET_SIZE as f32;
    let n_bins = ONSET_SIZE / 2 + 1;
    let edges: Vec<f32> = (0..=ONSET_BANDS + 1)
        .map(|i| 30.0 * (11_000.0f32 / 30.0).powf(i as f32 / (ONSET_BANDS + 1) as f32))
        .collect();
    let band_of: Vec<Option<usize>> = (0..n_bins)
        .map(|k| {
            let f = k as f32 * bin;
            (0..ONSET_BANDS).find(|&b| f >= edges[b] && f < edges[b + 2])
        })
        .collect();
    let centre = |b: usize| edges[b + 1];
    let kick: Vec<usize> = (0..ONSET_BANDS).filter(|&b| centre(b) < 150.0).collect();
    let snare: Vec<usize> = (0..ONSET_BANDS)
        .filter(|&b| (1000.0..4000.0).contains(&centre(b)))
        .collect();
    let hats: Vec<usize> = (0..ONSET_BANDS).filter(|&b| centre(b) >= 6000.0).collect();
    let all: Vec<usize> = (0..ONSET_BANDS).collect();
    let mut spectra = Spectra::new(ONSET_SIZE);
    let mut power = vec![0f32; n_bins];
    let count = x.len() / ONSET_HOP + 1;
    let mut out: [Vec<f32>; 4] = std::array::from_fn(|_| Vec::with_capacity(count));
    let mut last = [0f32; ONSET_BANDS];
    for i in 0..count {
        let start = (i * ONSET_HOP) as isize - (ONSET_SIZE / 2) as isize;
        spectra.power(x, start, &mut power);
        let mut bands = [1e-10f32; ONSET_BANDS];
        for (k, p) in power.iter().enumerate() {
            if let Some(b) = band_of[k] {
                bands[b] += p;
            }
        }
        let logs: [f32; ONSET_BANDS] = std::array::from_fn(|b| bands[b].log10());
        for (o, which) in out.iter_mut().zip([&all, &kick, &snare, &hats]) {
            let rise = which
                .iter()
                .map(|&b| (logs[b] - last[b]).max(0.0))
                .sum::<f32>()
                / which.len().max(1) as f32;
            o.push(if i == 0 { 0.0 } else { rise });
        }
        last = logs;
    }
    out
}

/// `x` (mono, at `rate`) taken down near 22 kHz by an integer step, averaging what's dropped.
fn decimate(x: &[f32], rate: f32) -> (Vec<f32>, f32) {
    let step = ((rate / WORK_RATE).floor() as usize).max(1);
    if step == 1 {
        return (x.to_vec(), rate);
    }
    let out = x
        .chunks(step)
        .map(|c| c.iter().sum::<f32>() / c.len() as f32)
        .collect();
    (out, rate / step as f32)
}

/// Where the song's notes sit between semitones, off A440 (-0.5 to 0.5): the mean, round the
/// circle, of where its spectral peaks between 150 Hz and 2.5 kHz fall (each peak placed by the
/// parabola through it), every fourth frame.
fn tuning(x: &[f32], sr: f32, spectra: &mut Spectra, power: &mut [f32]) -> f32 {
    let bin = sr / SIZE as f32;
    let (lo, hi) = (
        (150.0 / bin) as usize,
        ((2500.0 / bin) as usize).min(power.len() - 2),
    );
    let (mut c, mut s) = (0f64, 0f64);
    for i in (0..x.len() / HOP + 1).step_by(4) {
        spectra.power(x, (i * HOP) as isize - (SIZE / 2) as isize, power);
        for k in lo.max(1)..hi {
            if power[k] > power[k - 1] && power[k] >= power[k + 1] && power[k] > 1e-10 {
                let (a, b, cc) = (
                    power[k - 1].max(1e-20).ln(),
                    power[k].ln(),
                    power[k + 1].max(1e-20).ln(),
                );
                let bend = a - 2.0 * b + cc;
                let d = if bend < 0.0 {
                    (0.5 * (a - cc) / bend).clamp(-0.5, 0.5)
                } else {
                    0.0
                };
                let midi = 69.0 + 12.0 * ((k as f32 + d) * bin / 440.0).log2();
                let angle = std::f64::consts::TAU * (midi - midi.round()) as f64;
                let w = power[k].sqrt() as f64;
                c += w * angle.cos();
                s += w * angle.sin();
            }
        }
    }
    if c == 0.0 && s == 0.0 {
        0.0
    } else {
        (s.atan2(c) / std::f64::consts::TAU) as f32
    }
}

pub fn frames(mono: &[f32], rate: f32) -> Frames {
    let (x, sr) = decimate(mono, rate);
    let bin = sr / SIZE as f32;
    let n_bins = SIZE / 2 + 1;
    let mut spectra = Spectra::new(SIZE);
    let mut power = vec![0f32; n_bins];
    let tune = tuning(&x, sr, &mut spectra, &mut power);
    // Log-spaced bands, 40 Hz to 10 kHz, as weights over bins.
    let edges: Vec<f32> = (0..=BANDS + 1)
        .map(|i| 40.0 * (10_000.0f32 / 40.0).powf(i as f32 / (BANDS + 1) as f32))
        .collect();
    let band_of: Vec<Option<usize>> = (0..n_bins)
        .map(|k| {
            let f = k as f32 * bin;
            (0..BANDS)
                .find(|&b| f >= edges[b] && f < edges[b + 2])
                .filter(|_| f >= 40.0)
        })
        .collect();
    // Bins to pitch classes.
    let class_of: Vec<Option<(usize, bool)>> = (0..n_bins)
        .map(|k| {
            let f = k as f32 * bin;
            if !(27.0..4000.0).contains(&f) {
                return None;
            }
            let midi = 69.0 + 12.0 * (f / 440.0).log2() - tune;
            Some(((midi.round() as i32).rem_euclid(12) as usize, f < 250.0))
        })
        .collect();
    let count = x.len() / HOP + 1;
    let [onset, kick, snare, hats] = onsets(&x, sr);
    let mut out = Frames {
        rate: sr / HOP as f32,
        onset_rate: sr / ONSET_HOP as f32,
        onset,
        kick,
        snare,
        hats,
        chroma: Vec::with_capacity(count),
        bass: Vec::with_capacity(count),
        timbre: Vec::with_capacity(count),
        level: Vec::with_capacity(count),
        tuning: tune * 100.0,
    };
    // The DCT that turns band energies into cepstra.
    let dct: Vec<[f32; BANDS]> = (1..=TIMBRE)
        .map(|c| {
            std::array::from_fn(|b| {
                (std::f32::consts::PI * c as f32 * (b as f32 + 0.5) / BANDS as f32).cos()
            })
        })
        .collect();
    for i in 0..count {
        let start = (i * HOP) as isize - (SIZE / 2) as isize;
        spectra.power(&x, start, &mut power);
        let mut bands = [1e-10f32; BANDS];
        for (k, p) in power.iter().enumerate() {
            if let Some(b) = band_of[k] {
                bands[b] += p;
            }
        }
        let logs: [f32; BANDS] = std::array::from_fn(|b| (bands[b]).log10());
        // Pitch classes from the spectrum's peaks only: a peak is tonal, the floor between
        // isn't.
        let mut chroma = [0f32; 12];
        let mut bass = [0f32; 12];
        for k in 1..n_bins - 1 {
            if let Some((pc, low)) = class_of[k]
                && power[k] > power[k - 1]
                && power[k] >= power[k + 1]
            {
                let m = power[k].sqrt();
                chroma[pc] += m;
                if low {
                    bass[pc] += m;
                }
            }
        }
        out.chroma.push(chroma);
        out.bass.push(bass);
        let timbre: [f32; TIMBRE] = std::array::from_fn(|c| {
            (0..BANDS).map(|b| dct[c][b] * logs[b]).sum::<f32>() / BANDS as f32
        });
        out.timbre.push(timbre);
        let total: f32 = power.iter().sum();
        out.level.push(10.0 * total.max(1e-12).log10());
    }
    out
}

/// A curve with its slow drift taken away (each value less the mean around it), and what's left
/// under zero cut: what stands out from where it is.
pub fn peaks_of(x: &[f32], width: usize) -> Vec<f32> {
    let n = x.len();
    let mut sum = vec![0f32; n + 1];
    for i in 0..n {
        sum[i + 1] = sum[i] + x[i];
    }
    (0..n)
        .map(|i| {
            let (a, b) = (i.saturating_sub(width), (i + width + 1).min(n));
            let mean = (sum[b] - sum[a]) / (b - a) as f32;
            (x[i] - mean).max(0.0)
        })
        .collect()
}

/// The mean of `rows` from `from` to `to` (a frame range).
pub fn mean<const N: usize>(rows: &[[f32; N]], from: usize, to: usize) -> [f32; N] {
    let (a, b) = (from.min(rows.len()), to.min(rows.len()));
    let mut out = [0f32; N];
    if b <= a {
        return out;
    }
    for r in &rows[a..b] {
        for i in 0..N {
            out[i] += r[i];
        }
    }
    out.map(|v| v / (b - a) as f32)
}
