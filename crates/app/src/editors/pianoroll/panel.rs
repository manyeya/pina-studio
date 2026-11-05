//! The panel left of the roll's keyboard: the picked notes' settings, to read and change for all
//! of them at once (Notes), the tools that change notes (Transform), and those that write them
//! (Generate, Harmony).

use super::*;

/// What the roll's panel shows.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Panel {
    #[default]
    Notes,
    Transform,
    Generate,
    Harmony,
}

/// A setting every picked note has, changed for all of them at once.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum NoteField {
    Pitch,
    Start,
    Length,
    Velocity,
    Spread,
    Chance,
}

impl NoteField {
    pub fn name(self) -> &'static str {
        match self {
            NoteField::Pitch => "Pitch",
            NoteField::Start => "Start",
            NoteField::Length => "Length",
            NoteField::Velocity => "Velocity",
            NoteField::Spread => "Range",
            NoteField::Chance => "Chance",
        }
    }

    /// How far a drag goes for a step, in pixels.
    pub(crate) fn px(self) -> f32 {
        match self {
            NoteField::Pitch => 6.0,
            NoteField::Start | NoteField::Length => 8.0,
            // A full chance is as tall as the lane under the notes.
            NoteField::Chance => VELOCITY_H / 100.0,
            _ => 1.5,
        }
    }
}

/// Note `n` with `field` moved `by` steps (in time a step is `grid` ticks), kept in range: no
/// earlier than `floor` (the region's start), no shorter than a quarter of the grid.
pub(crate) fn step_note(mut n: Note, field: NoteField, by: i32, grid: u64, floor: i64) -> Note {
    let by = by as i64;
    match field {
        NoteField::Pitch => n.pitch = (n.pitch as i64 + by).clamp(0, 127) as u8,
        NoteField::Start => n.start = (n.start + by * grid as i64).max(floor),
        NoteField::Length => {
            let least = (grid / 4).max(1) as i64;
            n.length = (n.length as i64 + by * grid as i64).max(least) as u64;
        }
        NoteField::Velocity => n.velocity = (n.velocity as i64 + by).clamp(1, 127) as u8,
        NoteField::Spread => n.spread = (n.spread as i64 + by).clamp(0, 127) as u8,
        NoteField::Chance => n.chance = (n.chance as i64 + by).clamp(1, 100) as u8,
    }
    n
}

impl Workspace {
    /// The picked notes of the shown region, as they are.
    pub(crate) fn picked_notes(&self, at: At) -> Vec<(usize, Note)> {
        let notes = self.session.project.region(at).content.notes(0);
        let notes = notes.as_deref().unwrap_or(&[]);
        self.picked
            .iter()
            .filter_map(|&i| notes.get(i).map(|n| (i, *n)))
            .collect()
    }

    /// Steps a setting of every picked note, as one edit; a pitch moved is heard.
    pub(crate) fn step_notes(&mut self, field: NoteField, by: i32, cx: &mut Context<Self>) {
        let Some(at) = self.roll_region() else { return };
        let was = self.picked_notes(at);
        if was.is_empty() {
            return;
        }
        let (grid, floor) = (self.grid(), -(self.session.project.region(at).start as i64));
        self.edit(cx, |ws| {
            if let Some(notes) = ws.session.project.notes_mut(at) {
                for (i, n) in &was {
                    notes[*i] = step_note(*n, field, by, grid, floor);
                }
            }
            ws.grow_to_notes(at);
        });
        if field == NoteField::Pitch
            && let Some(n) = self.picked_notes(at).first().map(|p| p.1)
        {
            let track = self.session.project.tracks[at.0].id;
            self.player.preview(track, n.pitch, n.velocity);
            self.player.preview(track, n.pitch, 0);
        }
        cx.notify();
    }

    /// A setting dragged up or down: every picked note moved from where it was by as many steps.
    pub(crate) fn drag_note_field(&mut self, p: Point<f32>) {
        let Some(Drag::NoteField { at, y, field }) = self.drag else {
            return;
        };
        let by = (-(p.y - y) / field.px()).round() as i32;
        let (grid, floor) = (self.grid(), -(self.session.project.region(at).start as i64));
        let held = self.held.clone();
        if let Some(notes) = self.session.project.notes_mut(at) {
            for (i, was) in held {
                if let Some(n) = notes.get_mut(i) {
                    *n = step_note(was, field, by, grid, floor);
                }
            }
        }
    }

