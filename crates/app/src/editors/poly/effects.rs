//! The Effects page: the synth's own rack, part of its sound and kept with its patch. Across the
//! top the chain as it plays, its voices into each effect in turn and on into the track's own:
//! each effect a tile with its switch, a little live picture, what it's set to, what rides it,
//! dragged to change the order. Under it the stage: the effect picked, as the mixer's own editor
//! for it shows it (its picture to drag, every control, its meters), then the sources that can
//! ride its knobs and those that do.

use super::*;
use crate::mix::strip::Channel;
use matrix::source_name;
use pina_engine::song::project::Fx;

/// The chain's tiles, how tall.
const TILE_H: f32 = 84.0;
/// The chain across the top, all told; and the rides under the stage.
/// Its title row, its padding, the gap, the tiles: all of it, so nothing's cut.
const CHAIN_H: f32 = 12.0 + 30.0 + 10.0 + TILE_H + 12.0;
const RIDES_H: f32 = 46.0;

/// Round the stage.
const RACK_STAGE_PAD: f32 = 10.0;

/// The effects most often wanted first, for an empty rack.
const FIRST: [&str; 6] = [
    "Hall",
    "Ensemble",
    "Tape Delay",
    "Tube",
    "Filter",
    "Ping-Pong Delay",
];

impl Workspace {
    pub(crate) fn poly_effects_page(&mut self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let n = p.fx.len();
        let open = self
            .poly_view
            .as_ref()
            .map_or(0, |v| v.fx_open)
            .min(n.saturating_sub(1));
        match n {
            0 => self.let_go_rack_views(),
            _ => self.show_rack_fx(t, open, p, cx),
        }
        let chain = self.rack_chain(t, p, open, cx);
        let stage = match n {
            0 => self.rack_empty(t, cx),
            _ => self.rack_stage(t, open, p, cx),
        };
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(chain)
            .child(stage)
    }

    /// The editors pointed at the rack's effect at `slot`, unless the mixer has one open in its
    /// own window: that's left alone.
    fn show_rack_fx(&mut self, t: usize, slot: usize, p: &Poly, cx: &mut Context<Self>) {
        let Some(kind) = p.fx.get(slot).map(|u| u.kind()) else {
            return;
        };
        let here = Some((t, slot));
        let fx_free = self.fx_view.as_ref().is_none_or(|v| v.rack.is_some());
        let eq_free = self.eq_view.as_ref().is_none_or(|v| v.rack.is_some());
        let fresh = match kind {
            Fx::Eq => eq_free && self.eq_view.as_ref().is_none_or(|v| v.rack != here),
            _ => {
                fx_free
                    && self.fx_view.as_ref().is_none_or(|v| {
                        v.rack != here || Some(v.which) != crate::mix::strip::Builtin::of(kind)
                    })
            }
        };
        if fresh {
            self.open_rack_fx(t, slot, cx);
        }
    }

    /// Off the Effects page: the editors given back to the mixer.
    pub(crate) fn let_go_rack_views(&mut self) {
        if self.fx_view.as_ref().is_some_and(|v| v.rack.is_some()) {
            self.fx_view = None;
            self.player.watch_fx_more(None);
        }
        if self.eq_view.as_ref().is_some_and(|v| v.rack.is_some()) {
            self.eq_view = None;
            self.player.watch_eq(None);
            self.player.watch_eq_more(None);
            self.player.solo_band(None);
        }
    }

