//! The page: its rail of studies, the sheet, what shows when there are none, and files dropped on it.

use super::*;

impl Workspace {
    pub fn study_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let rail = self.study_rail(cx);
        let main = match (self.opened_picture(), self.opened_study()) {
            (Some(bp), _) => self.picture_sheet(&bp, cx).into_any_element(),
            (None, Some(s)) => self.study_sheet(&s, cx).into_any_element(),
            (None, None) => self.study_empty(cx).into_any_element(),
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
            .when(big, |d| d.flex_col())
            .items_center()
            .justify_center()
            .gap(px(if big { 10. } else { 8. }))
            .when(big, |d| d.w(px(460.)).h(px(200.)))
            .when(!big, |d| d.flex_none().h(px(36.)))
            .rounded(px(if big { 14. } else { 8. }))
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
                        "Study a song or picture"
                    }),
            )
            .when(big, |d| {
                d.child(
                    div()
                        .text_size(px(12.5))
                        .text_color(rgb(TEXT_3))
                        .child("Drop a WAV, AIFF, FLAC or MP3, a spectrogram picture, or click"),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| ws.pick_study(cx)),
            )
    }

    /// The songs studied, down the left.
    pub(crate) fn study_rail(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let open = self.studies.open.clone();
        let (query, only) = (self.studies.query.clone(), self.studies.only);
        let shown = |s: &Study| {
            let kind = match only {
                Only::All => true,
                Only::Songs => s.nature.song,
                Only::Loops => !s.nature.song,
            };
            kind && (query.is_empty()
                || title_of(s).to_lowercase().contains(&query)
                || s.nature.name.to_lowercase().contains(&query))
        };
        let total = self.studies.list.len();
        let count = self.studies.list.iter().filter(|s| shown(s)).count();
        let filters = [
            (Only::All, "All"),
            (Only::Songs, "Songs"),
            (Only::Loops, "Loops"),
        ]
        .map(|(which, name)| {
            let on = which == only;
            div()
                .id(name)
                .debug_selector(move || format!("study-only-{}", name.to_lowercase()))
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
                        ws.studies.only = which;
                        cx.notify();
                    }),
                )
        });
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
            .filter(|(_, s)| shown(s))
            .map(|(i, s)| {
                let on = open.as_ref() == Some(&s.file);
                let title = title_of(s);
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
                                    .child(format!(
                                        "{} · {}",
                                        s.nature.name,
                                        clock(s.identity.seconds)
                                    )),
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
            .child(self.study_drop(false, cx))
            .child(field_box(self.studies.search.clone(), "study-search").flex_none())
            .child(div().flex().flex_none().gap(px(4.)).children(filters))
            .child(
                div()
                    .px(px(2.))
                    .pt(px(2.))
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .debug_selector(|| "study-count".into())
                    .child(format!("{count} of {total}")),
            )
            .when(self.studies.stale.len() > 1, |d| {
                d.child(
                    div()
                        .px(px(2.))
                        .text_size(px(11.))
                        .text_color(rgb(AGENT_TEXT))
                        .child(format!(
                            "Studying {} older files again",
                            self.studies.stale.len()
                        )),
                )
            })
            .children(busy)
            .children(rows)
            .when(!self.studies.like.pictures.is_empty(), |d| {
                d.child(
                    div()
                        .px(px(6.))
                        .pt(px(12.))
                        .pb(px(4.))
                        .child(label("Spectrogram pictures")),
                )
            })
            .children(self.picture_rows(cx))
    }

    /// The pictures studied, in the rail: each with its spectrum's shape.
    fn picture_rows(&mut self, cx: &mut Context<Self>) -> Vec<Stateful<Div>> {
        let open = self.studies.open.clone();
        self.studies
            .like
            .pictures
            .iter()
            .enumerate()
            .map(|(i, bp)| {
                let on = open.as_ref() == Some(&bp.file);
                let file = bp.file.clone();
                let levels: Vec<f32> = (0..20)
                    .map(|k| {
                        let n = bp.frames.max(1);
                        let a = k * n / 20;
                        let z = ((k + 1) * n / 20).max(a + 1);
                        ((bp.level(a, z) + 40.0) / 40.0).clamp(0.05, 1.0)
                    })
                    .collect();
                div()
                    .id(("picture", i))
                    .debug_selector(move || format!("picture-{i}"))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(10.))
                    .py(px(7.))
                    .rounded(px(9.))
                    .cursor_pointer()
                    .when(on, |d| d.bg(rgb(RAISED)))
                    .when(!on, |d| d.hover(|d| d.bg(rgb(OFF))))
                    .child(
                        div()
                            .w(px(40.))
                            .h(px(26.))
                            .flex()
                            .items_end()
                            .gap(px(1.))
                            .children(levels.into_iter().map(|v| {
                                div()
                                    .flex_1()
                                    .h(gpui::relative(v))
                                    .rounded(px(1.))
                                    .bg(alpha(AMBER, 0.7))
                            })),
                    )
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
                                    .child(file_name(&bp.file)),
                            )
                            .child(div().text_size(px(11.)).text_color(rgb(TEXT_4)).child(
                                match bp.needs.is_empty() {
                                    true => "Picture · read".to_string(),
                                    false => format!("Picture · needs {}", bp.needs.len()),
                                },
                            )),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.show_study(file.clone(), cx)),
                    )
            })
            .collect()
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
}
