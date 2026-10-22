//! Step patterns, as the step sequencer makes them: rows of steps, each step's own settings (its
//! velocity or value, gate, tie, note, octave, repeat, chance, offset, rate, skip), how each row
//! runs through them (its loop, rate and playback mode), and the notes and automation they play.

mod edit;
mod play;

pub use edit::*;
pub use play::*;

use std::ops::Not;

use super::*;
use crate::dsp::modulate::ModParam;

pub(crate) fn sixteenth() -> u64 {
    PPQ / 4
}

pub(crate) fn straight() -> f32 {
    50.0
}

pub(crate) fn is_straight(v: &f32) -> bool {
    *v == 50.0
}

fn is_zero_usize(v: &usize) -> bool {
    *v == 0
}

fn is_zero_u8(v: &u8) -> bool {
    *v == 0
}

/// What a pattern made before steps had every setting is upgraded from (see `Pattern::upgrade`).
const VERSION: u8 = 1;

fn legacy() -> u8 {
    0
}

/// The most steps a pattern has.
pub const MAX_STEPS: usize = 64;

/// The rates a pattern, a row or a single step can run at, by name and ticks: whole notes and
/// longer, straight, then triplets, then dotted.
pub const RATES: [(&str, u64); 22] = [
    ("4", PPQ * 16),
    ("2", PPQ * 8),
    ("1", PPQ * 4),
    ("/2", PPQ * 2),
    ("/4", PPQ),
    ("/8", PPQ / 2),
    ("/16", PPQ / 4),
    ("/32", PPQ / 8),
    ("/64", PPQ / 16),
    ("1T", PPQ * 8 / 3),
    ("2T", PPQ * 4 / 3),
    ("4T", PPQ * 2 / 3),
    ("8T", PPQ / 3),
    ("16T", PPQ / 6),
    ("32T", PPQ / 12),
    ("1D", PPQ * 6),
    ("2D", PPQ * 3),
    ("4D", PPQ * 3 / 2),
    ("8D", PPQ * 3 / 4),
    ("16D", PPQ * 3 / 8),
    ("32D", PPQ * 3 / 16),
    ("64D", PPQ * 3 / 32),
];

/// A rate's name, like "/16" or "8T"; one that isn't in the list by its ticks.
pub fn rate_name(ticks: u64) -> String {
    RATES
        .iter()
        .find(|r| r.1 == ticks)
        .map_or_else(|| format!("{ticks}t"), |r| r.0.to_string())
}

/// How a row (or every row, as the pattern's) runs through its steps.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PlayMode {
    /// First to last, over and over.
    #[default]
    Forward,
    /// Last to first.
    Backward,
    /// There and back, the end steps played twice.
    PingPong,
    /// There and back, the end steps once.
    Pendulum,
    /// A step on or a step back, at random.
    Wander,
    /// Every step once a time round, in a new order each time.
    Shuffle,
    /// Any step, at random.
    Random,
}

impl PlayMode {
    pub const ALL: [PlayMode; 7] = [
        PlayMode::Forward,
        PlayMode::Backward,
        PlayMode::PingPong,
        PlayMode::Pendulum,
        PlayMode::Wander,
        PlayMode::Shuffle,
        PlayMode::Random,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PlayMode::Forward => "Forward",
            PlayMode::Backward => "Backward",
            PlayMode::PingPong => "Ping-Pong",
            PlayMode::Pendulum => "Pendulum",
            PlayMode::Wander => "Wander",
            PlayMode::Shuffle => "Shuffle",
            PlayMode::Random => "Random",
        }
    }

    fn is_forward(&self) -> bool {
        *self == PlayMode::Forward
    }
}

/// A scale a pattern's notes keep to, from its key.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Scale {
    Major,
    Minor,
    HarmonicMinor,
    MelodicMinor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Locrian,
    MajorPentatonic,
    MinorPentatonic,
    Blues,
}

