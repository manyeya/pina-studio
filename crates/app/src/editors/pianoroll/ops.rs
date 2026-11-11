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
                    let n = &notes[i];
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
            RollOp::CopyExpression => {
                let notes = notes_of(self);
                if let Some(n) = picked.first().and_then(|&i| notes.get(i)) {
                    self.expression_clip = Some((n.expression.clone(), n.length));
                }
            }
            RollOp::PasteExpression => {
                let Some((copied, length)) = self.expression_clip.clone() else {
                    return;
                };
                let onto: Vec<usize> = match picked.is_empty() {
                    true => (0..count).collect(),
                    false => picked.clone(),
                };
                self.edit(cx, |ws| {
                    if let Some(notes) = ws.session.project.notes_mut(at) {
                        for &i in &onto {
                            if let Some(n) = notes.get_mut(i) {
                                let mut e = copied.clone();
                                e.stretch(n.length as f64 / length.max(1) as f64);
                                n.expression = e;
                            }
                        }
                    }
                });
            }
            RollOp::Copy | RollOp::Cut => {
                let notes = notes_of(self);
                let mut clip: Vec<Note> = picked
                    .iter()
                    .filter_map(|&i| notes.get(i).cloned())
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
                        ..n.clone()
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
                    .filter_map(|&i| notes.get(i).cloned())
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
                        ..n.clone()
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
                // A grid step, or a quarter of one (a tick at least).
                let by = match self.grid() as i64 * quarters as i64 / 4 {
                    0 => quarters.signum() as i64,
                    by => by,
                };
                return self.roll_op(RollOp::Move(by), cx);
            }
            RollOp::Move(by) => {
                if picked.is_empty() {
                    return;
                }
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
            RollOp::Resize(by) => {
                if picked.is_empty() {
                    return;
                }
                self.edit(cx, |ws| {
                    let Some(notes) = ws.session.project.notes_mut(at) else {
                        return;
                    };
                    for &i in &picked {
                        if let Some(n) = notes.get_mut(i) {
                            n.length = (n.length as i64 + by).max(1) as u64;
                        }
                    }
                });
            }
            RollOp::Step { next, extend } => {
                let notes = notes_of(self);
                let order = in_order(&notes);
                let here = self.playhead() as i64 - self.session.project.region(at).start as i64;
                let last = picked
                    .last()
                    .and_then(|&i| order.iter().position(|&o| o == i));
                let to = match last {
                    Some(k) if next => order.get(k + 1),
                    Some(k) => k.checked_sub(1).and_then(|k| order.get(k)),
                    None if next => order.iter().find(|&&i| notes[i].start >= here),
                    None => order.iter().rev().find(|&&i| notes[i].start < here),
                };
                let Some(&to) = to else { return cx.notify() };
                self.region = Some(at);
                match (extend && !picked.is_empty(), picked.contains(&to)) {
                    (true, true) => drop(self.picked.pop()),
                    (true, false) => self.picked.push(to),
                    (false, _) => self.picked = vec![to],
                }
                self.hear_note(at, &notes[to]);
            }
            RollOp::Edge(last) => {
                let notes = notes_of(self);
                let order = in_order(&notes);
                let to = if last { order.last() } else { order.first() };
                if let Some(&to) = to {
                    self.region = Some(at);
                    self.picked = vec![to];
                    self.hear_note(at, &notes[to]);
                }
            }
            RollOp::Pick(rule) => {
                let notes = notes_of(self);
                let p = &self.session.project;
                let start = p.region(at).start as i64;
                let bar = p.bar().max(1) as i64;
                let here = self.playhead() as i64 - start;
                let chosen: Vec<&Note> = picked.iter().filter_map(|&i| notes.get(i)).collect();
                let needs_some = matches!(
                    rule,
                    NotePick::FollowingSamePitch
                        | NotePick::Equal
                        | NotePick::Similar
                        | NotePick::SameSubposition
                );
                if needs_some && chosen.is_empty() {
                    self.status = Some("Pick a note first".into());
                    return cx.notify();
                }
                let cycle = p.cycle.filter(|c| c.end > c.start);
                if rule == NotePick::InsideCycle && cycle.is_none() {
                    self.status = Some("Sweep a cycle in the ruler first".into());
                    return cx.notify();
                }
                let from = chosen.iter().map(|n| n.start).min().unwrap_or(here);
                let pitches: Vec<u8> = chosen.iter().map(|n| n.pitch).collect();
                let places: Vec<i64> = chosen
                    .iter()
                    .map(|n| (start + n.start).rem_euclid(bar))
                    .collect();
                let keep = |i: usize, n: &Note| match rule {
                    NotePick::Following => n.start >= from,
                    NotePick::FollowingSamePitch => n.start >= from && pitches.contains(&n.pitch),
                    NotePick::InsideCycle => cycle.is_some_and(|c| {
                        (c.start as i64..c.end as i64).contains(&(start + n.start))
                    }),
                    NotePick::Overlapped => notes.iter().enumerate().any(|(j, o)| {
                        j != i
                            && o.pitch == n.pitch
                            && o.start < n.start + n.length as i64
                            && n.start < o.start + o.length as i64
                    }),
                    NotePick::Equal => pitches.contains(&n.pitch),
                    NotePick::Similar => pitches.iter().any(|p| p % 12 == n.pitch % 12),
                    NotePick::SameSubposition => {
                        places.contains(&(start + n.start).rem_euclid(bar))
                    }
                    NotePick::Muted => n.muted,
                };
                self.region = Some(at);
                self.picked = (notes.iter().enumerate())
                    .filter(|(i, n)| keep(*i, n))
                    .map(|(i, _)| i)
                    .collect();
                self.status = Some(match self.picked.len() {
                    1 => "1 note picked".into(),
                    n => format!("{n} notes picked"),
                });
            }
            RollOp::SplitAtPlayhead => {
                let here = self.playhead() as i64 - self.session.project.region(at).start as i64;
                let notes = notes_of(self);
                let across: Vec<usize> = match picked.is_empty() {
                    true => (0..notes.len()).collect(),
                    false => picked.clone(),
                }
                .into_iter()
                .filter(|&i| {
                    notes
                        .get(i)
                        .is_some_and(|n| n.start < here && here < n.start + n.length as i64)
                })
                .collect();
                if across.is_empty() {
                    self.status = Some("No note crosses the playhead".into());
                    return cx.notify();
                }
                self.edit(cx, |ws| {
                    let Some(notes) = ws.session.project.notes_mut(at) else {
                        return;
                    };
                    let mut halves = Vec::new();
                    for &i in &across {
                        let n = &mut notes[i];
                        let end = n.start + n.length as i64;
                        n.length = (here - n.start) as u64;
                        let mut rest = n.clone();
                        rest.start = here;
                        rest.length = (end - here) as u64;
                        rest.expression = Default::default();
                        halves.push(rest);
                    }
                    let first = notes.len();
                    notes.extend(halves);
                    ws.picked = across.iter().copied().chain(first..notes.len()).collect();
                });
            }
            RollOp::StartToPlayhead | RollOp::EndToPlayhead => {
                let here = self.playhead() as i64 - self.session.project.region(at).start as i64;
                let start = op == RollOp::StartToPlayhead;
                self.edit(cx, |ws| {
                    let Some(notes) = ws.session.project.notes_mut(at) else {
                        return;
                    };
                    for &i in &picked {
                        let Some(n) = notes.get_mut(i) else { continue };
                        let end = n.start + n.length as i64;
                        match start {
                            true if here < end => {
                                n.start = here;
                                n.length = (end - here) as u64;
                            }
                            false if here > n.start => n.length = (here - n.start) as u64,
                            _ => {}
                        }
                    }
                });
            }
            RollOp::ToPlayhead { then_next } => {
                let notes = notes_of(self);
                let Some(first) = picked
                    .iter()
                    .filter_map(|&i| notes.get(i))
                    .map(|n| n.start)
                    .min()
                else {
                    return;
                };
                let here = self.playhead() as i64 - self.session.project.region(at).start as i64;
                // The note after the last picked, before they move.
                let order = in_order(&notes);
                let after = order
                    .iter()
                    .rposition(|i| picked.contains(i))
                    .and_then(|k| order.get(k + 1).copied());
                self.roll_op(RollOp::Move(here - first), cx);
                if let (true, Some(next)) = (then_next, after) {
                    self.picked = vec![next];
                }
            }
            RollOp::DeleteDuplicates => {
                let notes = notes_of(self);
                let among: Vec<usize> = match picked.is_empty() {
                    true => (0..notes.len()).collect(),
                    false => picked.clone(),
                };
                let mut gone: Vec<usize> = among
                    .iter()
                    .copied()
                    .filter(|&i| {
                        (0..i).any(|j| {
                            notes[j].pitch == notes[i].pitch && notes[j].start == notes[i].start
                        })
                    })
                    .collect();
                if gone.is_empty() {
                    self.status = Some("No doubled notes".into());
                    return cx.notify();
                }
                self.status = Some(match gone.len() {
                    1 => "1 doubled note taken out".into(),
                    n => format!("{n} doubled notes taken out"),
                });
                gone.sort_unstable_by(|a, b| b.cmp(a));
                self.edit(cx, |ws| {
                    if let Some(notes) = ws.session.project.notes_mut(at) {
                        for i in gone {
                            notes.remove(i);
                        }
                    }
                    ws.picked.clear();
                });
            }
        }
        cx.notify();
    }

    /// A note sounded as it's stepped onto, as Logic plays one picked from the keys.
    fn hear_note(&mut self, at: At, n: &Note) {
        let track = self.session.project.tracks[at.0].id;
        self.player.preview(track, n.pitch, n.velocity);
        self.player.preview(track, n.pitch, 0);
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
        add(
            "Select All Following    ⇧F".into(),
            RollOp::Pick(NotePick::Following),
            true,
        );
        add(
            "Select Equal Pitches    ⇧E".into(),
            RollOp::Pick(NotePick::Equal),
            some,
        );
        add("Invert Selection    ⇧I".into(), RollOp::Invert, true);
        add("Copy    ⌘C".into(), RollOp::Copy, some);
        add("Cut    ⌘X".into(), RollOp::Cut, some);
        add(
            "Paste at Playhead    ⌘V".into(),
            RollOp::Paste,
            !self.clip.is_empty(),
        );
        add("Repeat    ⌘R".into(), RollOp::Duplicate, some);
        add("Delete    ⌫".into(), RollOp::Delete, some);
        add("Mute / Unmute    ⌃M".into(), RollOp::Mute, some);
        let expressive = self
            .picked_notes(at)
            .iter()
            .any(|(_, n)| !n.expression.is_empty());
        add(
            "Copy Expression".into(),
            RollOp::CopyExpression,
            self.picked.len() == 1 && expressive,
        );
        add(
            "Paste Expression".into(),
            RollOp::PasteExpression,
            self.expression_clip.is_some(),
        );
        add(
            "Clear Expression".into(),
            RollOp::Edit(NoteEdit::Unexpress { dim: None }),
            expressive,
        );
        add("Quantize    Q".into(), RollOp::Quantize, true);
        add("Up an Octave    ⌥⇧↑".into(), RollOp::Transpose(12), some);
        add("Down an Octave    ⌥⇧↓".into(), RollOp::Transpose(-12), some);
        add("Legato".into(), RollOp::Edit(NoteEdit::Legato), true);
        add(
            "Strum".into(),
            RollOp::Edit(NoteEdit::Strum { ticks: PPQ / 32 }),
            true,
        );
        add("Join    ⌘J".into(), RollOp::Edit(NoteEdit::Glue), true);
        add(
            "Split at Playhead    ⌘T".into(),
            RollOp::SplitAtPlayhead,
            true,
        );
        add(
            "Delete Doubled Notes    ⌥D".into(),
            RollOp::DeleteDuplicates,
            true,
        );
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

/// The notes' indices by start, then pitch: the order ← and → walk.
fn in_order(notes: &[Note]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..notes.len()).collect();
    order.sort_by_key(|&i| (notes[i].start, notes[i].pitch));
    order
}
