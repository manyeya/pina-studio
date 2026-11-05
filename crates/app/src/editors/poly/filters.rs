//! The filters: both on one curve over what's playing, dragged by their point; a tab each for its
//! settings, and how the two are wired.

use super::*;
use crate::editors::sampler::response;

/// How the sampler's drawing of a filter stands in for each of the synth's.
fn mode_of(f: PolyFilter) -> FilterMode {
    match f {
        PolyFilter::High12 | PolyFilter::High24 => FilterMode::HighPass,
        PolyFilter::Band12 | PolyFilter::Band24 | PolyFilter::Peak => FilterMode::BandPass,
        PolyFilter::Notch => FilterMode::Notch,
        PolyFilter::CombUp | PolyFilter::CombDown => FilterMode::Comb,
        PolyFilter::Formant => FilterMode::Formant,
        PolyFilter::Ladder | PolyFilter::Diode | PolyFilter::Cascade => FilterMode::Ladder,
        _ => FilterMode::LowPass,
    }
}

/// Where a level (dB) sits on the curve's picture, 0 at the top.
fn db_y(db: f32) -> f32 {
    (0.42 - db.clamp(-42.0, 26.0) / 48.0).clamp(0.0, 1.0)
}

impl Workspace {
    pub(crate) fn poly_filters(&self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let tab = self.poly_view.as_ref().map_or(0, |v| v.filter);
        let plot = self.poly_view.as_ref().map(|v| v.curve.clone());
        let spectrum = self
            .poly_view
            .as_ref()
            .map(|v| v.spectrum.clone())
            .unwrap_or_default();
        let shapes: [(bool, FilterMode, f32, f32); FILTERS] = std::array::from_fn(|i| {
            let cut = self.poly_moving(t, filter_param(i, FilterField::Cutoff));
            let res = self.poly_moving(t, filter_param(i, FilterField::Resonance));
            let hz = POLY_PARAMS[filter_param(i, FilterField::Cutoff)]
                .scale
                .value(cut);
            (
                p.filters[i] != PolyFilter::None,
                mode_of(p.filters[i]),
                hz,
                res,
            )
        });
        let set_at = self.poly_at(t, filter_param(tab, FilterField::Cutoff));
        let curve = canvas(
            move |b, _, _| {
                if let Some(p) = &plot {
                    p.set(b);
                }
            },
            move |b, _, window, _| {
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                for hz in [100.0f32, 1000.0, 10000.0] {
                    let x = (hz / 20.0).log10() / 3.0;
                    window.paint_quad(gpui::fill(
                        Bounds::new(
                            point(b.origin.x + px(x * w), b.origin.y),
                            gpui::size(px(1.), b.size.height),
                        ),
                        alpha(TEXT, 0.05),
                    ));
                }
                if !spectrum.is_empty() {
                    let n = spectrum.len();
                    let points: Vec<(f32, f32)> = spectrum
                        .iter()
                        .enumerate()
                        .map(|(k, db)| {
                            let y = 1.0 - ((db + 90.0) / 80.0).clamp(0.0, 1.0) * 0.8;
                            (k as f32 / (n - 1) as f32, y)
                        })
                        .collect();
                    filled(window, b, &points, 1.0, alpha(TEXT, 0.06));
                }
                let line = |i: usize| {
                    let (live, mode, hz, res) = shapes[i];
                    (0..160).map(move |k| {
                        let x = k as f32 / 159.0;
                        let db = match live {
                            true => response(mode, hz, res, 20.0 * 1000f32.powf(x)),
                            false => 0.0,
                        };
                        (x, db_y(db))
                    })
                };
                let other = 1 - tab;
                polyline(window, b, line(other), 1.0, alpha(TEXT_3, 0.8));
                polyline(window, b, line(tab), 2.0, rgb(TEXT).into());
                let (live, mode, hz, res) = shapes[tab];
                if live {
                    let y = db_y(response(mode, hz, res, hz));
                    handle(window, b, set_at * w, y * h, TEXT);
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        let tabs = div().flex().gap(px(3.)).children((0..FILTERS).map(|i| {
            let on = i == tab;
            div()
                .flex_1()
                .px(px(9.))
                .py(px(5.))
                .rounded(px(6.))
                .cursor_pointer()
                .bg(rgb(if on { RAISED } else { WELL }))
                .text_color(rgb(if on { TEXT } else { TEXT_3 }))
                .debug_selector(move || format!("poly-filter-tab-{i}"))
                .child(format!("Filter {}", i + 1))
                .child(label(p.filters[i].name()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(v) = ws.poly_view.as_mut() {
                            v.filter = i;
                        }
                        cx.notify();
                    }),
                )
        }));
        let routing = self.poly_choices(
            t,
            "poly-routing",
            p.routing,
            &[
                ("Series", FilterRouting::Series),
                ("Parallel", FilterRouting::Parallel),
                ("Stereo", FilterRouting::Stereo),
            ],
            |p, r| p.routing = r,
            cx,
        );
        let f = move |x: FilterField| filter_param(tab, x);
        let kinds = self.poly_menu(
            format!("poly-filter-{tab}"),
            p.filters[tab].name().to_string(),
            move |_| {
                PolyFilter::ALL
                    .iter()
                    .map(|&k| {
                        (
                            k.name().to_string(),
                            MenuAction::Poly(t, PolyOp::Filter(tab, k)),
                        )
                    })
                    .collect()
            },
            false,
            cx,
        );
        let body = match p.filters[tab] != PolyFilter::None {
            true => div()
                .flex_1()
                .flex()
                .flex_col()
                .justify_center()
                .gap(px(12.))
                .px(px(12.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(self.poly_knob(t, f(FilterField::Cutoff), "Cutoff", 72.0, cx))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .flex()
                                .flex_col()
                                .gap(px(10.))
                                .child(kinds)
                                .child(
                                    div()
                                        .flex()
                                        .justify_around()
                                        .child(self.poly_knob(
                                            t,
                                            f(FilterField::Resonance),
                                            "Res",
                                            40.0,
                                            cx,
                                        ))
                                        .child(self.poly_knob(
                                            t,
                                            f(FilterField::Drive),
                                            "Drive",
                                            40.0,
                                            cx,
                                        )),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .child(self.poly_knob(t, f(FilterField::Bass), "Bass", 30.0, cx))
                        .child(self.poly_knob(t, f(FilterField::KeyTrack), "Key", 30.0, cx))
                        .child(self.poly_knob(t, f(FilterField::Spread), "Spread", 30.0, cx))
                        .child(self.poly_knob(t, f(FilterField::Mix), "Mix", 30.0, cx)),
                ),
            false => div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.))
                .px(px(24.))
                .child(
                    label("This filter is off, so sound passes through it untouched.")
                        .whitespace_normal()
                        .text_center(),
                )
                .child(
                    button("Turn it on", false)
                        .debug_selector(move || format!("poly-filter-on-{tab}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.poly_op(t, PolyOp::Filter(tab, PolyFilter::Low24), cx);
                            }),
                        ),
                ),
        };
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .h(px(36.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(title("Filters"))
                    .child(routing),
            )
            .child(
                picture()
                    .flex_none()
                    .h(px(170.))
                    .mx(px(12.))
                    .cursor_crosshair()
                    .debug_selector(|| "poly-filter-curve".into())
                    .child(curve)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let id = tab as u8;
                            ws.begin_drag(Drag::PolyPoint { id }, cx);
                            ws.drag_poly_point(id, pt(e.position));
                            cx.notify();
                        }),
                    ),
            )
            .child(div().flex_none().px(px(12.)).pt(px(8.)).child(tabs))
            .child(body)
    }
}
