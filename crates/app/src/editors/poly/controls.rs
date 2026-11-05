//! The window's controls: its knobs (the same knob as anywhere in the app), long sliders,
//! choices, and the few settings that aren't parameters.

use super::*;
use pina_engine::dsp::modulate::ModTarget;

/// The synth's settings that aren't automatable parameters: the arp's and its voicing's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolyAmount {
    Octaves,
    Gate,
    Swing,
    Voices,
    Bend,
}

impl PolyAmount {
    pub fn name(self) -> &'static str {
        match self {
            PolyAmount::Octaves => "Octaves",
            PolyAmount::Gate => "Gate",
            PolyAmount::Swing => "Swing",
            PolyAmount::Voices => "Voices",
            PolyAmount::Bend => "Bend",
        }
    }

    /// Where its knob sits, 0 to 1.
    pub fn get(self, p: &Poly) -> f32 {
        match self {
            PolyAmount::Octaves => (p.arp.octaves as f32 - 1.0) / 3.0,
            PolyAmount::Gate => (p.arp.gate - 0.05) / 0.95,
            PolyAmount::Swing => p.arp.swing / 0.75,
            PolyAmount::Voices => (p.voicing.polyphony as f32 - 1.0) / 31.0,
            PolyAmount::Bend => p.voicing.bend_semitones / 24.0,
        }
        .clamp(0.0, 1.0)
    }

    pub fn set(self, p: &mut Poly, pos: f32) {
        let pos = pos.clamp(0.0, 1.0);
        match self {
            PolyAmount::Octaves => p.arp.octaves = (1.0 + pos * 3.0).round() as u8,
            PolyAmount::Gate => p.arp.gate = 0.05 + pos * 0.95,
            PolyAmount::Swing => p.arp.swing = pos * 0.75,
            PolyAmount::Voices => p.voicing.polyphony = (1.0 + pos * 31.0).round() as u8,
            PolyAmount::Bend => p.voicing.bend_semitones = (pos * 24.0).round(),
        }
    }

    pub fn say(self, p: &Poly) -> String {
        match self {
            PolyAmount::Octaves => p.arp.octaves.to_string(),
            PolyAmount::Gate => format!("{:.0}%", p.arp.gate * 100.0),
            PolyAmount::Swing => format!("{:.0}%", p.arp.swing / 0.75 * 100.0),
            PolyAmount::Voices => p.voicing.polyphony.to_string(),
            PolyAmount::Bend => format!("±{:.0}", p.voicing.bend_semitones),
        }
    }
}

/// A knob's name, and its value in place of the name while pointed at (a small one), or under
/// it (a large one).
fn caption(
    name: gpui::SharedString,
    text: String,
    small: bool,
    showing: bool,
    group: gpui::SharedString,
) -> Div {
    let name = div()
        .text_size(px(11.))
        .text_color(rgb(TEXT_3))
        .whitespace_nowrap()
        .child(name);
    let value = div()
        .text_size(px(10.5))
        .font_family(MONO)
        .text_color(rgb(TEXT_2))
        .whitespace_nowrap()
        .child(text);
    match (small, showing) {
        (false, _) => div()
            .flex()
            .flex_col()
            .items_center()
            .child(name)
            .child(value),
        (true, true) => div().h(px(15.)).child(value),
        (true, false) => div()
            .relative()
            .h(px(15.))
            .flex()
            .justify_center()
            .child(name.group_hover(group.clone(), |s| s.invisible()))
            .child(
                value
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .invisible()
                    .group_hover(group, |s| s.visible()),
            ),
    }
}

impl Workspace {
    /// One of the synth's knobs, the same as any in the app: dragged, ⌘- or double-clicked back
    /// to its default, right-clicked for more. Lit in a source's colour while that source, if
    /// it moves it, is pointed at.
    pub(crate) fn poly_knob(
        &self,
        t: usize,
        i: usize,
        name: impl Into<gpui::SharedString>,
        size: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        self.poly_knob_lit(t, i, Some(name.into()), size, TEXT, cx)
    }

