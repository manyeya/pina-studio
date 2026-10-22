//! The page: its rail of genres and the sheet.

use super::*;

impl Workspace {
    pub fn genres_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let rail = self.genre_rail(cx);
        let main = match self.genres.draft.clone() {
            Some(p) => self.genre_sheet(&p, cx).into_any_element(),
            None => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(TEXT_3))
                .child("No playbooks")
                .into_any_element(),
        };
        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .bg(rgb(BG))
            .child(rail)
            .child(main)
    }

    /// The genres, down the left.
    pub(crate) fn genre_rail(&mut self, cx: &mut Context<Self>) -> Div {
        let selected = self.genres.draft.as_ref().map(|p| p.id.clone());
        let song = self.session.project.genre.clone();
        let rows: Vec<_> = self
            .genres
            .list
            .iter()
            .map(|p| {
                let on = selected.as_deref() == Some(p.id.as_str());
                let color = genre_color(&p.id);
                let id = p.id.clone();
                let mark = match playbook::origin(p) {
                    Origin::Pina => None,
                    Origin::Edited => Some("Edited"),
                    Origin::Yours => Some("Yours"),
                };
                let here = song.as_deref() == Some(p.id.as_str());
                div()
                    .id(gpui::SharedString::from(format!("genre-{}", p.id)))
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(10.))
                    .py(px(7.))
                    .rounded(px(9.))
                    .cursor_pointer()
                    .when(on, |d| d.bg(rgb(RAISED)))
                    .when(!on, |d| d.hover(|d| d.bg(rgb(OFF))))
                    .debug_selector({
                        let id = p.id.clone();
                        move || format!("genre-{id}")
                    })
                    .child(ribbon(&p.sections, color, 40., 26.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(if on { TEXT } else { TEXT_2 }))
                                            .whitespace_nowrap()
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .child(p.name.clone()),
                                    )
                                    .when(here, |d| {
                                        d.child(div().size(px(6.)).rounded(px(3.)).bg(rgb(AGENT)))
                                    }),
                            )
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_3))
                                    .child(format!(
                                        "{:.0}–{:.0} · {}",
                                        p.tempo.0,
                                        p.tempo.1,
                                        if p.minor { "minor" } else { "major" }
                                    )),
                            ),
                    )
                    .when_some(mark, |d, m| {
                        d.child(
                            div()
                                .px(px(6.))
                                .h(px(17.))
                                .flex()
                                .items_center()
                                .rounded(px(5.))
                                .text_size(px(10.))
                                .font_weight(FontWeight::MEDIUM)
                                .bg(alpha(color, 0.14))
                                .text_color(rgb(tint(color, 0.35)))
                                .child(m),
                        )
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.select_genre(&id, cx)),
                    )
            })
            .collect();
        let problems = self.genres.problems.clone();
        div()
            .w(px(272.))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(14.))
                    .h(px(52.))
                    .child(
                        div()
                            .id("genres-back")
                            .size(px(28.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.))
                            .cursor_pointer()
                            .text_color(rgb(TEXT_3))
                            .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                            .child(icon(Icon::Back, 15.).text_color(rgb(TEXT_2)))
                            .debug_selector(|| "genres-back".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    ws.view = View::Arrange;
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Playbooks"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .child(format!("{} genres", self.genres.list.len())),
                            ),
                    ),
            )
            .child(
                div()
                    .id("genre-list")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .px(px(8.))
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .children(rows),
            )
            .when(!problems.is_empty(), |d| {
                d.child(
                    div()
                        .m(px(10.))
                        .p(px(9.))
                        .rounded(px(8.))
                        .bg(alpha(RECORD, 0.1))
                        .border_1()
                        .border_color(alpha(RECORD, 0.3))
                        .text_size(px(11.))
                        .line_height(px(15.))
                        .text_color(rgb(TEXT_2))
                        .child(format!("Can't read: {}", problems.join("; "))),
                )
            })
            .child(
                div()
                    .id("genres-folder")
                    .flex_none()
                    .m(px(10.))
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(rgb(LINE))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(OFF)))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_2))
                            .child("Show the files in Finder"),
                    )
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_4))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(
                                pina_engine::paths::support()
                                    .join("playbooks")
                                    .display()
                                    .to_string(),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            if let Err(e) = crate::agent::produce::edit_playbooks(None) {
                                ws.status = Some(e);
                                cx.notify();
                            }
                        }),
                    ),
            )
    }

    /// One genre, top to bottom.
    pub(crate) fn genre_sheet(&mut self, p: &Playbook, cx: &mut Context<Self>) -> Stateful<Div> {
        let color = genre_color(&p.id);
        div()
            .id("genre-sheet")
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.genres.scroll)
            .child(self.genre_hero(p, color, cx))
            .child(
                div()
                    .px(px(36.))
                    .pb(px(48.))
                    .flex()
                    .flex_col()
                    .gap(px(30.))
                    .child(self.genre_feel(p, color, cx))
                    .child(self.genre_map(p, color, cx))
                    .child(self.genre_parts(p, cx))
                    .child(self.genre_mix(p, color, cx))
                    .child(self.genre_notes(p, cx)),
            )
    }
}
