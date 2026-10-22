//! The step sequencer laid out: the menu bar; the pane (browser or inspector); the pattern's
//! controls beside the grid's header (its name, an overview of each page, its key and scale, its
//! length); then every row, its header beside its steps, and its subrows under it.

use gpui::{Hsla, fill, point, size};
use pina_engine::song::project::{RATES, rate_name, row_step_at};

use super::*;

/// How a step is drawn, worked out before its element is.
struct StepLook {
    on: bool,
    inside: bool,
    skipped: bool,
    playing: bool,
    picked: bool,
    cursor: bool,
    /// Where its value stands (0 to 1, or -1 to 1 from the middle), and its text.
    level: Option<(f32, bool)>,
    text: Option<String>,
    /// Ratchet slices, a tie on to the next step, a chance's size.
    slices: u8,
    tied: Option<bool>,
    chance: Option<f32>,
    /// Edges of its row's loop, when the loop shows: (start, end).
    frame: Option<(bool, bool)>,
}

impl Workspace {
    /// The steps shown: the first, and how many (a page's worth).
    fn shown_steps(&mut self, n: usize) -> (usize, usize) {
        let width = self.steps.geom.borrow().width;
        let width = if width > 0.0 {
            width
        } else {
            (self.viewport_w - HEADER_W - 40.0).max(200.0)
        };
        let per_at = |w: f32| (((width + GAP) / (w + GAP)) as usize / 4 * 4).max(4);
        let per = match self.steps.wide {
            false => {
                let fits = per_at(STEP_MIN);
                if n <= fits { n } else { fits }
            }
            true => per_at(STEP_MAX).min(n),
        }
        .max(1);
        let pages = n.div_ceil(per);
        self.steps.page = self.steps.page.min(pages - 1);
        (self.steps.page * per, per.min(n - self.steps.page * per))
    }

    /// The pattern's step each row is on now, while it plays in its region.
    fn steps_now(&self, at: At, p: &Pattern) -> Vec<Option<usize>> {
        let r = self.session.project.region(at);
        let tick = self.playhead();
        let end = match r.looped {
            true => self.session.project.loop_end(at),
            false => r.start + r.length,
        };
        if !self.player.playing() || !(r.start..end).contains(&tick) {
            return vec![None; p.rows.len()];
        }
        let len = r.length.max(1);
        let pass = r.start + (tick - r.start) / len * len;
        let into = (tick - r.start) % len;
        (0..p.rows.len())
            .map(|k| row_step_at(p, k, into, pass))
            .collect()
    }

