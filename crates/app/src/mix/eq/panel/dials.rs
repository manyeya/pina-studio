//! The band panel's dials, values and the little pieces they're drawn with.

use super::*;

/// An arc round a center, from one angle to another (degrees, clockwise from the right).
pub(crate) fn arc(
    c: Point<Pixels>,
    r: f32,
    from: f32,
    to: f32,
    width: f32,
) -> Option<gpui::Path<Pixels>> {
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
pub(crate) fn disc(c: Point<Pixels>, r: f32) -> Option<gpui::Path<Pixels>> {
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
pub(crate) fn on(c: Point<Pixels>, r: f32, deg: f32) -> Point<Pixels> {
    let a = deg.to_radians();
    point(c.x + px(r * a.cos()), c.y + px(r * a.sin()))
}

/// A gain's angle on a dial: -30 dB at the bottom left, +30 at the bottom right.
pub(crate) fn gain_angle(db: f32) -> f32 {
    270.0 + db.clamp(-30.0, 30.0) / 30.0 * 135.0
}

/// A dial: a thick arc lit to its value (from the top, if it's either way of a middle), a
/// disc, and a dot where it points.
pub(crate) fn dial(
    value: f32,
    size: f32,
    thick: f32,
    color: u32,
    bipolar: bool,
) -> impl IntoElement {
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
pub(crate) fn number_unit(text: &str) -> (String, String) {
    match text.rsplit_once(' ') {
        Some((n, u)) if u.chars().all(|c| c.is_alphabetic() || c == '/') => {
            (n.to_string(), u.to_string())
        }
        _ => (text.to_string(), String::new()),
    }
}

/// The icon each shape is drawn as.
pub(crate) fn shape_icon(s: Shape) -> Icon {
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
pub(crate) fn kebab(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}

/// A dial's value under it: the number big, its unit small; or, being typed, the field.
pub(crate) fn value_line(
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

impl Workspace {
    /// A dial of the panel, its value under it as a number and its unit (drag it, ⌘-click
    /// resets it, double-click types a value).
    pub(crate) fn dial_column(
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
    pub(crate) fn hero_gain(
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

    /// A value in the dynamics tray, to drag up and down (⌘-click back to its own, double-click
    /// to type): its name, the value, and "auto" while it's its own.
    pub(crate) fn pill_value(
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
    pub(crate) fn small_value(&self, id: u16, k: EqKnob, cx: &mut Context<Self>) -> Div {
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
}
