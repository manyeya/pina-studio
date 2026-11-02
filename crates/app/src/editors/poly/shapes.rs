//! The envelopes and LFOs: a tab for each, the one in hand drawn, its points to drag, and its ↗ to
//! drop on any knob.

use super::*;

const PLOT_W: f32 = 480.0;
const PLOT_H: f32 = 84.0;
/// An LFO's wave at `p` (0 to 1), for drawing: -1 to 1.
fn wave_at(w: &LfoWave, p: f32) -> f32 {
    use std::f32::consts::TAU;
    match w {
        LfoWave::Sine => (TAU * p).sin(),
        LfoWave::Triangle => 1.0 - 4.0 * ((p + 0.25).fract() - 0.5).abs(),
        LfoWave::SawUp => 2.0 * p - 1.0,
        LfoWave::SawDown => 1.0 - 2.0 * p,
        LfoWave::Square => {
            if p < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        LfoWave::Hold => [0.4, -0.7, 0.9, -0.2, 0.6, -0.9, 0.1, 0.7][(p * 8.0) as usize % 8],
        LfoWave::Smooth => (TAU * p).sin() * 0.6 + (TAU * 3.0 * p).sin() * 0.3,
        LfoWave::Lorenz | LfoWave::Rossler => {
            (TAU * p).sin() * 0.5 + (TAU * 2.618 * p).sin() * 0.3 + (TAU * 7.1 * p).sin() * 0.2
        }
        LfoWave::Drawn(points) => {
            let at = points.partition_point(|q| q.0 <= p);
            let v = match (
                at.checked_sub(1).and_then(|i| points.get(i)),
                points.get(at),
            ) {
                (Some(a), Some(b)) => a.1 + (b.1 - a.1) * (p - a.0) / (b.0 - a.0).max(1e-5),
                (Some(a), None) => a.1,
                (None, Some(b)) => b.1,
                _ => 0.5,
            };
            v * 2.0 - 1.0
        }
    }
}

/// How many points a drawn LFO has.
const DRAWN: usize = 64;

impl Workspace {
    /// A stroke on an LFO's picture: the wave there drawn to where the pointer is. Its first turns
    /// whatever wave it had into points, so what's drawn starts from what was playing.
    pub(crate) fn draw_lfo(&mut self, t: usize, l: usize, x: f32, y: f32) {
        let Some(p) = self.poly_mut(t) else { return };
        let wave = &mut p.lfos[l].wave;
        if !matches!(wave, LfoWave::Drawn(_)) {
            let was = wave.clone();
            *wave = LfoWave::Drawn(
                (0..DRAWN)
                    .map(|i| {
                        let at = i as f32 / (DRAWN - 1) as f32;
                        (at, wave_at(&was, at.min(0.9999)) * 0.5 + 0.5)
                    })
                    .collect(),
            );
        }
        if let LfoWave::Drawn(points) = wave
            && let Some(q) = points.get_mut((x * (DRAWN - 1) as f32).round() as usize)
        {
            q.1 = 1.0 - y.clamp(0.0, 1.0);
        }
    }

    pub(crate) fn shape_card(&self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let now = self
            .poly_view
            .as_ref()
            .map_or(PolyShape::Env(0), |v| v.shape);
        let tab = |name: String, shape: PolyShape, source: PolySource, cx: &mut Context<Self>| {
            let sel = name.to_lowercase().replace(' ', "-");
            div()
                .flex()
                .items_center()
                .gap(px(2.))
                .child(
                    seg(name, now == shape, 22.)
                        .debug_selector(move || format!("poly-shape-{sel}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                if let Some(v) = ws.poly_view.as_mut() {
                                    v.shape = shape;
                                }
                                cx.notify();
                            }),
                        ),
                )
                .child(self.source_grip(t, source, cx))
        };
        let tabs = div()
            .flex()
            .flex_wrap()
            .gap(px(1.))
            .children((0..ENVS).map(|i| {
                tab(
                    format!("E{}", i + 1),
                    PolyShape::Env(i),
                    PolySource::Env(i as u8),
                    cx,
                )
            }))
            .children((0..LFOS).map(|i| {
                tab(
                    format!("L{}", i + 1),
                    PolyShape::Lfo(i),
                    PolySource::Lfo(i as u8),
                    cx,
                )
            }));
        let body = match now {
            PolyShape::Env(e) => self.env_body(t, e, cx),
            PolyShape::Lfo(l) => self.lfo_body(t, p, l, cx),
        };
        div()
            .w(px(868.))
            .p(px(8.))
            .flex()
            .flex_col()
            .gap(px(5.))
            .rounded(px(9.))
            .bg(rgb(0x14151a))
            .border_1()
            .border_color(rgb(LINE))
            .child(tabs)
            .child(body)
    }

    fn env_body(&self, t: usize, e: usize, cx: &mut Context<Self>) -> Div {
        let f = |x: EnvField| env_param(e, x);
        let k = |x: EnvField, name: &'static str, cx: &mut Context<Self>| {
            self.poly_knob(t, f(x), name, 28.0, SHAPE_COLOR, cx)
        };
        let (a, d, s, r) = (
            self.poly_at(t, f(EnvField::Attack)),
            self.poly_at(t, f(EnvField::Decay)),
            self.poly_at(t, f(EnvField::Sustain)),
            self.poly_at(t, f(EnvField::Release)),
        );
        let curve = POLY_PARAMS[f(EnvField::Curve)]
            .scale
            .value(self.poly_at(t, f(EnvField::Curve)));
        let plot = self.poly_view.as_ref().map(|v| v.plot.clone());
        let third = 1.0 / 3.0;
        let (ax, dx, rx) = (a * third, third + d * third, 2.0 * third + r * third);
        let bend = move |x: f32| {
            let c = curve * 5.0;
            match c < 0.01 {
                true => x,
                false => (1.0 - (-c * x).exp()) / (1.0 - (-c).exp()),
            }
        };
        let line = canvas(
            move |b, _, _| {
                if let Some(p) = &plot {
                    p.set(b);
                }
            },
            move |b, _, window, _| {
                let n = 40;
                let rise = (0..=n).map(|i| {
                    let x = i as f32 / n as f32;
                    (x * ax, 1.0 - bend(x))
                });
                let fall = (0..=n).map(|i| {
                    let x = i as f32 / n as f32;
                    (ax + x * (dx - ax), 1.0 - (1.0 + (s - 1.0) * bend(x)))
                });
                let stay = [(dx, 1.0 - s), (2.0 * third, 1.0 - s)].into_iter();
                let end = (0..=n).map(|i| {
                    let x = i as f32 / n as f32;
                    (
                        2.0 * third + x * (rx - 2.0 * third),
                        1.0 - s * (1.0 - bend(x)),
                    )
                });
                polyline(
                    window,
                    b,
                    rise.chain(fall).chain(stay).chain(end),
                    1.8,
                    alpha(SHAPE_COLOR, 0.95),
                );
            },
        )
        .absolute()
        .size_full();
        div()
            .flex()
            .gap(px(10.))
            .child(
                picture()
                    .w(px(PLOT_W))
                    .h(px(PLOT_H))
                    .debug_selector(|| "poly-env-plot".into())
                    .child(line)
                    .child(point_handle(
                        ax * PLOT_W,
                        6.0,
                        SHAPE_COLOR,
                        2,
                        "poly-env-attack",
                        cx,
                    ))
                    .child(point_handle(
                        dx * PLOT_W,
                        (1.0 - s) * PLOT_H,
                        SHAPE_COLOR,
                        3,
                        "poly-env-decay",
                        cx,
                    ))
                    .child(point_handle(
                        rx * PLOT_W,
                        PLOT_H - 6.0,
                        SHAPE_COLOR,
                        4,
                        "poly-env-release",
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_wrap()
                    .justify_between()
                    .child(k(EnvField::Attack, "Attack", cx))
                    .child(k(EnvField::Hold, "Hold", cx))
                    .child(k(EnvField::Decay, "Decay", cx))
                    .child(k(EnvField::Sustain, "Sustain", cx))
                    .child(k(EnvField::Release, "Release", cx))
                    .child(k(EnvField::Curve, "Curve", cx)),
            )
    }

    fn lfo_body(&self, t: usize, p: &Poly, l: usize, cx: &mut Context<Self>) -> Div {
        let f = |x: LfoField| lfo_param(l, x);
        let k = |x: LfoField, name: &'static str, cx: &mut Context<Self>| {
            self.poly_knob(t, f(x), name, 26.0, SHAPE_COLOR, cx)
        };
        let lfo = p.lfos[l].clone();
        let wave = lfo.wave.clone();
        let plot = self.poly_view.as_ref().map(|v| v.plot.clone());
        let line = canvas(
            move |b, _, _| {
                if let Some(p) = &plot {
                    p.set(b);
                }
            },
            move |b, _, window, _| {
                let points = (0..240).map(|i| {
                    let x = i as f32 / 239.0;
                    (x, 0.5 - 0.46 * wave_at(&wave, x.min(0.9999)))
                });
                polyline(window, b, points, 1.8, alpha(SHAPE_COLOR, 0.95));
            },
        )
        .absolute()
        .size_full();
        let waves = [
            ("Sine", LfoWave::Sine),
            ("Triangle", LfoWave::Triangle),
            ("Saw Up", LfoWave::SawUp),
            ("Saw Down", LfoWave::SawDown),
            ("Square", LfoWave::Square),
            ("Hold", LfoWave::Hold),
            ("Smooth", LfoWave::Smooth),
            ("Chaos: Lorenz", LfoWave::Lorenz),
            ("Chaos: Rossler", LfoWave::Rossler),
        ];
        let name = waves
            .iter()
            .find(|w| w.1 == lfo.wave)
            .map_or("Drawn", |w| w.0)
            .to_string();
        let menu = self.poly_menu(
            "poly-lfo-wave".into(),
            name,
            move |_| {
                waves
                    .iter()
                    .map(|(n, w)| {
                        (
                            n.to_string(),
                            MenuAction::Poly(t, PolyOp::LfoWave(l, w.clone())),
                        )
                    })
                    .collect()
            },
            false,
            cx,
        );
        const SYNC: [(&str, f32); 7] = [
            ("Free", 0.0),
            ("1/8", 0.5),
            ("1/4", 1.0),
            ("1/2", 2.0),
            ("1/1", 4.0),
            ("2/1", 8.0),
            ("4/1", 16.0),
        ];
        let beats = lfo.beats;
        let sync = tray()
            .p(px(2.))
            .rounded(px(7.))
            .children(SYNC.map(|(name, b)| {
                seg(name, beats == b, 20.).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.edit(cx, |ws| {
                            if let Some(p) = ws.poly_mut(t) {
                                p.lfos[l].beats = b;
                            }
                        });
                    }),
                )
            }));
        let flip =
            |name: &'static str, now: bool, set: fn(&mut PolyLfo), cx: &mut Context<Self>| {
                button(name, now).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.edit(cx, |ws| {
                            if let Some(p) = ws.poly_mut(t) {
                                set(&mut p.lfos[l]);
                            }
                        });
                    }),
                )
            };
        div()
            .flex()
            .gap(px(10.))
            .child(
                picture()
                    .w(px(PLOT_W))
                    .h(px(PLOT_H))
                    .cursor_crosshair()
                    .debug_selector(|| "poly-lfo-plot".into())
                    .child(line)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let id = DRAW_LFO + l as u8;
                            ws.begin_drag(Drag::PolyPoint { id }, cx);
                            ws.drag_poly_point(id, pt(e.position));
                        }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .gap(px(4.))
                            .child(div().w(px(112.)).child(menu))
                            .child(flip("Note", lfo.retrigger, |l| l.retrigger ^= true, cx))
                            .child(flip("Once", lfo.one_shot, |l| l.one_shot ^= true, cx)),
                    )
                    .child(sync)
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .child(k(LfoField::Rate, "Rate", cx))
                            .child(k(LfoField::Rise, "Rise", cx))
                            .child(k(LfoField::Smooth, "Smooth", cx))
                            .child(k(LfoField::Phase, "Phase", cx)),
                    ),
            )
    }
}
