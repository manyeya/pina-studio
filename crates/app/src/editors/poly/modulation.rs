//! What moves the sound: a chip for each source, picked to shape it and dragged onto any knob;
//! the source in hand drawn, its settings, and everything it moves; the eight macros, named.

use super::*;

/// An LFO's wave at `p` (0 to 1), for drawing: -1 to 1.
pub(crate) fn wave_at(w: &LfoWave, p: f32) -> f32 {
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

const WAVES: [(&str, LfoWave); 9] = [
    ("Sine", LfoWave::Sine),
    ("Triangle", LfoWave::Triangle),
    ("Saw up", LfoWave::SawUp),
    ("Saw down", LfoWave::SawDown),
    ("Square", LfoWave::Square),
    ("Hold", LfoWave::Hold),
    ("Smooth", LfoWave::Smooth),
    ("Chaos: Lorenz", LfoWave::Lorenz),
    ("Chaos: Rossler", LfoWave::Rossler),
];

const SYNC: [(&str, f32); 7] = [
    ("Free", 0.0),
    ("1/8", 0.5),
    ("1/4", 1.0),
    ("1/2", 2.0),
    ("1 bar", 4.0),
    ("2 bars", 8.0),
    ("4 bars", 16.0),
];

/// Where the envelope's points sit across its picture: attack, decay, release, 0 to 1.
fn env_points(a: f32, d: f32, r: f32) -> [f32; 3] {
    let third = 1.0 / 3.0;
    [a * third, third + d * third, 2.0 * third + r * third]
}

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

    /// A source's chip: picked, its settings show; dragged, a cord follows to the knob it lands
    /// on; pointed at, what it moves lights up. Its bar is where it is now.
    pub(crate) fn source_chip(
        &self,
        t: usize,
        p: &Poly,
        s: PolySource,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let picked = self.poly_view.as_ref().is_some_and(|v| v.source == s);
        let used = p.routes.iter().any(|r| r.source == s || r.by == Some(s));
        let hue = source_color(s);
        let now = self.poly_now(t, s).unwrap_or(0.0).abs().min(1.0);
        let sel = format!("poly-source-{s:?}").to_lowercase();
        div()
            .id(gpui::SharedString::from(sel.clone()))
            .relative()
            .h(px(25.))
            .min_w(px(0.))
            .flex_1()
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .bg(rgb(if picked { RAISED } else { WELL }))
            .when(picked, |d| d.border_1().border_color(rgb(hue)))
            .font_family(MONO)
            .text_size(px(10.5))
            .text_color(rgb(match (picked, used) {
                (true, _) => TEXT,
                (false, true) => hue,
                _ => TEXT_3,
            }))
            .cursor_grab()
            .hover(|d| d.bg(rgb(CARD)))
            .debug_selector(move || sel.clone())
            .child(source_short(s))
            .when(used, |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .bottom_0()
                        .h(px(2.5))
                        .w(gpui::relative(now))
                        .bg(rgb(hue)),
                )
            })
            .on_hover(cx.listener(move |ws, over: &bool, _, cx| {
                if let Some(v) = ws.poly_view.as_mut() {
                    v.pointed = match *over {
                        true => Some(s),
                        false => v.pointed.filter(|x| *x != s),
                    };
                }
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if let Some(v) = ws.poly_view.as_mut() {
                        v.source = s;
                        v.cord = Some(pt(e.position));
                    }
                    ws.begin_drag(
                        Drag::PolySource {
                            track: t,
                            source: s,
                        },
                        cx,
                    );
                }),
            )
    }

    /// The band under the sound: the chips, the source in hand, and what it moves.
    pub(crate) fn poly_modulation(&self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let source = self
            .poly_view
            .as_ref()
            .map_or(PolySource::Env(0), |v| v.source);
        let group =
            |name: &'static str, list: Vec<PolySource>, per: usize, cx: &mut Context<Self>| {
                let rows: Vec<Div> = list
                    .chunks(per)
                    .map(|row| {
                        div()
                            .flex()
                            .gap(px(3.))
                            .children(row.iter().map(|&s| self.source_chip(t, p, s, cx)))
                    })
                    .collect();
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(label(name))
                    .children(rows)
            };
        let envs: Vec<PolySource> = (0..ENVS as u8).map(PolySource::Env).collect();
        let lfos: Vec<PolySource> = (0..LFOS as u8).map(PolySource::Lfo).collect();
        let playing = vec![
            PolySource::Velocity,
            PolySource::Note,
            PolySource::Wheel,
            PolySource::Pressure,
            PolySource::Bend,
            PolySource::Random,
            PolySource::Slide,
        ];
        let chips = div()
            .flex_none()
            .w(px(SOURCES_W))
            .h_full()
            .px(px(12.))
            .py(px(7.))
            .flex()
            .flex_col()
            .gap(px(5.))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(group("Envelopes", envs, 4, cx))
            .child(group("LFOs", lfos, 4, cx))
            .child(group("Playing", playing, 6, cx));
        let moves = self.poly_moves(t, p, source, cx);
        div()
            .flex()
            .h_full()
            .child(chips)
            .child(self.source_editor(t, p, source, cx).flex_1().min_w(px(0.)))
            .child(moves)
    }

    /// The source in hand: its picture and settings.
    fn source_editor(&self, t: usize, p: &Poly, s: PolySource, cx: &mut Context<Self>) -> Div {
        let head = div()
            .flex_none()
            .h(px(30.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(title(source_name(s)).text_color(rgb(source_color(s))));
        let knobs = |list: Vec<Div>| {
            div()
                .flex_none()
                .grid()
                .grid_cols(4)
                .gap_x(px(2.))
                .gap_y(px(8.))
                .children(list)
        };
        // An LFO's shape and timing go in the head, its two switches under its knobs.
        let (mut choices, switches) = match s {
            PolySource::Lfo(l) => {
                let mut all = self.lfo_choices(t, p, l as usize, cx);
                let switches = all.split_off(2);
                (all, switches)
            }
            _ => (Vec::new(), Vec::new()),
        };
        let body = match s {
            PolySource::Env(e) => {
                let e = e as usize;
                let k = |f: EnvField, name: &'static str, cx: &mut Context<Self>| {
                    self.poly_knob(t, env_param(e, f), name, 30.0, cx)
                };
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .gap(px(14.))
                    .child(self.env_plot(t, e, cx))
                    .child(knobs(vec![
                        k(EnvField::Delay, "Delay", cx),
                        k(EnvField::Attack, "Attack", cx),
                        k(EnvField::Hold, "Hold", cx),
                        k(EnvField::Decay, "Decay", cx),
                        k(EnvField::Sustain, "Sustain", cx),
                        k(EnvField::Release, "Release", cx),
                        k(EnvField::Curve, "Curve", cx),
                    ]))
            }
            PolySource::Lfo(l) => {
                let l = l as usize;
                let k = |f: LfoField, name: &'static str, cx: &mut Context<Self>| {
                    self.poly_knob(t, lfo_param(l, f), name, 30.0, cx)
                };
                let free = p.lfos[l].beats == 0.0;
                let mut list = Vec::new();
                if free {
                    list.push(k(LfoField::Rate, "Rate", cx));
                }
                list.extend([
                    k(LfoField::Phase, "Phase", cx),
                    k(LfoField::Rise, "Rise", cx),
                    k(LfoField::Smooth, "Smooth", cx),
                ]);
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .gap(px(14.))
                    .child(self.lfo_plot(t, p, l, cx))
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap(px(10.))
                            .child(knobs(list))
                            .child(div().flex().gap(px(4.)).children(switches)),
                    )
            }
            PolySource::Macro(_) => div().flex_1().child(
                label("One knob that moves several things. Drag its name onto any control to add one more.")
                    .whitespace_normal(),
            ),
            _ => div().flex_1().child(
                label("It comes from how you play. Drag it onto any control.").whitespace_normal(),
            ),
        };
        let head = match s {
            PolySource::Lfo(_) => head.children(choices.drain(..)),
            PolySource::Env(0) => head.child(label("Shapes every note's loudness")),
            PolySource::Env(_) => head.child(label("Free to move anything")),
            _ => head,
        };
        div()
            .px(px(12.))
            .pt(px(6.))
            .pb(px(10.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(head)
            .child(body)
    }

    /// An LFO's shape, its timing, and when it starts.
    fn lfo_choices(&self, t: usize, p: &Poly, l: usize, cx: &mut Context<Self>) -> Vec<Div> {
        let lfo = &p.lfos[l];
        let name = WAVES
            .iter()
            .find(|w| w.1 == lfo.wave)
            .map_or("Drawn", |w| w.0)
            .to_string();
        let shape = self.poly_menu(
            "poly-lfo-wave".into(),
            name,
            move |_| {
                WAVES
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
        let beats = lfo.beats;
        let sync = self.poly_menu(
            "poly-lfo-sync".into(),
            SYNC.iter()
                .find(|s| s.1 == beats)
                .map_or_else(|| format!("{beats} beats"), |s| s.0.to_string()),
            move |_| {
                SYNC.iter()
                    .map(|&(n, b)| (n.to_string(), MenuAction::Poly(t, PolyOp::LfoBeats(l, b))))
                    .collect()
            },
            false,
            cx,
        );
        let flip = |name: &'static str,
                    sel: &'static str,
                    on: bool,
                    set: fn(&mut PolyLfo),
                    cx: &mut Context<Self>| {
            seg(name, on, 24.)
                .bg(rgb(if on { RAISED } else { WELL }))
                .debug_selector(move || sel.into())
                .on_mouse_down(
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
        vec![
            div().w(px(132.)).child(shape),
            div().w(px(92.)).child(sync),
            flip(
                "Retrigger",
                "poly-lfo-retrigger",
                lfo.retrigger,
                |l| l.retrigger ^= true,
                cx,
            ),
            flip(
                "One shot",
                "poly-lfo-once",
                lfo.one_shot,
                |l| l.one_shot ^= true,
                cx,
            ),
        ]
    }

    fn env_plot(&self, t: usize, e: usize, cx: &mut Context<Self>) -> Div {
        let f = |x: EnvField| env_param(e, x);
        let (a, d, s, r) = (
            self.poly_at(t, f(EnvField::Attack)),
            self.poly_at(t, f(EnvField::Decay)),
            self.poly_at(t, f(EnvField::Sustain)),
            self.poly_at(t, f(EnvField::Release)),
        );
        let curve = POLY_PARAMS[f(EnvField::Curve)]
            .scale
            .value(self.poly_at(t, f(EnvField::Curve)));
        let now = self.poly_now(t, PolySource::Env(e as u8));
        let plot = self.poly_view.as_ref().map(|v| v.plot.clone());
        let [ax, dx, rx] = env_points(a, d, r);
        let third = 1.0 / 3.0;
        let bend = move |x: f32| {
            let c = curve * 5.0;
            match c < 0.01 {
                true => x,
                false => (1.0 - (-c * x).exp()) / (1.0 - (-c).exp()),
            }
        };
        // Down a little from the top and up from the bottom, so the points can be reached.
        let y = |v: f32| 0.08 + (1.0 - v) * 0.84;
        let line = canvas(
            move |b, _, _| {
                if let Some(p) = &plot {
                    p.set(b);
                }
            },
            move |b, _, window, _| {
                let n = 40;
                let mut points = Vec::new();
                for i in 0..=n {
                    let x = i as f32 / n as f32;
                    points.push((x * ax, y(bend(x))));
                }
                for i in 0..=n {
                    let x = i as f32 / n as f32;
                    points.push((ax + x * (dx - ax), y(1.0 + (s - 1.0) * bend(x))));
                }
                points.push((2.0 * third, y(s)));
                for i in 0..=n {
                    let x = i as f32 / n as f32;
                    points.push((2.0 * third + x * (rx - 2.0 * third), y(s * (1.0 - bend(x)))));
                }
                filled(window, b, &points, y(0.0), alpha(AMBER, 0.08));
                polyline(window, b, points.iter().copied(), 2.0, rgb(AMBER).into());
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                for (px_, py_) in [(ax, y(1.0)), (dx, y(s)), (rx, y(0.0))] {
                    handle(window, b, px_ * w, py_ * h, AMBER);
                }
                // Where the newest note has it now.
                if let Some(v) = now.filter(|v| *v > 0.001) {
                    window.paint_quad(gpui::fill(
                        Bounds::new(
                            point(b.origin.x, b.origin.y + px(y(v) * h)),
                            gpui::size(b.size.width, px(1.)),
                        ),
                        alpha(TEXT, 0.35),
                    ));
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        picture()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .cursor_ew_resize()
            .debug_selector(|| "poly-env-plot".into())
            .child(line)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let Some(b) = ws.poly_view.as_ref().map(|v| v.plot.get()) else {
                        return;
                    };
                    let x = (pt(e.position).x - f32::from(b.origin.x))
                        / f32::from(b.size.width).max(1.0);
                    // The point nearest across: attack, decay (and sustain), or release.
                    let nearest = [ax, dx, rx]
                        .iter()
                        .enumerate()
                        .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
                        .map_or(1, |(k, _)| k);
                    ws.begin_drag(
                        Drag::PolyPoint {
                            id: ENV_POINTS[nearest],
                        },
                        cx,
                    );
                }),
            )
    }

    fn lfo_plot(&self, t: usize, p: &Poly, l: usize, cx: &mut Context<Self>) -> Div {
        let wave = p.lfos[l].wave.clone();
        let plot = self.poly_view.as_ref().map(|v| v.plot.clone());
        let now = self.poly_now(t, PolySource::Lfo(l as u8));
        let line = canvas(
            move |b, _, _| {
                if let Some(p) = &plot {
                    p.set(b);
                }
            },
            move |b, _, window, _| {
                let points: Vec<(f32, f32)> = (0..240)
                    .map(|i| {
                        let x = i as f32 / 239.0;
                        (x, 0.5 - 0.42 * wave_at(&wave, x.min(0.9999)))
                    })
                    .collect();
                window.paint_quad(gpui::fill(
                    Bounds::new(
                        point(b.origin.x, b.origin.y + b.size.height / 2.0),
                        gpui::size(b.size.width, px(1.)),
                    ),
                    alpha(TEXT, 0.06),
                ));
                filled(window, b, &points, 0.5, alpha(SKY, 0.08));
                polyline(window, b, points.into_iter(), 2.0, rgb(SKY).into());
                if let Some(v) = now {
                    let h = f32::from(b.size.height);
                    window.paint_quad(gpui::fill(
                        Bounds::new(
                            point(b.origin.x, b.origin.y + px((0.5 - 0.42 * v) * h)),
                            gpui::size(b.size.width, px(1.)),
                        ),
                        alpha(TEXT, 0.3),
                    ));
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        picture()
            .flex_1()
            .min_w(px(0.))
            .h_full()
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
            )
    }

    /// Everything the source in hand moves, each with how much: dragged across, or taken off.
    fn poly_moves(
        &self,
        t: usize,
        p: &Poly,
        s: PolySource,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let hue = source_color(s);
        let rows: Vec<Div> = p
            .routes
            .iter()
            .enumerate()
            .filter(|(_, r)| r.source == s)
            .map(|(k, r)| {
                let depth = r.depth;
                let w = 96.0;
                div()
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .w(px(92.))
                            .flex_none()
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_2))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(self.dest_name(t, r.dest)),
                    )
                    .child(depth_bar(depth, w, hue, k, t, cx))
                    .child(
                        div()
                            .w(px(38.))
                            .flex_none()
                            .font_family(MONO)
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_2))
                            .child(format!("{:+.0}%", depth * 100.0)),
                    )
                    .child(remove(k, t, "poly-move-remove", cx))
            })
            .collect();
        let none = rows.is_empty();
        div()
            .id("poly-moves")
            .flex_none()
            .w(px(266.))
            .h_full()
            .px(px(12.))
            .py(px(7.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(1.))
            .border_l_1()
            .border_color(rgb(LINE))
            .child(label("Moves").mb(px(4.)))
            .children(rows)
            .when(none, |d| {
                d.child(
                    label(format!(
                        "Nothing yet. Drag {} onto any control.",
                        source_name(s)
                    ))
                    .whitespace_normal(),
                )
            })
    }

    /// The eight macros: each a knob, its name a handle to drag onto any control, double-clicked
    /// to name it.
    pub(crate) fn poly_macros(&self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let naming = self.naming.as_ref().and_then(|n| match n.what {
            crate::workspace::Name::Macro(tt, i) if tt == t => Some((i, n.input.clone())),
            _ => None,
        });
        let cells = (0..MACROS)
            .map(|i| {
                let s = PolySource::Macro(i as u8);
                let name = match p.macros[i].is_empty() {
                    true => format!("Macro {}", i + 1),
                    false => p.macros[i].clone(),
                };
                let picked = self.poly_view.as_ref().is_some_and(|v| v.source == s);
                let field = naming.clone().filter(|n| n.0 == i).map(|n| n.1);
                let value = format!("{:.0}%", self.poly_at(t, macro_param(i)) * 100.0);
                let handle = div()
                    .id(("poly-macro-name", i))
                    .px(px(6.))
                    .rounded(px(4.))
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT))
                    .whitespace_nowrap()
                    .cursor_grab()
                    .hover(|d| d.bg(rgb(FIELD)))
                    .when(picked, |d| d.border_1().border_color(rgb(VIOLET)))
                    .debug_selector(move || format!("poly-macro-name-{i}"))
                    .child(name)
                    .on_hover(cx.listener(move |ws, over: &bool, _, cx| {
                        if let Some(v) = ws.poly_view.as_mut() {
                            v.pointed = match *over {
                                true => Some(s),
                                false => v.pointed.filter(|x| *x != s),
                            };
                        }
                        cx.notify();
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            if e.click_count == 2 {
                                ws.start_naming(crate::workspace::Name::Macro(t, i), window, cx);
                                return;
                            }
                            if let Some(v) = ws.poly_view.as_mut() {
                                v.source = s;
                                v.cord = Some(pt(e.position));
                            }
                            ws.begin_drag(
                                Drag::PolySource {
                                    track: t,
                                    source: s,
                                },
                                cx,
                            );
                        }),
                    );
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(3.))
                    .child(self.poly_knob_lit(t, macro_param(i), None, 40.0, VIOLET, cx))
                    .child(match field {
                        Some(input) => div()
                            .w(px(96.))
                            .h(px(20.))
                            .px(px(4.))
                            .rounded(px(4.))
                            .bg(rgb(FIELD))
                            .text_size(px(11.5))
                            .child(input)
                            .into_any_element(),
                        None => handle.into_any_element(),
                    })
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_2))
                            .child(value),
                    )
            })
            .collect::<Vec<_>>();
        // A synth from before patches had macros: offered them, its sound left as it is.
        let bare = p.macros.iter().all(String::is_empty)
            && !p
                .routes
                .iter()
                .any(|r| matches!(r.source, PolySource::Macro(_)));
        let give = bare.then(|| {
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(
                    button("Give it the eight macros", false)
                        .debug_selector(|| "poly-give-macros".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.poly_op(t, PolyOp::GiveMacros, cx);
                            }),
                        ),
                )
                .child(
                    label("Bright, Bite, Motion and the rest. The sound stays as it is until they're turned.")
                        .whitespace_normal(),
                )
        });
        div()
            .px(px(14.))
            .py(px(8.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(label("Macros"))
                    .child(label("Double-click a name to change it")),
            )
            .children(give)
            .child(
                div()
                    .flex_1()
                    .grid()
                    .grid_cols(4)
                    .gap_y(px(8.))
                    .children(cells),
            )
    }
}

