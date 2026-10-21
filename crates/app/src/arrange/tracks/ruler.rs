//! The ruler and the markers above it.

use super::*;

impl Workspace {
    /// The song's sections: each marker runs to the next.
    pub(crate) fn markers(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (ppt, sx) = (self.px_per_tick(), self.scroll_x);
        let markers = &self.session.project.markers;
        let playhead = self.playhead();
        let bar = self.session.project.bar();
        let chips: Vec<Div> = markers
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let end = markers
                    .get(i + 1)
                    .map_or(m.tick + bar * 8, |n| n.tick.max(m.tick + 1));
                let current = (m.tick..end).contains(&playhead);
                let (x, w) = (m.tick as f32 * ppt - sx, (end - m.tick) as f32 * ppt);
                let tick = m.tick;
                div()
                    .absolute()
                    .left(px(x + 1.))
                    .top(px(3.))
                    .w(px((w - 2.).max(4.)))
                    .h(px(15.))
                    .px(px(7.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .bg(rgb(if current { 0x23232b } else { 0x18181e }))
                    .text_color(rgb(if current { TEXT } else { TEXT_3 }))
                    .text_size(px(10.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .cursor_grab()
                    .child(match self.name_field(Name::Marker(i)) {
                        Some(field) => div().w_full().child(field),
                        None => div().child(m.name.clone()),
                    })
                    .debug_selector(move || format!("marker-{i}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            // Double-click a marker to rename it.
                            if e.click_count == 2 {
                                ws.start_naming(Name::Marker(i), window, cx);
                                return;
                            }
                            let x = f32::from(e.position.x);
                            ws.begin_drag(
                                Drag::Marker {
                                    index: i,
                                    x,
                                    tick,
                                    moved: false,
                                },
                                cx,
                            );
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let mut items =
                                vec![("Rename…".to_string(), MenuAction::Rename(Name::Marker(i)))];
                            items.extend(MARKER_NAMES.iter().map(|n| {
                                (
                                    format!("Call it {n}"),
                                    MenuAction::NameMarker(i, n.to_string()),
                                )
                            }));
                            items.push(("Delete Marker".into(), MenuAction::DeleteMarker(i)));
                            ws.menu = Some(Menu {
                                at: e.position,
                                items,
                                ..Default::default()
                            });
                            cx.notify();
                        }),
                    )
            })
            .collect();
        let empty = markers.is_empty();
        div()
            .h(px(MARKERS_H))
            .flex_none()
            .relative()
            .overflow_hidden()
            .bg(rgb(WELL))
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .children(chips)
            .when(empty, |d| {
                d.child(
                    div()
                        .absolute()
                        .left(px(8.))
                        .top(px(4.))
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_4))
                        .child("Double-click to add a marker"),
                )
            })
            .debug_selector(|| "markers".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, window, cx| {
                    if e.click_count != 2 {
                        return;
                    }
                    let bar = ws.session.project.bar() as f64;
                    let tick = ((ws.tick_at(f32::from(e.position.x)) / bar).floor() * bar) as u64;
                    let n = ws.session.project.markers.len();
                    let name = MARKER_NAMES
                        .get(n)
                        .map_or(format!("Marker {}", n + 1), |s| s.to_string());
                    ws.choose(MenuAction::AddMarker(tick, name), cx);
                    // Then name it.
                    let markers = &ws.session.project.markers;
                    if let Some(i) = markers.iter().rposition(|m| m.tick == tick) {
                        ws.start_naming(Name::Marker(i), window, cx);
                    }
                }),
            )
            .on_scroll_wheel(self.on_scroll(cx))
    }

    pub(crate) fn ruler(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (ppt, scroll) = (self.px_per_tick(), self.scroll_x);
        let (beat, per_bar) = (
            self.session.project.beat(),
            self.session.project.beats_per_bar(),
        );
        let bar_px = ppt * (beat * per_bar) as f32;
        let width = f32::from(self.lanes_bounds.get().size.width).max(2000.0);
        // Label every bar, or fewer when they'd crowd.
        let every = (40.0 / bar_px).ceil().max(1.0) as u64;
        let first = (scroll / bar_px) as u64;
        let last = ((scroll + width) / bar_px) as u64 + 1;
        let labels = (first..=last).filter(|b| b % every == 0).map(|b| {
            div()
                .absolute()
                .left(px(b as f32 * bar_px - scroll + 4.))
                .top(px(11.))
                .font_family(MONO)
                .text_size(px(10.))
                .line_height(px(12.))
                .text_color(rgb(TEXT_3))
                .child(format!("{}", b + 1))
        });
        let playhead = self.playhead() as f32 * ppt - scroll;
        let cycle = self.session.project.cycle.map(|c| {
            let (x0, x1) = (c.start as f32 * ppt - scroll, c.end as f32 * ppt - scroll);
            div()
                .absolute()
                .left(px(x0))
                .top(px(2.))
                .w(px((x1 - x0).max(2.)))
                .h(px(8.))
                .rounded(px(3.))
                .bg(if c.off {
                    alpha(TEXT_4, 0.6)
                } else {
                    alpha(ACCENT, 0.9)
                })
        });
        div()
            .h(px(RULER_H))
            .flex_none()
            .relative()
            .overflow_hidden()
            .bg(rgb(WELL))
            .border_b_1()
            .border_color(rgb(LINE))
            .children(cycle)
            .child(
                canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        let beat_px = ppt * beat as f32;
                        let first = (scroll / beat_px) as u64;
                        let last = ((scroll + f32::from(b.size.width)) / beat_px) as u64 + 1;
                        for beat in first..=last {
                            let bar = beat % per_bar == 0;
                            if !bar && beat_px < 8.0 {
                                continue;
                            }
                            let x = b.origin.x + px((beat as f32 * beat_px - scroll).round());
                            let (top, color) = if bar {
                                (px(11.), LINE_STRONG)
                            } else {
                                (px(20.), LINE)
                            };
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(x, b.origin.y + top),
                                    size(px(1.), b.size.height - top),
                                ),
                                rgb(color),
                            ));
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .children(labels)
            .child(
                gpui::svg()
                    .data(PLAYHEAD)
                    .absolute()
                    .left(px(playhead - 5.5))
                    .top(px(13.))
                    .w(px(11.))
                    .h(px(12.))
                    .text_color(rgb(TEXT)),
            )
            .debug_selector(|| "ruler".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    // The ruler's top half is the cycle's: sweep it to loop a stretch, click it
                    // to turn the cycle on or off. The rest moves the playhead.
                    let top = f32::from(ws.lanes_bounds.get().origin.y) - RULER_H;
                    if f32::from(e.position.y) < top + RULER_H / 2.0 {
                        let anchor = ws.snap(ws.tick_at(f32::from(e.position.x)));
                        ws.begin_drag(
                            Drag::Cycle {
                                anchor,
                                moved: false,
                            },
                            cx,
                        );
                    } else {
                        ws.begin_drag(Drag::Locate, cx);
                        ws.locate_at(f32::from(e.position.x), cx);
                    }
                }),
            )
            .on_scroll_wheel(self.on_scroll(cx))
    }
}
