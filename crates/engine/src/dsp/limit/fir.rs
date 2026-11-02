//! Linear-phase half-band filters, for the limiter's gain to run at 2, 4 or 8 times the rate.
//! The saturator's all-pass half-bands are cheaper, but they shift each frequency by a little
//! more or less, which moves a waveform's peaks. These only delay it, so the peaks the limiter
//! worked out are the peaks that come out.

/// The first stage's taps (flat to 0.43 of the sample rate, 70 dB down past 0.57), and the later
/// stages', whose sound is already band-limited well under their rate.
const FIRST: usize = 63;
const LATER: usize = 19;

/// The last samples, twice over so they read newest first in a row.
#[derive(Clone, Copy)]
struct History {
    buf: [f32; 64],
    at: usize,
}

impl History {
    const fn new() -> Self {
        History {
            buf: [0.0; 64],
            at: 0,
        }
    }

    fn push(&mut self, x: f32) {
        self.at = (self.at + 31) % 32;
        self.buf[self.at] = x;
        self.buf[self.at + 32] = x;
    }

    /// `i` samples back.
    fn back(&self, i: usize) -> f32 {
        self.buf[self.at + i]
    }

    fn dot(&self, taps: &[f32]) -> f32 {
        taps.iter()
            .zip(&self.buf[self.at..])
            .map(|(t, x)| t * x)
            .sum()
    }
}

/// One stage, both ways. A half-band filter's every other tap is 0 but its middle one (a half),
/// so each way is one short sum and one delay.
#[derive(Clone, Copy)]
pub(crate) struct Stage {
    /// The taps that aren't 0 or the middle one, in order.
    taps: [f32; 32],
    len: usize,
    /// How far back the middle tap reads, in input samples.
    mid: usize,
    up: History,
    even: History,
    odd: History,
}

impl Stage {
    fn new(n: usize) -> Self {
        let c = (n - 1) / 2;
        let beta = 7.0;
        let i0 = |x: f64| {
            // The zeroth modified Bessel function, by its series.
            let (mut sum, mut term) = (1.0, 1.0);
            for k in 1..32 {
                term *= (x / 2.0) / k as f64;
                sum += term * term;
            }
            sum
        };
        let mut taps = [0f32; 32];
        let len = n.div_ceil(2);
        let mut total = 0.0;
        let h: Vec<f64> = (0..len)
            .map(|j| {
                let m = (2 * j) as f64 - c as f64;
                let sinc = (std::f64::consts::PI * m / 2.0).sin() / (std::f64::consts::PI * m);
                let r = m / (c as f64 + 1.0);
                let w = i0(beta * (1.0 - r * r).max(0.0).sqrt()) / i0(beta);
                sinc * w
            })
            .collect();
        for v in &h {
            total += v;
        }
        // The taps add to a half, the middle the other half: flat at DC.
        for (t, v) in taps.iter_mut().zip(&h) {
            *t = (v * 0.5 / total) as f32;
        }
        Stage {
            taps,
            len,
            mid: (c - 1) / 2,
            up: History::new(),
            even: History::new(),
            odd: History::new(),
        }
    }

    /// A sample up to two.
    fn up(&mut self, x: f32) -> [f32; 2] {
        self.up.push(x);
        [
            2.0 * self.up.dot(&self.taps[..self.len]),
            self.up.back(self.mid),
        ]
    }

    /// Two samples back down to one: the filter centred a whole input sample back, so the
    /// trip's delay is whole too.
    fn down(&mut self, [a, b]: [f32; 2]) -> f32 {
        self.even.push(a);
        self.odd.push(b);
        self.even.dot(&self.taps[..self.len]) + 0.5 * self.odd.back(self.mid + 1)
    }
}

/// Up to three stages, each doubling the rate.
pub(crate) fn stages() -> [Stage; 3] {
    [Stage::new(FIRST), Stage::new(LATER), Stage::new(LATER)]
}

/// How late a trip up and back down through `factor` stages leaves the sound, in samples at the
/// sample rate: each stage's middle tap, twice, at its own rate.
pub(crate) fn trip(factor: usize) -> f32 {
    [FIRST, LATER, LATER][..factor]
        .iter()
        .enumerate()
        .map(|(k, &n)| (n - 1) as f32 / 2.0 / (1 << k) as f32)
        .sum()
}

/// `x` up 2^factor times, the samples in time order.
pub(crate) fn upsample(stages: &mut [Stage; 3], x: f32, factor: usize) -> [f32; 8] {
    let mut up = [0f32; 8];
    up[0] = x;
    let mut count = 1;
    for stage in stages.iter_mut().take(factor) {
        let was = up;
        for (j, &v) in was[..count].iter().enumerate() {
            [up[2 * j], up[2 * j + 1]] = stage.up(v);
        }
        count *= 2;
    }
    up
}

/// 2^factor samples back down to one.
pub(crate) fn downsample(stages: &mut [Stage; 3], mut s: [f32; 8], factor: usize) -> f32 {
    let mut count = 1 << factor;
    for stage in stages[..factor].iter_mut().rev() {
        count /= 2;
        for j in 0..count {
            s[j] = stage.down([s[2 * j], s[2 * j + 1]]);
        }
    }
    s[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tone through a trip comes out as it went in, only late: flat across the band, its
    /// shape (and so its peaks) kept.
    #[test]
    fn a_trip_up_and_back_is_only_a_delay() {
        let rate = 48_000.0;
        for factor in 1..=3 {
            let late = trip(factor);
            for hz in [100.0, 1_000.0, 10_000.0, 19_000.0] {
                let mut st = stages();
                let n = 4_800;
                let wave = |t: f64| (std::f64::consts::TAU * hz * t / rate).sin() as f32;
                let out: Vec<f32> = (0..n)
                    .map(|i| {
                        let u = upsample(&mut st, wave(i as f64), factor);
                        downsample(&mut st, u, factor)
                    })
                    .collect();
                let worst = (n / 2..n)
                    .map(|i| (out[i] - wave(i as f64 - late as f64)).abs())
                    .fold(0f32, f32::max);
                assert!(worst < 0.01, "{factor} stages, {hz} Hz: {worst}");
            }
        }
        assert_eq!((trip(1), trip(2), trip(3)), (31.0, 35.5, 37.75));
    }
}