impl Scale {
    pub const ALL: [Scale; 12] = [
        Scale::Major,
        Scale::Minor,
        Scale::HarmonicMinor,
        Scale::MelodicMinor,
        Scale::Dorian,
        Scale::Phrygian,
        Scale::Lydian,
        Scale::Mixolydian,
        Scale::Locrian,
        Scale::MajorPentatonic,
        Scale::MinorPentatonic,
        Scale::Blues,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Scale::Major => "Major",
            Scale::Minor => "Natural Minor",
            Scale::HarmonicMinor => "Harmonic Minor",
            Scale::MelodicMinor => "Melodic Minor",
            Scale::Dorian => "Dorian",
            Scale::Phrygian => "Phrygian",
            Scale::Lydian => "Lydian",
            Scale::Mixolydian => "Mixolydian",
            Scale::Locrian => "Locrian",
            Scale::MajorPentatonic => "Major Pentatonic",
            Scale::MinorPentatonic => "Minor Pentatonic",
            Scale::Blues => "Blues",
        }
    }

    /// Its notes, in semitones from the key.
    pub fn degrees(self) -> &'static [u8] {
        match self {
            Scale::Major => &[0, 2, 4, 5, 7, 9, 11],
            Scale::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Scale::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            Scale::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Scale::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Scale::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Scale::Locrian => &[0, 1, 3, 5, 6, 8, 10],
            Scale::MajorPentatonic => &[0, 2, 4, 7, 9],
            Scale::MinorPentatonic => &[0, 3, 5, 7, 10],
            Scale::Blues => &[0, 3, 5, 6, 7, 10],
        }
    }

    pub fn contains(self, key: u8, pitch: u8) -> bool {
        self.degrees()
            .contains(&((pitch as i32 - key as i32).rem_euclid(12) as u8))
    }

    /// The nearest note of the scale (the lower of two as near).
    pub fn snap(self, key: u8, pitch: u8) -> u8 {
        (0..=6i32)
            .flat_map(|d| [-d, d])
            .map(|d| (pitch as i32 + d).clamp(0, 127) as u8)
            .find(|&p| self.contains(key, p))
            .unwrap_or(pitch)
    }
}

/// What an automation row's steps set: a parameter of the track, a MIDI controller, pitch bend or
/// channel pressure.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RowTarget {
    Volume,
    Pan,
    Mute,
    Send {
        bus: u32,
    },
    Plugin {
        plugin: Which,
        param: u32,
    },
    Eq {
        band: u8,
        field: EqField,
    },
    Modulator {
        modulator: u8,
        field: ModParam,
    },
    /// A MIDI controller, 0 to 127.
    Cc(u8),
    PitchBend,
    Pressure,
}

impl RowTarget {
    /// The track's parameter it moves, if it's one.
    pub fn param(self) -> Option<Param> {
        Some(match self {
            RowTarget::Volume => Param::Volume,
            RowTarget::Pan => Param::Pan,
            RowTarget::Mute => Param::Mute,
            RowTarget::Send { bus } => Param::Send(bus),
            RowTarget::Plugin { plugin, param } => Param::Plugin(plugin, param),
            RowTarget::Eq { band, field } => Param::Eq(band, field),
            RowTarget::Modulator { modulator, field } => Param::Mod(modulator, field),
            RowTarget::Cc(_) | RowTarget::PitchBend | RowTarget::Pressure => return None,
        })
    }

    pub fn from_param(p: Param) -> Self {
        match p {
            Param::Volume => RowTarget::Volume,
            Param::Pan => RowTarget::Pan,
            Param::Mute => RowTarget::Mute,
            Param::Send(bus) => RowTarget::Send { bus },
            Param::Plugin(plugin, param) => RowTarget::Plugin { plugin, param },
            Param::Eq(band, field) => RowTarget::Eq { band, field },
            Param::Mod(modulator, field) => RowTarget::Modulator { modulator, field },
        }
    }

