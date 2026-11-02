//! Reading a sample between its frames: finely (4-point Hermite), from a copy filtered and
//! halved in rate once for each octave it's played up, so what's played high doesn't alias.

/// How many halved copies a sample has: played up to 32 times as fast stays clean.
pub const LEVELS: usize = 5;

/// A sample's channels at its own rate and halved, again and again: `levels[k]` at a 2^k-th of
/// it, filtered so nothing above its new Nyquist folds back.
pub struct Mips {
    pub levels: Vec<Vec<Vec<f32>>>,
}

/// The half-band filter halving a level: a Blackman-windowed sinc, cut off at a quarter of the
/// rate, 47 taps.
fn halfband() -> Vec<f32> {
    const N: usize = 47;
    let m = (N - 1) as f32 / 2.0;
    let mut h: Vec<f32> = (0..N)
        .map(|k| {
            let t = k as f32 - m;
            let sinc = match t == 0.0 {
                true => 0.5,
                false => (std::f32::consts::FRAC_PI_2 * t).sin() / (std::f32::consts::PI * t),
            };
            let a = std::f32::consts::TAU * k as f32 / (N - 1) as f32;
            sinc * (0.42 - 0.5 * a.cos() + 0.08 * (2.0 * a).cos())
        })
        .collect();
    let sum: f32 = h.iter().sum();
    h.iter_mut().for_each(|x| *x /= sum);
    h
}

/// A channel at half its rate: frame n is the filtered sound at frame 2n (no delay).
fn halve(x: &[f32], h: &[f32]) -> Vec<f32> {
    let m = h.len() as isize / 2;
    (0..x.len().div_ceil(2))
        .map(|n| {
            let centre = 2 * n as isize;
            h.iter()
                .enumerate()
                .map(|(k, &c)| {
                    let i = centre + k as isize - m;
                    match i >= 0 && (i as usize) < x.len() {
                        true => c * x[i as usize],
                        false => 0.0,
                    }
                })
                .sum()
        })
        .collect()
}

impl Mips {
    /// The copies of `channels` (the first level is them as they are).
    pub fn new(channels: Vec<Vec<f32>>) -> Mips {
        let h = halfband();
        let mut levels = vec![channels];
        while levels.len() < LEVELS && levels.last().is_some_and(|l| l[0].len() > 64) {
            let next = levels
                .last()
                .unwrap()
                .iter()
                .map(|c| halve(c, &h))
                .collect();
            levels.push(next);
        }
        Mips { levels }
    }

    /// The level to read at a speed (frames of the sample a frame out): the one whose rate
    /// keeps it under Nyquist, and the speed there.
    pub fn level(&self, speed: f64) -> usize {
        let octaves = speed.abs().max(1.0).log2().floor() as usize;
        octaves.min(self.levels.len() - 1)
    }

    /// A channel (the last, if it has fewer) at frame `pos` of the sample, read from `level`.
    pub fn read(&self, level: usize, channel: usize, pos: f64) -> f32 {
        let l = &self.levels[level];
        let ch = &l[channel.min(l.len() - 1)];
        hermite(ch, pos / (1u64 << level) as f64)
    }

    /// The frame nearest `pos`, as a sampler without interpolation read it.
    pub fn nearest(&self, channel: usize, pos: f64) -> f32 {
        let l = &self.levels[0];
        let ch = &l[channel.min(l.len() - 1)];
        ch.get(pos.max(0.0) as usize).copied().unwrap_or(0.0)
    }
}

/// `x` read between frames by a 4-point, third-order Hermite curve; silence outside it.
pub fn hermite(x: &[f32], pos: f64) -> f32 {
    let i = pos.floor();
    let t = (pos - i) as f32;
    let i = i as isize;
    let at = |k: isize| {
        let j = i + k;
        match j >= 0 && (j as usize) < x.len() {
            true => x[j as usize],
            false => 0.0,
        }
    };
    let (xm1, x0, x1, x2) = (at(-1), at(0), at(1), at(2));
    let c1 = 0.5 * (x1 - xm1);
    let c2 = xm1 - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
    let c3 = 0.5 * (x2 - xm1) + 1.5 * (x0 - x1);
    ((c3 * t + c2) * t + c1) * t + x0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f64, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (std::f64::consts::TAU * hz * i as f64 / 48_000.0).sin() as f32)
            .collect()
    }

    #[test]
    fn hermite_reads_between_frames_finely() {
        let x = sine(1_000.0, 4_800);
        let worst = (100..4_000)
            .map(|i| {
                let pos = i as f64 + 0.37;
                let want = (std::f64::consts::TAU * 1_000.0 * pos / 48_000.0).sin() as f32;
                (hermite(&x, pos) - want).abs()
            })
            .fold(0f32, f32::max);
        assert!(worst < 2e-3, "{worst}");
    }

    #[test]
    fn a_halved_copy_keeps_what_fits_and_drops_what_would_fold_back() {
        // 2 kHz stays; 18 kHz (over the halved rate's 12 kHz Nyquist) goes.
        let m = Mips::new(vec![sine(2_000.0, 48_000)]);
        let m2 = Mips::new(vec![sine(18_000.0, 48_000)]);
        let peak = |m: &Mips| {
            m.levels[1][0][1_000..20_000]
                .iter()
                .fold(0f32, |a, x| a.max(x.abs()))
        };
        assert!((peak(&m) - 1.0).abs() < 0.02, "{}", peak(&m));
        assert!(peak(&m2) < 0.01, "{}", peak(&m2));
        // In step with the original: frame n of the half is frame 2n of it.
        let (a, b) = (m.levels[1][0][5_000], m.levels[0][0][10_000]);
        assert!((a - b).abs() < 0.02, "{a} {b}");
        assert_eq!((m.level(1.5), m.level(2.0), m.level(5.0)), (0, 1, 2));
    }
}
