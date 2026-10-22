//! The mixer, in place of the arrangement: a strip for every track, aux and the master, labelled
//! down the left; and on the right the selected channel's chain: its instrument,
//! its EQ to drag about, its compressor and saturator, its plugins, its sends.

mod cards;
mod panel;
mod view;

pub(crate) use panel::*;

use gpui::{
    Bounds, Context, Div, FontWeight, MouseButton, MouseDownEvent, canvas, div, fill, point,
    prelude::*, px, rgb, size,
};
use pina_engine::{
    audio::player::Which,
    dsp::sat_curve,
    song::project::{Comp, Eq, Instrument, Saturator},
};

use crate::{
    mix::strip::{self, Builtin, Channel, Layout, MIXER_INSERTS, MIXER_SENDS},
    ui::menu::{Menu, MenuAction},
    ui::theme::*,
    workspace::{Channels, CompField, Drag, SatField, Slider, Target, Workspace, pt},
};
