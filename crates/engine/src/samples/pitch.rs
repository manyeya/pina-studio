//! A sound's pitch: YIN (de Cheveigné and Kawahara, 2002) frame by frame over its sustained
//! part, summed up as the note it settles on, so an 808's glide or a kick's drop doesn't move
//! it.

use std::sync::Arc;

use rustfft::{Fft, FftPlanner, num_complex::Complex32};

/// The lowest pitch looked for: under the lowest 808s and subs (G0 is 24.5 Hz).
pub const MIN_HZ: f32 = 20.0;
/// The highest: about C7.
pub const MAX_HZ: f32 = 2100.0;

/// One frame's pitch: its frequency, how periodic it is (0 to 1), and its level (RMS).
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub hz: f32,
    pub clarity: f32,
    pub level: f32,
}

/// What a pitch track comes to.
#[derive(Clone, Copy, Debug, Default)]
pub struct Summary {
    pub hz: f32,
    /// 0 to 1: how much of the sound holds this pitch, and how clearly.
    pub confidence: f32,
    /// How periodic its frames are on average (0 to 1).
    pub clarity: f32,
    /// How far (cents) its first clear frame is from where it settles: an 808's glide.
    pub glide: f32,
    /// How much it wanders around the note (cents, standard deviation).
    pub spread: f32,
}

/// Halves a signal's rate: a 31-tap low-pass (a Blackman-windowed sinc at a little under the new
/// Nyquist), then every other sample.
pub fn decimate(x: &[f32]) -> Vec<f32> {
    const HALF: i32 = 15;
    let taps: Vec<f32> = (-HALF..=HALF)
        .map(|i| {
            let t = i as f32;
            let cutoff = 0.45;
            let sinc = if i == 0 {
                2.0 * cutoff * 0.5
            } else {
                (std::f32::consts::PI * cutoff * t).sin() / (std::f32::consts::PI * t)
            };
            let w = 0.42
                + 0.5 * (std::f32::consts::PI * t / (HALF + 1) as f32).cos()
                + 0.08 * (2.0 * std::f32::consts::PI * t / (HALF + 1) as f32).cos();
            sinc * w
        })
        .collect();
    let sum: f32 = taps.iter().sum();
    (0..x.len() / 2)
        .map(|o| {
            let c = (o * 2) as i32;
            let mut acc = 0.0;
            for (k, t) in taps.iter().enumerate() {
                let i = c + k as i32 - HALF;
                if i >= 0 && (i as usize) < x.len() {
                    acc += t * x[i as usize];
                }
            }
            acc / sum
        })
        .collect()
}

/// YIN over frames of one signal.
pub struct Yin {
    rate: f32,
    window: usize,
    min_lag: usize,
    max_lag: usize,
    size: usize,
    forward: Arc<dyn Fft<f32>>,
    inverse: Arc<dyn Fft<f32>>,
    a: Vec<Complex32>,
    b: Vec<Complex32>,
    scratch: Vec<Complex32>,
}

impl Yin {
    /// For a signal at `rate`, looking for pitches from `MIN_HZ` to `MAX_HZ`. Frames are two of
    /// the longest periods long.
    pub fn new(rate: f32) -> Yin {
        let max_lag = (rate / MIN_HZ).ceil() as usize;
        let min_lag = ((rate / MAX_HZ).floor() as usize).max(2);
        let window = 2 * max_lag;
        let size = (window + max_lag + 1).next_power_of_two();
        let mut planner = FftPlanner::new();
        let forward = planner.plan_fft_forward(size);
        let inverse = planner.plan_fft_inverse(size);
        let scratch_len = forward
            .get_inplace_scratch_len()
            .max(inverse.get_inplace_scratch_len());
        Yin {
            rate,
            window,
            min_lag,
            max_lag,
            size,
            forward,
            inverse,
            a: vec![Complex32::default(); size],
            b: vec![Complex32::default(); size],
            scratch: vec![Complex32::default(); scratch_len],
        }
    }

    /// How many samples a frame takes.
    pub fn span(&self) -> usize {
        self.window + self.max_lag
    }