    /// Its name where nothing better is known (a plugin's parameter is named by its plugin).
    pub fn name(self) -> String {
        match self {
            RowTarget::Cc(1) => "Modulation (CC 1)".into(),
            RowTarget::Cc(7) => "Volume (CC 7)".into(),
            RowTarget::Cc(10) => "Pan (CC 10)".into(),
            RowTarget::Cc(11) => "Expression (CC 11)".into(),
            RowTarget::Cc(64) => "Sustain (CC 64)".into(),
            RowTarget::Cc(n) => format!("CC {n}"),
            RowTarget::PitchBend => "Pitch Bend".into(),
            RowTarget::Pressure => "Channel Pressure".into(),
            t => match t.param() {
                Some(Param::Volume) => "Volume".into(),
                Some(Param::Pan) => "Pan".into(),
                Some(Param::Mute) => "Mute".into(),
                Some(Param::Send(_)) => "Send".into(),
                Some(Param::Plugin(..)) => "Plugin parameter".into(),
                Some(Param::Eq(band, field)) => format!("EQ {} {:?}", band + 1, field),
                Some(Param::Mod(m, field)) => format!("Modulator {} {:?}", m + 1, field),
                None => String::new(),
            },
        }
    }
}

/// When a step plays, besides its chance: always, on some passes of a few (the second of every
/// three), on the first pass through the region only, or the last.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    #[default]
    Always,
    Recurrence,
    FirstLoop,
    LastLoop,
}

impl Condition {
    pub const ALL: [Condition; 4] = [
        Condition::Always,
        Condition::Recurrence,
        Condition::FirstLoop,
        Condition::LastLoop,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Condition::Always => "Always",
            Condition::Recurrence => "Recurrence",
            Condition::FirstLoop => "First Loop",
            Condition::LastLoop => "Last Loop",
        }
    }
}

/// A setting every step of a row has, edited one step at a time: what the step sequencer's edit
/// modes (beside On/Off and Loop) show and draw, and what a row's subrows are.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Lane {
    /// A note row's velocity, an automation row's value.
    Velocity,
    Gate,
    Tie,
    Note,
    Octave,
    Repeat,
    Chance,
    Offset,
    Rate,
    Skip,
}

impl Lane {
    pub const ALL: [Lane; 10] = [
        Lane::Velocity,
        Lane::Gate,
        Lane::Tie,
        Lane::Note,
        Lane::Octave,
        Lane::Repeat,
        Lane::Chance,
        Lane::Offset,
        Lane::Rate,
        Lane::Skip,
    ];

    /// Whether a row of its kind has it: notes have all, automation rows no gate, note, octave
    /// or repeat.
    pub fn applies(self, automation: bool) -> bool {
        !automation || !matches!(self, Lane::Gate | Lane::Note | Lane::Octave | Lane::Repeat)
    }

