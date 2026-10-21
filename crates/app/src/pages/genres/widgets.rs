//! Headings, tiles, labels and fields the pages draw with.

use super::*;

/// A section's heading: its title, what it's for, and room for a button on the right.
pub(crate) fn heading(title: &'static str, about: &'static str) -> Div {
    div().flex().items_end().gap(px(12.)).child(
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .text_size(px(17.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_3))
                    .child(about),
            ),
    )
}

/// A card holding one setting.
pub(crate) fn tile(title: &'static str, about: &'static str) -> Div {
    div()
        .min_w(px(200.))
        .p(px(14.))
        .rounded(px(14.))
        .bg(rgb(CARD))
        .border_1()
        .border_color(rgb(LINE))
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(10.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT_3))
                        .child(title.to_uppercase()),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_4))
                        .child(about),
                ),
        )
}

pub(crate) fn label(text: &'static str) -> Div {
    div()
        .text_size(px(10.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT_3))
        .child(text.to_uppercase())
}

/// A text field's box.
pub(crate) fn field_box(f: Option<Entity<TextInput>>, sel: &'static str) -> Div {
    div()
        .min_h(px(32.))
        .px(px(10.))
        .flex()
        .items_center()
        .rounded(px(8.))
        .bg(rgb(FIELD))
        .border_1()
        .border_color(rgb(LINE_STRONG))
        .text_size(px(12.5))
        .debug_selector(move || sel.into())
        .when_some(f, |d, f| d.child(f))
}

pub(crate) fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}
