//! The panel of the bands picked, floating under them at the bottom of the display: the band
//! before and after and the shapes along the top, with the slope; frequency, gain and width as
//! dials, gain the big one in the middle (its dynamic range a ring round it, the move now a dot
//! on that); the part of the sound it works on, gain-Q, solo, dynamic, off and delete along the
//! bottom; and a dynamic band's own settings in a tray under those.

use gpui::{
    Context, Div, FontWeight, MouseButton, MouseDownEvent, PathBuilder, Pixels, Point,
    ScrollWheelEvent, canvas, div, point, prelude::*, px, rgb,
};
use pina_engine::song::project::{Eq, Placement, Shape};

use super::{EqKnob, EqOp, Geo, display::Drawn, hz_text, note_text, q_text, slope_text};
use crate::{
    ui::menu::MenuAction,
    ui::theme::*,
    workspace::{Name, Workspace},
};

const W: f32 = 700.0;
/// The panel's height, closed and with the dynamics tray open: whether it covers its band.
const H: f32 = 178.0;
const H_OPEN: f32 = 272.0;
const BG_PANEL: u32 = 0x17181e;
/// The dials: frequency and width either side, gain in the middle.
const DIAL: f32 = 52.0;
const HERO: f32 = 72.0;

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
    let steps = ((to - from) / 4.0).ceil().max(1.0) as usize;
    for k in 1..=steps {
        p.line_to(at(from + (to - from) * k as f32 / steps as f32));
    }
    p.build().ok()
}

/// A filled circle.
fn disc(c: Point<Pixels>, r: f32) -> Option<gpui::Path<Pixels>> {
    let mut p = PathBuilder::fill();
    p.move_to(point(c.x + px(r), c.y));
    p.arc_to(
        point(px(r), px(r)),
        px(0.),
        false,
        false,
        point(c.x - px(r), c.y),
    );
    p.arc_to(
        point(px(r), px(r)),
        px(0.),
        false,
        false,
        point(c.x + px(r), c.y),
    );
    p.close();
    p.build().ok()
}

/// A point round a center, at an angle.
fn on(c: Point<Pixels>, r: f32, deg: f32) -> Point<Pixels> {
    let a = deg.to_radians();
    point(c.x + px(r * a.cos()), c.y + px(r * a.sin()))
}

/// A gain's angle on a dial: -30 dB at the bottom left, +30 at the bottom right.
fn gain_angle(db: f32) -> f32 {
    270.0 + db.clamp(-30.0, 30.0) / 30.0 * 135.0
}

/// A dial: a thick arc lit to its value (from the top, if it's either way of a middle), a
/// disc, and a dot where it points.
fn dial(value: f32, size: f32, thick: f32, color: u32, bipolar: bool) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let c = b.center();
            let r = size / 2.0 - thick / 2.0;
            if let Some(p) = arc(c, r, 135.0, 405.0, thick) {
                window.paint_path(p, rgb(0x24252c));
            }
            let (from, to) = match bipolar {
                true => (270.0, 270.0 + value.clamp(-1.0, 1.0) * 135.0),
                false => (135.0, 135.0 + value.clamp(0.0, 1.0) * 270.0),
            };
            if let Some(p) = arc(c, r, from, to, thick) {
                window.paint_path(p, rgb(color));
            }
            if bipolar {
                // Where nothing's added or taken: the top.
                let mut tick = PathBuilder::stroke(px(1.5));
                tick.move_to(on(c, size / 2.0 + 1.0, 270.0));
                tick.line_to(on(c, size / 2.0 + 4.0, 270.0));
                if let Ok(p) = tick.build() {
                    window.paint_path(p, rgb(TEXT_3));
                }
            }
            let body = r - thick / 2.0 - 4.0;
            if let Some(p) = disc(c, body) {
                window.paint_path(p, rgb(0x1e1f26));
            }
            if let Some(p) = disc(on(c, body * 0.72, to), (size / 30.0).clamp(2.2, 3.6)) {
                window.paint_path(p, rgb(TEXT));
            }
        },
    )
    .size(px(size))
    .flex_none()
}

/// A value's number and its unit: ("400", "Hz"), ("−3.0", "dB"), ("1.00", "").
fn number_unit(text: &str) -> (String, String) {
    match text.rsplit_once(' ') {
        Some((n, u)) if u.chars().all(|c| c.is_alphabetic() || c == '/') => {
            (n.to_string(), u.to_string())
        }
        _ => (text.to_string(), String::new()),
    }
}

