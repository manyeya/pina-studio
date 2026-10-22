//! The Browser, drawn: its search, rows and the device being dragged.

use gpui::StyledImage;

use super::*;

impl Workspace {
    /// The Browser's devices, presets, sounds and songs: under its search and tabs, in the
    /// right panel (`browser.rs`).
    pub fn device_browser(&mut self, cx: &mut Context<Self>) -> Div {
        let rows = self.catalog_rows(cx);
        let query = self.catalog_query(cx);
        let c = &self.catalog;
        let (scope, cat, picked) = (c.scope, c.cat, c.picked);
        let target = self
            .session
            .project
            .tracks
            .get(self.track)
            .map_or("No track: a device makes one".to_string(), |t| {
                format!("Adds to {}", t.name)
            });
        let chips: Vec<Cat> = match scope {
            Scope::Instruments => INSTRUMENT_CATS.to_vec(),
            Scope::Effects => EFFECT_CATS.to_vec(),
            _ => INSTRUMENT_CATS
                .iter()
                .chain(EFFECT_CATS.iter())
                .copied()
                .collect(),
        };
        let chip_row = div()
            .flex()
            .flex_wrap()
            .gap(px(5.))
            .children(chips.into_iter().map(|k| {
                let on = cat == Some(k);
                let color = k.color();
                div()
                    .id(k.name())
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .h(px(24.))
                    .px(px(8.))
                    .rounded(px(12.))
                    .text_size(px(11.5))
                    .cursor_pointer()
                    .border_1()
                    .when(on, |d| {
                        d.bg(alpha(color, 0.18))
                            .border_color(alpha(color, 0.55))
                            .text_color(rgb(TEXT))
                    })
                    .when(!on, |d| {
                        d.bg(rgb(CARD))
                            .border_color(rgb(LINE_SOFT))
                            .text_color(rgb(TEXT_2))
                            .hover(|d| d.bg(rgb(RAISED)))
                    })
                    .child(k.glyph(12.).text_color(rgb(color)))
                    .child(k.name())
                    .debug_selector(move || format!("cat-{}", k.name()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.catalog.cat = if ws.catalog.cat == Some(k) {
                                None
                            } else {
                                Some(k)
                            };
                            ws.catalog.picked = None;
                            cx.notify();
                        }),
                    )
            }));
        let favourites = self.catalog.favourites.clone();
        let open = self.catalog.open.clone();
        let current = self.session.dir.clone();
        let mut n = 0usize;
        let mut list = Vec::new();
        for row in rows {
            if let Row::Heading(h) = &row {
                list.push(
                    div()
                        .pt(px(10.))
                        .pb(px(4.))
                        .px(px(4.))
                        .text_size(px(10.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT_3))
                        .child(h.to_uppercase())
                        .into_any_element(),
                );
                continue;
            }
            let on = picked == Some(n);
            n += 1;
            list.push(match row {
                Row::Device(d) => {
                    let dev = &self.catalog.devices[d];
                    let fav = favourites.contains(&dev.key);
                    let shown = open.as_deref() == Some(dev.key.as_str());
                    self.device_row(d, dev.clone(), on, fav, shown, cx)
                        .into_any_element()
                }
                Row::Preset(d, p) => self.preset_row(d, p, on, cx).into_any_element(),
                Row::Sample(i) => self.sound_row(i, on, cx).into_any_element(),
                Row::Song(s) => {
                    let here = s.dir == current;
                    crate::pages::projects::song_row(s, on, here, cx).into_any_element()
                }
                Row::Heading(_) => unreachable!(),
            });
        }
        let empty = list.is_empty();
        div()
            .key_context("DeviceBrowser")
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .px(px(10.))
            .pb(px(10.))
            .when(query.is_empty() || cat.is_some(), |d| d.child(chip_row))
            .child(
                div()
                    .id("browser-results")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .children(list)
                    .when(empty, |d| {
                        d.child(
                            div()
                                .pt(px(20.))
                                .text_size(px(12.))
                                .text_color(rgb(TEXT_3))
                                .child(match (scope, query.is_empty()) {
                                    (_, true) => "Looking for plugins…".to_string(),
                                    _ => format!("Nothing called \u{201c}{query}\u{201d}."),
                                }),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .child(div().text_color(rgb(TEXT_3)).child(target))
                    .child("Click adds · double-click or ⇧↵ a new track · drag onto a track"),
            )
    }

    pub(crate) fn device_row(
        &self,
        d: usize,
        dev: Device,
        on: bool,
        fav: bool,
        shown: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let color = dev.cat.color();
        let key = dev.key.clone();
        let fav_key = key.clone();
        let open_key = key.clone();
        let plugin = dev.own.is_none();
        let kind = crate::sounds::plugins::subcategory(&dev).unwrap_or(dev.cat.name());
        let sub = match (&dev.own, dev.badges().as_slice()) {
            (Some(_), _) => format!("Pina Studio · {kind}"),
            (None, b) => format!("{} · {kind} · {}", dev.vendor, b.join(" ")),
        };
        // Its picture, once it's had it taken (the Plugins page).
        let picture = self
            .plugin_page
            .facts
            .get(&dev.key)
            .and_then(|f| f.picture.clone())
            .map(|n| crate::sounds::plugins::pictures_dir().join(n))
            .filter(|p| p.exists());
        let mark = match picture {
            Some(p) => div()
                .w(px(52.))
                .h(px(32.))
                .flex_none()
                .rounded(px(6.))
                .overflow_hidden()
                .border_1()
                .border_color(rgb(LINE_STRONG))
                .child(
                    gpui::img(p)
                        .w(px(52.))
                        .h(px(32.))
                        .object_fit(gpui::ObjectFit::Cover),
                ),
            None => div()
                .w(px(52.))
                .h(px(32.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .bg(alpha(color, 0.14))
                .child(dev.cat.glyph(15.).text_color(rgb(color))),
        };
        div()
            .group("device-row")
            .flex()
            .items_center()
            .gap(px(9.))
            .h(px(44.))
            .px(px(6.))
            .rounded(px(8.))
            .cursor_pointer()
            .when(on, |d| d.bg(alpha(ACCENT, 0.16)))
            .when(!on, |d| d.hover(|d| d.bg(rgb(OFF))))
            .debug_selector(move || format!("device-{key}"))
            .child(mark)
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
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(dev.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(sub),
                    ),
            )
            .when(plugin, |row| {
                row.child(
                    div()
                        .id(("presets", d))
                        .size(px(22.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .text_color(rgb(if shown { TEXT } else { TEXT_4 }))
                        .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT_2)))
                        .child(icon(Icon::Chevron, 12.).text_color(rgb(if shown {
                            TEXT
                        } else {
                            TEXT_3
                        })))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.catalog.open =
                                    match ws.catalog.open.as_deref() == Some(&open_key) {
                                        true => None,
                                        false => Some(open_key.clone()),
                                    };
                                cx.notify();
                            }),
                        ),
                )
            })
            .child(self.device_agent_button(d, "device-row", cx))
            .child(
                div()
                    .size(px(22.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .text_size(px(13.))
                    .text_color(rgb(if fav { AMBER } else { TEXT_4 }))
                    .hover(|d| d.bg(rgb(RAISED)))
                    .child(if fav { "★" } else { "☆" })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.toggle_favourite(&fav_key, cx);
                        }),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    let p = pt(e.position);
                    if e.click_count == 2 {
                        ws.use_device(d, Target::NewTrack, None, cx);
                        return;
                    }
                    ws.drag = Some(Drag::Device {
                        device: d,
                        from: p,
                        at: p,
                        moved: false,
                    });
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |ws, e: &gpui::MouseUpEvent, _, cx| {
                    // A click that didn't drag: onto the selected track. (Two clicks make a
                    // track of its own, above.)
                    if let Some(Drag::Device { moved: false, .. }) = ws.drag
                        && e.click_count < 2
                    {
                        ws.drag = None;
                        let t = ws.track;
                        ws.use_device(d, Target::Track(t), None, cx);
                    }
                }),
            )
    }

    pub(crate) fn preset_row(&self, d: usize, p: Preset, on: bool, cx: &mut Context<Self>) -> Div {
        let name = p.name.clone();
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(28.))
            .pl(px(41.))
            .pr(px(8.))
            .rounded(px(7.))
            .cursor_pointer()
            .text_size(px(12.))
            .text_color(rgb(TEXT_2))
            .when(on, |d| d.bg(alpha(ACCENT, 0.16)))
            .when(!on, |d| d.hover(|d| d.bg(rgb(OFF)).text_color(rgb(TEXT))))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(name),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(TEXT_4))
                    .child("preset"),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    let target = match e.click_count {
                        2 => Target::NewTrack,
                        _ => Target::Track(ws.track),
                    };
                    ws.use_device(d, target, Some(p.clone()), cx);
                }),
            )
    }

    pub(crate) fn sound_row(&self, i: usize, on: bool, cx: &mut Context<Self>) -> Div {
        let Some(s) = self.browser.sample(i) else {
            return div();
        };
        let color = crate::sounds::browser::category_color(s.analysis.category);
        let about = pina_engine::samples::search::describe(s);
        div()
            .flex()
            .items_center()
            .gap(px(9.))
            .h(px(36.))
            .px(px(6.))
            .rounded(px(8.))
            .cursor_pointer()
            .when(on, |d| d.bg(alpha(ACCENT, 0.16)))
            .when(!on, |d| d.hover(|d| d.bg(rgb(OFF))))
            .child(
                div()
                    .size(px(26.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.))
                    .bg(alpha(color, 0.14))
                    .child(
                        glyph(
                            pina_engine::music::look::category_icon(s.analysis.category),
                            15.,
                        )
                        .text_color(rgb(color)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(s.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(about),
                    ),
            )
            // Click to hear it (in the Samples browser); drag it onto the tracks as there.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    let p = pt(e.position);
                    ws.select_sample(i, true, cx);
                    ws.drag = Some(Drag::Sample {
                        index: i,
                        from: p,
                        at: p,
                        moved: false,
                    });
                }),
            )
    }
}
