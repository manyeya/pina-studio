//! The sources: a row each for the three oscillators, the sub and the noise, with their level and
//! which filters they go to; and the one picked, open in the middle, its wavetable in depth.

use super::*;
use pina_engine::dsp::poly::{
    drawn_wave,
    table::{TABLES, table},
};

const SUBS: [&str; 4] = ["Sine", "Triangle", "Saw", "Square"];
/// Points across a small wave, and across the one picked.
const SMALL: usize = 64;
const LARGE: usize = 192;

/// A source's level parameter.
fn level_of(s: usize) -> usize {
    match s {
        0..=2 => osc_param(s, OscField::Level),
        3 => SUB_LEVEL,
        _ => NOISE_LEVEL,
    }
}

/// The sub's wave at `p` (0 to 1).
fn sub_at(wave: u8, p: f32) -> f32 {
    use std::f32::consts::TAU;
    match wave {
        0 => (TAU * p).sin(),
        1 => 1.0 - 4.0 * ((p + 0.25).fract() - 0.5).abs(),
        2 => 2.0 * p - 1.0,
        _ => {
            if p < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
    }
}

/// Noise as drawn: the same every time, rougher where it's brighter.
fn noise_at(i: usize, color: f32) -> f32 {
    let x = ((i as f32 * 12.9898).sin() * 43758.547).fract() * 2.0 - 1.0;
    let y = ((i as f32 * 4.1414 + 1.0).sin() * 9631.17).fract() * 2.0 - 1.0;
    let smooth = (1.0 - color) / 2.0;
    (x * (1.0 - smooth) + y * smooth) * 0.9
}

/// A source's wave as it plays: `n` points, -1 to 1.
fn wave_of(p: &Poly, s: usize, position: f32, amounts: [f32; 2], n: usize) -> Vec<f32> {
    match s {
        0..=2 => drawn_wave(&table(&p.tables[s]), position, p.warps[s], amounts, s, n),
        3 => (0..n)
            .map(|i| sub_at(p.sub_wave, i as f32 / n as f32))
            .collect(),
        _ => {
            let color = p.params[NOISE_COLOR];
            (0..n).map(|i| noise_at(i, color)).collect()
        }
    }
}

/// A wave drawn small, white while it sounds.
fn small_wave(wave: Vec<f32>, sounding: bool) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let n = wave.len().max(2);
            let points = wave
                .iter()
                .enumerate()
                .map(|(i, v)| (i as f32 / (n - 1) as f32, 0.5 - v * 0.42));
            let color = if sounding { TEXT } else { TEXT_3 };
            polyline(window, b, points, 1.25, rgb(color).into());
        },
    )
    .size_full()
}

/// A table's frames as the wave view places them, one behind another: worked out again only when
/// the table, its warps or how far they're turned change.
type Placed = std::rc::Rc<Vec<Vec<(f32, f32)>>>;

