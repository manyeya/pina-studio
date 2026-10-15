//! The tracks area: track headers on the left, the ruler and the lanes with their regions on the
//! right. Regions move by dragging their body, and trim by dragging their edges.

use std::sync::Arc;

use gpui::{
    AnyElement, App, Bounds, Context, Div, Hsla, MouseButton, MouseDownEvent, PathBuilder, Pixels,
    Point, Window, canvas, div, fill, point, prelude::*, px, rgb, size,
};
use tempo_engine::{
    peaks::Peaks,
    project::{At, AutoPoint, BEATS_PER_BAR, Content, Instrument, PPQ, Param, comp_segments},
};

use crate::{
    menu::{Menu, MenuAction},
    strip,
    theme::*,
    workspace::{Bottom, Drag, Edge, Workspace},
};

/// Space kept free above and below an automation line.
const AUTO_PAD: f32 = 6.0;

/// Where a parameter value sits in a lane, from its top: volume on the fader's scale, pan with
/// left at the top.
pub fn auto_to_y(param: Param, value: f32) -> f32 {
    let t = match param {
        Param::Volume => 1.0 - strip::fader_pos(value),
        Param::Pan => (value.clamp(-1.0, 1.0) + 1.0) / 2.0,
    };
    AUTO_PAD + t * (LANE_H - 2.0 * AUTO_PAD)
}

pub fn auto_from_y(param: Param, y: f32) -> f32 {
    let t = ((y - AUTO_PAD) / (LANE_H - 2.0 * AUTO_PAD)).clamp(0.0, 1.0);
    match param {
        Param::Volume => strip::fader_db(1.0 - t),
        Param::Pan => t * 2.0 - 1.0,
    }
}

/// What a region shows inside.
#[derive(Default)]
pub struct Inside {
    /// Start (ticks from the region's start), length and pitch.
    notes: Vec<(i64, u64, u8)>,
    waves: Vec<Wave>,
}

/// A stretch of waveform: from `x0` to `x1` pixels into the region, `offset` the sample at `x0`.
struct Wave {
    x0: f32,
    x1: f32,
    peaks: Arc<Peaks>,
    offset: f32,
    samples_per_px: f32,
}

impl Workspace {
    pub fn tracks_area(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(RULER_H))
                    .flex_none()
                    .flex()
                    .child(self.corner(cx))
                    .child(self.ruler(cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .child(self.headers(cx))
                    .child(self.lanes(cx)),
            )
    }

