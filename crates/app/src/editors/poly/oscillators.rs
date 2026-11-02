//! The oscillators' cards: each wavetable drawn in depth, its knobs round it.

use super::*;
use crate::editors::sampler::response;

const WAVE_H: f32 = 88.0;
const KNOB: f32 = 28.0;
/// How wide a filter's card is, and its picture.
const FILTER_W: f32 = 404.0;
const CURVE_W: f32 = FILTER_W - 18.0;
/// A wavetable in depth: its frames one behind another, the one it's on picked out.
fn wave_view(name: &str, position: f32, color: u32) -> impl IntoElement + use<> {
    let t = table(name);
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let n = t.frames;
            let shown = n.min(24);
            let at = position.clamp(0.0, 1.0) * (n - 1) as f32;
            let frame = |f: usize, skew: f32, y0: f32, tall: f32, xs: f32| {
                let x = t.frame(f);
                let step = x.len() / 96;
                (0..96).map(move |i| {
                    let v = x[i * step];
                    let u = i as f32 / 95.0;
                    (u * xs + skew, y0 - v * tall)
                })
            };
            for k in (0..shown).rev() {
                let f = k * (n - 1) / (shown - 1).max(1);
                let depth = k as f32 / (shown - 1).max(1) as f32;
                let near = 1.0 - (f as f32 - at).abs() / n as f32 * 3.0;
                polyline(
                    window,
                    b,
                    frame(f, depth * 0.16, 0.66 - depth * 0.3, 0.26, 0.82),
                    1.0,
                    alpha(color, 0.1 + 0.3 * near.max(0.0)),
                );
            }
            // Where it is now, in front, whole.
            let f = at.round() as usize;
            let depth = at / (n - 1).max(1) as f32;
            polyline(
                window,
                b,
                frame(f, depth * 0.16, 0.66 - depth * 0.3, 0.26, 0.82),
                1.8,
                alpha(color, 0.98),
            );
        },
    )
    .absolute()
    .size_full()
}