    /// What a setting reads for the picked notes: theirs, or from the least to the most.
    fn field_text(&self, field: NoteField, notes: &[Note]) -> String {
        let say = |n: &Note| match field {
            NoteField::Pitch => self.note_name(n.pitch),
            NoteField::Start => {
                let start = self
                    .roll_region()
                    .map_or(0, |at| self.session.project.region(at).start);
                self.position((start as i64 + n.start).max(0) as u64)
            }
            NoteField::Length => length_name(n.length),
            NoteField::Velocity => n.velocity.to_string(),
            NoteField::Spread => format!("±{}", n.spread),
            NoteField::Chance => format!("{}%", n.chance),
        };
        let key = |n: &Note| match field {
            NoteField::Pitch => n.pitch as i64,
            NoteField::Start => n.start,
            NoteField::Length => n.length as i64,
            NoteField::Velocity => n.velocity as i64,
            NoteField::Spread => n.spread as i64,
            NoteField::Chance => n.chance as i64,
        };
        let (Some(lo), Some(hi)) = (
            notes.iter().min_by_key(|n| key(n)),
            notes.iter().max_by_key(|n| key(n)),
        ) else {
            return "–".into();
        };
        match key(lo) == key(hi) {
            true => say(lo),
            false => format!("{} – {}", say(lo), say(hi)),
        }
    }