    /// The pitch of the frame of `x` starting at `start` (zero past its end).
    pub fn frame(&mut self, x: &[f32], start: usize) -> Frame {
        let span = self.span();
        let get = |i: usize| x.get(start + i).copied().unwrap_or(0.0);
        for i in 0..self.size {
            self.a[i] = Complex32::new(if i < self.window { get(i) } else { 0.0 }, 0.0);
            self.b[i] = Complex32::new(if i < span { get(i) } else { 0.0 }, 0.0);
        }
        self.forward
            .process_with_scratch(&mut self.a, &mut self.scratch);
        self.forward
            .process_with_scratch(&mut self.b, &mut self.scratch);
        for (a, b) in self.a.iter_mut().zip(&self.b) {
            *a = a.conj() * b;
        }
        self.inverse
            .process_with_scratch(&mut self.a, &mut self.scratch);
        // r(τ) = Σ x[j] x[j + τ] over the window, from the cross-correlation just taken.
        let norm = 1.0 / self.size as f32;
        let mut energy = vec![0f32; span + 1];
        for i in 0..span {
            energy[i + 1] = energy[i] + get(i) * get(i);
        }
        let e0 = energy[self.window];
        let level = (e0 / self.window as f32).sqrt();
        if e0 <= 1e-12 {
            return Frame {
                hz: 0.0,
                clarity: 0.0,
                level,
            };
        }
        // The cumulative-mean-normalised difference.
        let mut cmnd = vec![1f32; self.max_lag + 1];
        let mut running = 0.0;
        for tau in 1..=self.max_lag {
            let e_tau = energy[tau + self.window] - energy[tau];
            let d = (e0 + e_tau - 2.0 * self.a[tau].re * norm).max(0.0);
            running += d;
            cmnd[tau] = if running > 0.0 {
                d * tau as f32 / running
            } else {
                1.0
            };
        }
        // The first dip under the threshold (the shortest period that fits, so no subharmonic),
        // else the deepest.
        const THRESHOLD: f32 = 0.2;
        let mut best = None;
        let mut tau = self.min_lag;
        while tau < self.max_lag {
            if cmnd[tau] < THRESHOLD {
                while tau + 1 < self.max_lag && cmnd[tau + 1] < cmnd[tau] {
                    tau += 1;
                }
                best = Some(tau);
                break;
            }
            tau += 1;
        }
        let tau = best.unwrap_or_else(|| {
            (self.min_lag..self.max_lag)
                .min_by(|&a, &b| cmnd[a].total_cmp(&cmnd[b]))
                .unwrap_or(self.min_lag)
        });
        let shift = if tau > 0 && tau < self.max_lag {
            crate::samples::dsp::parabolic(cmnd[tau - 1], cmnd[tau], cmnd[tau + 1])
        } else {
            0.0
        };
        Frame {
            hz: self.rate / (tau as f32 + shift),
            clarity: (1.0 - cmnd[tau]).clamp(0.0, 1.0),
            level,
        }
    }
}

/// The pitch of `x` (at `rate`) between `from` and `to` (samples), up to `frames` frames evenly
/// spread.
pub fn track(x: &[f32], rate: f32, from: usize, to: usize, frames: usize) -> Vec<Frame> {
    // Down to 16-32 kHz: plenty for pitches up to 2 kHz, and quicker.
    let (mut signal, mut rate) = (x[from.min(x.len())..to.min(x.len())].to_vec(), rate);
    while rate > 32_000.0 {
        signal = decimate(&signal);
        rate /= 2.0;
    }
    let mut yin = Yin::new(rate);
    let span = yin.span();
    if signal.len() < span / 2 {
        return Vec::new();
    }
    let room = signal.len().saturating_sub(span);
    let count = frames.min(room / (span / 8).max(1) + 1).max(1);
    (0..count)
        .map(|i| {
            let start = if count == 1 {
                0
            } else {
                i * room / (count - 1)
            };
            yin.frame(&signal, start)
        })
        .collect()
}

/// A pitch in MIDI notes (69 is A4), fractional.
pub fn midi(hz: f32) -> f32 {
    69.0 + 12.0 * (hz / 440.0).log2()
}

/// The note a track settles on: the pitch most of its clear frames agree on (within 40 cents),
/// weighted by how clear and loud they are.
pub fn summarize(frames: &[Frame]) -> Option<Summary> {
    let voiced: Vec<(f32, f32, f32)> = frames
        .iter()
        .filter(|f| f.clarity > 0.65 && f.hz > 0.0)
        .map(|f| {
            (
                midi(f.hz) * 100.0,
                f.clarity * f.clarity * f.level,
                f.clarity,
            )
        })
        .collect();
    if voiced.is_empty() {
        return None;
    }
    let all: f32 = frames
        .iter()
        .map(|f| f.clarity.max(0.3).powi(2) * f.level)
        .sum();
    let support = |c: f32| -> f32 {
        voiced
            .iter()
            .filter(|v| (v.0 - c).abs() < 40.0)
            .map(|v| v.1)
            .sum()
    };
    let center = voiced
        .iter()
        .map(|v| v.0)
        .max_by(|&a, &b| support(a).total_cmp(&support(b)))?;
    let near: Vec<&(f32, f32, f32)> = voiced
        .iter()
        .filter(|v| (v.0 - center).abs() < 40.0)
        .collect();
    let weight: f32 = near.iter().map(|v| v.1).sum();
    if weight <= 0.0 {
        return None;
    }
    let cents = near.iter().map(|v| v.0 * v.1).sum::<f32>() / weight;
    let clarity = near.iter().map(|v| v.2 * v.1).sum::<f32>() / weight;
    let total: f32 = voiced.iter().map(|v| v.1).sum();
    let spread = (voiced
        .iter()
        .map(|v| v.1 * ((v.0 - cents).clamp(-1200.0, 1200.0)).powi(2))
        .sum::<f32>()
        / total)
        .sqrt();
    Some(Summary {
        hz: 440.0 * 2f32.powf((cents / 100.0 - 69.0) / 12.0),
        confidence: (weight / all.max(1e-12)).min(1.0) * clarity,
        clarity,
        glide: voiced[0].0 - cents,
        spread,
    })
}

#[cfg(test)]
mod tests;
