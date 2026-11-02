//! Under the tracks: following the playhead and zooming (under the headers), and the whole song
//! in miniature (under the lanes): every region in its track's colour, the cycle, the playhead
//! and the part on screen, which drags to scroll (a click elsewhere jumps there, a double-click
//! fits the whole song). And the tracks' scrollbar, at the lanes' right edge.

use super::*;

/// The navigation bar's height.
pub const NAV_H: f32 = 32.0;
/// The shortest the part on screen is drawn, so it can always be grabbed.
const THUMB_MIN: f32 = 18.0;

impl Workspace {
    /// The overview's pixels per tick, and where it was drawn.
    fn overview_scale(&self) -> (f32, Bounds<Pixels>) {
        let b = self.nav_bounds.get();
        (
            f32::from(b.size.width) / self.timeline_end().max(1) as f32,
            b,
        )
    }

    /// The part of the song on screen, in the overview: its x and width.
    fn overview_thumb(&self) -> (f32, f32) {
        let (scale, b) = self.overview_scale();
        let ppt = self.px_per_tick();
        let width = f32::from(self.lanes_bounds.get().size.width);
        let w = (width / ppt * scale).clamp(THUMB_MIN, f32::from(b.size.width).max(THUMB_MIN));
        (self.scroll_x / ppt * scale, w)
    }

    /// Scrolls so the part on screen starts at window x `x`, in the overview.
    pub(crate) fn overview_to(&mut self, x: f32) {
        let (scale, b) = self.overview_scale();
        let tick = (x - f32::from(b.origin.x)) / scale.max(1e-9);
        self.scroll_x = (tick * self.px_per_tick()).clamp(0.0, self.max_scroll_x());
        self.scrolled_by_hand();
    }

    /// Scrolls the tracks so their scrollbar's thumb starts at window y `y`.
    pub(crate) fn tracks_to(&mut self, y: f32) {
        let b = self.lanes_bounds.get();
        let height = f32::from(b.size.height).max(1.0);
        let content = self.rows_h() + LANE_H;
        let at = (y - f32::from(b.origin.y)) / height * content;
        self.scroll_y = at.clamp(0.0, self.max_scroll_y());
    }

