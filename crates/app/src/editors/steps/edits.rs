//! Editing steps and sounds: cells, painting, velocities, and each sound's menu.

use super::*;

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

impl Workspace {
    pub(crate) fn edit_rows(
        &mut self,
        at: At,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Vec<PatternRow>),
    ) {
        self.edit(cx, |ws| {
            if let Content::Pattern { rows, .. } =
                &mut ws.session.project.tracks[at.0].regions[at.1].content
            {
                f(rows);
            }
        });
    }

    pub(crate) fn cell(&self, at: At, row: usize, step: usize) -> Option<u8> {
        match &self.session.project.region(at).content {
            Content::Pattern { rows, .. } => rows.get(row).and_then(|r| r.cells.get(step)).copied(),
            _ => None,
        }
    }

    pub(crate) fn row_pitch(&self, at: At, row: usize) -> Option<u8> {
        match &self.session.project.region(at).content {
            Content::Pattern { rows, .. } => rows.get(row).map(|r| r.pitch),
            _ => None,
        }
    }

    pub(crate) fn set_cell(&mut self, at: At, row: usize, step: usize, v: u8) {
        if let Content::Pattern { rows, .. } =
            &mut self.session.project.tracks[at.0].regions[at.1].content
            && let Some(c) = rows.get_mut(row).and_then(|r| r.cells.get_mut(step))
        {
            *c = v;
        }
    }

    /// A step clicked: on (or off, if it was), or with ⌥ an accent (or off its accent); and the
    /// mouse painting that along as it's dragged.
    pub(crate) fn press_step(
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
    pub(crate) fn paint_step(&mut self, row: usize, step: usize, cx: &mut Context<Self>) {
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
    pub(crate) fn draw_step_velocity(
        &mut self,
        at: At,
        step: usize,
        y: f32,
        cx: &mut Context<Self>,
    ) {
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

    /// A sound to add as a row: the kit's, or the song's key's notes.
    pub(crate) fn add_sound_menu(&mut self, at: At, e: &MouseDownEvent, cx: &mut Context<Self>) {
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
                    let label = crate::editors::pianoroll::name_in(key, pitch);
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
    pub(crate) fn row_menu(
        &mut self,
        at: At,
        r: usize,
        e: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
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
}
