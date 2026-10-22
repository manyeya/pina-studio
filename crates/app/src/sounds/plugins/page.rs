//! The Plugins page laid out as the sample library's: its bar (what it is, Grid, List or Keys,
//! Compact), the filters, what's found and the one picked; how it opens from the Browser and goes
//! back; and what its menus and buttons do.

use std::ops::Range;

use gpui::{
    Bounds, Div, FontWeight, MouseButton, MouseDownEvent, Pixels, div, px, rgb, uniform_list,
};

use super::*;
use crate::{
    sounds::browser::ROW_H,
    ui::theme::*,
    workspace::{Drag, Name, View, pt},
};

/// The list's columns: the kind, formats, load, size and songs.
const KIND_W: f32 = 110.0;
const FORMATS_W: f32 = 76.0;
const CPU_W: f32 = 48.0;
const SIZE_W: f32 = 58.0;
const SONGS_W: f32 = 44.0;

/// Something the page's menus and buttons do.
#[derive(Clone, Debug, PartialEq)]
pub enum PluginOp {
    Load(usize, bool),
    /// On a track (none: a new one), as a preset or not.
    AddTo(usize, Option<usize>, Option<Preset>),
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

impl Workspace {
    /// The Plugins page in the tracks' place (on the Browser's tab, all if it was on Sounds), or
    /// back to them.
    pub fn toggle_plugins_page(&mut self, cx: &mut Context<Self>) {
        match self.view {
            View::Plugins => self.view = View::Arrange,
            _ => self.open_plugins_page(cx),
        }
        cx.notify();
    }

    /// The Browser's instruments and effects as a page, the search and tab coming along.
    pub fn open_plugins_page(&mut self, cx: &mut Context<Self>) {
        if self.catalog.scope == Scope::Sounds {
            self.catalog.scope = Scope::All;
        }
        self.view = View::Plugins;
        self.refresh_plugin_facts(cx);
        cx.notify();
    }

    /// Compact: the tracks again, the Browser beside them on the page's tab.
    pub fn compact_plugins(&mut self, cx: &mut Context<Self>) {
        self.view = View::Arrange;
        self.show_browser = true;
        self.show_agent = false;
        cx.notify();
    }

