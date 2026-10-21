//! The tracks area: tools and track headers on the left; markers, the ruler (with the cycle) and
//! the lanes with their regions on the right. Regions move by dragging their body, and trim by
//! dragging their edges; the tools add, cut and erase them.

mod headers;
mod lanes;
mod regions;
mod ruler;
mod stacks;
mod toolbar;
mod warp;

pub use headers::*;
pub use regions::*;

use std::sync::Arc;

use gpui::{
    AnyElement, App, Bounds, BoxShadow, Context, Div, FontWeight, Hsla, MouseButton,
    MouseDownEvent, PathBuilder, Pixels, Point, SharedString, Window, canvas, div, fill,
    linear_color_stop, linear_gradient, point, prelude::*, px, rgb, size,
};
use pina_engine::{
    audio::peaks::Peaks,
    audio::stretch::Map,
    song::project::{At, Content, Instrument, PPQ, PanMode, Param, Region, comp_segments},
};

use crate::{
    mix::strip::{self, Channel},
    ui::menu::{MARKER_NAMES, Menu, MenuAction},
    ui::theme::*,
    workspace::{Bottom, Drag, Edge, Name, Snap, Tool, Workspace},
};

impl Workspace {
    pub fn tracks_area(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(TOP_H))
                    .flex_none()
                    .flex()
                    .child(self.toolbar(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .child(self.markers(cx))
                            .child(self.ruler(cx)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .child(self.headers(cx))
                    .child(self.lanes(cx)),
            )
    }
}

const PLAYHEAD: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 11 12"><path fill="#000" d="M1.5 0h8a1.5 1.5 0 0 1 1.5 1.5v4.6a1.5 1.5 0 0 1-.5 1.1L5.5 12 .5 7.2A1.5 1.5 0 0 1 0 6.1V1.5A1.5 1.5 0 0 1 1.5 0z"/></svg>"##;
const CURSOR: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path fill="#000" d="M1.5 1.2l12.8 7.4-5.6 1.5-2.6 5.4z"/></svg>"##;

#[cfg(test)]
mod tests;
