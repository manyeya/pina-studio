//! The Songs page: a new song along the top, every song below.

use super::*;

/// A grid card's least width.
const CARD_W: f32 = 236.0;

impl Workspace {
    /// The Songs page: a new song along the top (its name, tempo, key and time; a template's
    /// click makes it), then every song, to search, filter, sort, see as a list or a grid, open,
    /// pick several of (⌘-click, or their boxes) and move to the Trash. With no song open, it's
    /// the whole window.
    pub fn songs_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.session.dir.clone();
        let open = self.session.on_disk.then(|| {
            current
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
        let renaming = self
            .renaming
            .as_ref()
            .map(|r| (r.dir.clone(), r.field.clone()));
        let (search, picks, shelving) = match self.home.as_ref() {
            Some(h) => (
                h.search.read(cx).text().trim().to_lowercase(),
                h.picks.clone(),
                h.shelving.clone(),
            ),
            None => (String::new(), Vec::new(), Shelving::default()),
        };
        let songs = shelving.shown(&self.songs, &search);
        let grid = self.settings.songs_grid;
        let rename_for = |s: &Song| {
            renaming
                .as_ref()
                .filter(|(d, _)| *d == s.dir)
                .map(|(_, f)| f.clone())
        };
        let body = if songs.is_empty() {
            self.no_songs(&search, &shelving, cx).into_any_element()
        } else if grid {
            let cols = ((self.viewport_w - 64.0 + 16.0) / (CARD_W + 16.0))
                .floor()
                .clamp(2.0, 8.0) as u16;
            let cards: Vec<_> = songs
                .iter()
                .map(|s| {
                    let field = rename_for(s);
                    song_card(
                        s.clone(),
                        s.dir == current,
                        picks.contains(&s.dir),
                        field,
                        cx,
                    )
                })
                .collect();
            div()
                .id("songs-list")
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .child(
                    div()
                        .px(px(32.))
                        .pt(px(4.))
                        .pb(px(28.))
                        .grid()
                        .grid_cols(cols)
                        .gap(px(16.))
                        .children(cards),
                )
                .into_any_element()
        } else {
            // Newest first, under when they were edited; any other order, one run.
            let mut rows: Vec<gpui::AnyElement> = Vec::new();
            let mut last = None;
            for s in &songs {
                if shelving.sort == Sort::Edited {
                    let era = era(s.modified);
                    if last != Some(era) {
                        let n = songs
                            .iter()
                            .filter(|o| super::era(o.modified) == era)
                            .count();
                        rows.push(era_heading(era, n).into_any_element());
                        last = Some(era);
                    }
                }
                let field = rename_for(s);
                rows.push(
                    song_line(
                        s.clone(),
                        s.dir == current,
                        picks.contains(&s.dir),
                        field,
                        cx,
                    )
                    .into_any_element(),
                );
            }
            div()
                .flex_1()
                .min_h(px(0.))
                .flex()
                .flex_col()
                .child(self.list_heading(shelving.sort, cx))
                .child(
                    div()
                        .id("songs-list")
                        .flex_1()
                        .min_h(px(0.))
                        .overflow_y_scroll()
                        .pb(px(20.))
                        .children(rows),
                )
                .into_any_element()
        };
        let picked_bar = (!picks.is_empty()).then(|| {
            div()
                .mx(px(32.))
                .mb(px(12.))
                .h(px(46.))
                .pl(px(16.))
                .pr(px(8.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(12.))
                .rounded(px(10.))
                .bg(alpha(ACCENT, 0.12))
                .border_1()
                .border_color(alpha(ACCENT, 0.45))
                .child(
                    div()
                        .text_size(px(13.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(match picks.len() {
                            1 => "1 song picked".to_string(),
                            n => format!("{n} songs picked"),
                        }),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(ACCENT_TEXT))
                        .child("⌘-click to pick more · Delete to trash them"),
                )
                .child(div().flex_1())
                .child(
                    pill(Some(Icon::Trash), "Move to Trash", 32.)
                        .danger()
                        .debug_selector(|| "songs-trash-picked".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, window, cx| {
                                ws.song_action(MenuAction::TrashPicked, window, cx)
                            }),
                        ),
                )
                .child(
                    pill(None, "Done", 32.)
                        .calm()
                        .debug_selector(|| "songs-unpick".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                if let Some(h) = ws.home.as_mut() {
                                    h.picks.clear();
                                }
                                cx.notify();
                            }),
                        ),
                )
        });
        let shown = songs.len();
        div()
            .id("songs-page")
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .child(self.new_song_band(open, cx))
            .child(self.songs_bar(shown, &search, &shelving, grid, cx))
            .children(picked_bar)
            .child(body)
    }

