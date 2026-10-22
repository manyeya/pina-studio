//! The Plugins page laid out: the search and tabs across the top, the kind, source and formats
//! under them, the sidebar (collections, categories, makers) and the library, the map or the
//! library's health; and what its menus and buttons do.

use gpui::{
    AnyElement, Bounds, Div, FontWeight, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, div,
    px, rgb,
};

use super::*;
use crate::{
    ui::{
        menu::{Menu, MenuAction},
        theme::*,
    },
    workspace::{Drag, Name, View},
};

/// Something the page's menus and buttons do.
#[derive(Clone, Debug, PartialEq)]
pub enum PluginOp {
    Load(usize, bool),
    ToggleCollection(usize, usize),
    NewCollection(Option<usize>),
    RenameCollection(usize),
    CollectionColor(usize, u32),
    DeleteCollection(usize),
    Unmap(String),
    MapCard(usize),
    Favourite(usize),
    Retake(usize),
    Reveal(PathBuf),
    Trash(Vec<PathBuf>),
    /// The picker on a mixer slot, from its menu.
    OpenMini(
        crate::mix::strip::Channel,
        pina_engine::song::project::Which,
        gpui::Point<Pixels>,
    ),
}

/// A label over a part of the sidebar or the filters: "CATEGORY", "SOURCE".
fn label(text: &'static str) -> Div {
    div()
        .text_size(px(10.5))
        .font_weight(FontWeight::BOLD)
        .text_color(rgb(TEXT_4))
        .child(text)
}

/// A pill in a group of them, picked or not.
fn pill(text: impl Into<gpui::SharedString>, on: bool) -> Div {
    div()
        .h(px(28.))
        .px(px(13.))
        .flex()
        .items_center()
        .rounded(px(14.))
        .text_size(px(12.5))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .bg(if on {
            rgb(0xf4f4f6).into()
        } else {
            alpha(0, 0.0)
        })
        .text_color(rgb(if on { 0x111114 } else { TEXT_2 }))
        .when(!on, |d| d.hover(|s| s.bg(rgb(OFF))))
        .child(text.into())
}

fn pills() -> Div {
    div()
        .p(px(3.))
        .flex()
        .items_center()
        .gap(px(2.))
        .rounded(px(17.))
        .bg(rgb(0x18181d))
        .border_1()
        .border_color(rgb(LINE))
}

/// A sidebar row: a mark, a name, a count.
fn side_row(mark: AnyElement, name: String, count: usize, on: bool, indent: f32) -> Div {
    div()
        .h(px(28.))
        .pl(px(8. + indent))
        .pr(px(8.))
        .flex()
        .items_center()
        .gap(px(9.))
        .rounded(px(7.))
        .cursor_pointer()
        .bg(if on {
            rgb(RAISED).into()
        } else {
            alpha(0, 0.0)
        })
        .when(!on, |d| d.hover(|s| s.bg(rgb(0x1a1a20))))
        .child(mark)
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .text_size(px(13.))
                .font_weight(if on {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                })
                .text_color(rgb(if on { TEXT } else { TEXT_2 }))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(name),
        )
        .child(
            div()
                .font_family(MONO)
                .text_size(px(11.))
                .text_color(rgb(TEXT_4))
                .child(format!("{count}")),
        )
}

impl Workspace {
    /// The Plugins page in the tracks' place, or back to them.
    pub fn toggle_plugins_page(&mut self, cx: &mut Context<Self>) {
        self.view = match self.view {
            View::Plugins => View::Arrange,
            _ => View::Plugins,
        };
        if self.view == View::Plugins {
            self.refresh_plugin_facts(cx);
        }
        cx.notify();
    }

    pub fn plugins_page(&mut self, cx: &mut Context<Self>) -> Div {
        let top = self.plugins_top(cx);
        let body: AnyElement = match self.plugin_page.tab {
            Tab::Health => self.health_page(cx).into_any_element(),
            tab => {
                let filters = self.plugins_filters(cx);
                let side = self.plugins_sidebar(cx);
                let main = match tab {
                    Tab::Map => self.plugins_map(cx),
                    _ => self.plugins_library(cx),
                };
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .flex_col()
                    .child(filters)
                    .child(div().flex_1().min_h(px(0.)).flex().child(side).child(main))
                    .into_any_element()
            }
        };
        div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .bg(rgb(0x111114))
            .child(top)
            .child(body)
    }

