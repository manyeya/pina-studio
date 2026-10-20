//! The panel of the bands picked, floating under them at the bottom of the display, as
//! Pro-Q's: off and delete at its corners, the band before and after, the shape and slope, big
//! knobs for frequency, gain (its dynamic range a ring round it, the move now a bar in it) and
//! width, the part of the sound they work on and a split, solo to hold, and dynamic.

use gpui::{
    Context, Div, FontWeight, MouseButton, MouseDownEvent, PathBuilder, Pixels, Point, canvas, div,
    point, prelude::*, px, rgb,
};
use tempo_engine::project::{Eq, Placement, Shape};

use super::{EqKnob, EqOp, Geo, display::Drawn, slope_text};
use crate::{
    menu::MenuAction,
    theme::*,
    workspace::{Name, Workspace},
};

const W: f32 = 540.0;
const H: f32 = 132.0;
const KNOB: f32 = 46.0;

/// An arc round a center, from one angle to another (degrees, clockwise from the right).
fn arc(c: Point<Pixels>, r: f32, from: f32, to: f32, width: f32) -> Option<gpui::Path<Pixels>> {
    let at = |deg: f32| {
        let a = deg.to_radians();
        point(c.x + px(r * a.cos()), c.y + px(r * a.sin()))
    };
    let (from, to) = (from.min(to), from.max(to));
    if to - from < 0.5 {
        return None;
    }
    let mut p = PathBuilder::stroke(px(width));
    p.move_to(at(from));
    let steps = ((to - from) / 6.0).ceil().max(1.0) as usize;
    for k in 1..=steps {
        p.line_to(at(from + (to - from) * k as f32 / steps as f32));
    }
    p.build().ok()
}

/// A gain's angle on a knob: -30 dB at the bottom left, +30 at the bottom right.
fn gain_angle(db: f32) -> f32 {
    270.0 + db.clamp(-30.0, 30.0) / 30.0 * 135.0
}