    /// Along the top: a new song's name, tempo, key and time, a song from a folder, a song to
    /// study, back to the song open; then the templates, a click making the song and opening it.
    pub(crate) fn new_song_band(&self, open: Option<String>, cx: &mut Context<Self>) -> Div {
        let band = div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(18.))
            .px(px(32.))
            .pt(px(22.))
            .pb(px(24.))
            .bg(rgb(0x111115))
            .border_b_1()
            .border_color(rgb(LINE));
        let Some(h) = self.home.as_ref() else {
            return band;
        };
        let (key, signature) = (h.key, h.signature);
        let field = |f: &Entity<TextInput>, label: &'static str, sel: &'static str, w: f32| {
            div()
                .w(px(w))
                .h(px(38.))
                .px(px(12.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(8.))
                .rounded(px(10.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(0x3a3a44))
                .text_size(px(13.5))
                .debug_selector(move || sel.into())
                .when(label == "Name", |d| {
                    d.child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_3))
                            .child(label),
                    )
                })
                .child(div().flex_1().min_w(px(0.)).child(f.clone()))
                .when(label == "BPM", |d| {
                    d.child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child(label),
                    )
                })
        };
        let signatures = div()
            .h(px(38.))
            .p(px(3.))
            .flex()
            .flex_none()
            .gap(px(3.))
            .rounded(px(10.))
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(0x3a3a44))
            .children([[4, 4], [3, 4], [6, 8], [7, 8]].map(|sig| {
                let on = signature == sig;
                div()
                    .w(px(42.))
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.))
                    .font_family(MONO)
                    .text_size(px(12.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .map(|d| match on {
                        true => d.bg(rgb(LINE_STRONG)).text_color(rgb(TEXT)),
                        false => d
                            .text_color(rgb(0x9a9aa6))
                            .hover(|d| d.bg(rgb(OFF)).text_color(rgb(TEXT_2))),
                    })
                    .child(format!("{}/{}", sig[0], sig[1]))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            if let Some(h) = ws.home.as_mut() {
                                h.signature = sig;
                            }
                            cx.notify();
                        }),
                    )
            }));
        let key_pick = div()
            .w(px(140.))
            .h(px(38.))
            .px(px(12.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .rounded(px(10.))
            .bg(rgb(FIELD))
            .border_1()
            .border_color(rgb(0x3a3a44))
            .cursor_pointer()
            .hover(|d| d.bg(rgb(RAISED)).border_color(rgb(0x4a4a55)))
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT_3))
                    .child("Key"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .text_size(px(13.))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(key.map_or("None".to_string(), |k| k.short())),
            )
            .child(icon(Icon::Chevron, 12.).text_color(rgb(TEXT_3)))
            .debug_selector(|| "new-song-key".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    let mut items = vec![("No key".to_string(), MenuAction::NewSongKey(None))];
                    items.extend(
                        Key::all().map(|k| (k.name(), MenuAction::NewSongKey(Some(k.name())))),
                    );
                    ws.menu = Some(Menu {
                        at: e.position,
                        items,
                        columns: 2,
                        lead: 1,
                        ..Default::default()
                    });
                    cx.notify();
                }),
            );
        let tiles = TEMPLATES.map(|t| template_tile(t, cx));
        band.child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(12.))
                .child(
                    div()
                        .mr(px(8.))
                        .text_size(px(22.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("New song"),
                )
                .child(field(&h.name, "Name", "new-song-name", 230.))
                .child(field(&h.tempo, "BPM", "new-song-tempo", 112.))
                .child(key_pick)
                .child(signatures)
                .child(div().flex_1())
                .child(
                    pill(Some(Icon::Folder), "Open folder…", 38.)
                        .calm()
                        .debug_selector(|| "songs-open-folder".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.open_folder(cx)),
                        ),
                )
                .child(
                    pill(Some(Icon::Wave), "Study a song…", 38.)
                        .calm()
                        .debug_selector(|| "songs-study".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.open_study(cx)),
                        ),
                )
                .children(open.map(|name| {
                    div()
                        .h(px(38.))
                        .max_w(px(280.))
                        .px(px(16.))
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(8.))
                        .rounded(px(10.))
                        .bg(rgb(ACCENT))
                        .text_color(rgb(0xffffff))
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(tint(ACCENT, 0.12))))
                        .child(icon(Icon::ArrowLeft, 15.).text_color(rgb(0xffffff)))
                        .child(
                            div()
                                .min_w(px(0.))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(format!("Back to {name}")),
                        )
                        .debug_selector(|| "songs-back".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                ws.view = View::Arrange;
                                cx.notify();
                            }),
                        )
                })),
        )
        .child(div().flex().gap(px(14.)).children(tiles))
    }

    /// Over the songs: how many, where they're kept, the search, the filters, the order, and
    /// list or grid.
    fn songs_bar(
        &self,
        shown: usize,
        search: &str,
        shelving: &Shelving,
        grid: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let all = self.songs.len();
        let count = match search.is_empty() && !shelving.filtered() {
            true => all.to_string(),
            false => format!("{shown} of {all}"),
        };
        let home = std::env::var("HOME").unwrap_or_default();
        let place = root().display().to_string();
        let place = match !home.is_empty() && place.starts_with(&home) {
            true => format!("~{}", &place[home.len()..]),
            false => place,
        };
        let search_field = self.home.as_ref().map(|h| {
            div()
                .w(px(280.))
                .h(px(36.))
                .px(px(12.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(9.))
                .rounded(px(10.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(0x3a3a44))
                .text_size(px(13.5))
                .debug_selector(|| "songs-search".into())
                .child(icon(Icon::Search, 16.).text_color(rgb(0xb4b4be)))
                .child(div().flex_1().min_w(px(0.)).child(h.search.clone()))
        });
        // The keys and times the songs are in, to filter by.
        let mut keys: Vec<String> = self.songs.iter().filter_map(|s| s.key.clone()).collect();
        keys.sort_by_key(|k| Key::parse(k).map(|k| k.name()));
        keys.dedup();
        let mut times: Vec<[u64; 2]> = vec![[4, 4], [3, 4], [6, 8], [7, 8]];
        for s in &self.songs {
            if !times.contains(&s.signature) {
                times.push(s.signature);
            }
        }
        let tick = |on: bool, label: String| match on {
            true => format!("✓ {label}"),
            false => label,
        };
        let pace_items: Vec<_> = Pace::ALL
            .iter()
            .map(|&p| {
                let label = if p == Pace::Any {
                    "Any tempo"
                } else {
                    p.name()
                };
                (
                    tick(shelving.pace == p, label.to_string()),
                    MenuAction::Songs(Shelf::Pace(p)),
                )
            })
            .collect();
        let mut key_items = vec![(
            tick(shelving.key.is_none(), "Any key".into()),
            MenuAction::Songs(Shelf::Key(None)),
        )];
        key_items.extend(keys.into_iter().map(|k| {
            let label = Key::parse(&k).map_or(k.clone(), |k| k.short());
            (
                tick(shelving.key.as_ref() == Some(&k), label),
                MenuAction::Songs(Shelf::Key(Some(k))),
            )
        }));
        let mut time_items = vec![(
            tick(shelving.time.is_none(), "Any time".into()),
            MenuAction::Songs(Shelf::Time(None)),
        )];
        time_items.extend(times.into_iter().map(|t| {
            (
                tick(shelving.time == Some(t), format!("{}/{}", t[0], t[1])),
                MenuAction::Songs(Shelf::Time(Some(t))),
            )
        }));
        let sort_items: Vec<_> = Sort::ALL
            .iter()
            .map(|&s| {
                (
                    tick(shelving.sort == s, s.name().to_string()),
                    MenuAction::Songs(Shelf::Sort(s)),
                )
            })
            .collect();
        let key_value = shelving.key.as_deref().map_or("Any".to_string(), |k| {
            Key::parse(k).map_or(k.into(), |k| k.short())
        });
        let time_value = shelving
            .time
            .map_or("Any".to_string(), |t| format!("{}/{}", t[0], t[1]));
        let menu_of = |items: Vec<(String, MenuAction)>| {
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                ws.menu = Some(Menu {
                    at: e.position,
                    items: items.clone(),
                    ..Default::default()
                });
                cx.notify();
            })
        };
        let view = |glyph: Icon, on: bool, sel: &'static str| {
            div()
                .size(px(30.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.))
                .cursor_pointer()
                .map(|d| match on {
                    true => d.bg(rgb(LINE_STRONG)),
                    false => d.hover(|d| d.bg(rgb(OFF))),
                })
                .debug_selector(move || sel.into())
                .child(icon(glyph, 16.).text_color(rgb(if on { TEXT } else { TEXT_3 })))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.settings.songs_grid = glyph == Icon::Grid;
                        ws.settings.save();
                        cx.notify();
                    }),
                )
        };
        div()
            .flex_none()
            .px(px(32.))
            .pt(px(18.))
            .pb(px(14.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Songs"),
                    )
                    .child(
                        div()
                            .h(px(22.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .rounded(px(11.))
                            .bg(rgb(FIELD))
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT_2))
                            .debug_selector(|| "songs-count".into())
                            .child(count),
                    ),
            )
            .child(
                div()
                    .mr(px(8.))
                    .font_family(MONO)
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_3))
                    .whitespace_nowrap()
                    .child(place),
            )
            .child(div().flex_1())
            .children(search_field)
            .child(
                filter_chip(
                    "Tempo",
                    shelving.pace.name().into(),
                    shelving.pace != Pace::Any,
                )
                .debug_selector(|| "songs-filter-tempo".into())
                .on_mouse_down(MouseButton::Left, menu_of(pace_items)),
            )
            .child(
                filter_chip("Key", key_value, shelving.key.is_some())
                    .debug_selector(|| "songs-filter-key".into())
                    .on_mouse_down(MouseButton::Left, menu_of(key_items)),
            )
            .child(
                filter_chip("Time", time_value, shelving.time.is_some())
                    .debug_selector(|| "songs-filter-time".into())
                    .on_mouse_down(MouseButton::Left, menu_of(time_items)),
            )
            .when(shelving.filtered(), |d| {
                d.child(
                    div()
                        .h(px(34.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .text_size(px(12.5))
                        .text_color(rgb(ACCENT_TEXT))
                        .cursor_pointer()
                        .hover(|d| d.text_color(rgb(TEXT)))
                        .debug_selector(|| "songs-filter-clear".into())
                        .child("Clear")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                if let Some(h) = ws.home.as_mut() {
                                    h.shelving.change(Shelf::Clear);
                                }
                                cx.notify();
                            }),
                        ),
                )
            })
            .child(
                filter_chip("Sort", shelving.sort.name().into(), false)
                    .debug_selector(|| "songs-sort".into())
                    .on_mouse_down(MouseButton::Left, menu_of(sort_items)),
            )
            .child(
                div()
                    .h(px(36.))
                    .p(px(2.))
                    .flex()
                    .flex_none()
                    .gap(px(2.))
                    .rounded(px(10.))
                    .bg(rgb(SURFACE))
                    .border_1()
                    .border_color(rgb(LINE_STRONG))
                    .child(view(Icon::List, !grid, "songs-view-list"))
                    .child(view(Icon::Grid, grid, "songs-view-grid")),
            )
    }

    /// The list's heading: what each column is; a click on Name, Tempo, Length or Edited sorts
    /// by it.
    fn list_heading(&self, sort: Sort, cx: &mut Context<Self>) -> Div {
        let head = |label: &'static str, w: Option<f32>, right: bool, by: Option<Sort>| {
            let on = by == Some(sort);
            div()
                .map(|d| match w {
                    Some(w) => d.w(px(w)).flex_none(),
                    None => d.flex_1().min_w(px(0.)),
                })
                .flex()
                .items_center()
                .gap(px(4.))
                .when(right, |d| d.justify_end())
                .text_color(rgb(if on { TEXT } else { TEXT_3 }))
                .when(by.is_some(), |d| {
                    d.cursor_pointer().hover(|d| d.text_color(rgb(TEXT_2)))
                })
                .child(label)
                .when(on, |d| {
                    d.child(icon(Icon::Chevron, 10.).text_color(rgb(TEXT)))
                })
                .when_some(by, |d, by| {
                    d.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            if let Some(h) = ws.home.as_mut() {
                                h.shelving.change(Shelf::Sort(by));
                            }
                            cx.notify();
                        }),
                    )
                })
        };
        div()
            .flex_none()
            .h(px(32.))
            .px(px(32.))
            .flex()
            .items_center()
            .gap(px(COL_GAP))
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(div().w(px(COL_PICK + COL_GAP + COL_COVER)).flex_none())
            .child(head("NAME", None, false, Some(Sort::Name)))
            .child(head("TEMPO", Some(COL_TEMPO), true, Some(Sort::Tempo)))
            .child(head("KEY", Some(COL_KEY), false, None))
            .child(head("TIME", Some(COL_TIME), false, None))
            .child(head("TRACKS", Some(COL_TRACKS), true, None))
            .child(head("LENGTH", Some(COL_LENGTH), true, Some(Sort::Length)))
            .child(head("EDITED", Some(COL_EDITED), false, Some(Sort::Edited)))
            .child(div().w(px(COL_ACTS)).flex_none())
    }

    /// No songs to show: none yet, none by that name, or none the filters let through.
    fn no_songs(&self, search: &str, shelving: &Shelving, cx: &mut Context<Self>) -> Div {
        let (title, line) = match (self.songs.is_empty(), search.is_empty()) {
            (true, _) => (
                "No songs yet".to_string(),
                "Pick a template above to start one.",
            ),
            (false, false) => (
                format!("No songs called \u{201c}{search}\u{201d}"),
                "Esc clears the search.",
            ),
            (false, true) => (
                "No songs match these filters".to_string(),
                "Clear them to see every song.",
            ),
        };
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .pt(px(64.))
            .gap(px(8.))
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT_2))
                    .child(title),
            )
            .child(div().text_size(px(13.)).text_color(rgb(TEXT_3)).child(line))
            .when(!self.songs.is_empty() && shelving.filtered(), |d| {
                d.child(
                    pill(None, "Clear filters", 34.)
                        .calm()
                        .mt(px(8.))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                if let Some(h) = ws.home.as_mut() {
                                    h.shelving.change(Shelf::Clear);
                                }
                                cx.notify();
                            }),
                        ),
                )
            })
    }
}