    /// On or off, rather than a value to drag.
    pub fn switch(self) -> bool {
        matches!(self, Lane::Tie | Lane::Skip)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct PatternRow {
    /// The note it plays (for an automation row, unused).
    pub pitch: u8,
    /// A velocity per step; 0 is off. A row shorter than the pattern loops on its own length
    /// (as `loop_end` would have it).
    pub cells: Vec<u8>,
    /// Its own name, over the note's or the parameter's.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// An automation row's parameter: its steps set it rather than playing notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<RowTarget>,
    /// Its steps play notes of their own (by their note and octave), not one drum or note.
    #[serde(default, skip_serializing_if = "Not::not")]
    pub melodic: bool,
    /// Silent, its steps kept.
    #[serde(default, skip_serializing_if = "Not::not")]
    pub mute: bool,
    /// Heard alone, with the other soloed rows.
    #[serde(default, skip_serializing_if = "Not::not")]
    pub solo: bool,
    /// The steps it plays, round and round: from `loop_start` up to (not including) `loop_end`;
    /// none to the end of the pattern.
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub loop_start: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_end: Option<usize>,
    /// Its own step rate (ticks), and how it runs through its steps; none follows the pattern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<PlayMode>,
    /// An automation row slides from value to value, rather than holding each to the next.
    #[serde(default, skip_serializing_if = "Not::not")]
    pub slide: bool,
    /// Its colour (0xrrggbb), over the one its sound gives it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
    /// Its subrows: open, and the settings they show.
    #[serde(default, skip_serializing_if = "Not::not")]
    pub open: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subrows: Vec<Lane>,
    /// Each step's own settings, a lane each, as long as `cells` (left out, or empty, as usual):
    /// the velocity an off step comes back on with; the chance it plays (0 to 100); semitones up
    /// or down (-12 to 12) and octaves; how many times it hits in its step (1 to 16); how long it
    /// holds (% of a step, 1 to 100); whether it holds on into the next step (a tie); how far off
    /// the grid it is (% of a step, -50 to 50); its own rate (ticks, 0 the row's); whether it's
    /// skipped; an automation row's value (0 to 1); when it plays, besides its chance; and how
    /// its repeats go (velocity ramp and curve, distribution: %, -100 to 100).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub held: Vec<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chance: Vec<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transpose: Vec<i8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub octave: Vec<i8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repeat: Vec<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gate: Vec<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tie: Vec<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nudge: Vec<i8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rates: Vec<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skip: Vec<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub value: Vec<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub condition: Vec<Condition>,
    /// Recurrence: plays on the first of these passes of every second.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub every: Vec<[u8; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ramp: Vec<i8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub curve: Vec<i8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spread: Vec<i8>,
}

/// A step's velocity when it's turned on, and an automation step's value, as usual.
pub const VELOCITY: u8 = 100;
pub const VALUE: f32 = 0.5;

impl PatternRow {
    /// A note row, `steps` long, every step off.
    pub fn note(pitch: u8, steps: usize) -> Self {
        PatternRow {
            pitch,
            cells: vec![0; steps],
            ..Default::default()
        }
    }

    /// An automation row, `steps` long, every step off at `value`.
    pub fn automation(target: RowTarget, steps: usize, value: f32) -> Self {
        PatternRow {
            target: Some(target),
            cells: vec![0; steps],
            value: vec![value.clamp(0.0, 1.0); steps],
            ..Default::default()
        }
    }

    pub fn is_automation(&self) -> bool {
        self.target.is_some()
    }

    pub fn on(&self, i: usize) -> bool {
        self.cells.get(i).is_some_and(|&v| v > 0)
    }

    /// Turns a step on (with the velocity it had) or off (keeping it).
    pub fn set_on(&mut self, i: usize, on: bool) {
        let Some(&v) = self.cells.get(i) else {
            return;
        };
        match (on, v > 0) {
            (true, false) => {
                let back = self.held.get(i).copied().filter(|&h| h > 0);
                self.cells[i] = back.unwrap_or(VELOCITY);
                if i < self.held.len() {
                    self.held[i] = 0;
                }
            }
            (false, true) => {
                if v != VELOCITY {
                    self.held.resize(self.cells.len(), 0);
                    self.held[i] = v;
                }
                self.cells[i] = 0;
            }
            _ => {}
        }
    }

    /// A step's velocity, on or off.
    pub fn velocity_at(&self, i: usize) -> u8 {
        match self.cells.get(i).copied().unwrap_or(0) {
            0 => self
                .held
                .get(i)
                .copied()
                .filter(|&h| h > 0)
                .unwrap_or(VELOCITY),
            v => v,
        }
    }
    pub fn chance_at(&self, i: usize) -> u8 {
        self.chance.get(i).copied().unwrap_or(CHANCE).min(100)
    }
    pub fn transpose_at(&self, i: usize) -> i8 {
        self.transpose.get(i).copied().unwrap_or(0).clamp(-12, 12)
    }
    pub fn octave_at(&self, i: usize) -> i8 {
        self.octave.get(i).copied().unwrap_or(0).clamp(-10, 10)
    }
    pub fn repeat_at(&self, i: usize) -> u8 {
        self.repeat.get(i).copied().unwrap_or(1).clamp(1, 16)
    }
    pub fn gate_at(&self, i: usize) -> u8 {
        self.gate.get(i).copied().unwrap_or(GATE).clamp(1, 100)
    }
    pub fn tie_at(&self, i: usize) -> bool {
        self.tie.get(i).copied().unwrap_or(false)
    }
    pub fn nudge_at(&self, i: usize) -> i8 {
        self.nudge.get(i).copied().unwrap_or(0).clamp(-50, 50)
    }
    /// A step's own rate (ticks), if it has one.
    pub fn rate_at(&self, i: usize) -> Option<u64> {
        self.rates.get(i).copied().filter(|&r| r > 0)
    }
    pub fn skip_at(&self, i: usize) -> bool {
        self.skip.get(i).copied().unwrap_or(false)
    }
    pub fn value_at(&self, i: usize) -> f32 {
        self.value.get(i).copied().unwrap_or(VALUE).clamp(0.0, 1.0)
    }
    pub fn condition_at(&self, i: usize) -> Condition {
        self.condition.get(i).copied().unwrap_or_default()
    }
    /// Recurrence: (which pass, of how many), from 1.
    pub fn every_at(&self, i: usize) -> (u8, u8) {
        let [a, b] = self.every.get(i).copied().unwrap_or([1, 1]);
        let of = b.clamp(1, 16);
        (a.clamp(1, of), of)
    }
    pub fn ramp_at(&self, i: usize) -> i8 {
        self.ramp.get(i).copied().unwrap_or(0).clamp(-100, 100)
    }
    pub fn curve_at(&self, i: usize) -> i8 {
        self.curve.get(i).copied().unwrap_or(0).clamp(-100, 100)
    }
    pub fn spread_at(&self, i: usize) -> i8 {
        self.spread.get(i).copied().unwrap_or(0).clamp(-100, 100)
    }

    /// The note a step plays: the row's, moved by the step's note and octave.
    pub fn pitch_at(&self, i: usize) -> u8 {
        (self.pitch as i32 + self.transpose_at(i) as i32 + 12 * self.octave_at(i) as i32)
            .clamp(0, 127) as u8
    }

    /// Its notes move step by step: a melodic row, by its setting or its steps' notes.
    pub fn is_melodic(&self) -> bool {
        self.melodic
            || self.transpose.iter().any(|&t| t != 0)
            || self.octave.iter().any(|&o| o != 0)
    }

    /// A step's setting in a lane, as a number: velocity (1 to 127) or an automation row's value
    /// (0 to 1); gate (%); tie and skip (0 or 1); note (semitones); octave (the octave it plays
    /// in, -2 to 8); repeat (times); chance (%); offset (%); rate (an index into `RATES`, or -1
    /// for the row's own).
    pub fn get(&self, lane: Lane, i: usize) -> f32 {
        match lane {
            Lane::Velocity if self.is_automation() => self.value_at(i),
            Lane::Velocity => self.velocity_at(i) as f32,
            Lane::Gate => self.gate_at(i) as f32,
            Lane::Tie => self.tie_at(i) as u8 as f32,
            Lane::Note => self.transpose_at(i) as f32,
            Lane::Octave => {
                let base = self.pitch as i32 + self.transpose_at(i) as i32;
                (base.div_euclid(12) - 2 + self.octave_at(i) as i32) as f32
            }
            Lane::Repeat => self.repeat_at(i) as f32,
            Lane::Chance => self.chance_at(i) as f32,
            Lane::Offset => self.nudge_at(i) as f32,
            Lane::Rate => self
                .rate_at(i)
                .and_then(|r| RATES.iter().position(|x| x.1 == r))
                .map_or(-1.0, |k| k as f32),
            Lane::Skip => self.skip_at(i) as u8 as f32,
        }
    }

    /// The lowest and highest a lane goes on this row.
    pub fn range(&self, lane: Lane) -> (f32, f32) {
        match lane {
            Lane::Velocity if self.is_automation() => (0.0, 1.0),
            Lane::Velocity => (1.0, 127.0),
            Lane::Gate => (1.0, 100.0),
            Lane::Tie | Lane::Skip => (0.0, 1.0),
            Lane::Note => (-12.0, 12.0),
            Lane::Octave => (-2.0, 8.0),
            Lane::Repeat => (1.0, 16.0),
            Lane::Chance => (0.0, 100.0),
            Lane::Offset => (-50.0, 50.0),
            Lane::Rate => (-1.0, RATES.len() as f32 - 1.0),
        }
    }

    /// A lane's setting as usual (what Option-click puts back).
    pub fn default_of(&self, lane: Lane, i: usize) -> f32 {
        match lane {
            Lane::Velocity if self.is_automation() => VALUE,
            Lane::Velocity => VELOCITY as f32,
            Lane::Gate => GATE as f32,
            Lane::Tie | Lane::Skip | Lane::Note | Lane::Offset => 0.0,
            Lane::Octave => {
                let base = self.pitch as i32 + self.transpose_at(i) as i32;
                (base.div_euclid(12) - 2) as f32
            }
            Lane::Repeat => 1.0,
            Lane::Chance => CHANCE as f32,
            Lane::Rate => -1.0,
        }
    }

    /// Sets a step's setting in a lane (see `get`), kept in its range.
    pub fn set(&mut self, lane: Lane, i: usize, v: f32) {
        let n = self.cells.len();
        if i >= n || !v.is_finite() {
            return;
        }
        let (lo, hi) = self.range(lane);
        let v = v.clamp(lo, hi);
        match lane {
            Lane::Velocity if self.is_automation() => {
                self.value.resize(n, VALUE);
                self.value[i] = v;
            }
            Lane::Velocity => {
                let v = v.round() as u8;
                if self.cells[i] > 0 {
                    self.cells[i] = v;
                } else {
                    self.held.resize(n, 0);
                    self.held[i] = v;
                }
            }
            Lane::Gate => {
                self.gate.resize(n, GATE);
                self.gate[i] = v.round() as u8;
            }
            Lane::Tie => {
                self.tie.resize(n, false);
                self.tie[i] = v >= 0.5;
            }
            Lane::Note => {
                self.transpose.resize(n, 0);
                self.transpose[i] = v.round() as i8;
            }
            Lane::Octave => {
                let base = (self.pitch as i32 + self.transpose_at(i) as i32).div_euclid(12) - 2;
                // As far as the notes go: C-2 to G8.
                let pitch = self.pitch as i32 + self.transpose_at(i) as i32;
                let by = (v.round() as i32 - base).clamp(-pitch.div_euclid(12), (127 - pitch) / 12);
                self.octave.resize(n, 0);
                self.octave[i] = by as i8;
            }
            Lane::Repeat => {
                self.repeat.resize(n, 1);
                self.repeat[i] = v.round() as u8;
            }
            Lane::Chance => {
                self.chance.resize(n, CHANCE);
                self.chance[i] = v.round() as u8;
            }
            Lane::Offset => {
                self.nudge.resize(n, 0);
                self.nudge[i] = v.round() as i8;
            }
            Lane::Rate => {
                self.rates.resize(n, 0);
                self.rates[i] = match v.round() as i32 {
                    k if k < 0 => 0,
                    k => RATES[(k as usize).min(RATES.len() - 1)].1,
                };
            }
            Lane::Skip => {
                self.skip.resize(n, false);
                self.skip[i] = v >= 0.5;
            }
        }
    }

    /// Every lane as long as its steps, filled as usual, to move steps about with their settings.
    pub fn fill_lanes(&mut self) {
        let n = self.cells.len();
        self.held.resize(n, 0);
        self.chance.resize(n, CHANCE);
        self.transpose.resize(n, 0);
        self.octave.resize(n, 0);
        self.repeat.resize(n, 1);
        self.gate.resize(n, GATE);
        self.tie.resize(n, false);
        self.nudge.resize(n, 0);
        self.rates.resize(n, 0);
        self.skip.resize(n, false);
        self.condition.resize(n, Condition::Always);
        self.every.resize(n, [1, 1]);
        self.ramp.resize(n, 0);
        self.curve.resize(n, 0);
        self.spread.resize(n, 0);
        if self.is_automation() {
            self.value.resize(n, VALUE);
        }
    }

    /// Lanes cut to its steps, and those as usual throughout taken away, so the song keeps only
    /// what's set.
    pub fn tidy_lanes(&mut self) {
        fn tidy<T: PartialEq + Copy>(v: &mut Vec<T>, n: usize, usual: T) {
            v.truncate(n);
            if v.iter().all(|&x| x == usual) {
                v.clear();
            }
        }
        let n = self.cells.len();
        // The velocity an off step comes back with matters only while it's off.
        for (h, &c) in self.held.iter_mut().zip(&self.cells) {
            if c > 0 || *h == VELOCITY {
                *h = 0;
            }
        }
        tidy(&mut self.held, n, 0);
        tidy(&mut self.chance, n, CHANCE);
        tidy(&mut self.transpose, n, 0);
        tidy(&mut self.octave, n, 0);
        tidy(&mut self.repeat, n, 1);
        tidy(&mut self.gate, n, GATE);
        tidy(&mut self.tie, n, false);
        tidy(&mut self.nudge, n, 0);
        tidy(&mut self.rates, n, 0);
        tidy(&mut self.skip, n, false);
        tidy(&mut self.condition, n, Condition::Always);
        tidy(&mut self.every, n, [1, 1]);
        tidy(&mut self.ramp, n, 0);
        tidy(&mut self.curve, n, 0);
        tidy(&mut self.spread, n, 0);
        // An automation row keeps its values: they're what it sets.
        self.value.truncate(n);
    }

    /// Its loop: the first step it plays and the one after its last, within `steps`.
    pub fn loop_range(&self, steps: usize) -> (usize, usize) {
        let steps = steps.max(1);
        let end = self
            .loop_end
            .unwrap_or_else(|| self.cells.len().min(steps))
            .clamp(1, steps);
        (self.loop_start.min(end - 1), end)
    }
}

/// A step pattern, as a pattern region holds it: rows of steps, looped over the region.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Pattern {
    /// Ticks per step: 240 is a sixteenth (`RATES` has the rest).
    #[serde(default = "sixteenth")]
    pub step: u64,
    pub rows: Vec<PatternRow>,
    /// How late every second sixteenth (or eighth) comes: 50 is straight, 66 a triplet feel, up
    /// to 75.
    #[serde(default = "straight", skip_serializing_if = "is_straight")]
    pub swing: f32,
    /// Swing on eighths rather than sixteenths.
    #[serde(default, skip_serializing_if = "Not::not")]
    pub swing_eighths: bool,
    /// How many steps it has (1 to 64); none as long as its longest row.
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub length: usize,
    /// How its rows run through their steps, unless they have their own way.
    #[serde(default, skip_serializing_if = "PlayMode::is_forward")]
    pub mode: PlayMode,
    /// A step held its whole length runs a tick into the next, so a synth glides on.
    #[serde(default, skip_serializing_if = "Not::not")]
    pub legato: bool,
    /// Its key (0 C to 11 B), and the scale its notes keep to (none: every note).
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub key: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<Scale>,
    /// Which steps' settings it was made with: 0 before every step had a gate of its own
    /// (when it held 75% of a step), so it plays as it did.
    #[serde(default = "legacy")]
    pub version: u8,
}

impl Default for Pattern {
    fn default() -> Self {
        Pattern {
            step: sixteenth(),
            rows: Vec::new(),
            swing: straight(),
            swing_eighths: false,
            length: 0,
            mode: PlayMode::Forward,
            legato: false,
            key: 0,
            scale: None,
            version: VERSION,
        }
    }
}

impl Pattern {
    /// A pattern of these rows, at `step` ticks a step.
    pub fn new(step: u64, rows: Vec<PatternRow>, swing: f32) -> Self {
        let mut p = Pattern {
            step: step.max(1),
            rows,
            swing,
            ..Default::default()
        };
        p.tidy();
        p
    }

