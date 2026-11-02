//! The tempo lane under the ruler, the tempo's menu, and audio keeping time with the song: its
//! tempo found by listening, then following the song, or the song adapting to it, as the song's
//! tempo mode (Keep, Adapt, Auto) says for audio recorded or brought in.

use gpui::{
    Bounds, Div, FontWeight, MouseButton, MouseDownEvent, PathBuilder, Pixels, Point, canvas, div,
    fill, point, prelude::*, px, rgb, size,
};
use pina_engine::{
    audio::engine::read_audio,
    song::project::{At, Content, Follow, MAX_BPM, MIN_BPM, TempoMode},
    study::timing::{Found, find},
};

use crate::{
    ui::{menu::MenuAction, theme::*},
    workspace::{Drag, Workspace, pt},
};

/// The tempo lane's height.
pub const TEMPO_H: f32 = 44.0;

/// Something the tempo's menus and buttons do.
#[derive(Clone, Debug, PartialEq)]
pub enum TempoOp {
    Mode(TempoMode),
    /// The tempo lane shown or hidden.
    Lane(bool),
    /// One tempo again: the tempo changes taken out.
    Steady,
    Follow(At, Follow),
    /// Its tempo found by listening.
    Find(At),
    /// The song keeps time with it: bar by bar, or steady at its average.
    Adapt(At, bool),
    /// The tempo set so it fills the cycle.
    Fit(At),
    /// Its own tempo, times this (2, ½).
    Times(At, f64),
    Delete(u64),
}

/// What happens once a region's tempo is found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum After {
    Nothing,
    Follow(Follow),
    Adapt(bool),
    /// As the song's tempo mode says for audio just recorded or brought in.
    Mode,
}

impl Workspace {
    /// Whether the tempo lane shows: as asked, else when the tempo changes.
    pub fn tempo_lane_shown(&self) -> bool {
        self.tempo_lane
            .unwrap_or(!self.session.project.tempos.is_empty())
    }

    /// The tempo's change at or before a tick (0, the song's own, if none).
    pub fn tempo_change_at(&self, tick: u64) -> u64 {
        let p = &self.session.project;
        p.tempos
            .iter()
            .take_while(|c| c.tick <= tick)
            .last()
            .map_or(0, |c| c.tick)
    }

    /// Sets the tempo from a change on (0: the song's own; with no changes, the whole song's).
    pub fn set_tempo_from(&mut self, at: u64, bpm: f64) {
        let p = &mut self.session.project;
        match at == 0 && p.tempos.is_empty() {
            true => p.set_tempo(bpm),
            false => p.set_tempo_at(at, bpm),
        }
    }

    /// The lowest and highest tempos the lane spans, with room either side.
    fn tempo_range(&self) -> (f64, f64) {
        let p = &self.session.project;
        let all = std::iter::once(p.tempo).chain(p.tempos.iter().map(|c| c.bpm));
        let (lo, hi) = all.fold((f64::MAX, f64::MIN), |(lo, hi), b| (lo.min(b), hi.max(b)));
        let pad = ((hi - lo) * 0.2).max(8.0);
        ((lo - pad).max(MIN_BPM), (hi + pad).min(MAX_BPM))
    }

    /// The tempo's menu: how audio coming in treats it, the lane, one tempo again.
    pub fn tempo_menu(&self, at: Point<Pixels>) -> crate::ui::menu::Menu {
        let p = &self.session.project;
        let op = |label: String, o: TempoOp| (label, MenuAction::Tempo(o));
        let tick = |on: bool| if on { "✓" } else { "  " };
        let mut items = vec![(
            "Audio recorded or brought in:".to_string(),
            MenuAction::Nothing,
        )];
        for (m, what) in [
            (TempoMode::Keep, "the tempo stays; loops follow it"),
            (TempoMode::Adapt, "the song takes the audio's tempo"),
            (TempoMode::Auto, "adapts until something keeps time"),
        ] {
            items.push(op(
                format!("{} {} — {what}", tick(p.tempo_mode == m), m.name()),
                TempoOp::Mode(m),
            ));
        }
        items.push((String::new(), MenuAction::Nothing));
        let shown = self.tempo_lane_shown();
        items.push(op(
            format!("{} Tempo Lane\tG", if shown { "Hide" } else { "Show" }),
            TempoOp::Lane(!shown),
        ));
        if !p.tempos.is_empty() {
            items.push(op(
                format!("One Tempo Throughout ({} changes out)", p.tempos.len()),
                TempoOp::Steady,
            ));
        }
        crate::ui::menu::Menu {
            at,
            items,
            ..Default::default()
        }
    }