    /// The chain as it plays: the voices, each effect, the way to add one, then the track.
    fn rack_chain(&self, t: usize, p: &Poly, open: usize, cx: &mut Context<Self>) -> Div {
        let n = p.fx.len();
        let dragging = match self.drag {
            Some(Drag::RackTile {
                track,
                from,
                to,
                moved: true,
                ..
            }) if track == t => Some((from, to)),
            _ => None,
        };
        // While one's dragged, the order it would make.
        let mut order: Vec<usize> = (0..n).collect();
        if let Some((from, to)) = dragging {
            let k = order.remove(from);
            order.insert(to.min(order.len()), k);
        }
        if let Some(v) = self.poly_view.as_ref() {
            v.rack_tiles.borrow_mut().clear();
        }
        let arrow = || {
            div()
                .w(px(20.))
                .flex_none()
                .flex()
                .justify_center()
                .text_color(rgb(TEXT_4))
                .child(icon(Icon::Forward, 13.))
        };
        let mut row = div()
            .flex()
            .items_center()
            .child(node("Voices", Some(72.), cx).child(mini_canvas(voice_wave(), TEXT)));
        // The chain fills the bar: with few effects, the way to add one takes the room left;
        // with many, the effects do.
        let roomy = n < 4;
        for (place, &k) in order.iter().enumerate() {
            let u = p.fx[k];
            row = row.child(arrow()).child(self.rack_tile(
                t,
                k,
                place,
                &u,
                p,
                (k == open, roomy),
                dragging,
                cx,
            ));
        }
        if n < RACK {
            row = row.child(arrow()).child(self.rack_add_tile(t, roomy, cx));
        }
        let track = &self.session.project.tracks[t];
        let own: Vec<&str> = [
            track.eq.map(|_| "EQ"),
            track.comp.map(|_| "Comp"),
            track.sat.map(|_| "Sat"),
            (!track.more.is_empty() || !track.inserts.is_empty()).then_some("more"),
            track.limit.map(|_| "Limit"),
        ]
        .into_iter()
        .flatten()
        .collect();
        let mixer = div()
            .text_size(px(11.))
            .text_color(rgb(ACCENT))
            .cursor_pointer()
            .hover(|d| d.text_color(rgb(TEXT)))
            .debug_selector(|| "poly-open-mixer".into())
            .child("Mixer")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.channel = Channel::Track(t);
                    ws.view = crate::workspace::View::Mixer;
                    cx.notify();
                }),
            );
        row = row.child(arrow()).child(
            node("Track", Some(84.), cx)
                .child(label(match own.is_empty() {
                    true => "nothing yet".to_string(),
                    false => own.join(" · "),
                }))
                .child(mixer),
        );
        let on = !p.rack_off;
        div()
            .flex_none()
            .h(px(CHAIN_H))
            .px(px(20.))
            .pt(px(12.))
            .pb(px(12.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .bg(rgb(WELL))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                // A fixed height: what's in it (a name being typed too) never pushes the tiles.
                div()
                    .h(px(30.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT_2))
                            .child("Signal"),
                    )
                    .child(label(format!(
                        "{n} of {RACK} · drag to reorder · after its voices, before the track's own effects"
                    )))
                    .child(div().flex_1())
                    .child(self.rack_presets(t, p, cx))
                    .child(label(if on { "Rack" } else { "Rack off: passed by" }))
                    .child(
                        switch(on)
                            .debug_selector(|| "poly-rack-on".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.poly_op(t, PolyOp::RackOff, cx);
                                }),
                            ),
                    ),
            )
            .child(row.when(!on, |d| d.opacity(0.5)))
    }

    /// Keeping racks: this one saved by a name typed, or a kept one put in place of it.
    fn rack_presets(&self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let naming = self.naming.as_ref().and_then(|n| match n.what {
            crate::workspace::Name::RackSave(tt) if tt == t => Some(n.input.clone()),
            _ => None,
        });
        let empty = p.fx.is_empty();
        let save = match naming {
            Some(input) => div().w(px(180.)).child(crate::pages::genres::field_box(
                Some(input),
                "poly-rack-name",
            )),
            None => button("Save the rack", false)
                .h(px(26.))
                .text_size(px(12.))
                .debug_selector(|| "poly-rack-save".into())
                .when(empty, |d| d.opacity(0.4))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, window, cx| {
                        cx.stop_propagation();
                        if !empty {
                            ws.start_naming(crate::workspace::Name::RackSave(t), window, cx);
                        }
                    }),
                ),
        };
        let presets = self.poly_menu(
            "poly-rack-presets".into(),
            "Rack presets".into(),
            move |ws| {
                let racks = ws.library.as_ref().map(|l| l.racks()).unwrap_or_default();
                racks
                    .iter()
                    .enumerate()
                    .map(|(k, (name, units))| {
                        (
                            format!(
                                "{name}: {}",
                                units.iter().map(rack_name).collect::<Vec<_>>().join(" → ")
                            ),
                            MenuAction::Poly(t, PolyOp::LoadRack(k)),
                        )
                    })
                    .collect()
            },
            true,
            cx,
        );
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(save)
            .child(div().w(px(140.)).child(presets))
    }

    /// This rack kept as `name`.
    pub(crate) fn save_rack(&mut self, t: usize, name: &str, cx: &mut Context<Self>) {
        let Some(units) = self.poly(t).map(|p| p.fx.clone()) else {
            return;
        };
        let name = name.trim();
        let done = self.library().save_rack(name, &units);
        self.status = Some(match done {
            Ok(()) => format!("Rack kept as {name}: it's under Rack presets"),
            Err(e) => format!("Couldn't keep the rack: {e}"),
        });
        cx.notify();
    }

    /// The kept rack at `k` in place of this one: what rode the old one's knobs goes with it.
    pub(crate) fn load_rack(&mut self, t: usize, k: usize, cx: &mut Context<Self>) {
        let Some((name, units)) = self.library().racks().into_iter().nth(k) else {
            return;
        };
        self.edit(cx, |ws| {
            let track = &mut ws.session.project.tracks[t];
            while matches!(&track.instrument, Some(pina_engine::song::project::Instrument::Poly(p)) if !p.fx.is_empty()) {
                track.remove_rack_fx(0);
            }
            if let Some(p) = ws.poly_mut(t) {
                for u in units {
                    p.add_fx(u);
                }
            }
        });
        if let Some(v) = self.poly_view.as_mut() {
            v.fx_open = 0;
        }
        self.status = Some(format!("{name} rack in"));
        cx.notify();
    }

    /// An effect in the chain: its name and switch, its picture, what it's set to, what rides it
    /// and how wet; picked, it's on the stage; dragged, it moves.
    #[allow(clippy::too_many_arguments)]
    fn rack_tile(
        &self,
        t: usize,
        k: usize,
        place: usize,
        u: &Unit,
        p: &Poly,
        (picked, roomy): (bool, bool),
        dragging: Option<(usize, usize)>,
        cx: &mut Context<Self>,
    ) -> Div {
        let on = !u.bypassed();
        let lifted = dragging.is_some_and(|(from, _)| from == k);
        let tiles = self.poly_view.as_ref().map(|v| v.rack_tiles.clone());
        let mut riders: Vec<PolySource> = Vec::new();
        for r in &p.routes {
            if matches!(r.dest, PolyDest::Rack(s, _) if s as usize == k)
                && !riders.contains(&r.source)
            {
                riders.push(r.source);
            }
        }
        let say = |name: &str| {
            crate::mix::strip::knobs(u.kind())
                .iter()
                .find(|q| q.name == name)
                .map(|q| (q.say)(u))
        };
        let summary = match u {
            Unit::Eq(e) => Some(format!("{} bands", e.bands.len())),
            // As it plays at the song's tempo, synced or not.
            Unit::Delay(d) => {
                let [l, r] = d.times_s(self.session.project.tempo);
                Some(match (l - r).abs() < 1e-3 {
                    true => format!("{:.0} ms", l * 1000.0),
                    false => format!("{:.0} · {:.0} ms", l * 1000.0, r * 1000.0),
                })
            }
            _ => summary_knob(u.kind()).and_then(say),
        };
        let mix = say("Mix");
        div()
            .when(roomy, |d| d.w(px(236.)).flex_none())
            .when(!roomy, |d| d.flex_1().min_w(px(96.)))
            .h(px(TILE_H))
            .px(px(11.))
            .py(px(9.))
            .flex()
            .flex_col()
            .gap(px(5.))
            .rounded(px(10.))
            .bg(rgb(if picked { RAISED } else { CARD }))
            .border_1()
            .border_color(rgb(if picked { ACCENT } else { LINE }))
            .cursor_grab()
            .hover(|d| d.border_color(rgb(if picked { ACCENT } else { LINE_STRONG })))
            .when(lifted, |d| d.opacity(0.55).border_color(rgb(ACCENT)))
            .debug_selector(move || format!("poly-fx-{k}"))
            .child(
                canvas(
                    move |b, _, _| {
                        if let Some(tiles) = &tiles {
                            tiles.borrow_mut().push((place, b));
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let x = pt(e.position).x;
                    ws.begin_drag(
                        Drag::RackTile {
                            track: t,
                            from: k,
                            to: k,
                            start: x,
                            moved: false,
                        },
                        cx,
                    );
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(grip())
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(12.5))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(if on { TEXT } else { TEXT_4 }))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(rack_name(u)),
                    )
                    .child(
                        switch(on)
                            .debug_selector(move || format!("poly-fx-power-{k}"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.poly_op(t, PolyOp::ToggleFx(k), cx);
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .h(px(26.))
                    .when(!on, |d| d.opacity(0.35))
                    .child(mini_canvas(mini(u), TEXT)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .font_family(MONO)
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(summary.unwrap_or_default()),
                    )
                    .children(riders.into_iter().take(3).map(|s| {
                        div()
                            .text_color(rgb(source_color(s)))
                            .child(source_short(s))
                    }))
                    .children(mix.map(|m| div().whitespace_nowrap().child(format!("Mix {m}")))),
            )
    }

    /// The way to add an effect, in the chain where it would go. With room, it takes what's
    /// left of the bar: the browser a click away, and the effects most likely wanted (those
    /// added lately, else the usual first ones) a click each.
    fn rack_add_tile(&self, t: usize, roomy: bool, cx: &mut Context<Self>) -> Div {
        let plus = icon(Icon::Plus, 16.).text_color(rgb(TEXT_2));
        let tile = div()
            .h(px(TILE_H))
            .rounded(px(10.))
            .border_1()
            .border_dashed()
            .border_color(rgb(LINE_STRONG))
            .cursor_pointer()
            .hover(|d| d.bg(rgb(CARD)).border_color(rgb(TEXT_4)))
            .debug_selector(|| "poly-add-effect".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.browse_effects(true, cx);
                }),
            );
        if !roomy {
            return tile
                .w(px(56.))
                .flex_none()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .text_size(px(12.))
                .text_color(rgb(TEXT_2))
                .child(plus)
                .child("Add");
        }
        // Those added lately first, by the names the rack offers them under.
        let offered = |name: &str| {
            RACK_OFFERS
                .iter()
                .flat_map(|(_, names)| names.iter())
                .find(|n| **n == name)
                .copied()
        };
        let mut picks: Vec<&'static str> = self
            .settings
            .recent_effects
            .iter()
            .filter_map(|r| offered(r))
            .collect();
        for f in FIRST {
            if !picks.contains(&f) {
                picks.push(f);
            }
        }
        picks.truncate(5);
        let chips: Vec<Div> = picks
            .into_iter()
            .map(|name| {
                div()
                    .h(px(26.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(13.))
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(LINE))
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_2))
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                    .debug_selector(move || format!("poly-quick-{name}"))
                    .child(icon(Icon::Plus, 11.).text_color(rgb(TEXT_3)))
                    .child(rack_effect(name).map_or_else(|| name.to_string(), |u| rack_name(&u)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.poly_op(t, PolyOp::AddFx(name), cx);
                        }),
                    )
            })
            .collect();
        tile.flex_1()
            .min_w(px(220.))
            .px(px(14.))
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(plus)
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child("Add an effect"),
                    )
                    .child(label("all 37, searched by what they sound like")),
            )
            .child(
                div()
                    .h(px(26.))
                    .flex()
                    .flex_wrap()
                    .gap(px(6.))
                    .overflow_hidden()
                    .children(chips),
            )
    }

    /// The effect picked, as the mixer's editor for it shows it, and what rides it.
    fn rack_stage(&mut self, t: usize, slot: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let Some(u) = p.fx.get(slot).copied() else {
            return div();
        };
        let (w, h) = self.poly_size();
        let size = (
            w - 2.0 * RACK_STAGE_PAD,
            h - HEAD_H - KEYS_H - CHAIN_H - RIDES_H - 2.0 * RACK_STAGE_PAD,
        );
        let here = Some((t, slot));
        let body = match u.kind() {
            Fx::Eq if self.eq_view.as_ref().is_some_and(|v| v.rack == here) => {
                self.eq_stage(size, cx)
            }
            Fx::Eq => None,
            _ if self.fx_view.as_ref().is_some_and(|v| v.rack == here) => {
                if let Some(v) = self.fx_view.as_mut() {
                    v.wide = Some(size.0);
                }
                self.fx_body(cx)
            }
            _ => None,
        };
        let stage = match body {
            Some(parts) => div()
                .id("poly-stage")
                .flex_1()
                .min_h(px(0.))
                .m(px(RACK_STAGE_PAD))
                .flex()
                .flex_col()
                .rounded(px(10.))
                .overflow_y_scroll()
                .bg(rgb(0x101115))
                .border_1()
                .border_color(rgb(LINE))
                .children(parts),
            // The mixer has an editor open in its own window: this one waits for it.
            None => div()
                .id("poly-stage")
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.))
                .child(label("The mixer's effect window is open"))
                .child(
                    button("Show it here", false)
                        .h(px(28.))
                        .debug_selector(|| "poly-stage-here".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.fx_view = None;
                                if ws.eq_view.as_ref().is_some_and(|v| v.rack.is_none()) {
                                    ws.close_eq(cx);
                                }
                                ws.open_rack_fx(t, slot, cx);
                            }),
                        ),
                ),
        };
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(stage)
            .child(self.rack_rides(t, slot, p, cx))
    }

    /// Under the stage: the sources to drag onto its knobs, and those riding it now.
    fn rack_rides(
        &self,
        t: usize,
        slot: usize,
        p: &Poly,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let chips = |list: Vec<PolySource>, cx: &mut Context<Self>| {
            div()
                .w(px(list.len() as f32 * 31.0))
                .flex_none()
                .flex()
                .gap(px(3.))
                .children(list.into_iter().map(|s| self.source_chip(t, p, s, cx)))
        };
        let u = p.fx.get(slot).copied();
        let rides: Vec<Div> = p
            .routes
            .iter()
            .enumerate()
            .filter_map(|(i, r)| match r.dest {
                PolyDest::Rack(s, knob) if s as usize == slot => Some((i, *r, knob)),
                _ => None,
            })
            .map(|(i, r, knob)| {
                let hue = source_color(r.source);
                let name = u
                    .and_then(|u| crate::mix::strip::knobs(u.kind()).get(knob as usize))
                    .map_or("", |q| q.name);
                div()
                    .h(px(30.))
                    .px(px(8.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(7.))
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(LINE))
                    .child(div().size(px(7.)).rounded(px(3.5)).bg(rgb(hue)))
                    .child(
                        div()
                            .text_size(px(12.))
                            .whitespace_nowrap()
                            .child(format!("{} rides {name}", source_name(r.source))),
                    )
                    .child(modulation::depth_bar(r.depth, 90.0, hue, i, t, cx))
                    .child(modulation::remove(i, t, "poly-fx-ride-remove", cx))
            })
            .collect();
        let none = rides.is_empty();
        div()
            .id("poly-rides")
            .flex_none()
            .h(px(RIDES_H))
            .px(px(20.))
            .flex()
            .items_center()
            .gap(px(10.))
            .border_t_1()
            .border_color(rgb(LINE))
            .overflow_x_scroll()
            .child(label("Ride a knob: drag a source onto it"))
            .child(chips((0..ENVS as u8).map(PolySource::Env).collect(), cx))
            .child(chips((0..LFOS as u8).map(PolySource::Lfo).collect(), cx))
            .child(chips(
                (0..MACROS as u8).map(PolySource::Macro).collect(),
                cx,
            ))
            .child(div().w(px(1.)).h(px(22.)).flex_none().bg(rgb(LINE)))
            .when(none, |d| d.child(label("Nothing rides it yet")))
            .children(rides)
    }

    /// Under the head of the effect on the stage: its place in the rack to change, or out.
    pub(crate) fn rack_moves(&self, t: usize, k: usize, cx: &mut Context<Self>) -> Div {
        let n = self.poly(t).map_or(0, |p| p.fx.len());
        let step =
            |label: &'static str, id: &'static str, op: Option<PolyOp>, cx: &mut Context<Self>| {
                button(label, false)
                    .h(px(26.))
                    .px(px(10.))
                    .text_size(px(11.5))
                    .debug_selector(move || format!("poly-fx-{id}"))
                    .when(op.is_none(), |d| d.opacity(0.35).cursor_default())
                    .when_some(op, |d, op| {
                        d.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.poly_op(t, op.clone(), cx);
                            }),
                        )
                    })
            };
        div()
            .flex()
            .gap(px(6.))
            .child(step(
                "Earlier",
                "earlier",
                (k > 0).then(|| PolyOp::MoveFx(k, k - 1)),
                cx,
            ))
            .child(step(
                "Later",
                "later",
                (k + 1 < n).then(|| PolyOp::MoveFx(k, k + 1)),
                cx,
            ))
            .child(step("Take out", "remove", Some(PolyOp::RemoveFx(k)), cx))
    }

    /// A rack tile dragged to `x`: the place it'd take, among the tiles as last drawn.
    pub(crate) fn drag_rack_tile(&mut self, x: f32) {
        let Some(Drag::RackTile { start, .. }) = self.drag else {
            return;
        };
        let nearest = self.poly_view.as_ref().and_then(|v| {
            v.rack_tiles
                .borrow()
                .iter()
                .min_by(|a, b| {
                    let d = |b: &Bounds<Pixels>| (f32::from(b.center().x) - x).abs();
                    d(&a.1).total_cmp(&d(&b.1))
                })
                .map(|r| r.0)
        });
        if let Some(Drag::RackTile { to, moved, .. }) = &mut self.drag {
            *moved |= (x - start).abs() > 4.0;
            if let (true, Some(n)) = (*moved, nearest) {
                *to = n;
            }
        }
    }

    /// A rack tile let go: moved to its new place, or (not moved) picked.
    pub(crate) fn release_rack_tile(&mut self, cx: &mut Context<Self>) {
        let Some(Drag::RackTile {
            track,
            from,
            to,
            moved,
            ..
        }) = self.drag.take()
        else {
            return;
        };
        match moved && to != from {
            true => self.poly_op(track, PolyOp::MoveFx(from, to), cx),
            false => self.poly_op(track, PolyOp::OpenFx(from), cx),
        }
    }

    /// An empty rack: what it's for, and the effects most often wanted, a click each.
    fn rack_empty(&self, t: usize, cx: &mut Context<Self>) -> Div {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(12.))
            .child(title("No effects of its own yet"))
            .child(label(
                "They play after its voices, go wherever its sound goes, and are saved with it.",
            ))
            .child(
                button("Browse every effect", true)
                    .h(px(32.))
                    .px(px(16.))
                    .debug_selector(|| "poly-browse-effects".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.browse_effects(true, cx);
                        }),
                    ),
            )
            .child(label("or start with one of these"))
            .child(div().flex().gap(px(8.)).children(FIRST.map(|name| {
                button(name, false)
                    .h(px(30.))
                    .px(px(14.))
                    .debug_selector(move || format!("poly-quick-{name}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.poly_op(t, PolyOp::AddFx(name), cx);
                        }),
                    )
            })))
    }
}

