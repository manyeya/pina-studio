//! The pieces a strip is made of: slots, meters, the EQ thumbnail, pills.

use super::*;

/// A slot: an instrument, an effect, a send; with a dot of color before it, if given.
pub(crate) fn slot(label: impl Into<gpui::SharedString>, used: bool, h: f32) -> Div {
    dotted(label, used, h, None)
}

pub(crate) fn dotted(
    label: impl Into<gpui::SharedString>,
    used: bool,
    h: f32,
    dot: Option<u32>,
) -> Div {
    div()
        .flex_none()
        .h(px(h))
        .px(px(6.))
        .flex()
        .items_center()
        .gap(px(5.))
        .rounded(px(5.))
        .text_size(px(10.5))
        .whitespace_nowrap()
        .overflow_hidden()
        .cursor_pointer()
        .when(used, |d| {
            d.bg(rgb(FIELD))
                .text_color(rgb(TEXT_2))
                .hover(|s| s.bg(rgb(RAISED)).text_color(rgb(TEXT)))
        })
        .when(!used, |d| {
            d.border_1()
                .border_dashed()
                .border_color(rgb(RAISED))
                .text_color(rgb(TEXT_4))
                .hover(|s| s.border_color(rgb(LINE_STRONG)).text_color(rgb(TEXT_3)))
        })
        .when_some(dot, |d, c| {
            d.child(div().size(px(6.)).flex_none().rounded(px(3.)).bg(rgb(c)))
        })
        .child(
            div()
                .min_w(px(0.))
                .overflow_hidden()
                .text_ellipsis()
                .child(label.into()),
        )
}

/// A slot that says what feeds a channel but can't be changed: no hover, no pointer.
pub(crate) fn info(label: impl Into<gpui::SharedString>, h: f32) -> Div {
    div()
        .flex_none()
        .h(px(h))
        .px(px(6.))
        .flex()
        .items_center()
        .rounded(px(5.))
        .border_1()
        .border_color(rgb(LINE_SOFT))
        .text_size(px(10.5))
        .text_color(rgb(TEXT_3))
        .whitespace_nowrap()
        .overflow_hidden()
        .child(label.into())
}

/// An empty row, keeping the mixer's strips lined up.
pub(crate) fn blank(h: f32) -> Div {
    div().flex_none().h(px(h))
}

/// A plugin slot: left-click opens the plugin (or picks one, if empty), right-click picks one.
pub(crate) fn plugin_slot(
    label: String,
    used: bool,
    ch: Channel,
    which: Which,
    h: f32,
    dot: Option<u32>,
    cx: &mut Context<Workspace>,
) -> Div {
    dotted(label, used, h, dot)
        .debug_selector(move || format!("slot-{ch:?}-{which:?}"))
        .when(!used, |d| {
            d.justify_center()
                .child(icon(Icon::Plus, 11.).text_color(rgb(TEXT_4)))
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                if used && !(which == Which::Instrument && ws.has_builtin_instrument(ch)) {
                    ws.open_editor(ch, which, cx);
                } else {
                    ws.open_mini(ch, which, e.position, window, cx);
                }
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                ws.menu = Some(ws.plugin_menu(ch, which, e.position));
                cx.notify();
            }),
        )
}

/// A built-in processor's slot, the `row`th effect in its strip: click opens it, drag moves
/// it; right-click to move it or take it out.
#[allow(clippy::too_many_arguments)]
pub(crate) fn builtin_slot(
    label: impl Into<gpui::SharedString>,
    ch: Channel,
    b: Builtin,
    stacked: Option<usize>,
    row: usize,
    h: f32,
    color: u32,
    cx: &mut Context<Workspace>,
) -> Div {
    dotted(label, true, h, Some(color))
        .debug_selector(move || match stacked {
            None => format!("builtin-{ch:?}-{b:?}"),
            Some(k) => format!("builtin-{ch:?}-{b:?}-{k}"),
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                ws.press_effect(ch, row, false, e, cx);
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                let mut items = ws.move_items(ch, row);
                items.push((
                    "Remove".into(),
                    MenuAction::RemoveBuiltin {
                        channel: ch,
                        builtin: b,
                        stacked,
                    },
                ));
                ws.menu = Some(Menu {
                    at: e.position,
                    items,
                    ..Default::default()
                });
                cx.notify();
            }),
        )
}

/// A plugin in a strip's chain, its `n`th insert and `row`th effect: click opens its editor,
/// drag moves it; right-click to move it, take it out or pick a preset.
pub(crate) fn insert_slot(
    label: String,
    ch: Channel,
    n: usize,
    row: usize,
    h: f32,
    cx: &mut Context<Workspace>,
) -> Div {
    let which = Which::Insert(n);
    dotted(label, true, h, Some(TEXT_4))
        .debug_selector(move || format!("slot-{ch:?}-{which:?}"))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                ws.press_effect(ch, row, false, e, cx);
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                let mut menu = ws.plugin_menu(ch, which, e.position);
                menu.items.splice(0..0, ws.move_items(ch, row));
                ws.menu = Some(menu);
                cx.notify();
            }),
        )
}

