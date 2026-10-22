//! The chain's cards: the EQ's curve, the compressor's and the saturator's.

use super::*;

/// The EQ graph's range: 20 Hz to 20 kHz across, ±18 dB up and down.
pub fn graph_x(freq: f32) -> f32 {
    (freq.max(20.0) / 20.0).log10() / 3.0
}

pub fn graph_y(db: f32) -> f32 {
    0.5 - db.clamp(-18.0, 18.0) / 36.0
}

impl Workspace {
    pub(crate) fn eq_card(&self, ch: Channel, eq: Eq, cx: &mut Context<Self>) -> Div {
        let rate = self.session.project.sample_rate as f64;
        let curve = strip::eq_curve(&eq, rate, 160);
        let color = self.channel_look(ch).1;
        let on = !eq.bypass;
        let graph = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                let line = |window: &mut gpui::Window, x: f32, y: f32, ww: f32, hh: f32, c: u32| {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x + px(x), b.origin.y + px(y)),
                            size(px(ww), px(hh)),
                        ),
                        rgb(c),
                    ));
                };
                for f in [50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0] {
                    line(window, (graph_x(f) * w).round(), 0.0, 1.0, h, LINE_SOFT);
                }
                for db in [12.0, 6.0, -6.0, -12.0] {
                    line(window, 0.0, (graph_y(db) * h).round(), w, 1.0, LINE_SOFT);
                }
                line(window, 0.0, (h / 2.0).round(), w, 1.0, RAISED);
                let x = |i: usize| b.origin.x + px(i as f32 * w / (curve.len() - 1) as f32);
                let y = |g: f32| b.origin.y + px((graph_y(g) * h).clamp(1.0, h - 1.0));
                let mut area = gpui::PathBuilder::fill();
                area.move_to(point(b.origin.x, b.origin.y + px(h / 2.0)));
                for (i, &g) in curve.iter().enumerate() {
                    area.line_to(point(x(i), y(g)));
                }
                area.line_to(point(b.origin.x + b.size.width, b.origin.y + px(h / 2.0)));
                area.close();
                if let Ok(p) = area.build() {
                    window.paint_path(p, alpha(color, if on { 0.14 } else { 0.05 }));
                }
                let mut stroke = gpui::PathBuilder::stroke(px(2.));
                stroke.move_to(point(x(0), y(curve[0])));
                for (i, &g) in curve.iter().enumerate().skip(1) {
                    stroke.line_to(point(x(i), y(g)));
                }
                if let Ok(p) = stroke.build() {
                    window.paint_path(
                        p,
                        if on {
                            rgb(color).into()
                        } else {
                            alpha(TEXT_3, 1.0)
                        },
                    );
                }
            },
        )
        .size_full();
        // A dot where each band is; the graph opens the EQ's window to work on them.
        let width = PANEL_W - 32.0 - 22.0;
        let height = 120.0;
        let dots = eq
            .bands
            .iter()
            .enumerate()
            .map(|(i, band)| {
                let db = match band.shape.has_gain() {
                    true => band.gain(eq.scale),
                    false => 0.0,
                };
                div()
                    .absolute()
                    .left(px(graph_x(band.freq) * width - 6.0))
                    .top(px(graph_y(db) * height - 6.0))
                    .size(px(12.))
                    .rounded(px(6.))
                    .when(!band.off, |d| d.bg(rgb(crate::mix::eq::band_color(i))))
                    .when(band.off, |d| {
                        d.border_1()
                            .border_color(alpha(crate::mix::eq::band_color(i), 0.6))
                    })
                    .debug_selector(move || format!("eq-card-band-{i}"))
            })
            .collect::<Vec<_>>();
        let labels = [
            (50.0, "50"),
            (100.0, "100"),
            (500.0, "500"),
            (1000.0, "1k"),
            (5000.0, "5k"),
            (10000.0, "10k"),
        ]
        .map(|(f, label)| {
            div()
                .absolute()
                .left(px(graph_x(f) * width - 12.0))
                .w(px(24.))
                .flex()
                .justify_center()
                .font_family(MONO)
                .text_size(px(9.))
                .text_color(rgb(TEXT_3))
                .child(label)
        });
        let n = eq.bands.len();
        let readout = match n {
            0 => "No bands yet · click to open the EQ".to_string(),
            _ => {
                let shapes: Vec<String> = eq
                    .bands
                    .iter()
                    .take(4)
                    .map(|b| match b.shape.has_gain() {
                        true => format!("{} {}", strip::hz(b.freq), db_text(b.gain_db)),
                        false => format!("{} {}", b.shape.name(), strip::hz(b.freq)),
                    })
                    .collect();
                let more = if n > 4 {
                    format!(" · +{}", n - 4)
                } else {
                    String::new()
                };
                format!("{}{more}", shapes.join(" · "))
            }
        };
        card()
            .gap(px(10.))
            .child(self.card_header("Channel EQ", ch, Builtin::Eq, on, cx))
            .child(
                div()
                    .relative()
                    .h(px(height))
                    .rounded(px(8.))
                    .bg(rgb(WELL))
                    .cursor_pointer()
                    .debug_selector(|| "eq-card".into())
                    .child(graph)
                    .children(dots)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.open_eq(ch, cx);
                        }),
                    ),
            )
            .child(div().relative().h(px(10.)).mt(px(-4.)).children(labels))
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(10.))
                    .text_color(rgb(TEXT_3))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(readout),
            )
    }

    pub(crate) fn comp_card(&self, ch: Channel, comp: Comp, cx: &mut Context<Self>) -> Div {
        let id = self.channel_id(ch);
        let reduction = self.reductions.get(&id).copied().unwrap_or(0.0);
        let color = self.channel_look(ch).1;
        let on = !comp.bypass;
        let knobs = [
            (CompField::Threshold, "Threshold"),
            (CompField::Ratio, "Ratio"),
            (CompField::Attack, "Attack"),
            (CompField::Release, "Release"),
            (CompField::Makeup, "Makeup"),
        ]
        .map(|(field, label)| {
            let target = Target::Comp(ch, field);
            let value = strip::comp_pos(field, &comp);
            div()
                .w(px(58.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.))
                .cursor_ns_resize()
                .debug_selector(move || format!("comp-{field:?}"))
                .child(self.modded(self.mod_key(target), 36., knob(value, 36., color, false)))
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(11.))
                        .font_weight(FontWeight::MEDIUM)
                        .whitespace_nowrap()
                        .child(strip::comp_text(field, &comp)),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .child(label),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        let y = pt(e.position).y;
                        ws.begin_drag(Drag::Knob { target, y, value }, cx);
                    }),
                )
        });
        let shown = (reduction / 20.0).clamp(0.0, 1.0);
        card()
            .gap(px(10.))
            .child(self.card_header("Compressor", ch, Builtin::Comp, on, cx))
            .child(
                div()
                    .h(px(16.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .child("Gain reduction"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h(px(6.))
                            .rounded(px(3.))
                            .bg(rgb(WELL))
                            .flex()
                            .justify_end()
                            .child(
                                div()
                                    .h_full()
                                    .w(gpui::relative(shown))
                                    .rounded(px(3.))
                                    .bg(rgb(METER_MID)),
                            ),
                    )
                    .child(
                        div()
                            .w(px(52.))
                            .flex()
                            .justify_end()
                            .font_family(MONO)
                            .text_size(px(11.))
                            .text_color(rgb(if reduction > 0.05 { METER_MID } else { TEXT_3 }))
                            .child(format!("{} dB", db_text(-reduction))),
                    ),
            )
            .child(div().flex().justify_between().children(knobs))
            .child(
                // What it listens to: the peaks, or an RMS average (gentler, more like the ear).
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child("Detector"),
                    )
                    .child(tray().p(px(2.)).rounded(px(7.)).children(
                        [("Peak", false), ("RMS", true)].map(|(label, rms)| {
                            seg(label, comp.rms == rms, 22.)
                                .debug_selector(move || format!("comp-detector-{label}"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        ws.edit(cx, |ws| {
                                            if let Some(c) = ws.strip_dsp_mut(ch).1 {
                                                c.rms = rms;
                                            }
                                        })
                                    }),
                                )
                        }),
                    )),
            )
    }

    /// The saturator: its curve (how hard it rounds off what comes in), and its knobs.
    pub(crate) fn sat_card(&self, ch: Channel, sat: Saturator, cx: &mut Context<Self>) -> Div {
        let color = self.channel_look(ch).1;
        let on = !sat.bypass;
        let curve = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                let line = |window: &mut gpui::Window, x: f32, y: f32, ww: f32, hh: f32| {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x + px(x), b.origin.y + px(y)),
                            size(px(ww), px(hh)),
                        ),
                        rgb(LINE_SOFT),
                    ));
                };
                line(window, 0.0, (h / 2.0).round(), w, 1.0);
                line(window, (w / 2.0).round(), 0.0, 1.0, h);
                // In from -1 to 1 across, out from -1 to 1 up: straight is clean.
                let at = |x: f32, y: f32| {
                    point(
                        b.origin.x + px((x + 1.0) / 2.0 * w),
                        b.origin.y + px((1.0 - (y.clamp(-1.0, 1.0) + 1.0) / 2.0) * h),
                    )
                };
                let mut clean = gpui::PathBuilder::stroke(px(1.));
                clean.move_to(at(-1.0, -1.0));
                clean.line_to(at(1.0, 1.0));
                if let Ok(p) = clean.build() {
                    window.paint_path(p, alpha(TEXT_4, 1.0));
                }
                let mut path = gpui::PathBuilder::stroke(px(2.));
                for i in 0..=64 {
                    let x = i as f32 / 32.0 - 1.0;
                    let y = sat_curve(&sat, x);
                    match i {
                        0 => path.move_to(at(x, y)),
                        _ => path.line_to(at(x, y)),
                    }
                }
                if let Ok(p) = path.build() {
                    let c = if on {
                        rgb(color).into()
                    } else {
                        alpha(TEXT_3, 1.0)
                    };
                    window.paint_path(p, c);
                }
            },
        )
        .size_full();
        let knobs = [
            (SatField::Drive, "Drive", false),
            (SatField::Tone, "Tone", true),
            (SatField::Mix, "Mix", false),
            (SatField::Output, "Output", true),
        ]
        .map(|(field, label, bipolar)| {
            let target = Target::Sat(ch, field);
            let value = strip::sat_pos(field, &sat);
            let shown = if bipolar { value * 2.0 - 1.0 } else { value };
            div()
                .w(px(60.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.))
                .cursor_ns_resize()
                .debug_selector(move || format!("sat-{field:?}"))
                .child(self.modded(self.mod_key(target), 36., knob(shown, 36., color, bipolar)))
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(11.))
                        .font_weight(FontWeight::MEDIUM)
                        .whitespace_nowrap()
                        .child(strip::sat_text(field, &sat)),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .child(label),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        let y = pt(e.position).y;
                        ws.begin_drag(Drag::Knob { target, y, value }, cx);
                    }),
                )
        });
        card()
            .gap(px(10.))
            .child(self.card_header("Saturator", ch, Builtin::Sat, on, cx))
            .child(
                div()
                    .flex()
                    .gap(px(10.))
                    .child(
                        div()
                            .size(px(64.))
                            .flex_none()
                            .rounded(px(8.))
                            .bg(rgb(WELL))
                            .p(px(6.))
                            .child(curve),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .justify_center()
                            .gap(px(3.))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child(format!(
                                "{} into a soft clip",
                                strip::sat_text(SatField::Drive, &sat)
                            ))
                            .child(format!(
                                "{} · {} wet",
                                strip::sat_text(SatField::Tone, &sat),
                                strip::sat_text(SatField::Mix, &sat)
                            )),
                    ),
            )
            .child(div().flex().justify_between().children(knobs))
    }
}
