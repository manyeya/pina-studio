//! A song as a row or a card on the Songs page, or a row in the Browser.

use super::*;
use gpui::{Bounds, Stateful, canvas, fill, point, size};

/// The list's columns, shared by its heading and its rows.
pub(crate) const COL_PICK: f32 = 20.0;
pub(crate) const COL_COVER: f32 = 84.0;
pub(crate) const COL_TEMPO: f32 = 64.0;
pub(crate) const COL_KEY: f32 = 72.0;
pub(crate) const COL_TIME: f32 = 44.0;
pub(crate) const COL_TRACKS: f32 = 56.0;
pub(crate) const COL_LENGTH: f32 = 56.0;
pub(crate) const COL_EDITED: f32 = 96.0;
pub(crate) const COL_ACTS: f32 = 66.0;
pub(crate) const COL_GAP: f32 = 14.0;

/// A song's cover: its arrangement in miniature, each track a lane of its colour sharing the
/// whole height. Painted in one go, so a hundred songs cost a hundred elements, not thousands.
/// `h` tall; its width is the caller's.
pub(crate) fn cover(lanes: &[(u32, Vec<(f32, f32)>)], h: f32) -> Div {
    let lanes: Vec<_> = lanes.iter().take(14).cloned().collect();
    div()
        .h(px(h))
        .flex_none()
        .overflow_hidden()
        .rounded(px((h * 0.08).clamp(4.0, 10.0)))
        .bg(rgb(0x0a0a0d))
        .border_1()
        .border_color(rgb(0x1f1f26))
        .child(
            canvas(
                |_, _, _| {},
                move |b, _, window, _| {
                    let (x0, y0) = (f32::from(b.origin.x), f32::from(b.origin.y));
                    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                    let pad = (h * 0.09).clamp(3.0, 10.0);
                    let lane = (h - pad * 2.0) / lanes.len().max(1) as f32;
                    let span = w - pad * 2.0;
                    let radius = px((lane * 0.15).clamp(1.0, 3.0));
                    for (i, (color, spans)) in lanes.iter().enumerate() {
                        let y = y0 + pad + i as f32 * lane + lane * 0.12;
                        for (a, z) in spans {
                            let at = point(px(x0 + pad + a * span), px(y));
                            let sz = size(
                                px(((z - a) * span - 1.5).max(2.0)),
                                px((lane * 0.76).max(2.0)),
                            );
                            window.paint_quad(
                                fill(Bounds::new(at, sz), alpha(*color, 0.9)).corner_radii(radius),
                            );
                        }
                    }
                },
            )
            .size_full(),
        )
}

/// What can be done to a song, from its menu.
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

/// BPM · key · tracks · length.
fn meta(s: &Song) -> String {
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
}

/// Picks a song, or lets it go.
fn toggle_pick(ws: &mut Workspace, dir: &Path) {
    if let Some(h) = ws.home.as_mut() {
        match h.picks.iter().position(|p| p == dir) {
            Some(i) => {
                h.picks.remove(i);
            }
            None => h.picks.push(dir.to_path_buf()),
        }
    }
}

/// A row's or a card's clicks: a click opens it (or, while picking, picks it), ⌘- or ⇧-click
/// picks it, a right-click its menu (or, several picked and it among them, the Trash for all).
fn song_clicks(d: Stateful<Div>, dir: PathBuf, cx: &mut Context<Workspace>) -> Stateful<Div> {
    let right = dir.clone();
    d.on_mouse_down(
        MouseButton::Right,
        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            let picks = ws
                .home
                .as_ref()
                .map(|h| h.picks.clone())
                .unwrap_or_default();
            ws.menu = Some(match picks.len() > 1 && picks.contains(&right) {
                true => Menu {
                    at: e.position,
                    items: vec![(
                        format!("Move {} Songs to Trash…", picks.len()),
                        MenuAction::TrashPicked,
                    )],
                    ..Default::default()
                },
                false => card_menu(&right, e.position),
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
                toggle_pick(ws, &dir);
                cx.notify();
                return;
            }
            ws.open_song(dir.clone(), window, cx);
        }),
    )
}

