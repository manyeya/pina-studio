//! The synth's window, its own and over everything: oscillators you can see, filters you can
//! drag, envelopes and LFOs, macros, the matrix, and the track's effects.
//!
//! It's deep in the rest of the app: every knob is the same knob as anywhere (its automation, its
//! modulators' rings and a modulator dropped on it all work), its sources can be dropped on an
//! effect's knob as well as its own, and its Effects page is the track's chain, not a second one.

mod matrix;
mod oscillators;
mod shapes;

use std::{cell::Cell, rc::Rc};

use gpui::{
    Bounds, Context, Div, MouseButton, MouseDownEvent, PathBuilder, Pixels, anchored, canvas,
    deferred, div, point, prelude::*, px, rgb,
};
use pina_engine::{
    dsp::poly::table::{TABLES, table},
    song::project::*,
};

use crate::{
    mix::{
        eq::{dial, value_line},
        strip::{self, Builtin, Channel},
    },
    ui::{
        menu::{Menu, MenuAction},
        theme::*,
    },
    workspace::{Drag, Target, Workspace, pt},
};

pub const WINDOW_W: f32 = 1240.0;
pub const WINDOW_H: f32 = 772.0;
const HEAD_H: f32 = 48.0;
/// Each part of the synth's colour.
pub const OSC_COLORS: [u32; OSCS] = [0x34d399, 0x60a5fa, 0xf59e0b];
const FILTER_COLOR: u32 = 0xf472b6;
const SHAPE_COLOR: u32 = 0xa78bfa;

/// What marks a drag on LFO `n`'s picture: this and `n`.
const DRAW_LFO: u8 = 10;

/// Which page of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolyPage {
    Synth,
    Matrix,
    Effects,
}

/// The envelope or LFO in hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolyShape {
    Env(usize),
    Lfo(usize),
}

pub struct PolyView {
    pub track: usize,
    pub at: (f32, f32),
    pub page: PolyPage,
    pub shape: PolyShape,
    /// Where the filters' and the shape's pictures were drawn, for their points.
    pub filters: [Rc<Cell<Bounds<Pixels>>>; FILTERS],
    pub plot: Rc<Cell<Bounds<Pixels>>>,
}

