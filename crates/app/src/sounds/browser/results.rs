//! The page's results: their header, the list of samples, and what shows when there are none.

use super::*;

impl Workspace {
    /// Over the results: how many, and the scan's progress; for the list, its columns.
    pub(crate) fn results_header(&self, columns: bool, cx: &mut Context<Self>) -> Div {
        let b = &self.browser;
        let n = b.hits.len();
        let col = |label: &'static str, w: f32| {
            div()
                .w(px(w))
                .flex_none()
                .text_size(px(10.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_4))
                .child(label.to_uppercase())
        };
        div()
            .h(px(34.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(12.))
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(6.))
                    .whitespace_nowrap()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{n} {}", if n == 1 { "sound" } else { "sounds" })),
                    )
                    .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(
                        match b.text.is_empty() && b.key == KeyFilter::Any {
                            true => "by name",
                            false => "best match first",
                        },
                    )),
            )
            .when_some(b.area.as_ref(), |d, area| {
                d.child(
                    chip(format!("In the box · {}", area.len()), ACCENT)
                        .gap(px(4.))
                        .cursor_pointer()
                        .child(icon(Icon::Close, 10.).text_color(rgb(ACCENT)))
                        .debug_selector(|| "clear-area".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                ws.browser.area = None;
                                ws.refilter(cx);
                            }),
                        ),
                )
            })
            .child(div().flex_1())
            .children(self.scan_progress())
            .when(columns && b.scan.is_none(), |d| {
                d.child(col("Key", KEY_W))
                    .child(col("", SHIFT_W))
                    .child(col("BPM", BPM_W))
                    .child(col("Length", LENGTH_W))
                    .child(col("Fit", FIT_W))
                    // Over the agent's buttons.
                    .child(col("", 22.))
            })
    }

    /// Nothing to show: says why, and what to do.
    pub(crate) fn empty_results(&self, cx: &mut Context<Self>) -> Div {
        let b = &self.browser;
        let center = |title: String, body: String| {
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .p(px(24.))
                .child(
                    div()
                        .size(px(40.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(12.))
                        .bg(rgb(CARD))
                        .border_1()
                        .border_color(rgb(LINE))
                        .child(icon(Icon::Samples, 20.).text_color(rgb(TEXT_3))),
                )
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .max_w(px(420.))
                        .text_size(px(12.))
                        .text_color(rgb(TEXT_3))
                        .text_center()
                        .child(body),
                )
        };
        let add = |cx: &mut Context<Self>| {
            primary("Add folder…")
                .mt(px(6.))
                .debug_selector(|| "add-folder-empty".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, _, cx| ws.add_library_folder(cx)),
                )
        };
        if !b.loaded {
            return center("Loading the library…".into(), String::new());
        }
        if b.library.samples.is_empty() {
            if b.scan.is_some() {
                return center(
                    "Analysing your samples".into(),
                    "Each one is listened to once: what it is, its key and tempo, how it sounds. They show here as they're done."
                        .into(),
                )
                .children(self.scan_progress());
            }
            return match b.library.folders.is_empty() {
                true => center(
                    "Your library is empty".into(),
                    "Add a folder of samples. tempo listens to every sound in it, so you can find one by what it is, its key, its tempo, or how it sounds."
                        .into(),
                ),
                false => center(
                    "No samples here yet".into(),
                    format!(
                        "Nothing was found in {}. Add a folder with WAV, AIFF, FLAC or MP3 files.",
                        b.library
                            .folders
                            .iter()
                            .map(|f| f.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ),
            }
            .child(add(cx));
        }
        // Filters that found nothing: offer to loosen each.
        let mut loosen: Vec<(&'static str, Loosen)> = Vec::new();
        if !b.text.is_empty() {
            loosen.push((
                "Clear the search",
                Box::new(|ws: &mut Workspace| ws.browser.text.clear()),
            ));
        }
        if b.key != KeyFilter::Any {
            loosen.push((
                "Any key",
                Box::new(|ws: &mut Workspace| ws.browser.key = KeyFilter::Any),
            ));
        }
        if b.song_tempo {
            loosen.push((
                "Any tempo",
                Box::new(|ws: &mut Workspace| ws.browser.song_tempo = false),
            ));
        }
        if b.kind.is_some() {
            loosen.push((
                "All kinds",
                Box::new(|ws: &mut Workspace| ws.browser.kind = None),
            ));
        }
        if !b.categories.is_empty() {
            loosen.push((
                "Every sound",
                Box::new(|ws: &mut Workspace| ws.browser.categories.clear()),
            ));
        }
        if !b.traits.is_empty() {
            loosen.push((
                "Any character",
                Box::new(|ws: &mut Workspace| ws.browser.traits.clear()),
            ));
        }
        if b.area.is_some() {
            loosen.push((
                "The whole map",
                Box::new(|ws: &mut Workspace| ws.browser.area = None),
            ));
        }
        let what = match b.text.is_empty() {
            true => "Nothing matches all of these filters.".to_string(),
            false => format!("Nothing matches “{}” with these filters.", b.text),
        };
        let buttons: Vec<Div> = loosen
            .into_iter()
            .enumerate()
            .map(|(i, (label, undo))| {
                button(label, false)
                    .debug_selector(move || format!("loosen-{i}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            undo(ws);
                            if ws.browser.text.is_empty() {
                                ws.browser.search.update(cx, |i, cx| i.set_text("", cx));
                            }
                            ws.refilter(cx);
                        }),
                    )
            })
            .collect();
        center(
            "Nothing found".into(),
            format!("{what} Loosen one to see more:"),
        )
        .child(
            div()
                .mt(px(4.))
                .flex()
                .flex_wrap()
                .justify_center()
                .gap(px(6.))
                .children(buttons),
        )
    }

    pub(crate) fn results_list(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let n = self.browser.hits.len();
        let body = match n {
            0 => self.empty_results(cx).into_any_element(),
            n => uniform_list(
                "sample-list",
                n,
                cx.processor(|ws, range: Range<usize>, _, cx| ws.sample_rows(range, cx)),
            )
            .track_scroll(&self.browser.list)
            .flex_1()
            .into_any_element(),
        };
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(self.results_header(n > 0, cx))
            .child(body)
            .child(self.info_line())
    }

    /// Rows of the results list, as the list draws them.
    pub(crate) fn sample_rows(
        &mut self,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::Stateful<Div>> {
        range
            .filter_map(|pos| {
                let hit = self.browser.hits.get(pos)?.clone();
                Some(self.sample_row(pos, &hit, cx))
            })
            .collect()
    }

    pub(crate) fn sample_row(
        &self,
        pos: usize,
        hit: &Hit,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let b = &self.browser;
        let index = hit.index;
        let s = &b.library.samples[index];
        let a = &s.analysis;
        let color = category_color(a.category);
        let selected = b.selected == Some(index);
        let playing = b.audition.is_some_and(|x| x.0 == index);
        let semis = self.sample_transpose(index);
        let traits = a
            .traits
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(" · ");
        let mono = |s: String, w: f32, c: u32| {
            div()
                .w(px(w))
                .flex_none()
                .font_family(MONO)
                .text_size(px(11.5))
                .text_color(rgb(c))
                .whitespace_nowrap()
                .overflow_hidden()
                .child(s)
        };
        let tone = tone_text(a);
        let has_tone = tone.is_some();
        let give = self.agent_button(&s.path, "sample-row", cx);
        div()
            .id(("sample-row", pos))
            .group("sample-row")
            .w_full()
            .h(px(ROW_H))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(10.))
            .relative()
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .bg(if selected {
                alpha(ACCENT, 0.12)
            } else {
                gpui::transparent_black()
            })
            .when(!selected, |d| d.hover(|s| s.bg(rgb(HEADERS))))
            .cursor_pointer()
            .debug_selector(move || format!("sample-row-{pos}"))
            .when(selected, |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px(2.))
                        .h_full()
                        .bg(rgb(ACCENT)),
                )
            })
            .child(
                div()
                    .size(px(22.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(11.))
                    .bg(rgb(if playing { ACCENT } else { CARD }))
                    .border_1()
                    .border_color(rgb(if playing { ACCENT } else { LINE }))
                    .group_hover("sample-row", |s| s.border_color(rgb(LINE_STRONG)))
                    .child(icon(Icon::Play, 10.).text_color(rgb(if playing {
                        0xffffff
                    } else {
                        TEXT_2
                    }))),
            )
            .child(div().w(px(WAVE_W)).h(px(26.)).flex_none().child(wave(
                a.waveform.clone(),
                color,
                if selected { 1.0 } else { 0.75 },
            )))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(if selected { TEXT } else { TEXT_2 }))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(s.name.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(chip(a.category.name(), color).h(px(16.)).px(px(5.)))
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_3))
                                    .child(kind_name(a.kind)),
                            )
                            .when(!traits.is_empty(), |d| {
                                d.child(
                                    div()
                                        .text_size(px(10.5))
                                        .text_color(rgb(TEXT_3))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(format!("· {traits}")),
                                )
                            }),
                    ),
            )
            .child(
                mono(
                    tone.unwrap_or_else(|| "—".into()),
                    KEY_W,
                    if has_tone { TEXT_2 } else { TEXT_4 },
                )
                .text_ellipsis(),
            )
            // How far Play in Key moves it, to the nearest semitone.
            .child(
                mono(
                    match semis.round() {
                        0.0 => String::new(),
                        s => semis_text(s),
                    },
                    SHIFT_W,
                    ACCENT_TEXT,
                )
                .text_size(px(10.5)),
            )
            .child(mono(
                a.bpm.map_or("—".into(), |b| format!("{b:.0}")),
                BPM_W,
                if a.bpm.is_some() { TEXT_2 } else { TEXT_4 },
            ))
            .child(mono(seconds_text(a.seconds), LENGTH_W, TEXT_3))
            .child(
                div()
                    .w(px(FIT_W))
                    .flex_none()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .size(px(8.))
                            .rounded(px(4.))
                            .bg(alpha(ACCENT, 0.2 + 0.8 * hit.score.clamp(0.0, 1.0))),
                    ),
            )
            .child(give)
            .on_hover(cx.listener(move |ws, over: &bool, _, cx| {
                let now = if *over { Some(index) } else { None };
                if ws.browser.hovered != now && (*over || ws.browser.hovered == Some(index)) {
                    ws.browser.hovered = now;
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    ws.select_sample(index, true, cx);
                    let at = pt(e.position);
                    ws.begin_drag(
                        Drag::Sample {
                            index,
                            from: at,
                            at,
                            moved: false,
                        },
                        cx,
                    );
                }),
            )
    }

    /// Under the results: the hovered (else selected) sample in a line, and why it's there.
    pub(crate) fn info_line(&self) -> Div {
        let b = &self.browser;
        let shown = b
            .hovered
            .or(b.selected)
            .and_then(|i| Some((i, b.sample(i)?)));
        div()
            .h(px(28.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .border_t_1()
            .border_color(rgb(LINE_SOFT))
            .bg(rgb(SURFACE))
            .text_size(px(11.))
            .whitespace_nowrap()
            .overflow_hidden()
            .children(shown.map(|(i, s)| {
                let reasons = b.hit(i).map(|h| h.reasons.join(", ")).unwrap_or_default();
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .min_w(px(0.))
                    .child(
                        div()
                            .size(px(6.))
                            .flex_none()
                            .rounded(px(3.))
                            .bg(rgb(category_color(s.analysis.category))),
                    )
                    .child(
                        div()
                            .text_color(rgb(TEXT_2))
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(search::describe(s)),
                    )
                    .when(!reasons.is_empty(), |d| {
                        d.child(
                            div()
                                .flex_none()
                                .text_color(rgb(ACCENT_TEXT))
                                .child(format!("Why: {reasons}")),
                        )
                    })
            }))
            .when(shown.is_none(), |d| {
                d.text_color(rgb(TEXT_4))
                    .child("Hover a sound to see what it is · drag one onto the tracks to use it")
            })
    }
}