    /// The same, `arc` its colour; with no name, just the dial.
    pub(crate) fn poly_knob_lit(
        &self,
        t: usize,
        i: usize,
        name: Option<gpui::SharedString>,
        size: f32,
        arc: u32,
        cx: &mut Context<Self>,
    ) -> Div {
        let target = Target::Poly { track: t, param: i };
        let value = self.target(target);
        let q = &POLY_PARAMS[i];
        let reset = q.scale.pos(q.default);
        let shown = if q.bipolar { value * 2.0 - 1.0 } else { value };
        let text = q.say(q.scale.value(value));
        let pointed = self.poly_view.as_ref().and_then(|v| v.pointed);
        let lit = pointed.filter(|s| {
            self.poly(t).is_some_and(|p| {
                p.routes
                    .iter()
                    .any(|r| r.source == *s && r.dest == PolyDest::Param(i as u16))
            })
        });
        let sel = q.id.clone();
        let dragging = matches!(self.drag, Some(Drag::Knob { target: g, .. }) if g == target);
        let group: gpui::SharedString = format!("poly-k-{i}").into();
        let dial = div()
            .relative()
            .rounded_full()
            .cursor_ns_resize()
            .when_some(lit, |d, s| d.bg(alpha(source_color(s), 0.32)))
            .debug_selector(move || format!("poly-knob-{sel}"))
            .child(self.modded(
                self.mod_key(target),
                size,
                knob(shown, size, arc, q.bipolar),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if e.modifiers.platform || e.click_count == 2 {
                        ws.edit(cx, |ws| ws.set_target(target, reset));
                        return;
                    }
                    let y = pt(e.position).y;
                    ws.begin_drag(Drag::Knob { target, y, value }, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.poly_knob_menu(t, i, e.position, cx);
                }),
            );
        let Some(name) = name else {
            return div().flex_none().child(dial);
        };
        div()
            .group(group.clone())
            .flex_none()
            .min_w(px(size + 12.0))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(3.))
            .child(dial)
            .child(caption(name, text, size < 36.0, dragging, group))
    }

    /// What a knob's right-click offers: back to its default, a lane on the track, and each
    /// source on it taken off.
    fn poly_knob_menu(
        &mut self,
        t: usize,
        i: usize,
        at: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let q = &POLY_PARAMS[i];
        let mut items = vec![
            (
                format!("Back to {}", q.say(q.default)),
                MenuAction::Poly(t, PolyOp::Reset(i)),
            ),
            (
                "Automate it on the track".to_string(),
                MenuAction::Poly(t, PolyOp::Automate(i)),
            ),
        ];
        if let Some(p) = self.poly(t) {
            for (k, r) in p.routes.iter().enumerate() {
                if r.dest == PolyDest::Param(i as u16) {
                    items.push((
                        format!("Take {} off it", source_name(r.source)),
                        MenuAction::Poly(t, PolyOp::RemoveRoute(k)),
                    ));
                }
            }
        }
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    /// A long slider for a parameter, `h` tall: its fill white, the reach of what moves it in that
    /// source's colour, and a dot where that has it now. Clicked, it goes there.
    pub(crate) fn poly_slider(&self, t: usize, i: usize, h: f32, cx: &mut Context<Self>) -> Div {
        let target = Target::Poly { track: t, param: i };
        let value = self.target(target);
        let q = &POLY_PARAMS[i];
        let reset = q.scale.pos(q.default);
        let zero = if q.bipolar { 0.5 } else { 0.0 };
        let rings = self.mod_rings(t, ModTarget::Poly(i as u16));
        let offering = matches!(self.drag, Some(Drag::PolySource { track, .. }) if track == t);
        let sliders = self.poly_view.as_ref().map(|v| v.sliders.clone());
        let targets = self.mod_targets.clone();
        let sel = q.id.clone();
        div()
            .relative()
            .w_full()
            .h(px(h))
            .flex_none()
            .cursor_ew_resize()
            .debug_selector(move || format!("poly-slider-{sel}"))
            .child(
                canvas(
                    move |b, _, _| {
                        if let Some(s) = &sliders {
                            s.borrow_mut().push((i, b));
                        }
                        targets.borrow_mut().push((t, ModTarget::Poly(i as u16), b));
                    },
                    move |b, _, window, _| {
                        let (x0, w) = (b.origin.x, f32::from(b.size.width));
                        let mid = b.origin.y + b.size.height / 2.0;
                        let bar = |from: f32, to: f32, tall: f32| {
                            Bounds::new(
                                point(x0 + px(w * from.min(to)), mid - px(tall / 2.0)),
                                gpui::size(px((w * (to - from).abs()).max(1.0)), px(tall)),
                            )
                        };
                        if offering {
                            window.paint_quad(
                                gpui::fill(b, alpha(ACCENT, 0.12)).corner_radii(px(4.)),
                            );
                        }
                        window.paint_quad(
                            gpui::fill(bar(0.0, 1.0, 4.0), rgb(LINE_STRONG)).corner_radii(px(2.)),
                        );
                        for r in &rings {
                            window
                                .paint_quad(gpui::fill(bar(r.lo, r.hi, 4.0), alpha(r.color, 0.55)));
                        }
                        window.paint_quad(
                            gpui::fill(bar(zero, value, 4.0), rgb(TEXT)).corner_radii(px(2.)),
                        );
                        let thumb = Bounds::centered_at(
                            point(x0 + px(w * value), mid),
                            gpui::size(px(4.), px((h - 4.0).clamp(8.0, 16.0))),
                        );
                        window.paint_quad(gpui::fill(thumb, rgb(TEXT)).corner_radii(px(2.)));
                        for r in &rings {
                            window.paint_quad(
                                gpui::fill(
                                    Bounds::centered_at(
                                        point(x0 + px(w * r.live), mid),
                                        gpui::size(px(6.), px(6.)),
                                    ),
                                    rgb(r.color),
                                )
                                .corner_radii(px(3.)),
                            );
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if e.modifiers.platform || e.click_count == 2 {
                        ws.edit(cx, |ws| ws.set_target(target, reset));
                        return;
                    }
                    let Some(b) = ws.poly_view.as_ref().and_then(|v| {
                        v.sliders
                            .borrow()
                            .iter()
                            .rev()
                            .find(|s| s.0 == i)
                            .map(|s| s.1)
                    }) else {
                        return;
                    };
                    let (left, width) = (f32::from(b.origin.x), f32::from(b.size.width));
                    ws.begin_drag(
                        Drag::PolySlider {
                            target,
                            left,
                            width,
                        },
                        cx,
                    );
                    ws.slide_poly(target, left, width, pt(e.position).x);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.poly_knob_menu(t, i, e.position, cx);
                }),
            )
    }

    /// A long slider moved to `x`.
    pub(crate) fn slide_poly(&mut self, target: Target, left: f32, width: f32, x: f32) {
        let v = ((x - left) / width.max(1.0)).clamp(0.0, 1.0);
        self.set_target(target, v);
        if let Some((t, param)) = self.target_param(target) {
            let set = self.own_value(t, param);
            self.moved(t, param, set);
        }
    }

    /// A knob for a setting that isn't a parameter (the arp's, the voicing's): no rings, no lane.
    pub(crate) fn poly_amount(
        &self,
        t: usize,
        amount: PolyAmount,
        p: &Poly,
        cx: &mut Context<Self>,
    ) -> Div {
        let value = amount.get(p);
        let group: gpui::SharedString = format!("poly-a-{amount:?}").into();
        let sel = format!("poly-amount-{amount:?}").to_lowercase();
        div()
            .group(group.clone())
            .flex_none()
            .min_w(px(42.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(3.))
            .child(
                div()
                    .cursor_ns_resize()
                    .debug_selector(move || sel.clone())
                    .child(knob(value, 30.0, TEXT, false))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let y = pt(e.position).y;
                            ws.begin_drag(
                                Drag::PolyAmount {
                                    track: t,
                                    amount,
                                    y,
                                    value,
                                },
                                cx,
                            );
                        }),
                    ),
            )
            .child(caption(
                amount.name().into(),
                amount.say(p),
                true,
                false,
                group,
            ))
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
        dropdown(label, 28.)
            .bg(rgb(FIELD))
            .border_color(rgb(FIELD))
            .debug_selector(move || sel.clone())
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

    /// A row of choices, one lit; each changes the synth.
    pub(crate) fn poly_choices<T: Copy + PartialEq + 'static>(
        &self,
        t: usize,
        sel: &'static str,
        now: T,
        choices: &[(&'static str, T)],
        set: fn(&mut Poly, T),
        cx: &mut Context<Self>,
    ) -> Div {
        tray()
            .p(px(2.))
            .rounded(px(7.))
            .children(choices.iter().map(|&(name, c)| {
                seg(name, now == c, 24.)
                    .debug_selector(move || format!("{sel}-{}", name.to_lowercase()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit(cx, |ws| {
                                if let Some(p) = ws.poly_mut(t) {
                                    set(p, c);
                                }
                            });
                        }),
                    )
            }))
    }

    /// A switch on the synth, lit while it's on.
    pub(crate) fn poly_switch(
        &self,
        t: usize,
        name: &'static str,
        sel: &'static str,
        on: bool,
        flip: fn(&mut Poly),
        cx: &mut Context<Self>,
    ) -> Div {
        seg(name, on, 24.)
            .bg(rgb(if on { RAISED } else { WELL }))
            .debug_selector(move || sel.into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.edit(cx, |ws| {
                        if let Some(p) = ws.poly_mut(t) {
                            flip(p);
                        }
                    });
                }),
            )
    }
}
