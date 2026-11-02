//! The channel strip's compressor: feed-forward, both sides turned down by the louder, its
//! gain worked out in dB with a soft knee and smoothed by the attack (going down) and release
//! (coming back). Its style sets how it hears (peak or RMS), how soft its knee is, how its
//! release behaves and whether it colours the sound; a bus compressor's controls go in steps,
//! as a console bus compressor's do.

use crate::song::project::{Comp, CompStyle, nearest_step};
use spectral::Spectral;

/// The RMS average's time constant, in seconds.
const RMS_TAU: f32 = 0.015;
/// An automatic release's two speeds, and how slowly a long squeeze hands over to the slow one.
const AUTO_FAST: f32 = 0.1;
const AUTO_SLOW: f32 = 1.2;
const AUTO_HANDOVER: f32 = 0.3;

/// A compressor running: its smoothed gains and averages, which last through edits.
pub struct Compressor {
    rate: f32,
    /// Each side's gain, in dB (0 or under); and, releasing automatically, the slow one's.
    gain: [f32; 2],
    slow: [f32; 2],
    /// Each side's mean square, for RMS.
    ms: [f32; 2],
    /// The sidechain's high pass, each side, and the frequency it's set for.
    hp: [[f32; 4]; 2],
    hp_hz: f32,
    hp_coeffs: [f32; 5],
    /// The makeup gain and mix as they glide to where they're set.
    makeup: f32,
    mix: f32,
    started: bool,
    spectral: Spectral,
}

/// A one-pole smoother's pole for a time constant in seconds; none (0) for a time of nothing.
pub(crate) fn pole(tau: f32, rate: f32) -> f32 {
    match tau.abs() < f32::EPSILON {
        true => 0.0,
        false => (-1.0 / (tau * rate)).exp(),
    }
}

/// A denormal flushed to zero, so a fading state doesn't slow the CPU to a crawl.
pub(crate) fn ftz(x: f32) -> f32 {
    match x.to_bits() & 0x7f80_0000 {
        0 => 0.0,
        _ => x,
    }
}

/// The gain reduction (dB, 0 or under) for a level in dB: none under the knee, `strength` of
/// what's over the threshold above it, easing in over `knee` dB between.
fn gain_computer(level: f32, threshold: f32, strength: f32, knee: f32) -> f32 {
    let over = if level > threshold + knee / 2.0 {
        level - threshold
    } else if level > threshold - knee / 2.0 {
        (level - threshold + knee / 2.0).powi(2) / (2.0 * knee)
    } else {
        0.0
    };
    over.max(0.0) * -strength
}

/// What a style's settings come to: (ratio, attack ms, release ms, knee dB, RMS, auto release).
fn settings(c: &Comp) -> (f32, f32, f32, f32, bool, bool) {
    let (ratio, attack, release) = (c.ratio, c.attack_ms, c.release_ms);
    match c.style {
        CompStyle::Clean => (ratio, attack, release, 6.0, c.rms, c.auto_release),
        CompStyle::Smooth => (ratio, attack, release, 12.0, true, c.auto_release),
        CompStyle::Punchy => (ratio, attack, release, 3.0, false, c.auto_release),
        CompStyle::Bus => (
            nearest_step(ratio.max(1.0), &CompStyle::BUS_RATIOS),
            nearest_step(attack.max(0.05), &CompStyle::BUS_ATTACKS),
            nearest_step(release.max(1.0), &CompStyle::BUS_RELEASES),
            4.0,
            false,
            c.auto_release,
        ),
    }
}

/// Where a steady level (dB) comes out, makeup and all: the compressor's curve, for drawing.
pub fn curve(c: &Comp, level: f32) -> f32 {
    let (ratio, _, _, knee, _, _) = settings(c);
    let strength = 1.0 - 1.0 / ratio.clamp(1.0, 20.0);
    level + gain_computer(level, c.threshold_db.clamp(-60.0, 0.0), strength, knee) + c.makeup_db
}

/// A second-order Butterworth high pass's coefficients (b0, b1, b2, a1, a2).
fn high_pass(hz: f32, rate: f32) -> [f32; 5] {
    let w = 2.0 * std::f32::consts::PI * (hz / rate).clamp(1e-5, 0.49);
    let (c, alpha) = (w.cos(), w.sin() / std::f32::consts::SQRT_2);
    let a0 = 1.0 + alpha;
    [
        (1.0 + c) / 2.0 / a0,
        -(1.0 + c) / a0,
        (1.0 + c) / 2.0 / a0,
        -2.0 * c / a0,
        (1.0 - alpha) / a0,
    ]
}

impl Compressor {
    pub fn new(rate: f64) -> Self {
        Compressor {
            rate: rate as f32,
            gain: [0.0; 2],
            slow: [0.0; 2],
            ms: [0.0; 2],
            hp: [[0.0; 4]; 2],
            hp_hz: 0.0,
            hp_coeffs: [1.0, 0.0, 0.0, 0.0, 0.0],
            makeup: 0.0,
            mix: 1.0,
            started: false,
            spectral: Spectral::new(rate as f32),
        }
    }