/// Something picked in the window.
#[derive(Clone, Debug, PartialEq)]
pub enum PolyOp {
    Patch(&'static str),
    Table(usize, &'static str),
    Warp(usize, usize, Warp),
    Filter(usize, PolyFilter),
    LfoWave(usize, LfoWave),
    Routing(FilterRouting),
    RouteSource(usize, PolySource),
    RouteBy(usize, Option<PolySource>),
    RouteDest(usize, PolyDest),
    AddRoute,
    RemoveRoute(usize),
    ArpMode(ArpMode),
}

impl Workspace {
    /// The synth on track `t`.
    pub(crate) fn poly(&self, t: usize) -> Option<&Poly> {
        match self.session.project.tracks.get(t)?.instrument.as_ref()? {
            Instrument::Poly(p) => Some(p),
            _ => None,
        }
    }

    pub(crate) fn poly_mut(&mut self, t: usize) -> Option<&mut Poly> {
        match self
            .session
            .project
            .tracks
            .get_mut(t)?
            .instrument
            .as_mut()?
        {
            Instrument::Poly(p) => Some(p),
            _ => None,
        }
    }

    pub fn open_poly(&mut self, t: usize, cx: &mut Context<Self>) {
        if self.poly(t).is_none() {
            return;
        }
        let at = (
            ((self.viewport_w - WINDOW_W) / 2.0).max(0.0),
            ((self.viewport_h - WINDOW_H) / 2.0).max(40.0),
        );
        let held = self.poly_view.as_ref().map(|v| (v.at, v.page, v.shape));
        let (at, page, shape) = held.unwrap_or((at, PolyPage::Synth, PolyShape::Env(0)));
        self.poly_view = Some(PolyView {
            track: t,
            at,
            page,
            shape,
            filters: Default::default(),
            plot: Default::default(),
        });
        cx.notify();
    }

    pub fn drag_poly_window(
        &mut self,
        from: gpui::Point<f32>,
        start: (f32, f32),
        p: gpui::Point<f32>,
    ) {
        let (vw, vh) = (self.viewport_w, self.viewport_h);
        if let Some(v) = self.poly_view.as_mut() {
            v.at = (
                (start.0 + p.x - from.x).clamp(-WINDOW_W + 80.0, vw - 80.0),
                (start.1 + p.y - from.y).clamp(0.0, vh - HEAD_H),
            );
        }
    }

    /// A choice made in the window.
    pub fn poly_op(&mut self, t: usize, op: PolyOp, cx: &mut Context<Self>) {
        self.edit(cx, |ws| {
            let Some(p) = ws.poly_mut(t) else { return };
            match op {
                PolyOp::Patch(name) => {
                    if let Some(new) = poly_patch(name) {
                        *p = new;
                    }
                }
                PolyOp::Table(o, name) => p.tables[o] = name.to_string(),
                PolyOp::Warp(o, k, w) => p.warps[o][k] = w,
                PolyOp::Filter(i, kind) => p.filters[i] = kind,
                PolyOp::LfoWave(i, w) => p.lfos[i].wave = w,
                PolyOp::Routing(r) => p.routing = r,
                PolyOp::RouteSource(i, s) => {
                    if let Some(r) = p.routes.get_mut(i) {
                        r.source = s;
                    }
                }
                PolyOp::RouteBy(i, s) => {
                    if let Some(r) = p.routes.get_mut(i) {
                        r.by = s;
                    }
                }
                PolyOp::RouteDest(i, d) => {
                    if let Some(r) = p.routes.get_mut(i) {
                        r.dest = d;
                    }
                }
                PolyOp::AddRoute if p.routes.len() < MAX_POLY_ROUTES => p.routes.push(PolyRoute {
                    source: PolySource::Env(1),
                    dest: PolyDest::Param(filter_param(0, FilterField::Cutoff) as u16),
                    depth: 0.3,
                    by: None,
                }),
                PolyOp::AddRoute => {}
                PolyOp::RemoveRoute(i) => {
                    if i < p.routes.len() {
                        p.routes.remove(i);
                    }
                }
                PolyOp::ArpMode(m) => p.arp.mode = m,
            }
        });
    }

    /// A source dropped on a knob: routed to it, or to an effect's on the track.
    pub fn drop_poly_source(
        &mut self,
        t: usize,
        source: PolySource,
        at: gpui::Point<f32>,
        cx: &mut Context<Self>,
    ) {
        use pina_engine::dsp::modulate::ModTarget;
        let at = point(px(at.x), px(at.y));
        let hit = self
            .mod_targets
            .borrow()
            .iter()
            .rev()
            .find(|(tt, _, b)| *tt == t && b.contains(&at))
            .map(|&(_, mt, _)| mt);
        let dest = match hit {
            Some(ModTarget::Poly(i)) => PolyDest::Param(i),
            Some(ModTarget::Tool(k, i)) => PolyDest::Effect(k, i),
            _ => return,
        };
        self.edit(cx, |ws| {
            if let Some(p) = ws.poly_mut(t)
                && p.routes.len() < MAX_POLY_ROUTES
            {
                p.routes.push(PolyRoute {
                    source,
                    dest,
                    depth: 0.3,
                    by: None,
                });
            }
        });
    }

    /// A point on a picture dragged: filter `i`'s (`i` 0 or 1), or the shape's.
    pub fn drag_poly_point(&mut self, id: u8, p: gpui::Point<f32>) {
        let Some(v) = self.poly_view.as_ref() else {
            return;
        };
        let (t, shape) = (v.track, v.shape);
        let on = |b: Bounds<Pixels>| {
            (
                ((p.x - f32::from(b.origin.x)) / f32::from(b.size.width).max(1.0)).clamp(0.0, 1.0),
                ((p.y - f32::from(b.origin.y)) / f32::from(b.size.height).max(1.0)).clamp(0.0, 1.0),
            )
        };
        let filter = v.filters.get(id as usize).map(|b| on(b.get()));
        let shaped = on(v.plot.get());
        if id >= DRAW_LFO {
            return self.draw_lfo(
                t,
                (id - DRAW_LFO) as usize,
                shaped.0,
                1.0 - (1.0 - shaped.1 - 0.04) / 0.92,
            );
        }
        let Some(q) = self.poly_mut(t) else { return };
        if let Some((x, y)) = filter {
            let i = id as usize;
            q.params[filter_param(i, FilterField::Cutoff)] = POLY_PARAMS
                [filter_param(i, FilterField::Cutoff)]
            .scale
            .value(x);
            q.params[filter_param(i, FilterField::Resonance)] = 1.0 - y;
            return;
        }
        let PolyShape::Env(e) = shape else { return };
        let (x, y) = shaped;
        let time = |f| POLY_PARAMS[env_param(e, f)].scale;
        match id {
            2 => {
                q.params[env_param(e, EnvField::Attack)] =
                    time(EnvField::Attack).value((x * 3.0).min(1.0));
            }
            3 => {
                q.params[env_param(e, EnvField::Decay)] =
                    time(EnvField::Decay).value(((x - 1.0 / 3.0) * 3.0).clamp(0.0, 1.0));
                q.params[env_param(e, EnvField::Sustain)] = 1.0 - y;
            }
            _ => {
                q.params[env_param(e, EnvField::Release)] =
                    time(EnvField::Release).value(((x - 2.0 / 3.0) * 3.0).clamp(0.0, 1.0));
            }
        }
    }

    /// One of the synth's knobs: the same as any in the app.
    pub(crate) fn poly_knob(
        &self,
        t: usize,
        i: usize,
        name: &'static str,
        dial_size: f32,
        color: u32,
        cx: &mut Context<Self>,
    ) -> Div {
        let target = Target::Poly { track: t, param: i };
        let value = self.target(target);
        let q = &POLY_PARAMS[i];
        let reset = q.scale.pos(q.default);
        let shown = if q.bipolar { value * 2.0 - 1.0 } else { value };
        let (bipolar, text) = (q.bipolar, q.say(q.scale.value(value)));
        let sel = q.id.clone();
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2.))
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .whitespace_nowrap()
                    .child(name),
            )
            .child(
                div()
                    .cursor_ns_resize()
                    .debug_selector(move || format!("poly-knob-{sel}"))
                    .child(self.modded(
                        self.mod_key(target),
                        dial_size,
                        dial(
                            shown,
                            dial_size,
                            (dial_size / 9.0).clamp(3.5, 6.0),
                            color,
                            bipolar,
                        ),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.modifiers.platform {
                                ws.edit(cx, |ws| ws.set_target(target, reset));
                                return;
                            }
                            let y = pt(e.position).y;
                            ws.begin_drag(Drag::Knob { target, y, value }, cx);
                        }),
                    ),
            )
            .child(value_line(text, 10.5, None))
    }

    /// A button that opens a menu of choices.
    pub(crate) fn poly_menu(
        &self,
        sel: String,
        label: String,
        items: impl Fn(&Self) -> Vec<(String, MenuAction)> + 'static,
        search: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .h(px(24.))
            .px(px(9.))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(6.))
            .rounded(px(6.))
            .bg(rgb(0x191a20))
            .border_1()
            .border_color(rgb(LINE))
            .text_size(px(11.5))
            .text_color(rgb(TEXT_2))
            .whitespace_nowrap()
            .cursor_pointer()
            .hover(|d| d.border_color(rgb(LINE_STRONG)))
            .debug_selector(move || sel.clone())
            .child(label)
            .child(div().text_color(rgb(TEXT_4)).child("▾"))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let list = items(ws);
                    ws.menu = Some(match search {
                        true => Menu::search(e.position, list),
                        false => Menu {
                            at: e.position,
                            items: list,
                            ..Default::default()
                        },
                    });
                    cx.notify();
                }),
            )
    }

    pub fn poly_window(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let v = self.poly_view.as_ref()?;
        let (t, at, page) = (v.track, v.at, v.page);
        let poly = self.poly(t)?.clone();
        let head = self.poly_head(t, &poly, page, cx);
        let body = match page {
            PolyPage::Synth => self.poly_synth_page(t, &poly, cx),
            PolyPage::Matrix => self.poly_matrix_page(t, &poly, cx),
            PolyPage::Effects => self.poly_effects_page(t, cx),
        };
        let frame = div()
            .id("poly-window")
            .occlude()
            .w(px(WINDOW_W))
            .h(px(WINDOW_H))
            .flex()
            .flex_col()
            .rounded(px(12.))
            .overflow_hidden()
            .bg(rgb(0x101115))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .font_family(SANS)
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .child(head)
            .child(body);
        Some(
            deferred(
                anchored()
                    .position(point(px(at.0), px(at.1)))
                    .snap_to_window()
                    .child(frame),
            )
            .with_priority(0),
        )
    }

    fn poly_head(&self, t: usize, p: &Poly, page: PolyPage, cx: &mut Context<Self>) -> Div {
        let track = &self.session.project.tracks[t];
        let patch = match p.patch.is_empty() {
            true => "Init".to_string(),
            false => p.patch.clone(),
        };
        let patches = self.poly_menu(
            "poly-patch".into(),
            patch,
            move |_| {
                let mut items = Vec::new();
                let mut part = "";
                let mut sorted: Vec<_> = POLY_PATCHES.to_vec();
                sorted.sort_by_key(|p| p.1);
                for (name, of, _) in sorted {
                    if of != part {
                        part = of;
                        items.push((capitalized(of), MenuAction::Nothing));
                    }
                    items.push((name.to_string(), MenuAction::Poly(t, PolyOp::Patch(name))));
                }
                items
            },
            true,
            cx,
        );
        let tab = |name: &'static str, to: PolyPage, cx: &mut Context<Self>| {
            seg(name, page == to, 26.)
                .debug_selector(move || format!("poly-page-{}", name.to_lowercase()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(v) = ws.poly_view.as_mut() {
                            v.page = to;
                        }
                        cx.notify();
                    }),
                )
        };
        let quality = [
            ("Draft", PolyQuality::Draft),
            ("Good", PolyQuality::Good),
            ("Best", PolyQuality::Best),
        ];
        let quality = tray()
            .p(px(2.))
            .rounded(px(8.))
            .children(quality.map(|(name, q)| {
                seg(name, p.quality == q, 22.)
                    .debug_selector(move || format!("poly-quality-{}", name.to_lowercase()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit(cx, |ws| {
                                if let Some(p) = ws.poly_mut(t) {
                                    p.quality = q;
                                }
                            });
                        }),
                    )
            }));
        let at = self.poly_view.as_ref().map_or((0.0, 0.0), |v| v.at);
        div()
            .flex_none()
            .h(px(HEAD_H))
            .px(px(14.))
            .flex()
            .items_center()
            .gap(px(10.))
            .border_b_1()
            .border_color(rgb(LINE))
            .cursor_grab()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    ws.begin_drag(
                        Drag::PolyWindow {
                            from: pt(e.position),
                            start: at,
                        },
                        cx,
                    );
                }),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .child("Synth"),
            )
            .child(chip(track.name.clone(), TEXT_3))
            .child(div().w(px(180.)).child(patches))
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(8.))
                    .child(tab("Synth", PolyPage::Synth, cx))
                    .child(tab("Matrix", PolyPage::Matrix, cx))
                    .child(tab("Effects", PolyPage::Effects, cx)),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child("Quality"),
            )
            .child(quality)
            .child(
                div()
                    .id("poly-close")
                    .debug_selector(|| "poly-close".into())
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_color(rgb(TEXT_3))
                    .hover(|d| d.bg(rgb(OFF)))
                    .child("×")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.poly_view = None;
                            cx.notify();
                        }),
                    ),
            )
    }

    fn poly_synth_page(&mut self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let oscs = div()
            .flex()
            .gap(px(8.))
            .child(self.poly_osc_card(t, p, 0, cx))
            .child(self.poly_osc_card(t, p, 1, cx))
            .child(self.poly_osc_card(t, p, 2, cx))
            .child(self.sub_noise_card(t, p, cx));
        let filters = div()
            .flex()
            .gap(px(8.))
            .child(self.filter_card(t, p, 0, cx))
            .child(self.filter_card(t, p, 1, cx))
            .child(self.macro_card(t, cx));
        let shapes = div()
            .flex()
            .gap(px(8.))
            .child(self.shape_card(t, p, cx))
            .child(self.voice_card(t, p, cx));
        div()
            .flex_1()
            .p(px(10.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(oscs)
            .child(filters)
            .child(shapes)
    }

    fn poly_effects_page(&mut self, t: usize, cx: &mut Context<Self>) -> Div {
        let ch = Channel::Track(t);
        let track = &self.session.project.tracks[t];
        let (comp, sat, eq, limit) = (
            track.comp.is_some(),
            track.sat.is_some(),
            track.eq.is_some(),
            track.limit.is_some(),
        );
        let more = track.more.clone();
        let own = |name: &'static str, b: Builtin, ws: &Self, cx: &mut Context<Self>| {
            let open = cx.listener(move |ws: &mut Workspace, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                match b {
                    Builtin::Eq => ws.open_eq(ch, cx),
                    b => ws.open_fx(ch, b, cx),
                }
            });
            let _ = ws;
            effect_card(name, "the channel's own")
                .child(button("Open", false).on_mouse_down(MouseButton::Left, open))
        };
        let mut grid = Vec::new();
        if eq {
            grid.push(own("Channel EQ", Builtin::Eq, self, cx));
        }
        if comp {
            grid.push(own("Compressor", Builtin::Comp, self, cx));
        }
        if sat {
            grid.push(own("Saturator", Builtin::Sat, self, cx));
        }
        if limit {
            grid.push(own("Limiter", Builtin::Limit, self, cx));
        }
        for (k, u) in more.iter().enumerate() {
            let Some((b, name)) = self.stacked(ch, k) else {
                continue;
            };
            let table = strip::knobs(u.kind());
            let on = !u.bypassed();
            let row = div().flex().flex_wrap().gap(px(10.)).children(
                (0..table.len())
                    .filter(|&i| knob_shows(u, i))
                    .map(|i| self.effect_knob(ch, k, i, table[i].name, cx)),
            );
            let power = button(if on { "On" } else { "Off" }, on)
                .debug_selector(move || format!("poly-effect-power-{k}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.edit(cx, |ws| {
                            if let Some(u) = ws.chain_mut(ch).more.get_mut(k) {
                                let off = u.bypass_mut();
                                *off = !*off;
                            }
                        });
                    }),
                );
            let open = cx.listener(move |ws: &mut Workspace, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                ws.open_fx_at(ch, b, Some(k), cx);
            });
            grid.push(
                effect_card_owned(name).child(row).child(
                    div().flex().gap(px(6.)).child(power).child(
                        button("Open its window", false).on_mouse_down(MouseButton::Left, open),
                    ),
                ),
            );
        }
        let add = self.poly_menu(
            "poly-add-effect".into(),
            "Add an effect".into(),
            move |_| {
                let tools = [
                    Fx::Reverb,
                    Fx::Delay,
                    Fx::Filter,
                    Fx::Motion,
                    Fx::Shaper,
                    Fx::Gate,
                    Fx::DeEsser,
                    Fx::Stereo,
                    Fx::Multiband,
                    Fx::Pitch,
                    Fx::TapeStop,
                    Fx::Tune,
                ];
                [Builtin::Eq, Builtin::Comp, Builtin::Sat, Builtin::Limit]
                    .into_iter()
                    .chain(tools.map(Builtin::Tool))
                    .map(|builtin| {
                        (
                            builtin.name().to_string(),
                            MenuAction::AddBuiltin {
                                channel: ch,
                                builtin,
                            },
                        )
                    })
                    .collect()
            },
            false,
            cx,
        );
        let page = div()
            .id("poly-effects")
            .size_full()
            .overflow_y_scroll()
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(div().w(px(180.)).child(add))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_3))
                            .child("These are the track's own effects, as the mixer shows them. Drag any synth source's ↗ onto one of their knobs to move it."),
                    ),
            )
            .child(div().flex().flex_wrap().gap(px(10.)).children(grid));
        div().flex_1().overflow_hidden().child(page)
    }

    /// A stacked effect's knob, on the Effects page.
    fn effect_knob(
        &self,
        ch: Channel,
        k: usize,
        i: usize,
        name: &'static str,
        cx: &mut Context<Self>,
    ) -> Div {
        let target = Target::Stacked(ch, k, crate::workspace::Knob::Param(i));
        let value = self.target(target);
        let u = self.tool_heard(ch, k);
        let table = u.map(|u| strip::knobs(u.kind()));
        let bipolar = table.and_then(|t| t.get(i)).is_some_and(|p| p.bipolar);
        let text = match (u, table.and_then(|t| t.get(i))) {
            (Some(u), Some(p)) => (p.say)(&u),
            _ => String::new(),
        };
        let shown = if bipolar { value * 2.0 - 1.0 } else { value };
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2.))
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .child(name),
            )
            .child(
                div()
                    .cursor_ns_resize()
                    .child(self.modded(
                        self.mod_key(target),
                        30.0,
                        dial(shown, 30.0, 3.5, SHAPE_COLOR, bipolar),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let y = pt(e.position).y;
                            ws.begin_drag(Drag::Knob { target, y, value }, cx);
                        }),
                    ),
            )
            .child(value_line(text, 10.5, None))
    }

    fn macro_card(&self, t: usize, cx: &mut Context<Self>) -> Div {
        const NAMES: [&str; MACROS] = ["1", "2", "3", "4", "5", "6", "7", "8"];
        let mut rows = Vec::new();
        for chunk in (0..MACROS).collect::<Vec<_>>().chunks(4) {
            rows.push(
                div()
                    .flex()
                    .justify_between()
                    .children(chunk.iter().map(|&i| {
                        div()
                            .relative()
                            .child(self.poly_knob(
                                t,
                                macro_param(i),
                                NAMES[i],
                                28.0,
                                SHAPE_COLOR,
                                cx,
                            ))
                            .child(
                                self.source_grip(t, PolySource::Macro(i as u8), cx)
                                    .absolute()
                                    .top(px(-2.))
                                    .right(px(-2.)),
                            )
                    })),
            );
        }
        panel("Macros", SHAPE_COLOR)
            .flex_1()
            .child(div().flex().flex_col().gap(px(2.)).children(rows))
    }

    /// A ↗ to drag a source onto a knob.
    pub(crate) fn source_grip(&self, t: usize, source: PolySource, cx: &mut Context<Self>) -> Div {
        let sel = format!("poly-source-{source:?}").to_lowercase();
        div()
            .size(px(15.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .text_size(px(10.))
            .text_color(rgb(SHAPE_COLOR))
            .bg(rgb(0x1a1a22))
            .cursor_grab()
            .debug_selector(move || sel.clone())
            .child("↗")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.begin_drag(Drag::PolySource { track: t, source }, cx);
                }),
            )
    }

    fn voice_card(&self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let on = |name: &'static str, now: bool, set: fn(&mut Poly), cx: &mut Context<Self>| {
            button(name, now)
                .debug_selector(move || format!("poly-{}", name.to_lowercase()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.edit(cx, |ws| {
                            if let Some(p) = ws.poly_mut(t) {
                                set(p);
                            }
                        });
                    }),
                )
        };
        let modes = [
            ("Off", ArpMode::Off),
            ("Up", ArpMode::Up),
            ("Down", ArpMode::Down),
            ("Up/Down", ArpMode::UpDown),
            ("Played", ArpMode::Played),
            ("Random", ArpMode::Random),
            ("Chord", ArpMode::Chord),
        ];
        let mode = p.arp.mode;
        let arp = self.poly_menu(
            "poly-arp".into(),
            format!(
                "Arp: {}",
                modes.iter().find(|m| m.1 == mode).map_or("Off", |m| m.0)
            ),
            move |_| {
                modes
                    .iter()
                    .map(|&(name, m)| (name.to_string(), MenuAction::Poly(t, PolyOp::ArpMode(m))))
                    .collect()
            },
            false,
            cx,
        );
        let rates = [("1/32", 0.125), ("1/16", 0.25), ("1/8", 0.5), ("1/4", 1.0)];
        let beats = p.arp.beats;
        let rate = tray()
            .p(px(2.))
            .rounded(px(7.))
            .children(rates.map(|(name, b)| {
                seg(name, beats == b, 20.).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.edit(cx, |ws| {
                            if let Some(p) = ws.poly_mut(t) {
                                p.arp.beats = b;
                            }
                        });
                    }),
                )
            }));
        panel("Voice", TEXT_3)
            .flex_1()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(self.poly_knob(t, POLY_GLIDE, "Glide", 28.0, TEXT_2, cx))
                    .child(self.poly_knob(t, POLY_DRIFT, "Drift", 28.0, TEXT_2, cx))
                    .child(self.poly_knob(t, POLY_VELOCITY, "Velocity", 28.0, TEXT_2, cx))
                    .child(self.poly_knob(t, POLY_MASTER, "Master", 28.0, TEXT_2, cx)),
            )
            .child(
                div()
                    .flex()
                    .gap(px(6.))
                    .items_center()
                    .child(on("Mono", p.voicing.mono, |p| p.voicing.mono ^= true, cx))
                    .child(on(
                        "Legato",
                        p.voicing.legato,
                        |p| p.voicing.legato ^= true,
                        cx,
                    ))
                    .child(div().w(px(120.)).child(arp)),
            )
            .child(rate)
    }
}

