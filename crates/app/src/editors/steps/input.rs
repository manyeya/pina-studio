//! Notes played into the step sequencer (from a MIDI keyboard, or typed): learned as rows, put
//! in step by step (MIDI in), or recorded live as the pattern plays; and controls moved, learned
//! as automation rows.

use pina_engine::song::project::{Param, RowTarget, Which, row_hit_at};

use super::*;

/// How much a plugin parameter must move to count as turned.
const TURNED: f64 = 0.02;

/// A note held while recording live: its row and step, and where it began (ticks into the pass).
#[derive(Clone, Copy, Debug)]
pub struct Holding {
    pub pitch: u8,
    pub row: usize,
    pub step: usize,
    pub from: u64,
}

impl Workspace {
    /// The row a note goes in: the one for it (a melodic row picked takes any note), made if
    /// there's none. Learn (Assign) gives the picked row the note instead.
    fn row_for(&mut self, pitch: u8) -> Option<usize> {
        let (_, p) = self.shown_pattern()?;
        let picked = self.steps.row;
        if p.rows
            .get(picked)
            .is_some_and(|r| r.is_melodic() && !r.is_automation())
        {
            return Some(picked);
        }
        if let Some(r) = p
            .rows
            .iter()
            .position(|r| !r.is_automation() && !r.is_melodic() && r.pitch == pitch)
        {
            return Some(r);
        }
        let n = p.steps();
        let at = p.rows.len();
        let learn = self.steps.learn;
        let p = self.pattern_now()?;
        match learn {
            Some(Learn::Assign) if picked < p.rows.len() => {
                let row = &mut p.rows[picked];
                (row.pitch, row.target, row.melodic) = (pitch, None, false);
                Some(picked)
            }
            _ => {
                p.rows.push(PatternRow::note(pitch, n));
                Some(at)
            }
        }
    }

    /// A note put on a step: on, at its velocity, a melodic row's note moved to it.
    fn put_note(&mut self, r: usize, i: usize, pitch: u8, velocity: Option<u8>) {
        let mono = self.steps.mono;
        let Some(p) = self.pattern_now() else {
            return;
        };
        let Some(row) = p.rows.get_mut(r) else {
            return;
        };
        row.set_on(i, true);
        if let Some(v) = velocity {
            row.set(Lane::Velocity, i, v as f32);
        }
        if row.is_melodic() {
            let by = pitch as i32 - row.pitch as i32;
            let (oct, note) = (by.div_euclid(12), by.rem_euclid(12));
            let (oct, note) = if note > 6 {
                (oct + 1, note - 12)
            } else {
                (oct, note)
            };
            row.fill_lanes();
            row.transpose[i] = note as i8;
            row.octave[i] = oct.clamp(-10, 10) as i8;
        }
        row.tidy_lanes();
        if mono {
            p.mono(i, r);
        }
    }

