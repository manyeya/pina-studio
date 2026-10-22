//! The fitted constants the analysis uses: written by `examples/sample_eval.rs --fit` from
//! MusicRadar's free SampleRadar packs (808 drums, 808 weight, essential drum kits, drum
//! machines, analogue drums, trap, world percussion, lo-fi, synth chords, jazz keys, glitch
//! vocals, synth impacts, hip-hop essentials, hip hop, bass guitar, MIDI orchestra, noise and
//! crackle, soul and funk), the 90s Hip Hop pack held out.
// Fitted numbers, some near a constant by chance.
#![allow(clippy::approx_constant)]

use super::Category;

/// A classifier over a standardised feature vector: a hidden ReLU layer (if it has one), then
/// a softmax.
pub struct Net {
    pub classes: &'static [Category],
    pub mean: &'static [f32],
    pub scale: &'static [f32],
    pub hidden: &'static [&'static [f32]],
    pub hidden_bias: &'static [f32],
    pub weights: &'static [&'static [f32]],
    pub bias: &'static [f32],
}

mod embed;
mod kind;
mod loops;
mod one_shot;
mod traits;

pub use embed::*;
pub use kind::*;
pub use loops::*;
pub use one_shot::*;
pub use traits::*;
