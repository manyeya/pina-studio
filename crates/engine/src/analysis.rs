//! Measuring audio, for agents, which can't listen: level, loudness and tonal balance.

use serde::Serialize;

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Analysis {
    pub seconds: f64,
    /// Per channel, left and right, in dBFS.
    pub peak_db: [f32; 2],
    pub rms_db: [f32; 2],
    /// Integrated loudness, ITU-R BS.1770 (gated), in LUFS. -70 or below means silence.
    pub lufs: f32,
    /// Samples at or past full scale.
    pub clipped: usize,
    /// Energy below 250 Hz, between, and above 4 kHz, in dB relative to the total.
    pub bands_db: Bands,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Bands {
    pub low: f32,
    pub mid: f32,
    pub high: f32,
}

const FLOOR: f32 = -144.0;

fn wide(x: &[f32]) -> Vec<f64> {
    x.iter().map(|&v| v as f64).collect()
}

fn to_db(power: f64) -> f32 {
    if power <= 0.0 {
        FLOOR
    } else {
        (10.0 * power.log10()).max(FLOOR as f64) as f32
    }
}

/// A biquad filter, run over a whole signal.
#[derive(Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
}

impl Biquad {
    fn run(&self, x: &[f64]) -> Vec<f64> {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        x.iter()
            .map(|&x0| {
                let y0 = self.b[0] * x0 + self.b[1] * x1 + self.b[2] * x2
                    - self.a[0] * y1
                    - self.a[1] * y2;
                (x2, x1, y2, y1) = (x1, x0, y1, y0);
                y0
            })
            .collect()
    }

    /// RBJ cookbook low- or high-pass, Q 0.707.
    fn pass(rate: f64, freq: f64, high: bool) -> Biquad {
        let w = std::f64::consts::TAU * freq / rate;
        let alpha = w.sin() / (2.0 * std::f64::consts::FRAC_1_SQRT_2);
        let c = w.cos();
        let a0 = 1.0 + alpha;
        let (b0, b1) = if high {
            ((1.0 + c) / 2.0, -(1.0 + c))
        } else {
            ((1.0 - c) / 2.0, 1.0 - c)
        };
        Biquad {
            b: [b0 / a0, b1 / a0, b0 / a0],
            a: [-2.0 * c / a0, (1.0 - alpha) / a0],
        }
    }
}

