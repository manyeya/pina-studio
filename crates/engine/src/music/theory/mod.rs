//! Writing music that's right, so an agent (which can't hear) picks and Pina Studio makes it correct:
//! chords from symbols or roman numerals, progressions by genre and mood, voicings led smoothly
//! from chord to chord, basslines and melodies that follow the harmony, and drum grooves by
//! genre with their swing, ghosts, rolls, variations and fills. All of it the same every time
//! for a seed.

mod bass;
mod chords;
mod comping;
mod drums;
mod melody;
mod progressions;
mod voicing;

pub use bass::*;
pub use chords::*;
pub use comping::*;
pub use drums::*;
pub use melody::*;
pub use progressions::*;
pub use voicing::*;

use serde::{Deserialize, Serialize};

use crate::{music::Key, song::project::Note};

/// A seeded random number, the same every time for a seed.
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03)
    }

    pub fn bits(&mut self) -> u64 {
        // SplitMix64.
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// 0 to n-1.
    pub fn below(&mut self, n: usize) -> usize {
        (self.bits() % n.max(1) as u64) as usize
    }

    /// 0 to 1.
    pub fn unit(&mut self) -> f32 {
        (self.bits() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }
}

#[cfg(test)]
mod tests;
