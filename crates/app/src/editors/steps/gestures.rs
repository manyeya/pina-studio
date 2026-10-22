//! What the mouse does to steps: a click by the edit mode and the keys held, and what a drag on
//! from it does; found under the pointer from where each line was drawn.

use gpui::{Modifiers, Point};

use super::*;

/// A drag in the step sequencer. A row's line is (row, subrow's lane: none for the row itself).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StepDrag {
    /// On/off, tie or skip painted: each step passed over on the line's kind takes `on`.
    Paint { lane: Option<Lane>, on: bool },
    /// A value dragged up or down from where it was, and set on each step passed over.
    Value {
        row: usize,
        sub: Option<Lane>,
        lane: Lane,
        y: f32,
        from: f32,
        now: f32,
    },
    /// ⌥: each step passed over back as usual (on/off: off).
    Reset { row: usize, lane: Option<Lane> },
    /// ⌥⇧: an even line from `anchor` to the step under the pointer, whose value follows it.
    Ramp {
        row: usize,
        sub: Option<Lane>,
        lane: Lane,
        anchor: usize,
        y: f32,
        from: f32,
    },
    /// ⌘⌥: steps tied from `anchor` to the one under the pointer.
    Tie { row: usize, anchor: usize },
    /// A row's loop's start or end, dragged.
    Loop { row: usize, end: bool },
    /// A row moved up or down by its header.
    Row { from: usize },
    /// The menu bar's zoom slider.
    Zoom,
    /// An inspector field's value dragged up or down.
    Field {
        field: super::Field,
        y: f32,
        from: f32,
    },
    /// A down/up button dragged: every step a notch for each few pixels.
    Notch {
        row: Option<usize>,
        lane: Option<Lane>,
        y: f32,
    },
}

/// How far a drag goes for a lane's whole range, in pixels.
const FULL: f32 = 160.0;

/// Each step's place across a line `width` wide, `count` from `first`: (left, width).
pub fn columns(first: usize, count: usize, width: f32) -> Vec<(f32, f32)> {
    let gap = |k: usize| match k {
        k if k == first => 0.0,
        k if k % 4 == 0 => GROUP_GAP,
        _ => GAP,
    };
    let gaps: f32 = (first..first + count).map(gap).sum();
    let w = ((width - gaps) / count.max(1) as f32).max(1.0);
    let mut x = 0.0;
    (first..first + count)
        .map(|k| {
            x += gap(k);
            let c = (x, w);
            x += w;
            c
        })
        .collect()
}

impl Workspace {
    /// The step under a point: its line, its index, and how far across it (0 to 1).
    pub(crate) fn step_under(&self, p: Point<f32>) -> Option<(usize, Option<Lane>, usize, f32)> {
        let g = self.steps.geom.borrow();
        let (first, count) = g.shown;
        let (&(row, sub), b) = g.lines.iter().find(|(_, b)| {
            let (top, bottom) = (f32::from(b.origin.y), f32::from(b.origin.y + b.size.height));
            (top..bottom).contains(&p.y)
        })?;
        let x = p.x - f32::from(b.origin.x);
        let cols = columns(first, count, f32::from(b.size.width));
        let (k, &(left, w)) = cols.iter().enumerate().min_by(|a, b| {
            let d = |c: &(f32, f32)| (x - (c.0 + c.1 / 2.0)).abs();
            d(a.1).total_cmp(&d(b.1))
        })?;
        Some((row, sub, first + k, ((x - left) / w).clamp(0.0, 1.0)))
    }

    /// The step of a line (row, subrow) across from `x`, wherever the pointer is up or down.
    fn step_across(&self, row: usize, sub: Option<Lane>, x: f32) -> Option<usize> {
        let g = self.steps.geom.borrow();
        let b = g.lines.get(&(row, sub))?;
        let (first, count) = g.shown;
        let x = x - f32::from(b.origin.x);
        columns(first, count, f32::from(b.size.width))
            .iter()
            .enumerate()
            .min_by(|a, b| {
                let d = |c: &(f32, f32)| (x - (c.0 + c.1 / 2.0)).abs();
                d(a.1).total_cmp(&d(b.1))
            })
            .map(|(k, _)| first + k)
    }