pub(crate) fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// A part of the window, its name along the top.
pub(crate) fn panel(name: &'static str, color: u32) -> Div {
    div()
        .p(px(8.))
        .flex()
        .flex_col()
        .gap(px(5.))
        .rounded(px(9.))
        .bg(rgb(0x14151a))
        .border_1()
        .border_color(rgb(LINE))
        .child(
            div()
                .text_size(px(11.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(color))
                .child(name),
        )
}

fn effect_card(name: &'static str, about: &'static str) -> Div {
    effect_card_owned(name.to_string()).child(
        div()
            .text_size(px(11.))
            .text_color(rgb(TEXT_4))
            .child(about),
    )
}

fn effect_card_owned(name: String) -> Div {
    div()
        .w(px(590.))
        .p(px(12.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .rounded(px(9.))
        .bg(rgb(0x14151a))
        .border_1()
        .border_color(rgb(LINE))
        .child(
            div()
                .text_size(px(12.5))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(name),
        )
}

/// A faint picture's background.
pub(crate) fn picture() -> Div {
    div()
        .relative()
        .overflow_hidden()
        .rounded(px(6.))
        .bg(rgb(0x0b0b0e))
        .border_1()
        .border_color(rgb(LINE_SOFT))
}

/// A line through `points` on `b`, x and y each 0 to 1 from the top left.
pub(crate) fn polyline(
    window: &mut gpui::Window,
    b: Bounds<Pixels>,
    points: impl Iterator<Item = (f32, f32)>,
    width: f32,
    color: gpui::Hsla,
) {
    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
    let mut p = PathBuilder::stroke(px(width));
    let mut first = true;
    for (x, y) in points {
        let at = point(b.origin.x + px(x * w), b.origin.y + px(y * h));
        if first {
            p.move_to(at);
            first = false;
        } else {
            p.line_to(at);
        }
    }
    if let Ok(path) = p.build() {
        window.paint_path(path, color);
    }
}

/// A point to drag on a picture: `id` says which.
pub(crate) fn point_handle(
    x: f32,
    y: f32,
    color: u32,
    id: u8,
    sel: &'static str,
    cx: &mut Context<Workspace>,
) -> Div {
    div()
        .absolute()
        .left(px(x - 6.0))
        .top(px(y - 6.0))
        .size(px(12.))
        .rounded(px(6.))
        .bg(rgb(0x0b0b0e))
        .border_2()
        .border_color(rgb(color))
        .cursor_pointer()
        .debug_selector(move || sel.into())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                ws.begin_drag(Drag::PolyPoint { id }, cx);
            }),
        )
}