    /// Notes played since the last frame, as (sample, pitch, velocity; 0 off), into the step
    /// sequencer if it's taking them.
    pub(crate) fn steps_input(&mut self, played: &[(u64, u8, u8)], cx: &mut Context<Self>) {
        if let Some((t, param)) = self.steps.learned.take() {
            self.learned_param(t, param, cx);
        }
        self.learn_controls(cx);
        let taking = self.steps.learn.is_some() || self.steps.midi_in || self.steps.live;
        if played.is_empty() || !taking || self.bottom != Some(Bottom::Steps) {
            return;
        }
        let Some((at, _)) = self.shown_pattern() else {
            return;
        };
        let mut changed = false;
        for &(sample, pitch, velocity) in played {
            let on = velocity > 0;
            // Live: as the pattern plays, into the step nearest where it's played.
            if self.steps.live && self.player.playing() {
                let tick = self.session.project.ticks(sample);
                let r = self.session.project.region(at);
                if !(r.start..r.start + r.length.max(1) * 64).contains(&tick) {
                    continue;
                }
                let len = r.length.max(1);
                let pass = r.start + (tick - r.start) / len * len;
                let into = (tick - r.start) % len;
                if on {
                    if changed || self.drag.is_none() {
                        self.session.checkpoint();
                    }
                    let Some(row) = self.row_for(pitch) else {
                        continue;
                    };
                    let Some((_, p)) = self.shown_pattern() else {
                        continue;
                    };
                    let Some(hit) = row_hit_at(p, row, into, pass) else {
                        continue;
                    };
                    let late = into - hit.at;
                    // Quantized: the nearer step; else this one, its offset where it was played.
                    let (step, offset) = match self.steps.live_quantize {
                        true if late * 2 > hit.length => (
                            row_hit_at(p, row, hit.at + hit.length, pass)
                                .map_or(hit.step, |h| h.step),
                            0.0,
                        ),
                        true => (hit.step, 0.0),
                        false => {
                            let o = late as f32 / hit.length as f32 * 100.0;
                            if o > 50.0 {
                                let next = row_hit_at(p, row, hit.at + hit.length, pass)
                                    .map_or(hit.step, |h| h.step);
                                (next, o - 100.0)
                            } else {
                                (hit.step, o)
                            }
                        }
                    };
                    let v = self.steps.live_velocity.then_some(velocity);
                    self.put_note(row, step, pitch, v);
                    if let Some(rw) = self.pattern_now().and_then(|p| p.rows.get_mut(row)) {
                        rw.set(Lane::Offset, step, offset);
                        rw.set(Lane::Tie, step, 0.0);
                        rw.tidy_lanes();
                    }
                    self.steps.holding.retain(|h| h.pitch != pitch);
                    self.steps.holding.push(Holding {
                        pitch,
                        row,
                        step,
                        from: into,
                    });
                    changed = true;
                } else if let Some(k) = self.steps.holding.iter().position(|h| h.pitch == pitch) {
                    // Let go: how long it was held, as ties on and the last step's gate.
                    let h = self.steps.holding.remove(k);
                    if self.steps.live_length
                        && let Some(p) = self.pattern_now()
                    {
                        let rate = p.row_rate(h.row);
                        let held = into.saturating_sub(h.from).max(1) as f64 / rate as f64;
                        let n = p.steps();
                        if let Some(rw) = p.rows.get_mut(h.row) {
                            let whole = (held.ceil() as usize).max(1);
                            let last = (h.step + whole - 1).min(n - 1);
                            for t in h.step..last {
                                rw.set(Lane::Tie, t, 1.0);
                            }
                            let rest = held - (last - h.step) as f64;
                            rw.set(Lane::Gate, last, (rest * 100.0).clamp(1.0, 100.0) as f32);
                            rw.tidy_lanes();
                        }
                        changed = true;
                    }
                }
                continue;
            }
            if !on {
                // Step input moves on when the last key held comes up.
                if self.steps.midi_in && self.steps.held > 0 {
                    self.steps.held -= 1;
                    if self.steps.held == 0
                        && let Some((r, i)) = self.steps.cursor
                        && let Some((_, p)) = self.shown_pattern()
                    {
                        self.steps.cursor = Some((r, (i + 1) % p.steps()));
                    }
                }
                continue;
            }
            self.session.checkpoint();
            let learning = self.steps.learn;
            let Some(row) = self.row_for(pitch) else {
                continue;
            };
            changed = true;
            if learning.is_some() && !self.steps.midi_in {
                self.steps.row = row;
                if learning == Some(Learn::Assign) {
                    self.steps.learn = None;
                }
                continue;
            }
            if self.steps.midi_in {
                let i = self.steps.cursor.map_or(0, |c| c.1);
                self.put_note(row, i, pitch, Some(velocity));
                self.steps.held += 1;
                self.steps.cursor = Some((row, i));
                self.steps.row = row;
            }
        }
        if changed {
            if let Some(p) = self.pattern_now() {
                p.tidy();
            }
            self.changed(cx);
        }
    }

    /// Learn: a control moved on the pattern's track (a fader, a knob, a plugin's parameter)
    /// becomes an automation row, or the picked row's parameter.
    pub(crate) fn learned_param(&mut self, track: usize, param: Param, cx: &mut Context<Self>) {
        let Some(learn) = self.steps.learn else {
            return;
        };
        if self.pattern().is_none_or(|at| at.0 != track) {
            return;
        }
        let target = RowTarget::from_param(param);
        let has = self
            .shown_pattern()
            .is_some_and(|(_, p)| p.rows.iter().any(|r| r.target == Some(target)));
        if has {
            return;
        }
        match learn {
            Learn::Add => self.steps_op(StepsOp::AddAutomation(target), cx),
            Learn::Assign => {
                let r = self.steps.row;
                self.steps_op(StepsOp::AssignAutomation(r, target), cx);
                self.steps.learn = None;
            }
        }
    }

    /// While learning: the pattern's track's plugin parameters, as they were when learning began,
    /// checked for one turned in its window.
    fn learn_controls(&mut self, cx: &mut Context<Self>) {
        if self.steps.learn.is_none() {
            self.steps.learn_base.clear();
            return;
        }
        let Some(at) = self.pattern() else {
            return;
        };
        let id = self.session.project.tracks[at.0].id;
        if self.steps.learn_base.is_empty() {
            let slots = self.plugin_slots(at.0);
            self.steps.learn_base = slots
                .iter()
                .flat_map(|&which| {
                    self.player
                        .params(id, which)
                        .into_iter()
                        .map(move |p| (which, p.id, p.value))
                })
                .collect();
            return;
        }
        let turned: Option<(Which, u32)> = self
            .steps
            .learn_base
            .iter()
            .find(|&&(w, p, was)| (self.player.param_value(id, w, p) - was).abs() > TURNED)
            .map(|&(w, p, _)| (w, p));
        if let Some((w, p)) = turned {
            self.steps.learn_base.clear();
            self.learned_param(at.0, Param::Plugin(w, p), cx);
        }
    }
}