    /// The tempo lane's row: its name, the tempo where the playhead is and the mode on the
    /// left, the tempo as steps under the ruler.
    pub(crate) fn tempo_row(&mut self, cx: &mut Context<Self>) -> Div {
        let p = &self.session.project;
        let now = p.tempo_at(self.playhead());
        let mode = p.tempo_mode;
        let head = div()
            .w(px(self.header_w))
            .flex_none()
            .h_full()
            .flex()
            .items_center()
            .gap(px(8.))
            .pl(px(12.))
            .pr(px(8.))
            .bg(rgb(SURFACE))
            .border_b_1()
            .border_r_1()
            .border_color(rgb(LINE))
            .child(icon(Icon::Metronome, 13.).text_color(rgb(TEXT_3)))
            .child(
                div()
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT_2))
                    .child("Tempo"),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(format!("{now:.2}")),
            )
            .child(div().flex_1())
            .child(
                dropdown(mode.name(), 22.)
                    .debug_selector(|| "tempo-mode".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            ws.menu = Some(ws.tempo_menu(e.position));
                            cx.notify();
                        }),
                    ),
            )
            .child(
                icon_seg(Icon::Close, false, 22.)
                    .debug_selector(|| "tempo-hide".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.tempo_op(TempoOp::Lane(false), cx)),
                    ),
            );
        div()
            .h(px(TEMPO_H))
            .flex_none()
            .flex()
            .child(head)
            .child(self.tempo_lane(cx))
    }

    /// The song's tempo as steps: a line at each tempo, a point at each change (drag it in time
    /// and tempo, double-click it out), a stretch dragged up or down; double-click to add one.
    fn tempo_lane(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (ppt, sx) = (self.px_per_tick(), self.scroll_x);
        let p = &self.session.project;
        let (lo, hi) = self.tempo_range();
        let spans: Vec<(u64, f64)> = std::iter::once((0, p.tempo))
            .chain(p.tempos.iter().map(|c| (c.tick, c.bpm)))
            .collect();
        let area = self.tempo_area.clone();
        let y_of = move |bpm: f64, h: f32| 5.0 + ((hi - bpm) / (hi - lo)) as f32 * (h - 10.0);
        let line = spans.clone();
        let steps = canvas(
            move |b, _, _| area.set(b),
            move |b, _, window, _| {
                let h = f32::from(b.size.height);
                let x = |tick: u64| f32::from(b.origin.x) + tick as f32 * ppt - sx;
                let right = f32::from(b.origin.x + b.size.width);
                let mut path = PathBuilder::stroke(px(1.5));
                for (i, &(tick, bpm)) in line.iter().enumerate() {
                    let (x0, y) = (
                        x(tick).max(f32::from(b.origin.x)),
                        f32::from(b.origin.y) + y_of(bpm, h),
                    );
                    let x1 = line.get(i + 1).map_or(right, |n| x(n.0)).min(right);
                    if x1 < f32::from(b.origin.x) {
                        continue;
                    }
                    // Under the line, a little of its colour.
                    window.paint_quad(fill(
                        Bounds::new(
                            point(px(x0), px(y)),
                            size(px((x1 - x0).max(0.0)), b.origin.y + b.size.height - px(y)),
                        ),
                        alpha(ACCENT, 0.08),
                    ));
                    match i {
                        0 => path.move_to(point(px(x0), px(y))),
                        _ => path.line_to(point(px(x0), px(y))),
                    }
                    path.line_to(point(px(x1.max(x0)), px(y)));
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, rgb(ACCENT));
                }
            },
        )
        .absolute()
        .size_full();
        let h = TEMPO_H - 1.0;
        let points: Vec<Div> = spans
            .iter()
            .enumerate()
            .map(|(i, &(tick, bpm))| {
                let x = tick as f32 * ppt - sx;
                let y = y_of(bpm, h);
                let label = div()
                    .absolute()
                    .left(px(x + 6.))
                    .top(px((y - 13.).clamp(1.0, h - 12.)))
                    .font_family(MONO)
                    .text_size(px(9.5))
                    .text_color(rgb(TEXT_3))
                    .child(format!("{bpm:.1}").trim_end_matches(".0").to_string());
                let dot = div()
                    .absolute()
                    .left(px(x - 4.))
                    .top(px(y - 4.))
                    .size(px(8.))
                    .rounded(px(4.))
                    .bg(rgb(BG))
                    .border_2()
                    .border_color(rgb(ACCENT))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(ACCENT)))
                    .debug_selector(move || format!("tempo-point-{i}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.click_count == 2 && tick > 0 {
                                return ws.tempo_op(TempoOp::Delete(tick), cx);
                            }
                            let at = pt(e.position);
                            let range = ws.tempo_range();
                            let drag = match tick {
                                // The song's own tempo: only up and down.
                                0 => Drag::TempoLine {
                                    at: 0,
                                    y: at.y,
                                    bpm,
                                    range,
                                    moved: false,
                                },
                                _ => Drag::TempoPoint {
                                    tick,
                                    now: tick,
                                    x: at.x,
                                    y: at.y,
                                    bpm,
                                    range,
                                    moved: false,
                                },
                            };
                            ws.begin_drag(drag, cx);
                        }),
                    );
                div().child(label).child(dot)
            })
            .collect();
        div()
            .id("tempo-lane")
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .relative()
            .overflow_hidden()
            .bg(rgb(LANE))
            .border_b_1()
            .border_color(rgb(LINE))
            .debug_selector(|| "tempo-lane".into())
            .child(steps)
            .children(points)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    let at = pt(e.position);
                    let b = ws.tempo_area.get();
                    let h = f32::from(b.size.height);
                    let (lo, hi) = ws.tempo_range();
                    let y = at.y - f32::from(b.origin.y);
                    let bpm = (hi - ((y - 5.0) / (h - 10.0)) as f64 * (hi - lo))
                        .round()
                        .clamp(MIN_BPM, MAX_BPM);
                    let tick = ws.snap(ws.tick_at(at.x));
                    // Double-click: a tempo change there, at the tempo where it was clicked.
                    if e.click_count == 2 {
                        ws.edit(cx, |ws| ws.session.project.set_tempo_at(tick.max(1), bpm));
                        return;
                    }
                    // Near a stretch's line: that stretch's tempo, dragged up or down.
                    let from = ws.tempo_change_at(tick);
                    let now = ws.session.project.tempo_at(tick);
                    let line = 5.0 + ((hi - now) / (hi - lo)) as f32 * (h - 10.0);
                    if (y - line).abs() <= 6.0 {
                        ws.begin_drag(
                            Drag::TempoLine {
                                at: from,
                                y: at.y,
                                bpm: now,
                                range: (lo, hi),
                                moved: false,
                            },
                            cx,
                        );
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    ws.menu = Some(ws.tempo_menu(e.position));
                    cx.notify();
                }),
            )
            .on_scroll_wheel(self.on_scroll(cx))
    }

    /// A tempo point or stretch dragged: `range` is the lane's tempos when it was picked up, so
    /// it moves as far as the pointer does.
    pub(crate) fn drag_tempo(&mut self, p: Point<f32>) {
        let ppt = self.px_per_tick();
        let h = (f32::from(self.tempo_area.get().size.height) - 10.0).max(1.0);
        let fine = self.fine;
        let tempo = |bpm: f64, y: f32, range: (f64, f64)| {
            let v = bpm + (y - p.y) as f64 * (range.1 - range.0) / h as f64;
            let v = match fine {
                true => (v * 10.0).round() / 10.0,
                false => v.round(),
            };
            v.clamp(MIN_BPM, MAX_BPM)
        };
        match self.drag {
            Some(Drag::TempoLine {
                at, y, bpm, range, ..
            }) => {
                let to = tempo(bpm, y, range);
                self.set_tempo_from(at, to);
            }
            Some(Drag::TempoPoint {
                tick,
                now,
                x,
                y,
                bpm,
                range,
                ..
            }) => {
                let to_bpm = tempo(bpm, y, range);
                let to = self.snap(tick as f64 + ((p.x - x) / ppt) as f64).max(1);
                let project = &mut self.session.project;
                project.delete_tempo(now);
                project.set_tempo_at(to, to_bpm);
                if let Some(Drag::TempoPoint { now, .. }) = &mut self.drag {
                    *now = to;
                }
            }
            _ => {}
        }
    }

    /// What the tempo's menus and buttons do.
    pub fn tempo_op(&mut self, op: TempoOp, cx: &mut Context<Self>) {
        let grid = |ws: &Self, at: At| {
            matches!(
                ws.session
                    .project
                    .tracks
                    .get(at.0)
                    .and_then(|t| t.regions.get(at.1))
                    .map(|r| &r.content),
                Some(Content::Audio { grid: Some(_), .. })
            )
        };
        let own = |ws: &Self, at: At| match ws
            .session
            .project
            .tracks
            .get(at.0)
            .and_then(|t| t.regions.get(at.1))
            .map(|r| &r.content)
        {
            Some(Content::Audio { tempo, .. }) => *tempo,
            _ => None,
        };
        match op {
            TempoOp::Mode(m) => self.edit(cx, |ws| ws.session.project.tempo_mode = m),
            TempoOp::Lane(on) => self.tempo_lane = Some(on),
            TempoOp::Steady => self.edit(cx, |ws| ws.session.project.tempos.clear()),
            TempoOp::Delete(tick) => self.edit(cx, |ws| ws.session.project.delete_tempo(tick)),
            // Locking needs its beats, found first if not known. Following without a tempo of
            // its own takes the one it plays at where it is. Off gives back how it played before
            // it followed; Off again, as it was recorded.
            TempoOp::Follow(at, f) => {
                let needs = matches!(f, Follow::Bars | Follow::Beats) && !grid(self, at);
                let off = matches!(
                    self.session.project.region(at).content,
                    Content::Audio {
                        follow: Follow::Off,
                        ..
                    }
                );
                match needs {
                    true => self.find_tempo(at, After::Follow(f), cx),
                    false if f.is_off() && off => {
                        self.edit(cx, |ws| ws.session.project.unstretch(at));
                        self.status = Some("As recorded: not stretched".into());
                    }
                    false => self.edit(cx, |ws| ws.session.project.set_follow(at, f)),
                }
            }
            TempoOp::Find(at) => self.find_tempo(at, After::Nothing, cx),
            TempoOp::Adapt(at, steady) => match grid(self, at) {
                false => self.find_tempo(at, After::Adapt(steady), cx),
                true => self.adapt(at, steady, cx),
            },
            TempoOp::Fit(at) => {
                let Some(c) = self.session.project.cycle.filter(|c| c.end > c.start) else {
                    self.status = Some("Sweep a cycle in the ruler to fit it to.".into());
                    return cx.notify();
                };
                let mut said = Ok(0.0);
                self.edit(cx, |ws| {
                    said = ws.session.project.fit_tempo(at, c.start, c.end)
                });
                self.status = Some(match said {
                    Ok(bpm) => format!("The tempo is {bpm:.2} BPM: it fills the cycle"),
                    Err(e) => e,
                });
            }
            TempoOp::Times(at, k) => match own(self, at) {
                Some(t) => self.edit(cx, |ws| ws.session.project.set_audio_tempo(at, t * k)),
                None => self.find_tempo(at, After::Nothing, cx),
            },
        }
        cx.notify();
    }

    /// The song keeps time with a region whose beats are known.
    fn adapt(&mut self, at: At, steady: bool, cx: &mut Context<Self>) {
        let mut said = Ok(());
        self.edit(cx, |ws| {
            said = ws.session.project.adapt_to(at, steady, true)
        });
        let p = &self.session.project;
        self.status = Some(match said {
            Ok(()) if steady || p.tempos.is_empty() => {
                format!("The song is at {:.2} BPM, the region's tempo", p.tempo)
            }
            Ok(()) => format!(
                "The song keeps time with the region: {} tempo changes, bar by bar",
                p.tempos.len()
            ),
            Err(e) => e,
        });
        self.tempo_lane.get_or_insert(true);
        cx.notify();
    }

    /// Listens to an audio region for its tempo and beats, in the background; then does
    /// `after`.
    pub fn find_tempo(&mut self, at: At, after: After, cx: &mut Context<Self>) {
        let p = &self.session.project;
        let Some(r) = p.tracks.get(at.0).and_then(|t| t.regions.get(at.1)) else {
            return;
        };
        let Content::Audio { file, .. } = &r.content else {
            self.status = Some("Only audio has a tempo to find.".into());
            return cx.notify();
        };
        let (file, start) = (file.clone(), r.start);
        let path = self.session.dir.join(&file);
        let rate = p.sample_rate as f64;
        let name = file
            .file_stem()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        self.status = Some(format!("Listening to {name} for its tempo…"));
        self.listening.push((file.clone(), start));
        cx.notify();
        let work = cx.background_spawn(async move {
            let (channels, from) = read_audio(&path)?;
            Ok::<_, String>(find(&channels, from, rate, &name))
        });
        cx.spawn(async move |this, cx| {
            let found = work.await;
            this.update(cx, |ws, cx| {
                ws.found_tempo(at, &file, start, found, after, cx)
            })
            .ok();
        })
        .detach();
    }

    /// A region's tempo found: kept on it, then `after` done.
    fn found_tempo(
        &mut self,
        at: At,
        file: &std::path::Path,
        start: u64,
        found: Result<Option<Found>, String>,
        after: After,
        cx: &mut Context<Self>,
    ) {
        if let Some(i) = self
            .listening
            .iter()
            .position(|l| l.0.as_path() == file && l.1 == start)
        {
            self.listening.remove(i);
        }
        // The region, if it's still there (found again by its file and place).
        let Some(at) = self.session.project.tracks.get(at.0).and_then(|t| {
            t.regions
                .iter()
                .position(|r| {
                    r.start == start
                        && matches!(&r.content, Content::Audio { file: f, .. } if f == file)
                })
                .map(|i| (at.0, i))
        }) else {
            return;
        };
        let found = match found {
            Ok(Some(f)) => f,
            Ok(None) => {
                if after != After::Mode {
                    self.status = Some(
                        "No beat to follow in it: a one-shot, or nothing steady enough.".into(),
                    );
                }
                return cx.notify();
            }
            Err(e) => {
                self.status = Some(e);
                return cx.notify();
            }
        };
        let (tempo, looped) = (found.tempo, found.looped);
        self.edit(cx, |ws| {
            if let Content::Audio {
                tempo: t, grid: g, ..
            } = &mut ws.session.project.tracks[at.0].regions[at.1].content
            {
                (*t, *g) = (Some(found.tempo), Some(found.grid));
            }
        });
        self.status = Some(format!(
            "{tempo:.2} BPM{}",
            if looped { ", a loop" } else { "" }
        ));
        match after {
            After::Nothing => {}
            After::Follow(f) => self.edit(cx, |ws| ws.session.project.set_follow(at, f)),
            After::Adapt(steady) => self.adapt(at, steady, cx),
            After::Mode => self.take_tempo_mode(at, looped, cx),
        }
        cx.notify();
    }

    /// ⇧T: the audio regions picked time-stretched to the song's tempo (following it, their tempo
    /// found first if it isn't known); if they all follow it already, back as they played before.
    pub fn stretch_to_tempo(&mut self, regions: Vec<At>, cx: &mut Context<Self>) {
        let p = &self.session.project;
        let audio: Vec<(At, Follow, Option<f64>)> = regions
            .into_iter()
            .filter_map(|at| match &p.region(at).content {
                Content::Audio { follow, tempo, .. } => Some((at, *follow, *tempo)),
                _ => None,
            })
            .collect();
        if audio.is_empty() {
            self.status =
                Some("Pick an audio region to time-stretch it to the song's tempo.".into());
            return cx.notify();
        }
        if audio.iter().all(|a| !a.1.is_off()) {
            self.edit(cx, |ws| {
                for a in &audio {
                    ws.session.project.set_follow(a.0, Follow::Off);
                }
            });
            self.status = Some("Not stretched to the tempo: as it played before".into());
            return cx.notify();
        }
        let bpm = p.tempo_at(p.region(audio[0].0).start);
        self.edit(cx, |ws| {
            for a in audio.iter().filter(|a| a.1.is_off() && a.2.is_some()) {
                ws.session.project.set_follow(a.0, Follow::On);
            }
        });
        self.status = Some(format!("Time-stretched to the song's {bpm:.2} BPM"));
        for a in audio.iter().filter(|a| a.1.is_off() && a.2.is_none()) {
            self.find_tempo(a.0, After::Follow(Follow::On), cx);
        }
        cx.notify();
    }

    /// Audio just recorded or brought in, its tempo known: the song adapts to it (Adapt, or
    /// Auto with nothing else keeping time and the metronome off), or a loop follows the song.
    pub fn take_tempo_mode(&mut self, at: At, looped: bool, cx: &mut Context<Self>) {
        let p = &self.session.project;
        let r = p.region(at);
        let adapt = match p.tempo_mode {
            TempoMode::Keep => false,
            TempoMode::Adapt => true,
            TempoMode::Auto => {
                !self.player.metronome() && !p.keeps_time(at, r.start, r.start + r.length)
            }
        };
        match (adapt, looped) {
            (true, _) => self.adapt(at, looped, cx),
            (false, true) => self.edit(cx, |ws| ws.session.project.set_follow(at, Follow::On)),
            (false, false) => {}
        }
    }
}
