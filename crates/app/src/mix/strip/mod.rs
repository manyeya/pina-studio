//! A channel strip, as in the inspector and mixer: the instrument or input, an EQ curve,
//! effect slots (the built-in EQ and compressor, then plugins), sends, where it plays, pan,
//! level, fader and meters, mute and solo, name; for a track, an aux, or the master.
//!
//! Clicking a plugin opens its editor; clicking an empty slot, or right-clicking any, picks one.
//! Sends drag up and down for level; right-click one to remove it.

mod channel;
mod menus;
mod scales;
mod slots;
mod view;

pub use scales::*;
pub use slots::*;
pub use view::*;

use gpui::{
    Bounds, Context, Div, FontWeight, MouseButton, MouseDownEvent, Pixels, Point, canvas, div,
    fill, point, prelude::*, px, rgb, size,
};
use pina_engine::{
    audio::player::{MASTER_ID, Which},
    dsp::{LOW_CUT_OFF, eq_response},
    song::project::{Comp, Eq, Input, Instrument, Param, PluginRef, Saturator},
};

use crate::{
    ui::menu::{Menu, MenuAction},
    ui::theme::*,
    workspace::{CompField, Drag, Name, SatField, View, Workspace},
};

/// Quieter than anything audible: the fader's floor.
pub const SILENT: f32 = -144.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Channel {
    Track(usize),
    Aux(usize),
    Master,
}

/// The built-in processors every channel can have.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Builtin {
    Eq,
    Comp,
    Sat,
}

/// Where a strip is drawn: narrow in the inspector, or in a row of the mixer.
#[derive(Clone, Copy, PartialEq)]
pub enum Layout {
    /// Side by side in the inspector: at least this many effect and send rows, so strips with
    /// fewer line up with the one beside them.
    Compact { effects: usize, sends: usize },
    /// Which of the EQ, effect and send rows show, and how tall the fader stands.
    Mixer { rows: [bool; 3], fader: f32 },
}

impl Layout {
    pub fn fader_h(self) -> f32 {
        match self {
            Layout::Compact { .. } => 136.0,
            Layout::Mixer { fader, .. } => fader,
        }
    }
}

/// Mixer strips line up: this many effect and send rows each.
pub const MIXER_INSERTS: usize = 4;
pub const MIXER_SENDS: usize = 2;

#[cfg(test)]
mod tests;
