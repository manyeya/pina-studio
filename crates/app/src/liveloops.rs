//! Live Loops, as in Logic: a grid of cells, a row per track and a column per scene, each a loop
//! to launch. A launched cell starts on the next bar (or at once, starting playback) and plays
//! instead of the track's regions until it's stopped. Click an empty cell to copy the selected
//! region of that track into it; click a cell to launch or stop it; right-click to empty it. The
//! buttons under the grid launch a whole scene.

use gpui::{Context, Hsla, MouseButton, div, prelude::*, px, rgb};
use tempo_engine::project::{BEATS_PER_BAR, Cell, Content, PPQ, Region};

use crate::{theme::*, workspace::Workspace};

pub const SCENES: usize = 8;
const CELL_W: f32 = 104.0;
const ROW_H: f32 = 34.0;

impl Workspace {
    /// Launches a track's cell for a scene, or stops its cell with None: at the next bar when
    /// playing, at once (starting playback) when not.
    pub fn launch(&mut self, track: usize, scene: Option<usize>, cx: &mut Context<Self>) {
        let t = &self.session.project.tracks[track];
        let cell = scene.and_then(|s| t.cells.iter().position(|c| c.scene == s));
        let rate = self.player.rate();
        let bar = self
            .session
            .project
            .samples(PPQ * BEATS_PER_BAR, rate)
            .max(1);
        let now = self.player.position();
        let at = if self.player.playing() {
            (now / bar + 1) * bar
        } else {
            now
        };
        let id = t.id;
        self.player.launch(id, cell, at);
        self.live.insert(id, (cell.map(|c| t.cells[c].scene), at));
        if !self.player.playing() && cell.is_some() {
            self.player.play();
        }
        cx.notify();
    }

    /// Launches a scene: each track plays its cell in that column, or stops if it has none.
    pub fn launch_scene(&mut self, scene: usize, cx: &mut Context<Self>) {
        for track in 0..self.session.project.tracks.len() {
            self.launch(track, Some(scene), cx);
        }
    }

    fn click_cell(&mut self, track: usize, scene: usize, cx: &mut Context<Self>) {
        let t = &self.session.project.tracks[track];
        let filled = t.cells.iter().any(|c| c.scene == scene);
        if filled {
            // Launched (or waiting to be) stops; anything else launches.
            let on = self.live.get(&t.id).is_some_and(|&(s, _)| s == Some(scene));
            self.launch(track, if on { None } else { Some(scene) }, cx);
            return;
        }
        // An empty cell takes a copy of the track's selected region.
        let Some((_, r)) = self.region.filter(|at| at.0 == track) else {
            self.status = Some(
                "Select a region on this track, then click an empty cell to copy it in.".into(),
            );
            cx.notify();
            return;
        };
        self.edit(cx, |ws| {
            let t = &mut ws.session.project.tracks[track];
            let region = Region {
                start: 0,
                ..t.regions[r].clone()
            };
            t.cells.push(Cell { scene, region });
        });
    }

    fn clear_cell(&mut self, track: usize, scene: usize, cx: &mut Context<Self>) {
        let id = self.session.project.tracks[track].id;
        if self.live.get(&id).is_some_and(|&(s, _)| s == Some(scene)) {
            self.launch(track, None, cx);
        }
        self.edit(cx, |ws| {
            ws.session.project.tracks[track]
                .cells
                .retain(|c| c.scene != scene)
        });
    }

    pub fn loops_pane(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let project = &self.session.project;
        let now = self.player.position();
        let mut names = Vec::new();
        let mut rows = Vec::new();
        for (track, t) in project.tracks.iter().enumerate() {
            names.push(
                div()
                    .h(px(ROW_H))
                    .flex()
                    .items_center()
                    .px_2()
                    .text_size(px(11.))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(t.name.clone()),
            );
            let color = Hsla::from(rgb(track_color(t.instrument.is_some())));
            let live = self.live.get(&t.id).copied();
            let cells = (0..SCENES).map(|scene| {
                let cell = t.cells.iter().find(|c| c.scene == scene);
                let launched = live.filter(|&(s, _)| s == Some(scene));
                let playing = launched.is_some_and(|(_, at)| now >= at);
                let waiting = launched.is_some() && !playing;
                let label = cell.map(|c| {
                    let bars = c.region.length as f32 / (PPQ * BEATS_PER_BAR) as f32;
                    let kind = match c.region.content {
                        Content::Audio { .. } => "Audio",
                        Content::Pattern { .. } => "Pattern",
                        _ => "MIDI",
                    };
                    format!(
                        "{}{kind} · {bars:.0} bar{}",
                        if playing { "▶ " } else { "" },
                        if bars == 1.0 { "" } else { "s" }
                    )
                });
                div()
                    .w(px(CELL_W))
                    .h(px(ROW_H - 4.))
                    .px_1()
                    .flex()
                    .items_center()
                    .rounded(px(4.))
                    .text_size(px(10.))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .bg(match (cell, playing) {
                        (Some(_), true) => color.opacity(0.85),
                        (Some(_), false) => color.opacity(0.35),
                        (None, _) => Hsla::from(rgb(0x26262a)),
                    })
                    .border_1()
                    .border_color(if waiting {
                        Hsla::from(rgb(SOLO))
                    } else {
                        Hsla::from(rgb(BORDER))
                    })
                    .text_color(rgb(if playing { 0x101010 } else { TEXT }))
                    .debug_selector(move || format!("cell-{track}-{scene}"))
                    .children(label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.click_cell(track, scene, cx)),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |ws, _, _, cx| ws.clear_cell(track, scene, cx)),
                    )
            });
            rows.push(
                div()
                    .h(px(ROW_H))
                    .flex()
                    .items_center()
                    .gap_1()
                    .children(cells),
            );
        }
        let scenes = (0..SCENES).map(|scene| {
            div().w(px(CELL_W)).child(
                button(
                    ["▶ 1", "▶ 2", "▶ 3", "▶ 4", "▶ 5", "▶ 6", "▶ 7", "▶ 8"][scene],
                    false,
                )
                .debug_selector(move || format!("scene-{scene}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.launch_scene(scene, cx)),
                ),
            )
        });
        div()
            .h(px(ROLL_H))
            .flex_none()
            .flex()
            .bg(rgb(PANEL_DARK))
            .border_t_1()
            .border_color(rgb(BORDER))
            .when(self.show_inspector, |d| {
                d.child(
                    div()
                        .w(px(INSPECTOR_W))
                        .flex_none()
                        .p_2()
                        .border_r_1()
                        .border_color(rgb(BORDER))
                        .text_size(px(10.))
                        .text_color(rgb(TEXT_DIM))
                        .child("LIVE LOOPS"),
                )
            })
            .child(
                div()
                    .w(px(HEADER_W))
                    .flex_none()
                    .py_2()
                    .border_r_1()
                    .border_color(rgb(BORDER))
                    .children(names),
            )
            .child(
                div()
                    .id("loops")
                    .flex_1()
                    .p_2()
                    .overflow_scroll()
                    .children(rows)
                    .child(
                        div()
                            .h(px(ROW_H))
                            .flex()
                            .items_center()
                            .gap_1()
                            .children(scenes),
                    ),
            )
    }
}
