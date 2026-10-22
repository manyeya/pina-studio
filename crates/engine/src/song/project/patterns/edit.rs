//! What can be done to a pattern's steps and rows: rotated, nudged up or down, randomized,
//! cleared, ramped, tied, copied and pasted; the pattern made longer or shorter, transposed, kept
//! to its scale, its rows sorted.

use super::*;

/// Every setting one step has, to copy from one step to another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepCopy {
    pub on: bool,
    pub velocity: u8,
    pub value: f32,
    pub chance: u8,
    pub note: i8,
    pub octave: i8,
    pub repeat: u8,
    pub gate: u8,
    pub tie: bool,
    pub offset: i8,
    /// Its own rate, if it was set for it.
    pub rate: Option<u64>,
    pub skip: bool,
    pub condition: Condition,
    pub every: [u8; 2],
    pub ramp: i8,
    pub curve: i8,
    pub spread: i8,
}

impl PatternRow {
    pub fn copy_step(&self, i: usize) -> StepCopy {
        let (nth, of) = self.every_at(i);
        StepCopy {
            on: self.on(i),
            velocity: self.velocity_at(i),
            value: self.value_at(i),
            chance: self.chance_at(i),
            note: self.transpose_at(i),
            octave: self.octave_at(i),
            repeat: self.repeat_at(i),
            gate: self.gate_at(i),
            tie: self.tie_at(i),
            offset: self.nudge_at(i),
            rate: self.rate_at(i),
            skip: self.skip_at(i),
            condition: self.condition_at(i),
            every: [nth, of],
            ramp: self.ramp_at(i),
            curve: self.curve_at(i),
            spread: self.spread_at(i),
        }
    }

    /// Puts a copied step's settings on step `i`; a step whose rate wasn't its own keeps this
    /// one's.
    pub fn paste_step(&mut self, i: usize, s: &StepCopy) {
        if i >= self.cells.len() {
            return;
        }
        self.fill_lanes();
        self.cells[i] = if s.on { s.velocity.max(1) } else { 0 };
        self.held[i] = if s.on { 0 } else { s.velocity };
        if self.is_automation() {
            self.value[i] = s.value.clamp(0.0, 1.0);
        }
        self.chance[i] = s.chance;
        self.transpose[i] = s.note;
        self.octave[i] = s.octave;
        self.repeat[i] = s.repeat;
        self.gate[i] = s.gate;
        self.tie[i] = s.tie;
        self.nudge[i] = s.offset;
        if let Some(r) = s.rate {
            self.rates[i] = r;
        }
        self.skip[i] = s.skip;
        self.condition[i] = s.condition;
        self.every[i] = s.every;
        self.ramp[i] = s.ramp;
        self.curve[i] = s.curve;
        self.spread[i] = s.spread;
        self.tidy_lanes();
    }

    /// A step back as it was made: off, every setting as usual (an automation step keeps its
    /// value, a step its velocity).
    pub fn clear_step(&mut self, i: usize) {
        if i >= self.cells.len() {
            return;
        }
        let keep = (self.velocity_at(i), self.value_at(i));
        let mut s = PatternRow::note(self.pitch, 1).copy_step(0);
        s.velocity = keep.0;
        s.value = keep.1;
        self.paste_step(i, &s);
    }

    /// Every step back as it was made: off, every setting as usual.
    pub fn clear(&mut self) {
        for i in 0..self.cells.len() {
            self.clear_step(i);
        }
        self.held.clear();
        self.tidy_lanes();
    }

    /// One lane's settings back as usual throughout; on/off (no lane) turns every step off.
    pub fn clear_lane(&mut self, lane: Option<Lane>) {
        let n = self.cells.len();
        match lane {
            None => (0..n).for_each(|i| self.set_on(i, false)),
            Some(l) => (0..n).for_each(|i| {
                let v = self.default_of(l, i);
                self.set(l, i, v);
            }),
        }
        self.tidy_lanes();
    }

    /// Whether a lane (or on/off) is as usual throughout: nothing to clear.
    pub fn lane_is_clear(&self, lane: Option<Lane>) -> bool {
        let n = self.cells.len();
        match lane {
            None => (0..n).all(|i| !self.on(i)),
            Some(l) => (0..n).all(|i| self.get(l, i) == self.default_of(l, i)),
        }
    }