    pub(crate) fn navigator(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let following = self.settings.follow_playhead && self.caught;
        let button = |i: Icon, sel: &'static str| {
            icon_seg(i, false, 22.)
                .debug_selector(move || sel.into())
                .flex_none()
        };
        let zoom = move |by: f32| {
            move |ws: &mut Workspace,
                  _: &MouseDownEvent,
                  _: &mut Window,
                  cx: &mut Context<Workspace>| {
                let width = f32::from(ws.lanes_bounds.get().size.width);
                ws.zoom_around(ws.px_per_beat * by, width / 2.0);
                ws.scroll_x = ws.scroll_x.min(ws.max_scroll_x());
                cx.notify();
            }
        };
        let controls = div()
            .w(px(self.header_w))
            .flex_none()
            .h_full()
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(4.))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                seg("Follow", following, 22.)
                    .debug_selector(|| "nav-follow".into())
                    .child(div().size(px(6.)).rounded(px(3.)).bg(rgb(match following {
                        true => ACCENT,
                        false => TEXT_4,
                    })))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            match following {
                                true => ws.settings.follow_playhead = false,
                                false => {
                                    (ws.settings.follow_playhead, ws.caught) = (true, true);
                                    if ws.player.playing() {
                                        ws.follow_playhead();
                                    }
                                }
                            }
                            ws.settings.save();
                            cx.notify();
                        }),
                    ),
            )
            .child(div().flex_1())
            .child(
                button(Icon::Minus, "nav-zoom-out")
                    .on_mouse_down(MouseButton::Left, cx.listener(zoom(1.0 / 1.5))),
            )
            .child(
                button(Icon::Plus, "nav-zoom-in")
                    .on_mouse_down(MouseButton::Left, cx.listener(zoom(1.5))),
            )
            .child(button(Icon::Expand, "nav-fit").on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    ws.zoom_to_fit();
                    cx.notify();
                }),
            ));

        // The song in miniature: a thin line a track, its regions in its colour.
        let p = &self.session.project;
        let end = self.timeline_end().max(1) as f32;
        let shown: Vec<usize> = (0..p.tracks.len()).filter(|&t| !self.folded(t)).collect();
        // Each region: its row, where it starts and how long it is (shares of the whole), colour.
        let lines: Vec<(f32, f32, f32, u32)> = shown
            .iter()
            .enumerate()
            .flat_map(|(row, &t)| {
                let color = self.color(t);
                p.tracks[t]
                    .regions
                    .iter()
                    .filter(|r| !r.mute)
                    .map(move |r| {
                        let (a, w) = (r.start as f32 / end, r.length as f32 / end);
                        (row as f32, a, w, color)
                    })
            })
            .collect();
        let rows = shown.len().max(1) as f32;
        let cycle = p
            .cycle
            .filter(|c| !c.off)
            .map(|c| (c.start as f32 / end, c.end as f32 / end));
        let playhead = self.playhead() as f32 / end;
        let bounds = self.nav_bounds.clone();
        let map = canvas(
            move |b, _, _| bounds.set(b),
            move |b, _, window, _| {
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                if let Some((a, z)) = cycle {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x + px(a * w), b.origin.y),
                            size(px(((z - a) * w).max(1.0)), b.size.height),
                        ),
                        alpha(SOLO, 0.12),
                    ));
                }
                let line = ((h - 6.0) / rows).clamp(1.0, 5.0);
                let gap = ((h - 6.0) - line * rows).max(0.0) / rows;
                for &(row, at, width, color) in &lines {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(
                                b.origin.x + px(at * w),
                                b.origin.y + px(3.0 + row * (line + gap) + gap / 2.0),
                            ),
                            size(px((width * w).max(1.5)), px(line)),
                        ),
                        alpha(color, 0.85),
                    ));
                }
                window.paint_quad(fill(
                    Bounds::new(
                        point(b.origin.x + px((playhead * w).round()), b.origin.y),
                        size(px(1.), b.size.height),
                    ),
                    rgb(TEXT),
                ));
            },
        )
        .absolute()
        .inset_0();
        let (x, w) = self.overview_thumb();
        let thumb = div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(x))
            .w(px(w))
            .rounded(px(4.))
            .border_1()
            .border_color(alpha(TEXT, 0.55))
            .bg(alpha(0xffffff, 0.07))
            .debug_selector(|| "nav-thumb".into());
        let overview = div()
            .flex_1()
            .min_w(px(0.))
            .h(px(NAV_H - 8.0))
            .mx(px(8.))
            .relative()
            .rounded(px(5.))
            .bg(rgb(LANE))
            .overflow_hidden()
            .cursor(gpui::CursorStyle::OpenHand)
            .debug_selector(|| "nav-overview".into())
            .child(map)
            .child(thumb)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    // Twice: the whole song.
                    if e.click_count == 2 {
                        ws.zoom_to_fit();
                        cx.notify();
                        return;
                    }
                    let px_x = f32::from(e.position.x);
                    let left = f32::from(ws.nav_bounds.get().origin.x);
                    let (x, w) = ws.overview_thumb();
                    // On the part on screen, it's held where it's grabbed; elsewhere it jumps
                    // there, centred, and is held in its middle.
                    let grab = match (left + x..left + x + w).contains(&px_x) {
                        true => px_x - left - x,
                        false => {
                            ws.overview_to(px_x - w / 2.0);
                            w / 2.0
                        }
                    };
                    ws.begin_drag(Drag::Overview { grab }, cx);
                }),
            )
            .on_scroll_wheel(self.on_scroll(cx));
        div()
            .h(px(NAV_H))
            .flex_none()
            .flex()
            .items_center()
            .bg(rgb(HEADERS))
            .border_t_1()
            .border_color(rgb(LINE))
            .child(controls)
            .child(overview)
    }

    /// The tracks' scrollbar, when there are more than fit: a thumb at the lanes' right edge.
    pub(crate) fn tracks_scrollbar(&self, cx: &mut Context<Self>) -> Option<Div> {
        let height = f32::from(self.lanes_bounds.get().size.height);
        let content = self.rows_h() + LANE_H;
        if height <= 0.0 || content <= height {
            return None;
        }
        let h = (height / content * height).max(24.0);
        let top = self.scroll_y / content * height;
        Some(
            div()
                .absolute()
                .right(px(2.))
                .top(px(top))
                .w(px(7.))
                .h(px(h))
                .rounded(px(4.))
                .bg(alpha(TEXT, 0.22))
                .hover(|d| d.bg(alpha(TEXT, 0.4)))
                .debug_selector(|| "tracks-scrollbar".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let y = f32::from(e.position.y) - f32::from(ws.lanes_bounds.get().origin.y);
                        ws.begin_drag(Drag::TrackScroll { grab: y - top }, cx);
                    }),
                ),
        )
    }
}
