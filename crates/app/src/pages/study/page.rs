//! The page: its rail of studies, the sheet, what shows when there are none, and files dropped on it.

use super::*;

impl Workspace {
    pub fn study_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let rail = self.study_rail(cx);
        let main = match self.opened_study() {
            Some(s) => self.study_sheet(&s, cx).into_any_element(),
            None => self.study_empty(cx).into_any_element(),
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

    /// Where a song is dropped or picked: dashed, lit while a file's dragged over the window.
    pub(crate) fn study_drop(&self, big: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let dragging = self.file_drag.is_some();
        div()
            .id(if big { "study-drop-big" } else { "study-drop" })
            .debug_selector(move || {
                if big {
                    "study-drop-big".into()
                } else {
                    "study-drop".into()
                }
            })
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(if big { 10. } else { 4. }))
            .when(big, |d| d.w(px(460.)).h(px(200.)))
            .when(!big, |d| d.h(px(78.)))
            .rounded(px(14.))
            .border_1()
            .border_dashed()
            .border_color(if dragging {
                rgb(AGENT).into()
            } else {
                alpha(0xffffff, 0.16)
            })
            .bg(if dragging {
                alpha(AGENT, 0.08)
            } else {
                alpha(0xffffff, 0.02)
            })
            .cursor_pointer()
            .hover(|d| {
                d.bg(alpha(0xffffff, 0.05))
                    .border_color(alpha(0xffffff, 0.3))
            })
            .child(
                icon(Icon::Wave, if big { 26. } else { 16. }).text_color(rgb(if dragging {
                    AGENT
                } else {
                    TEXT_2
                })),
            )
            .child(
                div()
                    .text_size(px(if big { 15. } else { 12.5 }))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT))
                    .child(if dragging {
                        "Drop to study it"
                    } else {
                        "Study a song"
                    }),
            )
            .child(
                div()
                    .text_size(px(if big { 12.5 } else { 11. }))
                    .text_color(rgb(TEXT_3))
                    .child("Drop a WAV, AIFF, FLAC or MP3, or click"),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| ws.pick_study(cx)),
            )
    }

    /// The songs studied, down the left.
    pub(crate) fn study_rail(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let open = self.studies.open.clone();
        let busy: Vec<_> = self
            .studies
            .busy
            .iter()
            .map(|p| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(9.))
                    .bg(alpha(AGENT, 0.06))
                    .child(div().size(px(7.)).rounded(px(4.)).bg(rgb(AGENT)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(rgb(TEXT))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(file_name(p)),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(AGENT_TEXT))
                                    .child("Listening…"),
                            ),
                    )
            })
            .collect();
        let rows: Vec<_> = self
            .studies
            .list
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let on = open.as_ref() == Some(&s.file);
                let title = match s.identity.title.is_empty() {
                    true => file_name(&s.file),
                    false => s.identity.title.clone(),
                };
                let file = s.file.clone();
                let gone = s.file.clone();
                div()
                    .id(("study", i))
                    .group("study-row")
                    .debug_selector(move || format!("study-{i}"))
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
                    .child(barcode(s, 40., 26.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(if on { TEXT } else { TEXT_2 }))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_4))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(format!("{:.0} BPM · {}", s.tempo.bpm, s.key.name)),
                            ),
                    )
                    .child(
                        div()
                            .id(("forget", i))
                            .debug_selector(move || format!("study-forget-{i}"))
                            .invisible()
                            .group_hover("study-row", |d| d.visible())
                            .size(px(20.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.))
                            .hover(|d| d.bg(alpha(0xffffff, 0.08)))
                            .child(icon(Icon::Close, 11.).text_color(rgb(TEXT_3)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.forget_study(gone.clone(), cx);
                                }),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.show_study(file.clone(), cx)),
                    )
            })
            .collect();
        div()
            .id("study-rail")
            .w(px(256.))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(4.))
            .p(px(12.))
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(LINE))
            .overflow_y_scroll()
            .child(
                div()
                    .px(px(6.))
                    .pt(px(6.))
                    .pb(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Song Study"),
                    )
                    .child(div().text_size(px(11.5)).text_color(rgb(TEXT_3)).child(
                        "Any song, taken apart: its form, harmony, groove, sound and words.",
                    )),
            )
            .child(self.study_drop(false, cx))
            .child(div().h(px(10.)))
            .children(busy)
            .when(!self.studies.list.is_empty(), |d| {
                d.child(
                    div()
                        .px(px(6.))
                        .pt(px(6.))
                        .pb(px(4.))
                        .child(label("Studied")),
                )
            })
            .children(rows)
    }

    /// Nothing studied yet: what a study finds, and where to drop a song.
    pub(crate) fn study_empty(&mut self, cx: &mut Context<Self>) -> Div {
        let pillar = |title: &'static str, items: &'static str, color: u32| {
            div()
                .w(px(200.))
                .p(px(14.))
                .rounded(px(12.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(LINE))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .w(px(22.))
                        .h(px(3.))
                        .mb(px(4.))
                        .rounded(px(2.))
                        .bg(rgb(color)),
                )
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(TEXT_3))
                        .child(items),
                )
        };
        let busy = !self.studies.busy.is_empty();
        div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(28.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(28.))
                            .font_weight(FontWeight::BOLD)
                            .child(if busy { "Listening…" } else { "Take a song apart" }),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(TEXT_3))
                            .child(match busy {
                                true => "Its beats, chords, sections and sound, then its lyrics online.",
                                false => "Everything a song is made of, measured from its file. Its audio never leaves this Mac.",
                            }),
                    ),
            )
            .when(!busy, |d| d.child(self.study_drop(true, cx)))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(pillar("Identity", "Title, artist, year and genre, from its tags and online", SKY))
                    .child(pillar("Form", "Intro, verses, choruses, bridge: where each starts, and how hard it hits", CORAL))
                    .child(pillar("Music", "Tempo, key, chords, groove, swing, instruments, mix and dynamics", MINT))
                    .child(pillar("Words", "Lyrics by section, rhyme schemes, syllables, hooks, and what they mean", VIOLET)),
            )
    }

    pub(crate) fn study_sheet(&mut self, s: &Study, cx: &mut Context<Self>) -> Stateful<Div> {
        let color = tint_of(s);
        div()
            .id("study-sheet")
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.studies.scroll)
            .child(self.study_hero(s, color, cx))
            .child(
                div()
                    .px(px(36.))
                    .pb(px(56.))
                    .flex()
                    .flex_col()
                    .gap(px(34.))
                    .child(self.study_form(s, cx))
                    .child(
                        div()
                            .flex()
                            .gap(px(16.))
                            .child(harmony(s, color))
                            .child(rhythm(s, color)),
                    )
                    .child(instruments(s))
                    .child(self.study_mix(s, color))
                    .child(self.study_words(s, color, cx))
                    .child(self.study_genres(s, cx)),
            )
    }
}
