//! Row headers: the pattern's controls over them; each row's arrow, icon, assignment, mute,
//! solo, rate, playback mode, and its rotate, down and up, randomize and clear; each subrow's
//! setting and the same buttons for it alone.

use gpui::Window;
use pina_engine::song::project::rate_name;

use crate::workspace::Name;

use super::*;

/// A small grey button in a header, greyed out when it does nothing.
fn mini(i: Icon, enabled: bool) -> Div {
    div()
        .w(px(20.))
        .h(px(17.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .bg(rgb(RAISED))
        .when(enabled, |d| {
            d.cursor_pointer().hover(|s| s.bg(rgb(LINE_STRONG)))
        })
        .child(icon(i, 12.).text_color(rgb(if enabled { TEXT_2 } else { TEXT_4 })))
}

/// A pop-up in a header: its value and ⌃⌄.
fn popup(label: impl Into<gpui::SharedString>, w: f32, dim: bool) -> Div {
    div()
        .w(px(w))
        .h(px(17.))
        .flex_none()
        .pl(px(5.))
        .pr(px(2.))
        .flex()
        .items_center()
        .justify_between()
        .rounded(px(4.))
        .bg(rgb(RAISED))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(LINE_STRONG)))
        .font_family(MONO)
        .text_size(px(10.))
        .text_color(rgb(if dim { TEXT_3 } else { TEXT }))
        .child(label.into())
        .child(
            div()
                .flex()
                .flex_col()
                .child(icon(Icon::ChevronUp, 7.).text_color(rgb(TEXT_3)))
                .child(icon(Icon::Chevron, 7.).text_color(rgb(TEXT_3))),
        )
}

/// A playback mode's icon.
pub(crate) fn mode_icon(m: PlayMode) -> Icon {
    match m {
        PlayMode::Forward => Icon::ArrowRight,
        PlayMode::Backward => Icon::ArrowLeft,
        PlayMode::PingPong => Icon::PingPong,
        PlayMode::Pendulum => Icon::Pendulum,
        PlayMode::Wander => Icon::Wander,
        PlayMode::Shuffle => Icon::Shuffle,
        PlayMode::Random => Icon::Random,
    }
}

/// A playback mode pop-up: its icon and ⌃⌄.
fn mode_popup(m: PlayMode, dim: bool) -> Div {
    div()
        .w(px(34.))
        .h(px(17.))
        .flex_none()
        .pl(px(4.))
        .pr(px(2.))
        .flex()
        .items_center()
        .justify_between()
        .rounded(px(4.))
        .bg(rgb(RAISED))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(LINE_STRONG)))
        .child(icon(mode_icon(m), 12.).text_color(rgb(if dim { TEXT_3 } else { TEXT })))
        .child(
            div()
                .flex()
                .flex_col()
                .child(icon(Icon::ChevronUp, 7.).text_color(rgb(TEXT_3)))
                .child(icon(Icon::Chevron, 7.).text_color(rgb(TEXT_3))),
        )
}

/// A row's icon: what its sound is (a kick, a snare, hats), a note, or automation.
fn row_icon(look: &RowLook, melodic: bool) -> Icon {
    let n = look.name.to_lowercase();
    let has = |w: &[&str]| w.iter().any(|w| n.contains(w));
    match () {
        _ if look.automation => Icon::Curve,
        _ if melodic => Icon::Note,
        _ if has(&["kick", "bd", "808"]) => Icon::Kick,
        _ if has(&["snare", "sd", "rim", "stick"]) => Icon::Snare,
        _ if has(&["clap", "snap"]) => Icon::Clap,
        _ if has(&["hat", "hh"]) => Icon::HiHat,
        _ if has(&["tom"]) => Icon::Tom,
        _ if has(&["crash", "ride", "cymbal", "splash", "china"]) => Icon::Cymbal,
        _ if has(&[
            "perc", "shaker", "conga", "bongo", "tamb", "cow", "clave", "wood",
        ]) =>
        {
            Icon::Percussion
        }
        _ => Icon::Note,
    }
}