/// How much a route moves, as a bar from the middle: dragged across to change it.
pub(crate) fn depth_bar(
    depth: f32,
    w: f32,
    hue: u32,
    route: usize,
    t: usize,
    cx: &mut Context<Workspace>,
) -> Div {
    div()
        .relative()
        .flex_none()
        .w(px(w))
        .h(px(16.))
        .rounded(px(3.))
        .bg(rgb(WELL))
        .cursor_ew_resize()
        .debug_selector(move || format!("poly-route-depth-{route}"))
        .child(
            div()
                .absolute()
                .top(px(3.))
                .bottom(px(3.))
                .left(px(w / 2.0))
                .w(px(1.))
                .bg(rgb(LINE_STRONG)),
        )
        .child(
            div()
                .absolute()
                .top(px(3.))
                .bottom(px(3.))
                .left(px(w / 2.0 + depth.min(0.0) * w / 2.0))
                .w(px((depth.abs() * w / 2.0).max(1.0)))
                .rounded(px(1.))
                .bg(rgb(hue)),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                ws.begin_drag(
                    Drag::PolyDepth {
                        track: t,
                        route,
                        x: pt(e.position).x,
                        depth,
                    },
                    cx,
                );
            }),
        )
}

/// A × that takes a route off.
pub(crate) fn remove(
    route: usize,
    t: usize,
    sel: &'static str,
    cx: &mut Context<Workspace>,
) -> Div {
    div()
        .size(px(20.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .cursor_pointer()
        .text_color(rgb(TEXT_3))
        .hover(|d| d.bg(rgb(FIELD)).text_color(rgb(TEXT)))
        .debug_selector(move || format!("{sel}-{route}"))
        .child(icon(Icon::Close, 11.).text_color(rgb(TEXT_2)))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                ws.poly_op(t, PolyOp::RemoveRoute(route), cx);
            }),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_envelopes_points_keep_to_their_thirds() {
        let [a, d, r] = env_points(1.0, 0.0, 1.0);
        assert!((a - 1.0 / 3.0).abs() < 1e-6 && (d - 1.0 / 3.0).abs() < 1e-6);
        assert!((r - 1.0).abs() < 1e-6);
        let [a, d, r] = env_points(0.0, 0.5, 0.0);
        assert!(a == 0.0 && (d - 0.5).abs() < 1e-6 && (r - 2.0 / 3.0).abs() < 1e-6);
    }
}
