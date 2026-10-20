//! The EQ window's display: the plot (the analyzer, each band's area, the curves, the dots,
//! their readouts and the panel of the bands picked), the frequency scale or piano under it,
//! and the analyzer's scale and output meter beside it.

use gpui::{
    Bounds, Context, Div, MouseButton, MouseDownEvent, MouseMoveEvent, PathBuilder, Point,
    ScrollWheelEvent, Stateful, canvas, div, fill, linear_color_stop, linear_gradient, point,
    prelude::*, px, rgb, size,
};
use pina_engine::{
    eq::{Section, band_sections, response_db},
    project::{Eq, EqBand, MAX_BANDS, Placement},
};

use super::{
    CURVE, EqKnob, EqOp, Geo, RANGES, SCALE_H, SIDE_W, band_at, band_color, dot_db, hz_text,
    note_of, note_text, part_color, q_text,
};
use crate::{
    menu::MenuAction,
    theme::*,
    workspace::{Drag, Name, Workspace, pt},
};

/// A band as it's drawn: its curve (dB at each of the curve's points) and where its dot is.
pub(super) struct Drawn {
    pub id: u16,
    pub index: usize,
    pub color: u32,
    pub band: EqBand,
    pub curve: Vec<f32>,
    /// Its curve at the end of its dynamic range, if it's dynamic.
    pub reach: Option<Vec<f32>>,
    pub dot: (f32, f32),
}

/// A line through points, dashed if asked.
fn line(
    points: &[Point<gpui::Pixels>],
    width: f32,
    dashed: bool,
) -> Option<gpui::Path<gpui::Pixels>> {
    let mut p = PathBuilder::stroke(px(width));
    let mut drawn = 0.0f32;
    p.move_to(*points.first()?);
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let step = f32::from((b.x - a.x).abs() + (b.y - a.y).abs());
        drawn += step;
        // Dashes of 6, gaps of 4.
        if dashed && drawn % 10.0 > 6.0 {
            p.move_to(b);
        } else {
            p.line_to(b);
        }
    }
    p.build().ok()
}

/// Frequencies worth a line and a label across a stretch, `apart` pixels at least.
fn marks(geo: &Geo, apart: f32) -> Vec<(f64, bool)> {
    let mut out: Vec<(f64, bool)> = Vec::new();
    let mut last = f32::MIN;
    for decade in [1.0, 10.0, 100.0, 1_000.0, 10_000.0] {
        for k in 1..10 {
            let f = decade * k as f64;
            if f < geo.lo || f > geo.hi {
                continue;
            }
            let x = geo.x(f);
            let major = k == 1 || k == 2 || k == 5;
            if x - last >= apart || (major && x - last >= apart * 0.6) {
                out.push((f, major));
                last = x;
            }
        }
    }
    out
}

fn freq_label(f: f64) -> String {
    match f {
        f if f >= 1_000.0 => format!("{}k", f / 1_000.0),
        f => format!("{f}"),
    }
}

