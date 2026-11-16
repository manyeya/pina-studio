//! The short-time Fourier transform and its inverse, as separation models were trained on them
//! (`torch.stft` and `torch.istft`): a periodic Hann window, centred frames (the sound reflected
//! out half a window at each end), one-sided, unnormalised.

use std::sync::Arc;

use rustfft::{Fft, FftPlanner, num_complex::Complex32};

pub struct Stft {
    n: usize,
    hop: usize,
    window: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
}

/// `x` reflected out `by` samples at each end (its first and last samples not repeated), or
/// silence where it's too short to reflect.
pub fn reflect(x: &[f32], by: usize) -> Vec<f32> {
    let n = x.len();
    let at = |i: isize| -> f32 {
        if n > by {
            let i = if i < 0 { -i } else { i };
            let i = i as usize;
            x[if i >= n { 2 * (n - 1) - i } else { i }]
        } else if i < 0 {
            0.0
        } else {
            x.get(i as usize).copied().unwrap_or(0.0)
        }
    };
    (-(by as isize)..(n + by) as isize).map(at).collect()
}

impl Stft {
    pub fn new(n: usize, hop: usize) -> Stft {
        let mut planner = FftPlanner::new();
        let window = (0..n)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos())
            .collect();
        Stft {
            n,
            hop: hop.max(1),
            window,
            fft: planner.plan_fft_forward(n),
            ifft: planner.plan_fft_inverse(n),
        }
    }

    /// Its bins: n / 2 + 1.
    pub fn bins(&self) -> usize {
        self.n / 2 + 1
    }

    /// Frames for `len` samples.
    pub fn frames(&self, len: usize) -> usize {
        len / self.hop + 1
    }

    /// `x`'s spectrum, frame by frame (each its `bins`).
    pub fn forward(&self, x: &[f32]) -> Vec<Vec<Complex32>> {
        let padded = reflect(x, self.n / 2);
        let mut buf = vec![Complex32::default(); self.n];
        (0..self.frames(x.len()))
            .map(|t| {
                for (i, b) in buf.iter_mut().enumerate() {
                    let s = padded.get(t * self.hop + i).copied().unwrap_or(0.0);
                    *b = Complex32::new(s * self.window[i], 0.0);
                }
                self.fft.process(&mut buf);
                buf[..self.bins()].to_vec()
            })
            .collect()
    }

    /// The sound `len` samples long whose spectrum is `spec` (frame by frame, each its `bins`).
    pub fn inverse(&self, spec: &[Vec<Complex32>], len: usize) -> Vec<f32> {
        let (n, half) = (self.n, self.n / 2);
        let total = n + self.hop * spec.len().saturating_sub(1);
        let mut y = vec![0f32; total];
        let mut weight = vec![0f32; total];
        let mut buf = vec![Complex32::default(); n];
        for (t, frame) in spec.iter().enumerate() {
            for (f, b) in buf.iter_mut().enumerate() {
                // The other half mirrors the first, as a real sound's does.
                *b = match f <= half {
                    true => frame[f],
                    false => frame[n - f].conj(),
                };
            }
            buf[0].im = 0.0;
            buf[half].im = 0.0;
            self.ifft.process(&mut buf);
            let at = t * self.hop;
            for i in 0..n {
                let w = self.window[i];
                y[at + i] += buf[i].re / n as f32 * w;
                weight[at + i] += w * w;
            }
        }
        (0..len)
            .map(|i| {
                let k = i + half;
                match weight.get(k) {
                    Some(&w) if w > 1e-8 => y[k] / w,
                    _ => 0.0,
                }
            })
            .collect()
    }
}
