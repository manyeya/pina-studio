//! The roll panel's MPE tab: shapes of expression written on the picked notes (or all of them)
//! with a click (a scoop, a fall, a vibrato, a glide into the next note, a slide opening, a
//! pressure swell), how far and how fast they go, each note's expression copied, pasted and
//! taken off; and whether the track's instrument hears it, with what it takes if it doesn't.

use pina_engine::song::project::{Format, Gesture, Instrument};

use super::*;
use crate::workspace::RollSettings;

impl Workspace {
    /// Which instrument hears the notes' own expression, and how; a button to make it if not.
    fn heard_by(&self, t: usize, cx: &mut Context<Self>) -> Div {
        let track = &self.session.project.tracks[t];
        let note = |text: String| {
            div()
                .flex_1()
                .min_w(px(0.))
                .text_size(px(11.))
                .line_height(px(15.))
                .text_color(rgb(TEXT_3))
                .child(text)
        };
        let fix = |label: &'static str, cx: &mut Context<Self>, mpe: bool| {
            button(label, false)
                .h(px(24.))
                .px(px(8.))
                .flex_none()
                .text_size(px(11.))
                .debug_selector(move || format!("mpe-tab-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        match mpe {
                            true => ws.choose(MenuAction::Mpe(t), cx),
                            false => ws.open_mpe(t, e.position, cx),
                        }
                    }),
                )
        };
        let row = div().flex().items_center().gap(px(8.));
        match &track.instrument {
            Some(Instrument::Poly(_)) => {
                row.child(note("Synth-0 plays each note's own pitch, slide and pressure. Slide does nothing until it's routed to a knob.".into()))
            }
            Some(Instrument::Plugin(r)) if matches!(r.format, Format::Vst3 | Format::Clap) => {
                row.child(note(format!("{} hears each note's own as note expression.", r.class)))
            }
            Some(Instrument::Plugin(r)) if r.mpe => row
                .child(note(format!("{} hears MPE, a channel per note.", r.class)))
                .child(fix("Zone…", cx, false)),
            Some(Instrument::Plugin(r)) => row
                .child(note(format!("{} hears none of it until MPE is on.", r.class)))
                .child(fix("Turn on MPE", cx, true)),
            _ => row.child(note(
                "This instrument plays no note's own expression: Synth-0 and most plugins do."
                    .into(),
            )),
        }
    }

    /// Shapes of expression, a click each, with how far and how fast; copied, pasted, taken off.
    pub(crate) fn mpe_tab(&mut self, at: Option<At>, cx: &mut Context<Self>) -> (Div, Option<Div>) {
        let notes: Vec<Note> = at
            .and_then(|at| {
                self.session
                    .project
                    .region(at)
                    .content
                    .notes(0)
                    .map(|n| n.to_vec())
            })
            .unwrap_or_default();
        let on: Vec<&Note> = match self.picked.is_empty() {
            true => notes.iter().collect(),
            false => self.picked.iter().filter_map(|&i| notes.get(i)).collect(),
        };
        let summary = match (self.picked.len(), notes.len()) {
            (_, 0) => "No notes yet".to_string(),
            (0, n) => format!("On all {n} notes · click one to pick it"),
            (1, _) => "On the picked note".to_string(),
            (k, _) => format!("On the {k} picked notes"),
        };
        let (amount, rate) = (self.roll.gesture_amount, self.roll.gesture_rate);
        let shape = |g: Gesture, cx: &mut Context<Self>| {
            let b = button(g.name(), false)
                .h(px(24.))
                .text_size(px(11.))
                .debug_selector(move || format!("mpe-{}", g.name()));
            match at {
                Some(at) => b.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        let edit = NoteEdit::Express {
                            gesture: g,
                            amount: ws.roll.gesture_amount,
                            rate: ws.roll.gesture_rate,
                        };
                        ws.edit_notes(at, edit, cx);
                        // Its lane shows, to see it and change it.
                        ws.roll.lane = Lane::of(g.dim());
                    }),
                ),
                None => b.opacity(0.4).cursor_default(),
            }
        };
        let clear = |dim: Option<Dim>, label: &'static str, cx: &mut Context<Self>| {
            div()
                .text_size(px(10.5))
                .text_color(rgb(TEXT_3))
                .cursor_pointer()
                .hover(|s| s.text_color(rgb(TEXT)))
                .debug_selector(move || format!("mpe-clear-{label}"))
                .child("Clear")
                .when_some(at, |d, at| {
                    d.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.edit_notes(at, NoteEdit::Unexpress { dim }, cx)
                        }),
                    )
                })
        };
        let section = |dim: Dim, cx: &mut Context<Self>| {
            let has = on
                .iter()
                .filter(|n| !n.expression.get(dim).is_empty())
                .count();
            let shapes: Vec<Div> = Gesture::ALL
                .into_iter()
                .filter(|g| g.dim() == dim)
                .map(|g| shape(g, cx))
                .collect();
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap(px(6.))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .font_weight(FontWeight::MEDIUM)
                                .child(dim.name()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_4))
                                .child(match has {
                                    0 => String::new(),
                                    1 => "1 note has it".into(),
                                    n => format!("{n} notes have it"),
                                }),
                        )
                        .when(has > 0, |d| d.child(clear(Some(dim), dim.name(), cx))),
                )
                .child(div().grid().grid_cols(4).gap(px(4.)).children(shapes))
        };
        let stepper = |label: &'static str,
                       value: String,
                       cx: &mut Context<Self>,
                       f: fn(&mut RollSettings, i32)| {
            let step = |sign: &'static str, by: i32, cx: &mut Context<Self>| {
                button(sign, false)
                    .w(px(22.))
                    .h(px(22.))
                    .flex_none()
                    .debug_selector(move || format!("mpe-{label}-{sign}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            f(&mut ws.roll, by);
                            cx.notify();
                        }),
                    )
            };
            // Its name over its value, stepped down and up.
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .child(label),
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
                                .px(px(6.))
                                .flex()
                                .items_center()
                                .rounded(px(5.))
                                .bg(rgb(FIELD))
                                .font_family(MONO)
                                .text_size(px(11.))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .child(value),
                        )
                        .child(step("−", -1, cx))
                        .child(step("+", 1, cx)),
                )
        };
        let some = !on.is_empty() && at.is_some();
        let tool = |label: &'static str, op: RollOp, ok: bool, cx: &mut Context<Self>| {
            button(label, false)
                .flex_1()
                .h(px(24.))
                .text_size(px(11.))
                .debug_selector(move || format!("mpe-{label}"))
                .when(!ok, |d| d.opacity(0.4).cursor_default())
                .when(ok, |d| {
                    d.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.roll_op(op, cx)),
                    )
                })
        };
        let one = self.picked.len() == 1;
        let pasting = self.expression_clip.is_some() && some;
        let any = on.iter().any(|n| !n.expression.is_empty());
        // What the shapes go on, said where it can't be missed: the picked notes, or all.
        let target = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(8.))
            .h(px(26.))
            .rounded(px(7.))
            .bg(match self.picked.is_empty() {
                true => rgb(FIELD).into(),
                false => alpha(ACCENT, 0.16),
            })
            .debug_selector(|| "mpe-target".into())
            .child(
                div()
                    .size(px(7.))
                    .rounded(px(4.))
                    .bg(match self.picked.is_empty() {
                        true => rgb(TEXT_4),
                        false => rgb(ACCENT),
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .text_size(px(11.))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(summary),
            );
        let body = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(target)
            .when_some(at, |d, at| d.child(self.heard_by(at.0, cx)))
            .child(section(Dim::Pitch, cx))
            .child(section(Dim::Slide, cx))
            .child(section(Dim::Press, cx));
        let foot = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .gap(px(10.))
                    .child(stepper("Amount", format!("{amount}%"), cx, |r, by| {
                        r.gesture_amount = (r.gesture_amount as i32 + by * 10).clamp(10, 300) as u32
                    }))
                    .child(stepper(
                        "Speed",
                        format!("{rate:.1} a beat"),
                        cx,
                        |r, by| {
                            r.gesture_rate = (r.gesture_rate + by as f32 * 0.5).clamp(0.5, 12.0)
                        },
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .child(tool("Copy", RollOp::CopyExpression, one, cx))
                    .child(tool("Paste", RollOp::PasteExpression, pasting, cx))
                    .child(tool(
                        "Clear all",
                        RollOp::Edit(NoteEdit::Unexpress { dim: None }),
                        any,
                        cx,
                    )),
            );
        (body, Some(foot))
    }
}