    /// The steps of its loop moved `by` later (earlier when negative), round to its start: with
    /// every setting (no lane), or one lane's settings alone, the rhythm left where it is.
    pub fn rotate(&mut self, steps: usize, by: i32, lane: Option<Lane>) {
        let (from, to) = self.loop_range(steps);
        let n = to - from;
        if n < 2 {
            return;
        }
        let by = by.rem_euclid(n as i32) as usize;
        match lane {
            Some(l) => {
                let mut vals: Vec<f32> = (from..to).map(|i| self.get(l, i)).collect();
                vals.rotate_right(by);
                for (k, v) in vals.into_iter().enumerate() {
                    self.set(l, from + k, v);
                }
            }
            None => {
                self.fill_lanes();
                fn turn<T>(v: &mut [T], from: usize, to: usize, by: usize) {
                    if to <= v.len() {
                        v[from..to].rotate_right(by);
                    }
                }
                turn(&mut self.cells, from, to, by);
                turn(&mut self.held, from, to, by);
                turn(&mut self.chance, from, to, by);
                turn(&mut self.transpose, from, to, by);
                turn(&mut self.octave, from, to, by);
                turn(&mut self.repeat, from, to, by);
                turn(&mut self.gate, from, to, by);
                turn(&mut self.tie, from, to, by);
                turn(&mut self.nudge, from, to, by);
                turn(&mut self.rates, from, to, by);
                turn(&mut self.skip, from, to, by);
                turn(&mut self.value, from, to, by);
                turn(&mut self.condition, from, to, by);
                turn(&mut self.every, from, to, by);
                turn(&mut self.ramp, from, to, by);
                turn(&mut self.curve, from, to, by);
                turn(&mut self.spread, from, to, by);
            }
        }
        self.tidy_lanes();
    }

    /// Every step of its loop a notch up (or down) in a lane: on/off turns them all on (or
    /// off); tie ties each to the next but the last (or unties them); skip skips them all (or
    /// none).
    pub fn step_all(&mut self, steps: usize, lane: Option<Lane>, up: bool) {
        let (from, to) = self.loop_range(steps);
        match lane {
            None => (from..to).for_each(|i| self.set_on(i, up)),
            Some(Lane::Tie) => (from..to).for_each(|i| {
                self.set(Lane::Tie, i, (up && i + 1 < to) as u8 as f32);
            }),
            Some(Lane::Skip) => (from..to).for_each(|i| self.set(Lane::Skip, i, up as u8 as f32)),
            Some(l) => {
                let by = match (l, self.is_automation()) {
                    (Lane::Velocity, true) => 1.0 / 127.0,
                    _ => 1.0,
                };
                for i in from..to {
                    let v = self.get(l, i) + if up { by } else { -by };
                    self.set(l, i, v);
                }
            }
        }
        self.tidy_lanes();
    }

    /// A lane's settings from step `a` to step `b` in an even line between theirs.
    pub fn ramp(&mut self, lane: Lane, a: usize, b: usize) {
        let (a, b) = (a.min(b), a.max(b));
        if b >= self.cells.len() || b == a {
            return;
        }
        let (va, vb) = (self.get(lane, a), self.get(lane, b));
        for i in a + 1..b {
            let t = (i - a) as f32 / (b - a) as f32;
            self.set(lane, i, va + (vb - va) * t);
        }
        self.tidy_lanes();
    }

    /// Steps `a` to `b` on and tied together, a note held through them all.
    pub fn tie_steps(&mut self, a: usize, b: usize) {
        let (a, b) = (a.min(b), a.max(b).min(self.cells.len().saturating_sub(1)));
        for i in a..=b {
            self.set_on(i, true);
            self.set(Lane::Tie, i, (i < b) as u8 as f32);
        }
        self.tidy_lanes();
    }

    /// A step's tie, and the one tied to it, undone.
    pub fn untie(&mut self, i: usize) {
        self.set(Lane::Tie, i, 0.0);
        if i > 0 {
            self.set(Lane::Tie, i - 1, 0.0);
        }
        self.tidy_lanes();
    }
}

/// How a randomize goes: the chance a step comes out on (on/off, tie, skip); and for values,
/// either `spread` % either side of each step's, or anywhere from `low` to `high`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Randomize {
    pub probability: u8,
    pub within: bool,
    pub spread: u8,
    pub low: f32,
    pub high: f32,
}

impl Default for Randomize {
    fn default() -> Self {
        Randomize {
            probability: 50,
            within: false,
            spread: 50,
            low: f32::MIN,
            high: f32::MAX,
        }
    }
}

