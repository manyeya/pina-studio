//! A genre's song in miniature.

use super::*;

/// A genre's song in miniature: a bar a section, as tall as it's loud.
pub(crate) fn ribbon(sections: &[Section], color: u32, w: f32, h: f32) -> Div {
    div()
        .w(px(w))
        .h(px(h))
        .flex_none()
        .p(px(3.))
        .flex()
        .items_end()
        .gap(px(1.))
        .rounded(px(7.))
        .bg(alpha(color, 0.1))
        .children(sections.iter().map(|s| {
            div()
                .flex_grow(s.bars as f32)
                .flex_basis(px(0.))
                .h(gpui::relative(0.2 + 0.16 * s.energy as f32))
                .rounded(px(1.5))
                .bg(alpha(color, 0.35 + 0.13 * s.energy as f32))
        }))
}