/// An effect's slot (or card), with where one being dragged would land marked: the one dragged
/// left as a dashed outline, a bright line over (or under) the slot it'd take the place of.
pub(crate) fn dropping(slot: Div, row: usize, drop: Option<(usize, usize)>) -> Div {
    let Some((from, to)) = drop else {
        return slot;
    };
    let line = || {
        div()
            .absolute()
            .left_0()
            .right_0()
            .h(px(3.))
            .rounded(px(2.))
            .bg(rgb(ACCENT))
            .shadow_md()
    };
    slot.relative()
        .when(row == from, |d| {
            d.opacity(0.35)
                .border_1()
                .border_dashed()
                .border_color(rgb(ACCENT))
        })
        .when(row == to && to < from, |d| d.child(line().top_0()))
        .when(row == to && to > from, |d| d.child(line().bottom_0()))
}

/// A channel's EQ curve, small: the strip's thumbnail. Clicking it shows the channel's EQ.
pub fn eq_thumbnail(
    ws: &Workspace,
    ch: Channel,
    h: f32,
    color: u32,
    cx: &mut Context<Workspace>,
) -> Div {
    let eq = ws.strip_dsp(ch).0;
    let rate = ws.session.project.sample_rate as f64;
    let curve = eq.filter(|e| !e.bypass).map(|e| eq_curve(&e, rate, 48));
    div()
        .flex_none()
        .h(px(h))
        .rounded(px(6.))
        .bg(rgb(WELL))
        .overflow_hidden()
        .cursor_pointer()
        .debug_selector(move || format!("eq-{ch:?}"))
        .child(
            canvas(
                |_, _, _| {},
                move |b, _, window, _| {
                    let (w, hh) = (f32::from(b.size.width), f32::from(b.size.height));
                    let mid = hh / 2.0;
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x, b.origin.y + px(mid)),
                            size(b.size.width, px(1.)),
                        ),
                        rgb(LINE_SOFT),
                    ));
                    let Some(curve) = &curve else { return };
                    let scale = (mid - 3.0) / 18.0;
                    let y = |g: f32| {
                        b.origin.y + px((mid - g.clamp(-18.0, 18.0) * scale).clamp(1.0, hh - 1.0))
                    };
                    let x = |i: usize| b.origin.x + px(i as f32 * w / (curve.len() - 1) as f32);
                    let mut area = gpui::PathBuilder::fill();
                    area.move_to(point(b.origin.x, b.origin.y + b.size.height));
                    for (i, &g) in curve.iter().enumerate() {
                        area.line_to(point(x(i), y(g)));
                    }
                    area.line_to(point(b.origin.x + b.size.width, b.origin.y + b.size.height));
                    area.close();
                    if let Ok(p) = area.build() {
                        window.paint_path(p, alpha(color, 0.14));
                    }
                    let mut line = gpui::PathBuilder::stroke(px(1.5));
                    line.move_to(point(x(0), y(curve[0])));
                    for (i, &g) in curve.iter().enumerate().skip(1) {
                        line.line_to(point(x(i), y(g)));
                    }
                    if let Ok(p) = line.build() {
                        window.paint_path(p, rgb(color));
                    }
                },
            )
            .size_full(),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                ws.open_eq(ch, cx);
            }),
        )
}

/// A stereo meter, on the fader's scale: green, then yellow from -6 dB, red over 0 dB; a line
/// where the peak was a moment ago.
pub fn meter(level: [f32; 2], held: [f32; 2], w: f32, gap: f32) -> impl IntoElement {
    let pos = |l: f32| fader_pos(20.0 * l.max(1e-9).log10());
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let h = f32::from(b.size.height);
            let bottom = b.origin.y + b.size.height;
            for side in 0..2 {
                let x = b.origin.x + px(side as f32 * (w + gap));
                let bar = |from: f32, to: f32| {
                    Bounds::new(
                        point(x, bottom - px(to * h)),
                        size(px(w), px((to - from) * h)),
                    )
                };
                window.paint_quad(fill(bar(0.0, 1.0), rgb(0x0a0a0d)).corner_radii(px(2.)));
                let m = pos(level[side]);
                for (from, to, color) in [
                    (0.0, fader_pos(-6.0), METER_LOW),
                    (fader_pos(-6.0), fader_pos(0.0), METER_MID),
                    (fader_pos(0.0), 1.0, METER_HIGH),
                ] {
                    let top = m.min(to);
                    if top > from {
                        window.paint_quad(fill(bar(from, top), rgb(color)));
                    }
                }
                let p = pos(held[side]);
                if p > 0.02 {
                    let color = if p >= fader_pos(0.0) {
                        METER_HIGH
                    } else {
                        TEXT
                    };
                    window.paint_quad(fill(bar(p - 2.0 / h, p), rgb(color)));
                }
            }
        },
    )
    .w(px(w * 2.0 + gap))
    .h_full()
    .flex_none()
}

/// How many effect rows (its built-ins, its plugins and an empty slot) and send rows (its sends
/// and a slot for another) a strip shows in the inspector.
pub fn rows(ws: &Workspace, ch: Channel) -> (usize, usize) {
    // Every effect it shows (its limiter and stacked built-ins too), and the slot to add one.
    let effects = ws.shown_effects(ch).len() + 1;
    let sends = match ch {
        Channel::Track(i) => ws.session.project.tracks[i].sends.len() + 1,
        _ => 0,
    };
    (effects, sends)
}

/// A rounded outline around a little text: where a strip plays.
pub(crate) fn pill(label: String) -> Div {
    div()
        .flex_none()
        .h(px(22.))
        .px(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(11.))
        .border_1()
        .border_color(rgb(RAISED))
        .text_size(px(10.5))
        .text_color(rgb(TEXT_3))
        .whitespace_nowrap()
        .overflow_hidden()
        .child(label)
}