impl PatternRow {
    /// A lane (or on/off) of its loop's steps at random, as `how` says.
    pub fn randomize(&mut self, steps: usize, lane: Option<Lane>, how: &Randomize, seed: u64) {
        let (from, to) = self.loop_range(steps);
        let unit = |i: usize, salt: u64| {
            let mut x = seed ^ (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ salt;
            x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            ((x ^ (x >> 31)) >> 11) as f32 / (1u64 << 53) as f32
        };
        let coin = |i: usize| unit(i, 1) * 100.0 < how.probability as f32;
        for i in from..to {
            match lane {
                None => self.set_on(i, coin(i)),
                Some(l) if l.switch() => self.set(l, i, coin(i) as u8 as f32),
                Some(l) => {
                    let (lo, hi) = self.range(l);
                    let (lo, hi) = match how.within {
                        true => (how.low.max(lo), how.high.min(hi)),
                        false => {
                            let v = self.get(l, i);
                            let by = (hi - lo).max(v.abs()) * how.spread as f32 / 100.0 / 2.0;
                            ((v - by).max(lo), (v + by).min(hi))
                        }
                    };
                    let v = lo + (hi - lo).max(0.0) * unit(i, 2);
                    let v = if self.is_automation() && l == Lane::Velocity {
                        v
                    } else {
                        v.round()
                    };
                    self.set(l, i, v);
                }
            }
        }
        self.tidy_lanes();
    }
}

/// How rows can be put in order: by their kind (notes first, then automation), then their note
/// or their name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortRows {
    Type,
    TypeAndNote,
    TypeAndName,
}

impl Pattern {
    /// Its length in steps: every row cut short, or made longer by going round its steps again
    /// (so a bar's groove fills two).
    pub fn set_length(&mut self, steps: usize) {
        let steps = steps.clamp(1, MAX_STEPS.max(self.steps()));
        let had = self.steps();
        for row in &mut self.rows {
            row.fill_lanes();
            let len = had.min(row.cells.len()).max(1);
            fn again<T: Copy>(v: &mut Vec<T>, len: usize, steps: usize) {
                if v.is_empty() {
                    return;
                }
                let old: Vec<T> = v[..len.min(v.len())].to_vec();
                *v = (0..steps).map(|k| old[k % old.len()]).collect();
            }
            again(&mut row.cells, len, steps);
            again(&mut row.held, len, steps);
            again(&mut row.chance, len, steps);
            again(&mut row.transpose, len, steps);
            again(&mut row.octave, len, steps);
            again(&mut row.repeat, len, steps);
            again(&mut row.gate, len, steps);
            again(&mut row.tie, len, steps);
            again(&mut row.nudge, len, steps);
            again(&mut row.rates, len, steps);
            again(&mut row.skip, len, steps);
            again(&mut row.value, len, steps);
            again(&mut row.condition, len, steps);
            again(&mut row.every, len, steps);
            again(&mut row.ramp, len, steps);
            again(&mut row.curve, len, steps);
            again(&mut row.spread, len, steps);
            row.loop_end = row.loop_end.filter(|&e| e < steps);
            row.loop_start = row.loop_start.min(steps - 1);
        }
        self.length = steps;
        self.tidy();
    }

    /// Every note row moved by `semitones` (automation rows stay as they are).
    pub fn transpose(&mut self, semitones: i32) {
        for row in self.rows.iter_mut().filter(|r| !r.is_automation()) {
            row.pitch = (row.pitch as i32 + semitones).clamp(0, 127) as u8;
        }
    }

    /// Its key changed: its notes moved the nearer way to the new one (unless `moves` is off),
    /// and kept to its scale.
    pub fn set_key(&mut self, key: u8, moves: bool) {
        let key = key % 12;
        if moves {
            let up = (key as i32 - self.key as i32).rem_euclid(12);
            self.transpose(if up > 6 { up - 12 } else { up });
        }
        self.key = key;
        if moves && self.scale.is_some() {
            self.conform();
        }
    }

    /// Whether any note it plays is outside its scale.
    pub fn out_of_scale(&self) -> bool {
        let Some(scale) = self.scale else {
            return false;
        };
        self.rows.iter().filter(|r| !r.is_automation()).any(|r| {
            (0..r.cells.len()).any(|i| r.on(i) && !scale.contains(self.key, r.pitch_at(i)))
        })
    }