    fn plugins_top(&mut self, cx: &mut Context<Self>) -> Div {
        let tab = self.plugin_page.tab;
        let indexing = self.plugin_page.indexing.is_some();
        let tabs = [
            (Tab::Library, "Library"),
            (Tab::Map, "Map"),
            (Tab::Health, "Health"),
        ]
        .map(|(t, name)| {
            pill(name, tab == t)
                .debug_selector(move || format!("plugins-tab-{name}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.plugin_page.tab = t;
                        ws.plugin_page.armed = None;
                        if t == Tab::Health {
                            ws.refresh_plugin_facts(cx);
                        }
                        cx.notify();
                    }),
                )
        });
        let square = |i: Icon, name: &'static str, on: bool| {
            div()
                .size(px(34.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(9.))
                .border_1()
                .border_color(rgb(LINE))
                .bg(rgb(if on { RAISED } else { 0x18181d }))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(RAISED)))
                .debug_selector(move || format!("plugins-{name}"))
                .child(icon(i, 16.).text_color(rgb(TEXT_2)))
        };
        div()
            .h(px(60.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(18.))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex_1()
                    .h(px(38.))
                    .px(px(14.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .rounded(px(10.))
                    .bg(rgb(0x18181d))
                    .border_1()
                    .border_color(rgb(LINE))
                    .text_size(px(16.))
                    .child(icon(Icon::Search, 17.).text_color(rgb(TEXT_3)))
                    .child(div().flex_1().child(self.plugin_page.search.clone())),
            )
            .child(pills().children(tabs))
            .child(
                div()
                    .h(px(34.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .rounded(px(9.))
                    .border_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(if indexing { RAISED } else { 0x18181d }))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(RAISED)))
                    .text_size(px(12.5))
                    .font_weight(FontWeight::MEDIUM)
                    .debug_selector(|| "plugins-index".into())
                    .child(icon(Icon::Zoom, 15.).text_color(rgb(TEXT_2)))
                    .child(if indexing { "Stop" } else { "Take Pictures" })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.take_plugin_pictures(None, cx)),
                    ),
            )
            .child(square(Icon::Rescan, "rescan", false).on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    ws.status = Some("Looking for new plugins…".into());
                    ws.start_scan(cx);
                    ws.refresh_plugin_facts(cx);
                    cx.notify();
                }),
            ))
            .child(square(Icon::Close, "close", false).on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| ws.toggle_plugins_page(cx)),
            ))
    }

    fn plugins_filters(&mut self, cx: &mut Context<Self>) -> Div {
        let page = &self.plugin_page;
        let (kind, source, off) = (page.kind, page.source, page.formats_off.clone());
        let kinds = [
            (Kind::Effects, "Effects"),
            (Kind::Instruments, "Instruments"),
        ]
        .map(|(k, name)| {
            pill(name, kind == k)
                .debug_selector(move || format!("plugins-kind-{name}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        let page = &mut ws.plugin_page;
                        if page.kind != k {
                            (page.kind, page.cat, page.sub) = (k, None, None);
                        }
                        cx.notify();
                    }),
                )
        });
        let sources = [
            (Source::All, "All"),
            (Source::Plugins, "Plugins"),
            (Source::Stock, "Stock"),
            (Source::Saved, "Saved"),
        ]
        .map(|(s, name)| {
            pill(name, source == s)
                .debug_selector(move || format!("plugins-source-{name}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.plugin_page.source = s;
                        cx.notify();
                    }),
                )
        });
        let formats = [Format::Clap, Format::Vst3, Format::Au, Format::Vst2].map(|f| {
            let on = !off.contains(&f);
            let name = match f {
                Format::Clap => "CLAP",
                Format::Vst3 => "VST3",
                Format::Au => "AU",
                Format::Vst2 => "VST",
            };
            div()
                .h(px(26.))
                .px(px(10.))
                .flex()
                .items_center()
                .rounded(px(13.))
                .border_1()
                .border_color(rgb(if on { 0xf4f4f6 } else { LINE_STRONG }))
                .text_size(px(11.5))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(if on { TEXT } else { TEXT_4 }))
                .when(!on, |d| d.line_through())
                .cursor_pointer()
                .debug_selector(move || format!("plugins-format-{name}"))
                .child(name)
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
        });
        div()
            .h(px(54.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(16.))
            .px(px(18.))
            .child(pills().children(kinds))
            .child(label("SOURCE"))
            .child(pills().children(sources))
            .child(label("FORMAT"))
            .child(div().flex().gap(px(6.)).children(formats))
    }

    fn plugins_sidebar(&mut self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let pool = self.plugin_pool(cx);
        let page = &self.plugin_page;
        let (cat, sub, maker, collection) =
            (page.cat, page.sub, page.maker.clone(), page.collection);
        let devices = &self.catalog.devices;
        let in_pool: Vec<&Device> = pool.iter().map(|&i| &devices[i]).collect();
        // Your collections, each with what of it is shown.
        let mut yours: Vec<AnyElement> = self
            .catalog
            .collections
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let n = in_pool
                    .iter()
                    .filter(|d| c.devices.contains(&d.key))
                    .count();
                let naming = self.name_field(Name::PluginCollection(i));
                let row = match naming {
                    Some(input) => div()
                        .h(px(28.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .child(input),
                    None => side_row(
                        div()
                            .size(px(9.))
                            .rounded(px(3.))
                            .bg(rgb(c.color))
                            .into_any_element(),
                        c.name.clone(),
                        n,
                        collection == Some(i),
                        0.0,
                    ),
                };
                row.debug_selector(move || format!("plugins-collection-{i}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                            if e.click_count == 2 {
                                ws.start_naming(Name::PluginCollection(i), window, cx);
                                return;
                            }
                            let page = &mut ws.plugin_page;
                            page.collection = if page.collection == Some(i) {
                                None
                            } else {
                                Some(i)
                            };
                            cx.notify();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseUpEvent, _, cx| {
                            // A card dropped on it goes in.
                            if let Some(Drag::Device {
                                device,
                                moved: true,
                                ..
                            }) = ws.drag
                            {
                                ws.drag = None;
                                let key = ws.catalog.devices[device].key.clone();
                                if let Some(c) = ws.catalog.collections.get_mut(i)
                                    && !c.devices.contains(&key)
                                {
                                    c.devices.push(key);
                                    ws.catalog.save();
                                }
                                cx.notify();
                            }
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            let op =
                                |l: &str, op: PluginOp| (l.to_string(), MenuAction::Plugins(op));
                            let mut items = vec![op("Rename", PluginOp::RenameCollection(i))];
                            items.push(("Colour".into(), MenuAction::Nothing));
                            items.extend(COLLECTION_COLORS.iter().enumerate().map(|(k, &c)| {
                                op(
                                    &format!(
                                        "  ● {}",
                                        [
                                            "Orange", "Blue", "Green", "Pink", "Violet", "Yellow",
                                            "Sky", "Red"
                                        ][k]
                                    ),
                                    PluginOp::CollectionColor(i, c),
                                )
                            }));
                            items.push((String::new(), MenuAction::Nothing));
                            items.push(op("Delete Collection", PluginOp::DeleteCollection(i)));
                            ws.menu = Some(Menu {
                                at: e.position,
                                items,
                                ..Default::default()
                            });
                            cx.notify();
                        }),
                    )
                    .into_any_element()
            })
            .collect();
        yours.push(
            div()
                .h(px(26.))
                .px(px(8.))
                .flex()
                .items_center()
                .text_size(px(12.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_3))
                .cursor_pointer()
                .hover(|s| s.text_color(rgb(TEXT)))
                .debug_selector(|| "plugins-new-collection".into())
                .child("+ New")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, _, cx| ws.plugin_op(PluginOp::NewCollection(None), cx)),
                )
                .into_any_element(),
        );
        // The categories of the kind shown, each with what's in it; the picked one open on its
        // kinds.
        let all = side_row(
            div().into_any_element(),
            "All".into(),
            in_pool.len(),
            cat.is_none(),
            0.0,
        )
        .debug_selector(|| "plugins-cat-all".into())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|ws, _, _, cx| {
                (ws.plugin_page.cat, ws.plugin_page.sub) = (None, None);
                cx.notify();
            }),
        );
        let mut cats: Vec<AnyElement> = vec![all.into_any_element()];
        for &k in self.plugin_page.cats() {
            let of: Vec<&&Device> = in_pool.iter().filter(|d| d.cat == k).collect();
            if of.is_empty() {
                continue;
            }
            let on = cat == Some(k);
            cats.push(
                side_row(
                    k.glyph(14.).text_color(rgb(k.color())).into_any_element(),
                    k.name().into(),
                    of.len(),
                    on && sub.is_none(),
                    0.0,
                )
                .debug_selector(move || format!("plugins-cat-{}", k.name()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        let page = &mut ws.plugin_page;
                        (page.cat, page.sub) = (
                            if page.cat == Some(k) && page.sub.is_none() {
                                None
                            } else {
                                Some(k)
                            },
                            None,
                        );
                        cx.notify();
                    }),
                )
                .into_any_element(),
            );
            if on {
                let mut subs: Vec<(&'static str, usize)> = Vec::new();
                for d in &of {
                    if let Some(s) = subcategory(d) {
                        match subs.iter_mut().find(|x| x.0 == s) {
                            Some(x) => x.1 += 1,
                            None => subs.push((s, 1)),
                        }
                    }
                }
                subs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
                for (s, n) in subs {
                    cats.push(
                        side_row(div().into_any_element(), s.into(), n, sub == Some(s), 18.0)
                            .border_l_1()
                            .border_color(rgb(LINE))
                            .ml(px(16.))
                            .debug_selector(move || format!("plugins-sub-{s}"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    let page = &mut ws.plugin_page;
                                    page.sub = if page.sub == Some(s) { None } else { Some(s) };
                                    cx.notify();
                                }),
                            )
                            .into_any_element(),
                    );
                }
            }
        }
        // The makers, narrowed by what's typed over them.
        let typed = self
            .plugin_page
            .makers
            .read(cx)
            .text()
            .trim()
            .to_lowercase();
        let mut makers: Vec<(String, usize)> = Vec::new();
        for d in &in_pool {
            match makers.iter_mut().find(|m| m.0 == d.vendor) {
                Some(m) => m.1 += 1,
                None => makers.push((d.vendor.clone(), 1)),
            }
        }
        makers.retain(|m| typed.is_empty() || m.0.to_lowercase().contains(&typed));
        makers.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let maker_rows = makers.into_iter().map(|(m, n)| {
            let on = maker.as_deref() == Some(m.as_str());
            let mark = div()
                .size(px(18.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .bg(rgb(crate::ui::looks::hsl(
                    super::cards::maker_hue(&m),
                    0.45,
                    0.32,
                )))
                .text_size(px(8.5))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(0xffffff))
                .child(initials(&m));
            let key = m.clone();
            side_row(mark.into_any_element(), m, n, on, 0.0).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    let page = &mut ws.plugin_page;
                    page.maker = if page.maker.as_deref() == Some(key.as_str()) {
                        None
                    } else {
                        Some(key.clone())
                    };
                    cx.notify();
                }),
            )
        });
        div()
            .id("plugins-sidebar")
            .w(px(250.))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .px(px(12.))
            .pb(px(20.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(div().pt(px(8.)).pb(px(6.)).px(px(8.)).child(label("YOURS")))
            .children(yours)
            .child(
                div()
                    .pt(px(16.))
                    .pb(px(6.))
                    .px(px(8.))
                    .child(label("CATEGORY")),
            )
            .children(cats)
            .child(
                div()
                    .pt(px(16.))
                    .pb(px(6.))
                    .px(px(8.))
                    .child(label("MAKER")),
            )
            .child(
                div()
                    .h(px(30.))
                    .mx(px(4.))
                    .mb(px(6.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .rounded(px(8.))
                    .bg(rgb(0x1c1c22))
                    .text_size(px(12.))
                    .child(self.plugin_page.makers.clone()),
            )
            .children(maker_rows)
    }

    /// Over the cards: where the library is (All › Dynamics · Compressor) and how many, Health's
    /// set if one's shown, and the sort.
    fn plugins_bar(&mut self, n: usize, cx: &mut Context<Self>) -> Div {
        let page = &self.plugin_page;
        let mut trail = vec!["All".to_string()];
        if let Some(c) = page
            .collection
            .and_then(|c| self.catalog.collections.get(c))
        {
            trail.push(c.name.clone());
        }
        if let Some(c) = page.cat {
            trail.push(match page.sub {
                Some(s) => format!("{} · {s}", c.name()),
                None => c.name().to_string(),
            });
        }
        if let Some(m) = &page.maker {
            trail.push(m.clone());
        }
        let (sort, descending) = (page.sort, page.descending);
        let sorts = [
            (Sort::Name, "Name"),
            (Sort::Used, "Used"),
            (Sort::Cpu, "CPU"),
        ]
        .map(|(s, name)| {
            pill(name, sort == s)
                .h(px(24.))
                .debug_selector(move || format!("plugins-sort-{name}"))
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
        });
        div()
            .h(px(40.))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(trail.join("  ›  ")),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_3))
                    .child(format!("{n}")),
            )
            .children(page.set.map(|s| {
                div()
                    .h(px(24.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(12.))
                    .bg(alpha(ACCENT, 0.18))
                    .text_size(px(12.))
                    .text_color(rgb(ACCENT_TEXT))
                    .cursor_pointer()
                    .child(s.name())
                    .child("✕")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.plugin_page.set = None;
                            cx.notify();
                        }),
                    )
            }))
            .child(div().flex_1())
            .child(label("SORT BY"))
            .child(pills().children(sorts))
            .child(
                div()
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .child(
                        icon(
                            if descending {
                                Icon::ChevronUp
                            } else {
                                Icon::Chevron
                            },
                            14.,
                        )
                        .text_color(rgb(TEXT_2)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.plugin_page.descending ^= true;
                            cx.notify();
                        }),
                    ),
            )
    }

    /// Pictures being taken, or none yet: what it is, and the button.
    fn pictures_banner(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let page = &self.plugin_page;
        let missing = self
            .catalog
            .devices
            .iter()
            .filter(|d| d.own.is_none())
            .filter(|d| page.facts.get(&d.key).is_none_or(|f| f.wants_picture()))
            .count();
        let (text, progress) = match &page.indexing {
            Some(i) => (
                format!(
                    "Taking pictures: {} of {}{}",
                    i.total - i.todo.len(),
                    i.total,
                    i.now.as_ref().map_or(String::new(), |n| format!(" · {n}")),
                ),
                Some((i.total - i.todo.len()) as f32 / i.total.max(1) as f32),
            ),
            None if missing > 0 => (
                format!(
                    "{missing} plugins have no picture yet. Pina Studio opens each once to photograph its window, and times how hard it works the computer."
                ),
                None,
            ),
            None => return None,
        };
        let running = progress.is_some();
        Some(
            div()
                .mb(px(14.))
                .p(px(12.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .rounded(px(10.))
                .bg(rgb(0x18181d))
                .border_1()
                .border_color(rgb(LINE))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(12.5))
                                .text_color(rgb(TEXT_2))
                                .child(text),
                        )
                        .child(
                            crate::ui::theme::button(
                                if running { "Stop" } else { "Take Pictures" },
                                false,
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| ws.take_plugin_pictures(None, cx)),
                            ),
                        ),
                )
                .children(progress.map(|p| {
                    div().h(px(4.)).rounded(px(2.)).bg(rgb(RAISED)).child(
                        div()
                            .h_full()
                            .w(gpui::relative(p.max(0.02)))
                            .rounded(px(2.))
                            .bg(rgb(ACCENT)),
                    )
                })),
        )
    }

    fn plugins_library(&mut self, cx: &mut Context<Self>) -> Div {
        let cards = self.plugin_cards(cx);
        let bar = self.plugins_bar(cards.len(), cx);
        let banner = self.pictures_banner(cx);
        let empty = cards.is_empty();
        let grid = self.plugin_grid(cards, cx);
        let width = self.plugin_page.width.clone();
        div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .px(px(22.))
            .child(bar)
            .child(
                div()
                    .id("plugins-grid")
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .pb(px(30.))
                    .child(
                        gpui::canvas(
                            move |b: Bounds<Pixels>, _, _| width.set(f32::from(b.size.width)),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
                    .children(banner)
                    .child(grid)
                    .when(empty, |d| {
                        d.child(
                            div()
                                .pt(px(60.))
                                .flex()
                                .justify_center()
                                .text_color(rgb(TEXT_3))
                                .child("Nothing here: change the filters, or the search."),
                        )
                    }),
            )
    }

    fn plugins_map(&mut self, cx: &mut Context<Self>) -> Div {
        let keyboard = self.plugin_keyboard(cx);
        let cards = self.plugin_cards(cx);
        let bar = self.plugins_bar(cards.len(), cx);
        let grid = self.plugin_grid(cards, cx);
        let width = self.plugin_page.width.clone();
        let armed = self.plugin_page.armed.clone();
        div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .px(px(22.))
            .child(
                div()
                    .h(px(40.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .h(px(24.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .rounded(px(12.))
                            .border_1()
                            .border_color(rgb(0xd9a35a))
                            .text_size(px(11.5))
                            .text_color(rgb(0xd9a35a))
                            .child("Mappings paused while you're here"),
                    )
                    .child(
                        div().text_size(px(12.5)).text_color(rgb(TEXT_3)).child(match armed {
                            Some(k) => format!("{} picked: click a card to put on it", super::map::key_label(&k)),
                            None => "Press a key, click one, click a card and then a key, or drag a card onto one. Keys Pina Studio uses are marked in orange.".into(),
                        }),
                    ),
            )
            .child(
                div()
                    .id("plugins-map")
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .child(
                        gpui::canvas(
                            move |b: Bounds<Pixels>, _, _| width.set(f32::from(b.size.width)),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
                    .child(div().flex().justify_center().pb(px(18.)).child(keyboard))
                    .child(bar)
                    .child(grid),
            )
    }

    /// What the page's menus and buttons do.
    pub fn plugin_op(&mut self, op: PluginOp, cx: &mut Context<Self>) {
        match op {
            PluginOp::Load(d, new) => self.load_plugin_card(d, None, new, cx),
            PluginOp::ToggleCollection(d, i) => {
                let key = self.catalog.devices[d].key.clone();
                if let Some(c) = self.catalog.collections.get_mut(i) {
                    match c.devices.iter().position(|k| *k == key) {
                        Some(at) => drop(c.devices.remove(at)),
                        None => c.devices.push(key),
                    }
                }
                self.catalog.save();
            }
            PluginOp::NewCollection(with) => {
                let n = self.catalog.collections.len();
                let devices = with
                    .map(|d| vec![self.catalog.devices[d].key.clone()])
                    .unwrap_or_default();
                self.catalog.collections.push(Collection {
                    name: format!("Collection {}", n + 1),
                    color: COLLECTION_COLORS[n % COLLECTION_COLORS.len()],
                    devices,
                });
                self.catalog.save();
                self.plugin_page.ask = Some(Name::PluginCollection(n));
            }
            PluginOp::RenameCollection(i) => self.plugin_page.ask = Some(Name::PluginCollection(i)),
            PluginOp::CollectionColor(i, color) => {
                if let Some(c) = self.catalog.collections.get_mut(i) {
                    c.color = color;
                }
                self.catalog.save();
            }
            PluginOp::DeleteCollection(i) => {
                if i < self.catalog.collections.len() {
                    self.catalog.collections.remove(i);
                    self.plugin_page.collection = None;
                }
                self.catalog.save();
            }
            PluginOp::Unmap(k) => {
                self.catalog.keys.retain(|(key, _)| *key != k);
                self.catalog.save();
            }
            PluginOp::MapCard(d) => {
                self.plugin_page.picked = Some(self.catalog.devices[d].key.clone());
                self.plugin_page.tab = Tab::Map;
                self.status = Some("Press the key to put it on, or click one.".into());
            }
            PluginOp::Favourite(d) => {
                let key = self.catalog.devices[d].key.clone();
                self.toggle_favourite(&key, cx);
            }
            PluginOp::Retake(d) => {
                let key = self.catalog.devices[d].key.clone();
                if let Some(f) = self.plugin_page.facts.get_mut(&key) {
                    f.failed = None;
                }
                self.plugin_page.indexing = None;
                self.take_plugin_pictures(Some(vec![key]), cx);
            }
            PluginOp::Reveal(path) => {
                let _ = std::process::Command::new("open")
                    .arg("-R")
                    .arg(&path)
                    .spawn();
            }
            PluginOp::OpenMini(ch, which, at) => {
                self.plugin_page.mini_pending = Some((ch, which, at))
            }
            PluginOp::Trash(paths) => {
                let mut moved = 0;
                let mut kept = Vec::new();
                for p in &paths {
                    match crate::pages::projects::files::trash(p) {
                        Ok(()) => moved += 1,
                        Err(e) => kept.push(format!("{}: {e}", p.display())),
                    }
                }
                self.status = Some(match kept.is_empty() {
                    true => format!("Moved {moved} plugin files to the Trash"),
                    false => format!(
                        "Moved {moved} to the Trash; {} couldn't be: {}",
                        kept.len(),
                        kept.join("; ")
                    ),
                });
                self.start_scan(cx);
                self.refresh_plugin_facts(cx);
            }
        }
        cx.notify();
    }
}