/// The chain's ends: the voices going in, the track it goes on to.
fn node(name: &'static str, w: Option<f32>, _cx: &mut Context<Workspace>) -> Div {
    div()
        .w(px(w.unwrap_or(84.)))
        .flex_none()
        .h(px(TILE_H))
        .px(px(10.))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .rounded(px(10.))
        .bg(rgb(SURFACE))
        .border_1()
        .border_color(rgb(LINE))
        .child(
            div()
                .text_size(px(12.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(name),
        )
}

/// Six dots to hold a tile by.
fn grip() -> Div {
    div()
        .w(px(8.))
        .flex_none()
        .flex()
        .flex_wrap()
        .gap(px(2.))
        .children((0..6).map(|_| div().size(px(2.5)).rounded(px(1.25)).bg(rgb(TEXT_3))))
}

/// A little picture: lines through points (0 to 1, from the bottom), drawn to fill.
pub(super) fn mini_canvas(lines: Vec<Vec<(f32, f32)>>, color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            for line in &lines {
                polyline(
                    window,
                    b,
                    line.iter().map(|&(x, y)| (x, 1.0 - y)),
                    1.5,
                    rgb(color).into(),
                );
            }
        },
    )
    .w_full()
    .h(px(26.))
}

/// The voices going in: a wave.
fn voice_wave() -> Vec<Vec<(f32, f32)>> {
    vec![wave(|x| 0.5 + (x * 12.6).sin() * 0.4)]
}

