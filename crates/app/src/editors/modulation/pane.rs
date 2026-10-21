//! The Modulation pane: its cards, the modulators to add, and the one being dragged.

use super::*;

impl Workspace {
    /// The Modulation pane: the selected track's modulators as cards, and ones to add.
    pub fn modulation_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.track;
        let Some(track) = self.session.project.tracks.get(t) else {
            return div().flex_1().into_any_element();
        };
        let count = track.modulators.len();
        let cards: Vec<Div> = (0..count).map(|k| self.mod_card(t, k, cx)).collect();
        let add = |label: &'static str, kind: ModKind, cx: &mut Context<Self>| {
            div()
                .h(px(22.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(7.))
                .border_1()
                .border_color(rgb(LINE))
                .bg(rgb(CARD))
                .text_size(px(11.))
                .text_color(rgb(TEXT_2))
                .cursor_pointer()
                .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                .debug_selector(move || format!("add-mod-{label}"))
                .child(format!("+ {label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.add_modulator(t, kind.clone(), cx)),
                )
        };
        // A follower starts on the kick, or else the first other track.
        let tracks = &self.session.project.tracks;
        let source = tracks
            .iter()
            .find(|o| o.id != track.id && o.name.to_lowercase().contains("kick"))
            .or_else(|| tracks.iter().find(|o| o.id != track.id))
            .map_or(track.id, |o| o.id);
        let adders = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .flex_none()
            .child(
                div()
                    .mr(px(2.))
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child("Add"),
            )
            .child(add("LFO", ModKind::lfo(), cx))
            .child(add("Steps", ModKind::steps(), cx))
            .child(add("Envelope", ModKind::envelope(), cx))
            .child(add("Follower", ModKind::follower(source), cx))
            .child(add("Random", ModKind::random(), cx))
            .child(add("Macro", ModKind::mac(), cx))
            .when_some(self.shared_menu(t), |d, items| {
                d.child(
                    div()
                        .h(px(22.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .rounded(px(7.))
                        .border_1()
                        .border_color(rgb(LINE))
                        .bg(rgb(CARD))
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_2))
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                        .debug_selector(|| "add-mod-shared".into())
                        .child("+ Shared ▾")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.menu = Some(Menu {
                                    at: e.position,
                                    items: items.clone(),
                                    ..Default::default()
                                });
                                cx.notify();
                            }),
                        ),
                )
            });
        let hint = match (count, self.mod_edit) {
            (0, _) => "Add a modulator, then drag its ↗ Map onto any knob or fader of this track: its volume and pan, Smart Controls, EQ, compressor, saturator, Sampler, a send".to_string(),
            (_, Some((tt, k))) if tt == t => format!(
                "{} picked: drag a knob it moves to set how far · click the card again to let go",
                self.mod_name(t, k)
            ),
            _ => "Drag a card's ↗ onto a knob · drag a route sideways for its depth · click a card, then drag its knobs for depth".to_string(),
        };
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .bg(rgb(LANE))
            .child(
                div()
                    .h(px(34.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(14.))
                    .border_b_1()
                    .border_color(rgb(LINE_SOFT))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(hint),
                    )
                    .child(adders),
            )
            .child(
                div()
                    .id("mod-cards")
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .gap(px(10.))
                    .p(px(12.))
                    .overflow_x_scroll()
                    .children(cards),
            )
            .into_any_element()
    }

    /// The modulator being dragged to a knob, under the pointer.
    pub fn mod_ghost(&self) -> Option<impl IntoElement> {
        let Some(Drag::Modulator { track, index }) = self.drag else {
            return None;
        };
        let at = self.mod_drag_at?;
        let color = MOD_COLORS[index % MAX_MODS];
        Some(
            div()
                .absolute()
                .left(px(at.x + 14.))
                .top(px(at.y + 10.))
                .flex()
                .items_center()
                .gap(px(7.))
                .px(px(10.))
                .py(px(6.))
                .rounded(px(9.))
                .bg(rgb(0x1f1f26))
                .border_1()
                .border_color(alpha(color, 0.6))
                .shadow_lg()
                .text_size(px(12.))
                .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(color)))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(self.mod_name(track, index)),
                )
                .child(div().text_color(rgb(TEXT_3)).child("drop it on a knob")),
        )
    }
}
