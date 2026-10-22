//! The EQ's plot: the spectrum, every band's curve and the sum, drawn.

use super::*;

impl Workspace {
    pub(crate) fn eq_plot(
        &self,
        eq: &Eq,
        geo: &Geo,
        freqs: &[f64],
        drawn: &[Drawn],
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let view = self.eq_view.as_ref().expect("the window's open");
        let rate = self.session.project.sample_rate as f64;
        let g = *geo;
        let picked = view.picked.clone();
        let hover = view.hover;
        let dragging = matches!(
            self.drag,
            Some(Drag::EqNode { .. } | Drag::EqPress { .. } | Drag::EqKnob { .. })
        );
        let analysis = view.analysis;
        let columns: Vec<f32> = view.columns.iter().map(|&f| g.x(f)).collect();
        let (pre, post) = (view.pre.clone(), view.post.clone());
        let xs: Vec<f32> = freqs.iter().map(|&f| g.x(f)).collect();
        // The overall curve (the bands on all of the sound) and one for each part of it bands
        // work on alone: what that part gets.
        let part = |also: Option<Placement>| -> Vec<f32> {
            (0..freqs.len())
                .map(|c| {
                    drawn
                        .iter()
                        .filter(|d| {
                            !d.band.off
                                && (d.band.placement == Placement::Stereo
                                    || Some(d.band.placement) == also)
                        })
                        .map(|d| d.curve[c])
                        .sum::<f32>()
                        + eq.output_db
                })
                .collect()
        };
        let total = part(None);
        let parts: Vec<(Vec<f32>, u32)> = [
            Placement::Left,
            Placement::Right,
            Placement::Mid,
            Placement::Side,
        ]
        .into_iter()
        .filter(|p| drawn.iter().any(|d| !d.band.off && d.band.placement == *p))
        .map(|p| (part(Some(p)), part_color(p)))
        .collect();
        // Each band's curve, its dynamic reach, color, fill and whether it's outlined.
        type Area = (Vec<f32>, Option<Vec<f32>>, u32, f32, bool);
        let bands: Vec<Area> = drawn
            .iter()
            .map(|d| {
                let strong = picked.contains(&d.id) || hover == Some(d.id);
                // What takes a lot away (a cut, a notch) fills lightly, or it would cover all.
                let light = if d.band.shape.has_gain() { 1.0 } else { 0.4 };
                let a = match (d.band.off, strong) {
                    (true, _) => 0.03,
                    (false, true) => 0.28 * light,
                    (false, false) => 0.13 * light,
                };
                (
                    d.curve.clone(),
                    d.reach.clone(),
                    d.color,
                    a,
                    strong && !d.band.off,
                )
            })
            .collect();
        // Over the empty plot: the band a click would make there.
        let preview: Option<(Vec<f32>, (f32, f32), bool)> = view
            .pointer
            .filter(|_| hover.is_none() && !dragging && eq.bands.len() < MAX_BANDS)
            .filter(|&(x, y)| x >= 0.0 && x <= g.w && y >= 0.0 && y <= g.h)
            .map(|(x, y)| {
                let b = band_at(&g, x, y, false);
                let s = band_sections(&b, 1.0, rate);
                let curve = freqs
                    .iter()
                    .map(|&f| response_db(&s, f, rate) as f32)
                    .collect();
                (
                    curve,
                    (
                        g.x(b.freq as f64),
                        g.y(dot_db(&b, 1.0).clamp(-g.range, g.range)),
                    ),
                    !picked.is_empty(),
                )
            });
        let marquee = view.marquee;
        let bypass = eq.bypass;
        let floor = -(analysis.range as f32);
        // The track compared with, and where the two crowd each other.
        let ext = view
            .ext
            .map(|_| view.ext_spec.clone())
            .filter(|e| e.len() == columns.len());
        let collide = analysis.collisions;
        // Spectrum Grab: the spectrum held, the rest dimmed, its peaks named.
        let held = view.grabbing.clone().filter(|h| h.len() == columns.len());
        let dim = if held.is_some() { 0.3 } else { 1.0 };
        let grab_peaks: Vec<usize> = held
            .as_ref()
            .map(|h| super::super::tools::peaks(h, floor))
            .unwrap_or_default();
        // EQ Match: the difference it's fitting, and what the bands it fitted would do.
        let matched: Option<(Vec<f32>, Vec<f32>, Vec<f32>)> = view
            .matching
            .as_ref()
            .filter(|m| !m.proposed.is_empty())
            .map(|m| {
                let xs: Vec<f32> = m.freqs().iter().map(|&f| g.x(f)).collect();
                let fitted: Vec<f32> = m
                    .freqs()
                    .iter()
                    .map(|&f| {
                        m.proposed
                            .iter()
                            .map(|b| pina_engine::dsp::eq::band_response(b, 1.0, f, rate) as f32)
                            .sum()
                    })
                    .collect();
                (xs, m.difference.clone(), fitted)
            });
        let stroke = view.stroke.clone();
        let ay = move |db: f32| (-db / analysis.range as f32 * g.h).clamp(0.0, g.h);
        let steps = match g.range as u8 {
            3 => 1.0,
            6 => 2.0,
            12 => 3.0,
            _ => 6.0,
        };
        let grid_db: Vec<f32> = (1..=(g.range / steps) as i32)
            .flat_map(|k| [k as f32 * steps, -(k as f32) * steps])
            .filter(|d| d.abs() < g.range)
            .collect();
        let grid_f = marks(&g, 26.0);
        let grid_lines = grid_db.clone();
        let picture = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let o = b.origin;
                let at = |x: f32, y: f32| point(o.x + px(x), o.y + px(y));
                window.paint_quad(fill(
                    b,
                    linear_gradient(
                        180.,
                        linear_color_stop(rgb(0x0c0d11), 0.),
                        linear_color_stop(rgb(0x121319), 1.),
                    ),
                ));
                for &(f, major) in &grid_f {
                    window.paint_quad(fill(
                        Bounds::new(at(g.x(f), 0.0), size(px(1.), b.size.height)),
                        alpha(0xffffff, if major { 0.07 } else { 0.03 }),
                    ));
                }
                for &d in &grid_lines {
                    window.paint_quad(fill(
                        Bounds::new(at(0.0, g.y(d)), size(b.size.width, px(1.))),
                        alpha(0xffffff, 0.035),
                    ));
                }
                window.paint_quad(fill(
                    Bounds::new(at(0.0, g.y(0.0)), size(b.size.width, px(1.))),
                    alpha(CURVE, 0.16),
                ));
                // The analyzer: what goes in, dark; what comes out, lighter, with its line.
                let spectrum =
                    |s: &[f32], window: &mut gpui::Window, top: u32, a: f32, edge: Option<f32>| {
                        let n = columns.len().min(s.len());
                        if n < 2 {
                            return;
                        }
                        let mut area = PathBuilder::fill();
                        area.move_to(at(columns[0], g.h));
                        for i in 0..n {
                            area.line_to(at(columns[i], ay(s[i])));
                        }
                        area.line_to(at(columns[n - 1], g.h));
                        area.close();
                        if let Ok(p) = area.build() {
                            window.paint_path(
                                p,
                                linear_gradient(
                                    180.,
                                    linear_color_stop(alpha(top, a), 0.),
                                    linear_color_stop(alpha(top, a * 0.2), 1.),
                                ),
                            );
                        }
                        if let Some(edge) = edge {
                            let points: Vec<_> = (0..n).map(|i| at(columns[i], ay(s[i]))).collect();
                            if let Some(p) = line(&points, 1.0, false) {
                                window.paint_path(p, alpha(0xdfe8ff, edge));
                            }
                        }
                    };
                if analysis.pre && pre.iter().any(|&d| d > floor + 0.5) {
                    spectrum(&pre, window, 0x46507a, 0.45, None);
                }
                if analysis.post && post.iter().any(|&d| d > floor + 0.5) {
                    spectrum(&post, window, 0x7f9cf5, 0.30 * dim, Some(0.5 * dim));
                }
                if let Some(ext) = &ext {
                    // Where both are loud and close, a red glow up to the quieter.
                    if collide {
                        for i in 0..columns.len().min(post.len()) {
                            let (a, e) = (post[i], ext[i]);
                            let loud = ((a.min(e) - (floor + 36.0)) / 18.0).clamp(0.0, 1.0);
                            let close = (1.0 - (a - e).abs() / 9.0).clamp(0.0, 1.0);
                            let c = loud * close;
                            if c > 0.05 {
                                let top = ay(a.min(e));
                                window.paint_quad(fill(
                                    Bounds::new(
                                        at(columns[i] - 1.5, top),
                                        size(px(3.), px(g.h - top)),
                                    ),
                                    linear_gradient(
                                        180.,
                                        linear_color_stop(alpha(RECORD, 0.7 * c), 0.),
                                        linear_color_stop(alpha(RECORD, 0.08 * c), 1.),
                                    ),
                                ));
                            }
                        }
                    }
                    let points: Vec<_> = (0..columns.len())
                        .map(|i| at(columns[i], ay(ext[i])))
                        .collect();
                    if let Some(p) = line(&points, 1.2, false) {
                        window.paint_path(p, alpha(0xff8a8a, 0.75));
                    }
                }
                if let Some(held) = &held {
                    let points: Vec<_> = (0..columns.len())
                        .map(|i| at(columns[i], ay(held[i])))
                        .collect();
                    if let Some(p) = line(&points, 1.6, false) {
                        window.paint_path(p, alpha(0xffffff, 0.95));
                    }
                }
                let n = xs.len();
                if n < 2 {
                    return;
                }
                let zero = g.y(0.0);
                let clamp_y = |d: f32| g.y(d).clamp(-2.0, g.h + 2.0);
                let points =
                    |c: &[f32]| -> Vec<_> { (0..n).map(|i| at(xs[i], clamp_y(c[i]))).collect() };
                // Each band's area, from 0 dB to its curve; a dynamic one's reach, lighter.
                for (curve, reach, color, a, strong) in &bands {
                    if let Some(reach) = reach {
                        let mut area = PathBuilder::fill();
                        area.move_to(at(xs[0], clamp_y(curve[0])));
                        for i in 0..n {
                            area.line_to(at(xs[i], clamp_y(reach[i])));
                        }
                        for i in (0..n).rev() {
                            area.line_to(at(xs[i], clamp_y(curve[i])));
                        }
                        area.close();
                        if let Ok(p) = area.build() {
                            window.paint_path(p, alpha(*color, a * 0.8 * dim));
                        }
                    }
                    let mut area = PathBuilder::fill();
                    area.move_to(at(xs[0], zero));
                    for i in 0..n {
                        area.line_to(at(xs[i], clamp_y(curve[i])));
                    }
                    area.line_to(at(xs[n - 1], zero));
                    area.close();
                    if let Ok(p) = area.build() {
                        window.paint_path(p, alpha(*color, *a * dim));
                    }
                    if *strong && let Some(p) = line(&points(curve), 1.3, false) {
                        window.paint_path(p, alpha(*color, 0.9));
                    }
                }
                // The parts' curves, then the overall curve over all with a glow.
                for (curve, color) in &parts {
                    if let Some(p) = line(&points(curve), 1.6, false) {
                        window.paint_path(p, alpha(*color, if bypass { 0.3 } else { 0.85 }));
                    }
                }
                let main = if bypass { 0x6a6a74 } else { CURVE };
                for (width, a) in [(6.0, 0.10), (2.2, 1.0)] {
                    if let Some(p) = line(&points(&total), width, false) {
                        window.paint_path(p, alpha(main, a * dim));
                    }
                }
                if let Some((xs, difference, fitted)) = &matched {
                    let curve = |c: &[f32]| -> Vec<_> {
                        xs.iter().zip(c).map(|(&x, &d)| at(x, clamp_y(d))).collect()
                    };
                    if let Some(p) = line(&curve(difference), 3.0, false) {
                        window.paint_path(p, alpha(0xffffff, 0.8));
                    }
                    if let Some(p) = line(&curve(fitted), 2.0, true) {
                        window.paint_path(p, alpha(CURVE, 0.95));
                    }
                }
                if stroke.len() > 1 {
                    let points: Vec<_> = stroke.iter().map(|&(x, y)| at(x, y)).collect();
                    if let Some(p) = line(&points, 2.4, false) {
                        window.paint_path(p, alpha(0xffffff, 0.9));
                    }
                }
                if let Some((curve, _, dashed)) = &preview
                    && let Some(p) = line(&points(curve), 1.2, *dashed)
                {
                    window.paint_path(p, alpha(0xffffff, 0.35));
                }
                if let Some((a, c)) = marquee {
                    let (x0, y0) = (a.0.min(c.0), a.1.min(c.1));
                    let (w, h) = ((a.0 - c.0).abs(), (a.1 - c.1).abs());
                    let r = Bounds::new(at(x0, y0), size(px(w), px(h)));
                    window.paint_quad(fill(r, alpha(0xffffff, 0.05)));
                    window.paint_quad(gpui::outline(
                        r,
                        alpha(0xffffff, 0.35),
                        gpui::BorderStyle::Solid,
                    ));
                }
            },
        )
        .absolute()
        .size_full();

        // The EQ's scale down the plot's right edge, and its range to choose.
        let labels: Vec<Div> = grid_db
            .iter()
            .chain(&[0.0])
            .filter(|d| (g.y(**d) - 16.0).abs() > 10.0)
            .map(|&d| {
                div()
                    .absolute()
                    .right(px(6.))
                    .top(px(g.y(d) - 7.0))
                    .font_family(MONO)
                    .text_size(px(9.5))
                    .text_color(alpha(CURVE, if d == 0.0 { 0.75 } else { 0.45 }))
                    .child(if d > 0.0 {
                        format!("+{d}")
                    } else {
                        db_text(d).trim_end_matches(".0").to_string()
                    })
            })
            .collect();
        let range = div()
            .absolute()
            .right(px(4.))
            .top(px(4.))
            .h(px(18.))
            .px(px(6.))
            .flex()
            .items_center()
            .gap(px(3.))
            .rounded(px(4.))
            .cursor_pointer()
            .font_family(MONO)
            .text_size(px(10.))
            .text_color(alpha(CURVE, 0.85))
            .hover(|d| d.bg(alpha(CURVE, 0.12)))
            .debug_selector(|| "eq-range".into())
            .child(format!("{} dB", g.range))
            .child(icon(Icon::Chevron, 9.).text_color(alpha(CURVE, 0.7)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let now = ws.eq_view.as_ref().map_or(12, |v| v.range);
                    let items = RANGES
                        .iter()
                        .map(|&r| {
                            (
                                format!("{}± {r} dB", if r == now { "✓ " } else { "" }),
                                MenuAction::Eq(EqOp::Range(r)),
                            )
                        })
                        .collect();
                    ws.eq_menu(e.position, items, cx);
                }),
            );

        let dots: Vec<Div> = drawn
            .iter()
            .map(|d| self.eq_dot(d, &picked, hover, cx))
            .collect();
        // Where a dynamic band's range reaches: a handle to drag.
        let reach: Vec<Div> = drawn
            .iter()
            .filter(|d| !d.band.off && d.band.shape.has_gain())
            .filter_map(|d| {
                let dynamic = d.band.dynamic?;
                let id = d.id;
                let to =
                    g.y(((d.band.gain_db + dynamic.range_db) * eq.scale).clamp(-g.range, g.range));
                let (top, bottom) = (to.min(d.dot.1), to.max(d.dot.1));
                Some(
                    div()
                        .absolute()
                        .left(px(d.dot.0 - 1.0))
                        .top(px(top))
                        .w(px(2.))
                        .h(px((bottom - top).max(1.0)))
                        .bg(alpha(d.color, 0.5))
                        .child(
                            div()
                                .absolute()
                                .left(px(-6.0))
                                .top(px(if to < d.dot.1 {
                                    -3.0
                                } else {
                                    bottom - top - 3.0
                                }))
                                .w(px(14.))
                                .h(px(6.))
                                .rounded(px(3.))
                                .bg(rgb(d.color))
                                .cursor_ns_resize()
                                .debug_selector(move || format!("eq-reach-{id}"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                        cx.stop_propagation();
                                        ws.press_knob(id, EqKnob::Range, e, cx);
                                    }),
                                ),
                        ),
                )
            })
            .collect();
        // The values of the band under the pointer, or the one picked.
        let told = match (hover, picked.as_slice()) {
            (Some(h), _) => Some(h),
            (None, [one]) => Some(*one),
            _ => None,
        };
        let readout = told
            .and_then(|id| drawn.iter().find(|d| d.id == id))
            .map(|d| self.eq_readout(d, &g, cx));
        let panel = self.band_panel(eq, &g, drawn, cx);
        let peak_labels: Vec<Div> = grab_peaks
            .iter()
            .map(|&i| {
                let level = view.grabbing.as_ref().map_or(0.0, |h| h[i]);
                let x = g.x(view.columns[i]);
                div()
                    .absolute()
                    .left(px(x - 30.0))
                    .top(px(((-level / analysis.range as f32) * g.h - 22.0).max(2.0)))
                    .w(px(60.))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .px(px(4.))
                            .rounded(px(3.))
                            .bg(alpha(0x000000, 0.7))
                            .font_family(MONO)
                            .text_size(px(9.5))
                            .text_color(rgb(TEXT))
                            .child(hz_text(view.columns[i] as f32)),
                    )
            })
            .collect();
        let matching = self.match_panel(&g, cx);
        let empty = eq.bands.is_empty().then(|| {
            div()
                .absolute()
                .left(px(0.))
                .top(px(g.h * 0.22))
                .w(px(g.w))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.))
                .text_size(px(13.))
                .text_color(rgb(TEXT_3))
                .child("Click anywhere to add a band, or drag the line")
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_4))
                        .child("A cut at either end, a shelf dragged from them, a notch low down, a bell between · ⌥ for a dynamic one"),
                )
        });
        div()
            .id("eq-display")
            .relative()
            .w(px(g.w))
            .h(px(g.h))
            .overflow_hidden()
            .debug_selector(|| "eq-display".into())
            .child(picture)
            .children(labels)
            .child(range)
            .children(empty)
            .children(reach)
            .children(dots)
            .children(readout)
            .children(panel)
            .children(peak_labels)
            .children(matching)
            .on_mouse_move(cx.listener(move |ws, e: &MouseMoveEvent, _, cx| {
                let Some(v) = ws.eq_view.as_mut() else { return };
                let o = v.display.get().origin;
                v.pointer = Some((f32::from(e.position.x - o.x), f32::from(e.position.y - o.y)));
                v.hover = None;
                cx.notify();
            }))
            .on_hover(cx.listener(|ws, over: &bool, _, cx| {
                if !over && let Some(v) = ws.eq_view.as_mut() {
                    (v.pointer, v.hover) = (None, None);
                    cx.notify();
                }
            }))
            .on_scroll_wheel(cx.listener(|ws, e: &ScrollWheelEvent, _, cx| {
                // Over the plot, the wheel works on the band picked last.
                if let Some(id) = ws.eq_view.as_ref().and_then(|v| v.lead()) {
                    ws.wheel_band(id, e, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| ws.press_plot(e, cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| ws.plot_menu(e.position, cx)),
            )
    }
}
