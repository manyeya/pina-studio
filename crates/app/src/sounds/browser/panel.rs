//! The compact browser beside the tracks: its frame, edge, rows and footer.

use super::*;

impl Workspace {
    /// The browser shows: as its page, or beside the tracks.
    /// The sample library showing: its page, or the Browser on Sounds.
    pub fn browser_open(&self) -> bool {
        self.view == View::Samples
            || (self.view == View::Arrange
                && self.show_browser
                && self.catalog.scope == Scope::Sounds)
    }

    /// F: the compact browser beside the tracks, or not; from the page or the mixer, the
    /// tracks with it beside them. It opens with the search field ready to type in.
    pub fn toggle_samples(&mut self, cx: &mut Context<Self>) {
        self.show_browser = self.view != View::Arrange || !self.show_browser;
        self.show_agent &= !self.show_browser;
        self.picking &= self.show_browser;
        self.view = View::Arrange;
        self.browser.focus_search = self.show_browser;
        cx.notify();
    }

    /// ⇧F: the browser's page, in the tracks' place, or the tracks again.
    pub fn toggle_samples_page(&mut self, cx: &mut Context<Self>) {
        self.view = match self.view {
            View::Samples => View::Arrange,
            _ => View::Samples,
        };
        self.browser.focus_search = self.view == View::Samples;
        cx.notify();
    }