/// A bordered button with an icon: Open folder…, Study a song…, Done.
fn pill(glyph: Option<Icon>, label: &'static str, h: f32) -> Div {
    div()
        .h(px(h))
        .px(px(14.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(8.))
        .rounded(px(10.))
        .bg(rgb(OFF))
        .border_1()
        .border_color(rgb(LINE_STRONG))
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(TEXT))
        .whitespace_nowrap()
        .cursor_pointer()
        .children(glyph.map(|g| icon(g, 15.).text_color(rgb(TEXT_2))))
        .child(label)
}

/// A pill's look, hovered or warning: gpui takes one hover style per element, so it's added last.
trait PillHover {
    fn calm(self) -> Self;
    fn danger(self) -> Self;
}

impl PillHover for Div {
    fn calm(self) -> Self {
        self.hover(|d| d.bg(rgb(RAISED)).border_color(rgb(0x45454f)))
    }

    fn danger(self) -> Self {
        self.text_color(rgb(0xff7070))
            .border_color(alpha(RECORD, 0.4))
            .hover(|d| d.bg(alpha(RECORD, 0.14)).border_color(alpha(RECORD, 0.6)))
    }
}

/// A filter (or the order): what it is, what it's set to, and a chevron; tinted while set.
fn filter_chip(label: &'static str, value: String, on: bool) -> Div {
    div()
        .h(px(36.))
        .pl(px(12.))
        .pr(px(10.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .rounded(px(10.))
        .border_1()
        .text_size(px(12.5))
        .whitespace_nowrap()
        .cursor_pointer()
        .map(|d| match on {
            true => d.bg(alpha(ACCENT, 0.1)).border_color(alpha(ACCENT, 0.6)),
            false => d
                .bg(rgb(SURFACE))
                .border_color(rgb(LINE_STRONG))
                .hover(|d| d.border_color(rgb(0x4a4a55))),
        })
        .child(div().text_color(rgb(TEXT_3)).child(label))
        .child(
            div()
                .text_color(rgb(if on { ACCENT_TEXT } else { TEXT_2 }))
                .font_weight(FontWeight::MEDIUM)
                .child(value),
        )
        .child(icon(Icon::Chevron, 11.).text_color(rgb(TEXT_3)))
}

/// A heading in the list: Today, This week, This month, Earlier, and how many.
fn era_heading(era: &'static str, n: usize) -> Div {
    div()
        .h(px(38.))
        .px(px(32.))
        .pb(px(7.))
        .flex()
        .items_end()
        .gap(px(8.))
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT_2))
        .child(era)
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(TEXT_3))
                .child(n.to_string()),
        )
}