    fn corner(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(HEADER_W))
            .h_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .bg(rgb(PANEL))
            .border_b_1()
            .border_r_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(TEXT_DIM))
                    .child("TRACKS"),
            )
            .child(
                button("+", false)
                    .debug_selector(|| "new-track".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            let item = |name: &str, instrument| {
                                (name.to_string(), MenuAction::NewTrack { instrument })
                            };
                            let items = vec![
                                item("Software Instrument", Some(Instrument::Synth)),
                                item("Drum Kit", Some(Instrument::Drums)),
                                item("Audio", None),
                            ];
                            ws.menu = Some(Menu {
                                at: e.position,
                                items,
                            });
                            cx.notify();
                        }),
                    ),
            )
    }

    fn ruler(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (ppt, scroll) = (self.px_per_tick(), self.scroll_x);
        let bar_px = ppt * (PPQ * BEATS_PER_BAR) as f32;
        let width = f32::from(self.lanes_bounds.get().size.width).max(2000.0);
        // Label every bar, or fewer when they'd crowd.
        let every = (48.0 / bar_px).ceil().max(1.0) as u64;
        let first = (scroll / bar_px) as u64;
        let last = ((scroll + width) / bar_px) as u64 + 1;
        let labels = (first..=last).filter(|b| b % every == 0).map(|b| {
            div()
                .absolute()
                .left(px(b as f32 * bar_px - scroll + 4.))
                .top(px(2.))
                .text_size(px(10.))
                .text_color(rgb(TEXT_DIM))
                .child(format!("{}", b + 1))
        });
        let playhead = self.playhead() as f32 * ppt - scroll;
        div()
            .flex_1()
            .h_full()
            .relative()
            .overflow_hidden()
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        let beat_px = ppt * PPQ as f32;
                        let first = (scroll / beat_px) as u64;
                        let last = ((scroll + f32::from(b.size.width)) / beat_px) as u64 + 1;
                        for beat in first..=last {
                            let bar = beat % BEATS_PER_BAR == 0;
                            if !bar && beat_px < 8.0 {
                                continue;
                            }
                            let x = b.origin.x + px(beat as f32 * beat_px - scroll);
                            let h = if bar { b.size.height } else { px(6.) };
                            let top = b.origin.y + b.size.height - h;
                            window.paint_quad(fill(
                                Bounds::new(point(x, top), size(px(1.), h)),
                                rgb(GRID_BAR),
                            ));
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .children(labels)
            .child(
                div()
                    .absolute()
                    .left(px(playhead - 5.))
                    .bottom_0()
                    .w(px(11.))
                    .h(px(10.))
                    .rounded_t(px(2.))
                    .bg(rgb(0xffffff)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    ws.begin_drag(Drag::Locate, cx);
                    ws.locate_at(f32::from(e.position.x), cx);
                }),
            )
            .on_scroll_wheel(self.on_scroll(cx))
    }

    fn headers(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let tracks = &self.session.project.tracks;
        let headers: Vec<Div> = tracks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let selected = i == self.track;
                // The summing stack it plays into, if it's in one.
                let stack = t.output.and_then(|b| {
                    let aux = self
                        .session
                        .project
                        .auxes
                        .iter()
                        .find(|a| a.bus == b && a.stack)?;
                    Some(aux.name.clone())
                });
                div()
                    .absolute()
                    .top(px(i as f32 * LANE_H - self.scroll_y))
                    .left_0()
                    .w_full()
                    .h(px(LANE_H))
                    .flex()
                    .items_center()
                    .gap_2()
                    .pr_2()
                    .bg(rgb(if selected { HEADER_SELECTED } else { HEADER }))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .w(px(5.))
                            .h_full()
                            .bg(rgb(track_color(t.instrument.is_some()))),
                    )
                    .child(
                        div()
                            .w(px(16.))
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_DIM))
                            .child(format!("{}", i + 1)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(t.name.clone()),
                    )
                    .when_some(stack, |d, name| {
                        d.child(
                            div()
                                .px_1()
                                .rounded(px(3.))
                                .text_size(px(9.))
                                .bg(Hsla::from(rgb(AUX)).opacity(0.35))
                                .child(name),
                        )
                    })
                    .when(self.show_automation, |d| {
                        let id = t.id;
                        let param = self.params.get(&id).copied().unwrap_or_default();
                        let label = if param == Param::Volume {
                            "Volume"
                        } else {
                            "Pan"
                        };
                        d.child(button(label, true).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                let p = ws.params.entry(id).or_default();
                                *p = if *p == Param::Volume {
                                    Param::Pan
                                } else {
                                    Param::Volume
                                };
                                cx.notify();
                            }),
                        ))
                    })
                    .child(toggle("M", t.mute, MUTE).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit(cx, |ws| ws.session.project.tracks[i].mute ^= true);
                        }),
                    ))
                    .child(toggle("S", t.solo, SOLO).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit(cx, |ws| ws.session.project.tracks[i].solo ^= true);
                        }),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.track = i;
                            ws.region = None;
                            cx.notify();
                        }),
                    )
            })
            .collect();
        div()
            .w(px(HEADER_W))
            .flex_none()
            .h_full()
            .relative()
            .overflow_hidden()
            .bg(rgb(PANEL_DARK))
            .border_r_1()
            .border_color(rgb(BORDER))
            .children(headers)
            .on_scroll_wheel(self.on_scroll(cx))
    }

    fn lanes(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (ppt, sx, sy) = (self.px_per_tick(), self.scroll_x, self.scroll_y);
        let tracks = self.session.project.tracks.len();
        let selected = self.track;
        let bounds = self.lanes_bounds.clone();
        let grid = canvas(
            move |b, _, _| bounds.set(b),
            move |b, _, window, _| {
                for i in 0..tracks {
                    let y = b.origin.y + px(i as f32 * LANE_H - sy);
                    let bg = if i == selected { LANE_SELECTED } else { LANE };
                    window.paint_quad(fill(
                        Bounds::new(point(b.origin.x, y), size(b.size.width, px(LANE_H))),
                        rgb(bg),
                    ));
                    let line = Bounds::new(
                        point(b.origin.x, y + px(LANE_H - 1.)),
                        size(b.size.width, px(1.)),
                    );
                    window.paint_quad(fill(line, rgb(BORDER)));
                }
                let beat_px = ppt * PPQ as f32;
                let first = (sx / beat_px) as u64;
                let last = ((sx + f32::from(b.size.width)) / beat_px) as u64 + 1;
                for beat in first..=last {
                    let bar = beat % BEATS_PER_BAR == 0;
                    if !bar && beat_px < 12.0 {
                        continue;
                    }
                    let x = b.origin.x + px(beat as f32 * beat_px - sx);
                    let color = if bar { GRID_BAR } else { GRID_BEAT };
                    window.paint_quad(fill(
                        Bounds::new(point(x, b.origin.y), size(px(1.), b.size.height)),
                        rgb(color),
                    ));
                }
            },
        )
        .absolute()
        .size_full();

        // Clicking an empty lane selects its track.
        let rows: Vec<Div> = (0..tracks)
            .map(|i| {
                div()
                    .absolute()
                    .top(px(i as f32 * LANE_H - sy))
                    .left_0()
                    .w_full()
                    .h(px(LANE_H))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            ws.track = i;
                            ws.region = None;
                            if ws.show_automation {
                                ws.add_auto_point(i, e.position.map(f32::from), cx);
                            }
                            cx.notify();
                        }),
                    )
            })
            .collect();

        let width = f32::from(self.lanes_bounds.get().size.width).max(2000.0);
        let mut regions = Vec::new();
        for (t, track) in self.session.project.tracks.iter().enumerate() {
            for r in 0..track.regions.len() {
                let region = &track.regions[r];
                let x = region.start as f32 * ppt - sx;
                let w = region.length as f32 * ppt;
                if x + w >= 0.0 && x <= width {
                    regions.push(self.region_view((t, r), cx));
                }
            }
        }

        let playhead = self.playhead() as f32 * ppt - sx;
        let automation = if self.show_automation {
            self.automation(cx)
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
                    .top(px(t as f32 * LANE_H - sy + 1.))
                    .w(px((playhead - x).max(2.)))
                    .h(px(LANE_H - 3.))
                    .rounded(px(4.))
                    .bg(Hsla::from(rgb(RECORD)).opacity(0.45))
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
            .bg(rgb(BG))
            .child(grid)
            .children(rows)
            .children(regions)
            .children(take)
            .children(automation)
            .when(tracks == 0, |d| {
                d.child(
                    div()
                        .absolute()
                        .top(px(40.))
                        .left(px(40.))
                        .text_color(rgb(TEXT_DIM))
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
                    .bg(rgb(0xffffff)),
            )
            .on_scroll_wheel(self.on_scroll(cx))
    }

    fn region_view(&self, at: At, cx: &mut Context<Self>) -> Div {
        let project = &self.session.project;
        let track = &project.tracks[at.0];
        let region = &track.regions[at.1];
        let ppt = self.px_per_tick();
        let x = region.start as f32 * ppt - self.scroll_x;
        let w = (region.length as f32 * ppt).max(3.0);
        let y = at.0 as f32 * LANE_H - self.scroll_y + 1.0;
        let color = Hsla::from(rgb(track_color(track.instrument.is_some())));
        let color = if track.mute {
            Hsla { s: 0.0, ..color }
        } else {
            color
        };
        let selected = self.region == Some(at);
        let inside = self.inside(&region.content, region.length);
        let (start, end) = (region.start, region.start + region.length);
        let live = !self.show_automation;
        let handle = |edge: Edge, cx: &mut Context<Self>| {
            let h = div()
                .absolute()
                .top_0()
                .w(px(6.))
                .h_full()
                .cursor_ew_resize();
            let h = if edge == Edge::Start {
                h.left_0()
            } else {
                h.right_0()
            };
            h.on_mouse_down(MouseButton::Left, grab(at, edge, start, end, cx))
        };

        div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(w))
            .h(px(LANE_H - 3.))
            .rounded(px(4.))
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(color.opacity(0.45))
            .border_1()
            .border_color(if selected {
                gpui::white()
            } else {
                color.opacity(0.9)
            })
            .child(
                div()
                    .h(px(14.))
                    .flex_none()
                    .px_1()
                    .bg(color.opacity(0.85))
                    .text_size(px(10.))
                    .text_color(rgb(0x101010))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(match &region.content {
                        Content::Takes { takes, comp } => {
                            match &comp_segments(comp, takes.len(), region.length)[..] {
                                [(_, _, take)] => format!("{} · Take {}", track.name, take + 1),
                                _ => format!("{} · Comp", track.name),
                            }
                        }
                        _ => track.name.clone(),
                    }),
            )
            .child(
                div().flex_1().relative().child(
                    canvas(
                        |_, _, _| {},
                        move |b, _, window, _| paint_inside(&inside, b, ppt, color, window),
                    )
                    .absolute()
                    .size_full(),
                ),
            )
            .when(live, |d| {
                d.child(handle(Edge::Start, cx))
                    .child(handle(Edge::End, cx))
                    .cursor_grab()
                    .on_mouse_down(MouseButton::Left, grab(at, Edge::Move, start, end, cx))
            })
            .when(!live, |d| d.opacity(0.45))
    }

    /// Each track's automation for its chosen parameter: a line, and points to drag.
    fn automation(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (ppt, sx, sy) = (self.px_per_tick(), self.scroll_x, self.scroll_y);
        let mut out = Vec::new();
        for (t, track) in self.session.project.tracks.iter().enumerate() {
            let param = self.params.get(&track.id).copied().unwrap_or_default();
            let points: Vec<AutoPoint> = track.automation.points(param).to_vec();
            let count = points.len();
            let level = match param {
                Param::Volume => track.volume_db,
                Param::Pan => track.pan,
            };
            let color = rgb(if param == Param::Volume {
                AUTO_VOLUME
            } else {
                AUTO_PAN
            });
            let top = t as f32 * LANE_H - sy;
            for (i, p) in points.iter().enumerate() {
                let x = p.tick as f32 * ppt - sx;
                out.push(
                    div()
                        .absolute()
                        .left(px(x - 4.))
                        .top(px(top + auto_to_y(param, p.value) - 4.))
                        .size(px(9.))
                        .rounded(px(5.))
                        .bg(color)
                        .border_1()
                        .border_color(rgb(0x101010))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.track = t;
                                if e.modifiers.alt {
                                    ws.edit(cx, |ws| {
                                        let a = &mut ws.session.project.tracks[t].automation;
                                        a.points_mut(param).remove(i);
                                    });
                                } else {
                                    let drag = Drag::Auto {
                                        track: t,
                                        param,
                                        index: i,
                                        moved: false,
                                    };
                                    ws.begin_drag(drag, cx);
                                }
                            }),
                        )
                        .into_any_element(),
                );
            }
            // Drawn under the points: added first.
            let line = canvas(
                |_, _, _| {},
                move |b, _, window, _| {
                    let y = |v: f32| b.origin.y + px(top + auto_to_y(param, v));
                    let x = |tick: u64| b.origin.x + px(tick as f32 * ppt - sx);
                    let (first, last) = match (points.first(), points.last()) {
                        (Some(f), Some(l)) => (f.value, l.value),
                        _ => (level, level),
                    };
                    let mut path = PathBuilder::stroke(px(1.5));
                    path.move_to(point(b.origin.x, y(first)));
                    for p in &points {
                        path.line_to(point(x(p.tick), y(p.value)));
                    }
                    path.line_to(point(b.origin.x + b.size.width, y(last)));
                    if let Ok(path) = path.build() {
                        let faint = if points.is_empty() { 0.4 } else { 1.0 };
                        window.paint_path(path, Hsla::from(color).opacity(faint));
                    }
                },
            )
            .absolute()
            .size_full();
            out.insert(out.len() - count, line.into_any_element());
        }
        out
    }

    /// Adds an automation point where the pointer is, and starts dragging it.
    pub fn add_auto_point(&mut self, track: usize, p: Point<f32>, cx: &mut Context<Self>) {
        let id = self.session.project.tracks[track].id;
        let param = self.params.get(&id).copied().unwrap_or_default();
        let point = AutoPoint {
            tick: self.snap(self.tick_at(p.x)),
            value: self.auto_value(track, param, p.y),
        };
        self.session.checkpoint();
        let index = self.session.project.tracks[track]
            .automation
            .add(param, point);
        self.drag = Some(Drag::Auto {
            track,
            param,
            index,
            moved: true,
        });
        self.changed(cx);
    }
}