    /// Every note it plays moved onto its scale: a row's own note, or a step's.
    pub fn conform(&mut self) {
        let Some(scale) = self.scale else {
            return;
        };
        let key = self.key;
        for row in self.rows.iter_mut().filter(|r| !r.is_automation()) {
            if !row.is_melodic() {
                row.pitch = scale.snap(key, row.pitch);
                continue;
            }
            for i in 0..row.cells.len() {
                let p = row.pitch_at(i);
                let to = scale.snap(key, p) as i32;
                if to != p as i32 {
                    let note = row.transpose_at(i) as i32 + to - p as i32;
                    row.set(Lane::Note, i, note.clamp(-12, 12) as f32);
                }
            }
            row.tidy_lanes();
        }
    }

    /// Every step of every row (of note rows, or automation rows, or all) at random in a lane.
    pub fn randomize(
        &mut self,
        lane: Option<Lane>,
        how: &Randomize,
        which: Option<bool>,
        seed: u64,
    ) {
        let steps = self.steps();
        for (r, row) in self.rows.iter_mut().enumerate() {
            if which.is_some_and(|auto| auto != row.is_automation())
                || lane.is_some_and(|l| !l.applies(row.is_automation()))
            {
                continue;
            }
            row.randomize(
                steps,
                lane,
                how,
                seed ^ (r as u64).wrapping_mul(0x2545_f491),
            );
        }
    }

    /// Rows that never play a step, gone.
    pub fn remove_unused(&mut self) {
        self.rows.retain(|r| r.cells.iter().any(|&c| c > 0));
    }

    /// Its rows in order: notes, then melodic rows, then automation; within those by note or name.
    pub fn sort(&mut self, by: SortRows, descending: bool, names: impl Fn(&PatternRow) -> String) {
        let kind = |r: &PatternRow| match (r.target, r.is_melodic()) {
            (None, false) => 0,
            (None, true) => 1,
            (Some(RowTarget::Cc(_)), _) => 2,
            (Some(RowTarget::PitchBend), _) => 3,
            (Some(RowTarget::Pressure), _) => 4,
            (Some(RowTarget::Plugin { .. }), _) => 6,
            (Some(_), _) => 5,
        };
        let mut keyed: Vec<(u8, u8, String, PatternRow)> = self
            .rows
            .drain(..)
            .map(|r| (kind(&r), r.pitch, names(&r).to_lowercase(), r))
            .collect();
        keyed.sort_by(|a, b| {
            let first = a.0.cmp(&b.0);
            let then = match by {
                SortRows::Type => std::cmp::Ordering::Equal,
                SortRows::TypeAndNote => a.1.cmp(&b.1),
                SortRows::TypeAndName => a.2.cmp(&b.2),
            };
            let o = first.then(then);
            if descending { o.reverse() } else { o }
        });
        self.rows = keyed.into_iter().map(|k| k.3).collect();
    }

    /// Turns every other step in a column off (mono: one sound a step), but row `keep`'s.
    pub fn mono(&mut self, step: usize, keep: usize) {
        for (r, row) in self.rows.iter_mut().enumerate() {
            if r != keep && !row.is_automation() {
                row.set_on(step, false);
            }
        }
    }
}

/// A row's steps put on another (its sound and settings kept): between two rows of one note
/// each, the rhythm alone; from a melodic row, its notes too.
pub fn paste_row(onto: &mut PatternRow, from: &PatternRow) {
    if onto.is_automation() != from.is_automation() {
        return;
    }
    let keep = onto.clone();
    *onto = from.clone();
    (onto.pitch, onto.name, onto.target, onto.color) =
        (keep.pitch, keep.name, keep.target, keep.color);
    (onto.mute, onto.solo, onto.open, onto.subrows) =
        (keep.mute, keep.solo, keep.open, keep.subrows);
    (
        onto.loop_start,
        onto.loop_end,
        onto.rate,
        onto.mode,
        onto.slide,
    ) = (
        keep.loop_start,
        keep.loop_end,
        keep.rate,
        keep.mode,
        keep.slide,
    );
    if !from.is_melodic() {
        onto.transpose = keep.transpose;
        onto.octave = keep.octave;
    }
    onto.melodic = keep.melodic || from.is_melodic();
    onto.cells.resize(keep.cells.len(), 0);
    onto.tidy_lanes();
}

/// A row's settings (its loop, rate, playback mode, sliding) put on another, its steps kept.
pub fn paste_row_settings(onto: &mut PatternRow, from: &PatternRow) {
    onto.loop_start = from.loop_start;
    onto.loop_end = from.loop_end;
    onto.rate = from.rate;
    onto.mode = from.mode;
    onto.slide = from.slide;
}
