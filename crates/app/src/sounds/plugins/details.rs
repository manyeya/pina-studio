//! The page's right column, as the sample library's: the plugin picked, its picture and what it
//! is, Add to a track (which, or a new one), and its facts, files, key and collections.

use gpui::{Div, FontWeight, MouseButton, MouseDownEvent, SharedString, div, px, rgb};

use super::*;
use crate::{
    sounds::{
        browser::{DETAILS_W, fact, filter_chip, heading, link, primary},
        catalog::Own,
    },
    ui::{
        menu::{Menu, MenuAction},
        theme::*,
    },
};

fn part(head: Div, body: impl IntoElement) -> Div {
    div().flex().flex_col().gap(px(6.)).child(head).child(body)
}

fn line(i: Icon, text: impl Into<SharedString>, value: impl Into<SharedString>) -> Div {
    div()
        .h(px(26.))
        .px(px(6.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(6.))
        .child(icon(i, 13.).text_color(rgb(TEXT_3)))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .text_size(px(11.5))
                .text_color(rgb(TEXT_2))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(text.into()),
        )
        .child(
            div()
                .font_family(MONO)
                .text_size(px(10.))
                .text_color(rgb(TEXT_3))
                .child(value.into()),
        )
}

impl Workspace {
    /// The selected track's name, where Add puts a plugin.
    fn add_target(&self) -> Option<String> {
        self.session
            .project
            .tracks
            .get(self.track)
            .map(|t| t.name.clone())
    }

    /// The menu of where a plugin can go: each track, or a new one.
    fn add_menu(&self, d: usize, at: gpui::Point<gpui::Pixels>) -> Menu {
        let preset = self.plugin_page.preset.clone();
        let mut items: Vec<(String, MenuAction)> = self
            .session
            .project
            .tracks
            .iter()
            .enumerate()
            .map(|(t, track)| {
                (
                    format!(
                        "{}  {}",
                        if t == self.track { "✓" } else { " " },
                        track.name
                    ),
                    MenuAction::Plugins(PluginOp::AddTo(d, Some(t), preset.clone())),
                )
            })
            .collect();
        items.push((String::new(), MenuAction::Nothing));
        items.push((
            "On a New Track".into(),
            MenuAction::Plugins(PluginOp::AddTo(d, None, preset)),
        ));
        Menu {
            at,
            items,
            ..Default::default()
        }
    }