    /// A setting's row: its name, its value (drag it up or down), and a step down and up.
    fn note_field(
        &self,
        field: NoteField,
        notes: &[Note],
        at: Option<At>,
        cx: &mut Context<Self>,
    ) -> Div {
        let some = !notes.is_empty() && at.is_some();
        let step = |label: &'static str, by: i32, cx: &mut Context<Self>| {
            div()
                .size(px(22.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .bg(rgb(FIELD))
                .text_color(rgb(TEXT_2))
                .when(some, |d| d.cursor_pointer().hover(|s| s.bg(rgb(RAISED))))
                .debug_selector(move || format!("note-{}-{label}", field.name()))
                .child(match by > 0 {
                    true => "+",
                    false => "−",
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.step_notes(field, by, cx);
                    }),
                )
        };
        // A pitch steps a semitone, a time a grid step, a velocity, range or chance by 1 (5).
        let by = match field {
            NoteField::Chance => 5,
            _ => 1,
        };
        // Its name over it, the value (drag it) and its steps side by side.
        div()
            .flex()
            .flex_col()
            .gap(px(3.))
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .child(field.name()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .h(px(22.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .rounded(px(5.))
                            .bg(rgb(FIELD))
                            .font_family(MONO)
                            .text_size(px(11.5))
                            .text_color(rgb(if some { TEXT } else { TEXT_4 }))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .when(some, |d| d.cursor_ns_resize().hover(|s| s.bg(rgb(RAISED))))
                            .debug_selector(move || format!("note-field-{}", field.name()))
                            .child(self.field_text(field, notes))
                            .when_some(at.filter(|_| some), |d, at| {
                                d.on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                        cx.stop_propagation();
                                        ws.held = ws.picked_notes(at);
                                        let y = f32::from(e.position.y);
                                        ws.begin_drag(Drag::NoteField { at, y, field }, cx);
                                    }),
                                )
                            }),
                    )
                    .child(step("down", -by, cx))
                    .child(step("up", by, cx)),
            )
    }

    /// The picked notes' settings, their mute, and ways to pick others.
    fn notes_tab(&self, at: Option<At>, cx: &mut Context<Self>) -> Div {
        let picked: Vec<Note> = at.map_or(Vec::new(), |at| {
            self.picked_notes(at).into_iter().map(|p| p.1).collect()
        });
        let all = at
            .and_then(|at| {
                self.session
                    .project
                    .region(at)
                    .content
                    .notes(0)
                    .map(|n| n.len())
            })
            .unwrap_or(0);
        let muted = !picked.is_empty() && picked.iter().all(|n| n.muted);
        let some = !picked.is_empty();
        let pick = |label: &'static str, op: RollOp, cx: &mut Context<Self>| {
            button(label, false)
                .flex_1()
                .h(px(24.))
                .text_size(px(11.))
                .debug_selector(move || format!("roll-pick-{label}"))
                .when(at.is_none(), |d| d.opacity(0.4).cursor_default())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.roll_op(op, cx)),
                )
        };
        let summary = match (picked.len(), all) {
            (0, 0) => "No notes yet: double-click the grid".to_string(),
            (0, n) => format!("{n} notes · pick some (⌘A all)"),
            (1, _) => "1 note picked".to_string(),
            (k, n) => format!("{k} of {n} notes picked"),
        };
        let mute = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(11.5))
            .text_color(rgb(TEXT_3))
            .child("Muted")
            .child(
                div()
                    .px(px(5.))
                    .rounded(px(4.))
                    .border_1()
                    .border_color(rgb(LINE_STRONG))
                    .font_family(MONO)
                    .text_size(px(10.))
                    .text_color(rgb(TEXT_4))
                    .child("0"),
            )
            .child(
                switch(muted)
                    .debug_selector(|| "note-muted".into())
                    .when(!some, |d| d.opacity(0.4))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.roll_op(RollOp::Mute, cx)),
                    ),
            );
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .h(px(22.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .child(
                        div()
                            .min_w(px(0.))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(summary),
                    )
                    .child(mute),
            )
            // Two columns: where and how long, then how hard, how varied and how likely.
            .child(
                div().grid().grid_cols(2).gap_x(px(12.)).children(
                    [
                        NoteField::Pitch,
                        NoteField::Velocity,
                        NoteField::Start,
                        NoteField::Spread,
                        NoteField::Length,
                        NoteField::Chance,
                    ]
                    .map(|f| self.note_field(f, &picked, at, cx)),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .w(px(52.))
                            .flex_none()
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_3))
                            .child("Pick"),
                    )
                    .child(pick("Top", RollOp::SelectVoice(true), cx))
                    .child(pick("Bottom", RollOp::SelectVoice(false), cx))
                    .child(pick("Invert", RollOp::Invert, cx)),
            )
    }

    /// The quantize grid, strength, swing, and the one-click tools.
    fn transform_tab(&mut self, at: Option<At>, cx: &mut Context<Self>) -> Div {
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
            .flex()
            .flex_col()
            .gap(px(8.))
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
                    .grid_cols(3)
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
                    ))
                    .child(tool("Staccato", NoteEdit::Span { percent: 50 }, cx))
                    .child(tool(
                        "Flam",
                        NoteEdit::Ornament {
                            ticks: PPQ / 32,
                            grace: false,
                        },
                        cx,
                    ))
                    .child(tool(
                        "Grace",
                        NoteEdit::Ornament {
                            ticks: PPQ / 16,
                            grace: true,
                        },
                        cx,
                    ))
                    .child(tool("Connect", NoteEdit::Connect { grid: g }, cx))
                    .child(tool("2× speed", NoteEdit::Stretch { percent: 50 }, cx))
                    .child(tool("½ speed", NoteEdit::Stretch { percent: 200 }, cx))
                    .child(tool(
                        "Crescendo",
                        NoteEdit::VelocityShape { from: 50, to: 115 },
                        cx,
                    ))
                    .child(tool(
                        "Fade",
                        NoteEdit::VelocityShape { from: 115, to: 50 },
                        cx,
                    )),
            )
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .child("Recombine · in time order"),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(3)
                    .gap(px(4.))
                    .child(tool(
                        "Pitches",
                        NoteEdit::Recombine {
                            part: pina_engine::music::NotePart::Pitch,
                            shuffle: false,
                            seed: 0,
                        },
                        cx,
                    ))
                    .child(tool(
                        "Velocities",
                        NoteEdit::Recombine {
                            part: pina_engine::music::NotePart::Velocity,
                            shuffle: false,
                            seed: 0,
                        },
                        cx,
                    ))
                    .child(tool(
                        "Lengths",
                        NoteEdit::Recombine {
                            part: pina_engine::music::NotePart::Length,
                            shuffle: false,
                            seed: 0,
                        },
                        cx,
                    ))
                    .child(tool(
                        "Shuffle",
                        NoteEdit::Recombine {
                            part: pina_engine::music::NotePart::Pitch,
                            shuffle: true,
                            seed: fresh_seed(),
                        },
                        cx,
                    ))
                    .when_some(self.session.project.song_key(), |d, k| {
                        d.child(tool(
                            "Into key",
                            NoteEdit::ScaleQuantize {
                                root: k.root,
                                minor: k.minor,
                            },
                            cx,
                        ))
                    }),
            )
    }

    /// The panel: its tabs, and the one picked.
    pub(crate) fn roll_panel(
        &mut self,
        at: Option<At>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let panel = self.roll.panel;
        let tab = |label: &'static str, p: Panel, cx: &mut Context<Self>| {
            seg(label, panel == p, 22.)
                .flex_1()
                .debug_selector(move || format!("roll-panel-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.roll.panel = p;
                        cx.notify();
                    }),
                )
        };
        let body = match panel {
            Panel::Notes => self.notes_tab(at, cx),
            Panel::Transform => self.transform_tab(at, cx),
            Panel::Generate => self.generate_tab(cx),
            Panel::Harmony => self.harmony_tab(cx),
        };
        div()
            .id("roll-tools")
            .flex_1()
            .min_w(px(0.))
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .overflow_y_scroll()
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(7.))
                    .child(tab("Notes", Panel::Notes, cx))
                    .child(tab("Transform", Panel::Transform, cx))
                    .child(tab("Generate", Panel::Generate, cx))
                    .child(tab("Harmony", Panel::Harmony, cx)),
            )
            .child(body)
    }
}
