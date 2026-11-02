//! The page: its rail of genres and the sheet.

use super::*;
use crate::pages::study::act;

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
                .debug_selector(|| "no-playbooks".into())
                .child(
                    "No playbooks. Make one from a song on the Song Study page, or as JSON in the \
                     playbooks folder: the Studio Agent uses one only when you ask it to.",
                )
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
        let (query, shelf) = (self.genres.query.clone(), self.genres.shelf);
        let tempo = self.session.project.tempo;
        let shown = |p: &&Playbook| {
            let kept = match shelf {
                Shelf::All => true,
                Shelf::Fits => tempo >= p.tempo.0 && tempo <= p.tempo.1,
                Shelf::Minor => p.minor,
                Shelf::Major => !p.minor,
            };
            kept && (query.is_empty() || p.name.to_lowercase().contains(&query))
        };
        let total = self.genres.list.len();
        let count = self.genres.list.iter().filter(shown).count();
        let filters = [
            (Shelf::All, "All".to_string(), "all"),
            (Shelf::Fits, format!("Fits {tempo:.0} BPM"), "fits"),
            (Shelf::Minor, "Minor".to_string(), "minor"),
            (Shelf::Major, "Major".to_string(), "major"),
        ]
        .map(|(which, name, sel)| {
            let on = which == shelf;
            div()
                .id(sel)
                .debug_selector(move || format!("genres-only-{sel}"))
                .px(px(9.))
                .py(px(2.))
                .rounded(px(10.))
                .border_1()
                .border_color(rgb(if on { LINE_STRONG } else { LINE }))
                .when(on, |d| d.bg(rgb(FIELD)))
                .text_size(px(11.))
                .text_color(rgb(if on { TEXT } else { TEXT_3 }))
                .cursor_pointer()
                .child(name)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.genres.shelf = which;
                        cx.notify();
                    }),
                )
        });
        let rows: Vec<_> = self
            .genres
            .list
            .iter()
            .filter(shown)
            .map(|p| {
                let on = selected.as_deref() == Some(p.id.as_str());
                let color = genre_color(&p.id);
                let id = p.id.clone();
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
                                    .debug_selector(|| "genres-count".into())
                                    .child(format!("{count} of {total}")),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(10.))
                    .pb(px(8.))
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        act("New playbook", "genre-new", false)
                            .justify_center()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| ws.new_genre(cx)),
                            ),
                    )
                    .child(field_box(self.genres.search.clone(), "genres-search").flex_none())
                    .child(div().flex().flex_wrap().gap(px(4.)).children(filters)),
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
}