    /// The right column.
    pub(crate) fn plugin_details(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let column = div()
            .id("plugin-details")
            .w(px(DETAILS_W))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .bg(rgb(SURFACE))
            .border_l_1()
            .border_color(rgb(LINE));
        let Some(d) = self.picked_plugin() else {
            return column.child(
                div()
                    .size_full()
                    .p(px(24.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .text_center()
                    .child(icon(Icon::Plug, 22.).text_color(rgb(TEXT_4)))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT_2))
                            .child("Pick a plugin"),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_3))
                            .child("Its details show here: its window, what it is, where it goes, its files, and the songs it's in. Double-click one to put it on the selected track."),
                    ),
            );
        };
        let dev = self.catalog.devices[d].clone();
        let page = &self.plugin_page;
        let fact_of = page.facts.get(&dev.key).cloned().unwrap_or_default();
        let preset = page.preset.clone();
        let w = DETAILS_W - 28.0;
        let h = (w * self.picture_ratio(&dev.key)).min(320.0).round();
        let art = self.card_art(&dev, w, h);
        let kind = subcategory(&dev);
        let fav = self.catalog.favourites.contains(&dev.key);
        let mapped = self
            .catalog
            .keys
            .iter()
            .find(|(_, k)| *k == dev.key)
            .map(|(k, _)| k.clone());
        let on_tracks = self.tracks_with(&dev);
        let songs = page
            .songs
            .as_ref()
            .map(|s| (s.used.get(&dev.key).copied().unwrap_or(0), s.songs));
        let track = self.add_target();
        let builtin = matches!(dev.own, Some(Own::Eq | Own::Comp | Own::Sat));
        let hint = match (&track, dev.instrument(), builtin) {
            (None, ..) => "On a track of its own".to_string(),
            (Some(t), _, true) => format!("On {t}'s channel strip"),
            (Some(t), true, _) => format!("Plays on {t}, in place of its instrument"),
            (Some(t), false, _) => format!("After {t}'s other effects"),
        };
        // What it is.
        let mut facts = vec![
            fact("Category", dev.cat.name()),
            fact("Kind", kind.unwrap_or("—")),
        ];
        facts.push(fact(
            "Maker",
            match dev.own {
                Some(_) => "Pina Studio".to_string(),
                None => dev.vendor.clone(),
            },
        ));
        if dev.own.is_none() {
            facts.push(fact("Formats", dev.badges().join(" · ")));
            let version = dev
                .formats
                .iter()
                .find_map(|p| page.files.get(&p.path).and_then(|f| f.version.clone()));
            facts.push(fact("Version", version.unwrap_or_else(|| "—".into())));
            facts.push(fact(
                "Size",
                match self.device_size(&dev) {
                    0 => "—".to_string(),
                    n => size_text(n),
                },
            ));
        }
        facts.push(fact(
            "CPU",
            match fact_of.load {
                Some(l) => format!("{:.1}%", l * 100.0),
                None if dev.own.is_some() => "Light".into(),
                None => "Not timed yet".into(),
            },
        ));
        if let Some((n, of)) = songs {
            facts.push(fact("Songs", format!("{n} of {of}")));
        }
        facts.push(fact(
            "Used here",
            match self.catalog.uses.get(&dev.key).copied().unwrap_or(0) {
                0 => "Never".to_string(),
                1 => "Once".to_string(),
                n => format!("{n} times"),
            },
        ));
        let files: Vec<Div> = dev
            .formats
            .iter()
            .map(|p| {
                let path = p.path.clone();
                let name = path
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                let size = page.files.get(&path).map_or(0, |f| f.size);
                line(
                    Icon::Folder,
                    name,
                    match size {
                        0 => p.format.name().to_string(),
                        s => format!("{} · {}", p.format.name(), size_text(s)),
                    },
                )
                .cursor_pointer()
                .hover(|s| s.bg(rgb(OFF)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.plugin_op(PluginOp::Reveal(path.clone()), cx)
                    }),
                )
            })
            .collect();
        let collections: Vec<Div> = self
            .catalog
            .collections
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let has = c.devices.contains(&dev.key);
                filter_chip(c.name.clone(), None, has, Some(c.color)).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.plugin_op(PluginOp::ToggleCollection(d, i), cx)
                    }),
                )
            })
            .chain(std::iter::once(
                filter_chip("New", None, false, None)
                    .child(icon(Icon::Plus, 11.).text_color(rgb(TEXT_3)))
                    .flex_row_reverse()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.plugin_op(PluginOp::NewCollection(Some(d)), cx)
                        }),
                    ),
            ))
            .collect();
        let title = match &preset {
            Some(p) => p.name.clone(),
            None => dev.name.clone(),
        };
        let under = match (&preset, dev.own) {
            (Some(_), _) => format!("{} · {}", dev.name, dev.vendor),
            (None, Some(_)) => "Pina Studio".to_string(),
            (None, None) => dev.vendor.clone(),
        };
        let add_preset = preset.clone();
        let new_preset = preset.clone();
        column.child(
            div()
                .p(px(14.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(
                            div()
                                .text_size(px(15.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_3))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(under),
                        ),
                )
                .child(
                    div()
                        .w(px(w))
                        .h(px(h))
                        .rounded(px(8.))
                        .overflow_hidden()
                        .border_1()
                        .border_color(rgb(LINE))
                        .when(fact_of.failed.is_some(), |d| d.opacity(0.5))
                        .child(art),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .flex_wrap()
                        .child(chip(dev.cat.name(), dev.cat.color()))
                        .children(kind.map(|k| chip(k, TEXT_2)))
                        .children(dev.badges().into_iter().map(|b| chip(b, TEXT_3)))
                        .when(fav, |d| d.child(chip("★ Starred", AMBER)))
                        .children(
                            mapped
                                .as_ref()
                                .map(|k| chip(format!("Key {}", key_label(k)), ACCENT_TEXT)),
                        ),
                )
                .when_some(fact_of.failed.clone(), |el, why| {
                    el.child(
                        div()
                            .p(px(10.))
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .rounded(px(8.))
                            .bg(alpha(RECORD, 0.08))
                            .border_1()
                            .border_color(alpha(RECORD, 0.4))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(RECORD))
                                    .child("Won't load"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_2))
                                    .child(format!("When its picture was taken, {why}.")),
                            )
                            .child(button("Try again", false).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.plugin_op(PluginOp::Retake(d), cx)
                                }),
                            )),
                    )
                })
                // Where it goes.
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(
                            div()
                                .flex()
                                .gap(px(6.))
                                .child(
                                    primary(match &track {
                                        Some(t) => format!("Add to {t}"),
                                        None => "Add to a new track".to_string(),
                                    })
                                    .flex_1()
                                    .min_w(px(0.))
                                    .child(icon(Icon::Plus, 14.).text_color(rgb(0xffffff)))
                                    .flex_row_reverse()
                                    .debug_selector(|| "plugin-add".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            ws.load_plugin_card(d, add_preset.clone(), false, cx)
                                        }),
                                    ),
                                )
                                .child(
                                    div()
                                        .size(px(30.))
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(8.))
                                        .bg(rgb(CARD))
                                        .border_1()
                                        .border_color(rgb(LINE))
                                        .cursor_pointer()
                                        .hover(|s| s.bg(rgb(RAISED)))
                                        .debug_selector(|| "plugin-add-where".into())
                                        .child(icon(Icon::Chevron, 13.).text_color(rgb(TEXT_2)))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                                ws.menu = Some(ws.add_menu(d, e.position));
                                                cx.notify();
                                            }),
                                        ),
                                ),
                        )
                        .child(
                            button("On a new track", false)
                                .h(px(30.))
                                .w_full()
                                .debug_selector(|| "plugin-add-new".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        ws.load_plugin_card(d, new_preset.clone(), true, cx)
                                    }),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_3))
                                .text_center()
                                .child(format!("{hint} · or drag it onto a track")),
                        ),
                )
                .when(!on_tracks.is_empty(), |d| {
                    d.child(part(
                        heading("In this song"),
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(4.))
                            .children(on_tracks.into_iter().map(|t| chip(t, MINT))),
                    ))
                })
                .child(part(
                    heading("Plugin"),
                    div()
                        .grid()
                        .grid_cols(2)
                        .gap_x(px(12.))
                        .gap_y(px(10.))
                        .children(facts),
                ))
                .when(!files.is_empty(), |d| {
                    d.child(part(
                        heading("Files"),
                        div().flex().flex_col().children(files),
                    ))
                })
                .child(part(
                    heading("Key").when(mapped.is_some(), |d| {
                        let k = mapped.clone().unwrap_or_default();
                        d.child(link("Clear").on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                ws.plugin_op(PluginOp::Unmap(k.clone()), cx)
                            }),
                        ))
                    }),
                    match &mapped {
                        Some(k) => {
                            div()
                                .text_size(px(11.5))
                                .text_color(rgb(TEXT_2))
                                .child(format!(
                                    "{} puts it on the selected track, anywhere in Pina Studio.",
                                    key_label(k)
                                ))
                        }
                        None => button("Put it on a key…", false)
                            .debug_selector(|| "plugin-map".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.plugin_op(PluginOp::MapCard(d), cx)
                                }),
                            ),
                    },
                ))
                .child(part(
                    heading("Collections"),
                    div().flex().flex_wrap().gap(px(5.)).children(collections),
                ))
                .child(
                    div()
                        .flex()
                        .gap(px(6.))
                        .child(
                            button(if fav { "★ Starred" } else { "☆ Star" }, fav)
                                .debug_selector(|| "plugin-star".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        ws.plugin_op(PluginOp::Favourite(d), cx)
                                    }),
                                ),
                        )
                        .when(dev.own.is_none(), |row| {
                            row.child(button("Take its picture again", false).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.plugin_op(PluginOp::Retake(d), cx)
                                }),
                            ))
                        }),
                ),
        )
    }
}
