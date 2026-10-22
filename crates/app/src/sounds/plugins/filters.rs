//! The page's left column, as the sample library's: the search and what to look for, then the
//! chips (categories, kinds, makers, collections, Health) and the library.

use gpui::{
    Div, FontWeight, MouseButton, MouseDownEvent, MouseUpEvent, SharedString, div, px, rgb,
};

use super::*;
use crate::{
    sounds::browser::{filter_chip, heading, link},
    ui::{
        menu::{Menu, MenuAction},
        theme::*,
    },
    workspace::{Drag, Name},
};

/// How many makers show until All is clicked.
const MAKERS: usize = 12;

/// A label on the left of a row, its control on the right.
fn labelled(label: &'static str, control: Div) -> Div {
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
}

/// A part of the column: its heading, then what's in it.
fn part(head: Div, body: impl IntoElement) -> Div {
    div().flex().flex_col().gap(px(6.)).child(head).child(body)
}

fn chips(list: Vec<gpui::AnyElement>) -> Div {
    div().flex().flex_wrap().gap(px(5.)).children(list)
}

fn quiet(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(11.))
        .text_color(rgb(TEXT_4))
        .child(text.into())
}

/// A line of the Library part: an icon, what it is, a figure.
fn fact_row(i: Icon, label: impl Into<SharedString>, value: impl Into<SharedString>) -> Div {
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
                .child(label.into()),
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
    /// The search field: the Browser's own, its words carried between the two.
    fn plugin_search(&self, cx: &mut Context<Self>) -> Div {
        let focused = self.plugin_page.searching;
        let typed = !self.catalog_query(cx).is_empty();
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
            .debug_selector(|| "plugin-search".into())
            .child(icon(Icon::Search, 14.).text_color(rgb(if focused { TEXT_2 } else { TEXT_3 })))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .child(self.catalog.search.clone()),
            )
            .when(typed, |d| {
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
                                ws.catalog.search.update(cx, |i, cx| i.set_text("", cx));
                            }),
                        ),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    ws.plugin_page.focus_search = true;
                    cx.notify();
                }),
            )
    }

    /// All, instruments or effects: the Browser's tabs.
    fn plugin_kinds_tray(&self, cx: &mut Context<Self>) -> Div {
        let scope = self.catalog.scope;
        let tab = |label: &'static str, s: Scope, cx: &mut Context<Self>| {
            seg(label, scope == s, 24.)
                .flex_1()
                .debug_selector(move || format!("plugins-kind-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.catalog.scope = s;
                        let page = &mut ws.plugin_page;
                        (page.cats, page.kinds) = (Vec::new(), Vec::new());
                        cx.notify();
                    }),
                )
        };
        tray()
            .p(px(2.))
            .rounded(px(8.))
            .child(tab("All", Scope::All, cx))
            .child(tab("Instruments", Scope::Instruments, cx))
            .child(tab("Effects", Scope::Effects, cx))
    }

    /// The left column.
    pub(crate) fn plugin_filters(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let pool = self.plugin_pool(cx);
        let page = &self.plugin_page;
        let devices = &self.catalog.devices;
        let count = |skip: Chips, f: &dyn Fn(&Device) -> bool| {
            pool.iter()
                .map(|&i| &devices[i])
                .filter(|d| self.plugin_chips_fit(d, skip) && f(d))
                .count()
        };
        let starred = page.starred;
        let stars = self
            .catalog
            .devices
            .iter()
            .filter(|d| self.catalog.favourites.contains(&d.key))
            .count();
        let starred_row = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .p(px(10.))
            .rounded(px(8.))
            .bg(if starred {
                alpha(ACCENT, 0.1)
            } else {
                rgb(CARD).into()
            })
            .border_1()
            .border_color(if starred {
                alpha(ACCENT, 0.45)
            } else {
                rgb(LINE).into()
            })
            .cursor_pointer()
            .debug_selector(|| "plugins-starred".into())
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
                            .child("Starred"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(match stars {
                                0 => "Star a plugin to keep it to hand".to_string(),
                                1 => "The one you've starred".to_string(),
                                n => format!("The {n} you've starred"),
                            }),
                    ),
            )
            .child(switch(starred))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    ws.plugin_page.starred = !starred;
                    cx.notify();
                }),
            );
        // Where from, which formats, in what order.
        let source = page.source;
        let source_seg = |label: &'static str, s: Source, cx: &mut Context<Self>| {
            seg(label, source == s, 22.)
                .debug_selector(move || format!("plugins-source-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.plugin_page.source = s;
                        cx.notify();
                    }),
                )
        };
        let source_row = labelled(
            "Source",
            tray()
                .p(px(2.))
                .rounded(px(8.))
                .child(source_seg("All", Source::All, cx))
                .child(source_seg("Plugins", Source::Plugins, cx))
                .child(source_seg("Built in", Source::Stock, cx))
                .child(source_seg("Presets", Source::Saved, cx)),
        );
        let off = page.formats_off.clone();
        let format_segs: Vec<Div> = [Format::Clap, Format::Vst3, Format::Au, Format::Vst2]
            .into_iter()
            .filter(|f| {
                self.catalog
                    .devices
                    .iter()
                    .any(|d| d.formats.iter().any(|p| p.format == *f))
            })
            .map(|f| {
                let name = match f {
                    Format::Clap => "CLAP",
                    Format::Vst3 => "VST3",
                    Format::Au => "AU",
                    Format::Vst2 => "VST",
                };
                seg(name, !off.contains(&f), 22.)
                    .debug_selector(move || format!("plugins-format-{name}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            let off = &mut ws.plugin_page.formats_off;
                            match off.iter().position(|x| *x == f) {
                                Some(i) => drop(off.remove(i)),
                                None => off.push(f),
                            }
                            cx.notify();
                        }),
                    )
            })
            .collect();
        let format_row = labelled(
            "Format",
            tray().p(px(2.)).rounded(px(8.)).children(format_segs),
        );
        let (sort, descending) = (page.sort, page.descending);
        let sort_seg = |label: &'static str, s: Sort, cx: &mut Context<Self>| {
            let on = sort == s;
            seg(
                match (on, descending) {
                    (true, true) => format!("{label} ↑"),
                    _ => label.to_string(),
                },
                on,
                22.,
            )
            .debug_selector(move || format!("plugins-sort-{label}"))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    let page = &mut ws.plugin_page;
                    match page.sort == s {
                        true => page.descending ^= true,
                        false => (page.sort, page.descending) = (s, false),
                    }
                    cx.notify();
                }),
            )
        };
        let sort_row = labelled(
            "Order",
            tray()
                .p(px(2.))
                .rounded(px(8.))
                .child(sort_seg("Name", Sort::Name, cx))
                .child(sort_seg("Used", Sort::Used, cx))
                .child(sort_seg("CPU", Sort::Cpu, cx)),
        );
        // The categories, each with what it would find.
        let picked_cats = page.cats.clone();
        let cats: Vec<gpui::AnyElement> = scope_cats(self.catalog.scope)
            .into_iter()
            .filter_map(|c| {
                let n = count(Chips::Cats, &|d| d.cat == c);
                let on = picked_cats.contains(&c);
                (n > 0 || on).then(|| {
                    filter_chip(c.name(), Some(n), on, Some(c.color()))
                        .debug_selector(move || format!("plugins-cat-{}", c.name()))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                let page = &mut ws.plugin_page;
                                match page.cats.iter().position(|x| *x == c) {
                                    Some(i) => drop(page.cats.remove(i)),
                                    None => page.cats.push(c),
                                }
                                page.kinds.clear();
                                cx.notify();
                            }),
                        )
                        .into_any_element()
                })
            })
            .collect();
        // With a category picked, the kinds in it.
        let mut kinds: Vec<(&'static str, usize)> = Vec::new();
        if !picked_cats.is_empty() {
            for &i in &pool {
                let d = &devices[i];
                if !self.plugin_chips_fit(d, Chips::Kinds) {
                    continue;
                }
                if let Some(s) = subcategory(d) {
                    match kinds.iter_mut().find(|k| k.0 == s) {
                        Some(k) => k.1 += 1,
                        None => kinds.push((s, 1)),
                    }
                }
            }
            kinds.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        }
        let picked_kinds = page.kinds.clone();
        let kind_chips: Vec<gpui::AnyElement> = kinds
            .into_iter()
            .map(|(s, n)| {
                filter_chip(s, Some(n), picked_kinds.contains(&s), None)
                    .debug_selector(move || format!("plugins-sub-{s}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            let kinds = &mut ws.plugin_page.kinds;
                            match kinds.iter().position(|x| *x == s) {
                                Some(i) => drop(kinds.remove(i)),
                                None => kinds.push(s),
                            }
                            cx.notify();
                        }),
                    )
                    .into_any_element()
            })
            .collect();
        // The makers, the biggest first.
        let mut makers: Vec<(String, usize)> = Vec::new();
        for &i in &pool {
            let d = &devices[i];
            if !self.plugin_chips_fit(d, Chips::Makers) {
                continue;
            }
            match makers.iter_mut().find(|m| m.0 == d.vendor) {
                Some(m) => m.1 += 1,
                None => makers.push((d.vendor.clone(), 1)),
            }
        }
        makers.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let more_makers = makers.len().saturating_sub(MAKERS);
        let picked_makers = page.makers.clone();
        let all_makers = page.all_makers;
        let maker_chips: Vec<gpui::AnyElement> = makers
            .into_iter()
            .enumerate()
            .filter(|(i, m)| all_makers || *i < MAKERS || picked_makers.contains(&m.0))
            .map(|(_, (m, n))| {
                let on = picked_makers.contains(&m);
                let key = m.clone();
                filter_chip(m, Some(n), on, None)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            let makers = &mut ws.plugin_page.makers;
                            match makers.iter().position(|x| *x == key) {
                                Some(i) => drop(makers.remove(i)),
                                None => makers.push(key.clone()),
                            }
                            cx.notify();
                        }),
                    )
                    .into_any_element()
            })
            .collect();
        // Your collections: a plugin dragged onto one goes in it.
        let picked_collection = page.collection;
        let collections: Vec<gpui::AnyElement> = self
            .catalog
            .collections
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if let Some(input) = self.name_field(Name::PluginCollection(i)) {
                    return div().w(px(150.)).h(px(24.)).child(input).into_any_element();
                }
                let n = count(Chips::Collection, &|d| c.devices.contains(&d.key));
                filter_chip(
                    c.name.clone(),
                    Some(n),
                    picked_collection == Some(i),
                    Some(c.color),
                )
                .debug_selector(move || format!("plugins-collection-{i}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                        if e.click_count == 2 {
                            ws.start_naming(Name::PluginCollection(i), window, cx);
                            return;
                        }
                        let page = &mut ws.plugin_page;
                        page.collection = (page.collection != Some(i)).then_some(i);
                        cx.notify();
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseUpEvent, _, cx| {
                        if let Some(Drag::Device {
                            device,
                            moved: true,
                            ..
                        }) = ws.drag
                        {
                            ws.drag = None;
                            ws.add_to_collection(device, i);
                            cx.notify();
                        }
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        ws.menu = Some(ws.collection_menu(i, e.position));
                        cx.notify();
                    }),
                )
                .into_any_element()
            })
            .collect();
        // What Health found, each a filter.
        let set = page.set;
        let looked = page.songs.is_some();
        let health: Vec<gpui::AnyElement> = HealthSet::ALL
            .into_iter()
            .filter_map(|h| {
                let n = count(Chips::Health, &|d| self.in_set(d, h));
                let on = set == Some(h);
                (n > 0 || on).then(|| {
                    filter_chip(h.name(), Some(n), on, Some(h.color()))
                        .debug_selector(move || format!("plugins-health-{h:?}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                let page = &mut ws.plugin_page;
                                page.set = (page.set != Some(h)).then_some(h);
                                cx.notify();
                            }),
                        )
                        .into_any_element()
                })
            })
            .collect();
        let any_cats = !picked_cats.is_empty();
        let any_kinds = !picked_kinds.is_empty();
        let any_makers = !picked_makers.is_empty();
        let clear = |label: &'static str, f: fn(&mut PluginPage), cx: &mut Context<Self>| {
            link(label).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    f(&mut ws.plugin_page);
                    cx.notify();
                }),
            )
        };
        let library = self.plugin_library(cx);
        div()
            .id("plugin-filters")
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
                    .child(self.plugin_search(cx))
                    .child(self.plugin_kinds_tray(cx))
                    .child(starred_row)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(source_row)
                            .child(format_row)
                            .child(sort_row),
                    )
                    .child(part(
                        heading("Categories").when(any_cats, |d| {
                            d.child(clear(
                                "Clear",
                                |p| (p.cats, p.kinds) = (Vec::new(), Vec::new()),
                                cx,
                            ))
                        }),
                        match cats.is_empty() {
                            true => quiet("None with these filters."),
                            false => chips(cats),
                        },
                    ))
                    .when(!kind_chips.is_empty(), |d| {
                        d.child(part(
                            heading("Kinds").when(any_kinds, |d| {
                                d.child(clear("Clear", |p| p.kinds.clear(), cx))
                            }),
                            chips(kind_chips),
                        ))
                    })
                    .when(!maker_chips.is_empty(), |d| {
                        d.child(part(
                            heading("Makers").child(
                                div()
                                    .flex()
                                    .gap(px(10.))
                                    .when(any_makers, |d| {
                                        d.child(clear("Clear", |p| p.makers.clear(), cx))
                                    })
                                    .when(more_makers > 0 || all_makers, |d| {
                                        d.child(clear(
                                            if all_makers { "Fewer" } else { "All" },
                                            |p| p.all_makers ^= true,
                                            cx,
                                        ))
                                    }),
                            ),
                            chips(maker_chips),
                        ))
                    })
                    .child(part(
                        heading("Collections").child(
                            link("New")
                                .debug_selector(|| "plugins-new-collection".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| {
                                        ws.plugin_op(PluginOp::NewCollection(None), cx)
                                    }),
                                ),
                        ),
                        match collections.is_empty() {
                            true => quiet(
                                "Right-click a plugin, or pick one, to keep it in a collection.",
                            ),
                            false => chips(collections),
                        },
                    ))
                    .child(part(
                        heading("Health"),
                        match (health.is_empty(), looked) {
                            (_, false) => quiet("Looking through your plugins and songs…"),
                            (true, true) => quiet("Nothing found wrong."),
                            (false, true) => chips(health),
                        },
                    ))
                    .child(library),
            )
    }

    /// The library: what the plugins take on disk and what could go, the pictures, and the
    /// plugins the songs name that aren't here.
    fn plugin_library(&mut self, cx: &mut Context<Self>) -> Div {
        let c = self.checkup();
        let plugins = self
            .catalog
            .devices
            .iter()
            .filter(|d| d.own.is_none())
            .count();
        let page = &self.plugin_page;
        let pictured = self
            .catalog
            .devices
            .iter()
            .filter(|d| d.own.is_none())
            .filter(|d| page.facts.get(&d.key).is_some_and(|f| !f.wants_picture()))
            .count();
        let missing: Vec<(String, Vec<String>)> = page
            .songs
            .as_ref()
            .map(|s| {
                let mut m: Vec<_> = s.missing.clone().into_iter().collect();
                m.sort();
                m
            })
            .unwrap_or_default();
        let indexing = page.indexing.as_ref().map(|i| {
            (
                i.total - i.todo.len(),
                i.total,
                i.now.clone().unwrap_or_default(),
            )
        });
        let spare = c.spare.clone();
        let spare_size = c.spare_size;
        let running = indexing.is_some();
        let pictures: Div = match indexing {
            Some((done, of, now)) => div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_2))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(format!("Photographing {now}")),
                        )
                        .child(
                            div()
                                .font_family(MONO)
                                .text_size(px(10.))
                                .text_color(rgb(TEXT_3))
                                .child(format!("{done} of {of}")),
                        ),
                )
                .child(
                    div().h(px(4.)).rounded(px(2.)).bg(rgb(RAISED)).child(
                        div()
                            .h_full()
                            .w(gpui::relative(done as f32 / of.max(1) as f32))
                            .rounded(px(2.))
                            .bg(rgb(ACCENT)),
                    ),
                ),
            None => div(),
        };
        let take = match (running, pictured < plugins) {
            (true, _) => Some(button("Stop", false)),
            (false, true) => Some(
                button(format!("Take {} pictures", plugins - pictured), false)
                    .child(icon(Icon::Zoom, 12.).text_color(rgb(TEXT_3)))
                    .flex_row_reverse(),
            ),
            (false, false) => None,
        };
        part(
            heading("Library").child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .cursor_pointer()
                    .text_color(rgb(TEXT_3))
                    .hover(|s| s.text_color(rgb(TEXT)))
                    .debug_selector(|| "plugins-rescan".into())
                    .child(icon(Icon::Rescan, 12.).text_color(rgb(TEXT_3)))
                    .child("Rescan")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.status = Some("Looking for new plugins…".into());
                            ws.start_scan(cx);
                            cx.notify();
                        }),
                    ),
            ),
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(fact_row(
                    Icon::Plug,
                    format!("{plugins} plugins"),
                    size_text(c.total),
                ))
                .child(fact_row(
                    Icon::Zoom,
                    "Pictures",
                    format!("{pictured} of {plugins}"),
                ))
                .when(!spare.is_empty(), |d| {
                    d.child(fact_row(
                        Icon::Trash,
                        format!("Spare files · {}", spare.len()),
                        size_text(spare_size),
                    ))
                })
                .child(pictures)
                .children(take.map(|b| {
                    b.mt(px(4.))
                        .debug_selector(|| "plugins-index".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.take_plugin_pictures(None, cx)),
                        )
                }))
                .when(!spare.is_empty(), |d| {
                    d.child(
                        button(
                            format!("Move {} spare files to the Trash…", spare.len()),
                            false,
                        )
                        .debug_selector(|| "plugins-trash".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                let mut items: Vec<(String, MenuAction)> = vec![(
                                    "These go to the Trash, to put back from there if need be:"
                                        .into(),
                                    MenuAction::Nothing,
                                )];
                                items.extend(
                                    spare.iter().map(|p| {
                                        (format!("  {}", p.display()), MenuAction::Nothing)
                                    }),
                                );
                                items.push((String::new(), MenuAction::Nothing));
                                items.push((
                                    format!(
                                        "Move {} files ({}) to the Trash",
                                        spare.len(),
                                        size_text(spare_size)
                                    ),
                                    MenuAction::Plugins(PluginOp::Trash(spare.clone())),
                                ));
                                ws.menu = Some(Menu {
                                    at: e.position,
                                    items,
                                    ..Default::default()
                                });
                                cx.notify();
                            }),
                        ),
                    )
                })
                .when(!missing.is_empty(), |d| {
                    d.child(div().mt(px(6.)).child(heading("Missing from your songs")))
                        .children(missing.into_iter().map(|(name, songs)| {
                            fact_row(
                                Icon::Plug,
                                name,
                                match songs.len() {
                                    1 => songs[0].clone(),
                                    n => format!("{n} songs"),
                                },
                            )
                        }))
                }),
        )
    }

    /// A collection's menu: rename, colour, delete.
    pub(crate) fn collection_menu(&self, i: usize, at: gpui::Point<gpui::Pixels>) -> Menu {
        let op = |l: &str, op: PluginOp| (l.to_string(), MenuAction::Plugins(op));
        let mut items = vec![op("Rename", PluginOp::RenameCollection(i))];
        items.push(("Colour".into(), MenuAction::Nothing));
        let names = [
            "Orange", "Blue", "Green", "Pink", "Violet", "Yellow", "Sky", "Red",
        ];
        items.extend(
            COLLECTION_COLORS
                .iter()
                .zip(names)
                .map(|(&c, name)| op(&format!("  ● {name}"), PluginOp::CollectionColor(i, c))),
        );
        items.push((String::new(), MenuAction::Nothing));
        items.push(op("Delete Collection", PluginOp::DeleteCollection(i)));
        Menu {
            at,
            items,
            ..Default::default()
        }
    }

    pub(crate) fn add_to_collection(&mut self, d: usize, i: usize) {
        let key = self.catalog.devices[d].key.clone();
        if let Some(c) = self.catalog.collections.get_mut(i)
            && !c.devices.contains(&key)
        {
            c.devices.push(key);
            self.catalog.save();
        }
    }
}
