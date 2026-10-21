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

    /// The quantize grid, strength, swing and one-click tools, left of the keyboard.
    pub(crate) fn roll_tools(
        &mut self,
        at: Option<At>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let grid_seg = |label: &'static str, g: u64, on: bool, cx: &mut Context<Self>| {
            seg(label, on, 20.)
                .flex_1()
                .px(px(0.))
                .font_family(MONO)
                .text_size(px(10.5))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.roll.grid = g;
                        cx.notify();
                    }),
                )
        };
        let g = self.roll.grid;
        let slider =
            |ws: &Self, label: &'static str, s: Slider, text: String, cx: &mut Context<Self>| {
                div()
                    .h(px(18.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(div().w(px(46.)).child(label))
                    .child(ws.slider_bar(s, cx).flex_1().min_w(px(0.)))
                    .child(
                        div()
                            .w(px(30.))
                            .flex()
                            .justify_end()
                            .font_family(MONO)
                            .text_color(rgb(TEXT_2))
                            .child(text),
                    )
            };
        // The tools work on the picked notes (or all of them); with no region, greyed out.
        let tool = |label: &'static str, edit: NoteEdit, cx: &mut Context<Self>| {
            let b = button(label, false)
                .h(px(24.))
                .text_size(px(11.))
                .debug_selector(move || format!("roll-{label}"));
            match at {
                Some(at) => b.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.edit_notes(at, edit, cx)),
                ),
                None => b.opacity(0.4).cursor_default(),
            }
        };
        let picked = self.picked.len();
        div()
            .id("roll-tools")
            .flex_1()
            .min_w(px(0.))
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child("Quantize"),
                    )
                    .child(
                        tray()
                            .p(px(2.))
                            .rounded(px(7.))
                            .child(grid_seg("1/4", PPQ, g == PPQ, cx))
                            .child(grid_seg("1/8", PPQ / 2, g == PPQ / 2, cx))
                            .child(grid_seg("1/16", PPQ / 4, g == PPQ / 4, cx))
                            .child(grid_seg("1/32", PPQ / 8, g == PPQ / 8, cx)),
                    ),
            )
            .child(slider(
                self,
                "Strength",
                Slider::Strength,
                format!("{:.0}%", self.roll.strength * 100.0),
                cx,
            ))
            .child(slider(
                self,
                "Swing",
                Slider::Swing,
                format!("{:.0}%", self.roll.swing),
                cx,
            ))
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .child(match picked {
                        0 => "Tools · on every note".to_string(),
                        1 => "Tools · on the picked note".to_string(),
                        n => format!("Tools · on {n} picked notes"),
                    }),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(4.))
                    .child(tool(
                        "Humanize",
                        NoteEdit::Humanize {
                            timing: 20,
                            velocity: 8,
                            seed: fresh_seed(),
                        },
                        cx,
                    ))
                    .child(tool("Legato", NoteEdit::Legato, cx))
                    .child(tool("Chop", NoteEdit::Chop { grid: g }, cx))
                    .child(tool("Arpeggiate", NoteEdit::Arpeggiate { grid: g }, cx))
                    .child(tool("Strum", NoteEdit::Strum { ticks: PPQ / 32 }, cx))
                    .child(tool("Glue", NoteEdit::Glue, cx))
                    .child(tool("Reverse", NoteEdit::Reverse, cx))
                    .child(tool("Flip", NoteEdit::Flip, cx))
                    .child(tool(
                        "Softer",
                        NoteEdit::Dynamics {
                            percent: 100,
                            by: -10,
                        },
                        cx,
                    ))
                    .child(tool(
                        "Harder",
                        NoteEdit::Dynamics {
                            percent: 100,
                            by: 10,
                        },
                        cx,
                    )),
            )
    }
}
