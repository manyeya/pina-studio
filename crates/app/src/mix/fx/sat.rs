//! The saturator's window: its bands across the channel's sound (click one to work on it,
//! double-click to split one there, drag a split's handle to move it); the band picked's style,
//! curve and knobs under them; how finely it runs, how steep its splits are, its mix and output
//! along the bottom.

use pina_engine::{
    dsp::sat::static_curve,
    song::project::{MAX_SAT_BANDS, SatBand},
};

use super::comp::side_row;
use super::*;

const DISPLAY_H: f32 = 236.0;
/// The spectrum's floor, dB.
const FLOOR: f32 = -84.0;

/// The styles, by family: a family's name, and its styles as the picker names them.
const FAMILIES: [(&str, &[(&str, SatStyle)]); 5] = [
    (
        "Tape",
        &[("Warm", SatStyle::WarmTape), ("Clean", SatStyle::CleanTape)],
    ),
    (
        "Tube",
        &[("Warm", SatStyle::WarmTube), ("Clean", SatStyle::CleanTube)],
    ),
    (
        "Amp",
        &[("Crunch", SatStyle::Crunch), ("Heavy", SatStyle::Heavy)],
    ),
    ("Iron", &[("Transformer", SatStyle::Transformer)]),
    (
        "FX",
        &[
            ("Rectify", SatStyle::Rectify),
            ("Foldback", SatStyle::Foldback),
            ("Lo-Fi", SatStyle::Lofi),
        ],
    ),
];

/// A band's range, as it reads: "120 Hz – 1.2 kHz".
fn range_text(s: &Saturator, i: usize) -> String {
    let n = s.bands.len();
    let lo = if i == 0 { 20.0 } else { s.crossovers[i - 1] };
    let hi = if i + 1 >= n {
        20_000.0
    } else {
        s.crossovers[i]
    };
    format!("{} – {}", strip::hz(lo), strip::hz(hi))
}

