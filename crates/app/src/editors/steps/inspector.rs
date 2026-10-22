//! The step sequencer's inspector: the pattern's settings (length, loop, rate, playback mode,
//! legato, swing, key and scale), the selected row's (assignment, name, loop, rate, playback
//! mode, automation mode), and the selected steps' (on, chance, condition, rate, skip, note,
//! velocity, gate, tie, offset, repeat and how the repeats go).

use gpui::Window;
use pina_engine::song::project::{Condition, rate_name};

use crate::workspace::Name;

use super::*;

/// A number the inspector steps up and down, or drags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    PatternLoopStart,
    PatternLoopEnd,
    Swing,
    RowLoopStart,
    RowLoopEnd,
    Step(Lane),
    Chance,
    Iteration,
    Cycles,
    Ramp,
    Curve,
    Spread,
}

impl Field {
    /// How far one notch moves it.
    fn notch(self) -> f32 {
        1.0
    }
}

impl Workspace {
    /// The steps the Step inspector shows: those picked, else the cursor's, else the first of
    /// the selected row.
    fn inspected_steps(&self) -> Vec<(usize, usize)> {
        let mut s = self.steps.picked.clone();
        if s.is_empty() {
            s.push(self.steps.cursor.unwrap_or((self.steps.row, 0)));
        }
        s
    }

    /// A field's value now (for the first of the inspected steps).
    pub(crate) fn field_value(&self, f: Field) -> f32 {
        let Some((_, p)) = self.shown_pattern() else {
            return 0.0;
        };
        let n = p.steps();
        let r = self.steps.row;
        let row = p.rows.get(r);
        let (sr, si) = self.inspected_steps()[0];
        let step = p.rows.get(sr);
        match f {
            Field::PatternLoopStart => p.rows.first().map_or(1, |r| r.loop_range(n).0 + 1) as f32,
            Field::PatternLoopEnd => p.rows.first().map_or(n, |r| r.loop_range(n).1) as f32,
            Field::Swing => p.swing,
            Field::RowLoopStart => row.map_or(1, |r| r.loop_range(n).0 + 1) as f32,
            Field::RowLoopEnd => row.map_or(n, |r| r.loop_range(n).1) as f32,
            Field::Step(l) => step.map_or(0.0, |r| match (l, r.is_automation()) {
                (Lane::Velocity, true) => r.get(l, si) * 100.0,
                _ => r.get(l, si),
            }),
            Field::Chance => step.map_or(100.0, |r| r.chance_at(si) as f32),
            Field::Iteration => step.map_or(1.0, |r| r.every_at(si).0 as f32),
            Field::Cycles => step.map_or(1.0, |r| r.every_at(si).1 as f32),
            Field::Ramp => step.map_or(0.0, |r| r.ramp_at(si) as f32),
            Field::Curve => step.map_or(0.0, |r| r.curve_at(si) as f32),
            Field::Spread => step.map_or(0.0, |r| r.spread_at(si) as f32),
        }
    }

    /// Sets a field, in place (a drag's move, or within an edit).
    pub(crate) fn set_field(&mut self, f: Field, v: f32) {
        let r = self.steps.row;
        let steps = self.inspected_steps();
        let Some(p) = self.pattern_now() else {
            return;
        };
        let n = p.steps();
        let v = v.round();
        match f {
            Field::PatternLoopStart | Field::PatternLoopEnd => {
                for row in &mut p.rows {
                    let (a, b) = row.loop_range(n);
                    let (a, b) = match f {
                        Field::PatternLoopStart => ((v as usize).clamp(1, b) - 1, b),
                        _ => (a, (v as usize).clamp(a + 1, n)),
                    };
                    (row.loop_start, row.loop_end) = (a, Some(b));
                }
            }
            Field::Swing => p.swing = v.clamp(50.0, 75.0),
            Field::RowLoopStart | Field::RowLoopEnd => {
                if let Some(row) = p.rows.get_mut(r) {
                    let (a, b) = row.loop_range(n);
                    let (a, b) = match f {
                        Field::RowLoopStart => ((v as usize).clamp(1, b) - 1, b),
                        _ => (a, (v as usize).clamp(a + 1, n)),
                    };
                    (row.loop_start, row.loop_end) = (a, Some(b));
                }
            }
            _ => {
                for (sr, si) in steps {
                    let Some(row) = p.rows.get_mut(sr) else {
                        continue;
                    };
                    row.fill_lanes();
                    match f {
                        Field::Step(l) => {
                            // An automation row's value goes in steps of a hundredth.
                            let v = match (l, row.is_automation()) {
                                (Lane::Velocity, true) => v / 100.0,
                                _ => v,
                            };
                            row.set(l, si, v);
                        }
                        Field::Chance => row.chance[si] = v.clamp(0.0, 100.0) as u8,
                        Field::Iteration => {
                            let of = row.every[si][1].max(1);
                            row.every[si][0] = (v as u8).clamp(1, of);
                        }
                        Field::Cycles => {
                            let of = (v as u8).clamp(1, 16);
                            row.every[si] = [row.every[si][0].min(of), of];
                        }
                        Field::Ramp => row.ramp[si] = v.clamp(-100.0, 100.0) as i8,
                        Field::Curve => row.curve[si] = v.clamp(-100.0, 100.0) as i8,
                        Field::Spread => row.spread[si] = v.clamp(-100.0, 100.0) as i8,
                        _ => {}
                    }
                    row.tidy_lanes();
                }
            }
        }
    }