/// `f` across, sampled.
fn wave(f: impl Fn(f32) -> f32) -> Vec<(f32, f32)> {
    (0..=64)
        .map(|i| {
            let x = i as f32 / 64.0;
            (x, f(x).clamp(0.02, 0.98))
        })
        .collect()
}

/// The knob a tile names what its effect is set to by.
fn summary_knob(kind: Fx) -> Option<&'static str> {
    Some(match kind {
        Fx::Reverb => "Decay",
        Fx::Delay => "Left time",
        Fx::Sat => "Drive",
        Fx::Motion => "Rate",
        Fx::Filter => "Cutoff",
        Fx::Comp => "Threshold",
        Fx::Limit => "Ceiling",
        Fx::Stereo => "Width",
        Fx::Gate => "Threshold",
        Fx::Pitch => "Pitch",
        _ => {
            let first = crate::mix::strip::knobs(kind).first()?;
            first.name
        }
    })
}

/// A tile's picture of what its effect does, from how it's set.
pub(super) fn mini(u: &Unit) -> Vec<Vec<(f32, f32)>> {
    let kind = u.kind();
    let pos = |name: &str| {
        crate::mix::strip::knobs(kind)
            .iter()
            .find(|q| q.name == name)
            .map_or(0.5, |q| (q.pos)(u))
    };
    use pina_engine::song::project::{MotionStyle, SatStyle};
    use std::f32::consts::TAU;
    match u {
        // Each style its own curve, driven as hard as it is.
        Unit::Sat(s) => {
            let band = s.bands.first().copied().unwrap_or_default();
            let k = 1.0 + band.drive_db.clamp(0.0, 24.0) / 24.0 * 8.0;
            let soft = move |x: f32, k: f32| (k * x).tanh() / k.tanh();
            let curve = move |x: f32| -> f32 {
                let x = x * 2.0 - 1.0;
                match band.style {
                    SatStyle::WarmTape | SatStyle::CleanTape => soft(x, k * 0.7),
                    // Each half its own way.
                    SatStyle::WarmTube | SatStyle::CleanTube => match x > 0.0 {
                        true => soft(x, k),
                        false => soft(x, k * 1.6) * 0.85,
                    },
                    SatStyle::Transformer => soft(x, k * 1.2),
                    SatStyle::Crunch => soft(x, k * 2.0),
                    SatStyle::Heavy => soft(x, k * 4.0),
                    SatStyle::Rectify => x.abs() * 1.6 - 0.8,
                    SatStyle::Foldback => (x * (2.0 + k * 0.6)).sin(),
                    SatStyle::Lofi => ((x * 5.0).round() / 5.0).clamp(-1.0, 1.0),
                }
            };
            vec![wave(|x| 0.5 + curve(x) * 0.45)]
        }
        Unit::Motion(m) => {
            let f = 1.0 + pos("Rate") * 2.5;
            match m.style {
                MotionStyle::Chorus => (0..2)
                    .map(|v| wave(move |x| 0.5 + (x * f * TAU + v as f32 * 1.4).sin() * 0.3))
                    .collect(),
                MotionStyle::Ensemble => (0..3)
                    .map(|v| wave(move |x| 0.5 + (x * f * TAU + v as f32 * 2.1).sin() * 0.28))
                    .collect(),
                MotionStyle::Flanger => {
                    vec![wave(|x| 0.5 + (x * 40.0).sin() * (x * 3.0 * f).sin() * 0.4)]
                }
                MotionStyle::Phaser => vec![wave(|x| 0.85 - (x * 12.0).sin().abs() * 0.7)],
                MotionStyle::Vibrato => vec![wave(|x| {
                    0.5 + (x * 14.0 + (x * f * TAU).sin() * 2.0).sin() * 0.3
                })],
                MotionStyle::Tremolo => {
                    vec![wave(|x| match (x * f * 4.0).fract() < 0.5 {
                        true => 0.8,
                        false => 0.25,
                    })]
                }
            }
        }
        Unit::Eq(e) => {
            // Each band's bump, added, across 20 Hz to 20 kHz.
            let bands: Vec<(f32, f32, f32)> = e
                .bands
                .iter()
                .filter(|b| b.shape.has_gain())
                .map(|b| ((b.freq / 20.0).log10() / 3.0, b.gain_db, b.q))
                .collect();
            vec![wave(|x| {
                let db: f32 = bands
                    .iter()
                    .map(|&(at, g, q)| g * (-((x - at) * 6.0 * q.sqrt()).powi(2)).exp())
                    .sum();
                0.5 + db / 36.0
            })]
        }
        _ => match kind {
            Fx::Reverb => {
                let (pre, decay) = (pos("Pre-delay") * 0.15, pos("Decay"));
                vec![wave(|x| match x < pre {
                    true => 0.02,
                    false => 0.02 + 0.9 * (-(x - pre) * (7.0 - 5.5 * decay)).exp(),
                })]
            }
            Fx::Delay => {
                let (gap, fb) = (0.08 + pos("Left time") * 0.22, 0.25 + pos("Feedback") * 0.7);
                (0..12)
                    .map(|k| (0.04 + k as f32 * gap, fb.powi(k) * 0.9))
                    .take_while(|&(x, _)| x < 1.0)
                    .map(|(x, h)| vec![(x, 0.02), (x, h.max(0.03))])
                    .collect()
            }
            Fx::Filter => {
                let (c, res) = (pos("Cutoff"), pos("Resonance"));
                vec![wave(|x| {
                    let bump = res * 0.35 * (-((x - c) * 18.0).powi(2)).exp();
                    let fall = if x > c { (x - c) * 4.0 } else { 0.0 };
                    (0.7 + bump - fall).max(0.02)
                })]
            }
            Fx::Comp => {
                let (th, ratio) = (0.3 + pos("Threshold") * 0.6, 1.0 + pos("Ratio") * 9.0);
                vec![wave(|x| match x < th {
                    true => x * 0.95,
                    false => (th + (x - th) / ratio) * 0.95,
                })]
            }
            Fx::Limit => {
                let ceiling = 0.55 + pos("Ceiling") * 0.4;
                vec![wave(|x| (x * 0.95).min(ceiling))]
            }
            Fx::Stereo => {
                let w = 0.15 + pos("Width") * 0.35;
                vec![
                    (0..=48)
                        .map(|i| {
                            let a = i as f32 / 48.0 * TAU;
                            (0.5 + a.cos() * w, 0.5 + a.sin() * 0.42)
                        })
                        .collect(),
                ]
            }
            Fx::Gate => {
                let th = 0.2 + pos("Threshold") * 0.5;
                vec![wave(|x| if x < th { 0.03 } else { x * 0.95 })]
            }
            Fx::Shaper => vec![wave(|x| 0.1 + 0.85 * (-x * 9.0).exp() + 0.3 * (1.0 - x))],
            Fx::Pitch => {
                let f = 3.0 + pos("Pitch") * 8.0;
                vec![wave(|x| 0.5 + (x * x * f * TAU).sin() * 0.38)]
            }
            Fx::TapeStop => vec![wave(|x| {
                0.5 + ((x - x * x * 0.7) * 26.0).sin() * 0.38 * (1.0 - x * 0.6)
            })],
            Fx::DeEsser => vec![wave(|x| 0.6 - 0.35 * (-((x - 0.8) * 9.0).powi(2)).exp())],
            Fx::Multiband => (0..3)
                .map(|b| {
                    let at = 0.17 + b as f32 * 0.33;
                    wave(move |x| 0.1 + 0.6 * (-((x - at) * 7.0).powi(2)).exp())
                })
                .collect(),
            _ => vec![wave(|x| 0.5 + (x * 2.0 * TAU).sin() * 0.35)],
        },
    }
}
