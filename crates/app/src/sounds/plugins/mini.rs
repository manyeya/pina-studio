//! The picker a mixer slot opens: the library in small, for that slot (instruments for the
//! instrument slot, effects for an insert). A search; where from (built in or plugins), the
//! starred and the recent; the categories, the kinds in the one picked and the makers, each with
//! how many it would find; then every device found, a row each with its picture and its star. Up
//! and down pick, Enter puts it in, Escape closes.

use std::collections::HashMap;

use gpui::{
    AnyElement, Div, Focusable, FontWeight, KeyDownEvent, MouseButton, ObjectFit, Pixels, Point,
    ScrollStrategy, SharedString, StyledImage, UniformListScrollHandle, anchored, deferred, div,
    img, px, rgb, uniform_list,
};
use pina_engine::song::project::Which;

use super::*;
use crate::{
    mix::strip::{Builtin, Channel},
    sounds::{
        browser::{filter_chip, link},
        catalog::{Own, instrument_of, score},
    },
    ui::{menu::MenuAction, theme::*},
};

/// A row of the list, and how many show before it scrolls.
const ROW: f32 = 52.0;
const ROWS: usize = 6;
/// The makers offered to narrow by, the most found first (the search finds any other).
const MAKERS: usize = 6;

/// The picker open on a slot: which, where; what narrows it (a category, a kind in it, a maker,
/// where from, the starred, the recent); the row picked, and the list's scroll.
pub struct Mini {
    pub channel: Channel,
    pub which: Which,
    pub at: Point<Pixels>,
    pub cat: Option<Cat>,
    pub kind: Option<&'static str>,
    pub maker: Option<String>,
    pub from: Source,
    pub starred: bool,
    pub recent: bool,
    pub picked: usize,
    pub list: UniformListScrollHandle,
}

/// Whether a device is in the category, kind and maker picked, all but `skip`'s (to count what
/// each of its chips would find).
fn fits(m: &Mini, d: &Device, skip: Chips) -> bool {
    (skip == Chips::Cats || m.cat.is_none_or(|c| d.cat == c))
        && (skip == Chips::Kinds || m.kind.is_none_or(|k| subcategory(d) == Some(k)))
        && (skip == Chips::Makers || m.maker.as_ref().is_none_or(|v| d.vendor == *v))
}

/// A chip that narrows the picker (or widens it again), with how many it would find.
fn chip(
    label: impl Into<SharedString>,
    count: Option<usize>,
    on: bool,
    dot: Option<u32>,
    id: String,
    cx: &mut Context<Workspace>,
    set: impl Fn(&mut Mini) + 'static,
) -> Div {
    filter_chip(label, count, on, dot)
        .debug_selector(move || id.clone())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, window, cx| {
                cx.stop_propagation();
                ws.mini_set(window, cx, &set);
            }),
        )
}

