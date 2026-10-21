//! The step sequencer: a pattern region's sounds as pads (tap one to
//! hear it and pick it, right-click for its menu, an empty one adds a sound) beside every sound's
//! steps, each in its own colour (a kick warm, hats cool). Click a step on or off and drag across
//! to paint more (or rub them out); option-click for an accent (a white dot). How hard a step hits
//! shows in how bright it is, and the picked sound's velocities open under its steps, drawn with
//! the mouse. A sound with its own length (a polymeter) says so, its repeats drawn faint. Under the
//! grid, what can be done to the picked sound: fill every 1, 2, 4 or 8, shift, reverse, randomize,
//! Euclid's hits spread evenly, its length, clear, remove. The pattern's length (8 to 64 steps),
//! rate and swing are in the editor's bar, with "To MIDI" to edit it in the piano roll. A MIDI
//! region (the drums an agent wrote) becomes a pattern in a click; on the drum kit a row is a
//! drum, on any other instrument a note.

use gpui::{
    Bounds, Context, Div, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, div,
    prelude::*, px, rgb,
};
use pina_engine::{
    drums,
    modulate::{ModKind, ModTarget, SoundParam},
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
pub(crate) fn row_name(instrument: Option<&Instrument>, key: Option<Key>, pitch: u8) -> String {
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
/// A row's colour, by what it sounds like (a kick warm, hats cool), else the palette in turn.
fn row_color(name: &str, index: usize) -> u32 {
    let n = name.to_lowercase();
    let has = |w: &[&str]| w.iter().any(|w| n.contains(w));
    match () {
        _ if has(&["kick", "bd", "808"]) => CORAL,
        _ if has(&["snare", "sd", "rim"]) => AMBER,
        _ if has(&["clap", "snap"]) => PINK,
        _ if has(&["open"]) => SKY,
        _ if has(&["hat", "hh"]) => TEAL,
        _ if has(&["tom"]) => VIOLET,
        _ if has(&["crash", "ride", "cymbal", "splash", "china"]) => SLATE,
        _ if has(&[
            "perc", "shaker", "conga", "bongo", "tamb", "cow", "clave", "wood",
        ]) =>
        {
            MINT
        }
        _ => [SKY, TEAL, MINT, AMBER, CORAL, PINK, VIOLET, SLATE][index % 8],
    }
}

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
    /// This many hits spread as evenly as they go over its steps, from the first (Euclid's).
    Euclid(usize),
    Remove,
}

/// What the lane under the picked sound shows and draws, its edit
/// mode: each step's velocity, chance, pitch, ratchet, gate or nudge.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum LaneMode {
    #[default]
    Velocity,
    Chance,
    Pitch,
    Repeat,
    Gate,
    Nudge,
}

impl LaneMode {
    pub const ALL: [LaneMode; 6] = [
        LaneMode::Velocity,
        LaneMode::Chance,
        LaneMode::Pitch,
        LaneMode::Repeat,
        LaneMode::Gate,
        LaneMode::Nudge,
    ];

