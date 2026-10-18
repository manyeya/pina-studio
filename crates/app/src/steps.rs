//! The step sequencer, as Logic's and FL Studio's channel rack: a pattern region's rows of steps,
//! in the pane under the tracks. Click a step on or off and drag across to paint more (or rub
//! them out); option-click for an accent. Each step's velocity shows as how full it is, and the
//! picked row's velocities stand in a lane under the grid, drawn with the mouse. A row's name plays
//! its sound and picks it; M mutes it; right-click (or its ⋯) for fills, shifts, randomizing, its
//! own length (a polymeter), clearing and removing. The pattern's length (8 to 64 steps), rate and
//! swing are in the editor's bar, with "To MIDI" to edit it in the piano roll. A MIDI region (the
//! drums an agent wrote) becomes a pattern in a click; on the drum kit a row is a drum, on any
//! other instrument a note.

use gpui::{
    Bounds, Context, Div, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, div,
    prelude::*, px, rgb,
};
use tempo_engine::{
    drums,
    music::Key,
    project::{At, Content, Instrument, PPQ, PatternRow, Region, pattern_notes, to_pattern},
};

use crate::{
    menu::{Menu, MenuAction},
    theme::*,
    workspace::{Bottom, Drag, Workspace},
};

const ON: u8 = 100;
const ACCENTED: u8 = 127;
const ROW_H: f32 = 28.0;
const STEP_LANE: f32 = 64.0;

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

/// Something done to one row of a pattern, from its menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RowOp {
    Clear,
    /// A hit every this many steps, from the first.
    Fill(usize),
    /// Every step moved this many later (or earlier), wrapping round.
    Shift(i32),
    Reverse,
    /// Hits here and there: about one step in four, more on the beats.
    Randomize,
    /// Its length, in steps (its own, against the others: a polymeter).
    Length(usize),
    Remove,
}

impl Workspace {
    /// The pattern region the pane shows: the selected one, else the selected track's under the
    /// playhead, else its first.
    pub fn pattern(&self) -> Option<At> {
        let p = &self.session.project;
        let is = |at: At| matches!(p.region(at).content, Content::Pattern { .. });
        self.region.filter(|&at| is(at)).or_else(|| {
            let track = p.tracks.get(self.track)?;
            let tick = self.playhead();
            let mine: Vec<At> = (0..track.regions.len())
                .map(|r| (self.track, r))
                .filter(|&at| is(at))
                .collect();
            mine.iter()
                .copied()
                .find(|&at| {
                    let r = p.region(at);
                    r.start <= tick && tick < r.start + r.length
                })
                .or(mine.first().copied())
        })
    }

    /// Where a new region of `length` can go on a track: the bar the playhead is in, or the
    /// first bar after it clear of the regions there.
    fn free_bar(&self, track: usize, length: u64) -> u64 {
        let bar = self.session.project.bar().max(1);
        let mut start = self.playhead() / bar * bar;
        let regions = &self.session.project.tracks[track].regions;
        while let Some(r) = regions
            .iter()
            .find(|r| r.start < start + length && start < r.start + r.length)
        {
            start = (r.start + r.length).div_ceil(bar) * bar;
        }
        start
    }

