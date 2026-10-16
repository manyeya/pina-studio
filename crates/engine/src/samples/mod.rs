//! The sample library: every sample in the folders it's given, analysed (what it is, its pitch,
//! key, tempo, sound and character) and kept in an index, so people and agents can find what
//! they want, and what sounds good together.
//!
//! - `analyze`: what a sample is, from its audio (and its name, for hints).
//! - `library`: the folders, the index, and keeping it up to date in the background.
//! - `search`: finding samples by words, filters, likeness and fit; building kits.

pub mod analyze;
pub mod library;
pub mod search;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A sample in the library: where it is, and what analysis found.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Sample {
    pub path: PathBuf,
    /// The file's name, less its extension.
    pub name: String,
    /// Its folders under the library folder it's in, like "90s Hip Hop/Drum_Hits": often the pack.
    pub folder: String,
    /// The file's modification time (seconds) and size, to know when to look again.
    pub modified: u64,
    pub size: u64,
    pub analysis: Analysis,
}

/// What a sample is and how it sounds.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Analysis {
    pub seconds: f32,
    pub sample_rate: u32,
    pub channels: u16,
    /// A one-shot (a hit, a note) or a loop.
    pub kind: Kind,
    pub category: Category,
    /// 0 to 1: how sure the category is.
    pub confidence: f32,
    /// The note it plays, for something with one pitch (a bass note, an 808, a tuned kick).
    pub pitch: Option<Pitch>,
    /// The key it's in, for something with several notes (a chord, a melodic loop).
    pub key: Option<KeyGuess>,
    /// How its tonal energy spreads over the pitch classes, C to B, summing to 1.
    pub chroma: [f32; 12],
    /// 0 for noise (a hat, a crash), 1 for a clear pitch or chord.
    pub tonal: f32,
    /// A loop's tempo, and how many 4/4 bars it is at that tempo.
    pub bpm: Option<f32>,
    pub bars: Option<f32>,
    /// Loudness (LUFS, of its loudest part for a short sound) and peak (dBFS).
    pub loudness: f32,
    pub peak_db: f32,
    pub envelope: Envelope,
    pub spectrum: Spectrum,
    /// 0 mono to 1 wide.
    pub width: f32,
    /// Words for how it sounds, from `TRAITS`: "punchy", "dark", "wet"...
    pub traits: Vec<String>,
    /// Its timbre as a vector, `EMBEDDING` long and of length 1: near vectors sound alike.
    pub embedding: Vec<f32>,
    /// The same for the same sound: finds duplicates, whatever their names.
    pub fingerprint: u64,
    /// Its outline, 0 to 255, `WAVEFORM` points long, for drawing.
    pub waveform: Vec<u8>,
}

/// How long `Analysis::embedding` is.
pub const EMBEDDING: usize = 32;
/// How many points `Analysis::waveform` has.
pub const WAVEFORM: usize = 96;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    OneShot,
    Loop,
}

/// What a sample is. One-shots and loops share these: a `Kick` loop is a kick pattern, `Drums` a
/// whole beat.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Kick,
    Snare,
    Clap,
    Rim,
    Snap,
    HatClosed,
    HatOpen,
    Ride,
    Crash,
    Tom,
    Shaker,
    Tambourine,
    Perc,
    /// A whole beat.
    Drums,
    #[serde(rename = "808")]
    Bass808,
    Bass,
    Keys,
    Pad,
    Lead,
    Pluck,
    Bell,
    Strings,
    Brass,
    Woodwind,
    Guitar,
    Synth,
    /// A chord or stab hit.
    Chord,
    Vocal,
    /// Risers, sweeps, impacts, downlifters.
    Fx,
    Reverse,
    /// Vinyl, noise, ambience, field recordings.
    Texture,
    Foley,
    #[default]
    Other,
}

impl Category {
    pub const ALL: [Category; 33] = [
        Category::Kick,
        Category::Snare,
        Category::Clap,
        Category::Rim,
        Category::Snap,
        Category::HatClosed,
        Category::HatOpen,
        Category::Ride,
        Category::Crash,
        Category::Tom,
        Category::Shaker,
        Category::Tambourine,
        Category::Perc,
        Category::Drums,
        Category::Bass808,
        Category::Bass,
        Category::Keys,
        Category::Pad,
        Category::Lead,
        Category::Pluck,
        Category::Bell,
        Category::Strings,
        Category::Brass,
        Category::Woodwind,
        Category::Guitar,
        Category::Synth,
        Category::Chord,
        Category::Vocal,
        Category::Fx,
        Category::Reverse,
        Category::Texture,
        Category::Foley,
        Category::Other,
    ];

