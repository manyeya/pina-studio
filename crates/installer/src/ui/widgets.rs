//! Buttons and switches the installer is made of.

use super::*;

pub fn alpha(c: u32, a: f32) -> Hsla {
    Hsla::from(rgb(c)).opacity(a)
}

pub(crate) fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

/// A button: the main one filled, the others quiet.
pub(crate) fn button(label: impl Into<SharedString>, main: bool) -> Div {
    div()
        .h(px(38.))
        .px(px(if main { 22. } else { 16. }))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(9.))
        .cursor_pointer()
        .text_size(px(13.5))
        .font_weight(FontWeight::SEMIBOLD)
        .when(main, |d| {
            d.bg(rgb(ACCENT))
                .text_color(rgb(0xffffff))
                .hover(|d| d.bg(rgb(0x6d99ff)))
        })
        .when(!main, |d| {
            d.bg(rgb(FIELD))
                .text_color(rgb(TEXT_2))
                .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
        })
        .child(label.into())
}

/// An on/off switch.
pub(crate) fn switch(on: bool) -> Div {
    div()
        .relative()
        .w(px(34.))
        .h(px(20.))
        .flex_none()
        .rounded(px(10.))
        .bg(rgb(if on { ACCENT } else { RAISED }))
        .child(
            div()
                .absolute()
                .top(px(2.))
                .left(px(if on { 16. } else { 2. }))
                .size(px(16.))
                .rounded(px(8.))
                .bg(rgb(if on { 0xffffff } else { TEXT_3 })),
        )
}
