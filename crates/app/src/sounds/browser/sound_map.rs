//! The sound map: every sample laid out by how it sounds, to point at, box in and scroll.

use super::*;

impl Workspace {
    /// A map position's place in the window.
    pub(crate) fn map_point(&self, b: Bounds<Pixels>, m: [f32; 2]) -> Point<f32> {
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        let z = self.browser.zoom;
        let c = self.browser.center;
        point(
            f32::from(b.origin.x) + w / 2.0 + (m[0] - c[0]) * w / 2.0 * 0.9 * z,
            f32::from(b.origin.y) + h / 2.0 + (m[1] - c[1]) * h / 2.0 * 0.84 * z,
        )
    }

    /// The sample whose dot is nearest a window position, within reach.
    pub(crate) fn map_dot_at(&self, p: Point<f32>) -> Option<usize> {
        let b = self.browser.map_bounds.get();
        let places = self.browser.places()?;
        places
            .iter()
            .enumerate()
            .map(|(i, &m)| {
                let q = self.map_point(b, m);
                (i, (q.x - p.x).hypot(q.y - p.y))
            })
            .filter(|&(_, d)| d < 9.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// The results keep to the samples in a box drawn on the map; a click without a box shows
    /// them all again.
    pub(crate) fn box_area(&mut self, from: Point<f32>, to: Point<f32>) {
        let b = self.browser.map_bounds.get();
        let (x0, x1) = (from.x.min(to.x), from.x.max(to.x));
        let (y0, y1) = (from.y.min(to.y), from.y.max(to.y));
        self.browser.area = match (x1 - x0 > 4.0 || y1 - y0 > 4.0, self.browser.places()) {
            (true, Some(places)) => Some(
                places
                    .iter()
                    .enumerate()
                    .filter(|&(_, &m)| {
                        let q = self.map_point(b, m);
                        (x0..=x1).contains(&q.x) && (y0..=y1).contains(&q.y)
                    })
                    .map(|(i, _)| i)
                    .collect(),
            ),
            _ => None,
        };
        self.find_samples();
    }

    /// Scrolling the map pans it; with ⌘ (or a pinch) it zooms round the pointer.
    pub(crate) fn scroll_map(&mut self, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let d = match e.delta {
            ScrollDelta::Pixels(p) => p.map(f32::from),
            ScrollDelta::Lines(l) => l.map(|v| v * 24.0),
        };
        let b = self.browser.map_bounds.get();
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let z = self.browser.zoom;
        let (sx, sy) = (w / 2.0 * 0.9 * z, h / 2.0 * 0.84 * z);
        let c = &mut self.browser.center;
        if e.modifiers.platform {
            // Keep what's under the pointer there.
            let p = pt(e.position);
            let m = [
                c[0] + (p.x - f32::from(b.origin.x) - w / 2.0) / sx,
                c[1] + (p.y - f32::from(b.origin.y) - h / 2.0) / sy,
            ];
            let zoom = (z * (1.0 + d.y * 0.01)).clamp(0.6, 40.0);
            let k = z / zoom;
            *c = [m[0] - (m[0] - c[0]) * k, m[1] - (m[1] - c[1]) * k];
            self.browser.zoom = zoom;
        } else {
            c[0] -= d.x / sx;
            c[1] -= d.y / sy;
        }
        cx.notify();
    }

    pub(crate) fn hover_map(&mut self, p: Point<f32>, cx: &mut Context<Self>) {
        if self.drag.is_some() {
            return;
        }
        let dot = self.map_dot_at(p);
        if dot != self.browser.hovered {
            self.browser.hovered = dot;
            // As XO does: what's under the pointer plays.
            if let Some(i) = dot {
                self.audition_sample(i, cx);
            }
            cx.notify();
        }
    }

    pub(crate) fn results_map(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let b = &self.browser;
        let bounds = b.map_bounds.clone();
        let places: Vec<([f32; 2], u32, bool, usize)> = match b.places() {
            Some(places) => {
                let shown: HashSet<usize> = b.hits.iter().map(|h| h.index).collect();
                places
                    .iter()
                    .enumerate()
                    .map(|(i, &m)| {
                        let c = category_color(b.library.samples[i].analysis.category);
                        (m, c, shown.contains(&i), i)
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        let mb = b.map_bounds.get();
        let to_screen = {
            let (z, c) = (b.zoom, b.center);
            move |b: Bounds<Pixels>, m: [f32; 2]| {
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                point(
                    f32::from(b.origin.x) + w / 2.0 + (m[0] - c[0]) * w / 2.0 * 0.9 * z,
                    f32::from(b.origin.y) + h / 2.0 + (m[1] - c[1]) * h / 2.0 * 0.84 * z,
                )
            }
        };
        let (selected, hovered, playing) = (b.selected, b.hovered, b.audition.map(|a| a.0));
        let rect = match self.drag {
            Some(Drag::MapBox { from, to }) => Some((from, to)),
            _ => None,
        };
        let area = b.area.is_some();
        let painted = places.clone();
        let map = canvas(
            move |bnd, _, _| bounds.set(bnd),
            move |bnd, _, window, _| {
                let (w, h) = (f32::from(bnd.size.width), f32::from(bnd.size.height));
                // A faint grid, so panning and zooming show.
                for k in 1..8 {
                    let x = bnd.origin.x + px(w * k as f32 / 8.0);
                    let y = bnd.origin.y + px(h * k as f32 / 8.0);
                    window.paint_quad(fill(
                        Bounds::new(point(x, bnd.origin.y), size(px(1.), bnd.size.height)),
                        rgb(LINE_SOFT),
                    ));
                    window.paint_quad(fill(
                        Bounds::new(point(bnd.origin.x, y), size(bnd.size.width, px(1.))),
                        rgb(LINE_SOFT),
                    ));
                }
                let dot = |window: &mut Window, p: Point<f32>, r: f32, c: Hsla| {
                    window.paint_quad(
                        fill(
                            Bounds::new(
                                point(px(p.x - r), px(p.y - r)),
                                size(px(r * 2.), px(r * 2.)),
                            ),
                            c,
                        )
                        .corner_radii(px(r)),
                    );
                };
                // What the filters leave out, faintly, then what they find over it.
                for pass in [false, true] {
                    for &(m, c, shown, _) in painted.iter().filter(|p| p.2 == pass) {
                        let p = to_screen(bnd, m);
                        if p.x < f32::from(bnd.origin.x) - 6.0
                            || p.x > f32::from(bnd.origin.x) + w + 6.0
                        {
                            continue;
                        }
                        match shown {
                            true => dot(window, p, 3.5, alpha(c, 0.9)),
                            false => dot(window, p, 2.5, alpha(c, if area { 0.12 } else { 0.18 })),
                        }
                    }
                }
                for (i, ring) in [(hovered, TEXT_2), (playing, ACCENT_TEXT), (selected, TEXT)] {
                    if let Some(&(m, c, ..)) = i.and_then(|i| painted.get(i)) {
                        let p = to_screen(bnd, m);
                        dot(window, p, 8.0, alpha(ring, 0.9));
                        dot(window, p, 6.5, rgb(LANE).into());
                        dot(window, p, 5.0, rgb(c).into());
                    }
                }
                if let Some((a, b)) = rect {
                    let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
                    let r = Bounds::new(
                        point(px(x0), px(y0)),
                        size(px((a.x - b.x).abs()), px((a.y - b.y).abs())),
                    );
                    window.paint_quad(
                        fill(r, alpha(ACCENT, 0.1))
                            .border_widths(px(1.))
                            .border_color(alpha(ACCENT, 0.8)),
                    );
                }
            },
        )
        .absolute()
        .size_full();
        // The hovered sound's name, by its dot.
        let label = hovered
            .or(selected)
            .and_then(|i| Some((i, places.get(i)?.0, self.browser.sample(i)?)))
            .map(|(_, m, s)| {
                let p = to_screen(mb, m);
                div()
                    .absolute()
                    .left(px(p.x - f32::from(mb.origin.x) + 12.))
                    .top(px(p.y - f32::from(mb.origin.y) - 11.))
                    .h(px(22.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(6.))
                    .bg(rgb(0x1c1c22))
                    .border_1()
                    .border_color(rgb(LINE_STRONG))
                    .shadow_md()
                    .text_size(px(11.))
                    .whitespace_nowrap()
                    .child(s.name.clone())
                    .child(
                        div()
                            .text_color(rgb(TEXT_3))
                            .child(s.analysis.category.name()),
                    )
            });
        let present: HashSet<u32> = places.iter().map(|p| p.1).collect();
        let legend = LEGEND
            .iter()
            .filter(|(c, _)| present.contains(c))
            .map(|&(c, name)| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .child(div().size(px(6.)).rounded(px(3.)).bg(rgb(c)))
                    .child(name)
            });
        let empty = places.is_empty();
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(self.results_header(false, cx))
            .child(
                div()
                    .id("sample-map")
                    .flex_1()
                    .min_h(px(0.))
                    .relative()
                    .overflow_hidden()
                    .cursor_crosshair()
                    .debug_selector(|| "sample-map".into())
                    .child(map)
                    .children(label)
                    .when(empty, |d| {
                        d.child(
                            div()
                                .absolute()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(rgb(TEXT_3))
                                .child(match self.browser.library.samples.is_empty() {
                                    true => "The map shows every sample once the library has some.",
                                    false => "Placing sounds on the map…",
                                }),
                        )
                    })
                    .on_mouse_move(
                        cx.listener(|ws, e: &MouseMoveEvent, _, cx| {
                            ws.hover_map(pt(e.position), cx)
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            let p = pt(e.position);
                            match ws.map_dot_at(p) {
                                // A dot: select it, and drag it to the tracks if you like.
                                Some(i) => {
                                    ws.select_sample(i, true, cx);
                                    ws.begin_drag(
                                        Drag::Sample {
                                            index: i,
                                            from: p,
                                            at: p,
                                            moved: false,
                                        },
                                        cx,
                                    );
                                }
                                // Empty space: draw a box round the ones to keep.
                                None => ws.begin_drag(Drag::MapBox { from: p, to: p }, cx),
                            }
                        }),
                    )
                    .on_scroll_wheel(
                        cx.listener(|ws, e: &ScrollWheelEvent, _, cx| ws.scroll_map(e, cx)),
                    ),
            )
            // The legend, and the way back to the whole map.
            .child(
                div()
                    .h(px(36.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(12.))
                    .border_t_1()
                    .border_color(rgb(LINE_SOFT))
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .whitespace_nowrap()
                    .children(legend)
                    .child(div().flex_1())
                    .child(
                        button("Fit", false)
                            .h(px(24.))
                            .debug_selector(|| "map-fit".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    ws.browser.zoom = 1.0;
                                    ws.browser.center = [0.0, 0.0];
                                    cx.notify();
                                }),
                            ),
                    ),
            )
            .child(self.info_line())
    }
}
