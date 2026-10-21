//! The genres the song is closest to.

use super::*;

impl Workspace {
    /// The playbooks it's nearest, and why.
    pub(crate) fn study_genres(&mut self, s: &Study, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(heading(
                "Nearest playbooks",
                "By tempo, feel, mode, groove and spectrum",
            ))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .children(s.genres.iter().enumerate().map(|(i, g)| {
                        let c = genre_color(&g.id);
                        let id = g.id.clone();
                        div()
                            .id(("study-genre", i))
                            .debug_selector(move || format!("study-genre-{i}"))
                            .flex_1()
                            .min_w(px(0.))
                            .p(px(14.))
                            .rounded(px(14.))
                            .bg(rgb(CARD))
                            .border_1()
                            .border_color(rgb(LINE))
                            .cursor_pointer()
                            .hover(|d| d.border_color(alpha(c, 0.6)))
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(c)))
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_size(px(13.5))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(g.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .font_family(MONO)
                                            .text_size(px(12.))
                                            .text_color(rgb(tint(c, 0.3)))
                                            .child(format!("{:.0}%", g.score * 100.0)),
                                    ),
                            )
                            .child(
                                div().h(px(4.)).rounded(px(2.)).bg(rgb(WELL)).child(
                                    div()
                                        .h_full()
                                        .w(gpui::relative(g.score.clamp(0.0, 1.0)))
                                        .rounded(px(2.))
                                        .bg(rgb(c)),
                                ),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(rgb(TEXT_3))
                                    .child(g.why.clone()),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| ws.open_genres(Some(&id), cx)),
                            )
                    })),
            )
    }
}