/// BS.1770's K-weighting: a high shelf for the head, then a high-pass, at any sample rate (as
/// libebur128 derives them).
fn k_weighting(rate: f64) -> [Biquad; 2] {
    let (f0, gain, q) = (1681.974450955533, 3.999843853973347, 0.7071752369554196);
    let k = (std::f64::consts::PI * f0 / rate).tan();
    let vh = 10f64.powf(gain / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Biquad {
        b: [
            (vh + vb * k / q + k * k) / a0,
            2.0 * (k * k - vh) / a0,
            (vh - vb * k / q + k * k) / a0,
        ],
        a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    };
    let (f0, q) = (38.13547087602444, 0.5003270373238773);
    let k = (std::f64::consts::PI * f0 / rate).tan();
    let a0 = 1.0 + k / q + k * k;
    let highpass = Biquad {
        b: [1.0, -2.0, 1.0],
        a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    };
    [shelf, highpass]
}

/// Integrated loudness: K-weighted power in 400 ms blocks, 75% overlapped, gated at -70 LUFS and
/// then 10 LU under the ungated loudness.
fn loudness(l: &[f32], r: &[f32], rate: f64) -> f32 {
    let [shelf, highpass] = k_weighting(rate);
    let weigh = |x: &[f32]| highpass.run(&shelf.run(&wide(x)));
    let (wl, wr) = (weigh(l), weigh(r));
    let (block, step) = ((0.4 * rate) as usize, (0.1 * rate) as usize);
    if wl.len() < block {
        return -70.0;
    }
    let power: Vec<f64> = (0..=(wl.len() - block) / step)
        .map(|i| {
            let s = i * step;
            let sum: f64 = (s..s + block).map(|n| wl[n] * wl[n] + wr[n] * wr[n]).sum();
            sum / block as f64
        })
        .collect();
    let lufs = |p: f64| -0.691 + 10.0 * p.log10();
    let mean = |gate: f64| {
        let kept: Vec<f64> = power
            .iter()
            .copied()
            .filter(|&p| p > 0.0 && lufs(p) > gate)
            .collect();
        (!kept.is_empty()).then(|| kept.iter().sum::<f64>() / kept.len() as f64)
    };
    let Some(ungated) = mean(-70.0) else {
        return -70.0;
    };
    let gated = mean(lufs(ungated) - 10.0).unwrap_or(ungated);
    (lufs(gated) as f32).max(-70.0)
}

pub fn analyze(l: &[f32], r: &[f32], rate: f64) -> Analysis {
    let per = |x: &[f32]| {
        let peak = x.iter().fold(0f32, |m, s| m.max(s.abs()));
        let power = x.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64;
        (to_db((peak as f64).powi(2)), to_db(power))
    };
    let ((pl, rl), (pr, rr)) = (per(l), per(r));
    let mono: Vec<f64> = l
        .iter()
        .zip(r)
        .map(|(&a, &b)| (a as f64 + b as f64) / 2.0)
        .collect();
    let energy = |x: &[f64]| x.iter().map(|v| v * v).sum::<f64>();
    let total = energy(&mono);
    let low = energy(&Biquad::pass(rate, 250.0, false).run(&mono));
    let high = energy(&Biquad::pass(rate, 4000.0, true).run(&mono));
    let relative = |e: f64| if total > 0.0 { to_db(e / total) } else { FLOOR };
    Analysis {
        seconds: l.len() as f64 / rate,
        peak_db: [pl, pr],
        rms_db: [rl, rr],
        lufs: loudness(l, r, rate),
        clipped: l.iter().chain(r).filter(|s| s.abs() >= 1.0).count(),
        bands_db: Bands {
            low: relative(low),
            mid: relative((total - low - high).max(0.0)),
            high: relative(high),
        },
    }
}

/// Reads a WAV file's first two channels.
pub fn read_wav(path: &std::path::Path) -> Result<(Vec<f32>, Vec<f32>, f64), String> {
    let err = |e: hound::Error| format!("{}: {e}", path.display());
    let mut wav = hound::WavReader::open(path).map_err(err)?;
    let spec = wav.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => wav
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(err)?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            wav.samples::<i32>()
                .map(|s| s.map(|s| s as f32 * scale))
                .collect::<Result<_, _>>()
                .map_err(err)?
        }
    };
    let n = spec.channels as usize;
    let channel = |c: usize| {
        samples
            .iter()
            .skip(c.min(n - 1))
            .step_by(n)
            .copied()
            .collect::<Vec<_>>()
    };
    Ok((channel(0), channel(1), spec.sample_rate as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;

    fn sine(freq: f64, amp: f32, seconds: f64) -> Vec<f32> {
        (0..(seconds * RATE) as usize)
            .map(|i| amp * (std::f64::consts::TAU * freq * i as f64 / RATE).sin() as f32)
            .collect()
    }

    #[test]
    fn a_full_scale_sine_in_one_channel_is_minus_3_lufs() {
        // BS.1770's own check: a 0 dBFS 997 Hz sine in one channel reads -3.01 LUFS.
        let tone = sine(997.0, 1.0, 5.0);
        let silence = vec![0.0; tone.len()];
        let a = analyze(&tone, &silence, RATE);
        assert!((a.lufs + 3.01).abs() < 0.05, "{}", a.lufs);
        assert!(a.peak_db[0].abs() < 0.01);
        assert!((a.rms_db[0] + 3.01).abs() < 0.01); // a sine's RMS is 3 dB under its peak
        assert_eq!(a.peak_db[1], FLOOR);
        assert_eq!(analyze(&silence, &silence, RATE).lufs, -70.0);
    }

    #[test]
    fn bands_say_where_the_energy_is() {
        let bass = sine(80.0, 0.5, 1.0);
        let a = analyze(&bass, &bass, RATE);
        assert!(
            a.bands_db.low > -1.0 && a.bands_db.high < -40.0,
            "{:?}",
            a.bands_db
        );
        let hiss = sine(9_000.0, 0.5, 1.0);
        let a = analyze(&hiss, &hiss, RATE);
        assert!(
            a.bands_db.high > -1.0 && a.bands_db.low < -40.0,
            "{:?}",
            a.bands_db
        );
    }

    #[test]
    fn clipping_is_counted() {
        let loud: Vec<f32> = sine(100.0, 1.5, 0.1);
        let a = analyze(&loud, &loud, RATE);
        assert!(a.clipped > 0 && a.peak_db[0] > 3.0);
    }
}