    pub fn name(self) -> &'static str {
        match self {
            LaneMode::Velocity => "Velocity",
            LaneMode::Chance => "Chance",
            LaneMode::Pitch => "Pitch",
            LaneMode::Repeat => "Repeat",
            LaneMode::Gate => "Gate",
            LaneMode::Nudge => "Nudge",
        }
    }

    /// What a modulator dropped on the lane moves.
    pub fn sound_param(self) -> Option<SoundParam> {
        match self {
            LaneMode::Velocity => Some(SoundParam::Velocity),
            LaneMode::Chance => Some(SoundParam::Chance),
            LaneMode::Pitch => Some(SoundParam::Pitch),
            _ => None,
        }
    }

    /// A step's value in this lane, as a height (0 to 1, or -1 to 1 from the middle), and as
    /// text when it isn't as usual.
    fn shown(self, row: &PatternRow, i: usize) -> (f32, bool, Option<String>) {
        match self {
            LaneMode::Velocity => {
                let v = row.cells.get(i).copied().unwrap_or(0);
                (
                    v as f32 / 127.0,
                    false,
                    (v >= ACCENTED).then(|| "acc".into()),
                )
            }
            LaneMode::Chance => {
                let c = row.chance_at(i);
                (c as f32 / 100.0, false, (c < 100).then(|| format!("{c}%")))
            }
            LaneMode::Pitch => {
                let t = row.transpose_at(i);
                (t as f32 / 12.0, true, (t != 0).then(|| format!("{t:+}")))
            }
            LaneMode::Repeat => {
                let n = row.repeat_at(i);
                (n as f32 / 4.0, false, (n > 1).then(|| format!("×{n}")))
            }
            LaneMode::Gate => {
                let g = row.gate_at(i);
                (
                    g as f32 / 200.0,
                    false,
                    (g != pina_engine::project::GATE).then(|| format!("{g}%")),
                )
            }
            LaneMode::Nudge => {
                let n = row.nudge_at(i);
                (n as f32 / 50.0, true, (n != 0).then(|| format!("{n:+}")))
            }
        }
    }
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
        self.new_pattern_at(None, cx);
    }

    /// The same at a tick (snapped), from a lane's menu.
    pub fn new_pattern_at(&mut self, at: Option<u64>, cx: &mut Context<Self>) {
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
        let start = at.map_or_else(|| self.free_bar(self.track, bar), |t| self.snap(t as f64));
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
                ..Default::default()
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
    pub fn pattern_to_midi(&mut self, at: At, cx: &mut Context<Self>) {
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
        self.steps_pick = Some(row);
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
        let u = ((top + h - y) / h.max(1.0)).clamp(0.0, 1.0);
        let (r, mode) = (self.steps_row, self.steps_lane_mode);
        if mode == LaneMode::Velocity {
            let v = (u * 127.0).round().clamp(1.0, 127.0) as u8;
            if self.cell(at, r, step).is_some_and(|c| c > 0) {
                self.set_cell(at, r, step, v);
                self.changed(cx);
            }
            return;
        }
        let snap = |v: f32, by: f32| (v / by).round() * by;
        if let Content::Pattern { rows, .. } =
            &mut self.session.project.tracks[at.0].regions[at.1].content
            && let Some(row) = rows.get_mut(r)
            && step < row.cells.len()
        {
            row.fill_lanes();
            match mode {
                LaneMode::Chance => row.chance[step] = snap(u * 100.0, 5.0) as u8,
                LaneMode::Pitch => row.transpose[step] = ((u * 2.0 - 1.0) * 12.0).round() as i8,
                LaneMode::Repeat => row.repeat[step] = 1 + (u * 3.999).floor() as u8,
                LaneMode::Gate => row.gate[step] = snap(5.0 + u * 195.0, 5.0) as u8,
                LaneMode::Nudge => row.nudge[step] = snap((u * 2.0 - 1.0) * 50.0, 5.0) as i8,
                LaneMode::Velocity => {}
            }
            row.tidy_lanes();
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
            // The lanes go where their steps go.
            if matches!(op, RowOp::Shift(_) | RowOp::Reverse) {
                rows[row].fill_lanes();
            }
            let n = rows[row].cells.len();
            match op {
                RowOp::Shift(by) if n > 0 => {
                    let by = by.rem_euclid(n as i32) as usize;
                    let r = &mut rows[row];
                    r.chance.rotate_right(by);
                    r.transpose.rotate_right(by);
                    r.repeat.rotate_right(by);
                    r.gate.rotate_right(by);
                    r.nudge.rotate_right(by);
                }
                RowOp::Reverse => {
                    let r = &mut rows[row];
                    r.chance.reverse();
                    r.transpose.reverse();
                    r.repeat.reverse();
                    r.gate.reverse();
                    r.nudge.reverse();
                }
                RowOp::Clear => {
                    let r = &mut rows[row];
                    (r.chance, r.transpose, r.repeat, r.gate, r.nudge) =
                        (vec![], vec![], vec![], vec![], vec![]);
                }
                _ => {}
            }
            rows[row].tidy_lanes();
            let cells = &mut rows[row].cells;
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
                RowOp::Euclid(hits) => {
                    for (k, c) in cells.iter_mut().enumerate() {
                        *c = if (k * hits) % n.max(1) < hits { ON } else { 0 };
                    }
                }
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

    /// The step sequencer: the sounds as pads (tap one to hear it and
    /// pick it), every sound's steps in its own colour, the picked one opened on its velocities,
    /// and what can be done to it in a bar under them.
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
        let instrument = self.session.project.tracks[at.0].instrument.clone();
        let key = self.session.project.song_key();
        let track_id = self.session.project.tracks[at.0].id;
        let picked_row = self.steps_row.min(rows.len().saturating_sub(1));
        let names: Vec<String> = rows
            .iter()
            .map(|r| row_name(instrument.as_ref(), key, r.pitch))
            .collect();
        let colors: Vec<u32> = names
            .iter()
            .enumerate()
            .map(|(i, n)| row_color(n, i))
            .collect();
        // The step under the playhead, while playing in the region (each row on its own loop).
        let tick = self.playhead();
        let now = (self.player.playing()
            && (region.start..region.start + region.length).contains(&tick))
        .then(|| ((tick - region.start) / (*step).max(1)) as usize);
        let hit_now = |r: usize| {
            let cells = &rows[r].cells;
            now.is_some_and(|n| !cells.is_empty() && cells[n % cells.len()] > 0)
        };

        // The pads: a sound each, sixteen to a page; tap to hear it and pick it.
        let pad_h = 52.0;
        let pads = (0..16).map(|p| {
            let Some(row) = rows.get(p) else {
                return div()
                    .h(px(pad_h))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(9.))
                    .border_1()
                    .border_dashed()
                    .border_color(rgb(LINE_STRONG))
                    .text_color(rgb(TEXT_4))
                    .text_size(px(16.))
                    .cursor_pointer()
                    .hover(|d| d.text_color(rgb(TEXT_2)).border_color(rgb(TEXT_4)))
                    .debug_selector(move || format!("pad-{p}"))
                    .child("+")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            ws.add_sound_menu(at, e, cx)
                        }),
                    );
            };
            let (color, picked, hit) = (colors[p], p == picked_row, hit_now(p));
            let pitch = row.pitch;
            let mute = row.mute;
            div()
                .h(px(pad_h))
                .p(px(7.))
                .flex()
                .flex_col()
                .justify_between()
                .rounded(px(9.))
                .border_color(if picked {
                    rgb(TEXT).into()
                } else {
                    alpha(color, 0.45)
                })
                .when(picked, |d| d.border_2())
                .when(!picked, |d| d.border_1())
                .bg(alpha(
                    color,
                    if hit {
                        0.45
                    } else if mute {
                        0.06
                    } else {
                        0.16
                    },
                ))
                .when(hit, |d| {
                    d.shadow(vec![gpui::BoxShadow {
                        color: alpha(color, 0.5),
                        offset: gpui::point(px(0.), px(0.)),
                        blur_radius: px(14.),
                        spread_radius: px(0.),
                        inset: false,
                    }])
                })
                .cursor_pointer()
                .hover(|d| d.bg(alpha(color, 0.26)))
                .debug_selector(move || format!("pad-{p}"))
                .child(
                    div()
                        .text_size(px(11.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(if mute { TEXT_4 } else { TEXT }))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(names[p].clone()),
                )
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(9.5))
                        .text_color(rgb(TEXT_3))
                        .child(crate::pianoroll::name_in(None, pitch)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.steps_row = p;
                        ws.player.preview(track_id, pitch, 100);
                        ws.begin_drag(
                            Drag::Key {
                                track: track_id,
                                pitch,
                            },
                            cx,
                        );
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| ws.row_menu(at, p, e, cx)),
                )
        });
        let pads = div()
            .w(px(self.header_w))
            .flex_none()
            .h_full()
            .p(px(10.))
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(div().grid().grid_cols(4).gap(px(7.)).children(pads));

        // A row's steps, and the picked row's velocities under them, in the same columns.
        const LABEL_W: f32 = 120.0;
        let beat_gap = |k: usize, d: Div| d.when(k.is_multiple_of(4) && k > 0, |d| d.ml(px(6.)));
        let mut grid = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            let (color, picked, mute, len) =
                (colors[r], r == picked_row, row.mute, row.cells.len());
            let label = div()
                .w(px(LABEL_W))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(6.))
                .pr(px(6.))
                .child(
                    div()
                        .size(px(8.))
                        .flex_none()
                        .rounded(px(2.))
                        .bg(rgb(color)),
                )
                .child(
                    div()
                        .id(("row", r))
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
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
                        .cursor_pointer()
                        .debug_selector(move || format!("row-{r}"))
                        .child(names[r].clone())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
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
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                ws.row_menu(at, r, e, cx)
                            }),
                        ),
                )
                .when(len != steps, |d| {
                    d.child(
                        div()
                            .flex_none()
                            .px(px(4.))
                            .rounded(px(3.))
                            .bg(alpha(AMBER, 0.16))
                            .font_family(MONO)
                            .text_size(px(9.5))
                            .text_color(rgb(AMBER))
                            .child(format!("{len}")),
                    )
                })
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
                        .cursor_pointer()
                        .bg(rgb(if mute { ACCENT } else { 0x23232a }))
                        .text_color(rgb(if mute { 0xffffff } else { TEXT_3 }))
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
                );
            let cells = (0..steps).map(|k| {
                let inside = k < len;
                let v = row.cells.get(k % len.max(1)).copied().unwrap_or(0);
                let playing = now.is_some_and(|n| n % len.max(1) == k % len.max(1)) && inside;
                let base = if (k / 4) % 2 == 0 { 0x19191f } else { 0x202027 };
                let strength = 0.45 + 0.55 * v as f32 / 127.0;
                let cell = div()
                    .flex_1()
                    .h(px(22.))
                    .relative()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(if playing {
                        alpha(0xffffff, 0.6)
                    } else {
                        alpha(0xffffff, 0.0)
                    })
                    .bg(match (v > 0, inside) {
                        (false, true) => rgb(base).into(),
                        (false, false) => rgb(0x141418).into(),
                        (true, true) if mute => alpha(color, 0.25),
                        (true, true) => alpha(color, strength),
                        (true, false) => alpha(color, 0.22),
                    })
                    .when(v >= ACCENTED && inside, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(3.))
                                .right(px(3.))
                                .size(px(5.))
                                .rounded(px(2.5))
                                .bg(rgb(0xffffff)),
                        )
                    })
                    // Its own settings, marked: a ratchet split, a chance faded, a pitch moved.
                    .when(v > 0 && inside, |d| {
                        let i = k % len.max(1);
                        let (n, chance, t) =
                            (row.repeat_at(i), row.chance_at(i), row.transpose_at(i));
                        d.when(chance < 100, |d| {
                            d.opacity(0.45 + 0.55 * chance as f32 / 100.0)
                        })
                        .when(n > 1, |d| {
                            d.child(div().absolute().inset_0().flex().children((0..n).map(|j| {
                                div().flex_1().h_full().when(j > 0, |d| {
                                    d.border_l_1().border_color(alpha(0x000000, 0.55))
                                })
                            })))
                        })
                        .when(chance < 100 || t != 0, |d| {
                            d.child(
                                div()
                                    .absolute()
                                    .bottom(px(1.))
                                    .left(px(3.))
                                    .right(px(3.))
                                    .flex()
                                    .justify_between()
                                    .font_family(MONO)
                                    .text_size(px(8.5))
                                    .text_color(rgb(0xffffff))
                                    .child(if chance < 100 {
                                        format!("{chance}%")
                                    } else {
                                        String::new()
                                    })
                                    .child(if t != 0 {
                                        format!("{t:+}")
                                    } else {
                                        String::new()
                                    }),
                            )
                        })
                    })
                    .debug_selector(move || format!("step-{r}-{k}"))
                    .when(inside, |d| {
                        d.cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.press_step(at, r, k, e.modifiers.alt, cx);
                                }),
                            )
                            .on_mouse_move(cx.listener(move |ws, e: &MouseMoveEvent, _, cx| {
                                if e.pressed_button == Some(MouseButton::Left) {
                                    ws.paint_step(r, k, cx);
                                }
                            }))
                    });
                beat_gap(k, cell)
            });
            let mut block = div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .px(px(6.))
                .py(px(3.))
                .rounded(px(8.))
                .when(picked, |d| d.bg(rgb(0x17171c)))
                .child(
                    div()
                        .h(px(24.))
                        .flex()
                        .items_center()
                        .gap(px(3.))
                        .child(label)
                        .children(cells),
                );
            if picked {
                // Its lane: each step's velocity, chance, pitch, ratchet, gate or nudge, a bar a
                // step, drawn with the mouse; and where the modulators on the sound move it.
                let mode = self.steps_lane_mode;
                let lane_bounds = self.steps_lane.clone();
                let targets = self.mod_targets.clone();
                let param = mode.sound_param();
                let target = param.map(|p| ModTarget::Sound(r, p));
                let mods = self.session.project.mods_of(at.0);
                let tempo = self.session.project.tempo;
                // Each modulator moving this lane: its colour, depth and kind.
                let moving: Vec<(usize, u32, f32, ModKind)> = mods
                    .iter()
                    .enumerate()
                    .flat_map(|(k, m)| {
                        m.routes
                            .iter()
                            .filter(|route| Some(route.target) == target)
                            .map(move |route| {
                                (
                                    k,
                                    crate::modulation::MOD_COLORS[k % 8],
                                    route.depth,
                                    m.kind.clone(),
                                )
                            })
                    })
                    .collect();
                let offering = matches!(self.drag, Some(Drag::Modulator { track, .. }) if track == at.0)
                    && target.is_some();
                let bars = (0..steps).map(|k| {
                    let inside = k < len;
                    let i = k % len.max(1);
                    let (h, bipolar, text) = mode.shown(row, i);
                    let on = row.cells.get(i).copied().unwrap_or(0) > 0;
                    let lit = alpha(
                        color,
                        if inside {
                            if on { 0.9 } else { 0.4 }
                        } else {
                            0.25
                        },
                    );
                    let room = 34.0;
                    let bar = match bipolar {
                        false => div()
                            .absolute()
                            .bottom_0()
                            .left(gpui::relative(0.2))
                            .w(gpui::relative(0.6))
                            .h(px((h * room).max(2.0)))
                            .rounded_t(px(2.))
                            .bg(if h > 0.0 { lit } else { rgb(RAISED).into() }),
                        true => div()
                            .absolute()
                            .left(gpui::relative(0.2))
                            .w(gpui::relative(0.6))
                            .when(h >= 0.0, |d| d.bottom(px(room / 2.0)))
                            .when(h < 0.0, |d| d.top(px(room / 2.0 + 2.0)))
                            .h(px((h.abs() * room / 2.0).max(1.0)))
                            .rounded(px(1.))
                            .bg(if h != 0.0 { lit } else { rgb(RAISED).into() }),
                    };
                    // Where each modulator has this step, as it falls in the song.
                    let beats = (region.start + k as u64 * *step) as f64 / PPQ as f64;
                    let dots: Vec<Div> = moving
                        .iter()
                        .filter_map(|(_, c, depth, kind)| {
                            let v = pina_engine::modulate::value_at(kind, beats, tempo)?;
                            let (h0, _, _) = mode.shown(row, i);
                            let by = depth * v;
                            let to = match mode {
                                LaneMode::Velocity => (h0 + by / 127.0).clamp(0.0, 1.0),
                                LaneMode::Chance => (h0 + by / 100.0).clamp(0.0, 1.0),
                                _ => (h0 + by / 12.0).clamp(-1.0, 1.0),
                            };
                            let y = match bipolar {
                                false => to * room,
                                true => room / 2.0 + to * room / 2.0,
                            };
                            Some(
                                div()
                                    .absolute()
                                    .left(gpui::relative(0.5))
                                    .ml(px(-3.))
                                    .bottom(px(y - 3.0))
                                    .size(px(6.))
                                    .rounded(px(3.))
                                    .bg(rgb(*c))
                                    .border_1()
                                    .border_color(rgb(0x0b0b0e)),
                            )
                        })
                        .collect();
                    let cell = div()
                        .flex_1()
                        .h_full()
                        .relative()
                        .debug_selector(move || format!("step-velocity-{k}"))
                        .child(bar)
                        .children(dots)
                        .children(text.filter(|_| inside).map(|t| {
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .right_0()
                                .flex()
                                .justify_center()
                                .font_family(MONO)
                                .text_size(px(8.5))
                                .text_color(rgb(TEXT_2))
                                .child(t)
                        }))
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
                        }));
                    beat_gap(k, cell)
                });
                let tabs = LaneMode::ALL.map(|m| {
                    seg(m.name(), m == mode, 20.)
                        .px(px(7.))
                        .text_size(px(10.5))
                        .debug_selector(move || format!("lane-{}", m.name()))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.steps_lane_mode = m;
                                cx.notify();
                            }),
                        )
                });
                let chips: Vec<Div> = moving
                    .iter()
                    .map(|(k, c, depth, _)| {
                        let (k, mt) = (*k, target.unwrap_or(ModTarget::Volume));
                        div()
                            .h(px(20.))
                            .px(px(6.))
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .rounded(px(5.))
                            .bg(alpha(*c, 0.16))
                            .border_1()
                            .border_color(alpha(*c, 0.45))
                            .text_size(px(10.5))
                            .child(div().size(px(6.)).rounded(px(3.)).bg(rgb(*c)))
                            .child(self.mod_name(at.0, k))
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_color(rgb(TEXT_2))
                                    .child(pina_engine::modulate::depth_text(mt, *depth)),
                            )
                            .child(
                                div()
                                    .text_color(rgb(TEXT_3))
                                    .cursor_pointer()
                                    .hover(|d| d.text_color(rgb(TEXT)))
                                    .child("✕")
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            cx.stop_propagation();
                                            ws.unmap(at.0, k, mt, cx);
                                        }),
                                    ),
                            )
                            .cursor_ew_resize()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener({
                                    let depth = *depth;
                                    move |ws, e: &MouseDownEvent, _, cx| {
                                        let p = crate::workspace::pt(e.position);
                                        ws.begin_drag(
                                            Drag::ModDepth {
                                                track: at.0,
                                                index: k,
                                                target: mt,
                                                x: p.x,
                                                y: p.y,
                                                depth,
                                            },
                                            cx,
                                        );
                                    }
                                }),
                            )
                    })
                    .collect();
                let hint = match (param, moving.is_empty()) {
                    (Some(_), true) => "drag a modulator's ↗ here to move it",
                    (Some(_), false) => "",
                    (None, _) => "draw it",
                };
                block = block
                    .child(
                        div()
                            .h(px(22.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .w(px(LABEL_W))
                                    .flex_none()
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_3))
                                    .child("Each step's"),
                            )
                            .child(tray().p(px(2.)).rounded(px(6.)).children(tabs))
                            .children(chips)
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_4))
                                    .child(hint),
                            ),
                    )
                    .child(
                        div()
                            .h(px(40.))
                            .flex()
                            .gap(px(3.))
                            .child(div().w(px(LABEL_W)).flex_none())
                            .child(
                                div()
                                    .flex_1()
                                    .h_full()
                                    .relative()
                                    .flex()
                                    .gap(px(3.))
                                    .rounded(px(5.))
                                    .when(offering, |d| {
                                        d.bg(alpha(ACCENT, 0.1))
                                            .border_1()
                                            .border_color(alpha(ACCENT, 0.7))
                                    })
                                    .debug_selector(|| "steps-lane".into())
                                    .child(
                                        gpui::canvas(
                                            move |b: Bounds<Pixels>, _, _| {
                                                lane_bounds.set(b);
                                                if let Some(mt) = target {
                                                    targets.borrow_mut().push((at.0, mt, b));
                                                }
                                            },
                                            |_, _, _, _| {},
                                        )
                                        .absolute()
                                        .top_0()
                                        .left_0()
                                        .size_full(),
                                    )
                                    .children(bars),
                            ),
                    );
            }
            grid.push(block);
        }
        let add = button("+ Sound", false)
            .debug_selector(|| "add-sound".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| ws.add_sound_menu(at, e, cx)),
            );
        let step_beats = *step as f32 / PPQ as f32;
        let add_param = button("+ Parameter row", false)
            .debug_selector(|| "add-param".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    let t = at.0;
                    let k = ws.session.project.tracks[t].modulators.len();
                    let kind = ModKind::Steps {
                        values: vec![0.5; steps],
                        rate: pina_engine::modulate::ModRate::Beats(step_beats),
                        glide: 0.0,
                    };
                    ws.add_modulator(t, kind, cx);
                    if ws.session.project.tracks[t].modulators.len() > k {
                        ws.menu = Some(Menu {
                            at: e.position,
                            items: ws.target_menu(t, k),
                            ..Default::default()
                        });
                        ws.status =
                            Some("Pick what the row steps through; then paint its steps.".into());
                    }
                    cx.notify();
                }),
            );

        // The track's modulators, to drag onto a sound's lane (or any knob of the track); and
        // its parameter rows (steps modulators, as automation rows), painted in time with
        // the pattern.
        let t = at.0;
        let track_mods = self.session.project.mods_of(t);
        let mod_chips: Vec<Div> = track_mods
            .iter()
            .enumerate()
            .map(|(k, _)| {
                let c = crate::modulation::MOD_COLORS[k % 8];
                div()
                    .h(px(22.))
                    .px(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .rounded(px(6.))
                    .bg(alpha(c, 0.14))
                    .border_1()
                    .border_color(alpha(c, 0.45))
                    .text_size(px(11.))
                    .cursor(gpui::CursorStyle::ClosedHand)
                    .debug_selector(move || format!("steps-mod-{k}"))
                    .child(div().size(px(7.)).rounded(px(3.5)).bg(rgb(c)))
                    .child(self.mod_name(t, k))
                    .child(div().text_color(rgb(TEXT_2)).child("↗"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.click_count == 2 {
                                ws.show_bottom(Bottom::Modulation, cx);
                                return;
                            }
                            ws.mod_drag_at = Some(crate::workspace::pt(e.position));
                            ws.begin_drag(Drag::Modulator { track: t, index: k }, cx);
                        }),
                    )
            })
            .collect();
        let quick = |label: &'static str, kind: ModKind, cx: &mut Context<Self>| {
            button(format!("+ {label}"), false)
                .debug_selector(move || format!("steps-add-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.add_modulator(t, kind.clone(), cx)),
                )
        };
        let strip = div()
            .h(px(30.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(5.))
            .px(px(6.))
            .child(
                div()
                    .w(px(LABEL_W - 6.0))
                    .flex_none()
                    .text_size(px(10.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT_3))
                    .child("MODULATE"),
            )
            .children(mod_chips)
            .child(quick("LFO", ModKind::lfo(), cx))
            .child(quick("Random", ModKind::random(), cx))
            .child(add_param)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child("drag a modulator's ↗ onto a sound's lane, or any knob of the track"),
            );
        let param_rows: Vec<Div> = track_mods
            .iter()
            .enumerate()
            .filter_map(|(k, m)| match &m.kind {
                ModKind::Steps { values, .. } => Some((k, values.clone(), m.routes.clone())),
                _ => None,
            })
            .map(|(k, values, routes)| {
                let c = crate::modulation::MOD_COLORS[k % 8];
                let n = values.len().max(1);
                let label = match routes.first() {
                    Some(r) => format!(
                        "{} {}",
                        self.target_name(t, r.target),
                        pina_engine::modulate::depth_text(r.target, r.depth)
                    ),
                    None => "Map to…".to_string(),
                };
                let id = self.session.project.tracks[t].id;
                let live = self.mod_values.get(&id).map_or(0.0, |v| v[k]);
                let now_step = now;
                let bounds = self.mod_steps.clone();
                let cells = (0..steps).map(|i| {
                    let v = values[i % n];
                    let playing = now_step.is_some_and(|p| p % n == i % n);
                    let cell = div()
                        .flex_1()
                        .h_full()
                        .relative()
                        .rounded(px(3.))
                        .bg(rgb(if (i / 4) % 2 == 0 { 0x17171c } else { 0x1c1c22 }))
                        .when(playing, |d| d.border_1().border_color(alpha(0xffffff, 0.5)))
                        .child(
                            div()
                                .absolute()
                                .bottom_0()
                                .left(gpui::relative(0.15))
                                .w(gpui::relative(0.7))
                                .h(gpui::relative(v.clamp(0.0, 1.0)))
                                .rounded_t(px(2.))
                                .bg(alpha(c, if i < n { 0.75 } else { 0.3 })),
                        );
                    beat_gap(i, cell)
                });
                div()
                    .h(px(36.))
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .px(px(6.))
                    .py(px(3.))
                    .child(
                        div()
                            .w(px(LABEL_W))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .pr(px(6.))
                            .child(div().size(px(8.)).flex_none().rounded(px(2.)).bg(rgb(c)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .text_size(px(11.))
                                    .text_color(rgb(if routes.is_empty() { AMBER } else { TEXT_2 }))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .cursor_pointer()
                                    .debug_selector(move || format!("param-row-{k}"))
                                    .child(label)
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                            cx.stop_propagation();
                                            ws.menu = Some(Menu {
                                                at: e.position,
                                                items: ws.target_menu(t, k),
                                                ..Default::default()
                                            });
                                            cx.notify();
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(9.5))
                                    .text_color(rgb(TEXT_3))
                                    .child(format!("{live:.2}")),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .relative()
                            .flex()
                            .gap(px(3.))
                            .cursor_crosshair()
                            .debug_selector(move || format!("param-cells-{k}"))
                            .child(
                                gpui::canvas(
                                    move |b: Bounds<Pixels>, _, _| {
                                        bounds.borrow_mut().insert((t, k), b);
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .top_0()
                                .left_0()
                                .size_full(),
                            )
                            .children(cells)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.begin_drag(Drag::ModSteps { track: t, index: k }, cx);
                                    ws.paint_mod_step(t, k, crate::workspace::pt(e.position));
                                    ws.changed(cx);
                                }),
                            ),
                    )
            })
            .collect();

        // What can be done to the picked row, all in sight.
        let r = picked_row;
        let (pick_name, pick_color, pick_len) = (
            names.get(r).cloned().unwrap_or_default(),
            colors.get(r).copied().unwrap_or(TEXT_3),
            rows.get(r).map_or(steps, |row| row.cells.len()),
        );
        let op_button = |label: String, op: RowOp, name: &'static str, cx: &mut Context<Self>| {
            button(label, false)
                .debug_selector(move || format!("row-op-{name}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.row_op(at, r, op, cx)),
                )
        };
        let menu_button = |label: String,
                           name: &'static str,
                           items: Vec<(String, MenuAction)>,
                           cx: &mut Context<Self>| {
            button(label, false)
                .debug_selector(move || format!("row-op-{name}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        ws.menu = Some(Menu {
                            at: e.position,
                            items: items.clone(),
                            ..Default::default()
                        });
                        cx.notify();
                    }),
                )
        };
        let row_action = |op: RowOp| MenuAction::Row { at, row: r, op };
        let euclid: Vec<(String, MenuAction)> = (1..=pick_len.min(12))
            .map(|h| {
                (
                    format!("{h} hits over {pick_len}"),
                    row_action(RowOp::Euclid(h)),
                )
            })
            .collect();
        let mut lengths: Vec<(String, MenuAction)> = [3usize, 5, 6, 7, 12]
            .into_iter()
            .filter(|&l| l < steps)
            .map(|l| {
                (
                    format!("{l} steps (against {steps})"),
                    row_action(RowOp::Length(l)),
                )
            })
            .collect();
        lengths.push((format!("{steps} steps"), row_action(RowOp::Length(steps))));
        let label = |t: &'static str| div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(t);
        let bar = (!rows.is_empty()).then(|| {
            div()
                .h(px(42.))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(5.))
                .px(px(12.))
                .border_t_1()
                .border_color(rgb(LINE))
                .bg(rgb(SURFACE))
                .overflow_hidden()
                .child(
                    div()
                        .size(px(9.))
                        .flex_none()
                        .rounded(px(3.))
                        .bg(rgb(pick_color)),
                )
                .child(
                    div()
                        .mr(px(8.))
                        .text_size(px(12.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .whitespace_nowrap()
                        .child(pick_name),
                )
                .child(label("Fill"))
                .child(op_button("1".into(), RowOp::Fill(1), "fill-1", cx))
                .child(op_button("2".into(), RowOp::Fill(2), "fill-2", cx))
                .child(op_button("4".into(), RowOp::Fill(4), "fill-4", cx))
                .child(op_button("8".into(), RowOp::Fill(8), "fill-8", cx))
                .child(div().w(px(1.)).h(px(18.)).mx(px(3.)).bg(rgb(LINE)))
                .child(op_button("←".into(), RowOp::Shift(-1), "earlier", cx))
                .child(op_button("→".into(), RowOp::Shift(1), "later", cx))
                .child(op_button("Reverse".into(), RowOp::Reverse, "reverse", cx))
                .child(op_button(
                    "Randomize".into(),
                    RowOp::Randomize,
                    "randomize",
                    cx,
                ))
                .child(menu_button("Euclid ▾".into(), "euclid", euclid, cx))
                .child(div().flex_1())
                .child(menu_button(
                    format!("Length {pick_len} ▾"),
                    "length",
                    lengths,
                    cx,
                ))
                .child(op_button("Clear".into(), RowOp::Clear, "clear", cx))
                .child(op_button("Remove".into(), RowOp::Remove, "remove", cx))
        });

        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .bg(rgb(LANE))
            .child(
                div().flex_1().min_h(px(0.)).flex().child(pads).child(
                    div()
                        .id("steps")
                        .flex_1()
                        .min_w(px(0.))
                        .h_full()
                        .overflow_y_scroll()
                        .p(px(6.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(strip)
                        .children(grid)
                        .children(param_rows)
                        .child(div().pt(px(4.)).pl(px(6.)).flex().child(add)),
                ),
            )
            .children(bar)
            .into_any_element()
    }

    /// A sound to add as a row: the kit's, or the song's key's notes.
    fn add_sound_menu(&mut self, at: At, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let project = &self.session.project;
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
        self.menu = Some(Menu {
            at: e.position,
            items,
            ..Default::default()
        });
        cx.notify();
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

    /// No pattern to show: one to make, or the track's MIDI to make one of, each shown as what it
    /// would be.
    fn steps_none(&mut self, cx: &mut Context<Self>) -> Div {
        let p = &self.session.project;
        let track = p.tracks.get(self.track);
        let midi = self
            .region
            .filter(|&at| {
                at.0 == self.track && matches!(p.region(at).content, Content::Midi { .. })
            })
            .or_else(|| {
                let t = track?;
                t.regions
                    .iter()
                    .position(|r| matches!(r.content, Content::Midi { .. }))
                    .map(|r| (self.track, r))
            });
        let instrument = track.and_then(|t| t.instrument.clone());
        let name = track.map_or(String::new(), |t| t.name.clone());
        let color = if track.is_some() {
            self.color(self.track)
        } else {
            ACCENT
        };
        let heading = |title: String, about: &'static str| {
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.))
                .child(
                    div()
                        .text_size(px(16.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(rgb(TEXT_3))
                        .child(about),
                )
        };
        let Some(instrument) = instrument else {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(LANE))
                .child(heading(
                    "Steps need an instrument".into(),
                    "Select a drum kit, sampler or synth track to sequence it in steps.",
                ));
        };
        // What a new pattern starts as: its rows, empty.
        let fresh = match (&instrument, kit_sounds(Some(&instrument))) {
            (Instrument::Drums, _) => STARTING_ROWS.len(),
            (_, Some(sounds)) => sounds.len().min(6),
            _ => 4,
        };
        let empty = vec![vec![0u8; 16]; fresh.min(5)];
        // What the MIDI becomes: its busiest rows, the first bar.
        let converted = midi.map(|at| {
            let r = p.region(at);
            let notes = r
                .content
                .notes(r.length)
                .map(|n| n.to_vec())
                .unwrap_or_default();
            let mut rows: Vec<Vec<u8>> = to_pattern(&notes, PPQ / 4, 16)
                .into_iter()
                .map(|r| r.cells)
                .collect();
            rows.sort_by_key(|c| std::cmp::Reverse(c.iter().filter(|&&v| v > 0).count()));
            rows.truncate(5);
            let label = if r.name.is_empty() {
                name.clone()
            } else {
                r.name.clone()
            };
            (at, rows, notes.len(), label)
        });
        let tile = |title: &'static str, hint: Option<&'static str>, caption: String, grid: Div| {
            div()
                .w(px(300.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .p(px(14.))
                .rounded(px(14.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(LINE))
                .cursor_pointer()
                .hover(|d| d.border_color(alpha(color, 0.7)).bg(rgb(0x1e1e24)))
                .child(
                    div()
                        .h(px(104.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(10.))
                        .bg(rgb(WELL))
                        .child(grid),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(13.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .children(hint.map(|h| {
                            div()
                                .px(px(6.))
                                .py(px(2.))
                                .rounded(px(5.))
                                .bg(rgb(RAISED))
                                .font_family(MONO)
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_2))
                                .child(h)
                        })),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(TEXT_3))
                        .child(caption),
                )
        };
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(22.))
            .bg(rgb(LANE))
            .child(heading(
                format!("{name} has no pattern yet"),
                match converted {
                    Some(_) => "Start one, or turn the MIDI already on this track into steps.",
                    None => "Steps are the quickest way to write a beat or a riff.",
                },
            ))
            .child(
                div()
                    .flex()
                    .items_stretch()
                    .gap(px(16.))
                    .child(
                        tile(
                            "New pattern",
                            Some("⌥⌘P"),
                            "One bar of sixteen steps, at the playhead.".into(),
                            mini_grid(&empty, color),
                        )
                        .debug_selector(|| "steps-new".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.new_pattern(cx)),
                        ),
                    )
                    .when_some(converted, |d, (at, rows, count, label)| {
                        d.child(
                            tile(
                                "Convert to steps",
                                None,
                                format!("{label}'s {count} notes, each on its nearest sixteenth."),
                                mini_grid(&rows, color),
                            )
                            .debug_selector(|| "steps-convert".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| ws.midi_to_pattern(at, cx)),
                            ),
                        )
                    }),
            )
    }
}

/// A pattern in miniature: up to five rows of sixteen steps, in fours.
fn mini_grid(rows: &[Vec<u8>], color: u32) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(3.))
        .children(rows.iter().map(|cells| {
            div().flex().gap(px(2.)).children((0..16).map(|k| {
                let v = cells.get(k).copied().unwrap_or(0);
                div()
                    .size(px(12.))
                    .rounded(px(2.5))
                    .when(k % 4 == 0 && k > 0, |d| d.ml(px(4.)))
                    .bg(match v {
                        0 if (k / 4) % 2 == 0 => rgb(0x22222a).into(),
                        0 => rgb(0x1a1a20).into(),
                        v => alpha(color, 0.45 + 0.55 * v as f32 / 127.0),
                    })
            }))
        }))
}
