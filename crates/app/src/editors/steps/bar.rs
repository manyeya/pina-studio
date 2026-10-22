//! The step sequencer's menu bar: Edit, Functions and View; the pattern browser and inspector;
//! MIDI in, MIDI out and mono; the edit mode; live recording, preview, and the zoom.

use super::*;

/// A menu's name in the bar, that opens it.
fn menu_name(label: &'static str) -> Div {
    div()
        .h(px(24.))
        .pl(px(9.))
        .pr(px(6.))
        .flex()
        .items_center()
        .gap(px(4.))
        .rounded(px(6.))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(TEXT_2))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(OFF)).text_color(rgb(TEXT)))
        .child(label)
        .child(icon(Icon::Chevron, 11.).text_color(rgb(TEXT_3)))
}

/// An icon button lit in `color` when on.
pub(crate) fn lit(i: Icon, on: bool, color: u32, h: f32) -> Div {
    div()
        .size(px(h))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .cursor_pointer()
        .bg(if on {
            alpha(color, 0.22)
        } else {
            rgb(BG).into()
        })
        .when(!on, |d| d.hover(|s| s.bg(rgb(OFF))))
        .child(icon(i, 15.).text_color(rgb(if on { color } else { TEXT_3 })))
}

impl Workspace {
    pub(crate) fn steps_bar(&mut self, cx: &mut Context<Self>) -> Div {
        let v = &self.steps;
        let menu = |label: &'static str, name: &'static str, cx: &mut Context<Self>| {
            menu_name(label)
                .debug_selector(move || format!("steps-menu-{name}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        let items = match name {
                            "edit" => ws.steps_edit_menu(),
                            "functions" => ws.steps_functions_menu(),
                            _ => ws.steps_view_menu(),
                        };
                        ws.open_steps_menu(e.position, items, cx);
                    }),
                )
        };
        let pane = |p: Pane, i: Icon, name: &'static str, on: bool, cx: &mut Context<Self>| {
            icon_seg(i, on, 24.)
                .debug_selector(move || format!("steps-{name}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        let now = ws.steps.pane;
                        ws.steps_op(StepsOp::Pane((now != Some(p)).then_some(p)), cx);
                    }),
                )
        };
        let flag = |i: Icon, on: bool, color: u32, name: &'static str, cx: &mut Context<Self>| {
            lit(i, on, color, 24.)
                .debug_selector(move || format!("steps-{name}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        match name {
                            "midi-in" => {
                                ws.steps.midi_in ^= true;
                                if ws.steps.midi_in && ws.steps.cursor.is_none() {
                                    ws.steps.cursor = Some((ws.steps.row, 0));
                                }
                            }
                            "midi-out" => ws.steps.midi_out ^= true,
                            "mono" => ws.steps.mono ^= true,
                            "live" => ws.steps.live ^= true,
                            "fit" => ws.steps.fit ^= true,
                            "preview" => ws.toggle_preview(cx),
                            _ => {}
                        }
                        cx.notify();
                    }),
                )
        };
        // The edit mode: On/Off, beside the last value mode picked and its menu.
        let on_off = v.mode == EditMode::OnOff;
        let mode = div()
            .h(px(26.))
            .flex()
            .rounded(px(7.))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(BG))
            .overflow_hidden()
            .child(
                div()
                    .w(px(108.))
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .bg(rgb(if on_off { ACCENT } else { BG }))
                    .text_color(rgb(if on_off { 0xffffff } else { TEXT_2 }))
                    .debug_selector(|| "steps-mode-onoff".into())
                    .child("On/Off")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.steps_op(StepsOp::Mode(EditMode::OnOff), cx)),
                    ),
            )
            .child(
                div()
                    .w(px(150.))
                    .h_full()
                    .flex()
                    .items_center()
                    .border_l_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(if on_off { BG } else { ACCENT }))
                    .text_color(rgb(if on_off { TEXT_2 } else { 0xffffff }))
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .cursor_pointer()
                            .debug_selector(|| "steps-mode-value".into())
                            .child(v.value_mode.name())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    let m = ws.steps.value_mode;
                                    ws.steps_op(StepsOp::Mode(m), cx)
                                }),
                            ),
                    )
                    .child(
                        div()
                            .w(px(22.))
                            .h_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|s| s.bg(alpha(0xffffff, 0.08)))
                            .debug_selector(|| "steps-mode-menu".into())
                            .child(icon(Icon::ChevronUp, 9.).text_color(rgb(if on_off {
                                TEXT_3
                            } else {
                                0xffffff
                            })))
                            .child(icon(Icon::Chevron, 9.).text_color(rgb(if on_off {
                                TEXT_3
                            } else {
                                0xffffff
                            })))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                    let items = ws.edit_mode_menu();
                                    ws.open_steps_menu(e.position, items, cx);
                                }),
                            ),
                    ),
            );
        let previewing = self.steps.preview.is_some();
        let geom = self.steps.geom.clone();
        let zoom = (self.steps.row_h - ROW_MIN) / (ROW_MAX - ROW_MIN);
        let zoom_bar = div()
            .w(px(64.))
            .h(px(16.))
            .relative()
            .flex()
            .items_center()
            .cursor_pointer()
            .debug_selector(|| "steps-zoom".into())
            .child(
                gpui::canvas(
                    move |b: Bounds<Pixels>, _, _| geom.borrow_mut().zoom_bar = b,
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .child(div().w_full().h(px(3.)).rounded(px(2.)).bg(rgb(RAISED)))
            .child(
                div()
                    .absolute()
                    .left(gpui::relative(zoom.clamp(0.0, 1.0) * 0.85))
                    .size(px(11.))
                    .rounded(px(6.))
                    .bg(rgb(TEXT_2)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    ws.steps.fit = false;
                    ws.begin_drag(Drag::StepSeq(StepDrag::Zoom), cx);
                    ws.zoom_to(f32::from(e.position.x));
                    cx.notify();
                }),
            );
        let fit = self.steps.fit;
        let wide = self.steps.wide;
        let live = self.steps.live;
        let (midi_in, midi_out, mono) = (v.midi_in, v.midi_out, v.mono);
        let browser_on = v.pane == Some(Pane::Browser);
        let inspector_on = v.pane == Some(Pane::Inspector);
        div()
            .h(px(36.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(8.))
            .bg(rgb(SURFACE))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(8.))
                    .child(menu("Edit", "edit", cx))
                    .child(menu("Functions", "functions", cx))
                    .child(menu("View", "view", cx)),
            )
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(8.))
                    .child(pane(Pane::Browser, Icon::Grid, "browser", browser_on, cx))
                    .child(pane(
                        Pane::Inspector,
                        Icon::Settings,
                        "inspector",
                        inspector_on,
                        cx,
                    )),
            )
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(8.))
                    .child(flag(Icon::MidiIn, midi_in, AMBER, "midi-in", cx))
                    .child(flag(Icon::MidiOut, midi_out, MINT, "midi-out", cx)),
            )
            .child(tray().p(px(2.)).rounded(px(8.)).child(flag(
                Icon::Mono,
                mono,
                ACCENT_TEXT,
                "mono",
                cx,
            )))
            .child(mode)
            .child(div().flex_1())
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(8.))
                    .child(flag(Icon::LiveRecord, live, RECORD, "live", cx))
                    .child(flag(Icon::Speaker, previewing, MINT, "preview", cx)),
            )
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(8.))
                    .child(flag(Icon::AutoZoom, fit, ACCENT_TEXT, "fit", cx))
                    .child(div().px(px(6.)).child(zoom_bar)),
            )
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(8.))
                    .child(
                        icon_seg(Icon::StepNarrow, !wide, 24.)
                            .debug_selector(|| "steps-narrow".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    ws.steps.wide = false;
                                    ws.steps.page = 0;
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        icon_seg(Icon::StepWide, wide, 24.)
                            .debug_selector(|| "steps-wide".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    ws.steps.wide = true;
                                    cx.notify();
                                }),
                            ),
                    ),
            )
    }

    /// The row height from where the zoom bar is dragged.
    pub(crate) fn zoom_to(&mut self, x: f32) {
        let b = self.steps.geom.borrow().zoom_bar;
        let u = ((x - f32::from(b.origin.x)) / f32::from(b.size.width).max(1.0)).clamp(0.0, 1.0);
        self.steps.row_h = ROW_MIN + u * (ROW_MAX - ROW_MIN);
    }

    /// Preview: the pattern played round and round while the song's stopped; again, stopped.
    pub(crate) fn toggle_preview(&mut self, cx: &mut Context<Self>) {
        match self.steps.preview.take() {
            Some(was) => {
                if self.player.playing() {
                    self.stop(cx);
                }
                self.set_pattern_mode(was, cx);
            }
            None => {
                let Some(at) = self.pattern() else {
                    return;
                };
                self.region = Some(at);
                self.steps.preview = Some(self.pattern_mode);
                self.set_pattern_mode(true, cx);
                if !self.player.playing() {
                    self.play_stop(cx);
                }
            }
        }
    }
}