impl Workspace {
    /// Opens the picker on a slot.
    pub fn open_mini(
        &mut self,
        channel: Channel,
        which: Which,
        at: Point<Pixels>,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        self.plugin_page.mini = Some(Mini {
            channel,
            which,
            at,
            cat: None,
            kind: None,
            maker: None,
            from: Source::All,
            starred: false,
            recent: false,
            picked: 0,
            list: UniformListScrollHandle::new(),
        });
        let search = self.plugin_page.mini_search.clone();
        search.update(cx, |s, cx| s.set_text("", cx));
        let focus = search.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    fn mini_query(&self, cx: &gpui::App) -> String {
        let search = self.plugin_page.mini_search.read(cx);
        search.text().trim().to_lowercase()
    }

    /// What the slot takes, from where it's set to, starred and recent if asked, and matching
    /// what's typed: each with how well it matches. Every effect is offered on every channel:
    /// another of one it has stacks after the rest.
    fn mini_pool(&self, m: &Mini, cx: &gpui::App) -> Vec<(u32, usize)> {
        let instrument = m.which == Which::Instrument;
        let query = self.mini_query(cx);
        let c = &self.catalog;
        c.devices
            .iter()
            .enumerate()
            .filter(|(_, d)| d.instrument() == instrument)
            .filter(|(_, d)| match m.from {
                Source::Plugins => d.own.is_none(),
                Source::Stock => d.own.is_some(),
                _ => true,
            })
            .filter(|(_, d)| !m.starred || c.favourites.contains(&d.key))
            .filter(|(_, d)| !m.recent || c.recent.contains(&d.key))
            .filter_map(|(i, d)| match query.is_empty() {
                true => Some((0, i)),
                false => score(d, &query).map(|s| (s, i)),
            })
            .collect()
    }

    /// The devices the picker lists, best first: the best match to what's typed; then the
    /// latest used (with Recent on), else the most used; then by name.
    pub(crate) fn mini_rows(&self, cx: &gpui::App) -> Vec<usize> {
        let Some(m) = &self.plugin_page.mini else {
            return Vec::new();
        };
        let c = &self.catalog;
        let mut rows: Vec<(u32, usize)> = self
            .mini_pool(m, cx)
            .into_iter()
            .filter(|&(_, i)| fits(m, &c.devices[i], Chips::None))
            .collect();
        let used = |i: usize| c.uses.get(&c.devices[i].key).copied().unwrap_or(0);
        let latest = |i: usize| {
            let key = &c.devices[i].key;
            c.recent.iter().position(|k| k == key).unwrap_or(usize::MAX)
        };
        let name = |i: usize| c.devices[i].name.to_lowercase();
        rows.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| match m.recent {
                    true => latest(a.1).cmp(&latest(b.1)),
                    false => used(b.1).cmp(&used(a.1)),
                })
                .then_with(|| name(a.1).cmp(&name(b.1)))
        });
        rows.into_iter().map(|r| r.1).collect()
    }

    /// Changes what narrows the picker: back to its first row, the search keeping the keyboard.
    fn mini_set(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
        set: impl FnOnce(&mut Mini),
    ) {
        if let Some(m) = self.plugin_page.mini.as_mut() {
            set(m);
            m.picked = 0;
            m.list.scroll_to_item(0, ScrollStrategy::Top);
        }
        let focus = self.plugin_page.mini_search.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Everything that narrows the picker let go, what's typed too.
    fn mini_clear(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        let search = self.plugin_page.mini_search.clone();
        search.update(cx, |s, cx| s.set_text("", cx));
        self.mini_set(window, cx, |m| {
            (m.cat, m.kind, m.maker) = (None, None, None);
            (m.from, m.starred, m.recent) = (Source::All, false, false);
        });
    }

    /// Puts device `d` in the picker's slot, and closes it.
    pub(crate) fn mini_choose(&mut self, d: usize, cx: &mut Context<Self>) {
        let Some(m) = self.plugin_page.mini.take() else {
            return;
        };
        let dev = self.catalog.devices[d].clone();
        *self.catalog.uses.entry(dev.key.clone()).or_default() += 1;
        self.catalog.save();
        self.remember(&dev.key);
        let action = match (dev.own, m.channel, m.which) {
            (Some(Own::Eq), ch, _) => MenuAction::AddBuiltin {
                channel: ch,
                builtin: Builtin::Eq,
            },
            (Some(Own::Comp), ch, _) => MenuAction::AddBuiltin {
                channel: ch,
                builtin: Builtin::Comp,
            },
            (Some(Own::Sat), ch, _) => MenuAction::AddBuiltin {
                channel: ch,
                builtin: Builtin::Sat,
            },
            (Some(Own::Limit), ch, _) => MenuAction::AddBuiltin {
                channel: ch,
                builtin: Builtin::Limit,
            },
            (Some(Own::Tool(kind)), ch, _) => MenuAction::AddBuiltin {
                channel: ch,
                builtin: Builtin::Tool(kind),
            },
            (_, Channel::Track(track), Which::Instrument) => match instrument_of(&dev) {
                Some(instrument) => MenuAction::Instrument { track, instrument },
                None => return,
            },
            (None, ch, Which::Insert(index)) => MenuAction::Insert {
                channel: ch,
                index,
                plugin: dev.formats.first().map(|p| p.plugin_ref()),
            },
            _ => return,
        };
        self.choose(action, cx);
        self.refocus = true;
        cx.notify();
    }

    /// Enter: the picked row goes in.
    pub(crate) fn mini_enter(&mut self, cx: &mut Context<Self>) {
        let rows = self.mini_rows(cx);
        let picked = self.plugin_page.mini.as_ref().map_or(0, |m| m.picked);
        if let Some(&d) = rows.get(picked) {
            self.mini_choose(d, cx);
        }
    }

    fn mini_key(&mut self, e: &KeyDownEvent, cx: &mut Context<Self>) {
        let n = self.mini_rows(cx).len();
        let Some(m) = self.plugin_page.mini.as_mut() else {
            return;
        };
        match e.keystroke.key.as_str() {
            "down" | "up" => {
                m.picked = match e.keystroke.key.as_str() {
                    "down" => (m.picked + 1).min(n.saturating_sub(1)),
                    _ => m.picked.saturating_sub(1),
                };
                m.list.scroll_to_item(m.picked, ScrollStrategy::Nearest);
            }
            "escape" => {
                self.plugin_page.mini = None;
                self.refocus = true;
            }
            "enter" => self.mini_enter(cx),
            _ => {
                m.picked = 0;
                return cx.notify();
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// A row of the list: device `d`, its picture, what it is, its formats and its star.
    fn mini_item(&self, i: usize, d: usize, picked: usize, cx: &mut Context<Self>) -> AnyElement {
        let dev = &self.catalog.devices[d];
        let sub = subcategory(dev).unwrap_or(dev.cat.name());
        let thumb: AnyElement = match self
            .plugin_page
            .facts
            .get(&dev.key)
            .and_then(|f| f.picture.clone())
            .map(|n| pictures_dir().join(n))
            .filter(|p| p.exists())
        {
            Some(p) => img(p)
                .w(px(64.))
                .h(px(40.))
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            None => div()
                .w(px(64.))
                .h(px(40.))
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(crate::ui::looks::hsl(
                    super::cards::maker_hue(&dev.vendor),
                    0.28,
                    0.17,
                )))
                .child(dev.glyph(16.).text_color(rgb(dev.cat.color())))
                .into_any_element(),
        };
        let fav = self.catalog.favourites.contains(&dev.key);
        let key = dev.key.clone();
        let star = div()
            .size(px(22.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .text_size(px(13.))
            .text_color(rgb(if fav { AMBER } else { TEXT_4 }))
            .hover(|d| d.bg(rgb(RAISED)))
            .debug_selector(move || format!("mini-star-{i}"))
            .child(if fav { "★" } else { "☆" })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, window, cx| {
                    cx.stop_propagation();
                    ws.toggle_favourite(&key, cx);
                    ws.mini_set(window, cx, |_| {});
                }),
            );
        let on = i == picked;
        div()
            .h(px(ROW))
            .py(px(1.))
            .child(
                div()
                    .size_full()
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .bg(if on {
                        rgb(RAISED).into()
                    } else {
                        alpha(0, 0.0)
                    })
                    .hover(|s| s.bg(rgb(OFF)))
                    .debug_selector(move || format!("mini-{i}"))
                    .child(
                        div()
                            .rounded(px(5.))
                            .overflow_hidden()
                            .border_1()
                            .border_color(rgb(LINE_STRONG))
                            .child(thumb),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(dev.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(format!("{} · {sub}", dev.vendor)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(3.))
                            .children(dev.badges().into_iter().take(2).map(|b| {
                                div()
                                    .px(px(5.))
                                    .rounded(px(4.))
                                    .bg(rgb(OFF))
                                    .text_size(px(9.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(TEXT_3))
                                    .child(b)
                            })),
                    )
                    .child(star)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.mini_choose(d, cx);
                        }),
                    ),
            )
            .into_any_element()
    }

    /// The picker, over everything, where its slot was clicked.
    pub fn mini_view(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let m = self.plugin_page.mini.as_ref()?;
        let rows = self.mini_rows(cx);
        let pool = self.mini_pool(m, cx);
        let devices = &self.catalog.devices;
        let count = |skip: Chips, f: &dyn Fn(&Device) -> bool| {
            pool.iter()
                .map(|&(_, i)| &devices[i])
                .filter(|d| fits(m, d, skip) && f(d))
                .count()
        };
        let (at, picked, which) = (m.at, m.picked, m.which);

        // Where from, the starred and the recent.
        let from = m.from;
        let from_seg = |label: &'static str, s: Source, cx: &mut Context<Self>| {
            seg(label, from == s, 22.)
                .debug_selector(move || format!("mini-from-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, window, cx| {
                        cx.stop_propagation();
                        ws.mini_set(window, cx, |m| m.from = s);
                    }),
                )
        };
        let c = &self.catalog;
        let kind_fits = |d: &Device| d.instrument() == (which == Which::Instrument);
        let stars = devices
            .iter()
            .filter(|d| kind_fits(d) && c.favourites.contains(&d.key))
            .count();
        let recents = devices
            .iter()
            .filter(|d| kind_fits(d) && c.recent.contains(&d.key))
            .count();
        let (starred, recent) = (m.starred, m.recent);
        let where_row = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(7.))
                    .child(from_seg("All", Source::All, cx))
                    .child(from_seg("Built in", Source::Stock, cx))
                    .child(from_seg("Plugins", Source::Plugins, cx)),
            )
            .child(div().flex_1())
            .child(chip(
                "★ Starred",
                Some(stars),
                starred,
                None,
                "mini-starred".into(),
                cx,
                |m| m.starred ^= true,
            ))
            .child(chip(
                "Recent",
                Some(recents),
                recent,
                None,
                "mini-recent".into(),
                cx,
                |m| m.recent ^= true,
            ));

        // The categories with something in them, and the one picked's kinds.
        let cats = match which {
            Which::Instrument => INSTRUMENT_CATS.to_vec(),
            Which::Insert(_) => EFFECT_CATS.to_vec(),
        };
        let cat_chips: Vec<Div> = cats
            .into_iter()
            .filter_map(|k| {
                let n = count(Chips::Cats, &|d| d.cat == k);
                let on = m.cat == Some(k);
                (n > 0 || on).then(|| {
                    chip(
                        k.name(),
                        Some(n),
                        on,
                        Some(k.color()),
                        format!("mini-cat-{}", k.name()),
                        cx,
                        move |m| {
                            m.cat = (m.cat != Some(k)).then_some(k);
                            m.kind = None;
                        },
                    )
                })
            })
            .collect();
        let mut kinds: Vec<(&'static str, usize)> = Vec::new();
        if m.cat.is_some() {
            for &(_, i) in &pool {
                let d = &devices[i];
                let Some(s) = subcategory(d).filter(|_| fits(m, d, Chips::Kinds)) else {
                    continue;
                };
                match kinds.iter_mut().find(|k| k.0 == s) {
                    Some(k) => k.1 += 1,
                    None => kinds.push((s, 1)),
                }
            }
        }
        let kind = m.kind;
        let kind_chips: Vec<Div> = match kinds.len() > 1 || kind.is_some() {
            true => kinds
                .into_iter()
                .map(|(s, n)| {
                    chip(
                        s,
                        Some(n),
                        kind == Some(s),
                        None,
                        format!("mini-kind-{s}"),
                        cx,
                        move |m| m.kind = (m.kind != Some(s)).then_some(s),
                    )
                })
                .collect(),
            false => Vec::new(),
        };

        // The makers that would find the most, and the one picked.
        let mut makers: HashMap<&str, usize> = HashMap::new();
        for &(_, i) in &pool {
            let d = &devices[i];
            if fits(m, d, Chips::Makers) {
                *makers.entry(d.vendor.as_str()).or_default() += 1;
            }
        }
        let mut makers: Vec<(&str, usize)> = makers.into_iter().collect();
        makers.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let maker = m.maker.clone();
        let mut shown: Vec<(&str, usize)> = makers.iter().take(MAKERS).copied().collect();
        if let Some(v) = maker.as_deref()
            && !shown.iter().any(|s| s.0 == v)
        {
            shown.push((v, makers.iter().find(|s| s.0 == v).map_or(0, |s| s.1)));
        }
        let maker_chips: Vec<Div> = match makers.len() > 1 || maker.is_some() {
            true => shown
                .into_iter()
                .map(|(v, n)| {
                    let name = v.to_string();
                    let on = maker.as_deref() == Some(v);
                    chip(
                        name.clone(),
                        Some(n),
                        on,
                        None,
                        format!("mini-maker-{name}"),
                        cx,
                        move |m| m.maker = (m.maker.as_ref() != Some(&name)).then(|| name.clone()),
                    )
                })
                .collect(),
            false => Vec::new(),
        };

        // How many are found, and letting go of what narrows them.
        let narrowed = m.cat.is_some()
            || kind.is_some()
            || maker.is_some()
            || from != Source::All
            || starred
            || recent
            || !self.mini_query(cx).is_empty();
        let n = rows.len();
        let what = match (which, n) {
            (Which::Instrument, 1) => "instrument",
            (Which::Instrument, _) => "instruments",
            (_, 1) => "effect",
            _ => "effects",
        };
        let found = div()
            .px(px(4.))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .child(format!("{n} {what}")),
            )
            .when(narrowed, |d| {
                d.child(
                    link("Clear")
                        .debug_selector(|| "mini-clear".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, window, cx| {
                                cx.stop_propagation();
                                ws.mini_clear(window, cx);
                            }),
                        ),
                )
            });

        let list = match n {
            0 => div()
                .p(px(14.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(8.))
                .text_color(rgb(TEXT_3))
                .child("Nothing matches")
                .when(narrowed, |d| {
                    d.child(
                        button("Clear filters", false)
                            .debug_selector(|| "mini-clear-all".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, window, cx| {
                                    cx.stop_propagation();
                                    ws.mini_clear(window, cx);
                                }),
                            ),
                    )
                })
                .into_any_element(),
            n => uniform_list(
                "mini-list",
                n,
                cx.processor(move |ws, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|i| ws.mini_item(i, rows[i], picked, cx))
                        .collect::<Vec<_>>()
                }),
            )
            .track_scroll(&m.list)
            .h(px(ROW * n.min(ROWS) as f32))
            .into_any_element(),
        };
        let search = self.plugin_page.mini_search.clone();
        // A row of chips, what it narrows by on its left (the categories' dots say theirs).
        let line = |label: Option<&'static str>, chips: Vec<Div>| {
            div()
                .flex()
                .items_start()
                .gap(px(6.))
                .when_some(label, |d, l| {
                    d.child(
                        div()
                            .w(px(38.))
                            .h(px(24.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_4))
                            .child(l),
                    )
                })
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_wrap()
                        .gap(px(5.))
                        .children(chips),
                )
        };
        Some(
            deferred(
                anchored().position(at).snap_to_window().child(
                    div()
                        .id("mini-picker")
                        .w(px(460.))
                        .p(px(8.))
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .rounded(px(12.))
                        .bg(rgb(0x1c1c22))
                        .border_1()
                        .border_color(rgb(LINE_STRONG))
                        .shadow_lg()
                        .on_key_down(cx.listener(|ws, e: &KeyDownEvent, _, cx| ws.mini_key(e, cx)))
                        .on_mouse_down_out(cx.listener(|ws, _, _, cx| {
                            ws.plugin_page.mini = None;
                            ws.refocus = true;
                            cx.notify();
                        }))
                        .child(
                            div()
                                .h(px(32.))
                                .px(px(10.))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .rounded(px(8.))
                                .bg(rgb(FIELD))
                                .child(icon(Icon::Search, 14.).text_color(rgb(TEXT_3)))
                                .child(div().flex_1().child(search)),
                        )
                        .child(where_row)
                        .child(line(None, cat_chips))
                        .when(!kind_chips.is_empty(), |d| d.child(line(Some("Kind"), kind_chips)))
                        .when(!maker_chips.is_empty(), |d| d.child(line(Some("Maker"), maker_chips)))
                        .child(found)
                        .child(list)
                        .child(
                            div()
                                .px(px(4.))
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_4))
                                .child("↑↓ pick · return puts it in · esc closes · the Plugins page has them all"),
                        ),
                ),
            )
            .with_priority(2),
        )
    }
}