    /// Its name for people: "Closed Hat", "808".
    pub fn name(self) -> &'static str {
        match self {
            Category::Kick => "Kick",
            Category::Snare => "Snare",
            Category::Clap => "Clap",
            Category::Rim => "Rim",
            Category::Snap => "Snap",
            Category::HatClosed => "Closed Hat",
            Category::HatOpen => "Open Hat",
            Category::Ride => "Ride",
            Category::Crash => "Crash",
            Category::Tom => "Tom",
            Category::Shaker => "Shaker",
            Category::Tambourine => "Tambourine",
            Category::Perc => "Perc",
            Category::Drums => "Drums",
            Category::Bass808 => "808",
            Category::Bass => "Bass",
            Category::Keys => "Keys",
            Category::Pad => "Pad",
            Category::Lead => "Lead",
            Category::Pluck => "Pluck",
            Category::Bell => "Bell",
            Category::Strings => "Strings",
            Category::Brass => "Brass",
            Category::Woodwind => "Woodwind",
            Category::Guitar => "Guitar",
            Category::Synth => "Synth",
            Category::Chord => "Chord",
            Category::Vocal => "Vocal",
            Category::Fx => "FX",
            Category::Reverse => "Reverse",
            Category::Texture => "Texture",
            Category::Foley => "Foley",
            Category::Other => "Other",
        }
    }

    /// A drum or percussion sound: no pitch to match to a key.
    pub fn is_drum(self) -> bool {
        matches!(
            self,
            Category::Kick
                | Category::Snare
                | Category::Clap
                | Category::Rim
                | Category::Snap
                | Category::HatClosed
                | Category::HatOpen
                | Category::Ride
                | Category::Crash
                | Category::Tom
                | Category::Shaker
                | Category::Tambourine
                | Category::Perc
                | Category::Drums
        )
    }

    /// The General MIDI drum note it goes on in a kit, if it's a drum.
    pub fn drum_note(self) -> Option<u8> {
        Some(match self {
            Category::Kick => 36,
            Category::Snare => 38,
            Category::Clap => 39,
            Category::Rim => 37,
            Category::Snap => 40,
            Category::HatClosed => 42,
            Category::HatOpen => 46,
            Category::Ride => 51,
            Category::Crash => 49,
            Category::Tom => 45,
            Category::Shaker => 70,
            Category::Tambourine => 54,
            Category::Perc => 56,
            _ => return None,
        })
    }
}

/// A note: MIDI number (60 is middle C), how far off it is in cents, its frequency, and how sure.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Pitch {
    pub note: u8,
    pub cents: f32,
    pub hz: f32,
    pub confidence: f32,
}

/// A key: its tonic (0 is C), major or minor, its name ("C minor"), and how well the sample
/// fits it (-1 to 1).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct KeyGuess {
    pub tonic: u8,
    pub minor: bool,
    pub name: String,
    pub confidence: f32,
}

/// How it starts and dies away, in milliseconds: to its peak, to 20 dB under it, to 60 dB under
/// (its tail, and any reverb), and how much of its peak it holds through its middle (0 to 1).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct Envelope {
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub tail_ms: f32,
    pub sustain: f32,
}

/// Where its energy is: centroid and 85% roll-off (Hz), flatness (0 tonal to 1 noise), and six
/// bands (sub < 60 Hz, bass, low mid, mid, presence, air > 8 kHz) in dB under the total.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct Spectrum {
    pub centroid_hz: f32,
    pub rolloff_hz: f32,
    pub flatness: f32,
    pub bands_db: [f32; 6],
}

/// The words `Analysis::traits` uses, each with what it means.
pub const TRAITS: [(&str, &str); 22] = [
    ("punchy", "a hard, fast attack that stands out"),
    ("soft", "a slow or gentle attack"),
    ("snappy", "short and bright, with a crack"),
    ("short", "over in a moment"),
    ("long", "rings or sustains for seconds"),
    ("bright", "much of its energy high up"),
    ("dark", "its energy low, little top"),
    ("warm", "full low mids, gentle top"),
    ("airy", "sparkle above 8 kHz"),
    ("sub", "deep energy under 60 Hz"),
    ("boomy", "heavy low end that rings"),
    ("thin", "little low end"),
    ("tonal", "a clear pitch or chord"),
    ("noisy", "noise more than pitch"),
    ("dry", "no room or reverb on it"),
    ("wet", "a reverb or room tail"),
    ("wide", "a broad stereo image"),
    ("mono", "the same on both sides"),
    ("distorted", "clipped, saturated or crushed"),
    (
        "lo-fi",
        "band-limited or gritty, like an old sampler or tape",
    ),
    ("clean", "clear and undistorted"),
    ("loud", "loud for its kind: mastered, compressed"),
];

/// Pitch class names, C to B, as the library writes keys and notes.
pub const NOTES: [&str; 12] = [
    "C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
];

/// A note's name, like "G1" (60 is C4).
pub fn note_name(note: u8) -> String {
    format!("{}{}", NOTES[note as usize % 12], note as i32 / 12 - 1)
}
