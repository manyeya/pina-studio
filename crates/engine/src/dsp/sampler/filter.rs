//! A sampler voice's filter: the state-variable filter (Andy Simper's) as low, high, band pass
//! or notch; the four-pole ladder (Zavalishin's zero-delay form, its feedback resolved, driven);
//! a comb tuned to the cutoff; and three resonances at a vowel's formants. Coefficients are
//! worked out every few samples (`coefs`), the filter run each sample (`run`).

use crate::song::project::FilterMode;

/// The comb's longest delay a channel, in samples: 20 Hz at 96 kHz.
pub const COMB: usize = 4096;

/// Formants (Hz) of the vowels the formant filter sweeps through, and how loud each is.
const VOWELS: [[f32; 3]; 5] = [
    [730.0, 1090.0, 2440.0],
    [530.0, 1840.0, 2480.0],
    [270.0, 2290.0, 3010.0],
    [570.0, 840.0, 2410.0],
    [300.0, 870.0, 2240.0],
];
const FORMANT_GAINS: [f32; 3] = [1.0, 0.6, 0.35];

/// A state-variable filter's coefficients: g's three products and the damping.
#[derive(Clone, Copy, Default)]
struct Svf {
    a1: f32,
    a2: f32,
    a3: f32,
    k: f32,
}

impl Svf {
    fn new(hz: f32, k: f32, rate: f32) -> Svf {
        let g = (std::f32::consts::PI * hz.clamp(10.0, rate * 0.45) / rate).tan();
        let a1 = 1.0 / (1.0 + g * (g + k));
        Svf {
            a1,
            a2: g * a1,
            a3: g * g * a1,
            k,
        }
    }

    /// One sample through, `ic` its two integrators: (band, low).
    fn run(&self, ic: &mut [f32; 2], x: f32) -> (f32, f32) {
        let v3 = x - ic[1];
        let v1 = self.a1 * ic[0] + self.a2 * v3;
        let v2 = ic[1] + self.a2 * ic[0] + self.a3 * v3;
        ic[0] = 2.0 * v1 - ic[0];
        ic[1] = 2.0 * v2 - ic[1];
        (v1, v2)
    }
}

/// What the filter is set to, worked out every few samples.
#[derive(Clone, Copy, Default)]
pub struct Coefs {
    mode: FilterMode,
    svf: Svf,
    /// The ladder: each pole's G, its feedback and how its loudness is made up for.
    g: f32,
    feedback: f32,
    makeup: f32,
    /// The comb's delay (samples) and feedback.
    delay: f32,
    comb: f32,
    formants: [Svf; 3],
    /// Into the filter: how hard (1 clean), and the loudness that keeps.
    drive: f32,
}

/// The coefficients for `mode` at `hz` and `resonance` (0 to 1), `drive` 0 to 1.
pub fn coefs(mode: FilterMode, hz: f32, resonance: f32, drive: f32, rate: f32) -> Coefs {
    let resonance = resonance.clamp(0.0, 1.0);
    let hz = hz.clamp(20.0, rate * 0.45);
    let mut c = Coefs {
        mode,
        drive: 1.0 + 8.0 * drive.clamp(0.0, 1.0),
        ..Coefs::default()
    };
    match mode {
        FilterMode::Off => {}
        FilterMode::LowPass | FilterMode::HighPass | FilterMode::BandPass | FilterMode::Notch => {
            c.svf = Svf::new(hz, 2.0 * (1.0 - 0.97 * resonance), rate);
        }
        FilterMode::Ladder => {
            let g = (std::f32::consts::PI * hz / rate).tan();
            c.g = g / (1.0 + g);
            c.feedback = 3.9 * resonance;
            c.makeup = 1.0 + 0.6 * c.feedback;
        }
        FilterMode::Comb => {
            c.delay = (rate / hz).min(COMB as f32 - 2.0);
            c.comb = 0.98 * resonance;
        }
        FilterMode::Formant => {
            // The cutoff's place from 20 Hz to 20 kHz sweeps a, e, i, o, u.
            let at = ((hz / 20.0).log2() / 1000f32.log2() * 4.0).clamp(0.0, 4.0);
            let (i, t) = ((at.floor() as usize).min(3), at - at.floor().min(3.0));
            let q = 4.0 + 16.0 * resonance;
            for (f, svf) in c.formants.iter_mut().enumerate() {
                let (a, b) = (VOWELS[i][f], VOWELS[i + 1][f]);
                *svf = Svf::new(a * (b / a).powf(t), 1.0 / q, rate);
            }
        }
    }
    c
}

/// A voice's filter, a channel's worth of memory each.
#[derive(Clone, Copy, Default)]
pub struct Filter {
    svf: [[f32; 2]; 2],
    ladder: [[f32; 4]; 2],
    formants: [[[f32; 2]; 3]; 2],
    comb_at: [usize; 2],
}