/// The box that picks a song, always there: ticked in the accent once picked.
fn pick_box(picked: bool, s: &Song, cx: &mut Context<Workspace>) -> Div {
    let dir = s.dir.clone();
    let sel = format!("song-pick-{}", s.name);
    div()
        .size(px(20.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .border_1()
        .cursor_pointer()
        .map(|d| match picked {
            true => d.bg(rgb(ACCENT)).border_color(rgb(ACCENT)),
            false => d.border_color(rgb(0x4a4a55)),
        })
        .when(!picked, |d| d.hover(|d| d.border_color(rgb(TEXT_3))))
        .when(picked, |d| {
            d.child(icon(Icon::Check, 13.).text_color(rgb(0xffffff)))
        })
        .debug_selector(move || sel.clone())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                toggle_pick(ws, &dir);
                cx.notify();
            }),
        )
}

/// A song's ⋯ (its menu) and its Trash, side by side and always shown.
fn song_actions(s: &Song, cx: &mut Context<Workspace>) -> Div {
    let act = |glyph: Icon, sel: String, danger: bool| {
        div()
            .id(gpui::SharedString::from(sel.clone()))
            .group("song-act")
            .size(px(30.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .cursor_pointer()
            .hover(move |d| match danger {
                true => d.bg(alpha(RECORD, 0.14)).border_color(alpha(RECORD, 0.5)),
                false => d.bg(rgb(RAISED)).border_color(rgb(0x45454f)),
            })
            .debug_selector(move || sel.clone())
            .child(
                icon(glyph, if danger { 14. } else { 16. })
                    .text_color(rgb(TEXT_2))
                    .group_hover("song-act", move |d| {
                        d.text_color(rgb(if danger { 0xff7070 } else { TEXT }))
                    }),
            )
    };
    let (menu_dir, trash_dir) = (s.dir.clone(), s.dir.clone());
    div()
        .w(px(COL_ACTS))
        .flex_none()
        .flex()
        .justify_end()
        .gap(px(6.))
        .child(
            act(Icon::More, format!("song-more-{}", s.name), false).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.menu = Some(card_menu(&menu_dir, e.position));
                    cx.notify();
                }),
            ),
        )
        .child(
            act(Icon::Trash, format!("song-trash-{}", s.name), true).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.song_pending = Some(MenuAction::TrashSong(trash_dir.clone()));
                    cx.notify();
                }),
            ),
        )
}

/// "Open now", on the song open in this window.
fn open_now() -> Div {
    div()
        .flex_none()
        .h(px(20.))
        .px(px(8.))
        .flex()
        .items_center()
        .rounded(px(10.))
        .bg(alpha(ACCENT, 0.18))
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(ACCENT_TEXT))
        .child("Open now")
}

/// Its name, or the field renaming it.
fn name_of(s: &Song, renaming: Option<Entity<TextInput>>, size: f32) -> Div {
    match renaming {
        Some(f) => div()
            .h(px(26.))
            .px(px(7.))
            .flex()
            .items_center()
            .rounded(px(7.))
            .bg(rgb(FIELD))
            .border_1()
            .border_color(rgb(ACCENT))
            .text_size(px(size))
            .child(f),
        None => div()
            .min_w(px(0.))
            .text_size(px(size))
            .font_weight(FontWeight::SEMIBOLD)
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis()
            .child(s.name.clone()),
    }
}