    /// The step sequencer, or what to make when there's no pattern.
    pub fn steps_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some((at, _)) = self.shown_pattern() else {
            return self.steps_none(cx).into_any_element();
        };
        let bar = self.steps_bar(cx);
        let pane = match self.steps.pane {
            Some(Pane::Browser) => Some(self.pattern_browser(cx).into_any_element()),
            Some(Pane::Inspector) => Some(self.steps_inspector(at, cx).into_any_element()),
            None => None,
        };
        let strip = self.pattern_strip(cx);
        let header = self.grid_header(at, cx);
        let rows = self.step_rows(at, cx);
        let geom = self.steps.geom.clone();
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .bg(rgb(LANE))
            .child(bar)
            .child(
                div().flex_1().min_h(px(0.)).flex().children(pane).child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .h(px(32.))
                                .flex_none()
                                .flex()
                                .bg(rgb(SURFACE))
                                .border_b_1()
                                .border_color(rgb(LINE))
                                .child(strip)
                                .child(header),
                        )
                        .child(
                            div()
                                .id("steps-rows")
                                .relative()
                                .flex_1()
                                .min_h(px(0.))
                                .overflow_y_scroll()
                                .child(
                                    gpui::canvas(
                                        move |b: Bounds<Pixels>, _, _| {
                                            let mut g = geom.borrow_mut();
                                            g.height = f32::from(b.size.height);
                                            g.width = (f32::from(b.size.width) - HEADER_W - 12.0)
                                                .max(0.0);
                                        },
                                        |_, _, _, _| {},
                                    )
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .size_full(),
                                )
                                .child(rows),
                        ),
                ),
            )
            .into_any_element()
    }

    /// Over the steps: the region's name, the pages, the key and scale, the length.
    fn grid_header(&mut self, at: At, cx: &mut Context<Self>) -> Div {
        let Some((_, p)) = self.shown_pattern() else {
            return div();
        };
        let p = &p.clone();
        let n = p.steps();
        let (first, per) = self.shown_steps(n);
        let pages = n.div_ceil(per.max(1));
        let region = self.session.project.region(at);
        let track = &self.session.project.tracks[at.0];
        let name = if region.name.is_empty() {
            track.name.clone()
        } else {
            region.name.clone()
        };
        let color = self.color(at.0);
        let looks = self.row_looks(at, p);
        let page = first / per.max(1);
        let drums = matches!(track.instrument, Some(Instrument::Drums))
            || track.instrument.as_ref().is_some_and(|i| i.is_kit());
        let (key, scale, out) = (p.key, p.scale, p.out_of_scale());
        // Each page in miniature: a dot a step that's on, in its row's colour.
        let minis: Vec<Div> = (0..pages)
            .filter(|_| pages > 1)
            .map(|k| {
                let cells: Vec<(usize, usize, Hsla)> = p
                    .rows
                    .iter()
                    .enumerate()
                    .take(10)
                    .flat_map(|(r, row)| {
                        let color = looks[r].color;
                        (k * per..((k + 1) * per).min(n))
                            .filter(|&i| row.on(i))
                            .map(move |i| (r, i - k * per, alpha(color, 1.0)))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                let rows_n = p.rows.len().clamp(1, 10);
                div()
                    .w(px(64.))
                    .h(px(20.))
                    .p(px(2.))
                    .rounded(px(3.))
                    .border_1()
                    .border_color(rgb(if k == page { 0xffffff } else { LINE_STRONG }))
                    .bg(rgb(BG))
                    .cursor_pointer()
                    .debug_selector(move || format!("steps-page-{k}"))
                    .child(
                        gpui::canvas(
                            |_, _, _| {},
                            move |b: Bounds<Pixels>, _, window, _| {
                                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                                let (cw, ch) = (w / per.max(1) as f32, h / rows_n as f32);
                                for &(r, i, c) in &cells {
                                    let o = point(
                                        b.origin.x + px(i as f32 * cw),
                                        b.origin.y + px(r as f32 * ch),
                                    );
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            o,
                                            size(px((cw - 0.5).max(0.8)), px((ch - 0.5).max(0.8))),
                                        ),
                                        c,
                                    ));
                                }
                            },
                        )
                        .size_full(),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.steps.page = k;
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            let items = ws.page_menu(k);
                            ws.open_steps_menu(e.position, items, cx);
                        }),
                    )
            })
            .collect();
        const KEYS: [&str; 12] = [
            "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
        ];
        let pitch = (!drums).then(|| {
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(
                    dropdown(KEYS[key as usize % 12], 22.)
                        .debug_selector(|| "steps-key".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                let items = ws.pattern_key_menu();
                                ws.open_steps_menu(e.position, items, cx);
                            }),
                        ),
                )
                .child(
                    dropdown(scale.map_or("Chromatic", |s| s.name()), 22.)
                        .debug_selector(|| "steps-scale".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                let items = ws.pattern_scale_menu();
                                ws.open_steps_menu(e.position, items, cx);
                            }),
                        ),
                )
                .child(
                    div()
                        .size(px(22.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .bg(rgb(if out { RAISED } else { CARD }))
                        .text_size(px(11.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(if out { TEXT } else { TEXT_4 }))
                        .debug_selector(|| "steps-conform".into())
                        .child("Q")
                        .when(out, |d| {
                            d.cursor_pointer().on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| ws.steps_op(StepsOp::Conform, cx)),
                            )
                        }),
                )
        });
        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(8.))
            .child(
                div()
                    .px(px(8.))
                    .h(px(22.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .bg(rgb(BG))
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(color))
                    .whitespace_nowrap()
                    .child(name),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .justify_center()
                    .gap(px(6.))
                    .children(minis),
            )
            .children(pitch)
            .child(
                dropdown(format!("{n} Steps"), 22.)
                    .debug_selector(|| "steps-length".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            ws.menu = Some(ws.length_menu(e.position));
                            cx.notify();
                        }),
                    ),
            )
    }

    /// Every row: its header beside its steps, and its subrows (if open) under it.
    fn step_rows(&mut self, at: At, cx: &mut Context<Self>) -> Div {
        let Some((_, p)) = self.shown_pattern() else {
            return div();
        };
        let p = p.clone();
        let n = p.steps();
        let (first, per) = self.shown_steps(n);
        // While playing, the page with the playhead's step on it.
        let now = self.steps_now(at, &p);
        if let Some(Some(s)) = now.get(self.steps.row).or(now.first())
            && per > 0
            && self.player.playing()
            && !(first..first + per).contains(s)
        {
            self.steps.page = s / per;
        }
        let (first, per) = self.shown_steps(n);
        {
            let mut g = self.steps.geom.borrow_mut();
            g.lines.clear();
            g.shown = (first, per);
        }
        let looks = self.row_looks(at, &p);
        // Rows' height: as zoomed, or to fill the editor.
        let lines: usize = p
            .rows
            .iter()
            .map(|r| 1 + if r.open { r.subrows.len() } else { 0 })
            .sum();
        let h = match self.steps.fit {
            true => {
                let room = self.steps.geom.borrow().height;
                (room / lines.max(1) as f32).clamp(ROW_MIN, ROW_MAX)
            }
            false => self.steps.row_h,
        };
        let mut out = Vec::new();
        for (r, row) in p.rows.iter().enumerate() {
            let look = &looks[r];
            let header = self.row_header(at, r, look, h, cx);
            let line = self.step_line(&p, r, None, look, (first, per), now[r], h, cx);
            out.push(div().flex().child(header).child(line));
            if row.open {
                for (k, &lane) in row.subrows.iter().enumerate() {
                    let header = self.subrow_header(r, k, lane, look.automation, h, cx);
                    let line = self.step_line(&p, r, Some(lane), look, (first, per), now[r], h, cx);
                    out.push(div().flex().child(header).child(line));
                }
            }
        }
        div().flex().flex_col().children(out)
    }

    /// How step `i` of row `r` is drawn on a line showing `mode`.
    #[allow(clippy::too_many_arguments)]
    fn step_look(
        &self,
        p: &Pattern,
        r: usize,
        sub: Option<Lane>,
        mode: EditMode,
        i: usize,
        playing: Option<usize>,
        show: bool,
    ) -> StepLook {
        let row = &p.rows[r];
        let (a, b) = row.loop_range(p.steps());
        let auto = row.is_automation();
        let tip = self
            .steps
            .tip
            .as_ref()
            .filter(|t| t.1 == (r, sub, i))
            .map(|t| t.0.clone());
        let mut look = StepLook {
            on: row.on(i),
            inside: (a..b).contains(&i),
            skipped: row.skip_at(i),
            playing: playing == Some(i),
            picked: self.steps.picked.contains(&(r, i)),
            cursor: self.steps.midi_in && self.steps.cursor == Some((r, i)),
            level: None,
            text: None,
            slices: 0,
            tied: None,
            chance: None,
            frame: None,
        };
        let lane = match mode {
            EditMode::Loop => {
                look.frame = (a..b).contains(&i).then_some((i == a, i + 1 == b));
                return look;
            }
            EditMode::OnOff => return look,
            EditMode::Lane(l) if !l.applies(auto) => return look,
            EditMode::Lane(l) => l,
        };
        let (lo, hi) = row.range(lane);
        let v = row.get(lane, i);
        let text = show.then(|| self.step_text(r, lane, i)).or(tip.clone());
        match lane {
            Lane::Tie => look.tied = Some(row.tie_at(i)),
            Lane::Skip => look.text = row.skip_at(i).then(|| "Skip".to_string()),
            Lane::Repeat => {
                look.slices = row.repeat_at(i);
                look.text = text;
            }
            Lane::Chance => {
                look.chance = Some(v / 100.0);
                look.text = text;
            }
            Lane::Offset => {
                look.level = Some((v / 50.0, true));
                look.text = text;
            }
            Lane::Rate => {
                look.text = Some(match row.rate_at(i) {
                    Some(t) => rate_name(t),
                    None => rate_name(p.row_rate(r)),
                });
                look.level = None;
            }
            _ => {
                look.level = Some(((v - lo) / (hi - lo).max(1e-6), false));
                look.text = text;
            }
        }
        if tip.is_some() {
            look.text = tip;
        }
        let _ = RATES;
        look
    }

    /// A line of steps: a row's (following the edit mode) or a subrow's (its own setting).
    #[allow(clippy::too_many_arguments)]
    fn step_line(
        &mut self,
        p: &Pattern,
        r: usize,
        sub: Option<Lane>,
        look: &RowLook,
        (first, per): (usize, usize),
        playing: Option<usize>,
        h: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let mode = sub.map_or(self.steps.mode, EditMode::Lane);
        let picked_row = r == self.steps.row;
        let show = sub.is_some()
            || match self.steps.values {
                ShowValues::AllRows => true,
                ShowValues::SelectedRow => picked_row,
                ShowValues::OnlyWhenEditing => false,
            };
        let color = look.color;
        let mute = p.rows[r].mute;
        let geom = self.steps.geom.clone();
        // Loop: one frame round the row's loop, as far as it shows, painted where the steps
        // fall.
        let frame = (mode == EditMode::Loop)
            .then(|| {
                let (a, b) = p.rows[r].loop_range(p.steps());
                let (a, b) = (a.max(first), b.min(first + per));
                (a < b).then_some((a, b))
            })
            .flatten()
            .map(|(a, b)| {
                gpui::canvas(
                    |_, _, _| {},
                    move |bounds: Bounds<Pixels>, _, window, _| {
                        let w = f32::from(bounds.size.width);
                        let cols = gestures::columns(first, per, w);
                        let x0 = cols[a - first].0;
                        let x1 = cols[b - 1 - first].0 + cols[b - 1 - first].1;
                        let o =
                            gpui::point(bounds.origin.x + px(x0 - 2.0), bounds.origin.y - px(2.0));
                        let size = gpui::size(px(x1 - x0 + 4.0), bounds.size.height + px(4.0));
                        window.paint_quad(gpui::quad(
                            Bounds::new(o, size),
                            px(6.),
                            gpui::transparent_black(),
                            px(2.),
                            rgb(0xffffff),
                            gpui::BorderStyle::Solid,
                        ));
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full()
            });
        let cells: Vec<Div> = (first..first + per)
            .map(|i| {
                let s = self.step_look(p, r, sub, mode, i, playing, show);
                // A value shows as its colour filled up to a line, darker above it.
                let valued = s.level.is_some_and(|l| !l.1);
                let base = match (s.on, s.inside, valued) {
                    (true, true, false) => alpha(color, 1.0),
                    (true, true, true) => alpha(color, 0.42),
                    (true, false, _) => alpha(color, 0.38),
                    (false, true, _) => alpha(color, 0.17),
                    (false, false, _) => alpha(color, 0.07),
                };
                let ink = if s.on { 0xffffff } else { color };
                let mut cell = div()
                    .flex_1()
                    .h_full()
                    .relative()
                    .rounded(px(4.))
                    .bg(base)
                    .when(mute || s.skipped, |d| d.opacity(0.4))
                    .when(i > first, |d| {
                        d.ml(px(if i % 4 == 0 { GROUP_GAP } else { GAP }))
                    })
                    .debug_selector(move || match sub {
                        Some(l) => format!("step-{r}-{l:?}-{i}"),
                        None => format!("step-{r}-{i}"),
                    });
                // Its value: a line where it stands, lighter under it.
                if let Some((u, bipolar)) = s.level {
                    let line = alpha(0xffffff, if s.on { 0.85 } else { 0.28 });
                    let under = match (s.on, s.inside) {
                        (true, true) => alpha(color, 1.0),
                        (true, false) => alpha(color, 0.5),
                        (false, _) => alpha(color, 0.16),
                    };
                    cell = match bipolar {
                        false => cell.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .bottom_0()
                                .h(gpui::relative(u.clamp(0.0, 1.0)))
                                .rounded_b(px(4.))
                                .bg(under)
                                .child(div().w_full().h(px(2.)).bg(line)),
                        ),
                        true => cell.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .top(gpui::relative(0.5 - u.max(0.0) * 0.5))
                                .h(gpui::relative(u.abs() * 0.5))
                                .when(u.abs() < 0.02, |d| d.h(px(2.)))
                                .bg(line),
                        ),
                    };
                }
                if s.slices > 1 {
                    cell = cell.child(div().absolute().inset_0().flex().children(
                        (0..s.slices).map(|j| {
                            div()
                                .flex_1()
                                .h_full()
                                .when(j > 0, |d| d.border_l_1().border_color(alpha(0x000000, 0.6)))
                        }),
                    ));
                } else if s.slices == 1 {
                    cell = cell.child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.))
                            .text_color(alpha(ink, 0.7))
                            .child("⋮"),
                    );
                }
                if let Some(c) = s.chance {
                    let side = gpui::relative((c.sqrt() * 0.7).clamp(0.05, 0.7));
                    cell = cell.child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .w(side)
                                    .h(side)
                                    .rounded(px(2.))
                                    .border_1()
                                    .border_dashed()
                                    .border_color(alpha(0xffffff, if s.on { 0.75 } else { 0.25 }))
                                    .bg(alpha(0xffffff, if s.on { 0.18 } else { 0.05 })),
                            ),
                    );
                }
                if let Some(tied) = s.tied {
                    cell = cell.child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .right(px(3.))
                            .flex()
                            .items_center()
                            .child(
                                icon(Icon::Play, 11.)
                                    .text_color(alpha(0xffffff, if tied { 0.95 } else { 0.18 })),
                            ),
                    );
                    if tied {
                        // Joined to the next step across the gap.
                        cell = cell.child(
                            div()
                                .absolute()
                                .top(gpui::relative(0.42))
                                .h(gpui::relative(0.16))
                                .right(px(-(GROUP_GAP + 1.0)))
                                .w(px(GROUP_GAP + 2.0))
                                .bg(alpha(color, 0.9)),
                        );
                    }
                }
                if let Some(t) = s.text.filter(|t| !t.is_empty()) {
                    cell = cell.child(
                        div()
                            .absolute()
                            .left(px(4.))
                            .bottom(px(2.))
                            .right(px(2.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .font_family(MONO)
                            .text_size(px(10.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(alpha(ink, if s.on { 0.95 } else { 0.55 }))
                            .child(t),
                    );
                }
                if s.frame.is_none() && (s.playing || s.picked || s.cursor) {
                    cell = cell.border_2().border_color(rgb(match () {
                        _ if s.picked => ACCENT_TEXT,
                        _ if s.cursor => AMBER,
                        _ => 0xffffff,
                    }));
                }
                cell.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let p = crate::workspace::pt(e.position);
                        let across = ws.step_under(p).map_or(0.5, |h| h.3);
                        ws.press_step(r, sub, i, across, e.modifiers, p.y, cx);
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        ws.steps.row = r;
                        if !ws.steps.picked.contains(&(r, i)) {
                            ws.steps.picked = vec![(r, i)];
                        }
                        let items = ws.step_menu(r, i);
                        ws.open_steps_menu(e.position, items, cx);
                    }),
                )
            })
            .collect();
        div()
            .flex_1()
            .min_w(px(0.))
            .h(px(h))
            .px(px(6.))
            .py(px(3.))
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .bg(rgb(if picked_row { 0x111116 } else { LANE }))
            .child(
                div()
                    .relative()
                    .size_full()
                    .flex()
                    .child(
                        gpui::canvas(
                            move |b: Bounds<Pixels>, _, _| {
                                geom.borrow_mut().lines.insert((r, sub), b);
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
                    .children(cells)
                    .children(frame),
            )
    }
}
