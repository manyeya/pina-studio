//! What automation can move, on what scale, and how a track plays it.

use super::*;

/// What of an EQ band automation moves.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EqField {
    Freq,
    Gain,
    Q,
}

/// A plugin on a track or aux: its instrument, or an insert by index.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Which {
    Instrument,
    Insert(usize),
}

/// How a track uses its automation: Read plays it; Touch and Latch play it and
/// also record what's moved while the song plays (Touch until the control is let go, Latch until
/// playback stops); Trim records as Touch does, but moves the line that's there up or down rather
/// than drawing a new one; Write records every automated control from where playback starts, over
/// what was there; Off ignores it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutoMode {
    Off,
    #[default]
    Read,
    Touch,
    Latch,
    Trim,
    Write,
}

impl AutoMode {
    pub const ALL: [AutoMode; 6] = [
        AutoMode::Off,
        AutoMode::Read,
        AutoMode::Touch,
        AutoMode::Latch,
        AutoMode::Trim,
        AutoMode::Write,
    ];

    pub(crate) fn is_read(&self) -> bool {
        *self == AutoMode::Read
    }

    pub fn name(self) -> &'static str {
        match self {
            AutoMode::Off => "Off",
            AutoMode::Read => "Read",
            AutoMode::Touch => "Touch",
            AutoMode::Latch => "Latch",
            AutoMode::Trim => "Trim",
            AutoMode::Write => "Write",
        }
    }

    /// Whether moving a control while playing records it.
    pub fn writes(self) -> bool {
        !matches!(self, AutoMode::Off | AutoMode::Read)
    }
}

/// An automatable parameter: the fader, the pan knob, the mute button, a send's level (by its
/// bus), a plugin's parameter by id, or an EQ band's frequency, gain or width (by its place).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Param {
    #[default]
    Volume,
    Pan,
    Mute,
    Send(u32),
    Plugin(Which, u32),
    Eq(u8, EqField),
    /// A modulator's setting, by the modulator's place on the track.
    Mod(u8, crate::dsp::modulate::ModParam),
}

impl Param {
    /// The values it takes, lowest and highest: dB for the fader and sends.
    pub fn range(self) -> (f32, f32) {
        match self {
            Param::Volume | Param::Send(_) => (SILENT_DB, 6.0),
            Param::Pan => (-1.0, 1.0),
            Param::Mute | Param::Plugin(..) => (0.0, 1.0),
            Param::Eq(_, EqField::Freq) => (10.0, 30_000.0),
            Param::Eq(_, EqField::Gain) => (-30.0, 30.0),
            Param::Eq(_, EqField::Q) => (0.025, 40.0),
            Param::Mod(_, p) => p.range(),
        }
    }

    /// Whether its scale is octaves (frequencies, widths, times) rather than steps.
    pub(crate) fn logarithmic(self) -> bool {
        use crate::dsp::modulate::ModParam;
        matches!(
            self,
            Param::Eq(_, EqField::Freq | EqField::Q)
                | Param::Mod(_, ModParam::Attack | ModParam::Decay | ModParam::Release)
        )
    }

    /// Whether it only ever jumps from value to value.
    pub fn steps(self) -> bool {
        self == Param::Mute
    }

    /// A value as a share of its range, levels as their faders show them (so silence isn't
    /// a hundred dB away from quiet).
    pub fn to_unit(self, v: f32) -> f32 {
        let (lo, hi) = self.range();
        match self {
            Param::Volume | Param::Send(_) => 10f32.powf((v - 6.0) / 40.0).clamp(0.0, 1.0),
            p if p.logarithmic() => ((v.max(lo) / lo).ln() / (hi / lo).ln()).clamp(0.0, 1.0),
            _ => ((v - lo) / (hi - lo)).clamp(0.0, 1.0),
        }
    }

    pub fn from_unit(self, u: f32) -> f32 {
        let (lo, hi) = self.range();
        match self {
            Param::Volume | Param::Send(_) if u <= 1e-4 => SILENT_DB,
            Param::Volume | Param::Send(_) => (40.0 * u.log10() + 6.0).max(SILENT_DB),
            p if p.logarithmic() => lo * (hi / lo).powf(u.clamp(0.0, 1.0)),
            _ => lo + u.clamp(0.0, 1.0) * (hi - lo),
        }
    }
}
