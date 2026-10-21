//! The mouse in the roll: clicks, marquees, painting, moving notes, velocities and controller lanes.

use super::*;

/// Grabs note `i` of a region: picking it (shift adds or takes it away), ⌥ copying the picked
/// notes to drag the copies, and sounding it; or erases or cuts it, with those tools.
pub(crate) fn grab_note(
    at: At,
    i: usize,
    edge: Edge,
    cx: &mut Context<Workspace>,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
        cx.stop_propagation();
        let Some(n) = ws
            .session
            .project
            .region(at)
            .content
            .notes(0)
            .and_then(|n| n.get(i).copied())
        else {
            return;
        };
        match ws.tool {
            Tool::Eraser => {
                ws.edit(cx, |ws| {
                    if let Some(notes) = ws.session.project.notes_mut(at) {
                        notes.remove(i);
                    }
                    ws.picked.clear();
                });
                return;
            }
            Tool::Scissors => {
                let start = ws.session.project.region(at).start as i64;
                let cut = ws.snap(ws.tick_at(f32::from(e.position.x))) as i64 - start;
                if cut > n.start && cut < n.start + n.length as i64 {
                    ws.edit(cx, |ws| {
                        if let Some(notes) = ws.session.project.notes_mut(at) {
                            let first = (cut - n.start) as u64;
                            notes[i].length = first;
                            notes.push(Note {
                                start: cut,
                                length: n.length - first,
                                ..n
                            });
                        }
                    });
                }
                return;
            }
            _ => {}
        }
        if e.modifiers.shift {
            match ws.picked.iter().position(|&k| k == i) {
                Some(k) => {
                    ws.picked.remove(k);
                    cx.notify();
                    return;
                }
                None => ws.picked.push(i),
            }
        } else if !ws.picked.contains(&i) {
            ws.picked = vec![i];
        }
        ws.region = Some(at);
        // The length new notes are written at: this one's.
        ws.roll.length = n.length;
        let track = ws.session.project.tracks[at.0].id;
        ws.player.preview(track, n.pitch, n.velocity);
        let from = e.position.map(f32::from);
        ws.session.checkpoint();
        let mut lead = i;
        // ⌥: the picked notes copied, and the copies dragged.
        if e.modifiers.alt && edge == Edge::Move {
            let Some(notes) = ws.session.project.notes_mut(at) else {
                return;
            };
            let first = notes.len();
            let copies: Vec<Note> = ws
                .picked
                .iter()
                .filter_map(|&k| notes.get(k).copied())
                .collect();
            lead = first + ws.picked.iter().position(|&k| k == i).unwrap_or(0);
            notes.extend(copies);
            ws.picked = (first..notes.len()).collect();
        }
        let notes = ws
            .session
            .project
            .region(at)
            .content
            .notes(0)
            .map(|n| n.to_vec())
            .unwrap_or_default();
        ws.held = ws
            .picked
            .iter()
            .filter_map(|&k| notes.get(k).map(|n| (k, *n)))
            .collect();
        ws.drag = Some(Drag::Notes {
            at,
            lead,
            edge,
            from,
            moved: false,
        });
        cx.notify();
    })
}

impl Workspace {
    /// A click on the grid. With the pointer: a rubber band, or with a double-click a note. With
    /// the pencil: a note (or chord), and painting on as it's dragged.
    pub(crate) fn roll_click(&mut self, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let p = e.position.map(f32::from);
        let adds = e.click_count == 2 || self.tool == Tool::Pencil;
        if !adds {
            let base = if e.modifiers.shift {
                self.picked.clone()
            } else {
                Vec::new()
            };
            self.picked = base.clone();
            self.held_picks = base;
            self.begin_drag(Drag::Marquee { from: p, to: p }, cx);
            return;
        }
        let tick = self.snap_down(self.tick_at(p.x));
        let pitch = self.in_key(self.pitch_at(p.y), 0);
        let Some((track, target, span)) = self.region_for(tick, cx) else {
            return;
        };
        let length = self.new_length();
        let pitches = self
            .roll
            .stamp
            .pitches(pitch, self.session.project.song_key());
        let id = self.session.project.tracks[track].id;
        let mut made = None;
        self.edit(cx, |ws| {
            let at = match (target, span) {
                (Some(at), _) => at,
                (None, Some((start, len))) => {
                    let regions = &mut ws.session.project.tracks[track].regions;
                    regions.push(Region {
                        start,
                        length: len,
                        content: Content::Midi {
                            notes: Vec::new(),
                            controls: Vec::new(),
                        },
                        ..Default::default()
                    });
                    (track, regions.len() - 1)
                }
                (None, None) => return,
            };
            let start = ws.session.project.region(at).start;
            let Some(notes) = ws.session.project.notes_mut(at) else {
                return;
            };
            let first = notes.len();
            for &pitch in &pitches {
                notes.push(Note {
                    start: (tick - start) as i64,
                    length,
                    pitch,
                    velocity: 100,
                });
            }
            ws.picked = (first..notes.len()).collect();
            ws.region = Some(at);
            ws.grow_to_notes(at);
            made = Some((at, (tick - start) as i64));
        });
        let Some((at, start)) = made else { return };
        for &pitch in &pitches {
            self.player.preview(id, pitch, 100);
        }
        self.painting = pitches;
        self.drag = Some(Drag::Paint { at, last: start });
        cx.notify();
    }