    /// The edit mode a line follows: a subrow its own, a row the menu bar's.
    fn line_mode(&self, sub: Option<Lane>) -> EditMode {
        sub.map_or(self.steps.mode, EditMode::Lane)
    }

    /// A step pressed: what it does by the edit mode and the keys held, and the drag it starts.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn press_step(
        &mut self,
        row: usize,
        sub: Option<Lane>,
        step: usize,
        across: f32,
        m: Modifiers,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        let mode = self.line_mode(sub);
        let Some((_, p)) = self.shown_pattern() else {
            return;
        };
        let steps = p.steps();
        let Some(r) = p.rows.get(row).cloned() else {
            return;
        };
        let r = &r;
        let auto = r.is_automation();
        self.steps.row = row;
        // ⇧: picks steps (or unpicks them), for the Step inspector, copying and pasting.
        if m.shift && !m.alt && mode != EditMode::Loop {
            match self.steps.picked.iter().position(|&s| s == (row, step)) {
                Some(k) => {
                    self.steps.picked.remove(k);
                }
                None => self.steps.picked.push((row, step)),
            }
            self.steps.cursor = Some((row, step));
            cx.notify();
            return;
        }
        self.steps.picked.clear();
        self.steps.cursor = Some((row, step));
        let value_lane = mode.lane().filter(|l| !l.switch() && l.applies(auto));
        let drag = match mode {
            EditMode::Loop => {
                let (a, b) = r.loop_range(steps);
                let end = (step + 1).abs_diff(b) < step.abs_diff(a);
                let (a, b) = if end { (a, step + 1) } else { (step, b) };
                if let Some(r) = self.pattern_now().and_then(|p| p.rows.get_mut(row)) {
                    (r.loop_start, r.loop_end) = (a.min(b.saturating_sub(1)), Some(b.max(a + 1)));
                }
                StepDrag::Loop { row, end }
            }
            _ if m.platform && m.alt && !matches!(mode.lane(), Some(Lane::Tie | Lane::Rate)) => {
                let tied = r.tie_at(step) || (step > 0 && r.tie_at(step - 1));
                if let Some(r) = self.pattern_now().and_then(|p| p.rows.get_mut(row)) {
                    match tied {
                        true => r.untie(step),
                        false => r.set_on(step, true),
                    }
                }
                StepDrag::Tie { row, anchor: step }
            }
            _ if m.alt && m.shift && value_lane.is_some() => StepDrag::Ramp {
                row,
                sub,
                lane: value_lane.unwrap(),
                anchor: step,
                y,
                from: r.get(value_lane.unwrap(), step),
            },
            _ if m.alt => {
                let lane = mode.lane();
                self.reset_step(row, lane, step);
                StepDrag::Reset { row, lane }
            }
            EditMode::OnOff => {
                let on = !r.on(step);
                self.paint(row, None, step, on);
                StepDrag::Paint { lane: None, on }
            }
            EditMode::Lane(Lane::Tie) => {
                // The right of a step ties it to the next; the left, the one before to it.
                let at = if across < 0.5 && step > 0 {
                    step - 1
                } else {
                    step
                };
                let on = !r.tie_at(at);
                self.paint(row, Some(Lane::Tie), at, on);
                StepDrag::Paint {
                    lane: Some(Lane::Tie),
                    on,
                }
            }
            EditMode::Lane(Lane::Skip) => {
                let on = !r.skip_at(step);
                self.paint(row, Some(Lane::Skip), step, on);
                StepDrag::Paint {
                    lane: Some(Lane::Skip),
                    on,
                }
            }
            // ⌘ in a value mode: the step on or off.
            EditMode::Lane(_) if m.platform => {
                let on = !r.on(step);
                self.paint(row, None, step, on);
                StepDrag::Paint { lane: None, on }
            }
            EditMode::Lane(lane) if lane.applies(auto) => {
                let from = r.get(lane, step);
                self.steps.tip = Some((self.step_text(row, lane, step), (row, sub, step)));
                StepDrag::Value {
                    row,
                    sub,
                    lane,
                    y,
                    from,
                    now: from,
                }
            }
            EditMode::Lane(_) => return,
        };
        self.begin_drag(Drag::StepSeq(drag), cx);
        self.changed(cx);
    }

    /// A step turned on or off (or its tie or skip): mono turning the others in its column off,
    /// and heard when turned on.
    fn paint(&mut self, row: usize, lane: Option<Lane>, step: usize, on: bool) {
        let mono = self.steps.mono;
        let Some(p) = self.pattern_now() else {
            return;
        };
        let Some(r) = p.rows.get_mut(row) else {
            return;
        };
        match lane {
            None if r.on(step) == on => return,
            None => r.set_on(step, on),
            Some(l) => r.set(l, step, on as u8 as f32),
        }
        r.tidy_lanes();
        if lane.is_none() && on {
            if mono {
                p.mono(step, row);
            }
            self.audition(row, Some(step));
        }
    }

    /// A step's setting in a lane (or on/off) back as usual.
    fn reset_step(&mut self, row: usize, lane: Option<Lane>, step: usize) {
        if let Some(r) = self.pattern_now().and_then(|p| p.rows.get_mut(row)) {
            match lane {
                None => r.set_on(step, false),
                Some(l) if l.applies(r.is_automation()) => {
                    let v = r.default_of(l, step);
                    r.set(l, step, v);
                }
                Some(_) => {}
            }
            r.tidy_lanes();
        }
    }

    /// A step's value in a lane, as the grid writes it.
    pub(crate) fn step_text(&self, row: usize, lane: Lane, step: usize) -> String {
        let Some((at, p)) = self.shown_pattern() else {
            return String::new();
        };
        let Some(r) = p.rows.get(row) else {
            return String::new();
        };
        let v = r.get(lane, step);
        match lane {
            Lane::Velocity if r.is_automation() => match r.target.and_then(|t| t.param()) {
                Some(param) => self.value_text(at.0, param, param.from_unit(v)),
                None => format!("{}", (v * 127.0).round()),
            },
            Lane::Velocity | Lane::Repeat => format!("{v:.0}"),
            Lane::Gate | Lane::Chance => format!("{v:.0}%"),
            Lane::Offset => format!("{v:+.0}%"),
            Lane::Note => {
                let key = self.session.project.song_key();
                crate::editors::pianoroll::name_in(key, r.pitch_at(step))
            }
            Lane::Octave => format!("{v:.0}"),
            Lane::Rate => match r.rate_at(step) {
                Some(t) => pina_engine::song::project::rate_name(t),
                None => pina_engine::song::project::rate_name(p.row_rate(row)),
            },
            Lane::Tie => if v > 0.5 { "Tied" } else { "" }.into(),
            Lane::Skip => if v > 0.5 { "Skip" } else { "" }.into(),
        }
    }

    /// The mouse moving with a step sequencer drag.
    pub(crate) fn steps_drag_to(&mut self, p: Point<f32>, cx: &mut Context<Self>) {
        let Some(Drag::StepSeq(d)) = self.drag else {
            return;
        };
        let hit = self.step_under(p);
        let mode_of = |sub: Option<Lane>, global: EditMode| sub.map_or(global, EditMode::Lane);
        let global = self.steps.mode;
        match d {
            StepDrag::Paint { lane, on } => {
                // Over any line painting the same thing: on/off rows, or tie or skip lines.
                if let Some((row, sub, step, _)) = hit {
                    let line = mode_of(sub, global);
                    let same = match lane {
                        None => line == EditMode::OnOff || line.lane().is_some_and(|l| !l.switch()),
                        Some(l) => line == EditMode::Lane(l),
                    };
                    if same {
                        self.paint(row, lane, step, on);
                    }
                }
            }
            StepDrag::Value {
                row,
                sub,
                lane,
                y,
                from,
                ..
            } => {
                let Some(r) = self.shown_pattern().and_then(|(_, pt)| pt.rows.get(row)) else {
                    return;
                };
                let (lo, hi) = r.range(lane);
                let now = (from + (y - p.y) / FULL * (hi - lo)).clamp(lo, hi);
                let Some(step) = self.step_across(row, sub, p.x) else {
                    return;
                };
                let before = r.get(lane, step);
                if let Some(r) = self.pattern_now().and_then(|pt| pt.rows.get_mut(row)) {
                    r.set(lane, step, now);
                    r.tidy_lanes();
                }
                if let Some(Drag::StepSeq(StepDrag::Value { now: n, .. })) = &mut self.drag {
                    *n = now;
                }
                let changed = self
                    .shown_pattern()
                    .and_then(|(_, pt)| pt.rows.get(row))
                    .is_some_and(|r| r.get(lane, step) != before);
                if changed && matches!(lane, Lane::Note | Lane::Octave) {
                    self.audition(row, Some(step));
                }
                self.steps.tip = Some((self.step_text(row, lane, step), (row, sub, step)));
            }
            StepDrag::Reset { row, lane } => {
                if let Some((r2, _, step, _)) = hit
                    && r2 == row
                {
                    self.reset_step(row, lane, step);
                }
            }
            StepDrag::Ramp {
                row,
                sub,
                lane,
                anchor,
                y,
                from,
            } => {
                // The anchor keeps its value; the step under the pointer takes the drag's, and
                // those between a line from one to the other.
                let Some(step) = self.step_across(row, sub, p.x) else {
                    return;
                };
                let Some(r) = self.shown_pattern().and_then(|(_, pt)| pt.rows.get(row)) else {
                    return;
                };
                let (lo, hi) = r.range(lane);
                let to = (from + (y - p.y) / FULL * (hi - lo)).clamp(lo, hi);
                if step != anchor
                    && let Some(r) = self.pattern_now().and_then(|pt| pt.rows.get_mut(row))
                {
                    r.set(lane, step, to);
                    r.ramp(lane, anchor, step);
                }
                self.steps.tip = Some((self.step_text(row, lane, step), (row, sub, step)));
            }
            StepDrag::Tie { row, anchor } => {
                if let Some((r2, _, step, _)) = hit
                    && r2 == row
                    && step > anchor
                    && let Some(r) = self.pattern_now().and_then(|pt| pt.rows.get_mut(row))
                {
                    r.tie_steps(anchor, step);
                }
            }
            StepDrag::Loop { row, end } => {
                if let Some((_, _, step, _)) = hit
                    && let Some(pt) = self.pattern_now()
                {
                    let n = pt.steps();
                    // Dragged over other rows, theirs follow.
                    let rows: Vec<usize> = match hit {
                        Some((r2, None, ..)) if r2 != row => (row.min(r2)..=row.max(r2)).collect(),
                        _ => vec![row],
                    };
                    for r in rows {
                        if let Some(rw) = pt.rows.get_mut(r) {
                            let (a, b) = rw.loop_range(n);
                            let (a, b) = if end {
                                (a, (step + 1).max(a + 1))
                            } else {
                                (step.min(b - 1), b)
                            };
                            rw.loop_start = a;
                            rw.loop_end = Some(b);
                        }
                    }
                }
            }
            StepDrag::Row { from } => {
                if let Some((to, ..)) = hit
                    && to != from
                    && let Some(pt) = self.pattern_now()
                    && to < pt.rows.len()
                {
                    let row = pt.rows.remove(from);
                    pt.rows.insert(to, row);
                    self.steps.row = to;
                    if let Some(Drag::StepSeq(StepDrag::Row { from: f })) = &mut self.drag {
                        *f = to;
                    }
                }
            }
            StepDrag::Zoom => {
                self.zoom_to(p.x);
                cx.notify();
                return;
            }
            StepDrag::Field { field, y, from } => {
                self.set_field(field, from + (y - p.y) / 4.0);
            }
            StepDrag::Notch { row, lane, y } => {
                let dy = y - p.y;
                if dy.abs() < 6.0 {
                    return;
                }
                let up = dy > 0.0;
                if let Some(pt) = self.pattern_now() {
                    let n = pt.steps();
                    for (k, r) in pt.rows.iter_mut().enumerate() {
                        if row.is_none_or(|w| w == k)
                            && lane.is_none_or(|l| l.applies(r.is_automation()))
                        {
                            r.step_all(n, lane, up);
                        }
                    }
                }
                if let Some(Drag::StepSeq(StepDrag::Notch { y: y0, .. })) = &mut self.drag {
                    *y0 = p.y;
                }
            }
        }
        if let Some(pt) = self.pattern_now() {
            for r in &mut pt.rows {
                r.tidy_lanes();
            }
        }
        self.changed(cx);
    }

    /// A step sequencer drag let go.
    pub(crate) fn steps_release(&mut self) {
        self.steps.tip = None;
    }
}
