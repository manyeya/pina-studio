//! The panel of the bands picked, floating under them at the bottom of the display: the band
//! before and after and the shapes along the top, with the slope; frequency, gain and width as
//! dials, gain the big one in the middle (its dynamic range a ring round it, the move now a dot
//! on that); the part of the sound it works on, gain-Q, solo, dynamic, off and delete along the
//! bottom; and a dynamic band's own settings in a tray under those.

mod band;
mod dials;
mod dynamics;

pub(crate) use dials::*;

use gpui::{
    Context, Div, FontWeight, MouseButton, MouseDownEvent, PathBuilder, Pixels, Point,
    ScrollWheelEvent, canvas, div, point, prelude::*, px, rgb,
};
use pina_engine::song::project::{Eq, Placement, Shape};

use super::{EqKnob, EqOp, Geo, display::Drawn, hz_text, note_text, q_text, slope_text};
use crate::{
    ui::menu::MenuAction,
    ui::theme::*,
    workspace::{Name, Workspace},
};

const W: f32 = 700.0;
/// The panel's height, closed and with the dynamics tray open: whether it covers its band.
const H: f32 = 178.0;
const H_OPEN: f32 = 272.0;
const BG_PANEL: u32 = 0x17181e;
/// The dials: frequency and width either side, gain in the middle.
const DIAL: f32 = 52.0;
const HERO: f32 = 72.0;