    /// How many steps it has.
    pub fn steps(&self) -> usize {
        match self.length {
            0 => self.rows.iter().map(|r| r.cells.len()).max().unwrap_or(16),
            n => n,
        }
        .max(1)
    }

    /// A row's step rate, in ticks.
    pub fn row_rate(&self, r: usize) -> u64 {
        self.rows
            .get(r)
            .and_then(|row| row.rate)
            .unwrap_or(self.step)
            .max(1)
    }

    pub fn row_mode(&self, r: usize) -> PlayMode {
        self.rows
            .get(r)
            .and_then(|row| row.mode)
            .unwrap_or(self.mode)
    }

    /// Every row as long as the pattern (a shorter one looping on its own length, as it did),
    /// its lanes tidied, its loop within it.
    pub fn tidy(&mut self) {
        let n = self.steps();
        self.length = n;
        self.swing = if self.swing.is_finite() {
            self.swing.clamp(50.0, 75.0)
        } else {
            50.0
        };
        self.key %= 12;
        for row in &mut self.rows {
            if row.cells.len() < n && row.loop_end.is_none() && !row.cells.is_empty() {
                row.loop_end = Some(row.cells.len());
            }
            row.cells.resize(n, 0);
            row.loop_end = row.loop_end.map(|e| e.clamp(1, n)).filter(|&e| e < n);
            row.loop_start = row.loop_start.min(row.loop_end.unwrap_or(n) - 1);
            row.tidy_lanes();
            let mut seen = Vec::new();
            row.subrows.retain(|l| {
                let fresh = !seen.contains(l) && l.applies(row.target.is_some());
                seen.push(*l);
                fresh
            });
        }
    }

