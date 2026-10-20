//! A take folder's takes, a row each, in the pane under the tracks (double-click the folder).
//! As in Logic, swiping across a take comps that stretch from it; clicking a take uses all of it.
//! Lined up with the tracks above, like the piano roll.

use gpui::{
    Bounds, Context, Hsla, MouseButton, MouseDownEvent, canvas, div, fill, point, prelude::*, px,
    rgb, size,
};
use pina_engine::project::{At, Content, comp_segments};

use crate::{
    theme::*,
    tracks::{Inside, paint_inside},
    workspace::{Drag, Workspace},
};

pub const TAKE_H: f32 = 44.0;

impl Workspace {
    /// The take folder the pane shows: the selected region, if it's one.
    pub fn take_folder(&self) -> Option<At> {
        self.region.filter(|&at| {
            matches!(
                self.session.project.region(at).content,
                Content::Takes { .. }
            )
        })
    }

    /// Ends a comp swipe: the take plays for the swiped stretch, or throughout for a click.
    pub fn finish_comp(&mut self, at: At, take: usize, from: u64, to: u64, cx: &mut Context<Self>) {
        let length = self.session.project.region(at).length;
        let (from, to) = if from == to {
            (0, length)
        } else {
            (from.min(to), from.max(to))
        };
        self.session.checkpoint();
        match self.session.project.comp(at, from, to, take) {
            Ok(()) => self.changed(cx),
            Err(e) => {
                self.session.cancel();
                self.status = Some(e);
            }
        }
    }

    /// The tick a window x position falls on in a region, snapped, clamped to the region.
    pub fn region_tick(&self, at: At, x: f32) -> u64 {
        let r = self.session.project.region(at);
        let tick = self.snap(self.tick_at(x));
        tick.clamp(r.start, r.start + r.length) - r.start
    }

    pub fn takes_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (ppt, sx) = (self.px_per_tick(), self.scroll_x);
        let at = self.take_folder();
        let mut labels = Vec::new();
        let mut rows = Vec::new();
        if let Some(at) = at {
            let track = &self.session.project.tracks[at.0];
            let region = track.regions[at.1].clone();
            let Content::Takes { takes, comp } = &region.content else {
                unreachable!()
            };
            let color = Hsla::from(rgb(self.color(at.0)));
            let segments = comp_segments(comp, takes.len(), region.length);
            let left = region.start as f32 * ppt - sx;
            let swipe = match self.drag {
                Some(Drag::Comp { take, from, to, .. }) => Some((take, from.min(to), from.max(to))),
                _ => None,
            };
            for (i, take) in takes.iter().enumerate() {
                let mut inside = Inside::default();
                self.add_inside(&mut inside, &take.content, take.start, 0, region.length);
                // Where this take is in the comp, and where it's being swiped.
                let mut lit: Vec<(u64, u64)> = segments
                    .iter()
                    .filter(|s| s.2 == i)
                    .map(|s| (s.0, s.1))
                    .collect();
                if let Some((_, a, b)) = swipe.filter(|s| s.0 == i) {
                    lit.push((a, b));
                }
                let width = region.length as f32 * ppt;
                rows.push(
                    div()
                        .absolute()
                        .top(px(i as f32 * TAKE_H))
                        .left(px(left))
                        .w(px(width))
                        .h(px(TAKE_H - 2.))
                        .rounded(px(3.))
                        .bg(color.opacity(0.18))
                        .debug_selector(move || format!("take-{i}"))
                        .child(
                            canvas(
                                |_, _, _| {},
                                move |b, _, window, _| {
                                    for &(a, z) in &lit {
                                        let at = point(b.origin.x + px(a as f32 * ppt), b.origin.y);
                                        let span = size(px((z - a) as f32 * ppt), b.size.height);
                                        window.paint_quad(fill(
                                            Bounds::new(at, span),
                                            color.opacity(0.55),
                                        ));
                                    }
                                    paint_inside(&inside, b, ppt, color, window);
                                },
                            )
                            .absolute()
                            .size_full(),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                let tick = ws.region_tick(at, f32::from(e.position.x));
                                let drag = Drag::Comp {
                                    at,
                                    take: i,
                                    from: tick,
                                    to: tick,
                                };
                                ws.begin_drag(drag, cx);
                            }),
                        ),
                );
                labels.push(
                    div()
                        .h(px(TAKE_H))
                        .flex()
                        .items_center()
                        .px_2()
                        .text_size(px(11.))
                        .child(format!("Take {}", i + 1)),
                );
            }
        }
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .bg(rgb(LANE))
            .child(
                div()
                    .w(px(self.header_w))
                    .flex_none()
                    .bg(rgb(SURFACE))
                    .h_full()
                    .flex()
                    .flex_col()
                    .border_r_1()
                    .border_color(rgb(LINE))
                    .children(labels)
                    .when_some(at, |d, at| {
                        d.child(
                            div().p_2().child(
                                button("Flatten", false)
                                    .debug_selector(|| "flatten".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            ws.edit(cx, |ws| {
                                                let made = ws.session.project.flatten(at);
                                                ws.region = made.first().copied();
                                            });
                                        }),
                                    ),
                            ),
                        )
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .relative()
                    .overflow_hidden()
                    .children(rows)
                    .when(at.is_none(), |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(20.))
                                .left(px(20.))
                                .text_color(rgb(TEXT_3))
                                .child("Select a take folder: record over a region to make one."),
                        )
                    })
                    .on_scroll_wheel(self.on_scroll(cx)),
            )
    }
}