/// Starts dragging a region (`start` and `end` are its ticks when the drag starts).
fn grab(
    at: At,
    edge: Edge,
    start: u64,
    end: u64,
    cx: &mut Context<Workspace>,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
        cx.stop_propagation();
        if ws.region != Some(at) {
            ws.note = None;
        }
        ws.region = Some(at);
        ws.track = at.0;
        // Double-clicking a MIDI region opens it in the piano roll, and a take folder its takes.
        if e.click_count == 2 && edge == Edge::Move {
            let pane = match ws.session.project.region(at).content {
                Content::Takes { .. } => Some(Bottom::Takes),
                Content::Pattern { .. } => Some(Bottom::Steps),
                Content::Midi { .. } => Some(Bottom::PianoRoll),
                Content::Audio { .. } => None,
            };
            if let Some(pane) = pane {
                ws.bottom = Some(pane);
                ws.focus_roll();
                cx.notify();
                return;
            }
        }
        let from = e.position.map(f32::from);
        let drag = Drag::Region {
            at,
            edge,
            from,
            start,
            end,
            track: at.0,
            moved: false,
        };
        ws.begin_drag(drag, cx);
    })
}

impl Workspace {
    /// What a region's content shows, `length` ticks of it.
    pub fn inside(&self, content: &Content, length: u64) -> Inside {
        let mut inside = Inside::default();
        match content {
            Content::Takes { takes, comp } => {
                for (from, to, t) in comp_segments(comp, takes.len(), length) {
                    self.add_inside(&mut inside, &takes[t].content, takes[t].start, from, to);
                }
            }
            content => self.add_inside(&mut inside, content, 0, 0, length),
        }
        inside
    }

