//! The channel strip's built-in processors, as Logic's Channel EQ and Compressor: a low cut, two
//! bells and a high shelf; and a soft-knee compressor. Their settings are in the project; filters
//! and coefficients are worked out on the main thread, and their state lives with the lane.

use std::f64::consts::PI;

use crate::project::{Band, Comp, Eq};

/// Second-order filter coefficients, normalized so a0 is 1 (RBJ's audio EQ cookbook).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

/// A filter's memory, for one channel (transposed direct form II).
#[derive(Clone, Copy, Default)]
struct Memory {
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn new(b: [f64; 3], a: [f64; 3]) -> Self {
        Biquad {
            b0: (b[0] / a[0]) as f32,
            b1: (b[1] / a[0]) as f32,
            b2: (b[2] / a[0]) as f32,
            a1: (a[1] / a[0]) as f32,
            a2: (a[2] / a[0]) as f32,
        }
    }

    fn highpass(freq: f64, q: f64, rate: f64) -> Self {
        let w = 2.0 * PI * freq / rate;
        let (cos, alpha) = (w.cos(), w.sin() / (2.0 * q));
        Biquad::new(
            [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0],
            [1.0 + alpha, -2.0 * cos, 1.0 - alpha],
        )
    }

    fn bell(freq: f64, gain_db: f64, q: f64, rate: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let w = 2.0 * PI * freq / rate;
        let (cos, alpha) = (w.cos(), w.sin() / (2.0 * q));
        Biquad::new(
            [1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a],
            [1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a],
        )
    }

    fn high_shelf(freq: f64, gain_db: f64, q: f64, rate: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let w = 2.0 * PI * freq / rate;
        let (cos, alpha) = (w.cos(), w.sin() / (2.0 * q));
        let s = 2.0 * a.sqrt() * alpha;
        Biquad::new(
            [
                a * ((a + 1.0) + (a - 1.0) * cos + s),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                a * ((a + 1.0) + (a - 1.0) * cos - s),
            ],
            [
                (a + 1.0) - (a - 1.0) * cos + s,
                2.0 * ((a - 1.0) - (a + 1.0) * cos),
                (a + 1.0) - (a - 1.0) * cos - s,
            ],
        )
    }

    #[inline]
    fn run(&self, m: &mut Memory, x: f32) -> f32 {
        let y = self.b0 * x + m.z1;
        m.z1 = self.b1 * x - self.a1 * y + m.z2;
        m.z2 = self.b2 * x - self.a2 * y;
        y
    }

    /// The gain at a frequency, in dB.
    fn response_db(&self, freq: f64, rate: f64) -> f64 {
        let w = 2.0 * PI * freq / rate;
        // H(z) at z = e^jw, as (re, im) of numerator and denominator in z^-1.
        let (c1, s1, c2, s2) = (w.cos(), -w.sin(), (2.0 * w).cos(), -(2.0 * w).sin());
        let num = (
            self.b0 as f64 + self.b1 as f64 * c1 + self.b2 as f64 * c2,
            self.b1 as f64 * s1 + self.b2 as f64 * s2,
        );
        let den = (
            1.0 + self.a1 as f64 * c1 + self.a2 as f64 * c2,
            self.a1 as f64 * s1 + self.a2 as f64 * s2,
        );
        let mag = (num.0.hypot(num.1) / den.0.hypot(den.1)).max(1e-9);
        20.0 * mag.log10()
    }
}

/// The lowest low cut: at or under this, it's off.
pub const LOW_CUT_OFF: f32 = 20.0;

/// An EQ's filters at a sample rate; bands that do nothing are left out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EqFilters {
    filters: [Biquad; 4],
    active: [bool; 4],
}

fn safe(band: &Band, rate: f64) -> (f64, f64, f64) {
    let freq = (band.freq as f64).clamp(10.0, rate * 0.45);
    (freq, band.gain_db as f64, (band.q as f64).clamp(0.1, 18.0))
}

impl EqFilters {
    pub fn new(eq: &Eq, rate: f64) -> Self {
        let cut = (eq.low_cut as f64).clamp(LOW_CUT_OFF as f64, rate * 0.45);
        let (f1, g1, q1) = safe(&eq.low_mid, rate);
        let (f2, g2, q2) = safe(&eq.high_mid, rate);
        let (f3, g3, q3) = safe(&eq.high_shelf, rate);
        EqFilters {
            filters: [
                Biquad::highpass(cut, std::f64::consts::FRAC_1_SQRT_2, rate),
                Biquad::bell(f1, g1, q1, rate),
                Biquad::bell(f2, g2, q2, rate),
                Biquad::high_shelf(f3, g3, q3, rate),
            ],
            active: [
                eq.low_cut > LOW_CUT_OFF,
                g1.abs() > 0.01,
                g2.abs() > 0.01,
                g3.abs() > 0.01,
            ],
        }
    }

    /// The EQ's gain at a frequency, in dB.
    pub fn response_db(&self, freq: f64, rate: f64) -> f64 {
        (0..4)
            .filter(|&i| self.active[i])
            .map(|i| self.filters[i].response_db(freq, rate))
            .sum()
    }
}

/// What an EQ does at `freq`, in dB: for drawing its curve.
pub fn eq_response(eq: &Eq, freq: f64, rate: f64) -> f64 {
    EqFilters::new(eq, rate).response_db(freq, rate)
}

/// An EQ's memory, for a stereo channel.
#[derive(Default)]
pub struct EqState {
    memory: [[Memory; 4]; 2],
}