fn placed_frames(name: &str, warps: [Warp; 2], amounts: [f32; 2], osc: usize) -> Placed {
    thread_local! {
        static KEPT: std::cell::RefCell<Vec<(String, Placed)>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    let key = format!("{name}|{osc}|{warps:?}|{:.3}|{:.3}", amounts[0], amounts[1]);
    if let Some(p) = KEPT.with(|k| k.borrow().iter().find(|e| e.0 == key).map(|e| e.1.clone())) {
        return p;
    }
    let t = table(name);
    let shown = t.frames.min(24);
    let frames: Vec<Vec<(f32, f32)>> = (0..shown)
        .map(|k| {
            let depth = k as f32 / (shown - 1).max(1) as f32;
            let wave = drawn_wave(&t, depth, warps, amounts, osc, 96);
            let n = wave.len();
            wave.iter()
                .enumerate()
                .map(|(i, v)| place(depth, i as f32 / (n - 1) as f32, *v))
                .collect()
        })
        .collect();
    let p = std::rc::Rc::new(frames);
    KEPT.with(|k| {
        let mut k = k.borrow_mut();
        if k.len() >= 8 {
            k.remove(0);
        }
        k.push((key, p.clone()));
    });
    p
}

/// Where a point of a frame `depth` back sits in the wave view, 0 to 1 each way.
fn place(depth: f32, u: f32, v: f32) -> (f32, f32) {
    let scale = 1.0 - depth * 0.1;
    (
        0.07 + depth * 0.2 + u * 0.68 * scale,
        0.7 - depth * 0.4 - v * 0.2 * scale,
    )
}

/// A wavetable in depth: its frames one behind another, each hiding what's behind it, the one
/// playing whole in front.
fn wave_view(
    name: &str,
    position: f32,
    warps: [Warp; 2],
    amounts: [f32; 2],
    osc: usize,
) -> impl IntoElement + use<> {
    let t = table(name);
    let frames = placed_frames(name, warps, amounts, osc);
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let shown = frames.len();
            for (k, points) in frames.iter().enumerate().rev() {
                let depth = k as f32 / (shown - 1).max(1) as f32;
                filled(window, b, points, 0.7 - depth * 0.4, alpha(WELL, 0.86));
                let near = (1.0 - (depth - position).abs() * 3.5).max(0.0);
                polyline(
                    window,
                    b,
                    points.iter().copied(),
                    1.0,
                    alpha(TEXT, 0.13 + 0.3 * near),
                );
            }
            let wave = drawn_wave(&t, position, warps, amounts, osc, LARGE);
            let n = wave.len();
            polyline(
                window,
                b,
                wave.iter()
                    .enumerate()
                    .map(|(i, v)| place(position, i as f32 / (n - 1) as f32, *v)),
                2.0,
                rgb(TEXT).into(),
            );
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

impl Workspace {
    /// The left column: each source a row, picked to open in the middle.
    pub(crate) fn poly_sources(&self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let picked = self.poly_view.as_ref().map_or(0, |v| v.picked);
        let color = p.params[NOISE_COLOR];
        let names = [
            p.tables[0].clone(),
            p.tables[1].clone(),
            p.tables[2].clone(),
            format!(
                "{} −{} oct",
                SUBS[(p.sub_wave as usize).min(3)],
                p.sub_octave
            ),
            match color {
                c if c < -0.15 => "Dark".to_string(),
                c if c > 0.15 => "Bright".to_string(),
                _ => "White".to_string(),
            },
        ];
        let rows = (0..OSCS + 2).map(|s| {
            let on = s == picked;
            let sounding = p.params[level_of(s)] > 0.0;
            let wave = match s {
                0..=2 => wave_of(
                    p,
                    s,
                    self.poly_moving(t, osc_param(s, OscField::Position)),
                    [
                        POLY_PARAMS[osc_param(s, OscField::WarpA)]
                            .scale
                            .value(self.poly_moving(t, osc_param(s, OscField::WarpA))),
                        POLY_PARAMS[osc_param(s, OscField::WarpB)]
                            .scale
                            .value(self.poly_moving(t, osc_param(s, OscField::WarpB))),
                    ],
                    SMALL,
                ),
                _ => wave_of(p, s, 0.0, [0.0; 2], SMALL),
            };
            let pin = |f: usize, cx: &mut Context<Self>| {
                let lit = p.to_filter[s][f];
                div()
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .font_family(MONO)
                    .text_size(px(11.))
                    .cursor_pointer()
                    .bg(rgb(if lit { TEXT_2 } else { WELL }))
                    .text_color(rgb(if lit { BG } else { TEXT_3 }))
                    .hover(|d| d.text_color(rgb(if lit { BG } else { TEXT })))
                    .debug_selector(move || format!("poly-feed-{f}-{s}"))
                    .child(format!("{}", f + 1))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit(cx, |ws| {
                                if let Some(p) = ws.poly_mut(t) {
                                    p.to_filter[s][f] ^= true;
                                }
                            });
                        }),
                    )
            };
            div()
                .flex_1()
                .min_h(px(54.))
                .flex()
                .items_center()
                .gap(px(10.))
                .pl(px(14.))
                .pr(px(10.))
                .border_b_1()
                .border_color(rgb(LINE_SOFT))
                .cursor_pointer()
                .when(on, |d| d.bg(rgb(CARD)))
                .debug_selector(move || format!("poly-pick-{s}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        if let Some(v) = ws.poly_view.as_mut() {
                            v.picked = s;
                        }
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .w(px(16.))
                        .flex_none()
                        .font_family(MONO)
                        .text_size(px(17.))
                        .text_color(rgb(if sounding { TEXT } else { TEXT_3 }))
                        .child(["A", "B", "C", "S", "N"][s]),
                )
                .child(
                    div()
                        .w(px(60.))
                        .h(px(32.))
                        .flex_none()
                        .child(small_wave(wave, sounding)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(if sounding { TEXT } else { TEXT_3 }))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(names[s].clone()),
                        )
                        .child(self.poly_slider(t, level_of(s), 14.0, cx)),
                )
                .child(div().flex().gap(px(3.)).child(pin(0, cx)).child(pin(1, cx)))
        });
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .h(px(36.))
                    .px(px(14.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(rgb(LINE_SOFT))
                    .child(title("Sources"))
                    .child(label("To filter")),
            )
            .children(rows)
    }

    /// The source picked, open in the middle.
    pub(crate) fn poly_editor(&self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let picked = self.poly_view.as_ref().map_or(0, |v| v.picked);
        match picked {
            0..=2 => self.osc_editor(t, p, picked, cx),
            3 => self.sub_editor(t, p, cx),
            _ => self.noise_editor(t, p, cx),
        }
    }

    fn osc_editor(&self, t: usize, p: &Poly, o: usize, cx: &mut Context<Self>) -> Div {
        let f = |x: OscField| osc_param(o, x);
        let open_tables = cx.listener(move |ws, _: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            if let Some(v) = ws.poly_view.as_mut() {
                v.tables = Some(o);
            }
            cx.notify();
        });
        let warp = |k: usize, cx: &mut Context<Self>| {
            let now = p.warps[o][k];
            let amount = f(if k == 0 {
                OscField::WarpA
            } else {
                OscField::WarpB
            });
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(label(format!("Warp {}", k + 1)))
                .child(div().w(px(112.)).child(self.poly_menu(
                    format!("poly-warp-{o}-{k}"),
                    now.name().to_string(),
                    move |_| {
                        Warp::ALL
                            .iter()
                            .map(|&w| {
                                (
                                    w.name().to_string(),
                                    MenuAction::Poly(t, PolyOp::Warp(o, k, w)),
                                )
                            })
                            .collect()
                    },
                    false,
                    cx,
                )))
                .child(self.poly_knob_lit(t, amount, None, 26.0, TEXT, cx))
        };
        let position = self.poly_moving(t, f(OscField::Position));
        let amounts = [OscField::WarpA, OscField::WarpB]
            .map(|w| POLY_PARAMS[f(w)].scale.value(self.poly_moving(t, f(w))));
        let about = TABLES
            .iter()
            .find(|x| x.0 == p.tables[o])
            .map_or("", |x| x.1);
        let frame = (self.poly_at(t, f(OscField::Position)) * 31.0).round() as usize + 1;
        let cluster = |name: &'static str, knobs: Vec<Div>| {
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(label(name))
                .child(div().flex().gap(px(2.)).children(knobs))
        };
        let k = |x: OscField, name: &'static str, cx: &mut Context<Self>| {
            self.poly_knob(t, f(x), name, 30.0, cx)
        };
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .h(px(50.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .child(
                        dropdown(p.tables[o].clone(), 34.)
                            .min_w(px(150.))
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .bg(rgb(FIELD))
                            .border_color(rgb(FIELD))
                            .debug_selector(move || format!("poly-table-{o}"))
                            .on_mouse_down(MouseButton::Left, open_tables),
                    )
                    .child(div().flex_1())
                    .child(warp(0, cx))
                    .child(warp(1, cx)),
            )
            .child(
                picture()
                    .flex_1()
                    .min_h(px(120.))
                    .mx(px(12.))
                    .debug_selector(move || format!("poly-wave-{o}"))
                    .child(wave_view(&p.tables[o], position, p.warps[o], amounts, o))
                    .child(label(about).absolute().top(px(9.)).left(px(12.)))
                    .child(
                        div()
                            .absolute()
                            .top(px(9.))
                            .right(px(12.))
                            .font_family(MONO)
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_2))
                            .child(format!("Frame {frame} of 32")),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(12.))
                    .pt(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(label("Position").w(px(58.)))
                    .child(div().flex_1().min_w(px(0.)).child(self.poly_slider(
                        t,
                        f(OscField::Position),
                        22.0,
                        cx,
                    ))),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(12.))
                    .py(px(10.))
                    .flex()
                    .justify_between()
                    .child(cluster(
                        "Pitch",
                        vec![
                            k(OscField::Octave, "Octave", cx),
                            k(OscField::Semi, "Semi", cx),
                            k(OscField::Fine, "Fine", cx),
                        ],
                    ))
                    .child(cluster(
                        "Unison",
                        vec![
                            k(OscField::Unison, "Voices", cx),
                            k(OscField::Detune, "Detune", cx),
                            k(OscField::Blend, "Blend", cx),
                            k(OscField::Width, "Width", cx),
                        ],
                    ))
                    .child(cluster(
                        "Start",
                        vec![
                            k(OscField::Phase, "Phase", cx),
                            k(OscField::Random, "Random", cx),
                        ],
                    ))
                    .child(cluster(
                        "Out",
                        vec![k(OscField::Pan, "Pan", cx), k(OscField::Level, "Level", cx)],
                    )),
            )
    }

    fn sub_editor(&self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let waves: Vec<(&'static str, u8)> = (0..4u8).map(|w| (SUBS[w as usize], w)).collect();
        let wave = wave_of(p, 3, 0.0, [0.0; 2], LARGE);
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .h(px(50.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(title("Sub"))
                    .child(label("A plain wave under the oscillators, for weight")),
            )
            .child(
                picture()
                    .flex_1()
                    .min_h(px(120.))
                    .mx(px(12.))
                    .child(small_wave(wave, true)),
            )
            .child(
                div()
                    .flex_none()
                    .p(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(20.))
                    .child(self.poly_choices(
                        t,
                        "poly-sub",
                        p.sub_wave,
                        &waves,
                        |p, w| p.sub_wave = w,
                        cx,
                    ))
                    .child(self.poly_choices(
                        t,
                        "poly-sub-octave",
                        p.sub_octave,
                        &[("−1 oct", 1u8), ("−2 oct", 2u8)],
                        |p, o| p.sub_octave = o,
                        cx,
                    ))
                    .child(self.poly_knob(t, SUB_LEVEL, "Level", 40.0, cx))
                    .child(self.poly_knob(t, SUB_PAN, "Pan", 40.0, cx)),
            )
    }

    fn noise_editor(&self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let wave = wave_of(p, 4, 0.0, [0.0; 2], LARGE);
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .h(px(50.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(title("Noise"))
                    .child(label("Air and grit, dark to bright")),
            )
            .child(
                picture()
                    .flex_1()
                    .min_h(px(120.))
                    .mx(px(12.))
                    .child(small_wave(wave, true)),
            )
            .child(
                div()
                    .flex_none()
                    .p(px(12.))
                    .flex()
                    .gap(px(12.))
                    .child(self.poly_knob(t, NOISE_LEVEL, "Level", 40.0, cx))
                    .child(self.poly_knob(t, NOISE_PAN, "Pan", 40.0, cx))
                    .child(self.poly_knob(t, NOISE_COLOR, "Colour", 40.0, cx)),
            )
    }

    /// Every wavetable, each drawn, to pick one for oscillator `o`; narrowed down by typing.
    pub(crate) fn poly_tables(
        &self,
        t: usize,
        p: &Poly,
        o: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (q, find) = self
            .poly_view
            .as_ref()
            .map(|v| (v.table_query.clone(), v.table_find.clone()))
            .unwrap_or_default();
        let close = cx.listener(|ws, _: &MouseDownEvent, _, cx| {
            if let Some(v) = ws.poly_view.as_mut() {
                v.tables = None;
            }
            cx.notify();
        });
        let cells = TABLES
            .iter()
            .filter(|(name, about)| {
                q.is_empty()
                    || name.to_lowercase().contains(&q)
                    || about.to_lowercase().contains(&q)
            })
            .map(|&(name, about)| {
                let on = p.tables[o] == name;
                let t_ = table(name);
                let preview = canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        let last = t_.frames - 1;
                        for (f, a) in [(last / 4, 0.25), (last * 2 / 3, 0.45), (last, 1.0)] {
                            let x = t_.frame(f);
                            let step = (x.len() / 96).max(1);
                            polyline(
                                window,
                                b,
                                (0..96).map(|i| (i as f32 / 95.0, 0.5 - x[i * step] * 0.42)),
                                1.0,
                                alpha(TEXT, a),
                            );
                        }
                    },
                )
                .h(px(38.))
                .w_full();
                div()
                    .p(px(7.))
                    .rounded(px(6.))
                    .bg(rgb(WELL))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(CARD)))
                    .when(on, |d| d.border_1().border_color(rgb(TEXT)))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .debug_selector(move || format!("poly-table-pick-{name}"))
                    .child(preview)
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(name),
                    )
                    .child(
                        label(about)
                            .h(px(30.))
                            .line_height(px(15.))
                            .whitespace_normal()
                            .overflow_hidden(),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(v) = ws.poly_view.as_mut() {
                                v.tables = None;
                            }
                            ws.poly_op(t, PolyOp::Table(o, name), cx);
                        }),
                    )
            })
            .collect::<Vec<_>>();
        let none = cells.is_empty();
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_mouse_down(MouseButton::Left, close),
            )
            .child(
                div()
                    .id("poly-tables")
                    .absolute()
                    .top(px(HEAD_H + 46.0))
                    .left(px(SOURCES_W + 12.0))
                    .w(px(780.))
                    .max_h(px(520.))
                    .overflow_y_scroll()
                    .p(px(8.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .rounded(px(9.))
                    .bg(rgb(RAISED))
                    .border_1()
                    .border_color(rgb(LINE_STRONG))
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(crate::pages::genres::field_box(find, "poly-table-search"))
                    .child(div().grid().grid_cols(6).gap(px(5.)).children(cells))
                    .when(none, |d| d.child(label("No table matches."))),
            )
    }
}
