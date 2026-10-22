//! The built-in sampler: its zones and voice.

use super::*;

/// The built-in sampler: a sample on each note, as a drum
/// kit; one sample played across a range of notes, repitched, as an 808 or a bass; a loop sliced
/// across the keys; or layers by velocity and round robins. How every voice plays (envelopes,
/// filter, LFO, mono and glide) is the instrument's.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Sampler {
    pub zones: Vec<Zone>,
    #[serde(default, skip_serializing_if = "SamplerVoice::is_default")]
    pub voice: SamplerVoice,
}

/// What a sampler's filter does.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FilterMode {
    #[default]
    Off,
    LowPass,
    HighPass,
    BandPass,
}

/// How a zone's sample loops while its note is held.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LoopMode {
    #[default]
    Off,
    Forward,
    /// Forward, then back.
    PingPong,
}

/// How a sampler's voices play.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct SamplerVoice {
    /// The amp envelope: attack and decay (ms), the level held (0 to 1), and release (ms; a
    /// zone's own release, if it has one, instead).
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
    pub filter: FilterMode,
    pub cutoff_hz: f32,
    /// 0 to 1: a ringing peak at the cutoff as it rises.
    pub resonance: f32,
    /// How far the filter envelope moves the cutoff, in octaves (-6 to 6), and its shape.
    pub filter_env: f32,
    pub filter_attack_ms: f32,
    pub filter_decay_ms: f32,
    pub filter_sustain: f32,
    pub filter_release_ms: f32,
    /// How much the cutoff follows the note played: 1 an octave an octave.
    pub key_track: f32,
    /// How much velocity changes the level: 0 not at all, 1 as a drum does.
    pub velocity: f32,
    /// One note at a time; legato, a note played over another glides to it without starting
    /// over; glide, how long that takes (ms), mono or not.
    pub mono: bool,
    pub legato: bool,
    pub glide_ms: f32,
    /// The LFO: its rate (Hz), and how far it moves pitch (cents), the cutoff (octaves) and
    /// level (0 to 1).
    pub lfo_hz: f32,
    pub vibrato_cents: f32,
    pub lfo_filter: f32,
    pub tremolo: f32,
    /// Semitones, and dB, over every zone's own.
    pub transpose: f32,
    pub gain_db: f32,
}

impl Default for SamplerVoice {
    fn default() -> Self {
        SamplerVoice {
            attack_ms: 0.0,
            decay_ms: 0.0,
            sustain: 1.0,
            release_ms: 60.0,
            filter: FilterMode::Off,
            cutoff_hz: 18_000.0,
            resonance: 0.1,
            filter_env: 0.0,
            filter_attack_ms: 0.0,
            filter_decay_ms: 400.0,
            filter_sustain: 0.0,
            filter_release_ms: 200.0,
            key_track: 0.0,
            velocity: 1.0,
            mono: false,
            legato: false,
            glide_ms: 0.0,
            lfo_hz: 5.0,
            vibrato_cents: 0.0,
            lfo_filter: 0.0,
            tremolo: 0.0,
            transpose: 0.0,
            gain_db: 0.0,
        }
    }
}

impl SamplerVoice {
    pub fn is_default(&self) -> bool {
        *self == SamplerVoice::default()
    }
}

/// A sample, and the notes it plays on.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Zone {
    /// Relative to the project folder.
    pub file: PathBuf,
    /// What it's called: "Kick".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The note it plays on; with `high`, the lowest of a range.
    pub note: u8,
    /// The highest note of its range, if it has one: across it, it's repitched from `root`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub high: Option<u8>,
    /// The note the sample sounds at as recorded; `note` by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<u8>,
    /// Fine tuning, in cents: a sample a little sharp of its root comes down by this.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub tune_cents: f32,
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub gain_db: f32,
    /// -1 left to 1 right.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub pan: f32,
    /// Stops when the note does (fading over `release_ms`), rather than playing to its end as a
    /// drum hit does.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gate: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_ms: Option<f32>,
    /// Sounds in one group cut each other off, as a closed hat cuts off an open one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choke: Option<u8>,
    /// Where in the sample it plays from and to, as shares of its length (0 to 1): a slice of a
    /// loop, or a sound trimmed. All of it by default.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub start: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f32>,
    /// Looping while held, between these shares of the sample (its play range by default).
    #[serde(default, skip_serializing_if = "is_loop_off")]
    pub loop_mode: LoopMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_start: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_end: Option<f32>,
    /// Played backwards.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reverse: bool,
    /// The velocities it plays for (1 to 127): layers, a soft hit and a hard one. Zones that
    /// share notes and velocities take turns, as round robins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub velocities: Option<(u8, u8)>,
}

pub(crate) fn is_loop_off(m: &LoopMode) -> bool {
    *m == LoopMode::Off
}

impl Zone {
    /// Whether it plays a note.
    pub fn covers(&self, note: u8) -> bool {
        (self.note..=self.high.unwrap_or(self.note)).contains(&note)
    }

    /// Whether it plays a note at a velocity (1 to 127).
    pub fn takes(&self, note: u8, velocity: u8) -> bool {
        let (lo, hi) = self.velocities.unwrap_or((1, 127));
        self.covers(note) && (lo..=hi).contains(&velocity.max(1))
    }
}

pub(crate) fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
}
