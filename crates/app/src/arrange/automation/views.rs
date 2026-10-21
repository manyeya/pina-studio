//! The lanes' curves, points and selection, drawn.

use super::*;

impl Workspace {
    /// Every track's lanes, in the lanes area: a press area under each, the line painted over
    /// it (straight, bent or stepped, the area under it shaded, the playhead's value lit), and
    /// its points and bend handles to drag.
    pub fn auto_views(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (ppt, sx) = (self.px_per_tick(), self.scroll_x);
        let h = self.auto_h();
        let playhead = self.playhead();
        let mut out = Vec::new();
        for (t, track) in self.session.project.tracks.iter().enumerate() {
            if self.folded(t) {
                continue;
            }
            let color = self.color(t);
            let off = track.automation.mode == AutoMode::Off;
            for (k, param) in self.auto_lanes(t).into_iter().enumerate() {
                let top = self.lane_top(t, k);
                let points: Vec<AutoPoint> = track.automation.points(param).to_vec();
                let level = self.own_value(t, param);
                let picked = self.picked_points(t, param);
                let live = self.taken.contains_key(&(track.id, param));
                let marquee = match self.drag {
                    Some(Drag::AutoMarquee {
                        track: mt,
                        k: mk,
                        from,
                        to,
                        erase,
                        ..
                    }) if mt == t && mk == k => Some((from, to, erase)),
                    _ => None,
                };
                let group = format!("auto-{t}-{k}");
                // The press area.
                out.push(
                    div()
                        .id(gpui::ElementId::Name(format!("auto-lane-{t}-{k}").into()))
                        .group(group.clone())
                        .absolute()
                        .top(px(top))
                        .left_0()
                        .w_full()
                        .h(px(h))
                        .cursor(match self.tool {
                            Tool::Pencil => gpui::CursorStyle::Crosshair,
                            Tool::Eraser => gpui::CursorStyle::OperationNotAllowed,
                            _ => gpui::CursorStyle::Arrow,
                        })
                        .when(k == 0, |d| {
                            d.debug_selector(move || format!("auto-lane-{t}"))
                        })
                        .when(k > 0, |d| {
                            d.debug_selector(move || format!("auto-lane-{t}-{k}"))
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.lane_press(t, k, e, cx);
                            }),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.select_track(t);
                                ws.auto_lane_menu(t, k, e.position, cx);
                            }),
                        )
                        .into_any_element(),
                );
                // The line.
                let shown = points.clone();
                let lit_at = value_at(&points, playhead as f64)
                    .filter(|_| !off && !live)
                    .map(|v| (playhead, v));
                let picked_ticks: Vec<usize> = picked.clone();
                let lanes_left = f32::from(self.lanes_bounds.get().origin.x);
                let _ = lanes_left;
                out.push(
                    canvas(
                        |_, _, _| {},
                        move |b, _, window, _| {
                            let y = |v: f32| b.origin.y + px(top + to_y(param, v, h));
                            let x = |tick: u64| b.origin.x + px(tick as f32 * ppt - sx);
                            let right = b.origin.x + b.size.width;
                            let bottom = b.origin.y + px(top + h - 1.0);
                            // Guides: the middle (pan), 0 dB, or half.
                            let guide = match param {
                                Param::Volume | Param::Send(_) => Some(0.0),
                                Param::Pan => Some(0.0),
                                Param::Plugin(..) => Some(0.5),
                                Param::Eq(_, EqField::Gain) => Some(0.0),
                                Param::Mute | Param::Eq(..) | Param::Mod(..) => None,
                            };
                            if let Some(g) = guide {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(b.origin.x, y(g)),
                                        size(b.size.width, px(1.)),
                                    ),
                                    alpha(0xffffff, 0.045),
                                ));
                            }
                            let faint = if off || live { 0.35 } else { 1.0 };
                            // The line as a list of window points, bends sampled.
                            let mut line: Vec<Point<Pixels>> = Vec::new();
                            match (shown.first(), shown.last()) {
                                (Some(f), Some(_)) => {
                                    line.push(point(b.origin.x.min(x(f.tick)), y(f.value)));
                                    for (n, p) in shown.iter().enumerate() {
                                        line.push(point(x(p.tick), y(p.value)));
                                        let Some(q) = shown.get(n + 1) else { continue };
                                        if p.step {
                                            line.push(point(x(q.tick), y(p.value)));
                                        } else if p.curve != 0.0 || p.ease {
                                            let w = f32::from(x(q.tick) - x(p.tick));
                                            let steps = (w / 3.0).clamp(6.0, 64.0) as usize;
                                            for s in 1..steps {
                                                let f = s as f32 / steps as f32;
                                                let v = p.value + (q.value - p.value) * p.along(f);
                                                let tx =
                                                    p.tick as f32 + (q.tick - p.tick) as f32 * f;
                                                line.push(point(
                                                    b.origin.x + px(tx * ppt - sx),
                                                    y(v),
                                                ));
                                            }
                                        }
                                    }
                                    let l = shown.last().unwrap();
                                    line.push(point(right.max(x(l.tick)), y(l.value)));
                                }
                                _ => {
                                    line.push(point(b.origin.x, y(level)));
                                    line.push(point(right, y(level)));
                                }
                            }
                            // The area under it, darkening down.
                            let mut area = PathBuilder::fill();
                            area.move_to(point(line[0].x, bottom));
                            for p in &line {
                                area.line_to(*p);
                            }
                            area.line_to(point(line[line.len() - 1].x, bottom));
                            area.close();
                            let strength = if shown.is_empty() { 0.04 } else { 0.2 } * faint;
                            if let Ok(p) = area.build() {
                                window.paint_path(
                                    p,
                                    linear_gradient(
                                        180.,
                                        linear_color_stop(alpha(color, strength), 0.),
                                        linear_color_stop(alpha(color, strength * 0.15), 1.),
                                    ),
                                );
                            }
                            let mut path =
                                PathBuilder::stroke(px(if shown.is_empty() { 1. } else { 1.6 }));
                            path.move_to(line[0]);
                            for p in &line[1..] {
                                path.line_to(*p);
                            }
                            if let Ok(p) = path.build() {
                                let ink = if shown.is_empty() { 0.3 } else { faint };
                                window.paint_path(p, alpha(color, ink));
                            }
                            // Where the playhead is on it.
                            if let Some((tick, v)) = lit_at {
                                let c = point(x(tick), y(v));
                                window.paint_quad(
                                    fill(
                                        Bounds::new(
                                            c - point(px(5.), px(5.)),
                                            size(px(10.), px(10.)),
                                        ),
                                        alpha(color, 0.25),
                                    )
                                    .corner_radii(px(5.)),
                                );
                                window.paint_quad(
                                    fill(
                                        Bounds::new(
                                            c - point(px(2.5), px(2.5)),
                                            size(px(5.), px(5.)),
                                        ),
                                        rgb(0xffffff),
                                    )
                                    .corner_radii(px(2.5)),
                                );
                            }
                            // A marquee being dragged.
                            if let Some((from, to, erase)) = marquee {
                                let (x0, x1) = (from.0.min(to.0), from.0.max(to.0));
                                let (y0, y1) = (from.1.min(to.1), from.1.max(to.1));
                                let tint = if erase { RECORD } else { ACCENT };
                                let r = Bounds::new(
                                    point(px(x0), px(y0)),
                                    size(px(x1 - x0), px(y1 - y0)),
                                );
                                window.paint_quad(
                                    fill(r, alpha(tint, 0.12))
                                        .border_widths(px(1.))
                                        .border_color(alpha(tint, 0.7)),
                                );
                            }
                            let _ = &picked_ticks;
                        },
                    )
                    .absolute()
                    .size_full()
                    .into_any_element(),
                );
                // Bend handles, halfway along each line wide enough, while the lane is hovered.
                for (n, w) in points.windows(2).enumerate() {
                    let (a, b) = (w[0], w[1]);
                    let (xa, xb) = (a.tick as f32 * ppt - sx, b.tick as f32 * ppt - sx);
                    if a.step || xb - xa < 28.0 || (a.value - b.value).abs() < 1e-6 || param.steps()
                    {
                        continue;
                    }
                    let mid = a.value + (b.value - a.value) * a.along(0.5);
                    let (cx_, cy) = ((xa + xb) / 2.0, top + to_y(param, mid, h));
                    let bent = a.curve != 0.0 || a.ease;
                    let curve = a.curve;
                    out.push(
                        div()
                            .absolute()
                            .left(px(cx_ - 5.))
                            .top(px(cy - 5.))
                            .size(px(10.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .opacity(if bent { 0.8 } else { 0.0 })
                            .group_hover(group.clone(), |s| s.opacity(1.0))
                            .cursor_ns_resize()
                            .debug_selector(move || format!("auto-bend-{t}-{k}-{n}"))
                            .child(
                                div()
                                    .size(px(6.))
                                    .rounded(px(1.))
                                    .bg(rgb(0x0b0b0e))
                                    .border_1()
                                    .border_color(alpha(color, 0.9)),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.select_track(t);
                                    if e.click_count == 2 {
                                        ws.edit(cx, |ws| {
                                            let pts = ws.session.project.tracks[t]
                                                .automation
                                                .points_mut(param);
                                            if let Some(q) = pts.get_mut(n) {
                                                q.curve = 0.0;
                                            }
                                        });
                                        return;
                                    }
                                    ws.begin_drag(
                                        Drag::AutoBend {
                                            track: t,
                                            param,
                                            index: n,
                                            y: f32::from(e.position.y),
                                            curve,
                                        },
                                        cx,
                                    );
                                }),
                            )
                            .into_any_element(),
                    );
                }
                // The points.
                for (n, q) in points.iter().enumerate() {
                    let px_ = q.tick as f32 * ppt - sx;
                    if px_ < -10.0 || px_ > 6000.0 {
                        continue;
                    }
                    let on = picked.contains(&n);
                    let py = top + to_y(param, q.value, h);
                    out.push(
                        div()
                            .absolute()
                            .left(px(px_ - 6.))
                            .top(px(py - 6.))
                            .size(px(12.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_move()
                            .debug_selector(move || format!("auto-point-{t}-{k}-{n}"))
                            .child(
                                div()
                                    .size(px(if on { 9. } else { 8. }))
                                    .rounded(px(5.))
                                    .bg(rgb(if on { 0xffffff } else { 0x0b0b0e }))
                                    .border_2()
                                    .border_color(rgb(color))
                                    .when(on, |d| {
                                        d.shadow(vec![gpui::BoxShadow {
                                            color: alpha(color, 0.7),
                                            offset: point(px(0.), px(0.)),
                                            blur_radius: px(6.),
                                            spread_radius: px(0.),
                                            inset: false,
                                        }])
                                    }),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.point_press(t, k, n, e, cx);
                                }),
                            )
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.select_track(t);
                                    ws.point_menu(t, k, n, e.position, cx);
                                }),
                            )
                            .into_any_element(),
                    );
                }
            }
        }
        // What a point being dragged is at, beside it.
        if let Some((t, param, index)) = match self.drag {
            Some(Drag::Auto {
                track,
                param,
                index,
                moved: true,
                ..
            }) => Some((track, param, index)),
            Some(Drag::AutoLine {
                track,
                param,
                moved: true,
                ..
            }) => self
                .picked_points(track, param)
                .first()
                .map(|&i| (track, param, i)),
            Some(Drag::AutoDraw { track, param, last }) => self.session.project.tracks[track]
                .automation
                .points(param)
                .iter()
                .position(|q| q.tick == last.0)
                .map(|i| (track, param, i)),
            _ => None,
        } && let Some(q) = self.session.project.tracks[t]
            .automation
            .points(param)
            .get(index)
        {
            let k = self
                .auto_lanes(t)
                .iter()
                .position(|&p| p == param)
                .unwrap_or(0);
            let x = q.tick as f32 * ppt - sx;
            let y = self.lane_top(t, k) + to_y(param, q.value, h);
            let [bar, beat, div_, _] = self.session.project.bbt(q.tick);
            let when = format!("{}.{}.{}", bar + 1, beat + 1, div_ + 1);
            out.push(
                div()
                    .absolute()
                    .left(px(x + 10.))
                    .top(px((y - 24.).max(self.lane_top(t, k))))
                    .px(px(6.))
                    .py(px(2.))
                    .rounded(px(4.))
                    .bg(rgb(0x0b0b0e))
                    .border_1()
                    .border_color(rgb(LINE_STRONG))
                    .font_family(MONO)
                    .text_size(px(10.))
                    .text_color(rgb(TEXT))
                    .whitespace_nowrap()
                    .child(format!("{}  ·  {when}", self.value_text(t, param, q.value)))
                    .into_any_element(),
            );
        }
        out
    }
}