    /// The rubber band moved: the notes it touches picked (with those picked before, with shift).
    pub(crate) fn drag_marquee(&mut self, to: Point<f32>) {
        let Some(Drag::Marquee { from, .. }) = self.drag else {
            return;
        };
        let base = self.held_picks.clone();
        if let Some(Drag::Marquee { to: t, .. }) = &mut self.drag {
            *t = to;
        }
        let Some(at) = self.roll_region() else { return };
        let r = self.session.project.region(at).clone();
        let (t0, t1) = (
            self.tick_at(from.x.min(to.x)),
            self.tick_at(from.x.max(to.x)),
        );
        let (hi, lo) = (
            self.pitch_at(from.y.min(to.y)),
            self.pitch_at(from.y.max(to.y)),
        );
        let Content::Midi { notes, .. } = &r.content else {
            return;
        };
        let mut picked = base;
        for (i, n) in notes.iter().enumerate() {
            let s = r.start as f64 + n.start as f64;
            let touches = s < t1 && s + n.length as f64 > t0 && (lo..=hi).contains(&n.pitch);
            if touches && !picked.contains(&i) {
                picked.push(i);
            }
        }
        self.picked = picked;
    }

    /// The pencil dragged on: a note (or chord) at each note length it passes, at the pitch under
    /// it, painting.
    pub(crate) fn drag_paint(&mut self, p: Point<f32>) {
        let Some(Drag::Paint { at, last }) = self.drag else {
            return;
        };
        let sounding = self.painting.clone();
        if self.tool != Tool::Pencil {
            return;
        }
        let length = self.new_length().max(1) as i64;
        let start = self.session.project.region(at).start as i64;
        let tick = self.tick_at(p.x) as i64 - start;
        let steps = (tick - last).div_euclid(length);
        if steps < 1 {
            return;
        }
        let pitch = self.in_key(self.pitch_at(p.y), 0);
        let pitches = self
            .roll
            .stamp
            .pitches(pitch, self.session.project.song_key());
        let id = self.session.project.tracks[at.0].id;
        let Some(notes) = self.session.project.notes_mut(at) else {
            return;
        };
        for k in 1..=steps {
            let first = notes.len();
            for &pitch in &pitches {
                notes.push(Note {
                    start: last + k * length,
                    length: length as u64,
                    pitch,
                    velocity: 100,
                });
            }
            self.picked.extend(first..notes.len());
        }
        self.grow_to_notes(at);
        if sounding != pitches {
            for &p in &sounding {
                self.player.preview(id, p, 0);
            }
            for &p in &pitches {
                self.player.preview(id, p, 100);
            }
        }
        self.painting = pitches;
        self.drag = Some(Drag::Paint {
            at,
            last: last + steps * length,
        });
    }

    /// The picked notes dragged: moved (the grabbed one snapping, the rest keeping their
    /// distance), or stretched by either edge.
    pub(crate) fn drag_notes(&mut self, p: Point<f32>) {
        let Some(Drag::Notes {
            at,
            lead,
            edge,
            from,
            ..
        }) = self.drag
        else {
            return;
        };
        let was = self.held.clone();
        let Some(&(_, first)) = was.iter().find(|(i, _)| *i == lead) else {
            return;
        };
        let d = p - from;
        let dt = (d.x / self.px_per_tick()) as f64;
        let region_start = self.session.project.region(at).start as i64;
        let grid = self.grid() as i64;
        let snapped = |t: f64| self.snap(t.max(0.0)) as i64;
        let track = self.session.project.tracks[at.0].id;
        match edge {
            Edge::Move => {
                let earliest = was.iter().map(|(_, n)| n.start).min().unwrap_or(0);
                let shift = (snapped((region_start + first.start) as f64 + dt)
                    - region_start
                    - first.start)
                    .max(-earliest - region_start);
                let rows = (d.y / self.roll.key_h).round() as i32;
                let lo = was.iter().map(|(_, n)| n.pitch as i32).min().unwrap_or(0);
                let hi = was.iter().map(|(_, n)| n.pitch as i32).max().unwrap_or(127);
                let by = (-rows).clamp(-lo, 127 - hi);
                let keyed: Vec<(usize, u8)> = was
                    .iter()
                    .map(|&(i, n)| (i, self.in_key((n.pitch as i32 + by) as u8, by)))
                    .collect();
                let Some(notes) = self.session.project.notes_mut(at) else {
                    return;
                };
                let before = notes.get(lead).map(|n| n.pitch);
                for (&(i, n), &(_, pitch)) in was.iter().zip(&keyed) {
                    if let Some(note) = notes.get_mut(i) {
                        note.start = n.start + shift;
                        note.pitch = pitch;
                    }
                }
                let after = notes.get(lead).map(|n| n.pitch);
                if let (Some(old), Some(new)) = (before, after)
                    && old != new
                {
                    self.player.preview(track, old, 0);
                    self.player.preview(track, new, 100);
                }
            }
            Edge::End | Edge::Stretch | Edge::Loop => {
                let end = first.start + first.length as i64;
                let by = snapped((region_start + end) as f64 + dt) - region_start - end;
                let Some(notes) = self.session.project.notes_mut(at) else {
                    return;
                };
                for &(i, n) in &was {
                    if let Some(note) = notes.get_mut(i) {
                        let least = grid.min(n.length as i64).max(1);
                        note.length = (n.length as i64 + by).max(least) as u64;
                    }
                }
                if let Some(n) = notes.get(lead) {
                    self.roll.length = n.length;
                }
            }
            Edge::Start => {
                let by =
                    snapped((region_start + first.start) as f64 + dt) - region_start - first.start;
                // No note shorter than a grid step (or than it was), nor before the region.
                let most = was
                    .iter()
                    .map(|(_, n)| n.length as i64 - grid.min(n.length as i64).max(1))
                    .min()
                    .unwrap_or(0);
                let least = -was
                    .iter()
                    .map(|(_, n)| n.start + region_start)
                    .min()
                    .unwrap_or(0);
                let by = by.clamp(least, most);
                let Some(notes) = self.session.project.notes_mut(at) else {
                    return;
                };
                for &(i, n) in &was {
                    if let Some(note) = notes.get_mut(i) {
                        note.start = n.start + by;
                        note.length = (n.length as i64 - by) as u64;
                    }
                }
                if let Some(n) = notes.get(lead) {
                    self.roll.length = n.length;
                }
            }
        }
    }

