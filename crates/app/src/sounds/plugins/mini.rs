//! The picker a mixer slot opens: the library in small, for that slot (instruments for the
//! instrument slot, effects for an insert): a search, the categories, and a row a device with its
//! picture. Up and down pick, Enter puts it in, Escape closes.

use gpui::{
    AnyElement, Focusable, FontWeight, KeyDownEvent, MouseButton, ObjectFit, Pixels, Point,
    StyledImage, anchored, deferred, div, img, px, rgb,
};
use pina_engine::song::project::Which;

use super::*;
use crate::{
    mix::strip::{Builtin, Channel},
    sounds::catalog::{Own, instrument_of},
    ui::{menu::MenuAction, theme::*},
};

/// The picker open on a slot: which, where, the category picked, the row picked.
pub struct Mini {
    pub channel: Channel,
    pub which: Which,
    pub at: Point<Pixels>,
    pub cat: Option<Cat>,
    pub picked: usize,
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
            picked: 0,
        });
        let search = self.plugin_page.mini_search.clone();
        search.update(cx, |s, cx| s.set_text("", cx));
        let focus = search.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    /// The devices the picker lists, best first.
    fn mini_rows(&self, cx: &gpui::App) -> Vec<usize> {
        let Some(m) = &self.plugin_page.mini else {
            return Vec::new();
        };
        let instrument = m.which == Which::Instrument;
        let query = self
            .plugin_page
            .mini_search
            .read(cx)
            .text()
            .trim()
            .to_lowercase();
        // A channel's built-ins are only offered where it hasn't them yet.
        let (eq, comp) = self.strip_dsp(m.channel);
        let sat = self.sat(m.channel);
        let mut rows: Vec<(u32, usize)> = self
            .catalog
            .devices
            .iter()
            .enumerate()
            .filter(|(_, d)| d.instrument() == instrument)
            .filter(|(_, d)| match d.own {
                Some(Own::Eq) => eq.is_none(),
                Some(Own::Comp) => comp.is_none(),
                Some(Own::Sat) => sat.is_none(),
                Some(_) => matches!(m.channel, Channel::Track(_)),
                None => true,
            })
            .filter(|(_, d)| m.cat.is_none_or(|c| d.cat == c))
            .filter_map(|(i, d)| {
                let s = match query.is_empty() {
                    true => 0,
                    false => crate::sounds::catalog::score(d, &query)?,
                };
                let used = self.catalog.uses.get(&d.key).copied().unwrap_or(0);
                Some((s * 1000 + used.min(999), i))
            })
            .collect();
        rows.sort_by(|a, b| {
            b.0.cmp(&a.0).then_with(|| {
                self.catalog.devices[a.1]
                    .name
                    .to_lowercase()
                    .cmp(&self.catalog.devices[b.1].name.to_lowercase())
            })
        });
        rows.into_iter().map(|r| r.1).collect()
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
            "down" => m.picked = (m.picked + 1).min(n.saturating_sub(1)),
            "up" => m.picked = m.picked.saturating_sub(1),
            "escape" => {
                self.plugin_page.mini = None;
                self.refocus = true;
            }
            "enter" => {
                let rows = self.mini_rows(cx);
                let picked = self.plugin_page.mini.as_ref().map_or(0, |m| m.picked);
                if let Some(&d) = rows.get(picked) {
                    self.mini_choose(d, cx);
                }
            }
            _ => {
                m.picked = 0;
                return cx.notify();
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// The picker, over everything, where its slot was clicked.
    pub fn mini_view(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let m = self.plugin_page.mini.as_ref()?;
        let (at, cat, picked, which) = (m.at, m.cat, m.picked, m.which);
        let rows = self.mini_rows(cx);
        let cats: Vec<Cat> = match which {
            Which::Instrument => INSTRUMENT_CATS.to_vec(),
            Which::Insert(_) => EFFECT_CATS.to_vec(),
        };
        let chips = cats.into_iter().map(|k| {
            let on = cat == Some(k);
            div()
                .h(px(22.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(4.))
                .rounded(px(11.))
                .text_size(px(11.))
                .cursor_pointer()
                .bg(if on {
                    alpha(k.color(), 0.2)
                } else {
                    rgb(CARD).into()
                })
                .text_color(rgb(if on { TEXT } else { TEXT_2 }))
                .child(k.glyph(11.).text_color(rgb(k.color())))
                .child(k.name())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(m) = ws.plugin_page.mini.as_mut() {
                            m.cat = if m.cat == Some(k) { None } else { Some(k) };
                            m.picked = 0;
                        }
                        cx.notify();
                    }),
                )
        });
        let list: Vec<AnyElement> =
            rows.iter()
                .take(60)
                .enumerate()
                .map(|(i, &d)| {
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
                            .child(dev.cat.glyph(16.).text_color(rgb(dev.cat.color())))
                            .into_any_element(),
                    };
                    let on = i == picked;
                    div()
                        .h(px(50.))
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
                        .child(div().flex().gap(px(3.)).children(
                            dev.badges().into_iter().take(2).map(|b| {
                                div()
                                    .px(px(5.))
                                    .rounded(px(4.))
                                    .bg(rgb(OFF))
                                    .text_size(px(9.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(TEXT_3))
                                    .child(b)
                            }),
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.mini_choose(d, cx);
                            }),
                        )
                        .into_any_element()
                })
                .collect();
        let empty = list.is_empty();
        let search = self.plugin_page.mini_search.clone();
        Some(
            deferred(
                anchored().position(at).snap_to_window().child(
                    div()
                        .id("mini-picker")
                        .w(px(460.))
                        .max_h(px(560.))
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
                        .child(div().flex().flex_wrap().gap(px(5.)).children(chips))
                        .child(
                            div()
                                .id("mini-list")
                                .flex_1()
                                .min_h(px(0.))
                                .overflow_y_scroll()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .children(list)
                                .when(empty, |d| d.child(div().p(px(14.)).text_color(rgb(TEXT_3)).child("Nothing matches"))),
                        )
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