    /// Adds what `content`, beginning `at` ticks into a region, shows from `from` to `to` ticks
    /// into it.
    pub fn add_inside(&self, inside: &mut Inside, content: &Content, at: i64, from: u64, to: u64) {
        let project = &self.session.project;
        let ppt = self.px_per_tick();
        match content {
            Content::Midi { .. } | Content::Pattern { .. } => {
                let notes = content.notes((to as i64 - at).max(0) as u64).unwrap();
                let shown = notes
                    .iter()
                    .filter(|n| (from as i64..to as i64).contains(&(at + n.start)));
                inside
                    .notes
                    .extend(shown.map(|n| (at + n.start, n.length, n.pitch)));
            }
            Content::Audio { file, offset } => {
                let begin = (from as i64).max(at);
                let Some(peaks) = self.player.peaks(file).filter(|_| begin < to as i64) else {
                    return;
                };
                let samples_per_tick =
                    project.sample_rate as f32 * 60.0 / (project.tempo as f32 * PPQ as f32);
                inside.waves.push(Wave {
                    x0: begin as f32 * ppt,
                    x1: to as f32 * ppt,
                    peaks,
                    offset: *offset as f32 + (begin - at) as f32 * samples_per_tick,
                    samples_per_px: samples_per_tick / ppt,
                });
            }
            Content::Takes { .. } => {}
        }
    }
}

