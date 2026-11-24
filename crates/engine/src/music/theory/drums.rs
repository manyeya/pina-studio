//! The drums of a General MIDI kit, and a step's velocity from its character.

/// A drum in a kit, on its General MIDI note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drum {
    Kick,
    Snare,
    Clap,
    Rim,
    HatClosed,
    HatOpen,
    Shaker,
    Perc,
    TomHigh,
    TomMid,
    TomLow,
    Crash,
    Ride,
}

impl Drum {
    pub fn note(self) -> u8 {
        match self {
            Drum::Kick => 36,
            Drum::Snare => 38,
            Drum::Clap => 39,
            Drum::Rim => 37,
            Drum::HatClosed => 42,
            Drum::HatOpen => 46,
            Drum::Shaker => 70,
            Drum::Perc => 56,
            Drum::TomHigh => 48,
            Drum::TomMid => 45,
            Drum::TomLow => 41,
            Drum::Crash => 49,
            Drum::Ride => 51,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Drum::Kick => "kick",
            Drum::Snare => "snare",
            Drum::Clap => "clap",
            Drum::Rim => "rim",
            Drum::HatClosed => "closed hat",
            Drum::HatOpen => "open hat",
            Drum::Shaker => "shaker",
            Drum::Perc => "perc",
            Drum::TomHigh => "high tom",
            Drum::TomMid => "mid tom",
            Drum::TomLow => "low tom",
            Drum::Crash => "crash",
            Drum::Ride => "ride",
        }
    }
}

/// A drum's step velocity, from its character.
pub(crate) fn hit(c: char) -> u8 {
    match c {
        'X' => 120,
        'x' => 100,
        'o' => 72,
        'g' => 42,
        _ => 0,
    }
}
