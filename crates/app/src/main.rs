use std::{rc::Rc, time::Instant};

use gpui::{
    App, Bounds, Context, Hsla, ScrollDelta, ScrollWheelEvent, Window, WindowBounds,
    WindowOptions, canvas, div, fill, hsla, point, prelude::*, px, rgb, size,
};
use tempo_engine::peaks::{BLOCK, Peaks};

const RATE: f32 = 48_000.0;
const LANE: f32 = 72.0;

struct Arrange {
    tracks: Rc<Vec<(Peaks, Hsla)>>,
    /// First visible sample, and zoom in samples per pixel.
    start: f32,
    spp: f32,
    playing_since: Instant,
    frames: u32,
    fps: u32,
    fps_since: Instant,
}

impl Arrange {
    fn new() -> Self {
        let tracks = (0..8)
            .map(|i| {
                let hue = [0.58, 0.33, 0.08, 0.75][i % 4];
                (Peaks::from_samples(&test_signal(i, 60.0)), hsla(hue, 0.55, 0.45, 1.0))
            })
            .collect::<Vec<_>>()
            .into();
        Arrange {
            tracks,
            start: 0.0,
            spp: 256.0,
            playing_since: Instant::now(),
            frames: 0,
            fps: 0,
            fps_since: Instant::now(),
        }
    }

    fn scroll(&mut self, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let d = match e.delta {
            ScrollDelta::Pixels(p) => p.map(f32::from),
            ScrollDelta::Lines(l) => l.map(|v| v * 20.0),
        };
        if e.modifiers.platform {
            // Zoom around the pointer.
            let x = f32::from(e.position.x);
            let at = self.start + x * self.spp;
            self.spp = (self.spp * (1.0 - d.y * 0.01)).clamp(BLOCK as f32, 20_000.0);
            self.start = (at - x * self.spp).max(0.0);
        } else {
            self.start = (self.start - (d.x + d.y) * self.spp).max(0.0);
        }
        cx.notify();
    }
}

impl Render for Arrange {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.frames += 1;
        if self.fps_since.elapsed().as_secs_f32() >= 1.0 {
            self.fps = self.frames;
            self.frames = 0;
            self.fps_since = Instant::now();
        }
        // ponytail: redraws every frame to measure the frame rate; draw on change once
        // transport and meters drive redraws.
        window.request_animation_frame();

        let playhead = (self.playing_since.elapsed().as_secs_f32() * RATE) % (60.0 * RATE);
        let (start, spp) = (self.start, self.spp);
        let tracks = self.tracks.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1c1c1e))
            .text_color(rgb(0xd0d0d0))
            .text_sm()
            .child(
                div()
                    .h(px(36.))
                    .px_3()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(0x000000))
                    .bg(rgb(0x2a2a2c))
                    .child(format!(
                        "{} fps · {:.0} samples/px · scroll to move, ⌘-scroll to zoom",
                        self.fps, self.spp
                    )),
            )
            .child(
                div()
                    .id("arrange")
                    .flex_1()
                    .on_scroll_wheel(cx.listener(|this, e, _, cx| this.scroll(e, cx)))
                    .child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| {
                                let o = bounds.origin;
                                let width = f32::from(bounds.size.width) as usize;
                                for (i, (peaks, color)) in tracks.iter().enumerate() {
                                    let top = f32::from(o.y) + i as f32 * LANE;
                                    let mid = top + LANE / 2.0;
                                    let end = ((peaks.len_samples() as f32 - start) / spp).max(0.0);
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(o.x, px(top + 1.0)),
                                            size(px(end.min(width as f32)), px(LANE - 2.0)),
                                        ),
                                        color.opacity(0.35),
                                    ));
                                    for x in 0..width.min(end as usize) {
                                        let from = (start + x as f32 * spp) as usize;
                                        let (lo, hi) = peaks.range(from, from + spp as usize);
                                        let h = LANE / 2.0 - 4.0;
                                        let y0 = mid - hi * h;
                                        let y1 = (mid - lo * h).max(y0 + 1.0);
                                        window.paint_quad(fill(
                                            Bounds::new(
                                                point(o.x + px(x as f32), px(y0)),
                                                size(px(1.0), px(y1 - y0)),
                                            ),
                                            *color,
                                        ));
                                    }
                                }
                                let x = (playhead - start) / spp;
                                if (0.0..width as f32).contains(&x) {
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(o.x + px(x), o.y),
                                            size(px(1.0), bounds.size.height),
                                        ),
                                        rgb(0xffffff),
                                    ));
                                }
                            },
                        )
                        .size_full(),
                    ),
            )
    }
}

/// A minute of drum-ish hits over a tone, different per track, so the spike has something to draw.
fn test_signal(track: usize, seconds: f32) -> Vec<f32> {
    let n = (seconds * RATE) as usize;
    let beat = (RATE * 60.0 / 120.0) as usize / (1 + track % 3);
    let freq = 55.0 * (track + 1) as f32;
    let mut seed = 0x9e37_79b9u32.wrapping_mul(track as u32 + 1);
    (0..n)
        .map(|i| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let noise = seed as f32 / u32::MAX as f32 * 2.0 - 1.0;
            let env = (-((i % beat) as f32) / (RATE * 0.08)).exp();
            let tone = (i as f32 / RATE * freq * std::f32::consts::TAU).sin() * 0.25;
            (noise * env * 0.7 + tone * (0.5 + 0.5 * env)).clamp(-1.0, 1.0)
        })
        .collect()
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1200.), px(700.)), cx);
        cx.open_window(
            WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default() },
            |_, cx| cx.new(|_| Arrange::new()),
        )
        .unwrap();
        cx.activate(true);
    });
}