    /// A new empty pattern region, a bar long, at the first free bar from the playhead on the
    /// selected instrument track; and opens it.
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
        let start = self.free_bar(self.track, bar);
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
                mute: false,
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
            ws.steps_row = 0;
        });
    }

    /// A MIDI region made a pattern region: a row a pitch, each note on its nearest sixteenth,
    /// its velocity kept; up to four bars long, looped over the region.
    pub fn midi_to_pattern(&mut self, at: At, cx: &mut Context<Self>) {
        let r = self.session.project.region(at).clone();
        let Content::Midi { notes, .. } = &r.content else {
            return;
        };
        let bar = self.session.project.bar();
        let step = PPQ / 4;
        // As long as it takes to hold what it plays: one, two or four bars.
        let span = notes
            .iter()
            .map(|n| n.start.max(0) as u64 + 1)
            .max()
            .unwrap_or(bar);
        let bars = match span.div_ceil(bar) {
            0 | 1 => 1,
            2 => 2,
            _ => 4,
        };
        let steps = ((bar * bars) / step) as usize;
        let rows = to_pattern(notes, step, steps);
        if rows.is_empty() {
            self.status = Some("That region has no notes to make steps of.".into());
            cx.notify();
            return;
        }
        self.edit(cx, |ws| {
            ws.session.project.tracks[at.0].regions[at.1].content = Content::Pattern {
                step,
                rows,
                swing: 50.0,
            };
            ws.region = Some(at);
            ws.bottom = Some(Bottom::Steps);
            ws.steps_row = 0;
        });
        self.status = Some(format!("A pattern now: {steps} steps"));
    }

    /// A pattern region made a MIDI region, to edit in the piano roll.
    fn pattern_to_midi(&mut self, at: At, cx: &mut Context<Self>) {
        let r = self.session.project.region(at).clone();
        let Content::Pattern { step, rows, swing } = &r.content else {
            return;
        };
        let notes = pattern_notes(*step, rows, r.length, *swing);
        self.edit(cx, |ws| {
            ws.session.project.tracks[at.0].regions[at.1].content = Content::Midi {
                notes,
                controls: Vec::new(),
            };
            ws.region = Some(at);
            ws.picked.clear();
        });
        self.show_bottom(Bottom::PianoRoll, cx);
    }

    fn edit_rows(&mut self, at: At, cx: &mut Context<Self>, f: impl FnOnce(&mut Vec<PatternRow>)) {
        self.edit(cx, |ws| {
            if let Content::Pattern { rows, .. } =
                &mut ws.session.project.tracks[at.0].regions[at.1].content
            {
                f(rows);
            }
        });
    }

    fn cell(&self, at: At, row: usize, step: usize) -> Option<u8> {
        match &self.session.project.region(at).content {
            Content::Pattern { rows, .. } => rows.get(row).and_then(|r| r.cells.get(step)).copied(),
            _ => None,
        }
    }

    fn row_pitch(&self, at: At, row: usize) -> Option<u8> {
        match &self.session.project.region(at).content {
            Content::Pattern { rows, .. } => rows.get(row).map(|r| r.pitch),
            _ => None,
        }
    }

    fn set_cell(&mut self, at: At, row: usize, step: usize, v: u8) {
        if let Content::Pattern { rows, .. } =
            &mut self.session.project.tracks[at.0].regions[at.1].content
            && let Some(c) = rows.get_mut(row).and_then(|r| r.cells.get_mut(step))
        {
            *c = v;
        }
    }

    /// A step clicked: on (or off, if it was), or with ⌥ an accent (or off its accent); and the
    /// mouse painting that along as it's dragged.
    fn press_step(
        &mut self,
        at: At,
        row: usize,
        step: usize,
        accent: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(now) = self.cell(at, row, step) else {
            return;
        };
        let paint = match (now, accent) {
            (v, true) if v >= ACCENTED => ON,
            (_, true) => ACCENTED,
            (0, false) => ON,
            _ => 0,
        };
        self.steps_row = row;
        self.begin_drag(Drag::Steps { at, paint }, cx);
        self.set_cell(at, row, step, paint);
        if paint > 0
            && let Some(id) = self.session.project.tracks.get(at.0).map(|t| t.id)
            && let Some(pitch) = self.row_pitch(at, row)
        {
            self.player.preview(id, pitch, paint);
            self.player.preview(id, pitch, 0);
        }
        self.changed(cx);
    }

    /// The mouse over a step while painting: it takes the paint.
    fn paint_step(&mut self, row: usize, step: usize, cx: &mut Context<Self>) {
        let Some(Drag::Steps { at, paint }) = self.drag else {
            return;
        };
        if self.cell(at, row, step).is_some_and(|w| w != paint) {
            self.set_cell(at, row, step, paint);
            self.changed(cx);
        }
    }

    /// The picked row's velocity at a step, from where the mouse is in the lane (a step that's
    /// on; a rest stays one).
    fn draw_step_velocity(&mut self, at: At, step: usize, y: f32, cx: &mut Context<Self>) {
        let b = self.steps_lane.get();
        let (top, h) = (f32::from(b.origin.y) + 4.0, f32::from(b.size.height) - 8.0);
        let v = (((top + h - y) / h.max(1.0)) * 127.0)
            .round()
            .clamp(1.0, 127.0) as u8;
        let row = self.steps_row;
        if self.cell(at, row, step).is_some_and(|c| c > 0) {
            self.set_cell(at, row, step, v);
            self.changed(cx);
        }
    }

    pub fn row_op(&mut self, at: At, row: usize, op: RowOp, cx: &mut Context<Self>) {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(7, |d| d.as_nanos() as u64);
        self.edit_rows(at, cx, |rows| {
            if row >= rows.len() {
                return;
            }
            let cells = &mut rows[row].cells;
            let n = cells.len();
            match op {
                RowOp::Clear => cells.iter_mut().for_each(|c| *c = 0),
                RowOp::Fill(every) => {
                    for (k, c) in cells.iter_mut().enumerate() {
                        *c = if k % every.max(1) == 0 { ON } else { 0 };
                    }
                }
                RowOp::Shift(by) => {
                    if n > 0 {
                        cells.rotate_right(by.rem_euclid(n as i32) as usize);
                    }
                }
                RowOp::Reverse => cells.reverse(),
                RowOp::Randomize => {
                    let mut s = seed | 1;
                    for (k, c) in cells.iter_mut().enumerate() {
                        s ^= s << 13;
                        s ^= s >> 7;
                        s ^= s << 17;
                        let chance = if k % 4 == 0 { 45 } else { 20 };
                        *c = if s % 100 < chance {
                            70 + ((s >> 8) % 57) as u8
                        } else {
                            0
                        };
                    }
                }
                RowOp::Length(len) => cells.resize(len.max(1), 0),
                RowOp::Remove => {
                    rows.remove(row);
                }
            }
        });
        if op == RowOp::Remove {
            self.steps_row = self.steps_row.saturating_sub(1);
        }
    }

    /// The pattern's length in steps: every row taken to it, a row's steps repeated when it
    /// grows (so a bar's groove fills two), cut when it shrinks; the region at least a pass long.
    fn set_pattern_steps(&mut self, at: At, steps: usize, cx: &mut Context<Self>) {
        self.edit(cx, |ws| {
            let r = &mut ws.session.project.tracks[at.0].regions[at.1];
            if let Content::Pattern { rows, step, .. } = &mut r.content {
                for row in rows.iter_mut() {
                    let had = row.cells.len().max(1);
                    row.cells = (0..steps)
                        .map(|k| row.cells.get(k % had).copied().unwrap_or(0))
                        .collect();
                }
                r.length = r.length.max(*step * steps as u64);
            }
        });
    }

    fn set_pattern_rate(&mut self, at: At, rate: u64, cx: &mut Context<Self>) {
        self.edit(cx, |ws| {
            if let Content::Pattern { step, .. } =
                &mut ws.session.project.tracks[at.0].regions[at.1].content
            {
                *step = rate;
            }
        });
    }

    /// The step sequencer's own controls in the editor's bar: length, rate, swing, to MIDI.
    pub fn steps_controls(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let at = self.pattern()?;
        let Content::Pattern {
            swing,
            step,
            ref rows,
        } = self.session.project.region(at).content
        else {
            return None;
        };
        let steps = rows.iter().map(|r| r.cells.len()).max().unwrap_or(16);
        let length = [8usize, 16, 32, 64].map(|n| {
            seg(n.to_string(), steps == n, 22.)
                .px(px(7.))
                .font_family(MONO)
                .text_size(px(10.5))
                .debug_selector(move || format!("steps-{n}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.set_pattern_steps(at, n, cx)),
                )
        });
        let rates = [("1/8", PPQ / 2), ("1/16", PPQ / 4), ("1/32", PPQ / 8)].map(|(label, r)| {
            seg(label, step == r, 22.)
                .px(px(7.))
                .font_family(MONO)
                .text_size(px(10.5))
                .debug_selector(move || format!("rate-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.set_pattern_rate(at, r, cx)),
                )
        });
        let swing_button = button(format!("Swing {swing:.0}%"), swing > 50.0)
            .debug_selector(|| "pattern-swing".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    let items = SWINGS
                        .iter()
                        .map(|&s| {
                            let name = match s {
                                50.0 => "50%  straight".to_string(),
                                66.0 => "66%  triplet feel".to_string(),
                                s => format!("{s:.0}%"),
                            };
                            (name, MenuAction::PatternSwing { at, swing: s })
                        })
                        .collect();
                    ws.menu = Some(Menu {
                        at: e.position,
                        items,
                        ..Default::default()
                    });
                    cx.notify();
                }),
            );
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .min_w(px(0.))
                .overflow_hidden()
                .child(tray().p(px(2.)).rounded(px(7.)).children(length))
                .child(tray().p(px(2.)).rounded(px(7.)).children(rates))
                .child(swing_button)
                .child(
                    button("To MIDI", false)
                        .debug_selector(|| "pattern-to-midi".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| ws.pattern_to_midi(at, cx)),
                        ),
                ),
        )
    }

    pub fn steps_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(at) = self.pattern() else {
            return self.steps_none(cx).into_any_element();
        };
        let region = self.session.project.region(at).clone();
        let Content::Pattern { step, rows, .. } = &region.content else {
            unreachable!()
        };
        let steps = rows
            .iter()
            .map(|r| r.cells.len())
            .max()
            .unwrap_or(16)
            .max(1);
        let color = self.color(at.0);
        let instrument = self.session.project.tracks[at.0].instrument.clone();
        let key = self.session.project.song_key();
        let track_id = self.session.project.tracks[at.0].id;
        let picked_row = self.steps_row.min(rows.len().saturating_sub(1));
        // The step under the playhead, while playing in the region (each row on its own loop).
        let tick = self.playhead();
        let now = (self.player.playing()
            && (region.start..region.start + region.length).contains(&tick))
        .then(|| ((tick - region.start) / (*step).max(1)) as usize);
        // Steps as wide as the room allows, between 16 and 46 pixels.
        let room = f32::from(self.lanes_bounds.get().size.width).max(400.0) - 24.0;
        let groups = steps.div_ceil(4) as f32;
        let cell = ((room - groups * 6.0) / steps as f32 - 2.0).clamp(16.0, 46.0);
        let mut headers = Vec::new();
        let mut grid = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            let name = row_name(instrument.as_ref(), key, row.pitch);
            let picked = r == picked_row;
            let mute = row.mute;
            let len = row.cells.len();
            headers.push(
                div()
                    .id(("row", r))
                    .group("row")
                    .h(px(ROW_H))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(picked, |d| d.bg(rgb(RAISED)))
                    .when(!picked, |d| d.hover(|d| d.bg(rgb(OFF))))
                    .child(
                        div()
                            .size(px(18.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .text_size(px(9.5))
                            .font_weight(FontWeight::BOLD)
                            .bg(if mute { rgb(SOLO) } else { rgb(0x23232a) })
                            .text_color(rgb(if mute { BG } else { TEXT_3 }))
                            .debug_selector(move || format!("row-mute-{r}"))
                            .child("M")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.edit_rows(at, cx, |rows| {
                                        if let Some(row) = rows.get_mut(r) {
                                            row.mute ^= true;
                                        }
                                    });
                                }),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(12.))
                            .text_color(rgb(if mute {
                                TEXT_4
                            } else if picked {
                                TEXT
                            } else {
                                TEXT_2
                            }))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(name),
                    )
                    .when(len != steps, |d| {
                        d.child(
                            div()
                                .font_family(MONO)
                                .text_size(px(10.))
                                .text_color(rgb(AMBER))
                                .child(format!("{len}")),
                        )
                    })
                    .child(
                        div()
                            .invisible()
                            .group_hover("row", |d| d.visible())
                            .size(px(18.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .hover(|d| d.bg(alpha(0xffffff, 0.08)))
                            .debug_selector(move || format!("row-more-{r}"))
                            .child(icon(Icon::More, 12.).text_color(rgb(TEXT_3)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.row_menu(at, r, e, cx);
                                }),
                            ),
                    )
                    .debug_selector(move || format!("row-{r}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            // Picks the row (its velocities in the lane) and plays its sound.
                            ws.steps_row = r;
                            if let Some(pitch) = ws.row_pitch(at, r) {
                                ws.player.preview(track_id, pitch, 100);
                                ws.begin_drag(
                                    Drag::Key {
                                        track: track_id,
                                        pitch,
                                    },
                                    cx,
                                );
                            }
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| ws.row_menu(at, r, e, cx)),
                    ),
            );
            let cells = (0..steps).map(|k| {
                let inside = k < len;
                let v = row.cells.get(k).copied().unwrap_or(0);
                let playing = now.is_some_and(|n| inside && n % len.max(1) == k);
                let off = if (k / 4) % 2 == 0 { 0x1d1d23 } else { 0x16161b };
                let fill = v as f32 / 127.0;
                div()
                    .w(px(cell))
                    .h(px(ROW_H - 4.))
                    .flex_none()
                    .relative()
                    .rounded(px(4.))
                    .overflow_hidden()
                    .when(k % 4 == 0 && k > 0, |d| d.ml(px(6.)))
                    .bg(rgb(match (inside, playing) {
                        (false, _) => 0x101013,
                        (true, true) => RAISED,
                        _ => off,
                    }))
                    .when(v > 0, |d| {
                        // How hard it hits: how full it is.
                        d.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .bottom_0()
                                .h(gpui::relative(0.35 + 0.65 * fill))
                                .bg(if mute {
                                    alpha(color, 0.3)
                                } else {
                                    alpha(color, 0.55 + 0.45 * fill)
                                }),
                        )
                    })
                    .when(v >= ACCENTED, |d| {
                        d.border_1().border_color(rgb(tint(color, 0.6)))
                    })
                    .when(playing, |d| d.border_1().border_color(rgb(TEXT)))
                    .cursor_pointer()
                    .debug_selector(move || format!("step-{r}-{k}"))
                    .when(inside, |d| {
                        d.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.press_step(at, r, k, e.modifiers.alt, cx);
                            }),
                        )
                        .on_mouse_move(cx.listener(
                            move |ws, e: &MouseMoveEvent, _, cx| {
                                if e.pressed_button == Some(MouseButton::Left) {
                                    ws.paint_step(r, k, cx);
                                }
                            },
                        ))
                    })
            });
            grid.push(
                div()
                    .h(px(ROW_H))
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .children(cells),
            );
        }
        // The picked row's velocities, a bar a step, drawn with the mouse.
        let lane_row = rows.get(picked_row).cloned();
        let lane_bounds = self.steps_lane.clone();
        let bars = (0..steps).map(|k| {
            let v = lane_row
                .as_ref()
                .and_then(|r| r.cells.get(k))
                .copied()
                .unwrap_or(0);
            div()
                .w(px(cell))
                .h_full()
                .flex_none()
                .flex()
                .items_end()
                .when(k % 4 == 0 && k > 0, |d| d.ml(px(6.)))
                .debug_selector(move || format!("step-velocity-{k}"))
                .child(
                    div()
                        .w_full()
                        .h(gpui::relative(v as f32 / 127.0))
                        .rounded_t(px(3.))
                        .bg(if v > 0 {
                            alpha(color, 0.35 + 0.65 * v as f32 / 127.0)
                        } else {
                            rgb(0x16161b).into()
                        })
                        .when(v > 0, |d| {
                            d.border_t_2().border_color(rgb(tint(color, 0.3)))
                        }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        ws.begin_drag(Drag::StepVelocity, cx);
                        ws.draw_step_velocity(at, k, f32::from(e.position.y), cx);
                    }),
                )
                .on_mouse_move(cx.listener(move |ws, e: &MouseMoveEvent, _, cx| {
                    if e.pressed_button == Some(MouseButton::Left)
                        && matches!(ws.drag, Some(Drag::StepVelocity))
                    {
                        ws.draw_step_velocity(at, k, f32::from(e.position.y), cx);
                    }
                }))
        });
        let lane_name = lane_row
            .as_ref()
            .map(|r| row_name(instrument.as_ref(), key, r.pitch))
            .unwrap_or_default();
        let add = button("+ Sound", false)
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
            );
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .bg(rgb(LANE))
            .child(
                div()
                    .id("steps")
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .items_start()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .w(px(HEADER_W))
                            .flex_none()
                            .min_h_full()
                            .bg(rgb(SURFACE))
                            .p(px(6.))
                            .flex()
                            .flex_col()
                            .border_r_1()
                            .border_color(rgb(LINE))
                            .children(headers)
                            .child(div().pt(px(6.)).child(add)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .p(px(6.))
                            .pl(px(12.))
                            .flex()
                            .flex_col()
                            .children(grid),
                    ),
            )
            .child(
                div()
                    .h(px(STEP_LANE))
                    .flex_none()
                    .flex()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .w(px(HEADER_W))
                            .flex_none()
                            .h_full()
                            .px(px(12.))
                            .flex()
                            .flex_col()
                            .justify_center()
                            .gap(px(2.))
                            .bg(rgb(SURFACE))
                            .border_r_1()
                            .border_color(rgb(LINE))
                            .child(div().font_weight(FontWeight::MEDIUM).child("Velocity"))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(format!("{lane_name} · draw it")),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .relative()
                            .px(px(6.))
                            .pl(px(12.))
                            .py(px(4.))
                            .bg(rgb(WELL))
                            .debug_selector(|| "steps-lane".into())
                            .child(
                                gpui::canvas(
                                    move |b: Bounds<Pixels>, _, _| lane_bounds.set(b),
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .child(div().h_full().flex().gap(px(2.)).children(bars)),
                    ),
            )
            .into_any_element()
    }

    /// A row's menu: fills, shifts, its own length and the rest.
    fn row_menu(&mut self, at: At, r: usize, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let steps = match &self.session.project.region(at).content {
            Content::Pattern { rows, .. } => rows.iter().map(|r| r.cells.len()).max().unwrap_or(16),
            _ => 16,
        };
        let op = |label: &str, op: RowOp| (label.to_string(), MenuAction::Row { at, row: r, op });
        let mut items = vec![
            op("Every step", RowOp::Fill(1)),
            op("Every 2 steps", RowOp::Fill(2)),
            op("Every 4 steps", RowOp::Fill(4)),
            op("Every 8 steps", RowOp::Fill(8)),
            op("Shift later  →", RowOp::Shift(1)),
            op("Shift earlier  ←", RowOp::Shift(-1)),
            op("Reverse", RowOp::Reverse),
            op("Randomize", RowOp::Randomize),
            op("Clear", RowOp::Clear),
        ];
        // Its own length, against the pattern's: three against four, five against eight.
        for len in [3usize, 5, 6, 7, 12].into_iter().filter(|&l| l < steps) {
            items.push(op(&format!("Length {len} (polymeter)"), RowOp::Length(len)));
        }
        items.push(op(&format!("Length {steps}"), RowOp::Length(steps)));
        items.push(op("Remove row", RowOp::Remove));
        self.steps_row = r;
        self.menu = Some(Menu {
            at: e.position,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    /// No pattern to show: one to make, or the MIDI region there to make one of.
    fn steps_none(&mut self, cx: &mut Context<Self>) -> Div {
        let p = &self.session.project;
        let track = p.tracks.get(self.track);
        let midi = self
            .region
            .filter(|&at| matches!(p.region(at).content, Content::Midi { .. }))
            .or_else(|| {
                let t = track?;
                t.regions
                    .iter()
                    .position(|r| matches!(r.content, Content::Midi { .. }))
                    .map(|r| (self.track, r))
            });
        let instrument = track.is_some_and(|t| t.instrument.is_some());
        let name = track.map_or(String::new(), |t| t.name.clone());
        let card = |title: String, about: &'static str| {
            div()
                .w(px(300.))
                .p(px(16.))
                .rounded(px(12.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(LINE))
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(px(13.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(TEXT_3))
                        .child(about),
                )
        };
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(14.))
            .bg(rgb(LANE))
            .when(!instrument, |d| {
                d.child(div().text_color(rgb(TEXT_3)).child(
                    "Select an instrument track (a drum kit, a sampler or a synth) to sequence it in steps.",
                ))
            })
            .when(instrument, |d| {
                d.child(
                    card(
                        format!("A new pattern on {name}"),
                        "A bar of sixteen steps at the playhead (or the first free bar after it), its sounds ready.",
                    )
                    .child(
                        button("New Pattern    ⌥⌘P", true)
                            .debug_selector(|| "steps-new".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| ws.new_pattern(cx)),
                            ),
                    ),
                )
                .when_some(midi, |d, at| {
                    d.child(
                        card(
                            "Its MIDI, as steps".into(),
                            "The notes of the region there (drums an agent wrote, say), each on its nearest step, velocities kept.",
                        )
                        .child(
                            button("Make It a Pattern", false)
                                .debug_selector(|| "steps-convert".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| ws.midi_to_pattern(at, cx)),
                                ),
                        ),
                    )
                })
            })
    }
}
