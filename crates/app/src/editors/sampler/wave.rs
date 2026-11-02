//! The picked zone's sample, big: its waveform (its loudness a body inside its peaks), what
//! plays lit and the rest shaded, its loop (the crossfade drawn), the other slices of its
//! sample, where grains read and how far they scatter; the marks to drag; and every voice
//! playing it a glowing playhead, every grain a spark. The wheel zooms where it points
//! (sideways scrolls); double-click shows it all.

use super::*;

/// A place in the sample (0 to 1) across what's shown (0 to 1), and back.
fn across(at: f32, view: (f32, f32)) -> f32 {
    (at - view.0) / (view.1 - view.0).max(1e-6)
}

/// A number from 0 to 1 for a grain, so sparks scatter up and down the same way each frame.
fn scatter(i: usize) -> f32 {
    let mut x = (i as u32).wrapping_mul(2_654_435_761) ^ 0x9E37_79B9;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x % 10_000) as f32 / 10_000.0
}

impl Workspace {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sampler_wave(
        &self,
        t: usize,
        s: &Sampler,
        picked: usize,
        wave: Option<Arc<Wave>>,
        cx: &mut Context<Self>,
    ) -> Div {
        let z = s.zones[picked].clone();
        let v = s.voice;
        let color = zone_color(picked);
        let view = self.sampler_ui.view.unwrap_or((0.0, 1.0));
        let (start, end) = (z.start, z.end.unwrap_or(1.0));
        let looping = z.loop_mode != LoopMode::Off;
        let (ls, le) = (z.loop_start.unwrap_or(start), z.loop_end.unwrap_or(end));
        let fade = z.loop_fade * (le - ls);
        let grains = v.mode != SampleMode::Repitch;
        let place = start + v.position * (end - start);
        let seconds = wave.as_ref().map_or(1.0, |w| w.seconds.max(1e-3));
        let spray = (v.spray / seconds).min(1.0);
        // Who's playing this sample: the picked zone, and its sample's other slices.
        let same: Vec<usize> = (0..s.zones.len())
            .filter(|&i| s.zones[i].file == z.file)
            .collect();
        // Each slice's part of it (start, end, colour), the picked one lit.
        let parts: Vec<(f32, f32, u32, bool)> = same
            .iter()
            .map(|&i| {
                let o = &s.zones[i];
                (o.start, o.end.unwrap_or(1.0), zone_color(i), i == picked)
            })
            .collect();
        let playing: Vec<(Playing, u32)> = self
            .sampler_ui
            .playing
            .iter()
            .filter(|p| same.contains(&p.zone))
            .map(|p| (*p, zone_color(p.zone)))
            .collect();
        let drawn = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (x0, y0) = (f32::from(b.origin.x), f32::from(b.origin.y));
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                let quad = |window: &mut Window, x: f32, y: f32, ww: f32, hh: f32, c: Hsla| {
                    window.paint_quad(fill(
                        Bounds::new(point(px(x), px(y)), size(px(ww), px(hh))),
                        c,
                    ));
                };
                let x_of = |at: f32| x0 + across(at, view) * w;
                let mid = y0 + h / 2.0;
                quad(window, x0, mid, w, 1.0, alpha(0xffffff, 0.06));
                let Some(wave) = &wave else { return };
                // The waveform: its peaks, and its loudness as a brighter body inside them.
                let cols = w.max(1.0) as usize;
                for c in 0..cols {
                    let (fa, fb) = (
                        view.0 + (view.1 - view.0) * c as f32 / cols as f32,
                        view.0 + (view.1 - view.0) * (c + 1) as f32 / cols as f32,
                    );
                    let (a, bb) = (
                        (fa * wave.len as f32) as usize,
                        (fb * wave.len as f32) as usize,
                    );
                    let (lo, hi) = wave.peaks.range(a, bb.max(a + 1));
                    let loud = {
                        let k = |f: f32| ((f * LOUD as f32) as usize).min(LOUD - 1);
                        wave.loud[k(fa)..=k(fb).max(k(fa))]
                            .iter()
                            .fold(0f32, |m, &x| m.max(x))
                    };
                    let part = parts
                        .iter()
                        .filter(|p| fa >= p.0 && fa < p.1)
                        .max_by_key(|p| p.3);
                    let (outer, inner) = match part {
                        Some(&(_, _, c, true)) => (alpha(c, 0.4), alpha(c, 0.95)),
                        Some(&(_, _, c, false)) => (alpha(c, 0.16), alpha(c, 0.42)),
                        None => (alpha(0xffffff, 0.1), alpha(0xffffff, 0.2)),
                    };
                    let scale = h * 0.46;
                    let (top, bottom) = (mid - hi.max(0.0) * scale, mid - lo.min(0.0) * scale);
                    quad(
                        window,
                        x0 + c as f32,
                        top,
                        1.0,
                        (bottom - top).max(1.0),
                        outer,
                    );
                    let body = (loud * 1.4).min(1.0) * scale;
                    quad(
                        window,
                        x0 + c as f32,
                        mid - body,
                        1.0,
                        (body * 2.0).max(1.0),
                        inner,
                    );
                }
                // Outside what plays, shaded.
                let shade = alpha(0x000000, 0.45);
                if x_of(start) > x0 {
                    quad(window, x0, y0, x_of(start) - x0, h, shade);
                }
                if x_of(end) < x0 + w {
                    quad(window, x_of(end), y0, x0 + w - x_of(end), h, shade);
                }
                if looping {
                    let (a, bb) = (x_of(ls), x_of(le));
                    quad(window, a, y0, bb - a, h, alpha(color, 0.07));
                    quad(window, a, y0, bb - a, 3.0, rgb(color).into());
                    // The crossfade: what's before the loop's start fading in as its end fades
                    // out.
                    if fade > 0.0 {
                        for (from, to, rising) in [(le - fade, le, false), (ls - fade, ls, true)] {
                            let (fx0, fx1) = (x_of(from), x_of(to));
                            let mut ramp = PathBuilder::fill();
                            let (y_top, y_bottom) = (y0 + 3.0, y0 + h);
                            match rising {
                                true => {
                                    ramp.move_to(point(px(fx0), px(y_bottom)));
                                    ramp.line_to(point(px(fx1), px(y_top)));
                                    ramp.line_to(point(px(fx1), px(y_bottom)));
                                }
                                false => {
                                    ramp.move_to(point(px(fx0), px(y_top)));
                                    ramp.line_to(point(px(fx1), px(y_bottom)));
                                    ramp.line_to(point(px(fx0), px(y_bottom)));
                                }
                            }
                            ramp.close();
                            if let Ok(p) = ramp.build() {
                                window.paint_path(p, alpha(AMBER, 0.12));
                            }
                        }
                    }
                }
                // Where the grains read, and how far they scatter.
                if grains {
                    let (a, bb) = (x_of(place - spray), x_of(place + spray));
                    quad(window, a, y0, (bb - a).max(1.0), h, alpha(PINK, 0.08));
                }
                // The playheads: a glow and a line for each voice, a spark for each grain.
                for (i, (p, c)) in playing.iter().enumerate() {
                    let x = x_of(p.at);
                    if x < x0 - 6.0 || x > x0 + w + 6.0 {
                        continue;
                    }
                    let level = p.level.clamp(0.15, 1.0);
                    match p.grain {
                        true => {
                            let y = y0 + h * (0.12 + 0.76 * scatter(i));
                            if let Some(path) = disc(point(px(x), px(y)), 1.5 + 2.5 * level) {
                                window.paint_path(path, alpha(*c, 0.85 * level));
                            }
                            if let Some(path) = disc(point(px(x), px(y)), 5.0 + 4.0 * level) {
                                window.paint_path(path, alpha(*c, 0.12 * level));
                            }
                        }
                        false => {
                            for (width, a) in [(14.0, 0.08), (7.0, 0.16), (2.0, 1.0)] {
                                quad(window, x - width / 2.0, y0, width, h, alpha(*c, a * level));
                            }
                        }
                    }
                }
            },
        )
        .absolute()
        .size_full();
        let mark =
            |at: f32, which: Mark, label: &'static str, tone: u32, cx: &mut Context<Self>| {
                let x = across(at, view);
                div()
                    .absolute()
                    .left(gpui::relative(x))
                    .top_0()
                    .bottom_0()
                    .w(px(11.))
                    .ml(px(-5.))
                    .cursor_ew_resize()
                    .debug_selector(move || format!("mark-{which:?}"))
                    .when(!(-0.01..=1.01).contains(&x), |d| d.invisible())
                    .child(
                        div()
                            .absolute()
                            .left(px(5.))
                            .top_0()
                            .bottom_0()
                            .w(px(1.5))
                            .bg(rgb(tone)),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(5.))
                            .top(px(match which {
                                Mark::LoopStart | Mark::LoopEnd => 5.,
                                Mark::Position => 22.,
                                _ => 0.,
                            }))
                            .h(px(15.))
                            .px(px(4.))
                            .flex()
                            .items_center()
                            .rounded_r(px(4.))
                            .bg(rgb(tone))
                            .text_size(px(9.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(BG))
                            .child(label),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            let b = ws.sampler_wave_bounds.get();
                            let (left, width) = (f32::from(b.origin.x), f32::from(b.size.width));
                            ws.begin_drag(
                                Drag::Mark {
                                    track: t,
                                    zone: picked,
                                    mark: which,
                                    left,
                                    width,
                                },
                                cx,
                            );
                        }),
                    )
            };
        let siblings: Vec<(usize, f32, u8)> = s
            .zones
            .iter()
            .enumerate()
            .filter(|(i, o)| *i != picked && o.file == z.file)
            .map(|(i, o)| (i, o.start, o.note))
            .collect();
        let wave_bounds = self.sampler_wave_bounds.clone();
        // Seconds along the bottom, at a spacing that suits the zoom.
        let shown = seconds * (view.1 - view.0);
        let step = [
            0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0,
        ]
        .into_iter()
        .find(|&s| shown / s <= 10.0)
        .unwrap_or(120.0);
        let first = (seconds * view.0 / step).ceil() as i64;
        let ticks: Vec<(f32, String)> = (first..)
            .map(|k| k as f32 * step)
            .take_while(|&sec| sec <= seconds * view.1)
            .map(|sec| {
                let label = match step < 1.0 {
                    true => format!("{sec:.2}s"),
                    false => format!("{sec:.0}s"),
                };
                (across(sec / seconds, view), label)
            })
            .collect();
        let zoomed = view != (0.0, 1.0);
        div()
            .flex_1()
            .min_h(px(110.))
            .relative()
            .rounded(px(12.))
            .bg(rgb(0x0a0a0d))
            .border_1()
            .border_color(rgb(LINE_SOFT))
            .overflow_hidden()
            .debug_selector(|| "sampler-wave".into())
            .child(
                canvas(move |b, _, _| wave_bounds.set(b), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(drawn)
            .children(siblings.iter().map(|&(i, at, note)| {
                div()
                    .absolute()
                    .left(gpui::relative(across(at, view)))
                    .top_0()
                    .bottom_0()
                    .w(px(1.))
                    .bg(alpha(zone_color(i), 0.6))
                    .child(
                        div()
                            .absolute()
                            .left(px(4.))
                            .bottom(px(16.))
                            .font_family(MONO)
                            .text_size(px(9.))
                            .whitespace_nowrap()
                            .text_color(rgb(zone_color(i)))
                            .child(name_in(None, note)),
                    )
            }))
            .children(ticks.into_iter().map(|(x, label)| {
                div()
                    .absolute()
                    .left(gpui::relative(x))
                    .bottom(px(2.))
                    .pl(px(3.))
                    .border_l_1()
                    .border_color(alpha(0xffffff, 0.12))
                    .font_family(MONO)
                    .text_size(px(8.5))
                    .text_color(rgb(TEXT_4))
                    .child(label)
            }))
            .child(mark(start, Mark::Start, "S", MINT, cx))
            .child(mark(end, Mark::End, "E", CORAL, cx))
            .when(looping, |d| {
                d.child(mark(ls, Mark::LoopStart, "L", AMBER, cx))
                    .child(mark(le, Mark::LoopEnd, "L", AMBER, cx))
            })
            .when(grains, |d| {
                d.child(mark(place, Mark::Position, "P", PINK, cx))
            })
            .when(zoomed, |d| {
                // Where what's shown is in the whole sample.
                d.child(
                    div()
                        .absolute()
                        .left(gpui::relative(view.0))
                        .w(gpui::relative(view.1 - view.0))
                        .top(px(2.))
                        .h(px(3.))
                        .rounded(px(2.))
                        .bg(alpha(0xffffff, 0.3)),
                )
            })
            .on_scroll_wheel(cx.listener(|ws, e: &ScrollWheelEvent, _, cx| {
                let b = ws.sampler_wave_bounds.get();
                let x = f32::from(e.position.x - b.origin.x) / f32::from(b.size.width).max(1.0);
                let d = e.delta.pixel_delta(px(16.));
                ws.zoom_sample(x.clamp(0.0, 1.0), f32::from(d.y), f32::from(d.x), cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    if e.click_count >= 2 {
                        ws.sampler_ui.view = None;
                        cx.notify();
                    }
                }),
            )
    }

    /// The wheel over the sample: up and down zooms around `at` (0 to 1 across), sideways
    /// scrolls.
    pub(crate) fn zoom_sample(&mut self, at: f32, dy: f32, dx: f32, cx: &mut Context<Self>) {
        let (a, b) = self.sampler_ui.view.unwrap_or((0.0, 1.0));
        let span = b - a;
        let (mut a, mut b) = match dy.abs() > dx.abs() {
            true => {
                let factor = (dy * 0.004).exp();
                let new = (span * factor).clamp(0.002, 1.0);
                let pivot = a + at * span;
                (pivot - at * new, pivot + (1.0 - at) * new)
            }
            false => {
                let shift = -dx / 600.0 * span;
                (a + shift, b + shift)
            }
        };
        if a < 0.0 {
            (a, b) = (0.0, b - a);
        }
        if b > 1.0 {
            (a, b) = ((a - (b - 1.0)).max(0.0), 1.0);
        }
        self.sampler_ui.view = ((b - a) < 0.999).then_some((a, b));
        cx.notify();
    }
}