    /// A field stepped by a notch, as an undo step.
    fn step_field(&mut self, f: Field, up: bool, cx: &mut Context<Self>) {
        let now = self.field_value(f);
        let by = if up { f.notch() } else { -f.notch() };
        self.edit(cx, |ws| ws.set_field(f, now + by));
    }

    /// A row of the inspector: its label right-aligned, then what sets it.
    fn field_row(label: &'static str, control: impl IntoElement, enabled: bool) -> Div {
        div()
            .h(px(26.))
            .flex()
            .items_center()
            .gap(px(8.))
            .when(!enabled, |d| d.opacity(0.4))
            .child(
                div()
                    .w(px(92.))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .whitespace_nowrap()
                    .child(label),
            )
            .child(control)
    }

    /// A stepper: ⌄ value ⌃, the value dragged up or down.
    fn stepper(&self, f: Field, text: String, unit: &'static str, cx: &mut Context<Self>) -> Div {
        let name = format!("{f:?}");
        let arrow = |up: bool, cx: &mut Context<Self>| {
            div()
                .w(px(18.))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|s| s.bg(rgb(LINE_STRONG)))
                .child(
                    icon(if up { Icon::ChevronUp } else { Icon::Chevron }, 11.)
                        .text_color(rgb(TEXT_2)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.step_field(f, up, cx);
                    }),
                )
        };
        div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(
                div()
                    .w(px(96.))
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .bg(rgb(RAISED))
                    .overflow_hidden()
                    .child(arrow(false, cx))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .justify_center()
                            .font_family(MONO)
                            .text_size(px(11.))
                            .text_color(rgb(TEXT))
                            .cursor_ns_resize()
                            .debug_selector(move || format!("steps-field-{name}"))
                            .child(text)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    let from = ws.field_value(f);
                                    let y = f32::from(e.position.y);
                                    ws.begin_drag(
                                        Drag::StepSeq(StepDrag::Field { field: f, y, from }),
                                        cx,
                                    );
                                }),
                            ),
                    )
                    .child(arrow(true, cx)),
            )
            .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(unit))
    }

    /// A check box.
    fn check(on: bool) -> Div {
        div()
            .size(px(15.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .cursor_pointer()
            .bg(rgb(if on { ACCENT } else { RAISED }))
            .when(on, |d| {
                d.child(icon(Icon::Check, 11.).text_color(rgb(0xffffff)))
            })
    }

    pub(crate) fn steps_inspector(&mut self, at: At, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some((_, p)) = self.shown_pattern() else {
            return div().into_any_element();
        };
        let p = p.clone();
        let tab = self.steps.tab;
        let track = &self.session.project.tracks[at.0];
        let region = self.session.project.region(at);
        let title = if region.name.is_empty() {
            track.name.clone()
        } else {
            region.name.clone()
        };
        let on_track = format!("on Track {}", track.name);
        let color = self.color(at.0);
        let tabs = [
            (InspectorTab::Pattern, "Pattern"),
            (InspectorTab::Row, "Row"),
            (InspectorTab::Step, "Step"),
        ]
        .map(|(t, name)| {
            seg(name, tab == t, 22.)
                .flex_1()
                .debug_selector(move || format!("steps-tab-{name}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.steps.tab = t;
                        cx.notify();
                    }),
                )
        });
        let body = match tab {
            InspectorTab::Pattern => self.pattern_fields(&p, cx),
            InspectorTab::Row => self.row_fields(at, &p, cx),
            InspectorTab::Step => self.step_fields(&p, cx),
        };
        div()
            .id("steps-inspector")
            .w(px(PANE_W))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(LINE))
            .overflow_y_scroll()
            .child(
                div()
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(icon(Icon::Grid, 22.).text_color(rgb(color)))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .min_w(px(0.))
                                    .child(
                                        div()
                                            .text_size(px(12.5))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .whitespace_nowrap()
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .child(title),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(TEXT_3))
                                            .child(on_track),
                                    ),
                            ),
                    )
                    .child(tray().p(px(2.)).rounded(px(7.)).children(tabs)),
            )
            .child(
                div()
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .children(body),
            )
            .into_any_element()
    }

    fn pattern_fields(&mut self, p: &Pattern, cx: &mut Context<Self>) -> Vec<Div> {
        let n = p.steps();
        let (a, b) = p.rows.first().map_or((0, n), |r| r.loop_range(n));
        let (legato, eighths, swing) = (p.legato, p.swing_eighths, p.swing);
        const KEYS: [&str; 12] = [
            "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
        ];
        let out = p.out_of_scale();
        vec![
            Self::field_row(
                "Pattern Length",
                dropdown(format!("{n} Steps"), 22.).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                        ws.menu = Some(ws.length_menu(e.position));
                        cx.notify();
                    }),
                ),
                true,
            ),
            Self::field_row(
                "Loop Start",
                self.stepper(Field::PatternLoopStart, format!("{}", a + 1), "", cx),
                true,
            ),
            Self::field_row(
                "Loop End",
                self.stepper(Field::PatternLoopEnd, format!("{b}"), "", cx),
                true,
            ),
            Self::field_row(
                "Step Rate",
                dropdown(rate_name(p.step), 22.).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                        let items = ws.rate_menu(None);
                        ws.open_steps_menu(e.position, items, cx);
                    }),
                ),
                true,
            ),
            Self::field_row(
                "Playback Mode",
                dropdown(p.mode.name(), 22.).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                        let items = ws.play_menu(None);
                        ws.open_steps_menu(e.position, items, cx);
                    }),
                ),
                true,
            ),
            Self::field_row(
                "Legato",
                Self::check(legato)
                    .debug_selector(|| "steps-legato".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.steps_op(StepsOp::Legato(!legato), cx)),
                    ),
                true,
            ),
            div().h(px(1.)).my(px(6.)).bg(rgb(LINE)),
            Self::field_row(
                "Swing",
                self.stepper(Field::Swing, format!("{swing:.0}"), "%", cx),
                true,
            ),
            Self::field_row(
                "",
                tray()
                    .p(px(2.))
                    .rounded(px(6.))
                    .child(seg("8th", eighths, 20.).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.steps_op(StepsOp::SwingEighths(true), cx)),
                    ))
                    .child(seg("16th", !eighths, 20.).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.steps_op(StepsOp::SwingEighths(false), cx)),
                    )),
                true,
            ),
            Self::field_row(
                "Key",
                dropdown(KEYS[p.key as usize % 12], 22.).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                        let items = ws.pattern_key_menu();
                        ws.open_steps_menu(e.position, items, cx);
                    }),
                ),
                true,
            ),
            Self::field_row(
                "Scale",
                div()
                    .flex()
                    .gap(px(4.))
                    .child(
                        dropdown(p.scale.map_or("Chromatic", |s| s.name()), 22.).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                let items = ws.pattern_scale_menu();
                                ws.open_steps_menu(e.position, items, cx);
                            }),
                        ),
                    )
                    .child(
                        button("Q", false)
                            .h(px(22.))
                            .when(!out, |d| d.opacity(0.4))
                            .when(out, |d| {
                                d.on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| ws.steps_op(StepsOp::Conform, cx)),
                                )
                            }),
                    ),
                true,
            ),
        ]
    }

    fn row_fields(&mut self, at: At, p: &Pattern, cx: &mut Context<Self>) -> Vec<Div> {
        let r = self.steps.row;
        let Some(row) = p.rows.get(r) else {
            return vec![div().text_color(rgb(TEXT_3)).child("No row")];
        };
        let n = p.steps();
        let (a, b) = row.loop_range(n);
        let looks = self.row_looks(at, p);
        let auto = row.is_automation();
        let slide = row.slide;
        let name_field = self.name_field(Name::PatternRow(at, r));
        let row_name = row.name.clone();
        let rate = row
            .rate
            .map_or_else(|| format!("{} (pattern)", rate_name(p.step)), rate_name);
        let play = row.mode.map_or_else(
            || format!("{} (pattern)", p.mode.name()),
            |m| m.name().into(),
        );
        vec![
            Self::field_row(
                "Row Assignment",
                dropdown(looks[r].name.clone(), 22.)
                    .max_w(px(130.))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            let items = ws.assign_menu(r, e.position);
                            ws.open_steps_menu(e.position, items, cx);
                        }),
                    ),
                true,
            ),
            Self::field_row(
                "Row Name",
                match name_field {
                    Some(input) => div().w(px(130.)).h(px(22.)).child(input),
                    None => div()
                        .w(px(130.))
                        .h(px(22.))
                        .px(px(6.))
                        .flex()
                        .items_center()
                        .rounded(px(5.))
                        .bg(rgb(FIELD))
                        .text_size(px(11.5))
                        .cursor_text()
                        .child(row_name)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, window: &mut Window, cx| {
                                ws.start_naming(Name::PatternRow(at, r), window, cx);
                            }),
                        ),
                },
                true,
            ),
            div().h(px(1.)).my(px(6.)).bg(rgb(LINE)),
            Self::field_row(
                "Loop Start",
                self.stepper(Field::RowLoopStart, format!("{}", a + 1), "", cx),
                true,
            ),
            Self::field_row(
                "Loop End",
                self.stepper(Field::RowLoopEnd, format!("{b}"), "", cx),
                true,
            ),
            Self::field_row(
                "Step Rate",
                dropdown(rate, 22.).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        let items = ws.rate_menu(Some(r));
                        ws.open_steps_menu(e.position, items, cx);
                    }),
                ),
                true,
            ),
            Self::field_row(
                "Playback Mode",
                dropdown(play, 22.).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        let items = ws.play_menu(Some(r));
                        ws.open_steps_menu(e.position, items, cx);
                    }),
                ),
                true,
            ),
            div().h(px(1.)).my(px(6.)).bg(rgb(LINE)),
            Self::field_row(
                "Automation Mode",
                dropdown(if slide { "Slide" } else { "Latch" }, 22.).when(auto, |d| {
                    d.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            let items = vec![
                                (
                                    format!("{}Latch", if slide { "" } else { "✓ " }),
                                    MenuAction::Steps(StepsOp::Slide(r, false)),
                                ),
                                (
                                    format!("{}Slide", if slide { "✓ " } else { "" }),
                                    MenuAction::Steps(StepsOp::Slide(r, true)),
                                ),
                            ];
                            ws.open_steps_menu(e.position, items, cx);
                        }),
                    )
                }),
                auto,
            ),
        ]
    }

    fn step_fields(&mut self, p: &Pattern, cx: &mut Context<Self>) -> Vec<Div> {
        let (r, i) = self.inspected_steps()[0];
        let Some(row) = p.rows.get(r) else {
            return vec![div().text_color(rgb(TEXT_3)).child("No step")];
        };
        let auto = row.is_automation();
        let count = self.inspected_steps().len();
        let on = row.on(i);
        let cond = row.condition_at(i);
        let (nth, of) = row.every_at(i);
        let tie = row.tie_at(i);
        let skip = row.skip_at(i);
        let recur = cond == Condition::Recurrence;
        let rate = row
            .rate_at(i)
            .map_or_else(|| format!("{} (row)", rate_name(p.row_rate(r))), rate_name);
        let key = self.session.project.song_key();
        let note = crate::editors::pianoroll::name_in(key, row.pitch_at(i));
        let val = |l: Lane| self.step_text(r, l, i);
        let picked = self.inspected_steps();
        let toggle_on = picked.clone();
        let mut out = vec![
            div()
                .pb(px(4.))
                .text_size(px(11.))
                .text_color(rgb(TEXT_3))
                .child(match count {
                    1 => format!("Step {}", i + 1),
                    n => format!("{n} steps"),
                }),
            Self::field_row(
                "On/Off",
                Self::check(on)
                    .debug_selector(|| "steps-step-on".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            let steps = toggle_on.clone();
                            ws.edit_pattern(cx, |p| {
                                for &(r, i) in &steps {
                                    if let Some(row) = p.rows.get_mut(r) {
                                        row.set_on(i, !on);
                                    }
                                }
                            });
                        }),
                    ),
                true,
            ),
            Self::field_row(
                "Chance",
                self.stepper(Field::Chance, format!("{}", row.chance_at(i)), "%", cx),
                true,
            ),
            Self::field_row(
                "Condition",
                dropdown(cond.name(), 22.).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        let items = Condition::ALL
                            .into_iter()
                            .map(|c| {
                                (
                                    format!("{}{}", if c == cond { "✓ " } else { "" }, c.name()),
                                    MenuAction::Steps(StepsOp::StepCondition(r, i, c)),
                                )
                            })
                            .collect();
                        ws.open_steps_menu(e.position, items, cx);
                    }),
                ),
                true,
            ),
            Self::field_row(
                "Recurrence Iteration",
                self.stepper(Field::Iteration, format!("{nth}"), "", cx),
                recur,
            ),
            Self::field_row(
                "Recurrence Cycles",
                self.stepper(Field::Cycles, format!("{of}"), "", cx),
                recur,
            ),
            Self::field_row(
                "Step Rate",
                dropdown(rate, 22.).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        let now = ws
                            .shown_pattern()
                            .and_then(|(_, p)| p.rows.get(r)?.rate_at(i));
                        let mut items = vec![(
                            format!("{}Row's", if now.is_none() { "✓ " } else { "" }),
                            MenuAction::Steps(StepsOp::StepRate(r, i, None)),
                        )];
                        items.extend(pina_engine::song::project::RATES.iter().map(|(name, t)| {
                            (
                                format!("{}{name}", if now == Some(*t) { "✓ " } else { "" }),
                                MenuAction::Steps(StepsOp::StepRate(r, i, Some(*t))),
                            )
                        }));
                        ws.open_steps_menu(e.position, items, cx);
                    }),
                ),
                true,
            ),
            Self::field_row(
                "Skip",
                Self::check(skip).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.edit_row(r, cx, |row, _| row.set(Lane::Skip, i, (!skip) as u8 as f32));
                    }),
                ),
                true,
            ),
            div().h(px(1.)).my(px(6.)).bg(rgb(LINE)),
        ];
        if !auto {
            out.push(Self::field_row(
                "Note",
                self.stepper(Field::Step(Lane::Note), note, "", cx),
                true,
            ));
        }
        out.push(Self::field_row(
            if auto { "Value" } else { "Velocity" },
            self.stepper(Field::Step(Lane::Velocity), val(Lane::Velocity), "", cx),
            true,
        ));
        if !auto {
            out.push(Self::field_row(
                "Gate",
                self.stepper(
                    Field::Step(Lane::Gate),
                    format!("{}", row.gate_at(i)),
                    "%",
                    cx,
                ),
                true,
            ));
        }
        out.push(Self::field_row(
            "Tie",
            Self::check(tie).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    ws.edit_row(r, cx, |row, _| row.set(Lane::Tie, i, (!tie) as u8 as f32));
                }),
            ),
            true,
        ));
        out.push(Self::field_row(
            "Offset",
            self.stepper(
                Field::Step(Lane::Offset),
                format!("{}", row.nudge_at(i)),
                "%",
                cx,
            ),
            true,
        ));
        if !auto {
            out.push(div().h(px(1.)).my(px(6.)).bg(rgb(LINE)));
            out.push(Self::field_row(
                "Repeat",
                self.stepper(
                    Field::Step(Lane::Repeat),
                    format!("{}", row.repeat_at(i)),
                    "",
                    cx,
                ),
                true,
            ));
            out.push(Self::field_row(
                "Velocity Ramp",
                self.stepper(Field::Ramp, format!("{}", row.ramp_at(i)), "%", cx),
                true,
            ));
            out.push(Self::field_row(
                "Velocity Curve",
                self.stepper(Field::Curve, format!("{}", row.curve_at(i)), "%", cx),
                true,
            ));
            out.push(Self::field_row(
                "Distribution",
                self.stepper(Field::Spread, format!("{}", row.spread_at(i)), "%", cx),
                true,
            ));
        }
        let _ = picked;
        out
    }
}
