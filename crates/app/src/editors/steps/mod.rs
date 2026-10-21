//! The step sequencer: a pattern region's sounds as pads (tap one to
//! hear it and pick it, right-click for its menu, an empty one adds a sound) beside every sound's
//! steps, each in its own colour (a kick warm, hats cool). Click a step on or off and drag across
//! to paint more (or rub them out); option-click for an accent (a white dot). How hard a step hits
//! shows in how bright it is, and the picked sound's velocities open under its steps, drawn with
//! the mouse. A sound with its own length (a polymeter) says so, its repeats drawn faint. Under the
//! grid, what can be done to the picked sound: fill every 1, 2, 4 or 8, shift, reverse, randomize,
//! Euclid's hits spread evenly, its length, clear, remove. The pattern's length (8 to 64 steps),
//! rate and swing are in the editor's bar, with "To MIDI" to edit it in the piano roll. A MIDI
//! region (the drums an agent wrote) becomes a pattern in a click; on the drum kit a row is a
//! drum, on any other instrument a note.

mod body;
mod controls;
mod edits;
mod empty;
mod patterns;

pub use edits::*;

use gpui::{
    Bounds, Context, Div, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, div,
    prelude::*, px, rgb,
};
use pina_engine::{
    dsp::drums,
    dsp::modulate::{ModKind, ModTarget, SoundParam},
    music::Key,
    song::project::{At, Content, Instrument, PPQ, PatternRow, Region, pattern_notes, to_pattern},
};

use crate::{
    ui::menu::{Menu, MenuAction},
    ui::theme::*,
    workspace::{Bottom, Drag, Workspace},
};

const ON: u8 = 100;
const ACCENTED: u8 = 127;

/// The rows a new drum pattern starts with.
const STARTING_ROWS: [u8; 5] = [36, 38, 42, 46, 39];

/// The notes a pattern on a melodic instrument can have rows for: two octaves of the song's key
/// (C major without one) from the C below middle C, highest first.
fn scale_notes(key: Option<Key>) -> Vec<u8> {
    let key = key.unwrap_or(Key {
        root: 0,
        minor: false,
    });
    (48..=72).rev().filter(|&p| key.contains(p)).collect()
}

/// A row's name: a drum on the kit, a sampler's sound on its note, else a note, spelled by the
/// key.
pub(crate) fn row_name(instrument: Option<&Instrument>, key: Option<Key>, pitch: u8) -> String {
    match instrument {
        Some(Instrument::Drums) => drums::name(pitch),
        Some(Instrument::Sampler(s)) => match s.zones.iter().find(|z| z.covers(pitch)) {
            Some(z) if z.high.is_none() && !z.name.is_empty() => z.name.clone(),
            Some(z) if z.high.is_none() => z
                .file
                .file_stem()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            _ => crate::editors::pianoroll::name_in(key, pitch),
        },
        _ => crate::editors::pianoroll::name_in(key, pitch),
    }
}

/// The sounds a kit has to add rows for: the drum kit's, or a sampler's.
/// A row's colour, by what it sounds like (a kick warm, hats cool), else the palette in turn.
fn row_color(name: &str, index: usize) -> u32 {
    let n = name.to_lowercase();
    let has = |w: &[&str]| w.iter().any(|w| n.contains(w));
    match () {
        _ if has(&["kick", "bd", "808"]) => CORAL,
        _ if has(&["snare", "sd", "rim"]) => AMBER,
        _ if has(&["clap", "snap"]) => PINK,
        _ if has(&["open"]) => SKY,
        _ if has(&["hat", "hh"]) => TEAL,
        _ if has(&["tom"]) => VIOLET,
        _ if has(&["crash", "ride", "cymbal", "splash", "china"]) => SLATE,
        _ if has(&[
            "perc", "shaker", "conga", "bongo", "tamb", "cow", "clave", "wood",
        ]) =>
        {
            MINT
        }
        _ => [SKY, TEAL, MINT, AMBER, CORAL, PINK, VIOLET, SLATE][index % 8],
    }
}

fn kit_sounds(instrument: Option<&Instrument>) -> Option<Vec<(u8, String)>> {
    match instrument {
        Some(Instrument::Drums) => Some(
            drums::SOUNDS
                .iter()
                .map(|&(pitch, name)| (pitch, name.to_string()))
                .collect(),
        ),
        Some(i @ Instrument::Sampler(s)) if i.is_kit() => Some(
            s.zones
                .iter()
                .map(|z| (z.note, row_name(Some(i), None, z.note)))
                .collect(),
        ),
        _ => None,
    }
}

/// The swings the step sequencer offers, as MPC's: straight to a heavy shuffle.
const SWINGS: [f32; 6] = [50.0, 54.0, 58.0, 62.0, 66.0, 71.0];

/// What the lane under the picked sound shows and draws, its edit
/// mode: each step's velocity, chance, pitch, ratchet, gate or nudge.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum LaneMode {
    #[default]
    Velocity,
    Chance,
    Pitch,
    Repeat,
    Gate,
    Nudge,
}

impl LaneMode {
    pub const ALL: [LaneMode; 6] = [
        LaneMode::Velocity,
        LaneMode::Chance,
        LaneMode::Pitch,
        LaneMode::Repeat,
        LaneMode::Gate,
        LaneMode::Nudge,
    ];

    pub fn name(self) -> &'static str {
        match self {
            LaneMode::Velocity => "Velocity",
            LaneMode::Chance => "Chance",
            LaneMode::Pitch => "Pitch",
            LaneMode::Repeat => "Repeat",
            LaneMode::Gate => "Gate",
            LaneMode::Nudge => "Nudge",
        }
    }

    /// What a modulator dropped on the lane moves.
    pub fn sound_param(self) -> Option<SoundParam> {
        match self {
            LaneMode::Velocity => Some(SoundParam::Velocity),
            LaneMode::Chance => Some(SoundParam::Chance),
            LaneMode::Pitch => Some(SoundParam::Pitch),
            _ => None,
        }
    }

    /// A step's value in this lane, as a height (0 to 1, or -1 to 1 from the middle), and as
    /// text when it isn't as usual.
    fn shown(self, row: &PatternRow, i: usize) -> (f32, bool, Option<String>) {
        match self {
            LaneMode::Velocity => {
                let v = row.cells.get(i).copied().unwrap_or(0);
                (
                    v as f32 / 127.0,
                    false,
                    (v >= ACCENTED).then(|| "acc".into()),
                )
            }
            LaneMode::Chance => {
                let c = row.chance_at(i);
                (c as f32 / 100.0, false, (c < 100).then(|| format!("{c}%")))
            }
            LaneMode::Pitch => {
                let t = row.transpose_at(i);
                (t as f32 / 12.0, true, (t != 0).then(|| format!("{t:+}")))
            }
            LaneMode::Repeat => {
                let n = row.repeat_at(i);
                (n as f32 / 4.0, false, (n > 1).then(|| format!("×{n}")))
            }
            LaneMode::Gate => {
                let g = row.gate_at(i);
                (
                    g as f32 / 200.0,
                    false,
                    (g != pina_engine::song::project::GATE).then(|| format!("{g}%")),
                )
            }
            LaneMode::Nudge => {
                let n = row.nudge_at(i);
                (n as f32 / 50.0, true, (n != 0).then(|| format!("{n:+}")))
            }
        }
    }
}
