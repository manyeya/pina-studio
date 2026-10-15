//! The step sequencer, as in Logic: a pattern region's rows of steps, in the pane under the
//! tracks. Click a step to turn it on or off, option-click for an accent; "+ Sound" adds a row.

use gpui::{Context, MouseButton, MouseDownEvent, div, prelude::*, px, rgb};
use tempo_engine::{
    drums,
    project::{At, BEATS_PER_BAR, Content, PPQ, PatternRow, Region},
};

use crate::{
    menu::{Menu, MenuAction},
    theme::*,
    workspace::{Bottom, Workspace},
};

const CELL: f32 = 24.0;
const ON: u8 = 100;
const ACCENTED: u8 = 127;

/// The rows a new pattern starts with.
const STARTING_ROWS: [u8; 5] = [36, 38, 42, 46, 39];

impl Workspace {
    /// The pattern region the pane shows: the selected one, if it's a pattern.
    pub fn pattern(&self) -> Option<At> {
        self.region.filter(|&at| {
            matches!(
                self.session.project.region(at).content,
                Content::Pattern { .. }
            )
        })
    }

    /// A new empty pattern region, a bar long, at the bar the playhead is in, on the selected
    /// instrument track; and opens it.
    pub fn new_pattern(&mut self, cx: &mut Context<Self>) {
        let tracks = &self.session.project.tracks;
        if !tracks
            .get(self.track)
            .is_some_and(|t| t.instrument.is_some())
        {
            self.status = Some("Select an instrument track for the pattern.".into());
            cx.notify();
            return;
        }
        let bar = PPQ * BEATS_PER_BAR;
        let start = self.playhead() / bar * bar;
        let rows = STARTING_ROWS
            .iter()
            .map(|&pitch| PatternRow {
                pitch,
                cells: vec![0; 16],
            })
            .collect();
        let t = self.track;
        self.edit(cx, |ws| {
            let regions = &mut ws.session.project.tracks[t].regions;
            regions.push(Region {
                start,
                length: bar,
                content: Content::Pattern {
                    step: PPQ / 4,
                    rows,
                },
            });
            ws.region = Some((t, regions.len() - 1));
            ws.bottom = Some(Bottom::Steps);
        });
    }

    fn set_step(&mut self, at: At, row: usize, step: usize, accent: bool, cx: &mut Context<Self>) {
        self.edit(cx, |ws| {
            if let Content::Pattern { rows, .. } =
                &mut ws.session.project.tracks[at.0].regions[at.1].content
            {
                let cell = &mut rows[row].cells[step];
                *cell = match (*cell, accent) {
                    (0, false) => ON,
                    (_, true) => ACCENTED,
                    _ => 0,
                };
            }
        });
    }

    pub fn steps_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let at = self.pattern();
        let mut labels = Vec::new();
        let mut grid = Vec::new();
        if let Some(at) = at {
            let region = self.session.project.region(at);
            let Content::Pattern { step, rows } = &region.content else {
                unreachable!()
            };
            let steps = rows.iter().map(|r| r.cells.len()).max().unwrap_or(16);
            let track_color = self.color(at.0);
            // The step under the playhead, while playing in the region.
            let tick = self.playhead();
            let now = (self.player.playing()
                && (region.start..region.start + region.length).contains(&tick))
            .then(|| ((tick - region.start) / step) as usize % steps.max(1));
            for (r, row) in rows.iter().enumerate() {
                labels.push(
                    div()
                        .h(px(CELL + 2.))
                        .flex()
                        .items_center()
                        .px_2()
                        .text_size(px(11.))
                        .child(drums::name(row.pitch)),
                );
                let cells = row.cells.iter().enumerate().map(|(k, &v)| {
                    let beat_start = k % 4 == 0;
                    let off = if (k / 4) % 2 == 0 { 0x1d1d23 } else { 0x16161b };
                    let color = match v {
                        0 if now == Some(k) => RAISED,
                        0 => off,
                        v if v >= ACCENTED => tint(track_color, 0.55),
                        _ => track_color,
                    };
                    div()
                        .w(px(CELL))
                        .h(px(CELL))
                        .rounded(px(3.))
                        .bg(rgb(color))
                        .when(beat_start, |d| d.ml(px(4.)))
                        .debug_selector(move || format!("step-{r}-{k}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                ws.set_step(at, r, k, e.modifiers.alt, cx);
                            }),
                        )
                });
                grid.push(
                    div()
                        .h(px(CELL + 2.))
                        .flex()
                        .items_center()
                        .gap(px(2.))
                        .children(cells),
                );
            }
        }
        let add = at.map(|at| {
            button("+ Sound", false)
                .debug_selector(|| "add-sound".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        let items = drums::SOUNDS
                            .iter()
                            .map(|&(pitch, name)| {
                                (name.to_string(), MenuAction::AddRow { at, pitch })
                            })
                            .collect();
                        ws.menu = Some(Menu {
                            at: e.position,
                            items,
                        });
                        cx.notify();
                    }),
                )
        });
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .bg(rgb(LANE))
            .child(
                div()
                    .w(px(HEADER_W))
                    .flex_none()
                    .bg(rgb(SURFACE))
                    .py_2()
                    .flex()
                    .flex_col()
                    .border_r_1()
                    .border_color(rgb(LINE))
                    .children(labels)
                    .children(add.map(|b| div().p_2().child(b))),
            )
            .child(
                div()
                    .id("steps")
                    .flex_1()
                    .p_2()
                    .overflow_y_scroll()
                    .children(grid)
                    .when(at.is_none(), |d| {
                        d.child(div().text_color(rgb(TEXT_3)).child(
                            "Select a pattern region, or make one with ⌥⌘P on an instrument track.",
                        ))
                    }),
            )
    }
}
