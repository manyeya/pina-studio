//! The step sequencer, as in Logic: a pattern region's rows of steps, in the pane under the
//! tracks. Click a step to turn it on or off, option-click for an accent; "+ Sound" adds a row.
//! On the drum kit a row is a drum; on any other instrument, a note.

use gpui::{Context, Div, MouseButton, MouseDownEvent, div, prelude::*, px, rgb};
use tempo_engine::{
    drums,
    music::Key,
    project::{At, Content, Instrument, PPQ, PatternRow, Region},
};

use crate::{
    menu::{Menu, MenuAction},
    theme::*,
    workspace::{Bottom, Workspace},
};

const CELL: f32 = 24.0;
const ON: u8 = 100;
const ACCENTED: u8 = 127;

/// The rows a new drum pattern starts with.
const STARTING_ROWS: [u8; 5] = [36, 38, 42, 46, 39];

/// The notes a pattern on a melodic instrument can have rows for: two octaves of the song's key
/// (C major without one) from the C below middle C, highest first.
fn scale_notes(key: Option<Key>) -> Vec<u8> {
    let key = key.unwrap_or(Key {
        root: 0,
        minor: false,
    });
    (48..=72).rev().filter(|&p| key.contains(p)).collect()
}

/// A row's name: a drum on the kit, a sampler's sound on its note, else a note, spelled by the
/// key.
fn row_name(instrument: Option<&Instrument>, key: Option<Key>, pitch: u8) -> String {
    match instrument {
        Some(Instrument::Drums) => drums::name(pitch),
        Some(Instrument::Sampler(s)) => match s.zones.iter().find(|z| z.covers(pitch)) {
            Some(z) if z.high.is_none() && !z.name.is_empty() => z.name.clone(),
            Some(z) if z.high.is_none() => z
                .file
                .file_stem()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            _ => crate::pianoroll::name_in(key, pitch),
        },
        _ => crate::pianoroll::name_in(key, pitch),
    }
}

/// The sounds a kit has to add rows for: the drum kit's, or a sampler's.
fn kit_sounds(instrument: Option<&Instrument>) -> Option<Vec<(u8, String)>> {
    match instrument {
        Some(Instrument::Drums) => Some(
            drums::SOUNDS
                .iter()
                .map(|&(pitch, name)| (pitch, name.to_string()))
                .collect(),
        ),
        Some(i @ Instrument::Sampler(s)) if i.is_kit() => Some(
            s.zones
                .iter()
                .map(|z| (z.note, row_name(Some(i), None, z.note)))
                .collect(),
        ),
        _ => None,
    }
}

/// The swings the step sequencer offers, as MPC's: straight to a heavy shuffle.
const SWINGS: [f32; 6] = [50.0, 54.0, 58.0, 62.0, 66.0, 71.0];

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
        let bar = self.session.project.bar();
        let start = self.playhead() / bar * bar;
        // A drum kit starts with kick, snare and hats; anything else with the key's chord.
        let instrument = tracks[self.track].instrument.as_ref();
        let starting: Vec<u8> = match (instrument, kit_sounds(instrument)) {
            (Some(Instrument::Drums), _) => STARTING_ROWS.to_vec(),
            // A sampler kit: its first few sounds.
            (_, Some(sounds)) => sounds.iter().take(6).map(|s| s.0).collect(),
            _ => {
                let key = self.session.project.song_key().unwrap_or(Key {
                    root: 0,
                    minor: false,
                });
                let root = 60 + key.root;
                let third = if key.minor { 3 } else { 4 };
                vec![root + 12, root + 7, root + third, root]
            }
        };
        let steps = (bar / (PPQ / 4)).max(1) as usize;
        let rows = starting
            .iter()
            .map(|&pitch| PatternRow {
                pitch,
                cells: vec![0; steps],
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
                    swing: 50.0,
                },
                ..Default::default()
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

    /// The step sequencer's own control in the editor's bar: the pattern's swing.
    pub fn steps_controls(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let at = self.pattern()?;
        let Content::Pattern { swing, .. } = self.session.project.region(at).content else {
            return None;
        };
        let segs = SWINGS.map(|s| {
            seg(format!("{s:.0}"), (swing - s).abs() < 0.5, 22.)
                .px(px(7.))
                .font_family(MONO)
                .text_size(px(10.5))
                .debug_selector(move || format!("swing-{s:.0}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.edit(cx, |ws| {
                            let r = &mut ws.session.project.tracks[at.0].regions[at.1];
                            if let Content::Pattern { swing, .. } = &mut r.content {
                                *swing = s;
                            }
                        })
                    }),
                )
        });
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(11.))
                .text_color(rgb(TEXT_3))
                .child("Swing")
                .child(tray().p(px(2.)).rounded(px(7.)).children(segs)),
        )
    }

    pub fn steps_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let at = self.pattern();
        let mut labels = Vec::new();
        let mut grid = Vec::new();
        if let Some(at) = at {
            let region = self.session.project.region(at);
            let Content::Pattern { step, rows, .. } = &region.content else {
                unreachable!()
            };
            let steps = rows.iter().map(|r| r.cells.len()).max().unwrap_or(16);
            let track_color = self.color(at.0);
            let instrument = self.session.project.tracks[at.0].instrument.as_ref();
            let key = self.session.project.song_key();
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
                        .child(row_name(instrument, key, row.pitch)),
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
                        let project = &ws.session.project;
                        let instrument = project.tracks[at.0].instrument.as_ref();
                        let items = match kit_sounds(instrument) {
                            Some(sounds) => sounds
                                .into_iter()
                                .map(|(pitch, name)| (name, MenuAction::AddRow { at, pitch }))
                                .collect(),
                            None => scale_notes(project.song_key())
                                .into_iter()
                                .map(|pitch| {
                                    let key = project.song_key();
                                    let label = crate::pianoroll::name_in(key, pitch);
                                    (label, MenuAction::AddRow { at, pitch })
                                })
                                .collect(),
                        };
                        ws.menu = Some(Menu {
                            at: e.position,
                            items,
                            ..Default::default()
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
