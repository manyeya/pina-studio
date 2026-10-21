//! A card's picture of its modulator, moving.

use super::*;

/// What a card draws.
pub(crate) enum Visual {
    Wave(LfoShape, f32, f32, bool),
    Steps(Vec<f32>, usize),
    Envelope([f32; 4], f32),
    Trace(Vec<f32>, bool),
    Level(f32),
}

pub(crate) fn paint_visual(b: Bounds<Pixels>, v: &Visual, color: u32, window: &mut Window) {
    let (x0, y0) = (f32::from(b.origin.x), f32::from(b.origin.y));
    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
    let at = |x: f32, y: f32| point(px(x0 + x), px(y0 + y));
    // -1 to 1 (or 0 to 1) to a height, with room round it.
    let y_of = |v: f32, bipolar: bool| {
        let u = if bipolar { (v + 1.0) / 2.0 } else { v };
        6.0 + (1.0 - u.clamp(0.0, 1.0)) * (h - 12.0)
    };
    let line = |pts: &[(f32, f32)], width: f32, ink: Hsla, window: &mut Window| {
        let mut p = PathBuilder::stroke(px(width));
        for (i, &(x, y)) in pts.iter().enumerate() {
            match i {
                0 => p.move_to(at(x, y)),
                _ => p.line_to(at(x, y)),
            }
        }
        if let Ok(p) = p.build() {
            window.paint_path(p, ink);
        }
    };
    let area = |pts: &[(f32, f32)], base: f32, ink: Hsla, window: &mut Window| {
        if pts.len() < 2 {
            return;
        }
        let mut p = PathBuilder::fill();
        p.move_to(at(pts[0].0, base));
        for &(x, y) in pts {
            p.line_to(at(x, y));
        }
        p.line_to(at(pts[pts.len() - 1].0, base));
        p.close();
        if let Ok(p) = p.build() {
            window.paint_path(p, ink);
        }
    };
    let dot = |x: f32, y: f32, window: &mut Window| {
        window.paint_quad(
            fill(
                Bounds::centered_at(at(x, y), size(px(8.), px(8.))),
                rgb(0xffffff),
            )
            .corner_radii(px(4.)),
        );
        window.paint_quad(
            fill(
                Bounds::centered_at(at(x, y), size(px(5.), px(5.))),
                rgb(color),
            )
            .corner_radii(px(2.5)),
        );
    };
    // The middle, faint.
    window.paint_quad(fill(
        Bounds::new(at(0.0, h / 2.0), size(px(w), px(1.))),
        alpha(0xffffff, 0.05),
    ));
    match v {
        Visual::Wave(shape, phase, now, unipolar) => {
            // Two cycles across, as it plays; the dot where it is in the first.
            let n = 96;
            let pts: Vec<(f32, f32)> = (0..=n)
                .map(|i| {
                    let x = i as f32 / n as f32;
                    let s = shape.at((x * 2.0 + phase) as f64);
                    let s = if *unipolar { 0.5 + 0.5 * s } else { s };
                    (x * w, y_of(s, !unipolar))
                })
                .collect();
            area(&pts, y_of(0.0, !unipolar), alpha(color, 0.12), window);
            line(&pts, 2.0, rgb(color).into(), window);
            let x = ((*now - phase).rem_euclid(1.0)) / 2.0;
            let s = shape.at((x * 2.0 + phase) as f64);
            let s = if *unipolar { 0.5 + 0.5 * s } else { s };
            window.paint_quad(fill(
                Bounds::new(at(x * w, 0.0), size(px(1.), px(h))),
                alpha(color, 0.35),
            ));
            dot(x * w, y_of(s, !unipolar), window);
        }
        Visual::Steps(values, now) => {
            let n = values.len().max(1);
            let cell = w / n as f32;
            for (i, &v) in values.iter().enumerate() {
                let top = y_of(v, false);
                let lit = i == *now;
                window.paint_quad(
                    fill(
                        Bounds::new(
                            at(i as f32 * cell + 1.0, top),
                            size(px(cell - 2.0), px(h - 6.0 - top)),
                        ),
                        if lit {
                            rgb(color).into()
                        } else {
                            alpha(color, 0.45)
                        },
                    )
                    .corner_radii(px(2.)),
                );
            }
        }
        Visual::Envelope([a, d, s, r], level) => {
            // Its shape, each stage as wide as it's long (on a log scale), and its level now.
            let span = |ms: f32| (ms.max(1.0)).ln() + 1.0;
            let total = span(*a) + span(*d) + span(1000.0) * 0.6 + span(*r);
            let k = w / total;
            let xa = span(*a) * k;
            let xd = xa + span(*d) * k;
            let xs = xd + span(1000.0) * 0.6 * k;
            let pts = [
                (0.0, y_of(0.0, false)),
                (xa, y_of(1.0, false)),
                (xd, y_of(*s, false)),
                (xs, y_of(*s, false)),
                (w, y_of(0.0, false)),
            ];
            area(&pts, y_of(0.0, false), alpha(color, 0.12), window);
            line(&pts, 2.0, rgb(color).into(), window);
            let y = y_of(*level, false);
            window.paint_quad(fill(
                Bounds::new(at(0.0, y), size(px(w), px(1.))),
                alpha(color, 0.5),
            ));
            dot(w - 8.0, y, window);
        }
        Visual::Trace(values, bipolar) => {
            let n = TRACE.max(2);
            let pts: Vec<(f32, f32)> = values
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let x = w - (values.len() - 1 - i) as f32 * (w / (n - 1) as f32);
                    (x, y_of(v, *bipolar))
                })
                .collect();
            area(&pts, y_of(0.0, *bipolar), alpha(color, 0.18), window);
            line(&pts, 2.0, rgb(color).into(), window);
            if let Some(&(x, y)) = pts.last() {
                dot(x - 4.0, y, window);
            }
        }
        Visual::Level(v) => {
            let wv = w * v.clamp(0.0, 1.0);
            window.paint_quad(
                fill(
                    Bounds::new(at(0.0, h / 2.0 - 6.0), size(px(wv), px(12.))),
                    alpha(color, 0.6),
                )
                .corner_radii(px(3.)),
            );
            dot(wv.max(4.0), h / 2.0, window);
        }
    }
}

