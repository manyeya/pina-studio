//! Each lane's header: what it shows, its value, its mode and buttons.

use super::*;

impl Workspace {
    /// A track's lane headers, under its header: what each shows (pick another), its value now,
    /// the track's mode, and lanes to add or take away.
    pub fn auto_headers(&self, i: usize, cx: &mut Context<Self>) -> Vec<Div> {
        let t = &self.session.project.tracks[i];
        let (id, mode) = (t.id, t.automation.mode);
        let color = self.color(i);
        let h = self.auto_h();
        let lanes = self.auto_lanes(i);
        let n = lanes.len();
        let nested = self.stack_color(i);
        let mut out = Vec::new();
        for (k, &param) in lanes.iter().enumerate() {
            let top = self.lane_top(i, k);
            let has = !t.automation.points(param).is_empty();
            let value = self.now_value(i, param);
            // What modulators move to move it, and those that do.
            let target = self.mod_target_of(i, param);
            let moving: Vec<usize> = t
                .modulators
                .iter()
                .enumerate()
                .filter(|(_, m)| m.routes.iter().any(|r| Some(r.target) == target))
                .map(|(k, _)| k)
                .collect();
            let offering = target.is_some()
                && matches!(self.drag, Some(Drag::Modulator { track, .. }) if track == i);
            let lit = has && mode != AutoMode::Off;
            let (label, plugin) = match param {
                Param::Plugin(Which::Instrument, _) => (self.param_name(i, param), None),
                Param::Plugin(which, _) => {
                    (self.param_name(i, param), Some(self.plugin_name(i, which)))
                }
                _ => (self.param_name(i, param), None),
            };
            // The scale, at the lane's right edge: its top, middle and bottom.
            let marks: Vec<(f32, &str)> = match param {
                Param::Volume | Param::Send(_) => {
                    vec![(6.0, "+6"), (0.0, "0"), (-12.0, "−12"), (-30.0, "−30")]
                }
                Param::Pan => vec![(-1.0, "L"), (0.0, "C"), (1.0, "R")],
                Param::Mute => vec![(0.0, "play"), (1.0, "mute")],
                Param::Plugin(..) => vec![(1.0, "100"), (0.5, "50"), (0.0, "0")],
                Param::Eq(_, EqField::Freq) => {
                    vec![(10_000.0, "10k"), (1_000.0, "1k"), (100.0, "100")]
                }
                Param::Eq(_, EqField::Gain) => vec![(12.0, "+12"), (0.0, "0"), (-12.0, "−12")],
                Param::Eq(_, EqField::Q) => vec![(10.0, "10"), (1.0, "1"), (0.1, "0.1")],
                Param::Mod(_, s) if s.range().1 > 1.0 => {
                    vec![(5_000.0, "5s"), (100.0, "100ms"), (2.0, "2ms")]
                }
                Param::Mod(..) => vec![(1.0, "100"), (0.5, "50"), (0.0, "0")],
            };
            let scale = marks.into_iter().map(|(v, s)| {
                div()
                    .absolute()
                    .right(px(4.))
                    .top(px((to_y(param, v, h) - 5.0).clamp(0.0, h - 11.0)))
                    .text_size(px(8.5))
                    .font_family(MONO)
                    .text_color(rgb(TEXT_3))
                    .child(s)
            });
            let readout = div()
                .h(px(18.))
                .min_w(px(52.))
                .px(px(5.))
                .flex_none()
                .flex()
                .items_center()
                .justify_end()
                .rounded(px(4.))
                .bg(rgb(0x0b0b0e))
                .border_1()
                .border_color(rgb(0x22222a))
                .font_family(MONO)
                .text_size(px(10.))
                .text_color(rgb(if lit { mode_color(mode) } else { TEXT_2 }))
                .whitespace_nowrap()
                .debug_selector(move || format!("auto-value-{i}-{k}"))
                .child(self.value_text(i, param, value));
            let icon_button = |i_: Icon, name: String| {
                div()
                    .size(px(18.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .text_color(rgb(TEXT_3))
                    .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                    .cursor_pointer()
                    .debug_selector(move || name.clone())
                    .child(icon(i_, 11.).text_color(rgb(TEXT_2)))
            };
            let top_row = div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    dropdown(label, 20.)
                        .min_w(px(0.))
                        .max_w(px(150.))
                        .flex_shrink_1()
                        .overflow_hidden()
                        .when(k == 0, |d| {
                            d.debug_selector(move || format!("auto-param-{i}"))
                        })
                        .when(k > 0, |d| {
                            d.debug_selector(move || format!("auto-param-{i}-{k}"))
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.menu = Some(Menu::search(e.position, ws.auto_params(i, k)));
                                cx.notify();
                            }),
                        ),
                )
                .children(plugin.map(|name| {
                    div()
                        .min_w(px(0.))
                        .flex_shrink_1()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_size(px(10.))
                        .text_color(rgb(TEXT_3))
                        .text_ellipsis()
                        .child(name)
                }))
                .child(div().flex_1())
                // The modulators moving it, a dot each in its colour.
                .children(moving.iter().map(|&k| {
                    div()
                        .flex_none()
                        .size(px(7.))
                        .rounded(px(4.))
                        .bg(rgb(crate::editors::modulation::MOD_COLORS[k % 8]))
                }))
                .child(readout);
            let bottom_row = div()
                .flex()
                .items_center()
                .gap(px(4.))
                .when(k == 0, |d| {
                    d.child(
                        chip(mode.name(), mode_color(mode))
                            .cursor_pointer()
                            .debug_selector(move || format!("auto-mode-{i}"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.mode_menu(i, e.position, cx);
                                }),
                            ),
                    )
                })
                .when(k > 0, |d| {
                    d.child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_3))
                            .child(format!("{} points", t.automation.points(param).len())),
                    )
                })
                .child(div().flex_1())
                .when(k == n - 1, |d| {
                    d.child(
                        icon_button(Icon::Plus, format!("auto-add-{i}")).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.auto_op(id, AutoOp::AddLane, cx);
                            }),
                        ),
                    )
                })
                .when(n > 1, |d| {
                    d.child(
                        icon_button(Icon::Close, format!("auto-remove-{i}-{k}")).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.auto_op(id, AutoOp::RemoveLane(k), cx);
                            }),
                        ),
                    )
                });
            let targets = self.mod_targets.clone();
            out.push(
                div()
                    .absolute()
                    .top(px(top))
                    .left_0()
                    .w_full()
                    .h(px(h))
                    .bg(rgb(0x141419))
                    .border_b_1()
                    .border_color(rgb(LINE_SOFT))
                    .debug_selector(move || format!("auto-lane-{i}-{k}"))
                    // A modulator's ↗ dropped here moves its parameter; right-click to pick one.
                    .when_some(target, |d, mt| {
                        d.child(
                            canvas(
                                move |b, _, _| targets.borrow_mut().push((i, mt, b)),
                                move |b, _, window, _| {
                                    if offering {
                                        window.paint_quad(
                                            fill(b, alpha(ACCENT, 0.1))
                                                .border_widths(px(1.))
                                                .border_color(alpha(ACCENT, 0.7)),
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
                            MouseButton::Right,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                let title = ws.param_name(i, param);
                                let items = ws.lane_mod_menu(i, mt, title);
                                ws.menu = Some(Menu {
                                    at: e.position,
                                    items,
                                    ..Default::default()
                                });
                                cx.notify();
                            }),
                        )
                    })
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .bottom_0()
                            .w(px(3.))
                            .bg(alpha(color, if lit { 0.9 } else { 0.35 })),
                    )
                    .children(scale)
                    // Its stack's line, on down from its track's row.
                    .when_some(nested, |d, c| {
                        d.child(
                            div()
                                .absolute()
                                .left(px(12.))
                                .top(px(-1.))
                                .bottom(px(-1.))
                                .w(px(2.))
                                .bg(alpha(c, 0.55)),
                        )
                    })
                    .child(
                        div()
                            .absolute()
                            .left(px(if nested.is_some() { 22. } else { 12. }))
                            .right(px(30.))
                            .top(px(6.))
                            .bottom(px(5.))
                            .flex()
                            .flex_col()
                            .justify_between()
                            .child(top_row)
                            .when(h >= 44.0, |d| d.child(bottom_row)),
                    ),
            );
        }
        out
    }
}