    /// Brings a pattern made before steps had every setting up to date, sounding as it did: its
    /// steps held 75% of their length.
    pub fn upgrade(&mut self) {
        if self.version < 1 {
            for row in &mut self.rows {
                let n = row.cells.len();
                row.gate.resize(n, 75);
                for g in &mut row.gate {
                    *g = (*g).clamp(1, 100);
                }
                for r in &mut row.repeat {
                    *r = (*r).clamp(1, 16);
                }
            }
            self.version = VERSION;
        }
        self.tidy();
    }
}

/// A MIDI region's notes as a pattern's rows: a row a pitch (lowest last, as drums read: kick at
/// the bottom), each note on the step nearest its start, `steps` long, at its velocity; how far
/// off the step it starts kept as its offset, and how long it holds as ties and a gate.
pub fn to_pattern(notes: &[Note], step: u64, steps: usize) -> Vec<PatternRow> {
    let step = step.max(1);
    let steps = steps.max(1);
    let mut pitches: Vec<u8> = notes.iter().map(|n| n.pitch).collect();
    pitches.sort_unstable_by(|a, b| b.cmp(a));
    pitches.dedup();
    pitches
        .into_iter()
        .map(|pitch| {
            let mut row = PatternRow::note(pitch, steps);
            row.fill_lanes();
            for n in notes.iter().filter(|n| n.pitch == pitch && n.start >= 0) {
                let at = n.start as f64 / step as f64;
                let k = at.round() as usize;
                if k >= steps {
                    continue;
                }
                row.cells[k] = row.cells[k].max(n.velocity.max(1));
                row.nudge[k] = (((at - k as f64) * 100.0).round() as i8).clamp(-50, 50);
                // Held over steps: tied on to the step it ends in, which holds what's left.
                let held = n.length as f64 / step as f64;
                let whole = (held.ceil() as usize).max(1);
                let last = (k + whole - 1).min(steps - 1);
                for t in k..last {
                    row.tie[t] = true;
                }
                let rest = held - (last - k) as f64;
                row.gate[last] = ((rest * 100.0).round() as u8).clamp(1, 100);
            }
            row.tidy_lanes();
            row
        })
        .collect()
}

#[cfg(test)]
mod tests;

impl Project {
    /// Every pattern (on the timeline and in Live Loops cells) brought up to date as it opens.
    pub fn upgrade_patterns(&mut self) {
        for t in &mut self.tracks {
            let cells = t.cells.iter_mut().map(|c| &mut c.region);
            for r in t.regions.iter_mut().chain(cells) {
                if let Content::Pattern(p) = &mut r.content {
                    p.upgrade();
                }
            }
        }
    }
}
