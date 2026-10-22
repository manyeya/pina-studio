//! The Songs page: every song, and a new song's settings.

use super::*;

impl Workspace {
    /// The Songs page: a new song on the left (its name, tempo, key and time; a template's click
    /// makes it), every song on the right, newest first, to search, open, pick several of
    /// (⌘-click) and move to the Trash. With no song open, it's the whole window.
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
        let (search, picks) = match self.home.as_ref() {
            Some(h) => (
                h.search.read(cx).text().trim().to_lowercase(),
                h.picks.clone(),
            ),
            None => (String::new(), Vec::new()),
        };
        let songs: Vec<Song> = self
            .songs
            .iter()
            .filter(|s| search.is_empty() || s.name.to_lowercase().contains(&search))
            .cloned()
            .collect();
        let shown = songs.len();
        let cards: Vec<_> = songs
            .into_iter()
            .map(|s| {
                let here = s.dir == current;
                let picked = picks.contains(&s.dir);
                let field = renaming
                    .as_ref()
                    .filter(|(d, _)| *d == s.dir)
                    .map(|(_, f)| f.clone());
                song_card(s, here, picked, field, cx)
            })
            .collect();
        let empty = match (self.songs.is_empty(), shown) {
            (true, _) => Some("No songs yet. Pick how to start one, on the left.".to_string()),
            (false, 0) => Some(format!("No songs called \u{201c}{search}\u{201d}")),
            _ => None,
        };
        let picked_bar = (!picks.is_empty()).then(|| {
            div()
                .h(px(40.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(10.))
                .rounded(px(10.))
                .bg(alpha(ACCENT, 0.12))
                .border_1()
                .border_color(alpha(ACCENT, 0.4))
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(match picks.len() {
                            1 => "1 song picked".to_string(),
                            n => format!("{n} songs picked"),
                        }),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(TEXT_3))
                        .child("⌘-click to pick more · Delete to trash them"),
                )
                .child(div().flex_1())
                .child(
                    button("Move to Trash", false)
                        .text_color(rgb(RECORD))
                        .debug_selector(|| "songs-trash-picked".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, window, cx| {
                                ws.song_action(MenuAction::TrashPicked, window, cx)
                            }),
                        ),
                )
                .child(
                    button("Done", false)
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
        let new_song = self.new_song_column(open, cx);
        let search_field = self.home.as_ref().map(|h| {
            div()
                .w(px(240.))
                .h(px(34.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(7.))
                .rounded(px(9.))
                .bg(rgb(FIELD))
                .border_1()
                .border_color(rgb(LINE))
                .text_size(px(13.))
                .debug_selector(|| "songs-search".into())
                .child(icon(Icon::Search, 14.).text_color(rgb(TEXT_3)))
                .child(div().flex_1().child(h.search.clone()))
        });
        div()
            .id("songs-page")
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .bg(rgb(BG))
            .child(new_song)
            .child(
                div()
                    .id("songs-list")
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .px(px(32.))
                            .py(px(28.))
                            .flex()
                            .flex_col()
                            .gap(px(18.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(14.))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(3.))
                                            .child(
                                                div()
                                                    .text_size(px(22.))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child("Songs"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .text_color(rgb(TEXT_3))
                                                    .child(format!(
                                                        "{} in {}",
                                                        self.songs.len(),
                                                        root().display()
                                                    )),
                                            ),
                                    )
                                    .child(div().flex_1())
                                    .children(search_field),
                            )
                            .children(picked_bar)
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(18.))
                                    .children(cards)
                                    .children(empty.map(|e| {
                                        div()
                                            .py(px(40.))
                                            .text_size(px(13.))
                                            .text_color(rgb(TEXT_3))
                                            .child(e)
                                    })),
                            ),
                    ),
            )
    }

    /// The Songs page's left: back to the song open, if there is one; a new song's name,
    /// tempo, key and time, and the templates (a click makes it and opens it); a song from a
    /// folder anywhere; a song to study.
    pub(crate) fn new_song_column(&self, open: Option<String>, cx: &mut Context<Self>) -> Div {
        let label = |t: &'static str| section_title(t).mb(px(-2.));
        let Some(h) = self.home.as_ref() else {
            return div().w(px(360.));
        };
        let (key, signature) = (h.key, h.signature);
        let field = |f: &Entity<TextInput>, sel: &'static str| {
            div()
                .h(px(36.))
                .px(px(11.))
                .flex()
                .items_center()
                .rounded(px(9.))
                .bg(rgb(FIELD))
                .border_1()
                .border_color(rgb(LINE_STRONG))
                .text_size(px(13.5))
                .debug_selector(move || sel.into())
                .child(f.clone())
        };
        let signatures = tray().children([[4, 4], [3, 4], [6, 8], [7, 8]].map(|sig| {
            seg(format!("{}/{}", sig[0], sig[1]), signature == sig, 28.).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    if let Some(h) = ws.home.as_mut() {
                        h.signature = sig;
                    }
                    cx.notify();
                }),
            )
        }));
        let key_pick = dropdown(key.map_or("No key".to_string(), |k| k.name()), 36.)
            .flex_1()
            .rounded(px(9.))
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
        let templates = TEMPLATES.map(|t| {
            let none = |_: &Path| None;
            let marks: Vec<_> = t
                .tracks()
                .into_iter()
                .map(|(name, instrument)| {
                    let track = Track {
                        name: name.into(),
                        instrument,
                        ..Default::default()
                    };
                    let l = look::look(&track, &none);
                    (l.icon, family_shade(l.family, 0))
                })
                .collect();
            div()
                .id(t.name())
                .h(px(56.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(12.))
                .rounded(px(10.))
                .cursor_pointer()
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(LINE))
                .hover(|d| d.bg(rgb(RAISED)).border_color(rgb(LINE_STRONG)))
                .child(
                    div()
                        .w(px(92.))
                        .flex()
                        .gap(px(3.))
                        .children(
                            marks
                                .into_iter()
                                .map(|(i, c)| glyph(i, 15.).text_color(rgb(c))),
                        )
                        .when(t == Template::Empty, |d| {
                            d.child(icon(Icon::Plus, 15.).text_color(rgb(TEXT_3)))
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
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(t.name()),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_3))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(t.about()),
                        ),
                )
                .child(icon(Icon::Forward, 14.).text_color(rgb(TEXT_3)))
                .debug_selector(move || format!("template-{}", t.name()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, window, cx| ws.create_song(t, window, cx)),
                )
        });
        let link = |glyph: Icon, text: &'static str, sel: &'static str| {
            div()
                .h(px(34.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(9.))
                .rounded(px(8.))
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(rgb(TEXT_2))
                .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                .debug_selector(move || sel.into())
                .child(icon(glyph, 15.).text_color(rgb(TEXT_3)))
                .child(text)
        };
        div()
            .w(px(360.))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(16.))
            .px(px(24.))
            .py(px(26.))
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(24.))
                            .font_weight(FontWeight::BOLD)
                            .child("Pina Studio"),
                    )
                    .children(open.map(|name| {
                        button(format!("Back to {name}"), false)
                            .max_w(px(200.))
                            .overflow_hidden()
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
            .child(label("NEW SONG"))
            .child(field(&h.name, "new-song-name"))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        div()
                            .w(px(92.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(div().flex_1().child(field(&h.tempo, "new-song-tempo")))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .child("BPM"),
                            ),
                    )
                    .child(key_pick),
            )
            .child(div().flex().child(signatures))
            .child(label("START WITH"))
            .child(div().flex().flex_col().gap(px(8.)).children(templates))
            .child(div().flex_1())
            .child(div().h(px(1.)).bg(rgb(LINE)))
            .child(
                link(
                    Icon::Folder,
                    "Open a song from a folder…",
                    "songs-open-folder",
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, _, cx| ws.open_folder(cx)),
                ),
            )
            .child(
                link(Icon::Samples, "Study a song…", "songs-study").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, _, cx| ws.open_study(cx)),
                ),
            )
    }
}
