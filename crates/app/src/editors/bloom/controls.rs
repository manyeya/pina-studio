//! The plant's controls along the bottom, its knobs (the same knob as anywhere in the app), the
//! keys' modes, and the keyboard, each key in the colour of the branch it plays.

use super::*;
use crate::editors::poly::keys::{HIGHEST, LOWEST, key_at};

fn black(n: u8) -> bool {
    matches!(n % 12, 1 | 3 | 6 | 8 | 10)
}

impl Workspace {
    /// One of the plant's knobs: dragged, ⌘- or double-clicked back to its default, right-clicked
    /// for its lane. Its arc in `arc`.
    pub(crate) fn bloom_knob(
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
        let q = &BLOOM_PARAMS[i];
        let reset = q.scale.pos(q.default);
        let shown = if q.bipolar { value * 2.0 - 1.0 } else { value };
        let text = q.say(q.scale.value(value));
        let sel = q.id.clone();
        let dragging = matches!(self.drag, Some(Drag::Knob { target: g, .. }) if g == target);
        let dial = div()
            .relative()
            .rounded_full()
            .cursor_ns_resize()
            .debug_selector(move || format!("bloom-knob-{sel}"))
            .child(self.modded(
                self.mod_key(target),
                size,
                knob(shown, size, arc, q.bipolar),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.wake_bloom();
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
                    let q = &BLOOM_PARAMS[i];
                    let items = vec![
                        (
                            format!("Back to {}", q.say(q.default)),
                            MenuAction::Bloom(t, BloomOp::Reset(i)),
                        ),
                        (
                            "Automate it on the track".to_string(),
                            MenuAction::Bloom(t, BloomOp::Automate(i)),
                        ),
                    ];
                    ws.bloom_menu(e.position, items, cx);
                }),
            );
        let Some(name) = name else {
            return div().flex_none().child(dial);
        };
        div()
            .flex_none()
            .min_w(px(size + 14.0))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(3.))
            .child(dial)
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(if dragging { TEXT } else { TEXT_3 }))
                    .whitespace_nowrap()
                    .child(name),
            )
            .child(
                div()
                    .text_size(px(10.5))
                    .font_family(MONO)
                    .text_color(rgb(TEXT_2))
                    .whitespace_nowrap()
                    .child(text),
            )
    }

    /// A parameter as a long slider, `color` its fill: dragged or clicked, it goes there.
    pub(crate) fn bloom_slider(
        &self,
        t: usize,
        i: usize,
        color: u32,
        cx: &mut Context<Self>,
    ) -> Div {
        let target = Target::Poly { track: t, param: i };
        let value = self.target(target);
        let q = &BLOOM_PARAMS[i];
        let zero = if q.bipolar { 0.5 } else { 0.0 };
        let reset = q.scale.pos(q.default);
        let sel = q.id.clone();
        let rings = self.mod_rings(t, pina_engine::dsp::modulate::ModTarget::Poly(i as u16));
        let targets = self.mod_targets.clone();
        div()
            .relative()
            .w_full()
            .h(px(18.))
            .flex_none()
            .cursor_ew_resize()
            .debug_selector(move || format!("bloom-slider-{sel}"))
            .child(
                crate::ui::scale::scaled_canvas(
                    move |b, _, _| {
                        targets.borrow_mut().push((
                            t,
                            pina_engine::dsp::modulate::ModTarget::Poly(i as u16),
                            b,
                        ));
                    },
                    move |b, _, window, _| {
                        let (x0, w) = (b.origin.x, f32::from(b.size.width));
                        let mid = b.origin.y + b.size.height / 2.0;
                        let bar = |from: f32, to: f32, tall: f32| {
                            Bounds::new(
                                point(x0 + gpui::px(w * from.min(to)), mid - gpui::px(tall / 2.0)),
                                gpui::size(
                                    gpui::px((w * (to - from).abs()).max(1.0)),
                                    gpui::px(tall),
                                ),
                            )
                        };
                        window.paint_quad(
                            gpui::fill(bar(0.0, 1.0, 4.0), rgb(LINE_STRONG))
                                .corner_radii(gpui::px(2.)),
                        );
                        for r in &rings {
                            window
                                .paint_quad(gpui::fill(bar(r.lo, r.hi, 4.0), alpha(r.color, 0.55)));
                        }
                        window.paint_quad(
                            gpui::fill(bar(zero, value, 4.0), rgb(color))
                                .corner_radii(gpui::px(2.)),
                        );
                        let thumb = Bounds::centered_at(
                            point(x0 + gpui::px(w * value), mid),
                            gpui::size(gpui::px(10.), gpui::px(10.)),
                        );
                        window.paint_quad(
                            gpui::fill(thumb, rgb(TEXT))
                                .corner_radii(gpui::px(5.))
                                .border_widths(gpui::px(2.))
                                .border_color(rgb(color)),
                        );
                    },
                )
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
                    let Some(b) = ws
                        .mod_targets
                        .borrow()
                        .iter()
                        .rev()
                        .find(|(tt, mt, _)| {
                            *tt == t && *mt == pina_engine::dsp::modulate::ModTarget::Poly(i as u16)
                        })
                        .map(|x| x.2)
                    else {
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
                    ws.changed(cx);
                }),
            )
    }

    pub(crate) fn bloom_controls(&mut self, t: usize, b: &Bloom, cx: &mut Context<Self>) -> Div {
        let hue = hue_u32(b.chosen as usize);
        let k = |ws: &Self,
                 i: usize,
                 name: &'static str,
                 size: f32,
                 arc: u32,
                 cx: &mut Context<Self>| {
            ws.bloom_knob(t, i, Some(name.into()), size, arc, cx)
        };
        let group = |title: &'static str, d: Div| {
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(section_title(title))
                .child(d.flex().items_start().gap(px(10.)))
        };
        let keys = BloomKeys::ALL.map(|m| {
            seg(m.name(), b.keys == m, 24.)
                .debug_selector(move || format!("bloom-keys-{}", m.name().to_lowercase()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.bloom_op(t, BloomOp::Keys(m), cx);
                    }),
                )
        });
        let mono = b.voicing.mono;
        let voices = b.voicing.polyphony;
        let voice_seg = |name: &'static str, on: bool, m: bool, cx: &mut Context<Self>| {
            seg(name, on, 22.).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.bloom_op(t, BloomOp::Mono(m), cx);
                }),
            )
        };
        let voice_menu = button(format!("{voices} voices"), false)
            .debug_selector(|| "bloom-voices".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let items = [4u8, 8, 12, 16, 24, 32]
                        .iter()
                        .map(|&n| {
                            (
                                format!("{n} voices"),
                                MenuAction::Bloom(t, BloomOp::Voices(n)),
                            )
                        })
                        .collect();
                    ws.bloom_menu(e.position, items, cx);
                }),
            );
        let sync = b.sync;
        div()
            .flex_none()
            .h(px(CONTROLS_H))
            .px(px(18.))
            .py(px(8.))
            .flex()
            .items_start()
            .gap(px(22.))
            .border_t_1()
            .border_color(rgb(LINE))
            .bg(rgb(CARD))
            .child(group(
                "Growth",
                div()
                    .child(k(self, BLOOM_GROWTH, "Growth", 46.0, hue, cx))
                    .child(k(self, BLOOM_WILD, "Wildness", 36.0, hue, cx))
                    .child(k(self, BLOOM_ATONAL, "Atonality", 36.0, hue, cx)),
            ))
            .child(div().w(px(1.)).h_full().bg(rgb(LINE)))
            .child(group(
                "Sound",
                div()
                    .child(k(self, BLOOM_EFFECT, "Effect", 36.0, TEXT, cx))
                    .child(k(self, BLOOM_TUNE, "Tune", 36.0, TEXT, cx))
                    .child(k(self, BLOOM_RELEASE, "Release", 36.0, TEXT, cx))
                    .child(k(self, BLOOM_GLIDE, "Glide", 36.0, TEXT, cx))
                    .child(k(self, BLOOM_VOLUME, "Volume", 36.0, TEXT, cx)),
            ))
            .child(div().w(px(1.)).h_full().bg(rgb(LINE)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(5.))
                    .child(section_title("Keys"))
                    .child(tray().p(px(2.)).rounded(px(7.)).children(keys))
                    .child(hint(b.keys.about()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                tray()
                                    .p(px(2.))
                                    .rounded(px(7.))
                                    .child(voice_seg("Poly", !mono, false, cx))
                                    .child(voice_seg("Mono", mono, true, cx)),
                            )
                            .child(voice_menu)
                            .child(
                                seg("LFO in time", sync, 22.)
                                    .debug_selector(|| "bloom-sync".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            cx.stop_propagation();
                                            ws.bloom_op(t, BloomOp::Sync(!sync), cx);
                                        }),
                                    ),
                            ),
                    ),
            )
            .child(div().w(px(1.)).h_full().bg(rgb(LINE)))
            .child(group(
                "Grown by",
                div()
                    .child(k(self, BLOOM_WHEEL, "Wheel", 30.0, hue, cx))
                    .child(k(self, BLOOM_SLIDE, "Slide", 30.0, hue, cx)),
            ))
    }

    /// The pointer on the window's keys: the note it's over now, the one before let go.
    pub(crate) fn bloom_finger(&mut self, at: gpui::Point<f32>) {
        let Some(v) = self.bloom_view.as_ref() else {
            return;
        };
        let b = v.keys.get();
        let x = (at.x - f32::from(b.origin.x)) / f32::from(b.size.width).max(1.0);
        let y = (at.y - f32::from(b.origin.y)) / f32::from(b.size.height).max(1.0);
        let now = key_at(x, y, LOWEST, HIGHEST);
        let was = v.finger;
        if now == was {
            return;
        }
        if let Some(p) = was {
            self.play_note(p, 0);
        }
        if let Some(p) = now {
            self.play_note(p, (50.0 + 77.0 * y.clamp(0.0, 1.0)).round() as u8);
        }
        if let Some(v) = self.bloom_view.as_mut() {
            v.finger = now;
        }
    }

    pub(crate) fn bloom_lift(&mut self) {
        if let Some(p) = self.bloom_view.as_mut().and_then(|v| v.finger.take()) {
            self.play_note(p, 0);
        }
    }

    pub(crate) fn bloom_keys_row(&self, t: usize, b: &Bloom, cx: &mut Context<Self>) -> Div {
        let Some(v) = self.bloom_view.as_ref() else {
            return div();
        };
        let held = v.held.clone();
        let bounds = v.keys.clone();
        let (wheel, bend) = (v.wheel, v.bend);
        let typed = self
            .typing
            .map(|o| (60 + 12 * o as i32).clamp(0, 127) as u8);
        // The branch each key plays, as the keys are set.
        let plays: Vec<Option<usize>> = (LOWEST..=HIGHEST)
            .map(|n| match b.keys {
                BloomKeys::Octave => Some(n as usize % 12),
                BloomKeys::Spread => Some((n.saturating_sub(24) as usize * 12 / 85).min(11)),
                BloomKeys::One => Some(b.chosen as usize % BRANCHES),
                _ => None,
            })
            .collect();
        let layer: Vec<usize> = (0..BRANCHES).filter(|&k| b.layered(k)).collect();
        let mode = b.keys;
        let keys = crate::ui::scale::scaled_canvas(
            move |bb, _, _| bounds.set(bb),
            move |bb, _, window, _| {
                let whites: Vec<u8> = (LOWEST..=HIGHEST).filter(|n| !black(*n)).collect();
                let w = f32::from(bb.size.width) / whites.len() as f32;
                let h = f32::from(bb.size.height);
                let at = |x: f32, top: f32, wide: f32, tall: f32| {
                    Bounds::new(
                        point(bb.origin.x + gpui::px(x), bb.origin.y + gpui::px(top)),
                        gpui::size(gpui::px(wide), gpui::px(tall)),
                    )
                };
                let colour = |n: u8, a: f32| -> gpui::Hsla {
                    match (mode, plays[(n - LOWEST) as usize]) {
                        (_, Some(k)) => branch_hue(k, 0.62, a),
                        (BloomKeys::Layer, _) => {
                            let k = layer
                                .get(n as usize % layer.len().max(1))
                                .copied()
                                .unwrap_or(0);
                            branch_hue(k, 0.62, a * 0.8)
                        }
                        _ => {
                            let k = (n as usize * 5) % BRANCHES;
                            branch_hue(k, 0.62, a * 0.6)
                        }
                    }
                };
                for (i, &n) in whites.iter().enumerate() {
                    let down = held.contains(&n);
                    window.paint_quad(
                        gpui::fill(
                            at(i as f32 * w, 0.0, w - 1.0, h),
                            rgb(if down { 0xc9ccc8 } else { 0xe4e6e2 }),
                        )
                        .corner_radii(gpui::Corners {
                            bottom_left: gpui::px(3.),
                            bottom_right: gpui::px(3.),
                            ..Default::default()
                        }),
                    );
                    window.paint_quad(gpui::fill(
                        at(i as f32 * w, 0.0, w - 1.0, 4.0),
                        colour(n, 0.95),
                    ));
                    if down {
                        window.paint_quad(
                            gpui::fill(at(i as f32 * w, h - 10.0, w - 1.0, 10.0), colour(n, 0.85))
                                .corner_radii(gpui::Corners {
                                    bottom_left: gpui::px(3.),
                                    bottom_right: gpui::px(3.),
                                    ..Default::default()
                                }),
                        );
                    }
                    if Some(n) == typed {
                        window.paint_quad(
                            gpui::fill(
                                at(i as f32 * w + w / 2.0 - 3.0, h - 22.0, 6.0, 6.0),
                                rgb(ACCENT),
                            )
                            .corner_radii(gpui::px(3.)),
                        );
                    }
                }
                for (i, &n) in whites.iter().enumerate() {
                    if n < HIGHEST && black(n + 1) {
                        let down = held.contains(&(n + 1));
                        let k = at((i as f32 + 0.7) * w, 0.0, w * 0.6, h * 0.62);
                        window.paint_quad(
                            gpui::fill(k, rgb(if down { 0x3d4240 } else { 0x141716 }))
                                .corner_radii(gpui::Corners {
                                    bottom_left: gpui::px(2.),
                                    bottom_right: gpui::px(2.),
                                    ..Default::default()
                                }),
                        );
                        window.paint_quad(gpui::fill(
                            at((i as f32 + 0.7) * w, 0.0, w * 0.6, 3.0),
                            colour(n + 1, if down { 1.0 } else { 0.8 }),
                        ));
                    }
                }
            },
        )
        .size_full();
        let wheel_strip = |bend: bool, value: f32, cx: &mut Context<Self>| {
            let fill = if bend { (value + 1.0) / 2.0 } else { value };
            div()
                .relative()
                .w(px(18.))
                .h_full()
                .rounded(px(4.))
                .bg(rgb(WELL))
                .overflow_hidden()
                .cursor_ns_resize()
                .debug_selector(move || if bend { "bloom-bend" } else { "bloom-wheel" }.into())
                .when(!bend, |d| {
                    d.child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .h(gpui::relative(fill))
                            .bg(alpha(hue_u32(b.chosen as usize), 0.45)),
                    )
                })
                .child(
                    div()
                        .absolute()
                        .left(px(2.))
                        .right(px(2.))
                        .h(px(4.))
                        .bottom(gpui::relative(fill))
                        .mb(px(-2.))
                        .rounded(px(2.))
                        .bg(rgb(TEXT)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let Some(b) = ws.bloom_view.as_ref().map(|v| v.keys.get()) else {
                            return;
                        };
                        let (top, height) = (f32::from(b.origin.y), f32::from(b.size.height));
                        ws.begin_drag(
                            Drag::PolyWheel {
                                track: t,
                                bend,
                                top,
                                height,
                            },
                            cx,
                        );
                        ws.poly_wheel(t, bend, top, height, pt(e.position).y);
                        cx.notify();
                    }),
                )
        };
        div()
            .flex_none()
            .h(px(KEYS_H))
            .px(px(12.))
            .py(px(8.))
            .flex()
            .gap(px(10.))
            .border_t_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .h_full()
                    .flex()
                    .gap(px(4.))
                    .child(wheel_strip(true, bend, cx))
                    .child(wheel_strip(false, wheel, cx)),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .debug_selector(|| "bloom-keys".into())
                    .child(keys)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.begin_drag(Drag::BloomKeys, cx);
                            ws.bloom_finger(pt(e.position));
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .w(px(150.))
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap(px(3.))
                    .child(small("Bend · Grow"))
                    .child(hint("The mod wheel grows every branch"))
                    .child(small("Type A to K to play")),
            )
    }
}
