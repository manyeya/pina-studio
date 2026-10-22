//! The Sampler editor, in the pane under the tracks: the selected track's built-in sampler. Its
//! zones down the left (drop samples there, or anywhere on it); the picked zone's sample as a
//! waveform, its start, end and loop dragged in place and its slices marked; the keys, each zone
//! over the notes it plays (click a key to hear it); and how it plays: the zone's notes, tuning,
//! level, pan and velocities; the instrument's pitch and glide, amp envelope, filter (with its
//! envelope) and LFO, as knobs. A loop can be sliced across the keys at its hits or into equal
//! parts. Every change is an edit (undone like any) that the playing sampler follows at once.

mod body;
mod controls;
mod edits;
mod empty;
mod fields;
mod keys;

pub use fields::*;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui::{
    Bounds, Context, Div, FontWeight, Hsla, MouseButton, MouseDownEvent, PathPromptOptions, Point,
    Window, canvas, div, fill, point, prelude::*, px, rgb, size,
};
use pina_engine::{
    audio::engine::read_audio,
    audio::peaks::Peaks,
    dsp::sampler::{cuts, onsets, slices},
    song::commands::{self, Command},
    song::project::{FilterMode, Instrument, LoopMode, Sampler, Zone},
};

use crate::{
    editors::pianoroll::name_in,
    ui::menu::{Menu, MenuAction},
    ui::theme::*,
    workspace::{Drag, Target, Workspace, pt},
};

/// A sample's waveform and hits, worked out once.
pub struct Wave {
    peaks: Peaks,
    len: usize,
    seconds: f32,
    onsets: Vec<(f32, f32)>,
}

#[derive(Default)]
pub struct SamplerUi {
    /// The zone picked, by index.
    pub zone: usize,
    /// Waveforms by file (relative to the song), None while one's being read.
    waves: HashMap<PathBuf, Option<Arc<Wave>>>,
}

/// A sampler setting a knob turns: the instrument's, or a zone's (by index).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Field {
    Attack,
    Decay,
    Sustain,
    Release,
    Cutoff,
    Resonance,
    FilterEnv,
    FilterAttack,
    FilterDecay,
    FilterSustain,
    FilterRelease,
    KeyTrack,
    Velocity,
    Glide,
    LfoRate,
    Vibrato,
    LfoFilter,
    Tremolo,
    Transpose,
    Volume,
    Note(usize),
    High(usize),
    Root(usize),
    Tune(usize),
    Gain(usize),
    Pan(usize),
    VelLow(usize),
    VelHigh(usize),
}

/// How a zone's sample is cut into slices across the keys.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Slicing {
    /// At its strongest hits, at most this many.
    Hits(usize),
    /// Into this many equal parts.
    Equal(usize),
}

/// Which mark on the waveform is dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mark {
    Start,
    End,
    LoopStart,
    LoopEnd,
}

/// A zone's colour, by its place.
fn zone_color(i: usize) -> u32 {
    [CORAL, AMBER, MINT, SKY, VIOLET, PINK, TEAL, 0xfb923c][i % 8]
}

fn zone_name(z: &Zone) -> String {
    match z.name.is_empty() {
        true => z
            .file
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
        false => z.name.clone(),
    }
}

#[cfg(test)]
mod tests;
