//! Wavetables: frames of one cycle each, read at any note without aliasing. Each frame is kept at
//! every octave of bandwidth (a mip), its harmonics cut with an FFT, so a high note reads a copy
//! with nothing past Nyquist. They're all made here, by adding harmonics or shaping waves, so a
//! song needs no files; a table is built the first time it's asked for, off the audio thread.

use std::{
    collections::HashMap,
    f32::consts::{PI, TAU},
    sync::{Arc, LazyLock, Mutex},
};

use rustfft::{FftPlanner, num_complex::Complex};

pub const SIZE: usize = 2048;
/// One level for each halving of the harmonics, 1024 down to 1.
pub const MIPS: usize = 11;

pub struct Table {
    pub frames: usize,
    /// Mip by mip, frame by frame, each `SIZE + 1` long: the last repeats the first to read past.
    data: Vec<f32>,
}

impl Table {
    /// From frames of `SIZE` samples each: their DC taken out, each brought to the same peak.
    pub fn new(frames: &[Vec<f32>]) -> Table {
        let n = frames.len().max(1);
        let mut planner = FftPlanner::<f32>::new();
        let (fft, ifft) = (
            planner.plan_fft_forward(SIZE),
            planner.plan_fft_inverse(SIZE),
        );
        let mut data = vec![0.0; MIPS * n * (SIZE + 1)];
        let mut spectrum = vec![Complex::default(); SIZE];
        let mut wave = vec![Complex::default(); SIZE];
        for (f, frame) in frames.iter().enumerate() {
            for (b, x) in spectrum.iter_mut().zip(frame) {
                *b = Complex::new(*x, 0.0);
            }
            fft.process(&mut spectrum);
            spectrum[0] = Complex::default();
            let mut peak = 0f32;
            for mip in 0..MIPS {
                let top = (1024usize >> mip).max(1);
                wave.fill(Complex::default());
                for k in 1..=top.min(SIZE / 2 - 1) {
                    wave[k] = spectrum[k];
                    wave[SIZE - k] = spectrum[SIZE - k];
                }
                ifft.process(&mut wave);
                let at = (mip * n + f) * (SIZE + 1);
                for (i, c) in wave.iter().enumerate() {
                    data[at + i] = c.re / SIZE as f32;
                }
                data[at + SIZE] = data[at];
                // Every copy scaled by the fullest one's peak, so they match.
                if mip == 0 {
                    peak = data[at..at + SIZE]
                        .iter()
                        .fold(0f32, |m, x| m.max(x.abs()))
                        .max(1e-6);
                }
                for x in &mut data[at..at + SIZE + 1] {
                    *x *= 0.9 / peak;
                }
            }
        }
        Table { frames: n, data }
    }

    /// One frame, whole, at full bandwidth: for drawing.
    pub fn frame(&self, f: usize) -> &[f32] {
        let at = f.min(self.frames - 1) * (SIZE + 1);
        &self.data[at..at + SIZE]
    }

    /// The copy of a frame with no harmonic past Nyquist at `inc` cycles a sample.
    #[inline]
    pub fn mip(inc: f32) -> usize {
        let room = (0.5 / inc.abs().max(1e-7)) as u32;
        (10 - (31 - room.max(1).leading_zeros()).min(10)) as usize
    }

    /// Where a mip's frames start, for `read`.
    #[inline]
    pub fn level(&self, mip: usize) -> &[f32] {
        &self.data[mip * self.frames * (SIZE + 1)..(mip + 1) * self.frames * (SIZE + 1)]
    }
}

/// A frame of a level, one sample over so a read can run past its end.
pub type Wave = [f32; SIZE + 1];

/// Frame `f` of a level.
#[inline]
pub fn wave(level: &[f32], f: usize) -> &Wave {
    level[f * (SIZE + 1)..][..SIZE + 1].try_into().unwrap()
}

/// `turns` (a phase, the whole of a u32 one cycle) read into `out`: `a` and `b` two neighbouring
/// frames, `mix` between. The index can't leave the frame, so nothing is checked per sample.
#[inline]
pub fn read(a: &Wave, b: &Wave, mix: f32, turns: &[u32], out: &mut [f32]) {
    const SHIFT: u32 = 32 - SIZE.trailing_zeros();
    const PART: f32 = 1.0 / (1u32 << SHIFT) as f32;
    let at = |q: u32| {
        (
            (q >> SHIFT) as usize,
            (q << (32 - SHIFT) >> (32 - SHIFT)) as f32 * PART,
        )
    };
    if mix == 0.0 {
        for (q, o) in turns.iter().zip(out) {
            let (i, t) = at(*q);
            *o = a[i] + (a[i + 1] - a[i]) * t;
        }
        return;
    }
    for (q, o) in turns.iter().zip(out) {
        let (i, t) = at(*q);
        let one = a[i] + (a[i + 1] - a[i]) * t;
        let two = b[i] + (b[i + 1] - b[i]) * t;
        *o = one + (two - one) * mix;
    }
}