/// Draws a region's notes and waveforms in a lighter shade of its color.
pub fn paint_inside(
    inside: &Inside,
    b: Bounds<Pixels>,
    ppt: f32,
    color: Hsla,
    window: &mut Window,
) {
    let ink = Hsla {
        l: (color.l + 0.3).min(0.92),
        ..color
    };
    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
    let mut bar = |x: f32, y: f32, bw: f32, bh: f32| {
        let at = point(b.origin.x + px(x), b.origin.y + px(y));
        window.paint_quad(fill(Bounds::new(at, size(px(bw), px(bh))), ink));
    };
    if let Some(lo) = inside.notes.iter().map(|n| n.2).min() {
        let hi = inside.notes.iter().map(|n| n.2).max().unwrap();
        // At least an octave tall, centered on the notes.
        let span = (hi - lo).max(12) as f32 + 1.0;
        let top = (lo as f32 + hi as f32) / 2.0 + span / 2.0;
        let row = (h - 4.0) / span;
        for &(start, len, pitch) in &inside.notes {
            let (x, nw) = (start as f32 * ppt, (len as f32 * ppt).max(1.0));
            if x + nw > 0.0 && x < w {
                let y = 2.0 + (top - pitch as f32 - 0.5) * row;
                bar(x.max(0.0), y, nw.min(w - x.max(0.0)), row.clamp(2.0, 4.0));
            }
        }
    }
    let mid = h / 2.0;
    for wave in &inside.waves {
        for x in wave.x0.max(0.0) as usize..wave.x1.min(w) as usize {
            let from = wave.offset + (x as f32 - wave.x0) * wave.samples_per_px;
            if from as usize >= wave.peaks.len_samples() {
                break;
            }
            let (lo, hi) = wave
                .peaks
                .range(from as usize, (from + wave.samples_per_px) as usize);
            let (y1, y2) = (mid - hi * mid, mid - lo * mid);
            bar(x as f32, y1, 1.0, (y2 - y1).max(1.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automation_values_and_lane_heights_round_trip() {
        for (param, v) in [
            (Param::Volume, -12.0),
            (Param::Volume, 6.0),
            (Param::Pan, -0.5),
        ] {
            assert!((auto_from_y(param, auto_to_y(param, v)) - v).abs() < 1e-3);
        }
        assert_eq!(auto_to_y(Param::Pan, -1.0), AUTO_PAD); // left at the top
    }
}
