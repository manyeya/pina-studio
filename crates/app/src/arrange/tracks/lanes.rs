//! The lanes: the grid, a row per track, and writing a new region in one.

use super::*;

impl Workspace {
    pub(crate) fn lanes(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (ppt, sx, sy) = (self.px_per_tick(), self.scroll_x, self.scroll_y);
        let p = &self.session.project;
        let (beat_ticks, per_bar, bar_ticks) = (p.beat(), p.beats_per_bar(), p.bar());
        let tracks = self.session.project.tracks.len();
        let selected: Vec<usize> = self.picked_tracks();
        let automation = self.show_automation;
        // Each row's top and its lane's height, for the grid (none, folded in a stack).
        let rows_at: Vec<(f32, f32, f32)> = (0..tracks)
            .map(|i| match self.folded(i) {
                true => (self.row_top(i), 0.0, 0.0),
                false => (self.row_top(i), self.lane_h(i), self.autos_h(i)),
            })
            .collect();
        // Each stack's lane: its tracks' regions, a line each, in their colours.
        // (Its top, a line's height, and each region's x, top, width and colour.)
        type Bars = Vec<(f32, f32, f32, u32)>;
        let mut stacks: Vec<(f32, f32, Bars)> = Vec::new();
        for first in 0..tracks {
            let Some(a) = self.stack_above(first) else {
                continue;
            };
            let bus = p.auxes[a].bus;
            let members: Vec<usize> = (0..tracks)
                .filter(|&i| p.tracks[i].output == Some(bus))
                .collect();
            let line = (STACK_H - 10.0) / members.len().max(1) as f32;
            let bars = members
                .iter()
                .enumerate()
                .flat_map(|(m, &i)| {
                    let color = self.color(i);
                    p.tracks[i]
                        .regions
                        .iter()
                        .filter(|r| !r.mute)
                        .map(move |r| {
                            let x = r.start as f32 * ppt - sx;
                            (x, 5.0 + m as f32 * line, r.length as f32 * ppt, color)
                        })
                })
                .collect();
            stacks.push((self.row_top(first) - STACK_H, (line - 1.0).max(1.5), bars));
        }
        let auto_h = self.auto_h();
        let bounds = self.lanes_bounds.clone();
        let cycle = self
            .session
            .project
            .cycle
            .filter(|c| !c.off)
            .map(|c| (c.start as f32 * ppt - sx, c.end as f32 * ppt - sx));
        let (shading, beat_lines) = (self.settings.bar_shading, self.settings.beat_lines);
        let grid = canvas(
            move |b, _, _| bounds.set(b),
            move |b, _, window, _| {
                let w = f32::from(b.size.width);
                let quad = |window: &mut Window, x: f32, y: f32, ww: f32, hh: f32, c: Hsla| {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x + px(x), b.origin.y + px(y)),
                            size(px(ww), px(hh)),
                        ),
                        c,
                    ));
                };
                // Every other four bars a shade lighter.
                let group = ppt * (bar_ticks * 4) as f32;
                let mut k = (sx / group).floor() as i64;
                while shading && (k as f32 * group - sx) < w {
                    if k % 2 == 1 {
                        quad(
                            window,
                            k as f32 * group - sx,
                            0.0,
                            group,
                            f32::from(b.size.height),
                            rgb(LANE_ALT).into(),
                        );
                    }
                    k += 1;
                }
                if let Some((x0, x1)) = cycle {
                    quad(
                        window,
                        x0,
                        0.0,
                        x1 - x0,
                        f32::from(b.size.height),
                        alpha(ACCENT, 0.04),
                    );
                }
                // Stacks' lanes, a shade darker (their tracks' regions go on them over the grid).
                for (y, _, _) in &stacks {
                    quad(window, 0.0, *y, w, STACK_H, rgb(0x0e0e12).into());
                    quad(
                        window,
                        0.0,
                        y + STACK_H - 1.0,
                        w,
                        1.0,
                        rgb(LINE_SOFT).into(),
                    );
                }
                for (i, &(y, lh, ah)) in rows_at.iter().enumerate() {
                    if selected.contains(&i) {
                        quad(window, 0.0, y, w, lh, alpha(0xffffff, 0.025));
                    }
                    let row_h = lh + ah;
                    if automation {
                        quad(window, 0.0, y + lh, w, ah, rgb(0x0b0b0e).into());
                        quad(window, 0.0, y + lh - 1.0, w, 1.0, rgb(LINE_SOFT).into());
                        let mut k = auto_h;
                        while k < ah - 1.0 {
                            quad(window, 0.0, y + lh + k - 1.0, w, 1.0, rgb(0x17171c).into());
                            k += auto_h;
                        }
                    }
                    quad(window, 0.0, y + row_h - 1.0, w, 1.0, rgb(LINE_SOFT).into());
                }
                let _ = sy;
                let beat_px = ppt * beat_ticks as f32;
                let first = (sx / beat_px) as u64;
                let last = ((sx + w) / beat_px) as u64 + 1;
                for beat in first..=last {
                    let bar = beat % per_bar == 0;
                    if !bar && (beat_px < 10.0 || !beat_lines) {
                        continue;
                    }
                    let color = if beat % (per_bar * 4) == 0 {
                        0x2a2a32
                    } else if bar {
                        0x202027
                    } else {
                        0x16161b
                    };
                    let x = (beat as f32 * beat_px - sx).round();
                    quad(
                        window,
                        x,
                        0.0,
                        1.0,
                        f32::from(b.size.height),
                        rgb(color).into(),
                    );
                }
                for (y, line, bars) in &stacks {
                    for &(x, top, bw, color) in bars {
                        if x + bw < 0.0 || x > w {
                            continue;
                        }
                        quad(window, x, y + top, bw.max(1.0), *line, alpha(color, 0.7));
                    }
                }
            },
        )
        .absolute()
        .size_full();

        // Clicking an empty lane selects its track; the pencil draws a region there.
        let rows: Vec<Div> = (0..tracks)
            .filter(|&i| !self.folded(i))
            .map(|i| {
                div()
                    .absolute()
                    .top(px(self.row_top(i)))
                    .left_0()
                    .w_full()
                    .h(px(self.lane_h(i)))
                    .when(self.tool == Tool::Pencil, |d| d.cursor_crosshair())
                    .debug_selector(move || format!("lane-{i}"))
                    // An empty instrument track says how to start.
                    .when(
                        self.session.project.tracks[i].instrument.is_some()
                            && self.session.project.tracks[i].regions.is_empty()
                            && self.settings.double_click_writes,
                        |d| {
                            d.child(
                                div()
                                    .absolute()
                                    .left(px(12.))
                                    .top_0()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_4))
                                    .child("Double-click to write notes, or play your keyboard"),
                            )
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            ws.select_track(i);
                            ws.region = None;
                            if ws.tool == Tool::Zoom {
                                return ws.zoom_press(e, cx);
                            }
                            // The pencil draws a region; so does a double-click, opening the
                            // piano roll on it to write in.
                            let x = f32::from(e.position.x);
                            if ws.tool == Tool::Pencil {
                                ws.draw_region(i, x, cx);
                            } else if e.click_count == 2
                                && ws.settings.double_click_writes
                                && ws.draw_region(i, x, cx).is_some()
                            {
                                ws.show_bottom(Bottom::PianoRoll, cx);
                            }
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            ws.select_track(i);
                            ws.region = None;
                            let tick = ws.snap(ws.tick_at(f32::from(e.position.x)));
                            ws.lane_menu(i, tick, e.position, cx);
                        }),
                    )
            })
            .collect();
        let width = f32::from(self.lanes_bounds.get().size.width).max(2000.0);
        let mut regions = Vec::new();
        for (t, track) in self.session.project.tracks.iter().enumerate() {
            if self.folded(t) {
                continue;
            }
            for r in 0..track.regions.len() {
                let region = &track.regions[r];
                let x = region.start as f32 * ppt - sx;
                let w = region.length as f32 * ppt;
                if x + w >= 0.0 && x <= width {
                    regions.push(self.region_view((t, r), cx));
                }
                regions.extend(self.loop_views((t, r), cx));
            }
        }

        let playhead = self.playhead() as f32 * ppt - sx;
        let lines = if automation {
            self.auto_views(cx)
        } else {
            Vec::new()
        };
        // The take being recorded, growing with the playhead.
        let take = self.recording.as_ref().and_then(|rec| {
            let t = self
                .session
                .project
                .tracks
                .iter()
                .position(|t| t.id == rec.track())?;
            let x = rec.start() as f32 * ppt - sx;
            Some(
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(self.row_top(t) + 4.))
                    .w(px((playhead - x).max(2.)))
                    .h(px(self.lane_h(t) - 8.))
                    .rounded(px(6.))
                    .bg(alpha(RECORD, 0.3))
                    .border_1()
                    .border_color(rgb(RECORD)),
            )
        });
        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .relative()
            .overflow_hidden()
            .bg(rgb(LANE))
            .child(grid)
            .children(rows)
            .children(regions)
            .children(take)
            .children(lines)
            .children(self.agent_marker())
            // The Zoom tool's drag: the bars it'll fill the window with.
            .children(self.zoom_shown().map(|(x, w)| {
                div()
                    .absolute()
                    .top_0()
                    .left(px(x))
                    .w(px(w))
                    .h_full()
                    .bg(alpha(ACCENT, 0.12))
                    .border_l_1()
                    .border_r_1()
                    .border_color(alpha(ACCENT, 0.7))
            }))
            .children(self.sample_drop_view())
            .children(self.file_drop_view(cx))
            .when(tracks == 0, |d| {
                d.child(
                    div()
                        .absolute()
                        .top(px(40.))
                        .left(px(40.))
                        .text_color(rgb(TEXT_3))
                        .child("No tracks yet. Add one with + or ⌥⌘N."),
                )
            })
            .child(
                div()
                    .absolute()
                    .left(px(playhead))
                    .top_0()
                    .w(px(1.))
                    .h_full()
                    .bg(rgb(TEXT)),
            )
            .on_scroll_wheel(self.on_scroll(cx))
    }

    /// Where the agent is working, while it is: an outline round what it touched, and its cursor.
    pub(crate) fn agent_marker(&self) -> Vec<AnyElement> {
        let Some(a) = self.agent_presence() else {
            return Vec::new();
        };
        let Some(t) = a.track.filter(|&t| t < self.session.project.tracks.len()) else {
            return Vec::new();
        };
        let ppt = self.px_per_tick();
        let top = self.row_top(t);
        let width = f32::from(self.lanes_bounds.get().size.width).max(200.0);
        let (x, w, y, h) = match a
            .region
            .and_then(|r| self.session.project.tracks[t].regions.get(r))
        {
            Some(r) => (
                r.start as f32 * ppt - self.scroll_x - 1.0,
                r.length as f32 * ppt + 2.0,
                top + 2.0,
                self.lane_h(t) - 4.0,
            ),
            None => (0.0, width, top, self.lane_h(t)),
        };
        let fade =
            1.0 - (a.at.elapsed().as_secs_f32() / crate::workspace::PRESENCE.as_secs_f32()).powi(3);
        let cx_ = (x + w - 34.0).clamp(4.0, width - 110.0);
        let cy = y + h / 2.0;
        vec![
            div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(w))
                .h(px(h))
                .rounded(px(8.))
                .border_2()
                .border_color(alpha(AGENT, fade))
                .into_any_element(),
            gpui::svg()
                .data(CURSOR)
                .absolute()
                .left(px(cx_))
                .top(px(cy - 4.))
                .size(px(16.))
                .text_color(alpha(AGENT, fade))
                .into_any_element(),
            div()
                .absolute()
                .left(px(cx_ + 12.))
                .top(px(cy + 10.))
                .px(px(8.))
                .py(px(3.))
                .rounded(px(7.))
                .rounded_tl(px(2.))
                .bg(alpha(AGENT, fade))
                .text_color(rgb(BG))
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .whitespace_nowrap()
                .child("Pina Studio agent")
                .into_any_element(),
        ]
    }

    /// The pencil's click on an empty lane: a MIDI region over the bar clicked, on an instrument
    /// track.
    /// Where a new MIDI region for `tick` on `track` goes: the cycle's stretch if it's on and
    /// the tick is in it, else four bars from the tick's bar; kept clear of the track's other
    /// regions. None if one's there already.
    pub fn new_region_span(&self, track: usize, tick: u64) -> Option<(u64, u64)> {
        let p = &self.session.project;
        let bar = p.bar();
        let regions = &p.tracks.get(track)?.regions;
        if regions
            .iter()
            .any(|r| r.start <= tick && tick < r.start + r.length)
        {
            return None;
        }
        let (start, end) = match p
            .cycle
            .filter(|c| !c.off && c.start <= tick && tick < c.end)
        {
            Some(c) => (c.start, c.end),
            None => (tick / bar * bar, tick / bar * bar + 4 * bar),
        };
        let start = regions
            .iter()
            .map(|r| r.start + r.length)
            .filter(|&e| e <= tick)
            .fold(start, u64::max);
        let end = regions
            .iter()
            .map(|r| r.start)
            .filter(|&s| s > tick)
            .fold(end, u64::min);
        (end > start).then_some((start, end - start))
    }

    /// A new, empty MIDI region at `x` on an instrument track (see `new_region_span`),
    /// selected.
    pub fn draw_region(&mut self, track: usize, x: f32, cx: &mut Context<Self>) -> Option<At> {
        if self.session.project.tracks[track].instrument.is_none() {
            self.status = Some(
                "Audio tracks take recordings and audio files: drag one in from the Finder or the browser.".into(),
            );
            return None;
        }
        let (start, length) = self.new_region_span(track, self.tick_at(x) as u64)?;
        self.edit(cx, |ws| {
            let regions = &mut ws.session.project.tracks[track].regions;
            regions.push(Region {
                start,
                length,
                content: Content::Midi {
                    notes: Vec::new(),
                    controls: Vec::new(),
                },
                ..Default::default()
            });
            ws.region = Some((track, regions.len() - 1));
        });
        self.region
    }
}
