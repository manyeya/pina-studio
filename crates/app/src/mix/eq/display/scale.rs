//! The plot's scales: frequencies along the bottom, gain down the side.

use super::*;

/// Frequencies worth a line and a label across a stretch, `apart` pixels at least.
pub(crate) fn marks(geo: &Geo, apart: f32) -> Vec<(f64, bool)> {
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

pub(crate) fn freq_label(f: f64) -> String {
    match f {
        f if f >= 1_000.0 => format!("{}k", f / 1_000.0),
        f => format!("{f}"),
    }
}

impl Workspace {
    /// Under the plot: its frequencies (or a piano), the one under the pointer; drag it up to
    /// zoom in, sideways to pan, double-click it to see all.
    pub(crate) fn eq_scale(
        &self,
        g: &Geo,
        drawn: &[Drawn],
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
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
                        v.span = (super::super::LOW, super::super::HIGH);
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
    pub(crate) fn eq_side(&self, g: &Geo, dh: f32, cx: &mut Context<Self>) -> Div {
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
