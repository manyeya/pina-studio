//! A channel strip, as in the inspector and mixer: the instrument or input, an EQ curve,
//! effect slots (the built-in EQ and compressor, then plugins), sends, where it plays, pan,
//! level, fader and meters, mute and solo, name; for a track, an aux, or the master.
//!
//! Clicking a plugin opens its editor; clicking an empty slot, or right-clicking any, picks one.
//! Sends drag up and down for level; right-click one to remove it.

mod channel;
mod learn;
mod menus;
mod order;
pub use order::EffectRows;
mod scales;
mod slots;
mod space;
mod view;

pub use learn::bus_comp;
pub use pina_engine::song::project::{KnobScale, fresh_tool, knobs};
pub use scales::*;
pub use slots::*;
pub use space::*;
pub use view::*;

use gpui::{
    Bounds, Context, Div, FontWeight, MouseButton, MouseDownEvent, Pixels, Point, canvas, div,
    fill, point, prelude::*, px, rgb, size,
};
use pina_engine::{
    audio::player::{MASTER_ID, Which},
    dsp::{LOW_CUT_OFF, eq_response},
    song::project::{
        AsUnit, Comp, CompStyle, DeEsser, Delay, Eq, FilterFx, Fx, Gate, Input, Instrument,
        Limiter, Motion, Multiband, OWN_COMP, OWN_SAT, Param, PitchShift, PluginRef, Reverb,
        Saturator, Shaper, Stereo, TapeStop, Tune, Unit, nearest_step,
    },
};

use crate::{
    ui::menu::{Menu, MenuAction},
    ui::theme::*,
    workspace::{CompField, Drag, LimitField, Name, SatField, SatKnob, Workspace},
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
    Limit,
    /// One that only ever stacks (a reverb, a filter...), by what the chain calls it.
    Tool(Fx),
}

impl Builtin {
    /// Which it is, as a chain names a channel's own.
    pub fn of(fx: Fx) -> Option<Builtin> {
        Some(match fx {
            Fx::Eq => Builtin::Eq,
            Fx::Comp => Builtin::Comp,
            Fx::Sat => Builtin::Sat,
            Fx::Limit => Builtin::Limit,
            Fx::Plugin | Fx::More => return None,
            tool => Builtin::Tool(tool),
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Builtin::Eq => "Channel EQ",
            Builtin::Comp => "Compressor",
            Builtin::Sat => "Saturator",
            Builtin::Limit => "Limiter",
            Builtin::Tool(Fx::Reverb) => "Reverb",
            Builtin::Tool(Fx::Delay) => "Delay",
            Builtin::Tool(Fx::Filter) => "Filter",
            Builtin::Tool(Fx::Motion) => "Motion",
            Builtin::Tool(Fx::Shaper) => "Transient Shaper",
            Builtin::Tool(Fx::Gate) => "Gate",
            Builtin::Tool(Fx::DeEsser) => "De-esser",
            Builtin::Tool(Fx::Multiband) => "Multiband",
            Builtin::Tool(Fx::Pitch) => "Pitch Shifter",
            Builtin::Tool(Fx::TapeStop) => "Tape Stop",
            Builtin::Tool(Fx::Tune) => "Pitch Correction",
            Builtin::Tool(_) => "Stereo",
        }
    }

    /// Whether a channel has one of its own (the rest only stack).
    pub fn own(self) -> bool {
        !matches!(self, Builtin::Tool(_))
    }

    /// Where it is in a chain.
    pub fn fx(self) -> Fx {
        match self {
            Builtin::Eq => Fx::Eq,
            Builtin::Comp => Fx::Comp,
            Builtin::Sat => Fx::Sat,
            Builtin::Limit => Fx::Limit,
            Builtin::Tool(k) => k,
        }
    }
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
pub const MIXER_INSERTS: usize = 6;
pub const MIXER_SENDS: usize = 2;
/// How wide a mixer strip is: room for an effect's name.
pub const MIXER_STRIP_W: f32 = 124.0;

#[cfg(test)]
mod tests;