    /// Velocities drawn across the lane: each note starting between where the mouse was and is
    /// takes the height it's at (the picked notes only, if some are).
    pub(crate) fn drag_velocity_line(&mut self, p: Point<f32>) {
        let Some(Drag::VelocityLine { at, x }) = self.drag else {
            return;
        };
        let lane = self.lane_bounds.get();
        let bottom = f32::from(lane.origin.y) + f32::from(lane.size.height) - 5.0;
        let v = (((bottom - p.y) / VELOCITY_H) * 127.0)
            .round()
            .clamp(1.0, 127.0) as u8;
        let (t0, t1) = (self.tick_at(x.min(p.x)), self.tick_at(x.max(p.x)) + 1.0);
        let start = self.session.project.region(at).start as f64;
        let only = self.picked.clone();
        if let Some(notes) = self.session.project.notes_mut(at) {
            for (i, n) in notes.iter_mut().enumerate() {
                let s = start + n.start as f64;
                if (t0 - 2.0..=t1).contains(&s) && (only.is_empty() || only.contains(&i)) {
                    n.velocity = v;
                }
            }
        }
        self.drag = Some(Drag::VelocityLine { at, x: p.x });
    }

    /// A controller's moves drawn freehand: one a grid step (finer than the notes'), replacing
    /// what was there; or with ⌥, rubbed out.
    pub(crate) fn drag_control(&mut self, p: Point<f32>) {
        let Some(Drag::Control { at, last, erase }) = self.drag else {
            return;
        };
        let lane = self.roll.lane;
        let b = self.lane_bounds.get();
        let bottom = f32::from(b.origin.y) + f32::from(b.size.height) - 5.0;
        let level = (bottom - p.y) / VELOCITY_H;
        let start = self.session.project.region(at).start as i64;
        let step = (self.grid() / 2).max(PPQ / 16) as i64;
        let tick = ((self.tick_at(p.x) as i64 - start) / step * step).max(0);
        let (from, to) = match last {
            Some(l) => (l.min(tick), l.max(tick)),
            None => (tick, tick),
        };
        let r = &mut self.session.project.tracks[at.0].regions[at.1];
        let Content::Midi { controls, .. } = &mut r.content else {
            return;
        };
        controls.retain(|c| !(lane.holds(&c.kind) && c.start >= from && c.start <= to));
        if !erase {
            let mut t = from;
            while t <= to {
                controls.push(ControlPoint {
                    start: t,
                    kind: lane.make(level),
                });
                t += step;
            }
            controls.sort_by_key(|c| c.start);
        }
        self.drag = Some(Drag::Control {
            at,
            last: Some(tick),
            erase,
        });
    }

    /// A press in the lane under the notes: drawing velocities or a controller's moves.
    pub(crate) fn lane_down(&mut self, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(at) = self.roll_region() else { return };
        let p = e.position.map(f32::from);
        match self.roll.lane {
            Lane::Velocity => {
                self.begin_drag(Drag::VelocityLine { at, x: p.x }, cx);
                self.drag_velocity_line(p);
            }
            _ => {
                self.begin_drag(
                    Drag::Control {
                        at,
                        last: None,
                        erase: e.modifiers.alt,
                    },
                    cx,
                );
                self.drag_control(p);
            }
        }
        self.changed(cx);
    }
}
