//! Automation. Every control shows where its automation has it; moving
//! one records it (Touch, Latch, Write), moves the line up or down (Trim, or any time the song is
//! stopped), or takes over from it while held (Read). Under each track are its lanes, a
//! parameter each, to draw on: points to drag, lines to bend or step, shapes to draw in, a
//! marquee to select with, and points to copy and paste.

mod editing;
mod headers;
mod lanes;
mod menus;
mod recording;
mod values;
mod views;

pub use editing::*;
pub use menus::*;
pub use recording::*;
pub use values::*;

use gpui::{
    AnyElement, Bounds, Context, Div, MouseButton, MouseDownEvent, PathBuilder, Pixels, Point,
    canvas, div, fill, linear_color_stop, linear_gradient, point, prelude::*, px, rgb, size,
};
use pina_engine::song::project::{
    AutoClip, AutoMode, AutoPoint, AutoShape, EqField, PPQ, Param, Project, SILENT_DB, Which,
    value_at,
};

use crate::{
    mix::strip,
    ui::menu::{Menu, MenuAction},
    ui::theme::*,
    workspace::{Drag, Tool, Workspace},
};

/// Space kept free above and below a lane's line.
const PAD: f32 = 7.0;

/// How long a control let go takes to go back to its line (Touch, Trim): a sixteenth.
const RAMP: u64 = pina_engine::song::project::PPQ / 4;

/// How close (pixels) the pointer has to be to a line to take hold of it.
const NEAR: f32 = 6.0;

impl Workspace {
    // ---------------------------------------------------------------------------------------
    // Lanes

    // ---------------------------------------------------------------------------------------
    // Values

    // ---------------------------------------------------------------------------------------
    // Controls moved by hand

    // ---------------------------------------------------------------------------------------
    // Editing lanes

    // ---------------------------------------------------------------------------------------
    // Views
}

/// Ticks in a beat, for naming a grid step.
const PPQ_BEAT: u64 = pina_engine::song::project::PPQ;
