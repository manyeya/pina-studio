//! What moves a sampler voice: its envelopes, the LFOs' waves, and the matrix that routes
//! them (and velocity, key, the mod wheel, pressure, a random value per note, the bend) to its
//! settings.

use crate::song::project::{ModDest, ModSource, Routes, Wave};

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub enum Stage {
    Attack,
    Decay,
    Sustain,
    Release,
    #[default]
    Done,
}

/// An envelope as it goes.
#[derive(Clone, Copy, Default)]
pub struct Env {
    pub stage: Stage,
    pub level: f32,
}

/// An envelope's shape at a sample rate: the attack's rise a sample, the decay's and release's
/// fall (as a factor a sample toward their goal), and the level held.
#[derive(Clone, Copy)]
pub struct Shape {
    rise: f32,
    decay: f32,
    sustain: f32,
    pub release: f32,
}

/// The factor a sample that falls 60 dB over `ms`.
pub fn fall(ms: f32, rate: f32) -> f32 {
    if ms <= 0.05 {
        0.0
    } else {
        (-6.9 / (ms * 0.001 * rate)).exp()
    }
}

impl Shape {
    pub fn new(attack_ms: f32, decay_ms: f32, sustain: f32, release_ms: f32, rate: f32) -> Shape {
        Shape {
            rise: if attack_ms <= 0.05 {
                1.0
            } else {
                1.0 / (attack_ms * 0.001 * rate)
            },
            decay: fall(decay_ms, rate),
            sustain: sustain.clamp(0.0, 1.0),
            release: fall(release_ms, rate),
        }
    }
}

impl Env {
    pub fn started() -> Env {
        Env {
            stage: Stage::Attack,
            level: 0.0,
        }
    }

    pub fn release(&mut self) {
        if self.stage != Stage::Done {
            self.stage = Stage::Release;
        }
    }

    pub fn next(&mut self, s: &Shape, release: f32) -> f32 {
        match self.stage {
            Stage::Attack => {
                self.level += s.rise;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level = s.sustain + (self.level - s.sustain) * s.decay;
                if (self.level - s.sustain).abs() < 1e-4 {
                    self.level = s.sustain;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => self.level = s.sustain,
            Stage::Release => {
                self.level *= release;
                if self.level < 1e-3 {
                    self.level = 0.0;
                    self.stage = Stage::Done;
                }
            }
            Stage::Done => self.level = 0.0,
        }
        self.level
    }
}

/// A random value from -1 to 1 for a number: the same number, the same value.
pub fn random(n: u64) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    (x >> 40) as f32 / (1u64 << 23) as f32 - 1.0
}

/// A wave at `phase` (cycles since it started; the random ones draw a new value each cycle,
/// from `seed`), -1 to 1.
pub fn wave(w: Wave, phase: f64, seed: u64) -> f32 {
    let p = phase.rem_euclid(1.0) as f32;
    let cycle = phase.floor() as i64 as u64;
    match w {
        Wave::Sine => (p * std::f32::consts::TAU).sin(),
        Wave::Triangle => 1.0 - 4.0 * ((p + 0.25).rem_euclid(1.0) - 0.5).abs(),
        Wave::SawUp => 2.0 * p - 1.0,
        Wave::SawDown => 1.0 - 2.0 * p,
        Wave::Square => match p < 0.5 {
            true => 1.0,
            false => -1.0,
        },
        Wave::Random => random(cycle ^ seed),
        Wave::Drift => {
            let (a, b) = (random(cycle ^ seed), random(cycle.wrapping_add(1) ^ seed));
            let s = p * p * (3.0 - 2.0 * p);
            a + (b - a) * s
        }
    }
}

/// Every source's value for a voice now, by `ModSource` (None 0).
pub type Sources = [f32; 12];

pub fn source_index(s: ModSource) -> usize {
    s as usize
}

/// How far the matrix moves each setting, by `ModDest`: the sum of its routes' amounts times
/// their sources.
pub fn routed(matrix: &Routes, sources: &Sources) -> [f32; 15] {
    let mut out = [0f32; 15];
    for r in matrix.iter() {
        out[r.dest as usize] += r.amount * sources[source_index(r.source)];
    }
    out
}

pub fn dest(d: ModDest) -> usize {
    d as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song::project::Route;

    #[test]
    fn routes_add_up_and_random_waves_hold_or_drift() {
        let matrix = Routes::from_slice(&[
            Route {
                source: ModSource::Velocity,
                dest: ModDest::Cutoff,
                amount: 0.5,
            },
            Route {
                source: ModSource::Lfo1,
                dest: ModDest::Cutoff,
                amount: -1.0,
            },
        ]);
        let mut s: Sources = [0.0; 12];
        s[source_index(ModSource::Velocity)] = 1.0;
        s[source_index(ModSource::Lfo1)] = 0.25;
        assert!((routed(&matrix, &s)[dest(ModDest::Cutoff)] - 0.25).abs() < 1e-6);
        // Random holds through a cycle; drift moves from one value to the next.
        assert_eq!(wave(Wave::Random, 3.1, 7), wave(Wave::Random, 3.9, 7));
        assert_ne!(wave(Wave::Random, 3.1, 7), wave(Wave::Random, 4.1, 7));
        assert!((wave(Wave::Drift, 4.0, 7) - wave(Wave::Random, 4.5, 7)).abs() < 1e-6);
        assert!((-1.0..=1.0).contains(&random(12345)));
    }
}
