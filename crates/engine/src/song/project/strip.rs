//! The channel strip's compressor and saturator.

use super::*;

/// A channel's built-in compressor, after its EQ.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Comp {
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    #[serde(default)]
    pub makeup_db: f32,
    /// Its level is an RMS average over 15 ms rather than the peak: gentler, more like the ear.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rms: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bypass: bool,
}

impl Default for Comp {
    fn default() -> Self {
        Comp {
            threshold_db: -18.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 120.0,
            makeup_db: 0.0,
            rms: false,
            bypass: false,
        }
    }
}

/// A channel's built-in saturator, after its compressor: it drives the sound into a soft clip, as
/// tape or a tube would, adding harmonics. Quiet sounds keep their level; loud ones round off.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Saturator {
    /// How hard it's driven, in dB: 0 to 24.
    pub drive_db: f32,
    /// What it adds, darker (-1) to brighter (1).
    pub tone: f32,
    /// How much of the saturated sound is heard, 0 to 1; the rest is the dry sound.
    pub mix: f32,
    pub output_db: f32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bypass: bool,
}

impl Default for Saturator {
    fn default() -> Self {
        Saturator {
            drive_db: 6.0,
            tone: 0.0,
            mix: 1.0,
            output_db: 0.0,
            bypass: false,
        }
    }
}
