//! What the roll's tools and menus do to notes.

use super::*;

impl Workspace {
    pub fn delete_notes(&mut self, at: At, cx: &mut Context<Self>) {
        let mut gone = self.picked.clone();
        gone.sort_unstable_by(|a, b| b.cmp(a));
        gone.dedup();
        self.edit(cx, |ws| {
            if let Some(notes) = ws.session.project.notes_mut(at) {
                for i in gone {
                    if i < notes.len() {
                        notes.remove(i);
                    }
                }
            }
            ws.picked.clear();
        });
    }

    /// Does something to the shown region's notes: the picked ones, or for a tool with none
    /// picked, all of them.
    pub fn roll_op(&mut self, op: RollOp, cx: &mut Context<Self>) {
        let Some(at) = self.roll_region() else { return };
        let count = self
            .session
            .project
            .region(at)
            .content
            .notes(0)
            .map_or(0, |n| n.len());
        let picked = self.picked.clone();
        let notes_of = |ws: &Self| -> Vec<Note> {
            match &ws.session.project.region(at).content {
                Content::Midi { notes, .. } => notes.clone(),
                _ => Vec::new(),
            }
        };
        match op {
            RollOp::SelectAll => {
                self.region = Some(at);
                self.picked = (0..count).collect();
            }
            RollOp::SelectPitch(pitch) => {
                self.region = Some(at);
                self.picked = notes_of(self)
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.pitch == pitch)
                    .map(|(i, _)| i)
                    .collect();
            }
            RollOp::Deselect => self.picked.clear(),
            // The highest (or lowest) of the notes starting together, of those picked (or all).
            RollOp::SelectVoice(top) => {
                let notes = notes_of(self);
                let from: Vec<usize> = match picked.is_empty() {
                    true => (0..notes.len()).collect(),
                    false => picked.clone(),
                };
                let mut best: std::collections::BTreeMap<i64, usize> = Default::default();
                for &i in &from {
                    let n = notes[i];
                    let e = best.entry(n.start).or_insert(i);
                    let better = match top {
                        true => n.pitch > notes[*e].pitch,
                        false => n.pitch < notes[*e].pitch,
                    };
                    if better {
                        *e = i;
                    }
                }
                self.region = Some(at);
                self.picked = best.into_values().collect();
            }
            RollOp::Invert => {
                self.region = Some(at);
                self.picked = (0..count).filter(|i| !picked.contains(i)).collect();
            }
            // Muted if any picked note plays; else all of them play again.
            RollOp::Mute => {
                if picked.is_empty() {
                    return;
                }
                let notes = notes_of(self);
                let mute = picked
                    .iter()
                    .any(|&i| notes.get(i).is_some_and(|n| !n.muted));
                self.edit(cx, |ws| {
                    if let Some(notes) = ws.session.project.notes_mut(at) {
                        for &i in &picked {
                            if let Some(n) = notes.get_mut(i) {
                                n.muted = mute;
                            }
                        }
                    }
                });
            }
            RollOp::Copy | RollOp::Cut => {
                let notes = notes_of(self);
                let mut clip: Vec<Note> = picked
                    .iter()
                    .filter_map(|&i| notes.get(i).copied())
                    .collect();
                let first = clip.iter().map(|n| n.start).min().unwrap_or(0);
                for n in &mut clip {
                    n.start -= first;
                }
                if !clip.is_empty() {
                    self.status = Some(format!(
                        "Copied {} note{}",
                        clip.len(),
                        if clip.len() == 1 { "" } else { "s" }
                    ));
                    self.clip = clip;
                }
                if op == RollOp::Cut && !picked.is_empty() {
                    self.delete_notes(at, cx);
                }
            }
            RollOp::Paste => {
                if self.clip.is_empty() {
                    return;
                }
                let r = self.session.project.region(at).clone();
                // At the playhead, if it's in the region; else where the region starts.
                let tick = self.playhead();
                let to = match (r.start..r.start + r.length).contains(&tick) {
                    true => self.snap_down(tick as f64) - r.start,
                    false => 0,
                } as i64;
                let clip = self.clip.clone();
                self.edit(cx, |ws| {
                    let Some(notes) = ws.session.project.notes_mut(at) else {
                        return;
                    };
                    let first = notes.len();
                    notes.extend(clip.iter().map(|n| Note {
                        start: n.start + to,
                        ..*n
                    }));
                    ws.picked = (first..notes.len()).collect();
                    ws.region = Some(at);
                    ws.grow_to_notes(at);
                });
            }
            RollOp::Duplicate => {
                let notes = notes_of(self);
                let chosen: Vec<Note> = picked
                    .iter()
                    .filter_map(|&i| notes.get(i).copied())
                    .collect();
                let (Some(from), Some(to)) = (
                    chosen.iter().map(|n| n.start).min(),
                    chosen.iter().map(|n| n.start + n.length as i64).max(),
                ) else {
                    return;
                };
                // After itself, its span rounded up to the beat (a bar's worth, a bar on).
                let beat = self.session.project.beat() as i64;
                let span = ((to - from) + beat - 1) / beat * beat;
                self.edit(cx, |ws| {
                    let Some(notes) = ws.session.project.notes_mut(at) else {
                        return;
                    };
                    let first = notes.len();
                    notes.extend(chosen.iter().map(|n| Note {
                        start: n.start + span,
                        ..*n
                    }));
                    ws.picked = (first..notes.len()).collect();
                    ws.grow_to_notes(at);
                });
            }
            RollOp::Delete => {
                if !picked.is_empty() {
                    self.delete_notes(at, cx);
                }
            }
            RollOp::Transpose(by) => {
                if picked.is_empty() {
                    return;
                }
                let key = self
                    .session
                    .project
                    .song_key()
                    .filter(|_| self.roll.in_key && by.abs() == 1);
                let track = self.session.project.tracks[at.0].id;
                let mut heard = None;
                self.edit(cx, |ws| {
                    let Some(notes) = ws.session.project.notes_mut(at) else {
                        return;
                    };
                    for &i in &picked {
                        let Some(n) = notes.get_mut(i) else { continue };
                        let mut p = (n.pitch as i32 + by).clamp(0, 127);
                        // In key, a step is the scale's next note.
                        if let Some(k) = key {
                            while !k.contains(p as u8) && (1..127).contains(&p) {
                                p += by;
                            }
                        }
                        n.pitch = p as u8;
                        heard = Some((n.pitch, n.velocity));
                    }
                });
                if let Some((pitch, v)) = heard {
                    self.player.preview(track, pitch, v);
                    self.player.preview(track, pitch, 0);
                }
            }
            RollOp::Nudge(quarters) => {
                if picked.is_empty() {
                    return;
                }
                // A grid step, or a quarter of one (a tick at least).
                let by = match self.grid() as i64 * quarters as i64 / 4 {
                    0 => quarters.signum() as i64,
                    by => by,
                };
                let region_start = self.session.project.region(at).start as i64;
                self.edit(cx, |ws| {
                    let Some(notes) = ws.session.project.notes_mut(at) else {
                        return;
                    };
                    for &i in &picked {
                        if let Some(n) = notes.get_mut(i) {
                            n.start = (n.start + by).max(-region_start);
                        }
                    }
                });
            }
            RollOp::Quantize => {
                let edit = NoteEdit::Quantize {
                    grid: self.roll.grid,
                    strength: self.roll.strength,
                    swing: self.roll.swing,
                };
                self.edit_notes(at, edit, cx);
            }
            RollOp::Edit(edit) => self.edit_notes(at, edit, cx),
        }
        cx.notify();
    }

    /// The menu a right-click on the piano roll opens: on a note, or on the grid.
    pub(crate) fn roll_menu(
        &mut self,
        e: &MouseDownEvent,
        on: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        let Some(at) = self.roll_region() else { return };
        if let Some(i) = on
            && !self.picked.contains(&i)
        {
            self.picked = vec![i];
        }
        let pitch = self.pitch_at(f32::from(e.position.y));
        let some = !self.picked.is_empty();
        let mut items: Vec<(String, MenuAction)> = Vec::new();
        let mut add = |label: String, op: RollOp, on: bool| {
            items.push((
                label,
                if on {
                    MenuAction::Roll(op)
                } else {
                    MenuAction::Nothing
                },
            ))
        };
        add("Select All    ⌘A".into(), RollOp::SelectAll, true);
        add(
            format!("Select Every {}", self.note_name(pitch)),
            RollOp::SelectPitch(pitch),
            true,
        );
        add("Copy    ⌘C".into(), RollOp::Copy, some);
        add("Cut    ⌘X".into(), RollOp::Cut, some);
        add(
            "Paste at Playhead    ⌘V".into(),
            RollOp::Paste,
            !self.clip.is_empty(),
        );
        add("Duplicate    ⌘D".into(), RollOp::Duplicate, some);
        add("Delete    ⌫".into(), RollOp::Delete, some);
        add("Mute / Unmute    0".into(), RollOp::Mute, some);
        add("Quantize    Q".into(), RollOp::Quantize, true);
        add("Up an Octave    ⇧↑".into(), RollOp::Transpose(12), some);
        add("Down an Octave    ⇧↓".into(), RollOp::Transpose(-12), some);
        add("Legato".into(), RollOp::Edit(NoteEdit::Legato), true);
        add(
            "Strum".into(),
            RollOp::Edit(NoteEdit::Strum { ticks: PPQ / 32 }),
            true,
        );
        add("Glue".into(), RollOp::Edit(NoteEdit::Glue), true);
        add("Reverse".into(), RollOp::Edit(NoteEdit::Reverse), true);
        add(
            "Flip Upside Down".into(),
            RollOp::Edit(NoteEdit::Flip),
            true,
        );
        let _ = at;
        self.menu = Some(Menu {
            at: e.position,
            items,
            ..Default::default()
        });
        cx.notify();
    }
}
