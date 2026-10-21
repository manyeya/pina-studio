//! A genre's top: its name, tempo and feel at a glance.

use super::*;

impl Workspace {
    /// The genre's name over its song's shape.
    pub(crate) fn genre_hero(&mut self, p: &Playbook, color: u32, cx: &mut Context<Self>) -> Div {
        let origin = playbook::origin(p);
        let bars: u64 = p.sections.iter().map(|s| s.bars).sum();
        let here = self.session.project.genre.as_deref() == Some(p.id.as_str());
        let name = self.genres.fields.as_ref().map(|f| f.name.clone());
        let sections = p.sections.clone();
        let action = |label: &'static str, primary: bool| {
            div()
                .h(px(32.))
                .px(px(13.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(9.))
                .text_size(px(12.5))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .when(primary, |d| {
                    d.bg(rgb(TEXT))
                        .text_color(rgb(BG))
                        .hover(|d| d.bg(rgb(0xffffff)))
                })
                .when(!primary, |d| {
                    d.bg(alpha(0x000000, 0.35))
                        .border_1()
                        .border_color(alpha(0xffffff, 0.12))
                        .text_color(rgb(TEXT_2))
                        .hover(|d| d.bg(alpha(0xffffff, 0.08)).text_color(rgb(TEXT)))
                })
                .child(label)
        };
        div()
            .relative()
            .h(px(236.))
            .overflow_hidden()
            .mb(px(26.))
            .child(
                canvas(
                    |_, _, _| {},
                    move |b, _, window, _| landscape(b, &sections, color, window),
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .left(px(36.))
                    .right(px(36.))
                    .top(px(30.))
                    .flex()
                    .items_start()
                    .gap(px(16.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .text_size(px(10.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(tint(color, 0.3)))
                                    .child("PLAYBOOK")
                                    .child(div().text_color(rgb(TEXT_4)).child("·"))
                                    .child(
                                        div()
                                            .font_family(MONO)
                                            .font_weight(FontWeight::NORMAL)
                                            .text_color(rgb(TEXT_3))
                                            .child(p.id.clone()),
                                    )
                                    .child(div().text_color(rgb(TEXT_4)).child("·"))
                                    .child(div().text_color(rgb(TEXT_3)).child(match origin {
                                        Origin::Pina => "Pina Studio's",
                                        Origin::Edited => "Pina Studio's, edited",
                                        Origin::Yours => "yours",
                                    })),
                            )
                            .child(
                                div()
                                    .h(px(46.))
                                    .debug_selector(|| "genre-name".into())
                                    .when_some(name, |d, f| d.child(f)),
                            )
                            .child(div().text_size(px(13.)).text_color(rgb(TEXT_2)).child(
                                format!(
                                    "{bars} bars · {} sections · {} parts · {:.1} LUFS master",
                                    p.sections.len(),
                                    p.parts.len(),
                                    p.lufs
                                ),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                action(
                                    if here {
                                        "This song's genre"
                                    } else {
                                        "Use in this song"
                                    },
                                    !here,
                                )
                                .when(here, |d| d.opacity(0.8))
                                .debug_selector(|| "genre-use".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| ws.use_genre(cx)),
                                ),
                            )
                            .child(
                                action("Duplicate", false)
                                    .debug_selector(|| "genre-duplicate".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, _, _, cx| ws.duplicate_genre(cx)),
                                    ),
                            )
                            .when(origin != Origin::Pina, |d| {
                                d.child(
                                    action(
                                        match origin {
                                            Origin::Yours => "Delete",
                                            _ => "Reset to Pina Studio's",
                                        },
                                        false,
                                    )
                                    .debug_selector(|| "genre-reset".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, _, _, cx| ws.reset_genre(cx)),
                                    ),
                                )
                            }),
                    ),
            )
    }
}