    /// Starts afresh, as an effect that's just come on: no reduction, the makeup and mix where
    /// they're set.
    pub fn reset(&mut self, c: &Comp) {
        *self = Compressor::new(self.rate as f64);
        self.makeup = c.makeup_db;
        self.mix = c.mix;
        self.started = true;
    }

    /// Runs a block through it in place; `gr` gets its gain reduction at each sample, in dB.
    pub fn process(&mut self, c: &Comp, l: &mut [f32], r: &mut [f32], gr: &mut [f32]) {
        if !self.started {
            self.reset(c);
        }
        let rate = self.rate;
        let (ratio, attack_ms, release_ms, knee, rms, auto) = settings(c);
        let threshold = c.threshold_db.clamp(-60.0, 0.0);
        let strength = 1.0 - 1.0 / ratio.clamp(1.0, 20.0);
        let attack = pole(attack_ms.clamp(0.05, 1000.0) * 0.001, rate);
        let release = match auto {
            true => pole(AUTO_FAST, rate),
            false => pole(release_ms.clamp(1.0, 5000.0) * 0.001, rate),
        };
        let (handover, slow_release) = (pole(AUTO_HANDOVER, rate), pole(AUTO_SLOW, rate));
        let rms_pole = pole(RMS_TAU, rate);
        // As a control glides: a pole for 44.1 Hz's worth.
        let glide = 1.0 - 44.1 / rate;
        let makeup = c.makeup_db.clamp(-24.0, 24.0);
        let mix = c.mix.clamp(0.0, 1.0);
        let sidechain = c.sidechain_hz.clamp(0.0, 500.0);
        let spectral = c.spectral.clamp(0.0, 1.0);
        if sidechain != self.hp_hz {
            self.hp_coeffs = high_pass(sidechain.max(1.0), rate);
            self.hp_hz = sidechain;
        }
        let [b0, b1, b2, a1, a2] = self.hp_coeffs;
        for i in 0..l.len().min(r.len()).min(gr.len()) {
            let input = [l[i], r[i]];
            for (k, &x) in input.iter().enumerate() {
                // What it hears: through the sidechain's high pass, if it has one.
                let heard = match sidechain > 0.0 {
                    true => {
                        let z = &mut self.hp[k];
                        let y = b0 * x + b1 * z[0] + b2 * z[1] - a1 * z[2] - a2 * z[3];
                        *z = [x, z[0], ftz(y), z[2]];
                        y
                    }
                    false => x,
                };
                let level = match rms {
                    true => {
                        self.ms[k] = ftz((1.0 - rms_pole) * heard * heard + rms_pole * self.ms[k]);
                        self.ms[k].sqrt()
                    }
                    false => heard.abs(),
                };
                let target = gain_computer(20.0 * level.log10(), threshold, strength, knee);
                let p = if target > self.gain[k] {
                    release
                } else {
                    attack
                };
                self.gain[k] = ftz((1.0 - p) * target + p * self.gain[k]);
                if auto {
                    // The slow one follows only a squeeze that lasts, and lets go slowly.
                    let q = if self.gain[k] < self.slow[k] {
                        handover
                    } else {
                        slow_release
                    };
                    self.slow[k] = ftz((1.0 - q) * self.gain[k] + q * self.slow[k]);
                }
            }
            self.makeup = (1.0 - glide) * makeup + glide * self.makeup;
            self.mix = (1.0 - glide) * mix + glide * self.mix;
            let mut g = self.gain[0].min(self.gain[1]);
            if auto {
                g = g.min(self.slow[0].min(self.slow[1]));
            }
            let up = 10f32.powf(self.makeup / 20.0);
            let (dry, [mut wl, mut wr]) = match self.spectral.running(spectral) {
                true => self.spectral.run([l[i], r[i]], g, spectral, rate),
                false => {
                    let k = 10f32.powf(g / 20.0);
                    ([l[i], r[i]], [l[i] * k, r[i] * k])
                }
            };
            (wl, wr) = (wl * up, wr * up);
            if c.style == CompStyle::Punchy {
                // A little colour where it's working hardest.
                (wl, wr) = (soft(wl), soft(wr));
            }
            l[i] = dry[0] + (wl - dry[0]) * self.mix;
            r[i] = dry[1] + (wr - dry[1]) * self.mix;
            gr[i] = -g;
        }
    }
}

/// A gentle soft clip: unity for quiet sounds, rounding off near full scale.
fn soft(x: f32) -> f32 {
    const DRIVE: f32 = 1.25;
    (DRIVE * x).tanh() / DRIVE
}

pub mod learn;
mod spectral;

#[cfg(test)]
mod tests;
