//! Modulation: a track's modulators as cards in the Modulation pane (an LFO, steps,
//! an envelope its notes set off, a follower of another track's level, random, a macro), each
//! moving as it plays. Drag a card's ↗ onto any knob or fader of its track (the header's fader
//! and pan, Smart Controls, the EQ, compressor and saturator, the Sampler's knobs, sends) to map
//! it there; that knob then wears a ring in the modulator's colour, its range, and a dot riding
//! where it is now. Click a card to pick it: dragging a knob it moves then sets how far, not the
//! knob. Each card lists where it goes, each route's depth dragged sideways. The engine side is
//! `pina_engine::dsp::modulate`.

mod card;
mod edits;
mod fields;
mod learn;
mod live;
mod pane;
mod rings;
mod targets;
mod visual;

pub use fields::*;
pub use learn::*;
pub use rings::*;

use gpui::{
    Bounds, Context, Div, FontWeight, Hsla, MouseButton, MouseDownEvent, PathBuilder, Pixels,
    Point, Window, canvas, div, fill, point, prelude::*, px, rgb, size,
};
use pina_engine::{
    dsp::modulate::{
        self, LfoShape, MAX_MODS, ModKind, ModParam, ModRate, ModRoute, ModTarget, Modulator,
        SamplerParam,
    },
    song::project::{Instrument, Param, Sampler, Which},
};

use crate::{
    editors::sampler::{Field, field_pos},
    mix::strip::{self, Channel},
    ui::menu::{Menu, MenuAction},
    ui::theme::*,
    workspace::{CompField, Drag, SatField, Target, Workspace, pt},
};

/// Each modulator's colour, by its place on its track.
pub const MOD_COLORS: [u32; MAX_MODS] = [VIOLET, AMBER, TEAL, PINK, SKY, MINT, CORAL, SLATE];

/// How far a parameter must move to count as turned, of its 0 to 1.
const TURNED: f64 = 0.004;

/// How many of a modulator's values a card keeps to draw its trace.
const TRACE: usize = 96;

/// Rates to choose from: in time with the song, and free.
const SYNCED: [f32; 13] = [
    16.0,
    8.0,
    4.0,
    2.0,
    1.0,
    0.5,
    0.25,
    0.125,
    1.5,
    0.75,
    2.0 / 3.0,
    1.0 / 3.0,
    1.0 / 6.0,
];
const FREE: [f32; 7] = [0.1, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0];