impl Workspace {
    /// A card's picture of its modulator, moving: the wave and where it is, the steps (paint
    /// them), the envelope and its level, the follower's and random's recent values.
    pub(crate) fn mod_visual(
        &self,
        t: usize,
        k: usize,
        kind: &ModKind,
        color: u32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = self.session.project.tracks[t].id;
        let value = self.mod_values.get(&id).map_or(0.0, |v| v[k]);
        let trace: Vec<f32> = self
            .mod_trace
            .get(&(id, k))
            .map(|d| d.iter().copied().collect())
            .unwrap_or_default();
        let shape = match kind {
            ModKind::Lfo {
                shape,
                rate,
                phase,
                unipolar,
            } => {
                let at = (self.mod_phase(*rate) + *phase as f64).rem_euclid(1.0);
                Visual::Wave(*shape, *phase, at as f32, *unipolar)
            }
            ModKind::Steps { values, rate, .. } => {
                let n = values.len().max(1);
                let i = (self.mod_phase(*rate).floor() as i64).rem_euclid(n as i64) as usize;
                Visual::Steps(values.clone(), i)
            }
            ModKind::Envelope {
                attack_ms,
                decay_ms,
                sustain,
                release_ms,
            } => Visual::Envelope([*attack_ms, *decay_ms, *sustain, *release_ms], value),
            ModKind::Follower { .. } => Visual::Trace(trace, false),
            ModKind::Random { .. } => Visual::Trace(trace, true),
            ModKind::Macro { value } => Visual::Level(*value),
            ModKind::Shared { .. } => Visual::Level(0.0),
        };
        let steps = matches!(kind, ModKind::Steps { .. });
        let bounds = self.mod_steps.clone();
        div()
            .h(px(64.))
            .flex_none()
            .rounded(px(8.))
            .bg(rgb(0x0b0b0e))
            .border_1()
            .border_color(rgb(LINE_SOFT))
            .overflow_hidden()
            .relative()
            .debug_selector(move || format!("mod-{k}-visual"))
            .child(
                canvas(
                    move |b, _, _| {
                        if steps {
                            bounds.borrow_mut().insert((t, k), b);
                        }
                    },
                    move |b, _, window, _| paint_visual(b, &shape, color, window),
                )
                .absolute()
                .size_full(),
            )
            .when(steps, |d| {
                d.cursor_crosshair().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        ws.begin_drag(Drag::ModSteps { track: t, index: k }, cx);
                        ws.paint_mod_step(t, k, pt(e.position));
                        ws.changed(cx);
                    }),
                )
            })
    }
}