impl Workspace {
    /// The buttons a row, a subrow or the whole pattern has for its steps in a lane (or on/off):
    /// rotate, down and up, randomize, clear. `row` none is the pattern's.
    fn step_buttons(
        &self,
        row: Option<usize>,
        lane: Option<Lane>,
        clear: bool,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let tag = row.map_or("pattern".to_string(), |r| format!("{r}"));
        let rotate_ok = lane != Some(Lane::Tie);
        let rotate = |by: i32, i: Icon, name: &'static str, cx: &mut Context<Self>| {
            let tag = tag.clone();
            mini(i, rotate_ok)
                .debug_selector(move || format!("steps-{name}-{tag}"))
                .when(rotate_ok, |d| {
                    d.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            match row {
                                Some(r) => ws.steps_op(StepsOp::RotateRow(r, lane, by), cx),
                                None => ws.steps_op(StepsOp::RotatePattern(by), cx),
                            }
                        }),
                    )
                })
        };
        let notch = |up: bool, cx: &mut Context<Self>| {
            let tag = tag.clone();
            mini(if up { Icon::ChevronUp } else { Icon::Chevron }, true)
                .debug_selector(move || format!("steps-{}-{tag}", if up { "up" } else { "down" }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        match row {
                            Some(r) => ws.steps_op(StepsOp::StepRow(r, lane, up), cx),
                            None => ws.steps_op(StepsOp::StepPattern(up), cx),
                        }
                        // Held and dragged: on, a notch for each few pixels.
                        let y = f32::from(e.position.y);
                        ws.begin_drag(Drag::StepSeq(StepDrag::Notch { row, lane, y }), cx);
                    }),
                )
        };
        let dice = {
            let tag = tag.clone();
            mini(Icon::Dice, true)
                .debug_selector(move || format!("steps-randomize-{tag}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        if e.modifiers.control {
                            ws.steps_op(StepsOp::Open(StepsMenu::Randomize, e.position), cx);
                            return;
                        }
                        match row {
                            Some(r) => ws.steps_op(StepsOp::RandomizeRow(r, lane), cx),
                            None => ws.steps_op(StepsOp::RandomizePattern(None), cx),
                        }
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        ws.steps_op(StepsOp::Open(StepsMenu::Randomize, e.position), cx);
                    }),
                )
        };
        let wipe = {
            let tag = tag.clone();
            mini(Icon::ClearBox, clear)
                .debug_selector(move || format!("steps-clear-{tag}"))
                .when(clear, |d| {
                    d.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            match row {
                                Some(r) => ws.steps_op(StepsOp::ClearRowValues(r, lane), cx),
                                None => ws.steps_op(StepsOp::ClearPatternValues(None), cx),
                            }
                        }),
                    )
                })
        };
        vec![
            div()
                .flex()
                .gap(px(1.))
                .child(rotate(-1, Icon::RotateLeft, "rotate-left", cx))
                .child(rotate(1, Icon::RotateRight, "rotate-right", cx)),
            div()
                .flex()
                .gap(px(1.))
                .child(notch(false, cx))
                .child(notch(true, cx)),
            dice,
            wipe,
        ]
    }

    /// The pattern's controls over the row headers: add a row (or learn), its rate and playback
    /// mode, and the buttons for every row's steps.
    pub(crate) fn pattern_strip(&mut self, cx: &mut Context<Self>) -> Div {
        let Some((_, p)) = self.shown_pattern() else {
            return div();
        };
        let (step, mode) = (p.step, p.mode);
        let lane = self.steps.mode.lane();
        let clear = p
            .rows
            .iter()
            .any(|r| lane.is_none_or(|l| l.applies(r.is_automation())) && !r.lane_is_clear(lane));
        let add = match self.steps.learn {
            Some(l) => {
                let color = if l == Learn::Add { RECORD } else { AMBER };
                div()
                    .h(px(20.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .rounded(px(5.))
                    .bg(alpha(color, 0.25))
                    .border_1()
                    .border_color(rgb(color))
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(color))
                    .cursor_pointer()
                    .debug_selector(|| "steps-learn".into())
                    .child(icon(Icon::Learn, 11.).text_color(rgb(color)))
                    .child("Learn")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.steps_op(StepsOp::Learn(None), cx)),
                    )
            }
            None => div()
                .h(px(20.))
                .px(px(6.))
                .flex()
                .items_center()
                .gap(px(3.))
                .rounded(px(5.))
                .bg(rgb(RAISED))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(LINE_STRONG)))
                .debug_selector(|| "steps-add-row".into())
                .child(icon(Icon::Plus, 13.).text_color(rgb(TEXT)))
                .child(icon(Icon::Chevron, 10.).text_color(rgb(TEXT_3)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                        let items = ws.add_row_menu(e.position);
                        ws.open_steps_menu(e.position, items, cx);
                    }),
                ),
        };
        let buttons = self.step_buttons(None, lane, clear, cx);
        div()
            .w(px(HEADER_W))
            .flex_none()
            .h_full()
            .flex()
            .items_center()
            .gap(px(5.))
            .px(px(8.))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(add)
            .child(div().flex_1())
            .child(
                popup(rate_name(step), 46., false)
                    .debug_selector(|| "steps-pattern-rate".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            let items = ws.rate_menu(None);
                            ws.open_steps_menu(e.position, items, cx);
                        }),
                    ),
            )
            .child(
                mode_popup(mode, false)
                    .debug_selector(|| "steps-pattern-play".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            let items = ws.play_menu(None);
                            ws.open_steps_menu(e.position, items, cx);
                        }),
                    ),
            )
            .children(buttons)
    }

    /// A row's header.
    pub(crate) fn row_header(
        &mut self,
        at: At,
        r: usize,
        look: &RowLook,
        h: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let Some((_, p)) = self.shown_pattern() else {
            return div();
        };
        let row = &p.rows[r];
        let (open, mute, solo, melodic) = (row.open, row.mute, row.solo, row.is_melodic());
        let rate = row.rate.unwrap_or(p.step);
        let own_rate = row.rate.is_some();
        let play = row.mode.unwrap_or(p.mode);
        let own_play = row.mode.is_some();
        let picked = r == self.steps.row;
        let lane = self.steps.mode.lane();
        let lane_here = lane.filter(|l| l.applies(look.automation));
        let applies = lane.is_none() || lane_here.is_some();
        let clear = applies && !row.lane_is_clear(lane);
        let mode_label = match self.steps.mode {
            EditMode::Lane(l) => lane_name(l, Some(look.automation)),
            m => m.name(),
        };
        let name_field = self.name_field(Name::PatternRow(at, r));
        let color = look.color;
        let name = look.name.clone();
        let tall = h >= 40.0;
        let arrow = div()
            .w(px(14.))
            .flex_none()
            .flex()
            .justify_center()
            .cursor_pointer()
            .debug_selector(move || format!("steps-open-{r}"))
            .child(
                icon(
                    if open {
                        Icon::Chevron
                    } else {
                        Icon::ChevronRight
                    },
                    12.,
                )
                .text_color(rgb(TEXT_3)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let all = e.modifiers.alt;
                    ws.edit_pattern(cx, |p| {
                        let to = !p.rows.get(r).is_some_and(|row| row.open);
                        for (k, row) in p.rows.iter_mut().enumerate() {
                            if all || k == r {
                                row.open = to;
                                if to && row.subrows.is_empty() {
                                    row.subrows = default_subrows(row);
                                }
                            }
                        }
                    });
                }),
            );
        let glyph = div()
            .size(px(24.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(12.))
            .bg(alpha(color, 0.16))
            .cursor_pointer()
            .debug_selector(move || format!("steps-icon-{r}"))
            .child(icon(row_icon(look, melodic), 15.).text_color(rgb(color)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.steps.row = r;
                    let out = ws.steps.midi_out;
                    ws.steps.midi_out = true;
                    ws.audition(r, None);
                    ws.steps.midi_out = out;
                    cx.notify();
                }),
            );
        let title: gpui::AnyElement = match name_field {
            Some(input) => div()
                .flex_1()
                .min_w(px(0.))
                .h(px(20.))
                .child(input)
                .into_any_element(),
            None => div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .items_center()
                .gap(px(3.))
                .cursor_pointer()
                .debug_selector(move || format!("steps-row-{r}"))
                .child(
                    div()
                        .min_w(px(0.))
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(if mute { TEXT_4 } else { TEXT }))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(name),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .flex_col()
                        .child(icon(Icon::ChevronUp, 7.).text_color(rgb(TEXT_3)))
                        .child(icon(Icon::Chevron, 7.).text_color(rgb(TEXT_3))),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, window: &mut Window, cx| {
                        cx.stop_propagation();
                        ws.steps.row = r;
                        if e.click_count == 2 {
                            ws.start_naming(Name::PatternRow(at, r), window, cx);
                            return;
                        }
                        let items = ws.assign_menu(r, e.position);
                        ws.open_steps_menu(e.position, items, cx);
                    }),
                )
                .into_any_element(),
        };
        let line1 = div()
            .flex()
            .items_center()
            .gap(px(5.))
            .child(arrow)
            .child(glyph)
            .child(title)
            .child(
                div()
                    .flex_none()
                    .text_size(px(10.))
                    .text_color(rgb(if applies { TEXT_4 } else { 0x3a3a42 }))
                    .child(mode_label),
            );
        let ms = |label: &'static str, on: bool, bg: u32, fg: u32, cx: &mut Context<Self>| {
            toggle(label, on, bg, fg, 17.)
                .debug_selector(move || format!("steps-{}-{r}", label.to_lowercase()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        match label {
                            "M" => ws.steps_op(StepsOp::Mute(r), cx),
                            _ => ws.steps_op(StepsOp::Solo(r), cx),
                        }
                    }),
                )
        };
        let mut line2 = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .pl(px(19.))
            .child(ms("M", mute, ACCENT, 0xffffff, cx))
            .child(ms("S", solo, SOLO, 0x16161a, cx))
            .child(
                popup(rate_name(rate), 46., !own_rate)
                    .debug_selector(move || format!("steps-rate-{r}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let items = ws.rate_menu(Some(r));
                            ws.open_steps_menu(e.position, items, cx);
                        }),
                    ),
            )
            .child(
                mode_popup(play, !own_play)
                    .debug_selector(move || format!("steps-play-{r}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let items = ws.play_menu(Some(r));
                            ws.open_steps_menu(e.position, items, cx);
                        }),
                    ),
            );
        if applies {
            line2 = line2.children(self.step_buttons(Some(r), lane_here, clear, cx));
        }
        div()
            .w(px(HEADER_W))
            .h(px(h))
            .flex_none()
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(4.))
            .px(px(6.))
            .bg(rgb(if picked { HEADER_SELECTED } else { HEADERS }))
            .border_r_1()
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .child(line1)
            .when(tall, |d| d.child(line2))
            .debug_selector(move || format!("steps-header-{r}"))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    // Picked, and dragged up or down to move it.
                    ws.steps.row = r;
                    ws.begin_drag(Drag::StepSeq(StepDrag::Row { from: r }), cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    ws.steps.row = r;
                    let items = ws.row_menu(r, e.position);
                    ws.open_steps_menu(e.position, items, cx);
                }),
            )
    }

    /// A subrow's header: its setting (a pop-up), and the buttons for that setting alone; on
    /// hover, a subrow to remove or add.
    pub(crate) fn subrow_header(
        &mut self,
        r: usize,
        k: usize,
        lane: Lane,
        automation: bool,
        h: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let clear = self
            .shown_pattern()
            .and_then(|(_, p)| p.rows.get(r))
            .is_some_and(|row| !row.lane_is_clear(Some(lane)));
        let buttons = self.step_buttons(Some(r), Some(lane), clear, cx);
        let group: gpui::SharedString = format!("subrow-{r}-{k}").into();
        let edge = |i: Icon, name: &'static str, cx: &mut Context<Self>| {
            div()
                .size(px(14.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.))
                .bg(rgb(RAISED))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(LINE_STRONG)))
                .debug_selector(move || format!("steps-{name}-{r}-{k}"))
                .child(icon(i, 10.).text_color(rgb(TEXT_2)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        match name {
                            "subrow-remove" => ws.steps_op(StepsOp::DeleteSubrow(r, k), cx),
                            _ => ws.steps_op(StepsOp::AddSubrow(r, k), cx),
                        }
                    }),
                )
        };
        div()
            .group(group.clone())
            .relative()
            .w(px(HEADER_W))
            .h(px(h))
            .flex_none()
            .flex()
            .flex_col()
            .justify_center()
            .items_end()
            .gap(px(4.))
            .pl(px(36.))
            .pr(px(6.))
            .bg(rgb(HEADERS))
            .border_r_1()
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .child(
                div()
                    .absolute()
                    .left(px(8.))
                    .top_0()
                    .bottom_0()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap(px(3.))
                    .invisible()
                    .group_hover(group, |s| s.visible())
                    .child(edge(Icon::Close, "subrow-remove", cx))
                    .child(edge(Icon::Plus, "subrow-add", cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .cursor_pointer()
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(TEXT_2))
                    .debug_selector(move || format!("steps-subrow-{r}-{k}"))
                    .child(lane_name(lane, Some(automation)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(icon(Icon::ChevronUp, 7.).text_color(rgb(TEXT_3)))
                            .child(icon(Icon::Chevron, 7.).text_color(rgb(TEXT_3))),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let items = ws.subrow_menu(r, k);
                            ws.open_steps_menu(e.position, items, cx);
                        }),
                    ),
            )
            .when(h >= 40.0, |d| {
                d.child(div().flex().items_center().gap(px(4.)).children(buttons))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    ws.steps.row = r;
                    cx.notify();
                }),
            )
    }
}
