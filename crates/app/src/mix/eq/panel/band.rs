//! The band panel: the shapes along its top, slope, and the dials for frequency, gain and width.

use super::*;

impl Workspace {
    pub(crate) fn band_panel(
        &self,
        _eq: &Eq,
        g: &Geo,
        drawn: &[Drawn],
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let view = self.eq_view.as_ref()?;
        let lead = view.lead()?;
        let d = drawn.iter().find(|d| d.id == lead)?;
        let picked: Vec<&Drawn> = drawn
            .iter()
            .filter(|d| view.picked.contains(&d.id))
            .collect();
        let b = d.band;
        let id = d.id;
        let color = d.color;
        let dynamic = b.dynamic.filter(|_| b.shape.has_gain());
        let open = view.dyn_open && dynamic.is_some();
        let w = W.min(g.w - 16.0);
        let center = picked.iter().map(|p| p.dot.0).sum::<f32>() / picked.len() as f32;
        let left = (center - w / 2.0).clamp(8.0, (g.w - w - 8.0).max(8.0));
        // At the bottom of the display, unless it'd hide its band's dot (not just touch it)
        // and there's room above.
        let h = if open { H_OPEN } else { H };
        let low_top = g.h - 10.0 - h;
        let up = d.dot.1 > low_top + 12.0 && d.dot.1 > 10.0 + h + 14.0;
        // Pointing at the band it's for, if that's clear of it.
        let notch = (picked.len() == 1 && (up || d.dot.1 < low_top - 6.0))
            .then(|| (center - left).clamp(24.0, w - 24.0));
        let solo = view.solo.is_some_and(|s| s.id == id);
        let order = self.eq_order();
        let at = order.iter().position(|&o| o == id).unwrap_or(0);
        let (before, after) = (
            at.checked_sub(1).map(|i| order[i]),
            order.get(at + 1).copied(),
        );
        let title = match picked.len() {
            1 => format!("Band {}", d.index + 1),
            n => format!("{n} bands"),
        };
        let has_slope = b.shape.slope_range().is_some();
        // At 6 dB an octave (or gentler) a cut or shelf has no width to set.
        let q_idle = matches!(
            b.shape,
            Shape::LowCut | Shape::HighCut | Shape::LowShelf | Shape::HighShelf | Shape::TiltShelf
        ) && b.slope <= 6.0;
        let moved = match self.player.playing() {
            true => self.player.eq_tap().moved(d.index),
            false => 0.0,
        };
        let step = |glyph: Icon, to: Option<u16>, sel: &'static str, cx: &mut Context<Self>| {
            div()
                .size(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .when(to.is_some(), |s| {
                    s.cursor_pointer().hover(|s| s.bg(rgb(RAISED)))
                })
                .debug_selector(move || sel.into())
                .child(icon(glyph, 13.).text_color(rgb(if to.is_some() { TEXT_2 } else { TEXT_4 })))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(to) = to {
                            ws.eq_op(EqOp::Pick(to, false), cx);
                        }
                    }),
                )
        };
        // Along the bottom: a button with an icon, and a label if it has one; lit when on.
        let tool = |glyph: Icon,
                    label: Option<&'static str>,
                    sel: &'static str,
                    lit: Option<(u32, u32)>,
                    cx: &mut Context<Self>,
                    op: EqOp| {
            div()
                .h(px(28.))
                .px(px(if label.is_some() { 10. } else { 7. }))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(7.))
                .cursor_pointer()
                .text_size(px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .whitespace_nowrap()
                .map(|s| match lit {
                    Some((bg, fg)) => s.bg(rgb(bg)).text_color(rgb(fg)),
                    None => s
                        .bg(rgb(FIELD))
                        .text_color(rgb(TEXT_2))
                        .hover(|s| s.bg(rgb(RAISED)).text_color(rgb(TEXT))),
                })
                .debug_selector(move || sel.into())
                .child(icon(glyph, 13.).text_color(rgb(lit.map_or(TEXT_3, |l| l.1))))
                .children(label)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.eq_op(op, cx);
                    }),
                )
        };
        // The shapes, each its curve; the band's lit in its color.
        let shapes = tray().children(Shape::ALL.iter().map(|&s| {
            let on = s == b.shape;
            let sel = format!("eq-shape-{}", kebab(s.name()));
            div()
                .w(px(28.))
                .h(px(22.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .when(on, |d| d.bg(rgb(color)))
                .when(!on, |d| d.hover(|d| d.bg(rgb(OFF))))
                .debug_selector(move || sel.clone())
                .child(icon(shape_icon(s), 18.).text_color(rgb(if on { BG } else { TEXT_3 })))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.eq_op(EqOp::Shape(s), cx);
                    }),
                )
        }));
        let places = tray().children(Placement::ALL.iter().map(|&p| {
            let label = match p {
                Placement::Stereo => "Stereo",
                Placement::Left => "L",
                Placement::Right => "R",
                Placement::Mid => "M",
                Placement::Side => "S",
            };
            let sel = format!("eq-place-{}", p.name().to_lowercase());
            seg(label, p == b.placement, 22.)
                .debug_selector(move || sel.clone())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.eq_op(EqOp::Placement(p), cx);
                    }),
                )
        }));
        let freq = self.dial_column(id, EqKnob::Freq, DIAL, color, cx).child(
            div()
                .text_size(px(10.5))
                .text_color(rgb(TEXT_3))
                .child(format!("Frequency · {}", note_text(b.freq))),
        );
        let width = match b.shape {
            // A bell's, notch's or band pass's width, in octaves where it's half.
            Shape::Bell | Shape::Notch | Shape::BandPass => {
                let q = b.q.max(0.025) as f64;
                let octaves = 2.0 / std::f64::consts::LN_2 * (1.0 / (2.0 * q)).asinh();
                format!("Q · {octaves:.2} oct")
            }
            _ => "Q".to_string(),
        };
        let q = self
            .dial_column(id, EqKnob::Q, DIAL, color, cx)
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .child(width),
            )
            .when(q_idle, |k| k.opacity(0.35));
        let gain = b
            .shape
            .has_gain()
            .then(|| self.hero_gain(id, color, b.gain_db, dynamic.map(|d| d.range_db), moved, cx));
        let solo_button = div()
            .h(px(28.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(7.))
            .cursor_pointer()
            .text_size(px(11.5))
            .font_weight(FontWeight::MEDIUM)
            .when(solo, |s| s.bg(alpha(SOLO, 0.22)).text_color(rgb(SOLO)))
            .when(!solo, |s| {
                s.bg(rgb(FIELD))
                    .text_color(rgb(TEXT_2))
                    .hover(|s| s.bg(rgb(RAISED)).text_color(rgb(TEXT)))
            })
            .debug_selector(|| "eq-solo".into())
            .child(icon(Icon::Headphones, 13.).text_color(rgb(if solo { SOLO } else { TEXT_3 })))
            .child("Solo")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.press_solo(id, e, cx);
                }),
            );
        Some(
            div()
                .absolute()
                .left(px(left))
                .map(|el| match up {
                    true => el.top(px(10.)),
                    false => el.bottom(px(10.)),
                })
                .w(px(w))
                .flex()
                .flex_col()
                .rounded(px(14.))
                .bg(rgb(BG_PANEL))
                .border_1()
                .border_color(alpha(color, 0.45))
                .shadow_lg()
                .debug_selector(|| "eq-panel".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(v) = ws.eq_view.as_mut() {
                            v.focused = true;
                        }
                    }),
                )
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .when_some(notch, |el, x| {
                    el.child(
                        canvas(
                            |_, _, _| {},
                            move |b, _, window, _| {
                                let o = b.origin;
                                // Up, its tip at the top; down, at the bottom.
                                let at = |x: f32, y: f32| {
                                    point(o.x + px(x), o.y + px(if up { 9.0 - y } else { y }))
                                };
                                let mut fill = PathBuilder::fill();
                                fill.move_to(at(0.0, 9.0));
                                fill.line_to(at(8.0, 0.5));
                                fill.line_to(at(16.0, 9.0));
                                fill.close();
                                if let Ok(p) = fill.build() {
                                    window.paint_path(p, rgb(BG_PANEL));
                                }
                                let mut edge = PathBuilder::stroke(px(1.));
                                edge.move_to(at(0.0, 8.0));
                                edge.line_to(at(8.0, 0.5));
                                edge.line_to(at(16.0, 8.0));
                                if let Ok(p) = edge.build() {
                                    window.paint_path(p, alpha(color, 0.45));
                                }
                            },
                        )
                        .absolute()
                        .map(|el| match up {
                            true => el.bottom(px(-8.)),
                            false => el.top(px(-8.)),
                        })
                        .left(px(x - 8.0))
                        .w(px(16.))
                        .h(px(9.)),
                    )
                })
                // The band either side, the shapes and the slope; close.
                .child(
                    div()
                        .pt(px(8.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(step(Icon::Back, before, "eq-prev", cx))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(color)))
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .whitespace_nowrap()
                                        .child(title),
                                ),
                        )
                        .child(step(Icon::Forward, after, "eq-next", cx))
                        .child(div().flex_1())
                        .child(shapes)
                        .when(has_slope, |el| {
                            // Click for its slopes, drag for any between (⇧ finer), the wheel
                            // a step (⇧ a dB), ⌘-click back to 12.
                            el.child(
                                dropdown(slope_text(b.slope), 28.)
                                    .font_family(MONO)
                                    .cursor_ns_resize()
                                    .debug_selector(|| "eq-slope".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                            cx.stop_propagation();
                                            ws.press_knob(id, EqKnob::Slope, e, cx);
                                        }),
                                    )
                                    .on_scroll_wheel(cx.listener(
                                        move |ws, e: &ScrollWheelEvent, _, cx| {
                                            cx.stop_propagation();
                                            ws.wheel_slope(id, e, cx);
                                        },
                                    )),
                            )
                        })
                        .child(div().flex_1())
                        .child(
                            div()
                                .size(px(24.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(RAISED)))
                                .debug_selector(|| "eq-panel-close".into())
                                .child(icon(Icon::Close, 13.).text_color(rgb(TEXT_3)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| {
                                        cx.stop_propagation();
                                        if let Some(v) = ws.eq_view.as_mut() {
                                            v.picked.clear();
                                        }
                                        cx.notify();
                                    }),
                                ),
                        ),
                )
                // The part of the sound and gain-Q, solo; frequency, gain and width; dynamic,
                // off and delete.
                .child(
                    div()
                        .px(px(10.))
                        .pt(px(2.))
                        .pb(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .child(
                                    div().flex().gap(px(6.)).child(places).child(
                                        icon_seg(Icon::Scissors, false, 28.)
                                            .bg(rgb(FIELD))
                                            .debug_selector(|| "eq-split".into())
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|ws, _, _, cx| {
                                                    cx.stop_propagation();
                                                    ws.eq_op(EqOp::Split, cx);
                                                }),
                                            ),
                                    ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(6.))
                                        .when(b.shape.has_gain(), |el| {
                                            el.child(tool(
                                                Icon::Link,
                                                Some("G·Q"),
                                                "eq-gain-q",
                                                b.gain_q.then_some((color, BG)),
                                                cx,
                                                EqOp::GainQ,
                                            ))
                                        })
                                        .child(solo_button),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .justify_center()
                                .items_end()
                                .gap(px(22.))
                                .child(freq)
                                .children(gain)
                                .child(q),
                        )
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .flex_col()
                                .items_end()
                                .gap(px(8.))
                                .child(div().flex().gap(px(6.)).when(
                                    b.shape.can_be_dynamic(),
                                    |el| {
                                        el.child(tool(
                                            Icon::Pulse,
                                            Some("Dynamic"),
                                            "eq-dynamic",
                                            dynamic.is_some().then_some((color, BG)),
                                            cx,
                                            EqOp::Dynamic,
                                        ))
                                        .when(
                                            dynamic.is_some(),
                                            |el| {
                                                el.child(
                                                    icon_seg(Icon::Chevron, open, 28.)
                                                        .when(!open, |s| s.bg(rgb(FIELD)))
                                                        .debug_selector(|| "eq-dyn-expand".into())
                                                        .on_mouse_down(
                                                            MouseButton::Left,
                                                            cx.listener(|ws, _, _, cx| {
                                                                cx.stop_propagation();
                                                                ws.eq_op(EqOp::DynExpand, cx);
                                                            }),
                                                        ),
                                                )
                                            },
                                        )
                                    },
                                ))
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(6.))
                                        .child(tool(
                                            Icon::Power,
                                            None,
                                            "eq-band-off",
                                            (!b.off).then_some((OFF, color)),
                                            cx,
                                            EqOp::Off,
                                        ))
                                        .child(tool(
                                            Icon::Trash,
                                            None,
                                            "eq-delete",
                                            None,
                                            cx,
                                            EqOp::Delete,
                                        )),
                                ),
                        ),
                )
                .when(open, |el| el.child(self.dynamics(d, cx))),
        )
    }
}