/// The icon each shape is drawn as.
fn shape_icon(s: Shape) -> Icon {
    match s {
        Shape::Bell => Icon::Bell,
        Shape::LowShelf => Icon::LowShelf,
        Shape::LowCut => Icon::LowCut,
        Shape::HighShelf => Icon::HighShelf,
        Shape::HighCut => Icon::HighCut,
        Shape::Notch => Icon::Notch,
        Shape::BandPass => Icon::BandPass,
        Shape::TiltShelf => Icon::TiltShelf,
        Shape::FlatTilt => Icon::FlatTilt,
        Shape::AllPass => Icon::AllPass,
    }
}

/// "eq-shape-low-shelf": a name for picking it.
fn kebab(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
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

    /// A dial of the panel, its value under it as a number and its unit (drag it, ⌘-click
    /// resets it, double-click types a value).
    fn dial_column(
        &self,
        id: u16,
        k: EqKnob,
        size: f32,
        color: u32,
        cx: &mut Context<Self>,
    ) -> Div {
        let v = self.eq_knob_value(id, k);
        let pos = k.pos(v);
        let shown = if k.bipolar() { pos * 2.0 - 1.0 } else { pos };
        let field = self.name_field(Name::Eq(id, k));
        let text = match k {
            EqKnob::Freq => hz_text(v),
            EqKnob::Q => q_text(v),
            _ => k.text(v),
        };
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(3.))
            .child(
                div()
                    .cursor_ns_resize()
                    .debug_selector(move || format!("eq-knob-{}", k.label().to_lowercase()))
                    .child(dial(shown, size, 5.0, color, k.bipolar()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.press_knob(id, k, e, cx);
                        }),
                    ),
            )
            .child(value_line(text, 15.0, field))
    }

    /// Gain, the big dial: its dynamic range a ring round it (drag the ring for the range; a
    /// still band becomes dynamic) and the move it's making now a dot on that.
    fn hero_gain(
        &self,
        id: u16,
        color: u32,
        gain: f32,
        range: Option<f32>,
        moved: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let ring = HERO + 12.0;
        let pos = EqKnob::Gain.pos(gain) * 2.0 - 1.0;
        let field = self.name_field(Name::Eq(id, EqKnob::Gain));
        let label = match range {
            Some(r) => div()
                .text_color(rgb(color))
                .child(format!("Gain · range {}", db_text(r))),
            None => div().text_color(rgb(TEXT_3)).child("Gain"),
        };
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(1.))
            .child(
                div()
                    .relative()
                    .size(px(ring))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_ns_resize()
                    .debug_selector(|| "eq-knob-range".into())
                    .child(
                        canvas(
                            |_, _, _| {},
                            move |b, _, window, _| {
                                let c = b.center();
                                let r = f32::from(b.size.width) / 2.0 - 2.0;
                                if let Some(p) = arc(c, r, 135.0, 405.0, 2.0) {
                                    window.paint_path(p, alpha(0xffffff, 0.05));
                                }
                                if let Some(range) = range {
                                    let (a, z) = (gain_angle(gain), gain_angle(gain + range));
                                    if let Some(p) = arc(c, r, a, z, 3.0) {
                                        window.paint_path(p, rgb(color));
                                    }
                                    if let Some(p) = disc(on(c, r, gain_angle(gain + moved)), 3.2) {
                                        window.paint_path(p, rgb(TEXT));
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
                            .child(dial(pos, HERO, 7.0, color, true))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.press_knob(id, EqKnob::Gain, e, cx);
                                }),
                            ),
                    ),
            )
            .child(value_line(format!("{} dB", db_text(gain)), 20.0, field))
            .child(label.text_size(px(10.5)))
    }

    /// A dynamic band's own settings, in a tray under the rest: its threshold (its own unless
    /// set; the level it's hearing shown), attack and release (their own unless turned), the
    /// whole band or spectral (its density and tilt); what it listens to (what it covers, or
    /// between two frequencies; or the sidechain, from a track picked); bypassed, or cleared.
    fn dynamics(&self, d: &Drawn, cx: &mut Context<Self>) -> Div {
        let id = d.id;
        let color = d.color;
        let dynamic = d.band.dynamic.unwrap_or_default();
        let level = match self.player.playing() {
            true => self.player.eq_tap().level(d.index),
            false => -120.0,
        };
        let auto = dynamic.threshold_db.is_none();
        let threshold = dynamic.threshold_db.unwrap_or(-30.0);
        let share = |db: f32| (db.clamp(-80.0, 0.0) + 80.0) / 80.0;
        let free = dynamic.trigger.is_some();
        let source = self
            .shown_eq()
            .and_then(|(_, eq)| eq.sidechain)
            .and_then(|id| self.session.project.tracks.iter().find(|t| t.id == id))
            .map(|t| t.name.clone());
        let toggle =
            |label: String, sel: &'static str, on: bool, op: EqOp, cx: &mut Context<Self>| {
                div()
                    .h(px(24.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(11.))
                    .whitespace_nowrap()
                    .when(on, |s| s.bg(alpha(color, 0.18)).text_color(rgb(color)))
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
        let caption = |s: &'static str| div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(s);
        // Threshold: what it hears, filling the bar; where it's set, a line (drag it).
        let thresh = div()
            .w(px(150.))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(caption("Threshold"))
                    .child(
                        div()
                            .h(px(16.))
                            .px(px(6.))
                            .flex()
                            .items_center()
                            .rounded(px(4.))
                            .cursor_pointer()
                            .text_size(px(10.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .when(auto, |s| s.bg(alpha(color, 0.18)).text_color(rgb(color)))
                            .when(!auto, |s| {
                                s.bg(rgb(FIELD))
                                    .text_color(rgb(TEXT_3))
                                    .hover(|s| s.text_color(rgb(TEXT_2)))
                            })
                            .debug_selector(|| "eq-auto-threshold".into())
                            .child("Auto")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.eq_op(EqOp::AutoThreshold, cx);
                                }),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(11.))
                            .text_color(rgb(if auto { TEXT_3 } else { TEXT }))
                            .child(if auto {
                                "own".to_string()
                            } else {
                                EqKnob::Threshold.text(threshold)
                            }),
                    ),
            )
            .child(
                div()
                    .relative()
                    .h(px(8.))
                    .rounded(px(4.))
                    .bg(rgb(0x24252c))
                    .cursor_ns_resize()
                    .debug_selector(|| "eq-threshold".into())
                    .child(
                        div()
                            .absolute()
                            .left(px(0.))
                            .top(px(0.))
                            .h(px(8.))
                            .w(gpui::relative(share(level)))
                            .rounded(px(4.))
                            .bg(alpha(color, 0.45)),
                    )
                    .when(!auto, |el| {
                        el.child(
                            div()
                                .absolute()
                                .left(gpui::relative(share(threshold)))
                                .top(px(-3.))
                                .w(px(2.))
                                .h(px(14.))
                                .rounded(px(1.))
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
            );
        div()
            .mx(px(10.))
            .mb(px(10.))
            .p(px(10.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .rounded(px(10.))
            .bg(rgb(BG))
            .border_1()
            .border_color(rgb(LINE))
            .debug_selector(|| "eq-dynamics".into())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(thresh)
                    .child(self.pill_value(
                        id,
                        EqKnob::Attack,
                        "Attack",
                        dynamic.attack_ms.is_none(),
                        color,
                        cx,
                    ))
                    .child(self.pill_value(
                        id,
                        EqKnob::Release,
                        "Release",
                        dynamic.release_ms.is_none(),
                        color,
                        cx,
                    ))
                    .child(div().flex_1())
                    // The whole band moving, or only what stands out in it.
                    .child(
                        tray()
                            .child(
                                seg("Whole", !dynamic.spectral, 22.)
                                    .debug_selector(|| "eq-whole-band".into())
                                    .when(dynamic.spectral, |s| {
                                        s.on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|ws, _, _, cx| {
                                                cx.stop_propagation();
                                                ws.eq_op(EqOp::Spectral, cx);
                                            }),
                                        )
                                    }),
                            )
                            .child(
                                seg("Spectral", dynamic.spectral, 22.)
                                    .when(dynamic.spectral, |s| {
                                        s.bg(rgb(color)).text_color(rgb(BG))
                                    })
                                    .debug_selector(|| "eq-spectral".into())
                                    .when(!dynamic.spectral, |s| {
                                        s.on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|ws, _, _, cx| {
                                                cx.stop_propagation();
                                                ws.eq_op(EqOp::Spectral, cx);
                                            }),
                                        )
                                    }),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .when(dynamic.spectral, |el| {
                        el.child(self.pill_value(id, EqKnob::Density, "Density", false, color, cx))
                            .child(toggle(
                                "Tilt".into(),
                                "eq-spectral-tilt",
                                dynamic.tilt,
                                EqOp::SpectralTilt,
                                cx,
                            ))
                            .child(div().w(px(1.)).h(px(16.)).mx(px(4.)).bg(rgb(LINE_STRONG)))
                    })
                    .child(caption("Listens to"))
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
                    ))
                    .when(free, |el| {
                        el.child(self.small_value(id, EqKnob::Low, cx))
                            .child(self.small_value(id, EqKnob::High, cx))
                    })
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
                            .size(px(24.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .cursor_pointer()
                            .bg(rgb(FIELD))
                            .hover(|s| s.bg(rgb(RAISED)))
                            .debug_selector(|| "eq-sidechain".into())
                            .child(icon(Icon::Chevron, 11.).text_color(rgb(TEXT_3)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.eq_sidechain_menu(e.position, cx);
                                }),
                            ),
                    )
                    .child(div().flex_1())
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
            )
    }

    /// A value in the dynamics tray, to drag up and down (⌘-click back to its own, double-click
    /// to type): its name, the value, and "auto" while it's its own.
    fn pill_value(
        &self,
        id: u16,
        k: EqKnob,
        name: &'static str,
        own: bool,
        color: u32,
        cx: &mut Context<Self>,
    ) -> Div {
        let v = self.eq_knob_value(id, k);
        let field = self.name_field(Name::Eq(id, k));
        div()
            .h(px(26.))
            .px(px(9.))
            .flex()
            .items_center()
            .gap(px(7.))
            .rounded(px(7.))
            .bg(rgb(FIELD))
            .cursor_ns_resize()
            .hover(|s| s.bg(rgb(RAISED)))
            .debug_selector(move || format!("eq-knob-{}", k.label().to_lowercase()))
            .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(name))
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT))
                    .whitespace_nowrap()
                    .map(|el| match field {
                        Some(input) => el.w(px(60.)).child(input),
                        None => el.child(k.text(v)),
                    }),
            )
            .when(own, |el| {
                el.child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(color))
                        .child("auto"),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.press_knob(id, k, e, cx);
                }),
            )
    }

    /// A small value to drag, ⌘-click back, or double-click and type: a trigger's frequency.
    fn small_value(&self, id: u16, k: EqKnob, cx: &mut Context<Self>) -> Div {
        let v = self.eq_knob_value(id, k);
        let field = self.name_field(Name::Eq(id, k));
        div()
            .h(px(24.))
            .px(px(7.))
            .flex()
            .items_center()
            .gap(px(5.))
            .rounded(px(6.))
            .bg(rgb(FIELD))
            .cursor_ns_resize()
            .debug_selector(move || format!("eq-value-{}", k.label().to_lowercase()))
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(TEXT_3))
                    .child(k.label()),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(11.))
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
            crate::mix::strip::Channel::Track(i) => {
                self.session.project.tracks.get(i).map(|t| t.id)
            }
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
}

/// A dial's value under it: the number big, its unit small; or, being typed, the field.
fn value_line(
    text: String,
    size: f32,
    field: Option<gpui::Entity<crate::ui::text::TextInput>>,
) -> Div {
    let (number, unit) = number_unit(&text);
    div()
        .h(px(size + 6.0))
        .flex()
        .items_end()
        .gap(px(3.))
        .whitespace_nowrap()
        .map(|el| match field {
            Some(input) => el
                .w(px(84.))
                .px(px(6.))
                .rounded(px(6.))
                .bg(rgb(FIELD))
                .font_family(MONO)
                .text_size(px(13.))
                .child(input),
            None => el
                .child(
                    div()
                        .text_size(px(size))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT))
                        .child(number),
                )
                .child(
                    div()
                        .pb(px(size * 0.12))
                        .text_size(px((size * 0.55).max(11.0)))
                        .text_color(rgb(TEXT_3))
                        .child(unit),
                ),
        })
}