impl Workspace {
    pub(super) fn sat_window(
        &self,
        ch: Channel,
        s: Saturator,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let on = !s.bypass;
        let n = s.bands.len();
        let band = self.fx_view.as_ref().map_or(0, |v| v.band).min(n - 1);
        let count = div()
            .text_size(px(11.))
            .text_color(rgb(TEXT_4))
            .child(format!(
                "{n} band{}{}",
                if n == 1 { "" } else { "s" },
                if n < MAX_SAT_BANDS {
                    " · double-click the display to split one"
                } else {
                    ""
                }
            ));
        let learning = self.learning == Some(ch);
        let learn = bar_button(
            if learning { "Listening…" } else { "Learn" },
            learning,
            AGENT,
        )
        .debug_selector(|| "sat-learn".into())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                if ws.learning.is_none() {
                    ws.learn_sat_at(ch, ws.fx_stacked(), cx);
                }
            }),
        );
        let learn = match learning {
            true => breathing(learn, "sat-learning"),
            false => learn.into_any_element(),
        };
        let head = self.fx_head("Saturator", ch, Builtin::Sat, on, count, learn, cx);
        let display = self.sat_display(ch, &s, band, cx);
        let panel = self.sat_band_panel(ch, &s, band, cx);
        let foot = div()
            .h(px(48.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(22.))
            .px(px(16.))
            .bg(rgb(0x14151a))
            .border_t_1()
            .border_color(rgb(LINE))
            .child(side_row(
                "Quality",
                choice_tray(
                    "sat-quality",
                    &[("2×", 2u8), ("4×", 4), ("8×", 8)],
                    s.quality,
                    24.,
                    cx,
                    move |ws, q| {
                        if let Some(s) = ws.fx_unit_mut::<Saturator>(ch) {
                            s.quality = q;
                        }
                    },
                ),
            ))
            .child(side_row(
                "Slope",
                choice_tray(
                    "sat-slope",
                    &[("12", 12u8), ("24", 24), ("48 dB/oct", 48)],
                    s.slope,
                    24.,
                    cx,
                    move |ws, slope| {
                        if let Some(s) = ws.fx_unit_mut::<Saturator>(ch) {
                            s.slope = slope;
                        }
                    },
                ),
            ))
            .child(div().flex_1())
            .child(side_row(
                "Mix",
                self.fx_pill(
                    Target::Sat(ch, SatField::Mix),
                    strip::sat_text(SatField::Mix, &s),
                    cx,
                ),
            ))
            .child(side_row(
                "Output",
                self.fx_pill(
                    Target::Sat(ch, SatField::Output),
                    strip::sat_text(SatField::Output, &s),
                    cx,
                ),
            ));
        vec![
            head.into_any_element(),
            display.into_any_element(),
            panel.into_any_element(),
            foot.into_any_element(),
        ]
    }

    /// The bands across the channel's sound, each tinted its colour (the one picked most), the
    /// splits' handles between them.
    fn sat_display(
        &self,
        ch: Channel,
        s: &Saturator,
        picked: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let view = self.fx_view.as_ref().expect("open");
        let spectrum = view.spectrum.clone();
        let plot = view.plot.clone();
        let n = s.bands.len();
        let splits: Vec<f32> = s.crossovers[..n - 1].iter().map(|&c| freq_x(c)).collect();
        let edges: Vec<f32> = [0.0]
            .into_iter()
            .chain(splits.iter().copied())
            .chain([1.0])
            .collect();
        let bands: Vec<SatBand> = s.bands.to_vec();
        let soloing = bands.iter().any(|b| b.solo);
        let on = !s.bypass;
        let heard = move |b: &SatBand| on && !b.mute && (!soloing || b.solo);
        let graph = {
            let (edges, bands) = (edges.clone(), bands.clone());
            canvas(
                |_, _, _| {},
                move |b, _, window, _| {
                    plot.set(b);
                    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                    let quad = |window: &mut gpui::Window,
                                x: f32,
                                y: f32,
                                ww: f32,
                                hh: f32,
                                c: gpui::Hsla| {
                        window.paint_quad(fill(
                            Bounds::new(
                                point(b.origin.x + px(x), b.origin.y + px(y)),
                                size(px(ww), px(hh)),
                            ),
                            c,
                        ));
                    };
                    for (i, band) in bands.iter().enumerate() {
                        let (x0, x1) = (edges[i] * w, edges[i + 1] * w);
                        let color = band_color(i);
                        let (body, top) = match (i == picked, heard(band)) {
                            (true, _) => (0.085, 1.0),
                            (false, true) => (0.03, 0.45),
                            (false, false) => (0.0, 0.18),
                        };
                        quad(window, x0, 0.0, x1 - x0, h, alpha(color, body));
                        quad(window, x0 + 1.0, 0.0, x1 - x0 - 2.0, 3.0, alpha(color, top));
                    }
                    for f in [50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0] {
                        quad(
                            window,
                            (freq_x(f) * w).round(),
                            0.0,
                            1.0,
                            h,
                            rgb(LINE_SOFT).into(),
                        );
                    }
                    // The channel's sound.
                    let k = spectrum.len();
                    let x = |i: usize| b.origin.x + px(i as f32 * w / (k - 1) as f32);
                    let y = |db: f32| b.origin.y + px(level_y(db, FLOOR, h));
                    let mut area = PathBuilder::fill();
                    area.move_to(point(b.origin.x, b.origin.y + b.size.height));
                    for (i, &db) in spectrum.iter().enumerate() {
                        area.line_to(point(x(i), y(db)));
                    }
                    area.line_to(point(b.origin.x + b.size.width, b.origin.y + b.size.height));
                    area.close();
                    if let Ok(p) = area.build() {
                        window.paint_path(p, alpha(0xffffff, 0.07));
                    }
                    let mut line = PathBuilder::stroke(px(1.));
                    line.move_to(point(x(0), y(spectrum[0])));
                    for (i, &db) in spectrum.iter().enumerate().skip(1) {
                        line.line_to(point(x(i), y(db)));
                    }
                    if let Ok(p) = line.build() {
                        window.paint_path(p, alpha(0xffffff, 0.26));
                    }
                    for &e in &edges[1..edges.len() - 1] {
                        quad(window, (e * w).round(), 0.0, 1.0, h, alpha(0xffffff, 0.2));
                    }
                },
            )
            .absolute()
            .size_full()
        };
        // Laid out as drawn: the display's width inside the window.
        let w = super::window_size(Builtin::Sat).0 - 28.0 - 2.0;
        let labels = bands.iter().enumerate().map(|(i, b)| {
            let mid = (edges[i] + edges[i + 1]) / 2.0 * w;
            let color = if heard(b) { band_color(i) } else { TEXT_4 };
            let state = match (b.mute, b.solo) {
                (true, _) => " · muted",
                (_, true) => " · solo",
                _ => "",
            };
            div()
                .absolute()
                .left(px(mid - 60.0))
                .top(px(14.))
                .w(px(120.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(1.))
                .child(
                    div()
                        .text_size(px(11.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(color))
                        .whitespace_nowrap()
                        .child(b.style.name()),
                )
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .whitespace_nowrap()
                        .child(format!("{} dB{state}", strip::trim(b.drive_db))),
                )
        });
        let handles = splits.iter().enumerate().map(|(k, &x)| {
            div()
                .absolute()
                .left(px(x * w - 7.0))
                .top(px(DISPLAY_H / 2.0 - 18.0))
                .w(px(14.))
                .h(px(36.))
                .rounded(px(7.))
                .bg(rgb(0x2b2c35))
                .border_1()
                .border_color(rgb(0x4a4b57))
                .cursor_ew_resize()
                .hover(|d| d.bg(rgb(0x3a3b46)))
                .debug_selector(move || format!("sat-split-{k}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.begin_drag(Drag::SatSplit { k }, cx);
                    }),
                )
        });
        let split_tags = s.crossovers[..n - 1].iter().map(|&c| {
            div()
                .absolute()
                .left(px(freq_x(c) * w - 30.0))
                .top(px(DISPLAY_H / 2.0 + 22.0))
                .w(px(60.))
                .flex()
                .justify_center()
                .font_family(MONO)
                .text_size(px(9.5))
                .text_color(rgb(TEXT_2))
                .child(strip::hz(c))
        });
        let scale = [
            (50.0, "50"),
            (100.0, "100"),
            (200.0, "200"),
            (500.0, "500"),
            (1000.0, "1k"),
            (2000.0, "2k"),
            (5000.0, "5k"),
            (10000.0, "10k"),
        ]
        .map(|(f, label)| tag(freq_x(f) * w + 4.0, DISPLAY_H - 18.0, label, TEXT_4));
        well()
            .flex_none()
            .h(px(DISPLAY_H))
            .mx(px(14.))
            .mt(px(14.))
            .cursor_pointer()
            .debug_selector(|| "sat-display".into())
            .child(graph)
            .children(scale)
            .children(labels)
            .children(split_tags)
            .children(handles)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    let Some(b) = ws.fx_view.as_ref().map(|v| v.plot.get()) else {
                        return;
                    };
                    let x = (f32::from(e.position.x) - f32::from(b.origin.x))
                        / f32::from(b.size.width).max(1.0);
                    let hz = x_freq(x);
                    let mut picked = ws.sat(ch).map(|s| s.band_at(hz));
                    if e.click_count >= 2 {
                        ws.edit(cx, |ws| {
                            if let Some(i) = ws
                                .fx_unit_mut::<Saturator>(ch)
                                .and_then(|s| s.split(hz.round()))
                            {
                                picked = Some(i);
                            }
                        });
                    }
                    if let (Some(v), Some(i)) = (ws.fx_view.as_mut(), picked) {
                        v.band = i;
                    }
                    cx.notify();
                }),
            )
    }

    /// The band picked: its name, range, mute, solo and remove; its style by family, its curve,
    /// and its knobs.
    fn sat_band_panel(&self, ch: Channel, s: &Saturator, i: usize, cx: &mut Context<Self>) -> Div {
        let b = s.bands[i];
        let n = s.bands.len();
        let color = band_color(i);
        let lit = if s.bypass { TEXT_4 } else { color };
        let flag = |label: &'static str,
                    on: bool,
                    tint: u32,
                    cx: &mut Context<Self>,
                    f: fn(&mut SatBand)| {
            toggle(label, on, tint, 0x0c0c0f, 24.)
                .debug_selector(move || format!("sat-band-{}", label.to_lowercase()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.edit(cx, |ws| {
                            if let Some(b) = ws
                                .fx_unit_mut::<Saturator>(ch)
                                .and_then(|s| s.bands.get_mut(i))
                            {
                                f(b);
                            }
                        });
                    }),
                )
        };
        let title = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(div().size(px(9.)).rounded(px(5.)).bg(rgb(color)))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("Band {}", i + 1)),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(range_text(s, i)),
            )
            .child(div().flex_1())
            .child(flag("M", b.mute, AMBER, cx, |b| b.mute ^= true))
            .child(flag("S", b.solo, SOLO, cx, |b| b.solo ^= true))
            .when(n > 1, |d| {
                d.child(icon_button(Icon::Trash, "sat-band-remove").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.edit(cx, |ws| {
                            if let Some(s) = ws.fx_unit_mut::<Saturator>(ch) {
                                s.remove(i);
                            }
                        });
                        if let Some(v) = ws.fx_view.as_mut() {
                            v.band = i.saturating_sub(1);
                        }
                    }),
                ))
            });
        let families = div()
            .w(px(292.))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(6.))
            .children(FAMILIES.map(|(family, styles)| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .w(px(38.))
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_4))
                            .child(family),
                    )
                    .child(choice_tray(
                        "sat-style",
                        styles,
                        b.style,
                        22.,
                        cx,
                        move |ws, style| {
                            if let Some(b) = ws
                                .fx_unit_mut::<Saturator>(ch)
                                .and_then(|s| s.bands.get_mut(i))
                            {
                                b.style = style;
                            }
                        },
                    ))
            }));
        let curve = canvas(
            |_, _, _| {},
            move |bd, _, window, _| {
                let (w, h) = (f32::from(bd.size.width), f32::from(bd.size.height));
                let at = |x: f32, y: f32| {
                    point(
                        bd.origin.x + px((x + 1.0) / 2.0 * w),
                        bd.origin.y + px((1.0 - (y.clamp(-1.0, 1.0) + 1.0) / 2.0) * h),
                    )
                };
                for k in [0.25, 0.5, 0.75] {
                    let c = if k == 0.5 { LINE } else { LINE_SOFT };
                    window.paint_quad(fill(
                        Bounds::new(
                            point(bd.origin.x, bd.origin.y + px((k * h).round())),
                            size(px(w), px(1.)),
                        ),
                        rgb(c),
                    ));
                    window.paint_quad(fill(
                        Bounds::new(
                            point(bd.origin.x + px((k * w).round()), bd.origin.y),
                            size(px(1.), px(h)),
                        ),
                        rgb(c),
                    ));
                }
                let mut clean = PathBuilder::stroke(px(1.));
                clean.move_to(at(-1.0, -1.0));
                clean.line_to(at(1.0, 1.0));
                if let Ok(p) = clean.build() {
                    window.paint_path(p, alpha(0xffffff, 0.14));
                }
                let mut path = PathBuilder::stroke(px(2.));
                for k in 0..=160 {
                    let x = k as f32 / 80.0 - 1.0;
                    let y = static_curve(&b, x);
                    match k {
                        0 => path.move_to(at(x, y)),
                        _ => path.line_to(at(x, y)),
                    }
                }
                if let Ok(p) = path.build() {
                    window.paint_path(p, rgb(lit));
                }
            },
        )
        .absolute()
        .size_full();
        let curve = well()
            .size(px(150.))
            .flex_none()
            .debug_selector(|| "sat-curve".into())
            .child(curve)
            .child(tag(7., 5., b.style.name(), TEXT_3));
        let dial = |ws: &Self, k: SatKnob, name, size: f32, bipolar, cx: &mut Context<Self>| {
            let field = SatField::Band(i as u8, k);
            ws.fx_dial(
                Target::Sat(ch, field),
                name,
                strip::sat_text(field, s),
                size,
                lit,
                bipolar,
                cx,
            )
        };
        let feedback_hz = SatField::Band(i as u8, SatKnob::FeedbackHz);
        let knobs = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(dial(self, SatKnob::Drive, "Drive", 64., false, cx))
                    .child(dial(self, SatKnob::Tone, "Tone", 44., true, cx))
                    .child(dial(self, SatKnob::Dynamics, "Dynamics", 44., true, cx))
                    .child(dial(self, SatKnob::Feedback, "Feedback", 44., false, cx))
                    .child(dial(self, SatKnob::Mix, "Mix", 44., false, cx))
                    .child(dial(self, SatKnob::Level, "Level", 44., true, cx)),
            )
            .child(div().flex().justify_end().child(side_row(
                "Feedback rings at",
                self.fx_pill(
                    Target::Sat(ch, feedback_hz),
                    strip::sat_text(feedback_hz, s),
                    cx,
                ),
            )));
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(12.))
            .px(px(20.))
            .pt(px(14.))
            .child(title)
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(22.))
                    .child(families)
                    .child(curve)
                    .child(knobs),
            )
    }

    /// A split of the saturator's dragged: where it is now, kept a third of an octave from
    /// its neighbours.
    pub fn drag_sat_split(&mut self, k: usize, p: gpui::Point<f32>) {
        let Some((ch, b)) = self.fx_view.as_ref().map(|v| (v.ch, v.plot.get())) else {
            return;
        };
        let x = (p.x - f32::from(b.origin.x)) / f32::from(b.size.width).max(1.0);
        let Some(s) = self.fx_unit_mut::<Saturator>(ch) else {
            return;
        };
        let n = s.bands.len();
        if k + 1 >= n {
            return;
        }
        let gap = 2f32.powf(1.0 / 3.0);
        let lo = if k == 0 {
            20.0
        } else {
            s.crossovers[k - 1] * gap
        };
        let hi = if k + 2 >= n {
            20_000.0
        } else {
            s.crossovers[k + 1] / gap
        };
        s.crossovers[k] = x_freq(x).clamp(lo, hi).round();
    }
}