impl Filter {
    /// A sample of `channel` through it; `comb` is the channel's comb delay line (`COMB` long).
    pub fn run(&mut self, c: &Coefs, channel: usize, x: f32, comb: &mut [f32]) -> f32 {
        let x = match c.drive > 1.0 {
            true => soft(x * c.drive) / c.drive.sqrt(),
            false => x,
        };
        match c.mode {
            FilterMode::Off => x,
            FilterMode::LowPass => c.svf.run(&mut self.svf[channel], x).1,
            FilterMode::BandPass => c.svf.run(&mut self.svf[channel], x).0,
            FilterMode::HighPass => {
                let (band, low) = c.svf.run(&mut self.svf[channel], x);
                x - c.svf.k * band - low
            }
            FilterMode::Notch => {
                let (band, _) = c.svf.run(&mut self.svf[channel], x);
                x - c.svf.k * band
            }
            FilterMode::Ladder => {
                let s = &mut self.ladder[channel];
                let (g, b) = (c.g, 1.0 - c.g);
                // The four poles' memory as heard at the output, and the input that resolves
                // the feedback loop without a sample's delay.
                let sigma = b * (g * g * g * s[0] + g * g * s[1] + g * s[2] + s[3]);
                let u = soft((x - c.feedback * sigma) / (1.0 + c.feedback * g * g * g * g));
                let mut y = u;
                for state in s.iter_mut() {
                    let v = (y - *state) * g;
                    y = v + *state;
                    *state = y + v;
                }
                y * c.makeup
            }
            FilterMode::Comb => {
                let (n, at) = (COMB, &mut self.comb_at[channel]);
                let back = *at as f32 + n as f32 - c.delay;
                let (i, t) = (back.floor() as usize, back.fract());
                let delayed = comb[i % n] + (comb[(i + 1) % n] - comb[i % n]) * t;
                let y = x + c.comb * delayed;
                comb[*at] = y;
                *at = (*at + 1) % n;
                y * (1.0 - c.comb).sqrt()
            }
            FilterMode::Formant => c
                .formants
                .iter()
                .zip(self.formants[channel].iter_mut())
                .zip(FORMANT_GAINS)
                .map(|((svf, ic), gain)| svf.run(ic, x).0 * svf.k * gain * 2.5)
                .sum(),
        }
    }
}

/// Close to tanh, and quicker: a rational fit, flat at ±1 past ±3.
fn soft(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    /// How loud a sine of `hz` comes out of `mode` (after it settles), against how it went in.
    fn gain(mode: FilterMode, cutoff: f32, resonance: f32, hz: f32) -> f32 {
        let c = coefs(mode, cutoff, resonance, 0.0, RATE);
        let mut f = Filter::default();
        let mut comb = vec![0f32; COMB];
        let mut peak = 0f32;
        for i in 0..24_000 {
            let x = (std::f32::consts::TAU * hz * i as f32 / RATE).sin() * 0.1;
            let y = f.run(&c, 0, x, &mut comb);
            if i > 12_000 {
                peak = peak.max(y.abs());
            }
        }
        peak / 0.1
    }

    #[test]
    fn the_ladder_cuts_steeper_than_the_clean_low_pass() {
        let (svf, ladder) = (
            gain(FilterMode::LowPass, 500.0, 0.0, 4_000.0),
            gain(FilterMode::Ladder, 500.0, 0.0, 4_000.0),
        );
        assert!(ladder < svf / 4.0, "{svf} {ladder}");
        assert!(gain(FilterMode::Ladder, 500.0, 0.0, 100.0) > 0.8);
        // Resonance rings at the cutoff.
        assert!(gain(FilterMode::Ladder, 1_000.0, 0.9, 1_000.0) > 1.5);
    }

    #[test]
    fn a_notch_takes_out_its_frequency_and_a_comb_rings_at_its_harmonics() {
        assert!(gain(FilterMode::Notch, 1_000.0, 0.0, 1_000.0) < 0.05);
        assert!(gain(FilterMode::Notch, 1_000.0, 0.0, 100.0) > 0.9);
        let (on, off) = (
            gain(FilterMode::Comb, 500.0, 0.9, 1_000.0),
            gain(FilterMode::Comb, 500.0, 0.9, 750.0),
        );
        assert!(on > off * 4.0, "{on} {off}");
    }

    #[test]
    fn the_formant_filter_moves_its_peaks_with_the_vowel() {
        // "a" (the bottom of the sweep) peaks near 730 Hz; "i" (the middle) near 270 Hz.
        let a = |hz| gain(FilterMode::Formant, 20.0, 0.5, hz);
        let i = |hz| gain(FilterMode::Formant, 632.0, 0.5, hz);
        assert!(a(730.0) > a(270.0) * 2.0, "{} {}", a(730.0), a(270.0));
        assert!(i(270.0) > i(730.0) * 2.0, "{} {}", i(270.0), i(730.0));
    }
}