impl Workspace {
    pub(crate) fn poly_osc_card(
        &self,
        t: usize,
        p: &Poly,
        o: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let color = OSC_COLORS[o];
        let f = |x: OscField| osc_param(o, x);
        let k = |x: OscField, name: &'static str, size: f32, cx: &mut Context<Self>| {
            self.poly_knob(t, f(x), name, size, color, cx)
        };
        let name = p.tables[o].clone();
        let tables = self.poly_menu(
            format!("poly-table-{o}"),
            name.clone(),
            move |_| {
                TABLES
                    .iter()
                    .map(|&(n, _)| (n.to_string(), MenuAction::Poly(t, PolyOp::Table(o, n))))
                    .collect()
            },
            true,
            cx,
        );
        let warp = |k_: usize, cx: &mut Context<Self>| {
            let now = p.warps[o][k_];
            self.poly_menu(
                format!("poly-warp-{o}-{k_}"),
                now.name().to_string(),
                move |_| {
                    Warp::ALL
                        .iter()
                        .map(|&w| {
                            (
                                w.name().to_string(),
                                MenuAction::Poly(t, PolyOp::Warp(o, k_, w)),
                            )
                        })
                        .collect()
                },
                false,
                cx,
            )
        };
        let position = self.target(Target::Poly {
            track: t,
            param: f(OscField::Position),
        });
        let letter = ["A", "B", "C"][o];
        div()
            .w(px(330.))
            .p(px(8.))
            .flex()
            .flex_col()
            .gap(px(5.))
            .rounded(px(9.))
            .bg(rgb(0x14151a))
            .border_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .size(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .bg(alpha(color, 0.16))
                            .text_color(rgb(color))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(letter),
                    )
                    .child(div().flex_1().child(tables)),
            )
            .child(
                picture()
                    .h(px(WAVE_H))
                    .debug_selector(move || format!("poly-wave-{o}"))
                    .child(wave_view(&name, position, color)),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(k(OscField::Position, "Position", KNOB, cx))
                    .child(k(OscField::Level, "Level", KNOB, cx))
                    .child(k(OscField::Pan, "Pan", KNOB, cx))
                    .child(k(OscField::Octave, "Oct", KNOB, cx))
                    .child(k(OscField::Semi, "Semi", KNOB, cx))
                    .child(k(OscField::Fine, "Fine", KNOB, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().w(px(92.)).child(warp(0, cx)))
                    .child(k(OscField::WarpA, "Warp 1", KNOB, cx))
                    .child(div().w(px(92.)).child(warp(1, cx)))
                    .child(k(OscField::WarpB, "Warp 2", KNOB, cx)),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(k(OscField::Unison, "Unison", KNOB, cx))
                    .child(k(OscField::Detune, "Detune", KNOB, cx))
                    .child(k(OscField::Blend, "Blend", KNOB, cx))
                    .child(k(OscField::Width, "Width", KNOB, cx))
                    .child(k(OscField::Random, "Random", KNOB, cx)),
            )
    }

    pub(crate) fn sub_noise_card(&self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        const WAVES: [&str; 4] = ["Sine", "Tri", "Saw", "Square"];
        let now = p.sub_wave as usize;
        let octave = p.sub_octave;
        let octaves = tray()
            .p(px(2.))
            .rounded(px(7.))
            .children((1..=2u8).map(|o| {
                seg(["-1", "-2"][o as usize - 1], o == octave, 20.)
                    .debug_selector(move || format!("poly-sub-octave-{o}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit(cx, |ws| {
                                if let Some(p) = ws.poly_mut(t) {
                                    p.sub_octave = o;
                                }
                            });
                        }),
                    )
            }));
        let waves = tray().p(px(2.)).rounded(px(7.)).children((0..4u8).map(|w| {
            seg(WAVES[w as usize], w as usize == now, 20.)
                .debug_selector(move || format!("poly-sub-{w}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.edit(cx, |ws| {
                            if let Some(p) = ws.poly_mut(t) {
                                p.sub_wave = w;
                            }
                        });
                    }),
                )
        }));
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                panel("Sub", TEXT_2)
                    .flex_1()
                    .child(waves)
                    .child(octaves)
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .child(self.poly_knob(t, SUB_LEVEL, "Level", KNOB, TEXT_2, cx))
                            .child(self.poly_knob(t, SUB_PAN, "Pan", KNOB, TEXT_2, cx)),
                    ),
            )
            .child(
                panel("Noise", TEXT_2).flex_1().child(
                    div()
                        .flex()
                        .justify_between()
                        .child(self.poly_knob(t, NOISE_LEVEL, "Level", KNOB, TEXT_2, cx))
                        .child(self.poly_knob(t, NOISE_COLOR, "Color", KNOB, TEXT_2, cx)),
                ),
            )
    }

    pub(crate) fn filter_card(&self, t: usize, p: &Poly, i: usize, cx: &mut Context<Self>) -> Div {
        let f = move |x: FilterField| filter_param(i, x);
        let k = |x: FilterField, name: &'static str, cx: &mut Context<Self>| {
            self.poly_knob(t, f(x), name, KNOB, FILTER_COLOR, cx)
        };
        let now = p.filters[i];
        let kinds = self.poly_menu(
            format!("poly-filter-{i}"),
            now.name().to_string(),
            move |_| {
                PolyFilter::ALL
                    .iter()
                    .map(|&k| {
                        (
                            k.name().to_string(),
                            MenuAction::Poly(t, PolyOp::Filter(i, k)),
                        )
                    })
                    .collect()
            },
            false,
            cx,
        );
        let routing = [
            ("Series", FilterRouting::Series),
            ("Parallel", FilterRouting::Parallel),
            ("Stereo", FilterRouting::Stereo),
        ];
        let now_routing = p.routing;
        let routes = (i == 0).then(|| {
            self.poly_menu(
                "poly-routing".into(),
                routing
                    .iter()
                    .find(|r| r.1 == now_routing)
                    .map_or("Series", |r| r.0)
                    .to_string(),
                move |_| {
                    routing
                        .iter()
                        .map(|&(n, r)| (n.to_string(), MenuAction::Poly(t, PolyOp::Routing(r))))
                        .collect()
                },
                false,
                cx,
            )
        });
        // Which sounds go through it.
        let feeds = ["A", "B", "C", "S", "N"];
        let feed = div().flex().gap(px(3.)).children((0..OSCS + 2).map(|s| {
            let on = p.to_filter[s][i];
            div()
                .size(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .text_size(px(10.5))
                .cursor_pointer()
                .bg(rgb(if on { 0x3a2433 } else { 0x191a20 }))
                .text_color(rgb(if on { FILTER_COLOR } else { TEXT_4 }))
                .debug_selector(move || format!("poly-feed-{i}-{s}"))
                .child(feeds[s])
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.edit(cx, |ws| {
                            if let Some(p) = ws.poly_mut(t) {
                                p.to_filter[s][i] ^= true;
                            }
                        });
                    }),
                )
        }));
        let (cut, res) = (
            self.poly_at(t, f(FilterField::Cutoff)),
            self.poly_at(t, f(FilterField::Resonance)),
        );
        let (w, h) = (CURVE_W, 52.0);
        let plot = self.poly_view.as_ref().map(|v| v.filters[i].clone());
        let mode = match now {
            PolyFilter::High12 | PolyFilter::High24 => FilterMode::HighPass,
            PolyFilter::Band12 | PolyFilter::Band24 | PolyFilter::Peak => FilterMode::BandPass,
            PolyFilter::Notch => FilterMode::Notch,
            PolyFilter::CombUp | PolyFilter::CombDown => FilterMode::Comb,
            PolyFilter::Formant => FilterMode::Formant,
            PolyFilter::Ladder | PolyFilter::Diode | PolyFilter::Cascade => FilterMode::Ladder,
            _ => FilterMode::LowPass,
        };
        let live = now != PolyFilter::None;
        let curve = canvas(
            move |b, _, _| {
                if let Some(p) = &plot {
                    p.set(b);
                }
            },
            move |b, _, window, _| {
                if !live {
                    return;
                }
                let hz = POLY_PARAMS[f(FilterField::Cutoff)].scale.value(cut);
                let points = (0..120).map(|k| {
                    let x = k as f32 / 119.0;
                    let at = 20.0 * 1000f32.powf(x);
                    let db = response(mode, hz, res, at);
                    (x, (0.3 - db / 60.0).clamp(0.0, 1.0))
                });
                polyline(window, b, points, 1.8, alpha(FILTER_COLOR, 0.95));
            },
        )
        .absolute()
        .size_full();
        div()
            .w(px(FILTER_W))
            .p(px(8.))
            .flex()
            .flex_col()
            .gap(px(5.))
            .rounded(px(9.))
            .bg(rgb(0x14151a))
            .border_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(FILTER_COLOR))
                            .whitespace_nowrap()
                            .child(["Filter 1", "Filter 2"][i]),
                    )
                    .child(div().w(px(110.)).child(kinds))
                    .child(feed)
                    .children(routes),
            )
            .child(
                picture()
                    .w(px(w))
                    .h(px(h))
                    .debug_selector(move || format!("poly-filter-curve-{i}"))
                    .child(curve)
                    .when(live, |d| {
                        d.child(point_handle(
                            cut * w,
                            (1.0 - res) * h,
                            FILTER_COLOR,
                            i as u8,
                            ["poly-filter-point-0", "poly-filter-point-1"][i],
                            cx,
                        ))
                    }),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(k(FilterField::Cutoff, "Cutoff", cx))
                    .child(k(FilterField::Resonance, "Res", cx))
                    .child(k(FilterField::Drive, "Drive", cx))
                    .child(k(FilterField::Bass, "Bass", cx))
                    .child(k(FilterField::KeyTrack, "Keys", cx))
                    .child(k(FilterField::Spread, "Spread", cx))
                    .child(k(FilterField::Mix, "Mix", cx)),
            )
    }

    /// A parameter's knob position as it plays.
    pub(crate) fn poly_at(&self, t: usize, i: usize) -> f32 {
        self.target(Target::Poly { track: t, param: i })
    }
}