/// A song as a row of the list: its cover, name, tempo, key, time, tracks, length and when it
/// was edited, and its ⋯ and Trash.
pub(crate) fn song_line(
    s: Song,
    here: bool,
    picked: bool,
    renaming: Option<Entity<TextInput>>,
    cx: &mut Context<Workspace>,
) -> Stateful<Div> {
    let cell = |w: f32, right: bool, text: String| {
        div()
            .w(px(w))
            .flex_none()
            .when(right, |d| d.flex().justify_end())
            .font_family(MONO)
            .text_size(px(12.5))
            .text_color(rgb(TEXT_2))
            .whitespace_nowrap()
            .overflow_hidden()
            .child(text)
    };
    let key = s
        .key
        .as_deref()
        .and_then(Key::parse)
        .map_or("—".to_string(), |k| k.short());
    let row = div()
        .id(gpui::SharedString::from(format!("song-{}", s.name)))
        .h(px(60.))
        .px(px(32.))
        .flex()
        .items_center()
        .gap(px(COL_GAP))
        .border_b_1()
        .border_color(rgb(0x17171c))
        .cursor_pointer()
        .map(|d| match (picked, here) {
            (true, _) => d.bg(alpha(ACCENT, 0.12)),
            (false, true) => d.bg(alpha(ACCENT, 0.05)).hover(|d| d.bg(rgb(0x17171d))),
            (false, false) => d.hover(|d| d.bg(rgb(0x141419))),
        })
        .debug_selector({
            let name = s.name.clone();
            move || format!("song-{name}")
        })
        .child(div().w(px(COL_PICK)).child(pick_box(picked, &s, cx)))
        .child(cover(&s.lanes, 44.0).w(px(COL_COVER)))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .items_center()
                .gap(px(10.))
                .child(name_of(&s, renaming, 13.5))
                .when(here, |d| d.child(open_now())),
        )
        .child(cell(COL_TEMPO, true, format!("{:.0}", s.tempo)))
        .child(cell(COL_KEY, false, key))
        .child(cell(
            COL_TIME,
            false,
            format!("{}/{}", s.signature[0], s.signature[1]),
        ))
        .child(cell(COL_TRACKS, true, s.tracks.to_string()))
        .child(cell(
            COL_LENGTH,
            true,
            if s.seconds > 0.5 {
                length(s.seconds)
            } else {
                "—".into()
            },
        ))
        .child(
            div()
                .w(px(COL_EDITED))
                .flex_none()
                .text_size(px(12.5))
                .text_color(rgb(TEXT_3))
                .whitespace_nowrap()
                .child(when(s.modified)),
        )
        .child(song_actions(&s, cx));
    song_clicks(row, s.dir.clone(), cx)
}

/// A song as a card of the grid: its cover large, its name, what it is, when it was edited, and
/// its ⋯ and Trash.
pub(crate) fn song_card(
    s: Song,
    here: bool,
    picked: bool,
    renaming: Option<Entity<TextInput>>,
    cx: &mut Context<Workspace>,
) -> Stateful<Div> {
    let card = div()
        .id(gpui::SharedString::from(format!("song-{}", s.name)))
        .min_w(px(0.))
        .flex()
        .flex_col()
        .gap(px(7.))
        .p(px(9.))
        .rounded(px(13.))
        .bg(rgb(if picked { 0x1c2233 } else { 0x141418 }))
        .border_1()
        .border_color(match (picked, here) {
            (true, _) => alpha(ACCENT, 0.9),
            (false, true) => alpha(ACCENT, 0.55),
            _ => rgb(0x24242b).into(),
        })
        .cursor_pointer()
        .when(!picked, |d| {
            d.hover(|d| d.bg(rgb(0x18181d)).border_color(rgb(0x3d3d47)))
        })
        .debug_selector({
            let name = s.name.clone();
            move || format!("song-{name}")
        })
        .child(
            div()
                .relative()
                .child(cover(&s.lanes, 120.0).w_full())
                .when(here, |d| {
                    d.child(div().absolute().top(px(9.)).right(px(9.)).child(open_now()))
                }),
        )
        .child(div().px(px(2.)).child(name_of(&s, renaming, 14.)))
        .child(
            div()
                .px(px(2.))
                .text_size(px(12.))
                .text_color(rgb(0xb4b4be))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(meta(&s)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .pl(px(2.))
                .child(pick_box(picked, &s, cx))
                .child(
                    div()
                        .ml(px(4.))
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(12.))
                        .text_color(rgb(TEXT_3))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(format!("Edited {}", when(s.modified).to_lowercase())),
                )
                .child(song_actions(&s, cx)),
        );
    song_clicks(card, s.dir.clone(), cx)
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
        .child(cover(&s.lanes, 28.0).w(px(44.)))
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