    /// The bar over the page: what it is and how many, Grid, List or Keys, and Compact.
    fn plugins_bar(&mut self, cx: &mut Context<Self>) -> Div {
        let shown = self.plugin_page.shown;
        let view = |label: &'static str, s: Shown, cx: &mut Context<Self>| {
            seg(label, shown == s, 24.)
                .debug_selector(move || format!("plugins-view-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.plugin_page.shown = s;
                        ws.plugin_page.armed = None;
                        cx.notify();
                    }),
                )
        };
        let detail = match &self.plugin_page.indexing {
            Some(i) => format!("Pictures {} of {}", i.total - i.todo.len(), i.total),
            None => format!("{} devices", self.catalog.devices.len()),
        };
        div()
            .h(px(40.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(16.))
            .bg(rgb(SURFACE))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Plugins"),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(detail),
            )
            .child(div().flex_1())
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(8.))
                    .child(view("Grid", Shown::Grid, cx))
                    .child(view("List", Shown::List, cx))
                    .child(view("Keys", Shown::Keys, cx)),
            )
            .child(
                button("Compact", false)
                    .child(icon(Icon::PanelRight, 13.).text_color(rgb(TEXT_3)))
                    .flex_row_reverse()
                    .debug_selector(|| "plugins-compact".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.compact_plugins(cx)),
                    ),
            )
    }

    pub fn plugins_page(&mut self, cx: &mut Context<Self>) -> Div {
        self.plugin_page.cards = self.plugin_cards(cx);
        let bar = self.plugins_bar(cx);
        let middle = match self.plugin_page.shown {
            Shown::Grid => self.plugin_results(false, cx),
            Shown::List => self.plugin_results(true, cx),
            Shown::Keys => self.plugin_keys(cx),
        };
        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .flex_col()
            .child(bar)
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .bg(rgb(LANE))
                    .child(self.plugin_filters(cx))
                    .child(div().flex_1().min_w(px(0.)).flex().flex_col().child(middle))
                    .child(self.plugin_details(cx)),
            )
    }

    /// Over what's found: how many, in what order; for the list, its columns.
    fn plugin_results_header(&self, columns: bool, cx: &gpui::App) -> Div {
        let n = self.plugin_page.cards.len();
        let what = match (self.plugin_page.source, self.catalog.scope) {
            (Source::Saved, _) => ("preset", "presets"),
            (_, Scope::Instruments) => ("instrument", "instruments"),
            (_, Scope::Effects) => ("effect", "effects"),
            _ => ("device", "devices"),
        };
        let order = match (self.plugin_page.sort, self.plugin_page.descending) {
            (Sort::Name, _) if !self.catalog_query(cx).is_empty() => "best match first",
            (Sort::Name, false) => "by name",
            (Sort::Name, true) => "by name, Z to A",
            (Sort::Used, false) => "most used first",
            (Sort::Used, true) => "least used first",
            (Sort::Cpu, false) => "lightest first",
            (Sort::Cpu, true) => "heaviest first",
        };
        let col = |label: &'static str, w: f32| {
            div()
                .w(px(w))
                .flex_none()
                .text_size(px(10.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_4))
                .child(label.to_uppercase())
        };
        div()
            .h(px(34.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(12.))
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(6.))
                    .whitespace_nowrap()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{n} {}", if n == 1 { what.0 } else { what.1 })),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child(order),
                    ),
            )
            .child(div().flex_1())
            .when(columns && n > 0, |d| {
                d.child(col("Kind", KIND_W))
                    .child(col("Format", FORMATS_W))
                    .child(col("CPU", CPU_W))
                    .child(col("Size", SIZE_W))
                    .child(col("Songs", SONGS_W))
            })
    }

    /// Nothing found: says why, and loosens what's narrowing it.
    fn plugins_empty(&self, cx: &mut Context<Self>) -> Div {
        let page = &self.plugin_page;
        let narrowed = !page.cats.is_empty()
            || !page.kinds.is_empty()
            || !page.makers.is_empty()
            || page.collection.is_some()
            || page.set.is_some()
            || page.starred
            || page.source != Source::All
            || !page.formats_off.is_empty();
        let typed = !self.catalog_query(cx).is_empty();
        let (title, body) = match (self.catalog.devices.is_empty(), narrowed || typed) {
            (true, _) => (
                "Looking for your plugins",
                "Every plugin installed shows here once it's found.",
            ),
            (false, _) => (
                "Nothing found",
                "Nothing matches all of these filters. Loosen them to see more:",
            ),
        };
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .p(px(24.))
            .child(
                div()
                    .size(px(40.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(12.))
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(LINE))
                    .child(icon(Icon::Plug, 20.).text_color(rgb(TEXT_3))),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .max_w(px(420.))
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_3))
                    .text_center()
                    .child(body),
            )
            .when(narrowed || typed, |d| {
                d.child(
                    button("Show everything", false)
                        .mt(px(4.))
                        .debug_selector(|| "plugins-loosen".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                let page = &mut ws.plugin_page;
                                (page.cats, page.kinds, page.makers) =
                                    (Vec::new(), Vec::new(), Vec::new());
                                (page.collection, page.set, page.starred) = (None, None, false);
                                page.source = Source::All;
                                page.formats_off.clear();
                                ws.catalog.search.update(cx, |i, cx| i.set_text("", cx));
                                cx.notify();
                            }),
                        ),
                )
            })
    }

    /// What's found, as pictures or as a list, with the line under it.
    fn plugin_results(&mut self, list: bool, cx: &mut Context<Self>) -> Div {
        let n = self.plugin_page.cards.len();
        let width = self.plugin_page.width.clone();
        let body = match (n, list) {
            (0, _) => self.plugins_empty(cx).into_any_element(),
            (n, true) => uniform_list(
                "plugin-list",
                n,
                cx.processor(|ws, range: Range<usize>, _, cx| ws.plugin_rows(range, cx)),
            )
            .track_scroll(&self.plugin_page.list)
            .flex_1()
            .into_any_element(),
            (_, false) => {
                let cards = self.plugin_page.cards.clone();
                div()
                    .id("plugin-grid")
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .p(px(14.))
                    .child(measure(width))
                    .child(self.plugin_grid(cards, cx))
                    .into_any_element()
            }
        };
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(self.plugin_results_header(list, cx))
            .child(body)
            .child(self.plugin_info_line())
    }

    /// Keys: the keyboard, then what's found to put on it.
    fn plugin_keys(&mut self, cx: &mut Context<Self>) -> Div {
        let keyboard = self.plugin_keyboard(cx);
        let cards = self.plugin_page.cards.clone();
        let width = self.plugin_page.width.clone();
        let mapped = self.catalog.keys.len();
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(34.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(12.))
                    .border_b_1()
                    .border_color(rgb(LINE_SOFT))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(6.))
                            .whitespace_nowrap()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(match mapped {
                                        1 => "1 key".to_string(),
                                        n => format!("{n} keys"),
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .child("each puts its plugin on the selected track"),
                            ),
                    )
                    .child(div().flex_1())
                    .child(chip("Keys wait while this is open", TEXT_3)),
            )
            .child(
                div()
                    .id("plugin-keys")
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .p(px(14.))
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(measure(width))
                    .child(keyboard)
                    .child(self.plugin_grid(cards, cx)),
            )
            .child(self.plugin_info_line())
    }

    /// Rows of the list, as the list draws them.
    fn plugin_rows(
        &mut self,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::Stateful<Div>> {
        range
            .filter_map(|pos| {
                let card = self.plugin_page.cards.get(pos)?.clone();
                Some(self.plugin_row(pos, &card, cx))
            })
            .collect()
    }

    fn plugin_row(&self, pos: usize, card: &Card, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let d = card.device();
        let dev = &self.catalog.devices[d];
        let page = &self.plugin_page;
        let selected = page.picked.as_deref() == Some(dev.key.as_str())
            && page.preset.as_ref() == card_preset(card);
        let f = page.facts.get(&dev.key);
        let failed = f.is_some_and(|f| f.failed.is_some());
        let mono = |s: String, w: f32, c: u32| {
            div()
                .w(px(w))
                .flex_none()
                .font_family(MONO)
                .text_size(px(11.5))
                .text_color(rgb(c))
                .whitespace_nowrap()
                .overflow_hidden()
                .child(s)
        };
        let (title, maker) = match card {
            Card::Preset(_, p) => (p.name.clone(), dev.name.clone()),
            Card::Device(_) => (
                dev.name.clone(),
                match dev.own {
                    Some(_) => "Pina Studio".to_string(),
                    None => dev.vendor.clone(),
                },
            ),
        };
        let songs = page
            .songs
            .as_ref()
            .and_then(|s| s.used.get(&dev.key).copied());
        let size = self.device_size(dev);
        let key = dev.key.clone();
        let thumb = self.card_art(dev, 64., 36.);
        div()
            .id(("plugin-row", pos))
            .group("plugin-row")
            .w_full()
            .h(px(ROW_H))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(10.))
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
            .debug_selector(move || format!("plugin-row-{pos}"))
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
                    .w(px(64.))
                    .h(px(36.))
                    .flex_none()
                    .rounded(px(5.))
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgb(LINE))
                    .when(failed, |d| d.opacity(0.45))
                    .child(thumb),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(if selected { TEXT } else { TEXT_2 }))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(title),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(chip(dev.cat.name(), dev.cat.color()).h(px(16.)).px(px(5.)))
                            .when(failed, |d| {
                                d.child(chip("Won't load", RECORD).h(px(16.)).px(px(5.)))
                            })
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_3))
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(maker),
                            ),
                    ),
            )
            .child(
                div()
                    .w(px(KIND_W))
                    .flex_none()
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT_2))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(subcategory(dev).unwrap_or(dev.cat.name())),
            )
            .child(mono(
                match dev.own {
                    Some(_) => "Built in".into(),
                    None => dev.badges().join(" "),
                },
                FORMATS_W,
                TEXT_3,
            ))
            .child(mono(
                f.and_then(|f| f.load)
                    .map_or("—".into(), |l| format!("{:.1}%", l * 100.0)),
                CPU_W,
                TEXT_2,
            ))
            .child(mono(
                match size {
                    0 => "—".into(),
                    s => size_text(s),
                },
                SIZE_W,
                TEXT_3,
            ))
            .child(mono(
                songs.map_or("—".into(), |n| n.to_string()),
                SONGS_W,
                if songs.is_some() { TEXT_2 } else { TEXT_4 },
            ))
            .on_hover(cx.listener(move |ws, over: &bool, _, cx| {
                ws.hover_plugin(&key, *over, cx);
            }))
            .on_mouse_down(MouseButton::Left, self.plugin_press(card.clone(), cx))
            .on_mouse_down(MouseButton::Right, self.plugin_menu_press(d, cx))
    }

    /// Under what's found: the plugin under the pointer (else the one picked) in a line.
    fn plugin_info_line(&self) -> Div {
        let page = &self.plugin_page;
        let shown = page
            .hovered
            .as_deref()
            .or(page.picked.as_deref())
            .and_then(|k| self.catalog.device(k));
        let armed = page.armed.as_deref().map(key_label);
        let track = self
            .session
            .project
            .tracks
            .get(self.track)
            .map_or("a new track".to_string(), |t| t.name.clone());
        div()
            .h(px(28.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .border_t_1()
            .border_color(rgb(LINE_SOFT))
            .bg(rgb(SURFACE))
            .text_size(px(11.))
            .whitespace_nowrap()
            .overflow_hidden()
            .children(armed.clone().map(|k| {
                div()
                    .text_color(rgb(ACCENT_TEXT))
                    .child(format!("{k} picked: click a plugin to put on it"))
            }))
            .when(armed.is_none(), |d| {
                d.children(shown.map(|i| {
                    let dev = &self.catalog.devices[i];
                    let mut parts = vec![match dev.own {
                        Some(_) => "Pina Studio".to_string(),
                        None => dev.vendor.clone(),
                    }];
                    parts.push(match subcategory(dev) {
                        Some(k) => format!("{} · {k}", dev.cat.name()),
                        None => dev.cat.name().to_string(),
                    });
                    if dev.own.is_none() {
                        parts.push(dev.badges().join(" "));
                    }
                    if let Some(l) = page.facts.get(&dev.key).and_then(|f| f.load) {
                        parts.push(format!("CPU {:.1}%", l * 100.0));
                    }
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .min_w(px(0.))
                        .child(
                            div()
                                .size(px(6.))
                                .flex_none()
                                .rounded(px(3.))
                                .bg(rgb(dev.cat.color())),
                        )
                        .child(
                            div()
                                .text_color(rgb(TEXT))
                                .font_weight(FontWeight::MEDIUM)
                                .child(dev.name.clone()),
                        )
                        .child(
                            div()
                                .text_color(rgb(TEXT_2))
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(parts.join(" · ")),
                        )
                }))
                .when(shown.is_none(), |d| {
                    d.text_color(rgb(TEXT_4)).child(format!(
                        "Click a plugin to see it · double-click puts it on {track} · drag one out onto a track"
                    ))
                })
            })
    }

    /// A plugin under the pointer, or not any more.
    pub(crate) fn hover_plugin(&mut self, key: &str, over: bool, cx: &mut Context<Self>) {
        let now = over.then(|| key.to_string());
        let page = &mut self.plugin_page;
        if page.hovered != now && (over || page.hovered.as_deref() == Some(key)) {
            page.hovered = now;
            cx.notify();
        }
    }

    /// Pressed on a plugin: picked (on Keys, put on the key waiting), a double-click puts it on
    /// the selected track (⌥: a new one), and it's ready to drag.
    pub(crate) fn plugin_press(
        &self,
        card: Card,
        cx: &mut Context<Self>,
    ) -> impl Fn(&MouseDownEvent, &mut gpui::Window, &mut gpui::App) + 'static {
        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
            let d = card.device();
            let key = ws.catalog.devices[d].key.clone();
            let preset = card_preset(&card).cloned();
            if ws.plugin_page.shown == Shown::Keys
                && let Some(k) = ws.plugin_page.armed.take()
            {
                ws.map_key(&k, &key);
            }
            ws.plugin_page.picked = Some(key);
            ws.plugin_page.preset = preset.clone();
            if e.click_count == 2 {
                ws.load_plugin_card(d, preset, e.modifiers.alt, cx);
                return;
            }
            let at = pt(e.position);
            ws.drag = Some(Drag::Device {
                device: d,
                from: at,
                at,
                moved: false,
            });
            cx.notify();
        })
    }

    /// Right-pressed on a plugin: picked, and its menu.
    pub(crate) fn plugin_menu_press(
        &self,
        d: usize,
        cx: &mut Context<Self>,
    ) -> impl Fn(&MouseDownEvent, &mut gpui::Window, &mut gpui::App) + 'static {
        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
            ws.plugin_page.picked = Some(ws.catalog.devices[d].key.clone());
            ws.plugin_page.preset = None;
            ws.menu = Some(crate::ui::menu::Menu {
                at: e.position,
                items: ws.card_menu(d),
                ..Default::default()
            });
            cx.notify();
        })
    }

    /// ↑ ↓ (and ← → on the pictures): the next plugin picked, scrolled to in the list.
    pub fn step_plugin(&mut self, by: isize, cx: &mut Context<Self>) {
        let page = &self.plugin_page;
        let n = page.cards.len();
        if n == 0 {
            return;
        }
        let at = page.cards.iter().position(|c| {
            page.picked.as_deref() == Some(self.catalog.devices[c.device()].key.as_str())
                && page.preset.as_ref() == card_preset(c)
        });
        let next = match at {
            Some(at) => (at as isize + by).clamp(0, n as isize - 1) as usize,
            None if by > 0 => 0,
            None => n - 1,
        };
        let card = page.cards[next].clone();
        self.plugin_page.picked = Some(self.catalog.devices[card.device()].key.clone());
        self.plugin_page.preset = card_preset(&card).cloned();
        self.plugin_page
            .list
            .scroll_to_item(next, gpui::ScrollStrategy::Center);
        cx.notify();
    }

    /// Enter: the plugin picked goes on the selected track.
    pub fn add_picked_plugin(&mut self, cx: &mut Context<Self>) {
        let d = self.picked_plugin().or_else(|| {
            // Typed and Enter: the best match.
            self.plugin_page.cards.first().map(|c| c.device())
        });
        if let Some(d) = d {
            let preset = self.plugin_page.preset.clone();
            self.load_plugin_card(d, preset, false, cx);
        }
    }

    /// What the page's menus and buttons do.
    pub fn plugin_op(&mut self, op: PluginOp, cx: &mut Context<Self>) {
        match op {
            PluginOp::Load(d, new) => self.load_plugin_card(d, None, new, cx),
            PluginOp::AddTo(d, track, preset) => match track {
                Some(t) => {
                    self.track = t;
                    self.load_plugin_card(d, preset, false, cx)
                }
                None => self.load_plugin_card(d, preset, true, cx),
            },
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
                self.plugin_page.shown = Shown::Keys;
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
            }
        }
        cx.notify();
    }
}

/// A card's preset, if it's one.
pub(crate) fn card_preset(card: &Card) -> Option<&Preset> {
    match card {
        Card::Preset(_, p) => Some(p),
        Card::Device(_) => None,
    }
}

/// Records how wide its parent is drawn, for the pictures' columns.
fn measure(width: Rc<Cell<f32>>) -> impl IntoElement {
    gpui::canvas(
        move |b: Bounds<Pixels>, _, _| width.set(f32::from(b.size.width) - 28.0),
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}
