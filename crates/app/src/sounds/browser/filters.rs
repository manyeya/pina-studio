//! The page's search field and filters.

use super::*;

impl Workspace {
    /// The editor bar's controls: List, Map or Kit, and Play in Key.
    pub fn samples_controls(&mut self, cx: &mut Context<Self>) -> Div {
        let view = |label: &'static str, v: Results, on: bool, cx: &mut Context<Self>| {
            seg(label, on, 24.)
                .debug_selector(move || format!("results-{v:?}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.browser.view = v;
                        if v == Results::Kit {
                            ws.build_kit();
                        }
                        cx.notify();
                    }),
                )
        };
        let b = &self.browser;
        let key = self
            .sample_key()
            .or_else(|| self.session.project.song_key());
        div()
            .flex()
            .items_center()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .debug_selector(|| "play-in-key".into())
                    .child(switch(b.play_in_key))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(if b.play_in_key { TEXT } else { TEXT_2 }))
                            .whitespace_nowrap()
                            .child(match key {
                                Some(k) => format!("Play in {}", k.name()),
                                None => "Play in key".to_string(),
                            }),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.browser.play_in_key = !ws.browser.play_in_key;
                            cx.notify();
                        }),
                    ),
            )
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(8.))
                    .child(view("List", Results::List, b.view == Results::List, cx))
                    .child(view("Map", Results::Map, b.view == Results::Map, cx))
                    .child(view("Kit", Results::Kit, b.view == Results::Kit, cx)),
            )
    }

    /// The search field: words, as they're typed.
    pub(crate) fn search_field(&self, cx: &mut Context<Self>) -> Div {
        let b = &self.browser;
        let focused = b.searching;
        div()
            .h(px(32.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(8.))
            .bg(rgb(WELL))
            .border_1()
            .border_color(if focused {
                alpha(ACCENT, 0.8)
            } else {
                rgb(LINE).into()
            })
            .cursor_text()
            .debug_selector(|| "sample-search".into())
            .child(icon(Icon::Search, 14.).text_color(rgb(if focused { TEXT_2 } else { TEXT_3 })))
            .child(div().flex_1().min_w(px(0.)).child(b.search.clone()))
            .when(!b.text.is_empty(), |d| {
                d.child(
                    div()
                        .size(px(18.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(9.))
                        .bg(rgb(RAISED))
                        .cursor_pointer()
                        .child(icon(Icon::Close, 11.).text_color(rgb(TEXT_2)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.browser.search.update(cx, |i, cx| i.set_text("", cx));
                            }),
                        ),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    ws.browser.focus_search = true;
                    cx.notify();
                }),
            )
    }

    /// All, one-shots or loops.
    pub(crate) fn kind_tray(&self, cx: &mut Context<Self>) -> Div {
        let b = &self.browser;
        let kind = |label: &'static str, k: Option<Kind>, on: bool, cx: &mut Context<Self>| {
            seg(label, on, 24.)
                .flex_1()
                .debug_selector(move || format!("kind-{k:?}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.browser.kind = k;
                        ws.refilter(cx);
                    }),
                )
        };
        tray()
            .p(px(2.))
            .rounded(px(8.))
            .child(kind("Any", None, b.kind.is_none(), cx))
            .child(kind(
                "One-shots",
                Some(Kind::OneShot),
                b.kind == Some(Kind::OneShot),
                cx,
            ))
            .child(kind(
                "Loops",
                Some(Kind::Loop),
                b.kind == Some(Kind::Loop),
                cx,
            ))
    }

    /// A chip for each sound the library has, drums first, with how many it would find.
    pub(crate) fn category_chips(&self, cx: &mut Context<Self>) -> Vec<Div> {
        let b = &self.browser;
        // What the search's words name lights too; clicked, those words come out of it.
        let said = search::named(&b.text).0;
        Category::ALL
            .iter()
            .copied()
            .filter(|c| b.counts.get(c).is_some_and(|&n| n > 0) || b.categories.contains(c))
            .map(|c| {
                let picked = b.categories.contains(&c);
                let worded = said.contains(&c) && !picked;
                filter_chip(
                    c.name(),
                    Some(b.counts.get(&c).copied().unwrap_or(0)),
                    picked || worded,
                    Some(category_color(c)),
                )
                .debug_selector(move || format!("category-{}", c.name()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| match worded {
                        true => ws.unsay(Some(c), None, cx),
                        false => ws.toggle_category(c, cx),
                    }),
                )
            })
            .collect()
    }

    /// The left column: what to look for, and the library.
    pub(crate) fn filters(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let b = &self.browser;
        let search = self.search_field(cx);
        let song_key = self.session.project.song_key();
        let tempo = self.session.project.tempo;
        let fits = self.fits_song();
        let fits_row = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .p(px(10.))
            .rounded(px(8.))
            .bg(if fits {
                alpha(ACCENT, 0.1)
            } else {
                rgb(CARD).into()
            })
            .border_1()
            .border_color(if fits {
                alpha(ACCENT, 0.45)
            } else {
                rgb(LINE).into()
            })
            .cursor_pointer()
            .debug_selector(|| "fits-song".into())
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Fits the song"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(match song_key {
                                Some(k) => format!("{} · {tempo:.0} BPM", k.name()),
                                None => format!("Its key and {tempo:.0} BPM"),
                            }),
                    ),
            )
            .child(switch(fits))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| ws.set_fits_song(!fits, cx)),
            );
        let key_label = match b.key {
            KeyFilter::Any => "Any key".to_string(),
            KeyFilter::Song => match song_key {
                Some(k) => format!("Song key ({})", k.short()),
                None => "Song key".to_string(),
            },
            KeyFilter::Key(k) => k.name(),
        };
        let key_label = match b.compatible && b.key != KeyFilter::Any {
            true => format!("{key_label} +"),
            false => key_label,
        };
        let labelled = |label: &'static str, control: Div| {
            div()
                .h(px(28.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(TEXT_3))
                        .child(label),
                )
                .child(control)
        };
        let key_row = labelled(
            "Key",
            dropdown(key_label, 26.)
                .max_w(px(140.))
                .debug_selector(|| "sample-key".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                        ws.menu = Some(ws.sample_key_menu(e.position));
                        cx.notify();
                    }),
                ),
        );
        let tempo_seg = |label: String, on: bool, song: bool, cx: &mut Context<Self>| {
            seg(label, on, 22.)
                .debug_selector(move || format!("sample-tempo-{song}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.browser.song_tempo = song;
                        ws.refilter(cx);
                    }),
                )
        };
        let tempo_row = labelled(
            "Tempo",
            tray()
                .p(px(2.))
                .rounded(px(8.))
                .child(tempo_seg("Any".into(), !b.song_tempo, false, cx))
                .child(tempo_seg(format!("{tempo:.0} BPM"), b.song_tempo, true, cx)),
        );
        let categories = self.category_chips(cx);
        let said_traits = search::named(&b.text).1;
        let traits: Vec<Div> = TRAITS
            .iter()
            .map(|(t, _)| *t)
            .filter(|t| {
                b.trait_counts.get(*t).is_some_and(|&n| n > 0) || b.traits.iter().any(|x| x == t)
            })
            .map(|t| {
                let picked = b.traits.iter().any(|x| x == t);
                let worded = said_traits.iter().any(|x| x == t) && !picked;
                filter_chip(t, b.trait_counts.get(t).copied(), picked || worded, None)
                    .debug_selector(move || format!("trait-{t}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| match worded {
                            true => ws.unsay(None, Some(t), cx),
                            false => ws.toggle_trait(t, cx),
                        }),
                    )
            })
            .collect();
        let any_sounds = !b.categories.is_empty();
        let any_traits = !b.traits.is_empty();
        let has_traits = !traits.is_empty();
        let folders: Vec<gpui::Stateful<Div>> = b
            .library
            .folders
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let count = b
                    .library
                    .samples
                    .iter()
                    .filter(|s| s.path.starts_with(f))
                    .count();
                let name = f.file_name().map_or_else(
                    || f.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                div()
                    .id(SharedString::from(format!("folder-{i}")))
                    .group("folder")
                    .h(px(26.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(6.))
                    .hover(|s| s.bg(rgb(OFF)))
                    .child(icon(Icon::Folder, 13.).text_color(rgb(TEXT_3)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_2))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(name),
                    )
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_3))
                            .child(count.to_string()),
                    )
                    .child(
                        div()
                            .invisible()
                            .group_hover("folder", |s| s.visible())
                            .cursor_pointer()
                            .child(icon(Icon::Close, 12.).text_color(rgb(TEXT_3)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| ws.remove_library_folder(i, cx)),
                            ),
                    )
            })
            .collect();
        let scanning = b.scan.is_some();
        div()
            .id("sample-filters")
            .w(px(self.header_w))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(search)
                    .child(self.kind_tray(cx))
                    .child(fits_row)
                    .child(div().flex().flex_col().child(key_row).child(tempo_row))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(heading("Sounds").when(any_sounds, |d| {
                                d.child(link("Clear").on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| {
                                        ws.browser.categories.clear();
                                        ws.refilter(cx);
                                    }),
                                ))
                            }))
                            .child(match categories.is_empty() {
                                true => {
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(TEXT_4))
                                        .child(match b.library.samples.is_empty() {
                                            true => "What the library holds shows here.",
                                            false => "None with these filters.",
                                        })
                                }
                                false => div().flex().flex_wrap().gap(px(5.)).children(categories),
                            }),
                    )
                    .when(has_traits, |d| {
                        d.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(6.))
                                .child(heading("Character").when(any_traits, |d| {
                                    d.child(link("Clear").on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, _, _, cx| {
                                            ws.browser.traits.clear();
                                            ws.refilter(cx);
                                        }),
                                    ))
                                }))
                                .child(div().flex().flex_wrap().gap(px(5.)).children(traits)),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                heading("Library").child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(4.))
                                        .cursor_pointer()
                                        .text_color(rgb(TEXT_3))
                                        .hover(|s| s.text_color(rgb(TEXT)))
                                        .debug_selector(|| "rescan".into())
                                        .child(icon(Icon::Rescan, 12.).text_color(rgb(TEXT_3)))
                                        .child(if scanning { "Scanning" } else { "Rescan" })
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|ws, _, _, cx| ws.rescan(cx)),
                                        ),
                                ),
                            )
                            .children(folders)
                            .child(
                                button("Add folder…", false)
                                    .mt(px(4.))
                                    .child(icon(Icon::Plus, 12.).text_color(rgb(TEXT_3)))
                                    .flex_row_reverse()
                                    .debug_selector(|| "add-folder".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, _, _, cx| ws.add_library_folder(cx)),
                                    ),
                            ),
                    ),
            )
    }

    /// The scan's progress, as a line and a bar.
    pub(crate) fn scan_progress(&self) -> Option<Div> {
        let p = self.browser.scan.as_ref()?;
        let (done, of) = (p[0].load(Ordering::Relaxed), p[1].load(Ordering::Relaxed));
        let text = match of {
            0 => "Looking for new samples…".to_string(),
            of => format!("Analysing {done} of {of}…"),
        };
        let part = if of == 0 {
            0.0
        } else {
            done as f32 / of as f32
        };
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(11.))
                .text_color(rgb(TEXT_2))
                .whitespace_nowrap()
                .child(text)
                .child(
                    div()
                        .w(px(80.))
                        .h(px(4.))
                        .rounded(px(2.))
                        .bg(rgb(RAISED))
                        .child(
                            div()
                                .h_full()
                                .w(relative(part))
                                .rounded(px(2.))
                                .bg(rgb(ACCENT)),
                        ),
                ),
        )
    }
}