impl EqState {
    /// Filters a block in place. Allocation-free.
    pub fn process(&mut self, eq: &EqFilters, l: &mut [f32], r: &mut [f32]) {
        for (band, f) in eq.filters.iter().enumerate() {
            if !eq.active[band] {
                continue;
            }
            for (ch, buf) in [&mut *l, &mut *r].into_iter().enumerate() {
                let m = &mut self.memory[ch][band];
                for x in buf.iter_mut() {
                    *x = f.run(m, *x);
                }
            }
        }
    }
}

/// A compressor's settings, worked out for a sample rate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompParams {
    threshold: f32,
    slope: f32,
    attack: f32,
    release: f32,
    makeup: f32,
}

/// The soft knee, in dB.
const KNEE: f32 = 6.0;

impl CompParams {
    pub fn new(c: &Comp, rate: f64) -> Self {
        let coef = |ms: f32| (-1.0 / (ms.max(0.05) as f64 * 0.001 * rate)).exp() as f32;
        CompParams {
            threshold: c.threshold_db,
            slope: 1.0 - 1.0 / c.ratio.max(1.0),
            attack: coef(c.attack_ms),
            release: coef(c.release_ms),
            makeup: c.makeup_db,
        }
    }

    /// How far a level (dB) is turned down, on the static curve.
    pub fn reduction(&self, level_db: f32) -> f32 {
        let over = level_db - self.threshold;
        if over <= -KNEE / 2.0 {
            0.0
        } else if over >= KNEE / 2.0 {
            over * self.slope
        } else {
            self.slope * (over + KNEE / 2.0).powi(2) / (2.0 * KNEE)
        }
    }
}

/// A compressor's memory: the gain reduction it's at, in dB.
#[derive(Default)]
pub struct CompState {
    reduction: f32,
}

impl CompState {
    /// Compresses a block in place, both sides by the same amount. Returns the most it turned
    /// the block down, in dB. Allocation-free.
    pub fn process(&mut self, p: &CompParams, l: &mut [f32], r: &mut [f32]) -> f32 {
        let mut most = 0f32;
        for i in 0..l.len() {
            let peak = l[i].abs().max(r[i].abs()).max(1e-9);
            let target = p.reduction(20.0 * peak.log10());
            let coef = if target > self.reduction {
                p.attack
            } else {
                p.release
            };
            self.reduction = coef * self.reduction + (1.0 - coef) * target;
            most = most.max(self.reduction);
            let g = 10f32.powf((p.makeup - self.reduction) / 20.0);
            l[i] *= g;
            r[i] *= g;
        }
        most
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;

    fn sine(freq: f64, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * PI * freq * i as f64 / RATE).sin() as f32)
            .collect()
    }

    fn peak(s: &[f32]) -> f32 {
        s.iter().fold(0.0, |m, x| m.max(x.abs()))
    }

    /// The level of a sine through an EQ, in dB, after it settles.
    fn through(eq: &Eq, freq: f64) -> f64 {
        let filters = EqFilters::new(eq, RATE);
        let mut state = EqState::default();
        let (mut l, mut r) = (sine(freq, 48_000), sine(freq, 48_000));
        state.process(&filters, &mut l, &mut r);
        20.0 * (peak(&l[24_000..]) as f64).log10()
    }

    #[test]
    fn a_flat_eq_does_nothing() {
        let eq = Eq::default();
        assert_eq!(EqFilters::new(&eq, RATE).active, [false; 4]);
        assert!(through(&eq, 1000.0).abs() < 0.01);
        assert_eq!(eq_response(&eq, 1000.0, RATE), 0.0);
    }

    #[test]
    fn bells_shelves_and_the_low_cut_do_what_they_say() {
        let mut eq = Eq::default();
        eq.high_mid.gain_db = 6.0;
        eq.high_mid.freq = 2_000.0;
        assert!((through(&eq, 2_000.0) - 6.0).abs() < 0.1);
        assert!(through(&eq, 100.0).abs() < 0.2);
        // The curve the UI draws agrees with what the filter does.
        assert!((eq_response(&eq, 2_000.0, RATE) - 6.0).abs() < 0.01);

        let eq = Eq {
            low_cut: 200.0,
            ..Eq::default()
        };
        assert!(through(&eq, 50.0) < -20.0);
        assert!(through(&eq, 2_000.0).abs() < 0.2);
        assert!((eq_response(&eq, 200.0, RATE) + 3.0).abs() < 0.1); // -3 dB at the corner

        let mut eq = Eq::default();
        eq.high_shelf.gain_db = -9.0;
        assert!((through(&eq, 18_000.0) + 9.0).abs() < 0.5);
    }

    #[test]
    fn the_compressor_turns_loud_signals_down_by_the_ratio() {
        let comp = Comp {
            threshold_db: -20.0,
            ratio: 4.0,
            attack_ms: 1.0,
            release_ms: 50.0,
            makeup_db: 0.0,
            bypass: false,
        };
        let p = CompParams::new(&comp, RATE);
        // 20 dB over the threshold comes out 5 dB over it: 15 dB down.
        assert!((p.reduction(0.0) - 15.0).abs() < 1e-4);
        assert_eq!(p.reduction(-40.0), 0.0);
        let mut state = CompState::default();
        let (mut l, mut r) = (sine(1000.0, 48_000), sine(1000.0, 48_000));
        let most = state.process(&p, &mut l, &mut r);
        assert!((most - 15.0).abs() < 0.5, "{most}");
        let out = 20.0 * peak(&l[40_000..]).log10();
        assert!((out + 15.0).abs() < 1.0, "{out}");
        // Quiet: untouched.
        let mut state = CompState::default();
        let (mut l, mut r) = (sine(1000.0, 4_800), sine(1000.0, 4_800));
        l.iter_mut().chain(r.iter_mut()).for_each(|x| *x *= 0.01);
        assert_eq!(state.process(&p, &mut l, &mut r), 0.0);
    }
}
