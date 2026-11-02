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

/// What of a limiter automation (and modulation) moves: its gain and ceiling (dB), release (ms)
/// and clip (0 to 1).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LimitParam {
    Gain,
    Ceiling,
    Release,
    Clip,
}

impl LimitParam {
    pub const ALL: [LimitParam; 4] = [
        LimitParam::Gain,
        LimitParam::Ceiling,
        LimitParam::Release,
        LimitParam::Clip,
    ];

    pub fn name(self) -> &'static str {
        match self {
            LimitParam::Gain => "Gain",
            LimitParam::Ceiling => "Ceiling",
            LimitParam::Release => "Release",
            LimitParam::Clip => "Clip",
        }
    }

    pub fn range(self) -> (f32, f32) {
        match self {
            LimitParam::Gain => (0.0, 30.0),
            LimitParam::Ceiling => (-30.0, 0.0),
            LimitParam::Release => (1.0, 2000.0),
            LimitParam::Clip => (0.0, 1.0),
        }
    }

    pub fn get(self, l: &Limiter) -> f32 {
        match self {
            LimitParam::Gain => l.gain_db,
            LimitParam::Ceiling => l.ceiling_db,
            LimitParam::Release => l.release_ms,
            LimitParam::Clip => l.clip,
        }
    }

    /// Sets it, kept in its range.
    pub fn set(self, l: &mut Limiter, v: f32) {
        let (lo, hi) = self.range();
        let v = v.clamp(lo, hi);
        match self {
            LimitParam::Gain => l.gain_db = v,
            LimitParam::Ceiling => l.ceiling_db = v,
            LimitParam::Release => l.release_ms = v,
            LimitParam::Clip => l.clip = v,
        }
    }
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
/// bus), a plugin's parameter by id, an EQ band's frequency, gain or width (by its place), or a
/// setting of the limiter's.
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
    Limit(LimitParam),
    /// A knob of a stacked effect (the effect's place among them, or the track's own compressor
    /// or saturator) by its place in the effect's table: where the knob sits, 0 to 1.
    Tool(u8, u8),
    /// A setting of the Sampler's voice, by its place in `VOICE`: where it sits, 0 to 1.
    Voice(u8),
    /// One of the synth's parameters, by its place: where its knob sits, 0 to 1.
    Poly(u16),
}

impl Param {
    /// The values it takes, lowest and highest: dB for the fader and sends.
    pub fn range(self) -> (f32, f32) {
        match self {
            Param::Volume | Param::Send(_) => (SILENT_DB, 6.0),
            Param::Pan => (-1.0, 1.0),
            Param::Mute
            | Param::Plugin(..)
            | Param::Tool(..)
            | Param::Voice(_)
            | Param::Poly(_) => (0.0, 1.0),
            Param::Eq(_, EqField::Freq) => (10.0, 30_000.0),
            Param::Eq(_, EqField::Gain) => (-30.0, 30.0),
            Param::Eq(_, EqField::Q) => (0.025, 40.0),
            Param::Mod(_, p) => p.range(),
            Param::Limit(p) => p.range(),
        }
    }

    /// Whether its scale is octaves (frequencies, widths, times) rather than steps.
    pub(crate) fn logarithmic(self) -> bool {
        use crate::dsp::modulate::ModParam;
        matches!(
            self,
            Param::Eq(_, EqField::Freq | EqField::Q)
                | Param::Mod(_, ModParam::Attack | ModParam::Decay | ModParam::Release)
                | Param::Limit(LimitParam::Release)
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