/// A sawtooth's harmonic.
fn saw(k: usize) -> f32 {
    1.0 / k as f32
}

/// `n` frames, each by a formula over its place `t` (0 to 1) and the phase `x` (0 to 1).
fn shaped(n: usize, f: impl Fn(f32, f32) -> f32) -> Vec<Vec<f32>> {
    (0..n)
        .map(|j| {
            let t = j as f32 / (n - 1).max(1) as f32;
            (0..SIZE).map(|i| f(t, i as f32 / SIZE as f32)).collect()
        })
        .collect()
}

/// `n` frames, each a sum of harmonics, by a formula over `t` and the harmonic `k`.
fn added(n: usize, top: usize, f: impl Fn(f32, usize) -> f32) -> Vec<Vec<f32>> {
    (0..n)
        .map(|j| {
            let t = j as f32 / (n - 1).max(1) as f32;
            let mut x = vec![0f32; SIZE];
            for k in 1..=top {
                let a = f(t, k);
                if a.abs() < 1e-5 {
                    continue;
                }
                for (i, s) in x.iter_mut().enumerate() {
                    *s += a * (TAU * k as f32 * i as f32 / SIZE as f32).sin();
                }
            }
            x
        })
        .collect()
}

/// A resonance's shape over frequency.
fn bump(hz: f32, at: f32, wide: f32) -> f32 {
    (-((hz / at).ln() / wide).powi(2)).exp()
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// The shapes every synth starts from: sine, triangle, saw, square, in that order to sweep through.
fn basic(i: usize, x: f32) -> f32 {
    match i {
        0 => (TAU * x).sin(),
        1 => 1.0 - 4.0 * ((x + 0.25).rem_euclid(1.0) - 0.5).abs(),
        2 => 2.0 * (x + 0.5).rem_euclid(1.0) - 1.0,
        _ => {
            if x < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
    }
}

const VOWELS: [[f32; 3]; 5] = [
    [730.0, 1090.0, 2440.0],
    [530.0, 1840.0, 2480.0],
    [270.0, 2290.0, 3010.0],
    [570.0, 840.0, 2410.0],
    [300.0, 870.0, 2240.0],
];

/// The wavetables, each with what it's for.
pub const TABLES: [(&str, &str); 24] = [
    ("Basic", "sine, triangle, saw and square, swept through"),
    (
        "Analog",
        "saw, square and narrowing pulses: what old synths have",
    ),
    ("PWM", "a pulse narrowing to nothing"),
    ("Harmonics", "one harmonic, then more and more"),
    ("Odd", "odd harmonics, hollow, growing"),
    ("Soft Saw", "a saw, its top rolled off further each frame"),
    ("Stack", "octaves and fifths piling up"),
    ("Formant", "a saw singing through its vowels"),
    ("Choir", "breathy voices opening"),
    ("FM 2:1", "two-operator FM, its index rising"),
    ("FM 3:1", "brassy FM, its index rising"),
    ("FM 7:2", "bell-like FM, its index rising"),
    ("Sync", "a saw hard-synced, its ratio climbing"),
    ("Fold", "a sine wave-folded harder and harder"),
    ("Bend", "a saw's phase bent further each frame"),
    ("Organ", "drawbars pulled out one by one"),
    ("Bell", "bright, ringing partials"),
    ("Digital", "stepped and crushed: early samplers"),
    ("Growl", "a formant saw, folded and snarling"),
    ("Metal", "harmonics that gap and clash"),
    ("Comb", "a notch sweeping through the harmonics"),
    ("Glass", "faint high partials, glassy"),
    ("Pluck", "a bright attack dying away, frame by frame"),
    ("Air", "soft, random-phase harmonics, close to noise"),
];

fn build(name: &str) -> Vec<Vec<f32>> {
    match name {
        "Analog" => {
            let pulse = |w: f32, x: f32| if x < w { 1.0 } else { -1.0 };
            vec![
                (0..SIZE)
                    .map(|i| basic(2, i as f32 / SIZE as f32))
                    .collect(),
                (0..SIZE)
                    .map(|i| pulse(0.5, i as f32 / SIZE as f32))
                    .collect(),
                (0..SIZE)
                    .map(|i| pulse(0.25, i as f32 / SIZE as f32))
                    .collect(),
                (0..SIZE)
                    .map(|i| pulse(0.1, i as f32 / SIZE as f32))
                    .collect(),
            ]
        }
        "PWM" => shaped(32, |t, x| if x < lerp(0.5, 0.01, t) { 1.0 } else { -1.0 }),
        "Harmonics" => added(32, 96, |t, k| {
            let top = 1.0 + t * t * 95.0;
            saw(k).min(1.0) * ((top - k as f32 + 1.0).clamp(0.0, 1.0))
        }),
        "Odd" => added(32, 95, |t, k| {
            let top = 1.0 + t * t * 94.0;
            match k % 2 {
                1 => saw(k) * (top - k as f32 + 1.0).clamp(0.0, 1.0),
                _ => 0.0,
            }
        }),
        "Soft Saw" => added(32, 256, |t, k| {
            saw(k) * (-(k as f32) * lerp(0.0, 0.4, t * t)).exp()
        }),
        "Stack" => added(32, 256, |t, k| {
            let kept = [1, 2, 3, 4, 6, 8, 12, 16, 24, 32];
            let bonus = if kept.contains(&k) { 1.0 } else { 0.0 };
            lerp(saw(k), bonus / (k as f32).sqrt(), t)
        }),
        "Formant" => added(32, 128, |t, k| {
            let at = t * 3.999;
            let (i, f) = (at as usize, at.fract());
            let hz = 110.0 * k as f32;
            let lift: f32 = (0..3)
                .map(|n| {
                    let v = lerp(VOWELS[i][n], VOWELS[i + 1][n], f);
                    bump(hz, v, 0.16) * [1.0, 0.65, 0.4][n]
                })
                .sum();
            saw(k) * (0.03 + 4.0 * lift)
        }),
        "Choir" => added(32, 96, |t, k| {
            let hz = 150.0 * k as f32;
            let lift =
                bump(hz, lerp(400.0, 800.0, t), 0.4) + 0.6 * bump(hz, lerp(900.0, 1300.0, t), 0.3);
            saw(k).sqrt() * lift
        }),
        "FM 2:1" => shaped(32, |t, x| {
            (TAU * x + t * t * 8.0 * (TAU * 2.0 * x).sin()).sin()
        }),
        "FM 3:1" => shaped(32, |t, x| {
            (TAU * x + t * t * 6.0 * (TAU * 3.0 * x).sin()).sin()
        }),
        "FM 7:2" => shaped(32, |t, x| {
            (TAU * 2.0 * x + t * t * 5.0 * (TAU * 7.0 * x).sin()).sin()
        }),
        "Sync" => shaped(32, |t, x| {
            let ratio = lerp(1.0, 8.0, t);
            let p = (x * ratio).fract();
            // Faded at the wrap, so it's the sync's sweep and not a click.
            (2.0 * p - 1.0) * (PI * x).sin().sqrt()
        }),
        "Fold" => shaped(32, |t, x| {
            let drive = lerp(1.0, 8.0, t);
            (drive * (TAU * x).sin() * PI / 2.0).sin()
        }),
        "Bend" => shaped(32, |t, x| {
            let p = x.powf(lerp(1.0, 5.0, t));
            2.0 * p - 1.0
        }),
        "Organ" => added(32, 16, |t, k| {
            let bars = [
                (1, 0.0),
                (2, 0.1),
                (3, 0.25),
                (4, 0.4),
                (6, 0.55),
                (8, 0.7),
                (12, 0.85),
                (16, 0.95),
            ];
            bars.iter().find(|b| b.0 == k).map_or(0.0, |b| {
                ((t - b.1 + 0.05) * 12.0).clamp(0.0, 1.0) / (k as f32).sqrt()
            })
        }),
        "Bell" => added(32, 64, |t, k| {
            let hit = [1, 3, 5, 8, 12, 17, 23, 31, 41, 53];
            match hit.contains(&k) {
                true => (-(k as f32) * lerp(0.2, 0.01, t)).exp(),
                false => 0.0,
            }
        }),
        "Digital" => shaped(32, |t, x| {
            let steps = lerp(32.0, 2.0, t).round();
            let v = basic(2, x);
            (v * steps).round() / steps
        }),
        "Growl" => shaped(32, |t, x| {
            let at = t * 3.999;
            let hz = lerp(
                VOWELS[at as usize][0],
                VOWELS[at as usize + 1][0],
                at.fract(),
            );
            let v = basic(2, x) + 0.8 * (TAU * x * hz / 110.0).sin();
            (v * lerp(1.0, 6.0, t)).sin()
        }),
        "Metal" => added(32, 128, |t, k| {
            let on = ((k as f32 * (0.37 + t * 0.21)).sin()).max(0.0);
            on / (k as f32).powf(0.7)
        }),
        "Comb" => added(32, 128, |t, k| {
            let notch = 1.0 - (-((k as f32 / (1.0 + t * 60.0)).fract() - 0.5).powi(2) * 40.0).exp();
            saw(k) * notch
        }),
        "Glass" => added(32, 64, |t, k| match k {
            1 => 1.0,
            3 | 5 | 8 | 13 | 21 | 34 | 55 => lerp(0.05, 0.4, t) / (k as f32).sqrt(),
            _ => 0.0,
        }),
        "Pluck" => added(32, 128, |t, k| {
            saw(k) * (-(k as f32) * lerp(0.002, 0.5, t)).exp()
        }),
        "Air" => {
            let mut seed = 0x9e37_79b9u32;
            let mut chance = || {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed as f32 / u32::MAX as f32
            };
            (0..16)
                .map(|j| {
                    let mut x = vec![0f32; SIZE];
                    for k in 1..=200usize {
                        let (a, ph) = (
                            chance() * (-(k as f32) * 0.012).exp(),
                            chance() * TAU + j as f32 * 0.2 * k as f32,
                        );
                        for (i, s) in x.iter_mut().enumerate() {
                            *s += a * (TAU * k as f32 * i as f32 / SIZE as f32 + ph).sin();
                        }
                    }
                    x
                })
                .collect()
        }
        // Basic, and any name it doesn't know.
        _ => (0..4)
            .map(|f| {
                (0..SIZE)
                    .map(|i| basic(f, i as f32 / SIZE as f32))
                    .collect()
            })
            .collect(),
    }
}

static BUILT: LazyLock<Mutex<HashMap<String, Arc<Table>>>> = LazyLock::new(Default::default);

/// A wavetable by name: built once, then shared. Never call it on the audio thread.
pub fn table(name: &str) -> Arc<Table> {
    let mut built = BUILT.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(t) = built.get(name) {
        return t.clone();
    }
    let t = Arc::new(Table::new(&build(name)));
    built.insert(name.to_string(), t.clone());
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(x: &[f32], k: usize) -> f32 {
        let (mut re, mut im) = (0f32, 0f32);
        for (j, s) in x.iter().enumerate() {
            let ph = TAU * k as f32 * j as f32 / x.len() as f32;
            (re, im) = (re + s * ph.cos(), im + s * ph.sin());
        }
        (re * re + im * im).sqrt() / x.len() as f32 * 2.0
    }

    #[test]
    fn every_table_builds_full_and_quiet_past_each_mips_harmonics() {
        for (name, _) in TABLES {
            let t = table(name);
            assert!(t.frames >= 4, "{name}");
            for f in [0, t.frames - 1] {
                let peak = t.frame(f).iter().fold(0f32, |m, x| m.max(x.abs()));
                assert!((peak - 0.9).abs() < 0.02, "{name} frame {f}: {peak}");
                assert!(t.frame(f).iter().all(|x| x.is_finite()), "{name}");
            }
        }
        // A saw at mip 6 holds its first sixteen harmonics and nothing past.
        let t = table("Basic");
        let level6 = t.level(6);
        let saw = &level6[2 * (SIZE + 1)..2 * (SIZE + 1) + SIZE];
        assert!(level(saw, 16) > 0.02, "{}", level(saw, 16));
        assert!(level(saw, 17) < 1e-3, "{}", level(saw, 17));
    }

    #[test]
    fn a_note_reads_the_copy_it_can_play() {
        // 110 Hz at 96 kHz has room for 436 harmonics; a note at 12 kHz, for four.
        assert_eq!(Table::mip(110.0 / 96_000.0), 2);
        assert_eq!(Table::mip(12_000.0 / 96_000.0), 8);
        assert_eq!(Table::mip(0.0), 0);
        assert_eq!(Table::mip(0.6), 10);
    }

    #[test]
    fn a_table_reads_between_its_frames_and_samples() {
        let t = table("Basic");
        let level0 = t.level(0);
        let (a, b) = (wave(level0, 0), wave(level0, 1));
        let mut out = [0.0; 2];
        read(a, b, 0.0, &[1 << 30, u32::MAX], &mut out);
        assert!((out[0] - 0.9).abs() < 0.01);
        // At the last sample it reads into the repeat of the first, without a jump.
        assert!(out[1].abs() < 0.01, "{}", out[1]);
        let mut both = [0.0; 2];
        read(a, b, 1e-3, &[1 << 30, u32::MAX], &mut both);
        assert!((both[0] - out[0]).abs() < 0.01);
    }
}
