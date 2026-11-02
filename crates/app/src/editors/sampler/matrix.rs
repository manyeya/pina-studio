//! The modulation matrix: each route a source, what it moves and how far (drag the bar up and
//! down; ⌘-click it back to nothing), and a slot to add another.

use super::*;

/// A chip naming a route's end; click it for the list to pick from.
fn end_chip(label: &'static str, color: u32) -> Div {
    div()
        .h(px(20.))
        .w(px(72.))
        .px(px(7.))
        .flex()
        .items_center()
        .justify_between()
        .rounded(px(5.))
        .bg(rgb(FIELD))
        .cursor_pointer()
        .hover(|d| d.bg(rgb(RAISED)))
        .text_size(px(10.5))
        .text_color(rgb(color))
        .whitespace_nowrap()
        .overflow_hidden()
        .child(label)
        .child(icon(Icon::Chevron, 8.).text_color(rgb(TEXT_4)))
}

impl Workspace {
    pub(crate) fn sampler_matrix(&self, t: usize, color: u32, cx: &mut Context<Self>) -> Div {
        let v = self.sampler_of(t).map(|s| s.voice).unwrap_or_default();
        let slots = *v.matrix.slots();
        let used: Vec<usize> = (0..slots.len())
            .filter(|&i| slots[i].source != ModSource::None)
            .collect();
        let free = (0..slots.len()).find(|&i| slots[i].source == ModSource::None);
        let rows = used.iter().map(|&i| {
            let r = slots[i];
            let amount = r.amount.clamp(-1.0, 1.0);
            let target = Target::Sampler {
                track: t,
                field: Field::Route(i),
            };
            let bar = div()
                .relative()
                .flex_1()
                .h(px(20.))
                .rounded(px(5.))
                .bg(rgb(0x0b0b0e))
                .cursor_ns_resize()
                .overflow_hidden()
                .debug_selector(move || format!("route-{i}-amount"))
                // From the middle, toward the side it moves.
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(gpui::relative(0.5 + amount.min(0.0) / 2.0))
                        .w(gpui::relative(amount.abs() / 2.0))
                        .bg(alpha(color, 0.55)),
                )
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(gpui::relative(0.5))
                        .w(px(1.))
                        .bg(rgb(TEXT_4)),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .font_family(MONO)
                        .text_size(px(10.))
                        .text_color(rgb(TEXT))
                        .child(format!("{:+.0}%", amount * 100.0).replace('-', "−")),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        if e.modifiers.platform {
                            ws.edit_sampler(t, cx, |s| s.voice.matrix.slots_mut()[i].amount = 0.0);
                            return;
                        }
                        let value = ws.target(target);
                        let y = pt(e.position).y;
                        ws.begin_drag(Drag::Knob { target, y, value }, cx);
                    }),
                );
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(
                    end_chip(r.source.name(), TEXT)
                        .debug_selector(move || format!("route-{i}-source"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                let items = ModSource::ALL
                                    .map(|s| {
                                        let op = SamplerOp::Route {
                                            track: t,
                                            slot: i,
                                            source: Some(s),
                                            dest: None,
                                        };
                                        (s.name().to_string(), MenuAction::Sampler(op))
                                    })
                                    .to_vec();
                                ws.menu = Some(Menu {
                                    at: e.position,
                                    items,
                                    ..Default::default()
                                });
                                cx.notify();
                            }),
                        ),
                )
                .child(div().text_size(px(10.)).text_color(rgb(TEXT_4)).child("→"))
                .child(
                    end_chip(r.dest.name(), color)
                        .debug_selector(move || format!("route-{i}-dest"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                let items = ModDest::ALL
                                    .map(|d| {
                                        let op = SamplerOp::Route {
                                            track: t,
                                            slot: i,
                                            source: None,
                                            dest: Some(d),
                                        };
                                        (d.name().to_string(), MenuAction::Sampler(op))
                                    })
                                    .to_vec();
                                ws.menu = Some(Menu {
                                    at: e.position,
                                    items,
                                    ..Default::default()
                                });
                                cx.notify();
                            }),
                        ),
                )
                .child(bar)
                .child(
                    div()
                        .size(px(20.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(RAISED)))
                        .debug_selector(move || format!("route-{i}-remove"))
                        .child(icon(Icon::Close, 9.).text_color(rgb(TEXT_3)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.edit_sampler(t, cx, |s| {
                                    s.voice.matrix.slots_mut()[i] = Route::default();
                                });
                            }),
                        ),
                )
        });
        div()
            .flex()
            .flex_col()
            .gap(px(5.))
            .children(rows)
            .when(used.is_empty(), |d| {
                d.child(
                    div()
                        .py(px(6.))
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_3))
                        .child("Route an LFO, an envelope, velocity, the wheel or a random value to anything."),
                )
            })
            .when_some(free, |d, i| {
                d.child(
                    div()
                        .h(px(22.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.))
                        .border_1()
                        .border_dashed()
                        .border_color(alpha(0xffffff, 0.14))
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .cursor_pointer()
                        .hover(|d| d.bg(alpha(0xffffff, 0.04)).text_color(rgb(TEXT_2)))
                        .debug_selector(|| "route-add".into())
                        .child("+ Route")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.edit_sampler(t, cx, |s| {
                                    s.voice.matrix.slots_mut()[i] = Route {
                                        source: ModSource::Lfo1,
                                        dest: ModDest::Cutoff,
                                        amount: 0.3,
                                    };
                                });
                            }),
                        ),
                )
            })
    }
}
