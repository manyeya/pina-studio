//! A song as a card or a row on the Songs page.

use super::*;

/// A song's cover: its arrangement in miniature, each track a lane of its colour.
pub(crate) fn cover(lanes: &[(u32, Vec<(f32, f32)>)], w: f32, h: f32) -> Div {
    let rows = lanes.len().clamp(1, 14);
    let lane_h = ((h - 16.0) / rows as f32).min(9.0);
    div()
        .w(px(w))
        .h(px(h))
        .relative()
        .overflow_hidden()
        .rounded(px(10.))
        .bg(rgb(0x101015))
        .children(
            lanes
                .iter()
                .take(rows)
                .enumerate()
                .flat_map(|(i, (color, spans))| {
                    let top = 8.0 + i as f32 * lane_h;
                    let color = *color;
                    spans
                        .iter()
                        .map(move |(a, b)| {
                            div()
                                .absolute()
                                .top(px(top + 1.0))
                                .left(px(8.0 + a * (w - 16.0)))
                                .w(px(((b - a) * (w - 16.0) - 1.0).max(2.0)))
                                .h(px((lane_h - 2.0).max(2.0)))
                                .rounded(px(2.))
                                .bg(alpha(color, 0.78))
                        })
                        .collect::<Vec<_>>()
                }),
        )
}

/// What can be done to a song, from its card's menu.
pub(crate) fn card_menu(dir: &Path, at: gpui::Point<gpui::Pixels>) -> Menu {
    let d = dir.to_path_buf();
    Menu {
        at,
        items: vec![
            ("Open".into(), MenuAction::OpenSong(d.clone())),
            ("Rename…".into(), MenuAction::RenameSong(d.clone())),
            ("Duplicate".into(), MenuAction::DuplicateSong(d.clone())),
            ("Show in Finder".into(), MenuAction::RevealSong(d.clone())),
            ("Move to Trash…".into(), MenuAction::TrashSong(d)),
        ],
        ..Default::default()
    }
}

/// A song's card on the Songs page: a click opens it, ⌘-click picks it (to trash several); its
/// menu (⋯, or a right-click) and, on hover, the Trash.
pub(crate) fn song_card(
    s: Song,
    here: bool,
    picked: bool,
    renaming: Option<Entity<TextInput>>,
    cx: &mut Context<Workspace>,
) -> gpui::Stateful<Div> {
    let dir = s.dir.clone();
    let meta = {
        let mut parts = vec![format!("{:.0} BPM", s.tempo)];
        if let Some(k) = s.key.as_deref().and_then(Key::parse) {
            parts.push(k.short());
        }
        parts.push(format!(
            "{} track{}",
            s.tracks,
            if s.tracks == 1 { "" } else { "s" }
        ));
        if s.seconds > 0.5 {
            parts.push(length(s.seconds));
        }
        parts.join(" · ")
    };
    let (open_dir, menu_dir, right_dir, trash_dir) =
        (dir.clone(), dir.clone(), dir.clone(), dir.clone());
    let name = s.name.clone();
    let corner = |glyph: Icon, sel: String, hot: u32| {
        div()
            .size(px(26.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.))
            .text_color(rgb(TEXT_3))
            .hover(move |d| d.bg(rgb(RAISED)).text_color(rgb(hot)))
            .debug_selector(move || sel.clone())
            .child(icon(glyph, 15.))
    };
    div()
        .id(gpui::SharedString::from(format!("song-{}", s.name)))
        .group("song-card")
        .relative()
        .w(px(264.))
        .flex()
        .flex_col()
        .gap(px(10.))
        .p(px(10.))
        .rounded(px(14.))
        .bg(rgb(if picked { 0x1c2233 } else { CARD }))
        .border_1()
        .border_color(match (picked, here) {
            (true, _) => alpha(ACCENT, 0.9),
            (false, true) => alpha(ACCENT, 0.5),
            _ => rgb(LINE).into(),
        })
        .cursor_pointer()
        .when(!picked, |d| d.hover(|d| d.bg(rgb(0x1f1f25))))
        .debug_selector({
            let name = s.name.clone();
            move || format!("song-{name}")
        })
        .child(cover(&s.lanes, 244.0, 112.0))
        .when(picked, |d| {
            d.child(
                div()
                    .absolute()
                    .top(px(16.))
                    .left(px(16.))
                    .size(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.))
                    .bg(rgb(ACCENT))
                    .child(icon(Icon::Check, 13.).text_color(rgb(0xffffff))),
            )
        })
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(4.))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(match renaming {
                            Some(f) => div()
                                .h(px(24.))
                                .px(px(6.))
                                .rounded(px(6.))
                                .bg(rgb(FIELD))
                                .text_size(px(14.))
                                .child(f),
                            None => div()
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(s.name.clone()),
                        })
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(rgb(TEXT_3))
                                .child(meta),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(if here { ACCENT_TEXT } else { TEXT_3 }))
                                .child(if here {
                                    "Open now".to_string()
                                } else {
                                    when(s.modified)
                                }),
                        ),
                )
                .child(
                    corner(Icon::Trash, format!("song-trash-{name}"), RECORD)
                        .invisible()
                        .group_hover("song-card", |d| d.visible())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.song_pending = Some(MenuAction::TrashSong(trash_dir.clone()));
                                cx.notify();
                            }),
                        ),
                )
                .child(
                    corner(Icon::More, format!("song-more-{}", s.name), TEXT).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.menu = Some(card_menu(&menu_dir, e.position));
                            cx.notify();
                        }),
                    ),
                ),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                // Several picked, this among them: the Trash for them all.
                let picks = ws
                    .home
                    .as_ref()
                    .map(|h| h.picks.clone())
                    .unwrap_or_default();
                ws.menu = Some(match picks.len() > 1 && picks.contains(&right_dir) {
                    true => Menu {
                        at: e.position,
                        items: vec![(
                            format!("Move {} Songs to Trash…", picks.len()),
                            MenuAction::TrashPicked,
                        )],
                        ..Default::default()
                    },
                    false => card_menu(&right_dir, e.position),
                });
                cx.notify();
            }),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                if ws.renaming.is_some() {
                    return;
                }
                let picking = ws.home.as_ref().is_some_and(|h| !h.picks.is_empty());
                if e.modifiers.platform || e.modifiers.shift || picking {
                    if let Some(h) = ws.home.as_mut() {
                        match h.picks.iter().position(|p| *p == open_dir) {
                            Some(i) => {
                                h.picks.remove(i);
                            }
                            None => h.picks.push(open_dir.clone()),
                        }
                    }
                    cx.notify();
                    return;
                }
                ws.open_song(open_dir.clone(), window, cx);
            }),
        )
}

/// A song in the Browser's results.
pub fn song_row(s: Song, on: bool, here: bool, cx: &mut Context<Workspace>) -> Div {
    let dir = s.dir.clone();
    div()
        .flex()
        .items_center()
        .gap(px(9.))
        .h(px(40.))
        .px(px(6.))
        .rounded(px(8.))
        .cursor_pointer()
        .when(on, |d| d.bg(alpha(ACCENT, 0.16)))
        .when(!on, |d| d.hover(|d| d.bg(rgb(OFF))))
        .child(cover(&s.lanes, 44.0, 28.0))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .child(s.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .child(if here {
                            "Open now".to_string()
                        } else {
                            format!("{:.0} BPM · {}", s.tempo, when(s.modified))
                        }),
                ),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, window, cx| ws.open_song(dir.clone(), window, cx)),
        )
}