impl Workspace {
    pub(super) fn band_panel(
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
        let open = view.dyn_open && b.dynamic.is_some() && b.shape.can_be_dynamic();
        let w = (if open { W + 330.0 } else { W }).min(g.w - 16.0);
        let center = picked.iter().map(|p| p.dot.0).sum::<f32>() / picked.len() as f32;
        let left = (center - w / 2.0).clamp(8.0, g.w - w - 8.0);
        let top = g.h - H - 10.0;
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
        let has_slope = !b.shape.slopes().is_empty();
        let dynamic = b.dynamic.filter(|_| b.shape.has_gain());
        let moved = match self.player.playing() {
            true => self.player.eq_tap().moved(d.index),
            false => 0.0,
        };
        let small = |glyph: Icon,
                     sel: &'static str,
                     on: bool,
                     color: u32,
                     cx: &mut Context<Self>,
                     op: EqOp| {
            div()
                .size(px(22.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .cursor_pointer()
                .when(on, |s| s.bg(alpha(color, 0.22)))
                .when(!on, |s| s.hover(|s| s.bg(rgb(RAISED))))
                .debug_selector(move || sel.into())
                .child(icon(glyph, 12.).text_color(rgb(if on { color } else { TEXT_3 })))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.eq_op(op, cx);
                    }),
                )
        };
        let step =
            |glyph: &'static str, to: Option<u16>, sel: &'static str, cx: &mut Context<Self>| {
                div()
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .text_size(px(11.))
                    .text_color(rgb(if to.is_some() { TEXT_2 } else { TEXT_4 }))
                    .when(to.is_some(), |s| {
                        s.cursor_pointer().hover(|s| s.bg(rgb(RAISED)))
                    })
                    .debug_selector(move || sel.into())
                    .child(glyph)
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
        let choose = |label: String, sel: &'static str, cx: &mut Context<Self>| {
            dropdown(label, 24.)
                .debug_selector(move || sel.into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let Some((_, b)) = ws.eq_band(id) else { return };
                        let tick = |on: bool| if on { "✓ " } else { "" };
                        let items: Vec<(String, MenuAction)> = match sel {
                            "eq-shape" => Shape::ALL
                                .iter()
                                .map(|&s| {
                                    (
                                        format!("{}{}", tick(s == b.shape), s.name()),
                                        MenuAction::Eq(EqOp::Shape(s)),
                                    )
                                })
                                .collect(),
                            "eq-slope" => b
                                .shape
                                .slopes()
                                .iter()
                                .map(|&s| {
                                    (
                                        format!("{}{}", tick(s == b.slope), slope_text(s)),
                                        MenuAction::Eq(EqOp::Slope(s)),
                                    )
                                })
                                .collect(),
                            _ => Placement::ALL
                                .iter()
                                .map(|&p| {
                                    (
                                        format!("{}{}", tick(p == b.placement), p.name()),
                                        MenuAction::Eq(EqOp::Placement(p)),
                                    )
                                })
                                .collect(),
                        };
                        ws.eq_menu(e.position, items, cx);
                    }),
                )
        };
        let knob = |k: EqKnob, cx: &mut Context<Self>| self.panel_knob(id, k, d.color, cx);
        let gain = self.gain_knob(
            id,
            d.color,
            b.gain_db,
            dynamic.map(|d| d.range_db),
            moved,
            cx,
        );
        Some(
            div()
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(w))
                .h(px(H))
                .flex()
                .flex_col()
                .rounded(px(14.))
                .bg(alpha(0x15161c, 0.95))
                .border_1()
                .border_color(alpha(d.color, 0.5))
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
                // Off and delete at its corners, the bands either side between.
                .child(
                    div()
                        .h(px(30.))
                        .px(px(6.))
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .child(small(
                            Icon::Mute,
                            "eq-band-off",
                            b.off,
                            RECORD,
                            cx,
                            EqOp::Off,
                        ))
                        .child(div().flex_1())
                        .child(step("‹", before, "eq-prev", cx))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(d.color)))
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(TEXT_2))
                                        .child(title),
                                ),
                        )
                        .child(step("›", after, "eq-next", cx))
                        .child(div().flex_1())
                        .child(small(
                            Icon::Close,
                            "eq-delete",
                            false,
                            RECORD,
                            cx,
                            EqOp::Delete,
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .px(px(12.))
                        .pb(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(14.))
                        .child(
                            div()
                                .w(px(112.))
                                .flex()
                                .flex_col()
                                .gap(px(6.))
                                .child(choose(b.shape.name().into(), "eq-shape", cx))
                                .when(has_slope, |el| {
                                    el.child(choose(slope_text(b.slope), "eq-slope", cx))
                                }),
                        )
                        .child(knob(EqKnob::Freq, cx))
                        .when(b.shape.has_gain(), |el| el.child(gain))
                        // Gain-Q interaction, between the two it ties.
                        .when(b.shape.has_gain(), |el| {
                            el.child(
                                div()
                                    .h(px(20.))
                                    .px(px(5.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .text_size(px(9.5))
                                    .font_weight(FontWeight::BOLD)
                                    .when(b.gain_q, |s| {
                                        s.bg(alpha(d.color, 0.25)).text_color(rgb(d.color))
                                    })
                                    .when(!b.gain_q, |s| {
                                        s.bg(rgb(FIELD))
                                            .text_color(rgb(TEXT_4))
                                            .hover(|s| s.text_color(rgb(TEXT_2)))
                                    })
                                    .debug_selector(|| "eq-gain-q".into())
                                    .child("G·Q")
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, _, _, cx| {
                                            cx.stop_propagation();
                                            ws.eq_op(EqOp::GainQ, cx);
                                        }),
                                    ),
                            )
                        })
                        .child(knob(EqKnob::Q, cx))
                        .when(open, |el| el.child(self.dynamics(d, cx)))
                        .child(div().flex_1())
                        .child(
                            div()
                                .w(px(112.))
                                .flex()
                                .flex_col()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(4.))
                                        .child(choose(b.placement.name().into(), "eq-place", cx))
                                        .child(small(
                                            Icon::Scissors,
                                            "eq-split",
                                            false,
                                            TEXT,
                                            cx,
                                            EqOp::Split,
                                        )),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(4.))
                                        .child(
                                            div()
                                                .h(px(24.))
                                                .px(px(8.))
                                                .flex()
                                                .items_center()
                                                .gap(px(5.))
                                                .rounded(px(6.))
                                                .cursor_pointer()
                                                .text_size(px(11.))
                                                .when(solo, |s| {
                                                    s.bg(alpha(SOLO, 0.22)).text_color(rgb(SOLO))
                                                })
                                                .when(!solo, |s| {
                                                    s.bg(rgb(FIELD))
                                                        .text_color(rgb(TEXT_3))
                                                        .hover(|s| s.bg(rgb(RAISED)))
                                                })
                                                .debug_selector(|| "eq-solo".into())
                                                .child(icon(Icon::Headphones, 12.).text_color(rgb(
                                                    if solo { SOLO } else { TEXT_3 },
                                                )))
                                                .child("Solo")
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(
                                                        move |ws, e: &MouseDownEvent, _, cx| {
                                                            cx.stop_propagation();
                                                            ws.press_solo(id, e, cx);
                                                        },
                                                    ),
                                                ),
                                        )
                                        .when(b.shape.has_gain(), |el| {
                                            let on = dynamic.is_some();
                                            el.child(
                                                div()
                                                    .h(px(24.))
                                                    .px(px(7.))
                                                    .flex()
                                                    .items_center()
                                                    .rounded(px(6.))
                                                    .cursor_pointer()
                                                    .text_size(px(11.))
                                                    .when(on, |s| {
                                                        s.bg(alpha(AGENT, 0.18))
                                                            .text_color(rgb(AGENT_TEXT))
                                                    })
                                                    .when(!on, |s| {
                                                        s.bg(rgb(FIELD))
                                                            .text_color(rgb(TEXT_3))
                                                            .hover(|s| s.bg(rgb(RAISED)))
                                                    })
                                                    .debug_selector(|| "eq-dynamic".into())
                                                    .child("Dyn")
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|ws, _, _, cx| {
                                                            cx.stop_propagation();
                                                            ws.eq_op(EqOp::Dynamic, cx);
                                                        }),
                                                    ),
                                            )
                                            .when(
                                                on,
                                                |el| {
                                                    el.child(
                                                        div()
                                                            .h(px(24.))
                                                            .px(px(6.))
                                                            .flex()
                                                            .items_center()
                                                            .rounded(px(6.))
                                                            .cursor_pointer()
                                                            .text_size(px(11.))
                                                            .bg(rgb(FIELD))
                                                            .text_color(rgb(TEXT_3))
                                                            .hover(|s| s.bg(rgb(RAISED)))
                                                            .debug_selector(|| {
                                                                "eq-dyn-expand".into()
                                                            })
                                                            .child(if open { "‹" } else { "›" })
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
                                        }),
                                ),
                        ),
                ),
        )
    }

    /// A dynamic band's own settings, opened beside its knobs: its threshold (its own at the
    /// top, the level it's hearing shown), attack and release (its own unless turned), spectral
    /// (its density and tilt), what it listens to (what it covers, or between two frequencies; or the sidechain, from a track
    /// picked), its dynamics bypassed, or cleared.
    fn dynamics(&self, d: &Drawn, cx: &mut Context<Self>) -> Div {
        let id = d.id;
        let dynamic = d.band.dynamic.unwrap_or_default();
        let level = match self.player.playing() {
            true => self.player.eq_tap().level(d.index),
            false => -120.0,
        };
        let auto = dynamic.threshold_db.is_none();
        let threshold = dynamic.threshold_db.unwrap_or(-30.0);
        let (bar_h, top) = (64.0f32, 0.0f32);
        let y = |db: f32| top + (1.0 - (db.clamp(-80.0, 0.0) + 80.0) / 80.0) * bar_h;
        let free = dynamic.trigger.is_some();
        let source = self
            .shown_eq()
            .and_then(|(_, eq)| eq.sidechain)
            .and_then(|id| self.session.project.tracks.iter().find(|t| t.id == id))
            .map(|t| t.name.clone());
        let knob = |k: EqKnob, cx: &mut Context<Self>| self.panel_knob(id, k, d.color, cx);
        let toggle =
            |label: String, sel: &'static str, on: bool, op: EqOp, cx: &mut Context<Self>| {
                div()
                    .h(px(22.))
                    .px(px(7.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .cursor_pointer()
                    .text_size(px(10.5))
                    .whitespace_nowrap()
                    .when(on, |s| s.bg(alpha(d.color, 0.22)).text_color(rgb(d.color)))
                    .when(!on, |s| {
                        s.bg(rgb(FIELD))
                            .text_color(rgb(TEXT_3))
                            .hover(|s| s.bg(rgb(RAISED)).text_color(rgb(TEXT_2)))
                    })
                    .debug_selector(move || sel.into())
                    .child(label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.eq_op(op, cx);
                        }),
                    )
            };
        let thresh = div()
            .w(px(56.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(3.))
            .child(
                div()
                    .text_size(px(9.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT_4))
                    .child("THRESH"),
            )
            .child(
                div()
                    .relative()
                    .w(px(14.))
                    .h(px(bar_h))
                    .rounded(px(3.))
                    .bg(rgb(0x0c0d10))
                    .cursor_ns_resize()
                    .debug_selector(|| "eq-threshold".into())
                    // What it's hearing, rising from the bottom.
                    .child(
                        div()
                            .absolute()
                            .left(px(0.))
                            .bottom(px(0.))
                            .w(px(14.))
                            .h(px(bar_h - y(level)))
                            .rounded(px(3.))
                            .bg(alpha(d.color, 0.45)),
                    )
                    .when(!auto, |el| {
                        el.child(
                            div()
                                .absolute()
                                .left(px(-3.))
                                .top(px(y(threshold) - 1.5))
                                .w(px(20.))
                                .h(px(3.))
                                .rounded(px(1.5))
                                .bg(rgb(TEXT)),
                        )
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.press_knob(id, EqKnob::Threshold, e, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .h(px(16.))
                    .px(px(4.))
                    .rounded(px(3.))
                    .cursor_pointer()
                    .font_family(MONO)
                    .text_size(px(10.))
                    .when(auto, |s| {
                        s.bg(alpha(d.color, 0.22)).text_color(rgb(d.color))
                    })
                    .when(!auto, |s| s.text_color(rgb(TEXT_2)))
                    .debug_selector(|| "eq-auto-threshold".into())
                    .child(if auto {
                        "Auto".to_string()
                    } else {
                        EqKnob::Threshold.text(threshold)
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.eq_op(EqOp::AutoThreshold, cx);
                        }),
                    ),
            );
        let timing = |k: EqKnob, auto: bool, cx: &mut Context<Self>| {
            knob(k, cx).when(auto, |el| {
                el.child(
                    div()
                        .text_size(px(9.))
                        .text_color(rgb(d.color))
                        .child("auto"),
                )
            })
        };
        div()
            .flex()
            .items_center()
            .gap(px(10.))
            .pl(px(12.))
            .border_l_1()
            .border_color(rgb(LINE_STRONG))
            .debug_selector(|| "eq-dynamics".into())
            .child(thresh)
            .child(timing(EqKnob::Attack, dynamic.attack_ms.is_none(), cx))
            .child(timing(EqKnob::Release, dynamic.release_ms.is_none(), cx))
            // Spectral: only what stands out moves, frequency by frequency; how many move
            // together, and whether it hears the highs as a mix's tilt has them.
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(5.))
                    .child(toggle(
                        "Spectral".into(),
                        "eq-spectral",
                        dynamic.spectral,
                        EqOp::Spectral,
                        cx,
                    ))
                    .when(dynamic.spectral, |el| {
                        el.child(toggle(
                            "Tilt".into(),
                            "eq-spectral-tilt",
                            dynamic.tilt,
                            EqOp::SpectralTilt,
                            cx,
                        ))
                    }),
            )
            .when(dynamic.spectral, |el| el.child(knob(EqKnob::Density, cx)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(5.))
                    .child(
                        div()
                            .flex()
                            .gap(px(3.))
                            .child(toggle(
                                "Band".into(),
                                "eq-trigger-band",
                                !free,
                                EqOp::Trigger,
                                cx,
                            ))
                            .child(toggle(
                                "Free".into(),
                                "eq-trigger-free",
                                free,
                                EqOp::Trigger,
                                cx,
                            )),
                    )
                    .when(free, |el| {
                        el.child(
                            div()
                                .flex()
                                .gap(px(2.))
                                .child(self.small_value(id, EqKnob::Low, cx))
                                .child(self.small_value(id, EqKnob::High, cx)),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap(px(3.))
                            .child(
                                toggle(
                                    match &source {
                                        Some(name) => format!("Ext · {name}"),
                                        None => "Ext".into(),
                                    },
                                    "eq-external",
                                    dynamic.external,
                                    EqOp::External,
                                    cx,
                                )
                                .on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                        cx.stop_propagation();
                                        ws.eq_sidechain_menu(e.position, cx);
                                    }),
                                ),
                            )
                            .child(
                                div()
                                    .h(px(22.))
                                    .px(px(5.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .bg(rgb(FIELD))
                                    .hover(|s| s.bg(rgb(RAISED)))
                                    .debug_selector(|| "eq-sidechain".into())
                                    .child(icon(Icon::Chevron, 10.).text_color(rgb(TEXT_3)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                            cx.stop_propagation();
                                            ws.eq_sidechain_menu(e.position, cx);
                                        }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(3.))
                            .child(toggle(
                                "Bypass".into(),
                                "eq-dyn-bypass",
                                dynamic.bypass,
                                EqOp::DynBypass,
                                cx,
                            ))
                            .child(toggle(
                                "Clear".into(),
                                "eq-dyn-clear",
                                false,
                                EqOp::DynClear,
                                cx,
                            )),
                    ),
            )
    }

    /// A small value to drag, ⌘-click back, or double-click and type: a trigger's frequency.
    fn small_value(&self, id: u16, k: EqKnob, cx: &mut Context<Self>) -> Div {
        let v = self.eq_knob_value(id, k);
        let field = self.name_field(Name::Eq(id, k));
        div()
            .h(px(20.))
            .px(px(5.))
            .flex()
            .items_center()
            .gap(px(4.))
            .rounded(px(4.))
            .bg(rgb(FIELD))
            .cursor_ns_resize()
            .debug_selector(move || format!("eq-value-{}", k.label().to_lowercase()))
            .child(
                div()
                    .text_size(px(8.5))
                    .text_color(rgb(TEXT_4))
                    .child(k.label()),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(10.))
                    .text_color(rgb(TEXT_2))
                    .map(|el| match field {
                        Some(input) => el.w(px(54.)).child(input),
                        None => el.child(k.text(v)),
                    }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.press_knob(id, k, e, cx);
                }),
            )
    }

    /// The tracks the EQ's sidechain can be: what its dynamic bands listen to with Ext on.
    fn eq_sidechain_menu(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let Some((ch, eq)) = self.shown_eq() else {
            return;
        };
        let own = match ch {
            crate::strip::Channel::Track(i) => self.session.project.tracks.get(i).map(|t| t.id),
            _ => None,
        };
        let tick = |on: bool| if on { "✓ " } else { "" };
        let mut items = vec![("Sidechain from".to_string(), MenuAction::Nothing)];
        items.push((
            format!("{}None", tick(eq.sidechain.is_none())),
            MenuAction::Eq(EqOp::Sidechain(None)),
        ));
        items.extend(
            self.session
                .project
                .tracks
                .iter()
                .filter(|t| Some(t.id) != own)
                .map(|t| {
                    (
                        format!("{}{}", tick(eq.sidechain == Some(t.id)), t.name),
                        MenuAction::Eq(EqOp::Sidechain(Some(t.id))),
                    )
                }),
        );
        self.eq_menu(at, items, cx);
    }

    /// A knob of the panel: its name, the knob (drag it, ⌘-click resets it, double-click
    /// types a value) and its value.
    fn panel_knob(&self, id: u16, k: EqKnob, color: u32, cx: &mut Context<Self>) -> Div {
        let v = self.eq_knob_value(id, k);
        let pos = k.pos(v);
        let shown = if k.bipolar() { pos * 2.0 - 1.0 } else { pos };
        let field = self.name_field(Name::Eq(id, k));
        div()
            .w(px(64.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(3.))
            .child(
                div()
                    .text_size(px(9.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT_4))
                    .child(k.label()),
            )
            .child(
                div()
                    .cursor_ns_resize()
                    .debug_selector(move || format!("eq-knob-{}", k.label().to_lowercase()))
                    .child(knob(shown, KNOB, color, k.bipolar()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.press_knob(id, k, e, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .h(px(16.))
                    .font_family(MONO)
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_2))
                    .whitespace_nowrap()
                    .map(|el| match field {
                        Some(input) => el.w(px(64.)).bg(rgb(FIELD)).child(input),
                        None => el.child(k.text(v)),
                    }),
            )
    }

    /// The gain knob, its dynamic range a ring round it (drag the ring for the range) and the
    /// move it's making now a bar in that.
    fn gain_knob(
        &self,
        id: u16,
        color: u32,
        gain: f32,
        range: Option<f32>,
        moved: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let ring = KNOB + 16.0;
        let pos = EqKnob::Gain.pos(gain) * 2.0 - 1.0;
        let field = self.name_field(Name::Eq(id, EqKnob::Gain));
        let text = match range {
            Some(r) => format!("{} dB  ↕{}", db_text(gain), db_text(r)),
            None => format!("{} dB", db_text(gain)),
        };
        div()
            .w(px(ring + 8.0))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(0.))
            .child(
                div()
                    .text_size(px(9.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT_4))
                    .child("GAIN"),
            )
            .child(
                div()
                    .relative()
                    .size(px(ring))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_ns_resize()
                    .debug_selector(|| "eq-knob-range".into())
                    // The ring: grab it for the dynamic range (a still band becomes dynamic).
                    .child(
                        canvas(
                            |_, _, _| {},
                            move |b, _, window, _| {
                                let c = b.center();
                                let r = f32::from(b.size.width) / 2.0 - 3.0;
                                if let Some(p) = arc(c, r, 135.0, 405.0, 3.0) {
                                    window.paint_path(p, alpha(0xffffff, 0.06));
                                }
                                if let Some(range) = range {
                                    let (a, z) = (gain_angle(gain), gain_angle(gain + range));
                                    if let Some(p) = arc(c, r, a, z, 3.0) {
                                        window.paint_path(p, rgb(RECORD));
                                    }
                                    let m = gain_angle(gain + moved);
                                    if let Some(p) = arc(c, r - 3.5, gain_angle(gain), m, 2.5) {
                                        window.paint_path(p, rgb(SOLO));
                                    }
                                }
                            },
                        )
                        .absolute()
                        .size_full(),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.press_knob(id, EqKnob::Range, e, cx);
                        }),
                    )
                    .child(
                        div()
                            .debug_selector(|| "eq-knob-gain".into())
                            .child(knob(pos, KNOB, color, true))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.press_knob(id, EqKnob::Gain, e, cx);
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .h(px(16.))
                    .font_family(MONO)
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_2))
                    .whitespace_nowrap()
                    .map(|el| match field {
                        Some(input) => el.w(px(70.)).bg(rgb(FIELD)).child(input),
                        None => el.child(text),
                    }),
            )
    }
}
