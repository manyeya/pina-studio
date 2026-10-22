//! What the sampler editor shows with no sampler, or no samples yet.

use super::*;

impl Workspace {
    /// The selected track isn't a sampler.
    pub(crate) fn sampler_none(&mut self, cx: &mut Context<Self>) -> Div {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(10.))
            .bg(rgb(LANE))
            .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child("The Sampler plays samples on the keys"))
            .child(div().text_size(px(12.)).text_color(rgb(TEXT_3)).child("A kit, one sound across the keys, a loop sliced at its hits, layers by velocity, round robins."))
            .child(button("New Sampler Track", true).debug_selector(|| "sampler-new".into()).on_mouse_down(MouseButton::Left, cx.listener(|ws, _, _, cx| ws.new_sampler_track(cx))))
    }

    /// A sampler with nothing in it yet.
    pub(crate) fn sampler_empty(&mut self, cx: &mut Context<Self>) -> Div {
        let lit = self.file_drag.is_some();
        div().flex_1().flex().items_center().justify_center().bg(rgb(LANE)).child(
            div()
                .w(px(520.))
                .h(px(170.))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .rounded(px(14.))
                .border_1()
                .border_dashed()
                .border_color(if lit { rgb(ACCENT).into() } else { alpha(0xffffff, 0.16) })
                .bg(if lit { alpha(ACCENT, 0.06) } else { alpha(0xffffff, 0.02) })
                .cursor_pointer()
                .hover(|d| d.bg(alpha(0xffffff, 0.05)))
                .debug_selector(|| "sampler-drop".into())
                .child(icon(Icon::Wave, 24.).text_color(rgb(TEXT_2)))
                .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child("Drop samples here"))
                .child(div().text_size(px(12.)).text_color(rgb(TEXT_3)).child("One plays across the keys; several make a kit from C1. Or click to choose."))
                .on_mouse_down(MouseButton::Left, cx.listener(|ws, _, _, cx| ws.pick_samples(cx))),
        )
    }
}