    /// The right panel's left edge (Samples' or Agent's): a strip to grab, lit while it's held or
    /// under the pointer. Double-click puts the width back.
    pub fn panel_edge(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let width = self.panel_width();
        let resizing = matches!(self.drag, Some(Drag::Panel { .. }));
        div()
            .id("panel-edge")
            .absolute()
            .left(px(-4.))
            .top_0()
            .h_full()
            .w(px(8.))
            .flex()
            .justify_center()
            .cursor_col_resize()
            .child(
                div()
                    .h_full()
                    .w(px(2.))
                    .when(resizing, |d| d.bg(rgb(ACCENT))),
            )
            .hover(|s| s.bg(alpha(ACCENT, 0.35)))
            .debug_selector(|| "panel-edge".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if e.click_count == 2 {
                        ws.panel_w = PANEL_W;
                        cx.notify();
                        return;
                    }
                    let x = pt(e.position).x;
                    ws.begin_drag(Drag::Panel { x, width }, cx);
                }),
            )
    }

    /// The compact browser's width: as dragged, but never so wide the lanes lose their room,
    /// nor so narrow its controls don't fit.
    pub fn panel_width(&self) -> f32 {
        let inspector = if self.show_inspector {
            self.side_w()
        } else {
            0.0
        };
        let room = self.viewport_w - inspector - self.header_w - LANES_MIN_W;
        self.panel_w.min(room).max(PANEL_MIN_W)
    }

    /// What the compact list shows: what sounds like a sample, or the results.
    pub(crate) fn panel_hits(&self) -> &[Hit] {
        match &self.browser.like {
            Some((_, like)) => like,
            None => &self.browser.hits,
        }
    }

    /// Each frame: who has the keyboard, whether the preview has ended, and whether the song's
    /// key or tempo moved under results that follow them.
    pub fn browser_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = self.browser_open();
        let focus = self.browser.search.focus_handle(cx);
        if std::mem::take(&mut self.browser.focus_search) {
            if open {
                window.focus(&focus, cx);
            } else if self.view == View::Arrange && self.show_browser {
                let devices = self.catalog.search.focus_handle(cx);
                window.focus(&devices, cx);
            }
        }
        let focused = focus.is_focused(window);
        // The field went with the pane: the keyboard comes back.
        if focused && !open {
            self.refocus = true;
        }
        self.browser.searching = focused && open;
        if let Some((_, since, seconds)) = &mut self.browser.audition
            && since.elapsed().as_secs_f32() > *seconds
        {
            // A loop in time with the song goes round again while it plays.
            match self.browser.looping && self.player.playing() {
                true => *since += Duration::from_secs_f32(*seconds),
                false => self.browser.audition = None,
            }
        }
        if open && self.browser.loaded {
            let follows = self.browser.key == KeyFilter::Song || self.browser.song_tempo;
            let now = (self.session.project.song_key(), self.session.project.tempo);
            if follows && now != self.browser.seen {
                self.find_samples();
            }
        }
    }

    /// The Browser, beside the tracks: everything that goes in a song in one place, under one
    /// search. All (devices, presets, sounds and songs that match), Instruments, Effects and the
    /// sample library (Sounds); the search follows from tab to tab. Starred devices lead each
    /// list of devices.
    pub fn browser_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let scope = self.catalog.scope;
        let sounds = scope == Scope::Sounds;
        let detail = match sounds {
            true => self.samples_detail(),
            false => format!("{} devices", self.catalog.devices.len()),
        };
        let header = div()
            .h(px(TOP_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(12.))
            .pr(px(7.))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Browser"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(detail),
            )
            // The sample library has a page of its own, with its map and kits.
            .when(sounds, |d| {
                d.child(
                    icon_seg(Icon::Expand, false, 26.)
                        .debug_selector(|| "samples-page".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.toggle_samples_page(cx)),
                        ),
                )
            })
            .child(
                icon_seg(Icon::Close, false, 26.)
                    .debug_selector(|| "close-samples".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.show_browser = false;
                            cx.notify();
                        }),
                    ),
            );
        let search = match sounds {
            true => self.search_field(cx),
            false => div()
                .h(px(32.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(8.))
                .bg(rgb(WELL))
                .border_1()
                .border_color(rgb(LINE))
                .cursor_text()
                .child(icon(Icon::Search, 14.).text_color(rgb(TEXT_3)))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .debug_selector(|| "browser-search".into())
                        .child(self.catalog.search.clone()),
                ),
        };
        let tab = |label: &'static str, s: Scope, cx: &mut Context<Self>| {
            seg(label, scope == s, 26.)
                .flex_auto()
                .justify_center()
                .debug_selector(move || format!("scope-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.browse(s, cx)),
                )
        };
        let tabs = tray()
            .rounded(px(9.))
            .child(tab("All", Scope::All, cx))
            .child(tab("Instruments", Scope::Instruments, cx))
            .child(tab("Effects", Scope::Effects, cx))
            .child(tab("Sounds", Scope::Sounds, cx));
        let body = match sounds {
            true => self.sounds_body(cx),
            false => self.device_browser(cx),
        };
        div()
            .w(px(self.panel_width()))
            .flex_none()
            .h_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(SURFACE))
            .border_l_1()
            .border_color(rgb(LINE))
            .child(header)
            .children(self.picking_bar(cx))
            .child(
                div()
                    .flex_none()
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(search)
                    .child(tabs),
            )
            .child(body)
            .child(self.panel_edge(cx))
    }

    /// A Browser tab: its search carried over between the devices' field and the sounds', and
    /// the field ready to type in.
    pub fn browse(&mut self, scope: Scope, cx: &mut Context<Self>) {
        let (was, now) = (self.catalog.scope == Scope::Sounds, scope == Scope::Sounds);
        if was != now {
            let (from, to) = match now {
                true => (self.catalog.search.clone(), self.browser.search.clone()),
                false => (self.browser.search.clone(), self.catalog.search.clone()),
            };
            let text = from.read(cx).text().to_string();
            to.update(cx, |i, cx| i.set_text(&text, cx));
        }
        self.catalog.scope = scope;
        self.catalog.cat = self.catalog.cat.filter(|c| match scope {
            Scope::Instruments => c.instrument(),
            Scope::Effects => !c.instrument(),
            _ => true,
        });
        self.catalog.picked = None;
        self.show_browser = true;
        self.browser.focus_search = true;
        cx.notify();
    }

    /// The compact browser, beside the tracks: a search, what kind, the song's fit and what
    /// sounds; a list to hear and drag onto the tracks; and the picked sound. Its page has the
    /// rest.
    /// The Browser's Sounds: the sample library's kinds, the song's fit, its categories, the list
    /// and the picked sound.
    pub(crate) fn sounds_body(&mut self, cx: &mut Context<Self>) -> Div {
        let b = &self.browser;
        let p = &self.session.project;
        let song = match p.song_key() {
            Some(k) => format!("Fits {} · {:.0}", k.short(), p.tempo),
            None => format!("Fits {:.0} BPM", p.tempo),
        };
        let fits = self.fits_song();
        let in_key = b.play_in_key;
        let toggles = div()
            .flex()
            .gap(px(6.))
            .child(
                filter_chip(song, None, fits, None)
                    .flex_1()
                    .justify_center()
                    .debug_selector(|| "fits-song".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.set_fits_song(!fits, cx)),
                    ),
            )
            .child(
                filter_chip("Play in key", None, in_key, None)
                    .flex_1()
                    .justify_center()
                    .debug_selector(|| "play-in-key".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.browser.play_in_key = !ws.browser.play_in_key;
                            cx.notify();
                        }),
                    ),
            );
        let chips: Vec<Div> = self
            .category_chips(cx)
            .into_iter()
            .map(|c| c.flex_none())
            .collect();
        let like = b
            .like
            .as_ref()
            .and_then(|(i, _)| b.sample(*i))
            .map(|s| s.name.clone());
        let shown = self.panel_hits().len();
        let list_head = div()
            .h(px(28.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(12.))
            .border_t_1()
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .text_size(px(11.))
            .text_color(rgb(TEXT_3))
            .whitespace_nowrap()
            .child(match &like {
                Some(name) => div()
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_color(rgb(ACCENT_TEXT))
                    .child(format!("Sounds like {name}")),
                None => div()
                    .flex_1()
                    .child(format!("{shown} sounds · best match first")),
            })
            .when(like.is_some(), |d| {
                d.child(
                    link("Back")
                        .debug_selector(|| "like-back".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                ws.browser.like = None;
                                cx.notify();
                            }),
                        ),
                )
            });
        let list = match shown {
            0 if like.is_none() => self.empty_results(cx).into_any_element(),
            n => uniform_list(
                "sample-panel-list",
                n,
                cx.processor(|ws, range: Range<usize>, _, cx| ws.panel_rows(range, cx)),
            )
            .track_scroll(&self.browser.panel_list)
            .flex_1()
            .into_any_element(),
        };
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .px(px(10.))
                    .pb(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(self.kind_tray(cx))
                    .child(toggles)
                    .when(!chips.is_empty(), |d| {
                        d.child(
                            div()
                                .id("panel-categories")
                                .flex()
                                .gap(px(5.))
                                .overflow_x_scroll()
                                .children(chips),
                        )
                    }),
            )
            .child(list_head)
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .flex_col()
                    .bg(rgb(LANE))
                    .child(list),
            )
            .children(self.panel_footer(cx))
    }

    /// Rows of the compact list, as the list draws them.
    pub(crate) fn panel_rows(
        &mut self,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::Stateful<Div>> {
        range
            .filter_map(|pos| {
                let hit = self.panel_hits().get(pos)?.clone();
                Some(self.panel_row(pos, &hit, cx))
            })
            .collect()
    }

    /// A row of the compact list: its outline, name and what it is, its key or note, and its
    /// tempo. A click hears it, a drag puts it on a track, a double-click adds it to the song.
    pub(crate) fn panel_row(
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
        let heard = self.heard(index);
        let semis = self.sample_transpose(index).round();
        let mut what = match (a.kind, a.bars) {
            (Kind::Loop, Some(bars)) => {
                let n = match (bars - bars.round()).abs() < 0.05 {
                    true => format!("{bars:.0}"),
                    false => format!("{bars:.1}"),
                };
                let s = if n == "1" { "" } else { "s" };
                format!("{} loop · {n} bar{s}", a.category.name())
            }
            (Kind::Loop, None) => {
                format!("{} loop · {}", a.category.name(), seconds_text(a.seconds))
            }
            (Kind::OneShot, _) => format!("{} · {}", a.category.name(), seconds_text(a.seconds)),
        };
        if let Some(t) = a.traits.first() {
            what = format!("{what} · {t}");
        }
        // Its key, short, or its note.
        let tone = a
            .key
            .as_ref()
            .map(|k| {
                let mode = if k.minor { "min" } else { "maj" };
                format!("{} {mode}", NOTES[k.tonic as usize % 12])
            })
            .or_else(|| a.pitch.map(|p| note_name(p.note)));
        let group = format!("panel-row-{pos}");
        let give = self.agent_button(&s.path, &group, cx);
        div()
            .id(("panel-row", pos))
            .group(group)
            .w_full()
            .h(px(PANEL_ROW_H))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(9.))
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
            .debug_selector(move || format!("panel-row-{pos}"))
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
                    .w(px(52.))
                    .h(px(26.))
                    .flex_none()
                    .relative()
                    .child(wave(
                        a.waveform.clone(),
                        color,
                        if selected || heard.is_some() {
                            1.0
                        } else {
                            0.7
                        },
                    ))
                    .when_some(heard, |d, f| {
                        d.child(
                            div()
                                .absolute()
                                .top_0()
                                .left(relative(f))
                                .w(px(1.5))
                                .h_full()
                                .bg(rgb(TEXT)),
                        )
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(12.))
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
                            .gap(px(5.))
                            .min_w(px(0.))
                            .child(
                                div()
                                    .size(px(6.))
                                    .flex_none()
                                    .rounded(px(3.))
                                    .bg(rgb(color)),
                            )
                            .child(
                                div()
                                    .min_w(px(0.))
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_3))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(what),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(2.))
                    .font_family(MONO)
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(if tone.is_some() { TEXT_2 } else { TEXT_4 }))
                            .child(tone.unwrap_or_else(|| "—".into())),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(5.))
                            .text_size(px(10.))
                            .when(semis != 0.0, |d| {
                                d.child(div().text_color(rgb(ACCENT_TEXT)).child(semis_text(semis)))
                            })
                            .when_some(a.bpm, |d, bpm| {
                                d.child(
                                    div().text_color(rgb(TEXT_3)).child(format!("{bpm:.0} BPM")),
                                )
                            }),
                    ),
            )
            .child(give)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    if e.click_count == 2 {
                        ws.add_sample(index, cx);
                        return;
                    }
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

    /// Under the compact list: the picked sound, to hear, add to the song, or find more like.
    pub(crate) fn panel_footer(&self, cx: &mut Context<Self>) -> Option<Div> {
        let b = &self.browser;
        let index = b.selected?;
        let s = b.sample(index)?;
        let a = &s.analysis;
        let color = category_color(a.category);
        let heard = self.heard(index);
        let liking = b.like.as_ref().is_some_and(|l| l.0 == index);
        Some(
            div()
                .flex_none()
                .p(px(10.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .border_t_1()
                .border_color(rgb(LINE))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(chip(a.category.name(), color))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(s.name.clone()),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_3))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(search::describe(s)),
                )
                .child(
                    div()
                        .h(px(34.))
                        .relative()
                        .cursor_pointer()
                        .debug_selector(|| "panel-wave".into())
                        .child(wave(a.waveform.clone(), color, 0.9))
                        .when_some(heard, |d, f| {
                            d.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left(relative(f))
                                    .w(px(1.5))
                                    .h_full()
                                    .bg(rgb(TEXT)),
                            )
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| ws.audition_sample(index, cx)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(6.))
                        .child(
                            primary("Add to song")
                                .flex_1()
                                .child(icon(Icon::Plus, 14.).text_color(rgb(0xffffff)))
                                .flex_row_reverse()
                                .debug_selector(|| "panel-add".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| ws.add_sample(index, cx)),
                                ),
                        )
                        .child(
                            button("More like it", liking)
                                .h(px(30.))
                                .debug_selector(|| "panel-like".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| ws.like_sample(index, cx)),
                                ),
                        )
                        .child(self.give_button(s.path.clone(), 30., cx)),
                ),
        )
    }
}
