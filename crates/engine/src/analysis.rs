//! Measuring audio, for agents, which can't listen: level, loudness, dynamics, tonal balance,
//! stereo, and the notes sounding.

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
    /// Finer: energy in six bands, in dB relative to the total.
    pub spectrum_db: Spectrum,
    /// The loudest 3 seconds (short-term loudness), in LUFS.
    pub loudest_lufs: f32,
    /// Peak over RMS, in dB: how much the loudest moments stand out (a squashed mix is small).
    pub crest_db: f32,
    pub stereo: Stereo,
    /// The notes sounding (by pitch class, C to B), each its share of the tonal energy in
    /// percent, and the key they fit best.
    pub notes: Notes,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Spectrum {
    /// Under 60 Hz: sub, an 808's weight.
    pub sub: f32,
    /// 60-250 Hz: bass and kick.
    pub bass: f32,
    /// 250 Hz-1 kHz: body, and mud.
    pub low_mid: f32,
    /// 1-4 kHz: presence, the ear's most sensitive range.
    pub mid: f32,
    /// 4-8 kHz: bite, sibilance.
    pub presence: f32,
    /// Over 8 kHz: air.
    pub air: f32,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Stereo {
    /// 1 is mono, 0 unrelated sides, below 0 sides that cancel when summed to mono.
    pub correlation: f32,
    /// The sides' energy against the middle's, in dB: very low is near mono.
    pub width_db: f32,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Notes {
    pub classes: [(String, f32); 12],
    /// Like "C minor"; empty when there's nothing tonal to go on.
    pub key: String,
    /// How well it fits, -1 to 1 (above 0.7 is a clear key).
    pub key_fit: f32,
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
pub(crate) struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
}

impl Biquad {
    pub(crate) fn run(&self, x: &[f64]) -> Vec<f64> {
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
pub(crate) fn k_weighting(rate: f64) -> [Biquad; 2] {
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

/// The FFT's twiddles for `n` points: `e^(-2πik/n)` for k below n/2.
fn twiddles(n: usize) -> Vec<(f64, f64)> {
    (0..n / 2)
        .map(|k| {
            let (s, c) = (-std::f64::consts::TAU * k as f64 / n as f64).sin_cos();
            (c, s)
        })
        .collect()
}

/// An in-place radix-2 FFT of `re` and `im` (their length a power of two, `twiddles` for it).
fn fft(re: &mut [f64], im: &mut [f64], twiddles: &[(f64, f64)]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let stride = n / len;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (c, s) = twiddles[k * stride];
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                (re[b], im[b]) = (re[a] - tr, im[a] - ti);
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

/// The power spectrum averaged over Hann windows of `N` samples, half overlapped: bin `k` is
/// `k * rate / N` Hz.
const N: usize = 8192;

fn power_spectrum(x: &[f64]) -> Vec<f64> {
    let mut sum = vec![0.0; N / 2];
    let hann: Vec<f64> = (0..N)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / N as f64).cos())
        .collect();
    let turns = twiddles(N);
    let mut frames = 0;
    let mut start = 0;
    while start + N <= x.len() || (frames == 0 && !x.is_empty()) {
        let mut re: Vec<f64> = (0..N)
            .map(|i| x.get(start + i).copied().unwrap_or(0.0) * hann[i])
            .collect();
        let mut im = vec![0.0; N];
        fft(&mut re, &mut im, &turns);
        for k in 0..N / 2 {
            sum[k] += re[k] * re[k] + im[k] * im[k];
        }
        frames += 1;
        start += N / 2;
    }
    sum.iter_mut().for_each(|p| *p /= frames.max(1) as f64);
    sum
}

/// Major and minor key profiles (Krumhansl and Kessler), from the tonic up.
const MAJOR: [f64; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const MINOR: [f64; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];
const NAMES: [&str; 12] = [
    "C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
];

/// The pitch classes in a power spectrum (55 Hz to 4 kHz, where notes are clear), and the key
/// they fit best.
fn notes(spectrum: &[f64], rate: f64) -> Notes {
    let mut classes = [0f64; 12];
    for (k, &p) in spectrum.iter().enumerate() {
        let f = k as f64 * rate / N as f64;
        if (55.0..4000.0).contains(&f) {
            let midi = (69.0 + 12.0 * (f / 440.0).log2()).round() as i64;
            classes[midi.rem_euclid(12) as usize] += p.sqrt();
        }
    }
    let total: f64 = classes.iter().sum();
    let corr = |a: &[f64; 12], b: &[f64]| {
        let (ma, mb) = (a.iter().sum::<f64>() / 12.0, b.iter().sum::<f64>() / 12.0);
        let (mut num, mut da, mut db) = (0.0, 0.0, 0.0);
        for i in 0..12 {
            num += (a[i] - ma) * (b[i] - mb);
            da += (a[i] - ma).powi(2);
            db += (b[i] - mb).powi(2);
        }
        if da > 0.0 && db > 0.0 {
            num / (da * db).sqrt()
        } else {
            0.0
        }
    };
    let mut best = (f64::MIN, String::new());
    if total > 0.0 {
        for tonic in 0..12 {
            for (profile, mode) in [(&MAJOR, "major"), (&MINOR, "minor")] {
                let rotated: Vec<f64> = (0..12).map(|i| profile[(i + 12 - tonic) % 12]).collect();
                let c = corr(&classes, &rotated);
                if c > best.0 {
                    best = (c, format!("{} {mode}", NAMES[tonic]));
                }
            }
        }
    }
    Notes {
        classes: std::array::from_fn(|i| {
            let share = if total > 0.0 {
                classes[i] / total * 100.0
            } else {
                0.0
            };
            (NAMES[i].to_string(), (share * 10.0).round() as f32 / 10.0)
        }),
        key: best.1,
        key_fit: if total > 0.0 { best.0 as f32 } else { 0.0 },
    }
}

/// The loudest short-term (3 s) loudness, stepping a second at a time.
fn loudest(l: &[f32], r: &[f32], rate: f64) -> f32 {
    let (window, step) = ((3.0 * rate) as usize, rate as usize);
    if l.len() <= window {
        return loudness(l, r, rate);
    }
    (0..=(l.len() - window) / step)
        .map(|i| {
            loudness(
                &l[i * step..i * step + window],
                &r[i * step..i * step + window],
                rate,
            )
        })
        .fold(-70.0, f32::max)
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
    let spectrum = power_spectrum(&mono);
    let band = |lo: f64, hi: f64| {
        let all: f64 = spectrum.iter().sum();
        let part: f64 = spectrum
            .iter()
            .enumerate()
            .filter(|(k, _)| (lo..hi).contains(&(*k as f64 * rate / N as f64)))
            .map(|(_, p)| p)
            .sum();
        if all > 0.0 { to_db(part / all) } else { FLOOR }
    };
    let (mut lr, mut ll, mut rr2, mut side) = (0.0, 0.0, 0.0, 0.0);
    for (&a, &b) in l.iter().zip(r) {
        let (a, b) = (a as f64, b as f64);
        lr += a * b;
        ll += a * a;
        rr2 += b * b;
        side += ((a - b) / 2.0).powi(2);
    }
    let mid = energy(&mono);
    let peak = l.iter().chain(r).fold(0f32, |m, s| m.max(s.abs()));
    let rms = ((ll + rr2) / (2 * l.len()).max(1) as f64).sqrt();
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
        spectrum_db: Spectrum {
            sub: band(0.0, 60.0),
            bass: band(60.0, 250.0),
            low_mid: band(250.0, 1000.0),
            mid: band(1000.0, 4000.0),
            presence: band(4000.0, 8000.0),
            air: band(8000.0, rate),
        },
        loudest_lufs: loudest(l, r, rate),
        crest_db: if rms > 0.0 {
            (20.0 * (peak as f64 / rms).log10()) as f32
        } else {
            0.0
        },
        stereo: Stereo {
            correlation: if ll > 0.0 && rr2 > 0.0 {
                (lr / (ll * rr2).sqrt()) as f32
            } else {
                1.0
            },
            width_db: if mid > 0.0 { to_db(side / mid) } else { FLOOR },
        },
        notes: notes(&spectrum, rate),
    }
}

/// Reads a WAV file's first two channels.
/// Stereo audio to a 32-bit float WAV file, written whole or not at all.
pub fn write_wav(path: &std::path::Path, l: &[f32], r: &[f32], rate: u32) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let part = path.with_extension("wav.part");
    let err = |e: hound::Error| e.to_string();
    let mut w = hound::WavWriter::create(&part, spec).map_err(err)?;
    for (a, b) in l.iter().zip(r) {
        w.write_sample(*a).map_err(err)?;
        w.write_sample(*b).map_err(err)?;
    }
    w.finalize().map_err(err)?;
    std::fs::rename(&part, path).map_err(|e| e.to_string())
}

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
    fn the_spectrum_stereo_and_notes_say_what_is_playing() {
        // An A minor chord, A C E, with an 808-ish 50 Hz under it.
        let chord: Vec<f32> = (0..RATE as usize * 2)
            .map(|i| {
                let t = i as f64 / RATE;
                let n = |f: f64| (std::f64::consts::TAU * f * t).sin();
                (0.2 * (n(220.0) + n(261.63) + n(329.63)) + 0.3 * n(50.0)) as f32
            })
            .collect();
        let a = analyze(&chord, &chord, RATE);
        assert!(
            a.spectrum_db.sub > -6.0 && a.spectrum_db.air < -40.0,
            "{:?}",
            a.spectrum_db
        );
        assert!((a.stereo.correlation - 1.0).abs() < 1e-6 && a.stereo.width_db < -100.0);
        assert!(
            a.notes.key == "A minor" || a.notes.key == "C major",
            "{}",
            a.notes.key
        );
        let share = |name: &str| a.notes.classes.iter().find(|(n, _)| n == name).unwrap().1;
        assert!(share("A") > 15.0 && share("C") > 15.0 && share("E") > 15.0 && share("F#") < 5.0);
        // A sine is 3 dB of crest; opposite sides cancel.
        let tone = sine(440.0, 0.5, 1.0);
        let flipped: Vec<f32> = tone.iter().map(|x| -x).collect();
        let a = analyze(&tone, &flipped, RATE);
        assert!((a.crest_db - 3.01).abs() < 0.05, "{}", a.crest_db);
        assert!(a.stereo.correlation < -0.99);
    }

    #[test]
    fn clipping_is_counted() {
        let loud: Vec<f32> = sine(100.0, 1.5, 0.1);
        let a = analyze(&loud, &loud, RATE);
        assert!(a.clipped > 0 && a.peak_db[0] > 3.0);
    }
}