/// A template as a tile: its tracks drawn as lanes, named in their colours; a click makes a song
/// from it and opens it.
fn template_tile(t: Template, cx: &mut Context<Workspace>) -> gpui::Stateful<Div> {
    let none = |_: &Path| None;
    let lanes: Vec<(String, u32)> = t
        .tracks()
        .into_iter()
        .map(|(name, instrument)| {
            let track = Track {
                name: name.into(),
                instrument,
                ..Default::default()
            };
            let l = look::look(&track, &none);
            (name.to_uppercase(), family_shade(l.family, 0))
        })
        .collect();
    let empty = lanes.is_empty();
    // Where each lane's part sits along it: staggered, as an arrangement would have them.
    const AT: [(f32, f32); 5] = [
        (0.0, 1.0),
        (0.25, 0.75),
        (0.12, 0.88),
        (0.5, 0.5),
        (0.38, 0.62),
    ];
    let preview = div()
        .h(px(88.))
        .px(px(10.))
        .py(px(9.))
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(5.))
        .rounded(px(9.))
        .bg(rgb(0x0a0a0d))
        .border_1()
        .border_color(rgb(0x1f1f26))
        .children(lanes.into_iter().enumerate().map(|(i, (name, color))| {
            let (x, w) = AT[i % AT.len()];
            div()
                .h(px(10.))
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .w(px(46.))
                        .flex_none()
                        .text_size(px(9.5))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(color))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .child(name),
                )
                .child(
                    div().flex_1().h(px(6.)).relative().child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(gpui::relative(x))
                            .w(gpui::relative(w))
                            .rounded(px(3.))
                            .bg(alpha(color, 0.85)),
                    ),
                )
        }))
        .when(empty, |d| {
            d.items_center()
                .gap(px(6.))
                .child(icon(Icon::Plus, 22.).text_color(rgb(0x9a9aa6)))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_3))
                        .child("A blank arrangement"),
                )
        });
    let create = div()
        .h(px(28.))
        .px(px(10.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(5.))
        .rounded(px(8.))
        .bg(rgb(FIELD))
        .border_1()
        .border_color(rgb(0x3a3a44))
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(0xe4e4ea))
        .group_hover("template", |d| {
            d.bg(rgb(ACCENT))
                .border_color(rgb(ACCENT))
                .text_color(rgb(0xffffff))
        })
        .child("Create");
    div()
        .id(t.name())
        .group("template")
        .flex_1()
        .min_w(px(0.))
        .flex()
        .flex_col()
        .gap(px(12.))
        .p(px(10.))
        .pb(px(12.))
        .rounded(px(13.))
        .bg(rgb(0x17171c))
        .border_1()
        .border_color(rgb(if empty { 0x3a3a44 } else { RAISED }))
        .when(empty, |d| d.border_dashed())
        .cursor_pointer()
        .hover(|d| d.bg(rgb(0x1d1d23)).border_color(rgb(0x4a4a55)))
        .child(preview)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(t.name()),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(rgb(0x9a9aa6))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(t.about()),
                        ),
                )
                .child(create),
        )
        .debug_selector(move || format!("template-{}", t.name()))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, window, cx| ws.create_song(t, window, cx)),
        )
}
