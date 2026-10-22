//! Each band's dot on the plot, and the readout beside it.

use super::*;

impl Workspace {
    /// A band's dot: its color, ringed white when picked, hollow when off.
    pub(crate) fn eq_dot(
        &self,
        d: &Drawn,
        picked: &[u16],
        hover: Option<u16>,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = d.id;
        let on = picked.contains(&id);
        let lit = on || hover == Some(id);
        let size = if lit { 16.0 } else { 13.0 };
        let i = d.index;
        div()
            .absolute()
            .left(px(d.dot.0 - size / 2.0))
            .top(px(d.dot.1 - size / 2.0))
            .size(px(size))
            .rounded_full()
            .cursor_pointer()
            .when(!d.band.off, |n| n.bg(rgb(d.color)))
            .when(d.band.off, |n| {
                n.bg(rgb(0x14151a))
                    .border_2()
                    .border_color(alpha(d.color, 0.55))
            })
            .when(on, |n| n.border_2().border_color(rgb(0xffffff)))
            .when(d.band.dynamic.is_some() && d.band.shape.has_gain(), |n| {
                n.shadow(vec![gpui::BoxShadow {
                    color: alpha(d.color, 0.6),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(3.),
                    inset: false,
                }])
            })
            .debug_selector(move || format!("eq-band-{i}"))
            .on_mouse_move(cx.listener(move |ws, _: &MouseMoveEvent, _, cx| {
                cx.stop_propagation();
                if let Some(v) = ws.eq_view.as_mut()
                    && v.hover != Some(id)
                {
                    v.hover = Some(id);
                    cx.notify();
                }
            }))
            .on_scroll_wheel(cx.listener(move |ws, e: &ScrollWheelEvent, _, cx| {
                cx.stop_propagation();
                ws.wheel_band(id, e, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.press_dot(id, e, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.band_menu(id, e.position, cx);
                }),
            )
    }

    /// Beside a band's dot: its frequency (and note), gain and width, each to drag, scroll or
    /// double-click to type; and quick buttons for off, solo, delete and its menu.
    pub(crate) fn eq_readout(&self, d: &Drawn, g: &Geo, cx: &mut Context<Self>) -> Div {
        let id = d.id;
        let b = d.band;
        let piano = self.eq_view.as_ref().is_some_and(|v| v.piano);
        let value = |knob: EqKnob, text: String, cx: &mut Context<Self>| -> Div {
            let field = self.name_field(Name::Eq(id, knob));
            div()
                .h(px(18.))
                .px(px(4.))
                .flex()
                .items_center()
                .rounded(px(3.))
                .cursor_ns_resize()
                .hover(|s| s.bg(alpha(0xffffff, 0.08)))
                .debug_selector(move || format!("eq-value-{}", knob.label().to_lowercase()))
                .map(|el| match field {
                    Some(input) => el.w(px(70.)).bg(rgb(FIELD)).child(input),
                    None => el.child(text),
                })
                .on_scroll_wheel(cx.listener(move |ws, e: &ScrollWheelEvent, _, cx| {
                    cx.stop_propagation();
                    let dy = f32::from(e.delta.pixel_delta(px(16.)).y);
                    let v = ws.eq_knob_value(id, knob);
                    let pos = knob.pos(v) + dy / 800.0;
                    ws.edit(cx, |ws| ws.set_eq_knob(id, knob, knob.value(pos)));
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        ws.press_knob(id, knob, e, cx);
                    }),
                )
        };
        let quick = |glyph: Icon, sel: &'static str, on: bool, color: u32| -> Div {
            div()
                .size(px(18.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .cursor_pointer()
                .when(on, |s| s.bg(alpha(color, 0.22)))
                .hover(|s| s.bg(alpha(0xffffff, 0.1)))
                .debug_selector(move || sel.into())
                .child(icon(glyph, 11.).text_color(rgb(if on { color } else { TEXT_3 })))
        };
        let freq = match piano {
            true => format!("{}  {}", hz_text(b.freq), note_text(b.freq)),
            false => hz_text(b.freq),
        };
        let solo = self
            .eq_view
            .as_ref()
            .and_then(|v| v.solo)
            .is_some_and(|s| s.id == id);
        let w = 238.0;
        let left = (d.dot.0 - w / 2.0).clamp(4.0, g.w - w - 4.0);
        let top = if d.dot.1 > 56.0 {
            d.dot.1 - 50.0
        } else {
            d.dot.1 + 16.0
        };
        div()
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(w))
            .flex()
            .justify_center()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .px(px(4.))
                    .py(px(3.))
                    .rounded(px(6.))
                    .bg(alpha(0x08090c, 0.9))
                    .border_1()
                    .border_color(alpha(d.color, 0.55))
                    .font_family(MONO)
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT))
                    .debug_selector(|| "eq-readout".into())
                    .on_mouse_move(cx.listener(move |ws, _: &MouseMoveEvent, _, cx| {
                        // Over its readout, the band stays the one told.
                        cx.stop_propagation();
                        if let Some(v) = ws.eq_view.as_mut()
                            && v.hover != Some(id)
                        {
                            v.hover = Some(id);
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(value(EqKnob::Freq, freq, cx))
                    .when(b.shape.has_gain(), |el| {
                        el.child(value(
                            EqKnob::Gain,
                            format!("{} dB", db_text(b.gain_db)),
                            cx,
                        ))
                    })
                    .child(value(EqKnob::Q, format!("Q {}", q_text(b.q)), cx))
                    .child(div().w(px(1.)).h(px(14.)).bg(rgb(LINE_STRONG)))
                    .child(
                        quick(Icon::Mute, "eq-quick-off", b.off, RECORD).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.eq_op(EqOp::Pick(id, false), cx);
                                ws.eq_op(EqOp::Off, cx);
                            }),
                        ),
                    )
                    .child(
                        quick(Icon::Headphones, "eq-quick-solo", solo, SOLO).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.press_solo(id, e, cx);
                            }),
                        ),
                    )
                    .child(
                        quick(Icon::Close, "eq-quick-delete", false, RECORD).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.eq_op(EqOp::Pick(id, false), cx);
                                ws.eq_op(EqOp::Delete, cx);
                            }),
                        ),
                    )
                    .child(
                        quick(Icon::Chevron, "eq-quick-menu", false, TEXT).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.band_menu(id, e.position, cx);
                            }),
                        ),
                    ),
            )
    }
}