impl Workspace {
    /// Each band as it's drawn, at `freqs`: its curve (with its dynamic move, while playing) and
    /// where its dot sits.
    pub(super) fn eq_drawn(&self, eq: &Eq, geo: &Geo, freqs: &[f64]) -> Vec<Drawn> {
        let rate = self.session.project.sample_rate as f64;
        let playing = self.player.playing();
        let tap = self.player.eq_tap();
        let curve = |s: &[Section]| -> Vec<f32> {
            freqs
                .iter()
                .map(|&f| response_db(s, f, rate) as f32)
                .collect()
        };
        eq.bands
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let moved = if playing { tap.moved(i) } else { 0.0 };
                let mut heard = *b;
                if eq.scale > 0.0 {
                    heard.gain_db += moved / eq.scale;
                }
                let reach = b.dynamic.filter(|_| b.shape.has_gain()).map(|d| {
                    let far = EqBand {
                        gain_db: b.gain_db + d.range_db,
                        ..*b
                    };
                    curve(&band_sections(&far, eq.scale, rate))
                });
                let db = super::dot_db(&heard, eq.scale).clamp(-geo.range, geo.range);
                Drawn {
                    id: b.id,
                    index: i,
                    color: band_color(i),
                    band: *b,
                    curve: curve(&band_sections(&heard, eq.scale, rate)),
                    reach,
                    dot: (geo.x(b.freq as f64), geo.y(db)),
                }
            })
            .collect()
    }

    /// The display: the plot with its scale under it, and the analyzer's scale and meter.
    pub(super) fn eq_display_area(&mut self, eq: &Eq, cx: &mut Context<Self>) -> Div {
        let rate = self.session.project.sample_rate as f64;
        let Some(view) = self.eq_view.as_ref() else {
            return div();
        };
        let geo = view.geo(rate);
        let (dw, dh) = view.display_size();
        let n = (geo.w / 2.0) as usize;
        let freqs: Vec<f64> = (0..n)
            .map(|i| geo.freq(i as f32 / (n - 1) as f32 * geo.w))
            .collect();
        let bounds = view.display.clone();
        if view.list {
            let list = self.eq_list(dw, dh, cx);
            return div()
                .relative()
                .w(px(dw))
                .h(px(dh))
                .child(
                    canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
                .child(list);
        }
        let drawn = self.eq_drawn(eq, &geo, &freqs);
        let plot = self.eq_plot(eq, &geo, &freqs, &drawn, cx);
        let scale = self.eq_scale(&geo, &drawn, cx);
        let side = self.eq_side(&geo, dh, cx);
        div()
            .relative()
            .w(px(dw))
            .h(px(dh))
            .flex()
            .child(
                canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(
                div()
                    .w(px(geo.w))
                    .flex()
                    .flex_col()
                    .child(plot)
                    .child(scale),
            )
            .child(side)
    }

    fn eq_plot(
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
            .map(|h| super::tools::peaks(h, floor))
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
                            .map(|b| pina_engine::eq::band_response(b, 1.0, f, rate) as f32)
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

    /// A band's dot: its color, ringed white when picked, hollow when off.
    fn eq_dot(&self, d: &Drawn, picked: &[u16], hover: Option<u16>, cx: &mut Context<Self>) -> Div {
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
    fn eq_readout(&self, d: &Drawn, g: &Geo, cx: &mut Context<Self>) -> Div {
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

    /// Under the plot: its frequencies (or a piano), the one under the pointer; drag it up to
    /// zoom in, sideways to pan, double-click it to see all.
    fn eq_scale(&self, g: &Geo, drawn: &[Drawn], cx: &mut Context<Self>) -> Stateful<Div> {
        let view = self.eq_view.as_ref().expect("the window's open");
        let g = *g;
        let piano = view.piano;
        let at = view.scale_pointer.or(view.pointer.map(|p| p.0));
        let labels: Vec<Div> = marks(&g, 42.0)
            .into_iter()
            .filter(|_| !piano)
            .map(|(f, major)| {
                div()
                    .absolute()
                    .left(px(g.x(f) + 3.0))
                    .top(px(4.))
                    .font_family(MONO)
                    .text_size(px(9.5))
                    .text_color(rgb(if major { TEXT_3 } else { TEXT_4 }))
                    .child(freq_label(f))
            })
            .collect();
        // The keys, A0 to C8, where they fall; each band's dot on them.
        let keys = piano.then(|| {
            let dots: Vec<(f32, u32)> = drawn.iter().map(|d| (d.dot.0, d.color)).collect();
            let hovered = at.map(|x| note_of(g.freq(x) as f32).0);
            canvas(
                |_, _, _| {},
                move |b, _, window, _| {
                    let o = b.origin;
                    let h = f32::from(b.size.height);
                    for note in 21u8..=108 {
                        let edge = |n: f32| g.x((440.0 * 2f32.powf((n - 69.0) / 12.0)) as f64);
                        let (x0, x1) = (edge(note as f32 - 0.5), edge(note as f32 + 0.5));
                        if x1 < 0.0 || x0 > g.w {
                            continue;
                        }
                        let black = matches!(note % 12, 1 | 3 | 6 | 8 | 10);
                        let lit = hovered == Some(note);
                        let color = match (black, lit) {
                            (_, true) => alpha(ACCENT, 0.9),
                            (true, false) => rgb(0x1a1b20).into(),
                            (false, false) => rgb(0xd9dbe2).into(),
                        };
                        let r = Bounds::new(
                            point(o.x + px(x0 + 0.5), o.y),
                            size(
                                px((x1 - x0 - 1.0).max(0.5)),
                                px(if black { h * 0.62 } else { h }),
                            ),
                        );
                        window.paint_quad(fill(r, color));
                    }
                    for (x, color) in &dots {
                        window.paint_quad(fill(
                            Bounds::new(
                                point(o.x + px(x - 3.0), o.y + px(h - 7.0)),
                                size(px(6.), px(6.)),
                            ),
                            rgb(*color),
                        ));
                    }
                },
            )
            .absolute()
            .size_full()
        });
        let told = at.filter(|&x| (0.0..=g.w).contains(&x)).map(|x| {
            let f = g.freq(x) as f32;
            let text = match piano {
                true => note_text(f),
                false => hz_text(f),
            };
            div()
                .absolute()
                .left(px((x - 34.0).clamp(0.0, g.w - 68.0)))
                .top(px(2.))
                .w(px(68.))
                .flex()
                .justify_center()
                .child(
                    div()
                        .px(px(5.))
                        .rounded(px(3.))
                        .bg(rgb(0x2a2c35))
                        .font_family(MONO)
                        .text_size(px(9.5))
                        .text_color(rgb(TEXT))
                        .child(text),
                )
        });
        div()
            .id("eq-scale")
            .relative()
            .w(px(g.w))
            .h(px(SCALE_H))
            .flex_none()
            .bg(rgb(0x0b0c0f))
            .border_t_1()
            .border_color(rgb(LINE))
            .cursor(gpui::CursorStyle::ResizeUpDown)
            .debug_selector(|| "eq-scale".into())
            .children(keys)
            .children(labels)
            .children(told)
            .on_mouse_move(cx.listener(|ws, e: &MouseMoveEvent, _, cx| {
                let Some(v) = ws.eq_view.as_mut() else { return };
                let o = v.display.get().origin;
                v.scale_pointer = Some(f32::from(e.position.x - o.x));
                cx.notify();
            }))
            .on_hover(cx.listener(|ws, over: &bool, _, cx| {
                if !over && let Some(v) = ws.eq_view.as_mut() {
                    v.scale_pointer = None;
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let Some(v) = ws.eq_view.as_mut() else { return };
                    if e.click_count == 2 {
                        v.span = (super::LOW, super::HIGH);
                        cx.notify();
                        return;
                    }
                    let span = v.span;
                    ws.begin_drag(
                        Drag::EqScale {
                            from: pt(e.position),
                            span,
                        },
                        cx,
                    );
                }),
            )
    }

    /// Beside the plot: the analyzer's scale and the output meter, its loudest peak over it
    /// (click to clear).
    fn eq_side(&self, g: &Geo, dh: f32, cx: &mut Context<Self>) -> Div {
        let view = self.eq_view.as_ref().expect("the window's open");
        let range = view.analysis.range as f32;
        let gh = g.h;
        let ay = move |db: f32| (-db / range * gh).clamp(0.0, gh);
        let step = if range > 90.0 { 20.0 } else { 10.0 };
        let labels: Vec<Div> = (1..(range / step) as i32)
            .map(|k| -(k as f32) * step)
            .map(|d| {
                div()
                    .absolute()
                    .left(px(4.))
                    .top(px(ay(d) - 6.0))
                    .font_family(MONO)
                    .text_size(px(9.))
                    .text_color(rgb(TEXT_4))
                    .child(format!("{d:.0}").replace('-', "−"))
            })
            .collect();
        let meter = view.meter;
        let peak = view.peak;
        let h = g.h;
        let bars = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let o = b.origin;
                for (side, db) in meter.into_iter().enumerate() {
                    let x = o.x + px(side as f32 * 7.0);
                    window.paint_quad(fill(
                        Bounds::new(point(x, o.y), size(px(5.), px(h))),
                        rgb(0x1a1b21),
                    ));
                    let top = ay(db.min(0.0));
                    if top < h {
                        window.paint_quad(fill(
                            Bounds::new(point(x, o.y + px(top)), size(px(5.), px(h - top))),
                            linear_gradient(
                                180.,
                                linear_color_stop(
                                    rgb(if db > -1.0 { METER_HIGH } else { METER_MID }),
                                    0.,
                                ),
                                linear_color_stop(rgb(METER_LOW), 0.35),
                            ),
                        ));
                    }
                }
            },
        )
        .absolute()
        .left(px(SIDE_W - 22.0))
        .top(px(0.))
        .w(px(14.))
        .h(px(h));
        div()
            .relative()
            .w(px(SIDE_W))
            .h(px(dh))
            .flex_none()
            .bg(rgb(0x0b0c0f))
            .border_l_1()
            .border_color(rgb(LINE))
            .children(labels)
            .child(bars)
            .child(
                div()
                    .absolute()
                    .left(px(0.))
                    .bottom(px(3.))
                    .w(px(SIDE_W))
                    .flex()
                    .justify_center()
                    .cursor_pointer()
                    .font_family(MONO)
                    .text_size(px(9.5))
                    .text_color(rgb(if peak > -0.1 { METER_HIGH } else { TEXT_3 }))
                    .debug_selector(|| "eq-peak".into())
                    .child(if peak <= -100.0 {
                        "−∞".to_string()
                    } else {
                        db_text(peak)
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            if let Some(v) = ws.eq_view.as_mut() {
                                v.peak = -120.0;
                                cx.notify();
                            }
                        }),
                    ),
            )
    }
}
