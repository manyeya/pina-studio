//! The EQ window's bars: along the top, the channel (to switch to another's EQ), presets,
//! undo, A/B and full screen; along the bottom, the piano, the processing, the analyzer and
//! the output, whose settings open over it.

use gpui::{
    Context, Div, FontWeight, MouseButton, MouseDownEvent, Stateful, div, prelude::*, px, rgb,
};
use tempo_engine::project::Eq;

use super::{EqKnob, EqOp, FOOT_H, HEAD_H, Popup, factory, saved_presets};
use crate::{
    menu::MenuAction,
    strip::Channel,
    theme::*,
    workspace::{Drag, Workspace, pt},
};

/// A button in the window's bars: a label (and icon), lit when on.
fn bar_button(label: impl Into<gpui::SharedString>, on: bool, color: u32) -> Div {
    div()
        .h(px(26.))
        .px(px(9.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(6.))
        .cursor_pointer()
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .when(on, |d| d.bg(alpha(color, 0.18)).text_color(rgb(color)))
        .when(!on, |d| {
            d.bg(rgb(0x1c1d23))
                .text_color(rgb(TEXT_2))
                .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
        })
        .child(label.into())
}

fn icon_button(glyph: Icon, sel: &'static str) -> Div {
    div()
        .size(px(26.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(RAISED)))
        .debug_selector(move || sel.into())
        .child(icon(glyph, 13.).text_color(rgb(TEXT_3)))
}

/// A row of choices in a settings pop-up.
fn choices<T: Copy + PartialEq + 'static>(
    label: &'static str,
    options: &[(&'static str, T)],
    now: T,
    op: impl Fn(T) -> EqOp + Copy + 'static,
    cx: &mut Context<Workspace>,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(
            div()
                .w(px(74.))
                .text_size(px(11.))
                .text_color(rgb(TEXT_3))
                .child(label),
        )
        .child(
            tray()
                .rounded(px(7.))
                .children(options.iter().map(|&(name, v)| {
                    seg(name, v == now, 22.)
                        .debug_selector(move || {
                            format!("eq-{}-{}", label.to_lowercase(), name.to_lowercase())
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.eq_op(op(v), cx);
                            }),
                        )
                })),
        )
}

impl Workspace {
    /// Along the top: whose EQ (click for another's), presets, undo and redo, A/B, full screen
    /// and close. Drag it to move the window.
    pub(super) fn eq_head(&self, ch: Channel, eq: &Eq, cx: &mut Context<Self>) -> Stateful<Div> {
        let (name, color) = self.channel_look(ch);
        let view = self.eq_view.as_ref().expect("the window's open");
        let on_b = view.on_b;
        let full = view.restore.is_some();
        div()
            .id("eq-head")
            .h(px(HEAD_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .bg(rgb(0x14151a))
            .border_b_1()
            .border_color(rgb(LINE))
            .cursor(gpui::CursorStyle::OpenHand)
            .debug_selector(|| "eq-title".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    let Some(v) = ws.eq_view.as_mut() else { return };
                    v.focused = true;
                    let start = v.at;
                    ws.begin_drag(
                        Drag::EqWindow {
                            from: pt(e.position),
                            start,
                            resize: false,
                        },
                        cx,
                    );
                }),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::BOLD)
                    .child("Channel EQ"),
            )
            .child(
                div()
                    .h(px(26.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(6.))
                    .bg(rgb(0x1c1d23))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(RAISED)))
                    .debug_selector(|| "eq-channel".into())
                    .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(color)))
                    .child(
                        div()
                            .max_w(px(160.))
                            .text_size(px(11.5))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(name),
                    )
                    .child(icon(Icon::Chevron, 10.).text_color(rgb(TEXT_3)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.eq_channel_menu(e.position, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_4))
                    .child(format!(
                        "{} band{}",
                        eq.bands.len(),
                        if eq.bands.len() == 1 { "" } else { "s" }
                    )),
            )
            .child(div().flex_1())
            .child(
                bar_button("Presets", false, TEXT)
                    .debug_selector(|| "eq-presets".into())
                    .child(icon(Icon::Chevron, 10.).text_color(rgb(TEXT_3)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.eq_presets_menu(e.position, cx);
                        }),
                    ),
            )
            .child(icon_button(Icon::Back, "eq-undo").on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.undo(cx);
                }),
            ))
            .child(icon_button(Icon::Forward, "eq-redo").on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.redo(cx);
                }),
            ))
            .child(
                tray()
                    .rounded(px(7.))
                    .p(px(2.))
                    .child(
                        seg("A", !on_b, 22.)
                            .debug_selector(|| "eq-a".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    if on_b {
                                        ws.eq_op(EqOp::Ab, cx);
                                    }
                                }),
                            ),
                    )
                    .child(
                        seg("B", on_b, 22.)
                            .debug_selector(|| "eq-b".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    if !on_b {
                                        ws.eq_op(EqOp::Ab, cx);
                                    }
                                }),
                            ),
                    ),
            )
            .child(
                bar_button(if on_b { "B→A" } else { "A→B" }, false, TEXT)
                    .debug_selector(|| "eq-copy-ab".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.eq_op(EqOp::CopyAb, cx);
                        }),
                    ),
            )
            .child(div().w(px(1.)).h(px(18.)).bg(rgb(LINE_STRONG)))
            .child(
                icon_button(if full { Icon::Minus } else { Icon::Expand }, "eq-full")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.eq_op(EqOp::FullScreen, cx);
                        }),
                    ),
            )
            .child(icon_button(Icon::Close, "eq-close").on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.close_eq(cx);
                }),
            ))
    }

    /// Every channel, to show its EQ instead (one added if it has none).
    fn eq_channel_menu(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let p = &self.session.project;
        let now = self.eq_view.as_ref().map(|v| v.ch);
        let tick = |ch: Channel| if Some(ch) == now { "✓ " } else { "" };
        let has = |eq: &Option<Eq>| match eq {
            Some(e) => format!("  · {} bands", e.bands.len()),
            None => String::new(),
        };
        let mut items: Vec<(String, MenuAction)> = p
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let ch = Channel::Track(i);
                (
                    format!("{}{}{}", tick(ch), t.name, has(&t.eq)),
                    MenuAction::OpenEq(ch),
                )
            })
            .collect();
        items.extend(p.auxes.iter().enumerate().map(|(i, a)| {
            let ch = Channel::Aux(i);
            (
                format!("{}{}{}", tick(ch), a.name, has(&a.eq)),
                MenuAction::OpenEq(ch),
            )
        }));
        items.push((
            format!("{}Stereo Out{}", tick(Channel::Master), has(&p.master_eq)),
            MenuAction::OpenEq(Channel::Master),
        ));
        self.eq_menu(at, items, cx);
    }

    fn eq_presets_menu(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let mut items = vec![("tempo's".to_string(), MenuAction::Nothing)];
        items.extend(
            factory()
                .into_iter()
                .enumerate()
                .map(|(i, (name, _))| (name.to_string(), MenuAction::Eq(EqOp::Factory(i)))),
        );
        let saved = saved_presets();
        if !saved.is_empty() {
            items.push((String::new(), MenuAction::Nothing));
            items.push(("Yours".to_string(), MenuAction::Nothing));
            items.extend(
                saved
                    .into_iter()
                    .enumerate()
                    .map(|(i, (name, _))| (name, MenuAction::Eq(EqOp::Preset(i)))),
            );
        }
        items.push((String::new(), MenuAction::Nothing));
        items.push((
            "Save as Preset".to_string(),
            MenuAction::Eq(EqOp::SavePreset),
        ));
        self.eq_menu(at, items, cx);
    }

    /// Along the bottom: the piano, the processing, the analyzer, the output; and a corner to
    /// resize the window by.
    pub(super) fn eq_foot(&self, eq: &Eq, cx: &mut Context<Self>) -> Div {
        let view = self.eq_view.as_ref().expect("the window's open");
        let a = view.analysis;
        let piano = view.piano;
        let popup = view.popup;
        let analyzing = a.pre || a.post;
        let (sketching, listing, matching) = (view.sketching, view.list, view.matching.is_some());
        let tool =
            |label: &'static str, on: bool, sel: &'static str, op: EqOp, cx: &mut Context<Self>| {
                bar_button(label, on, ACCENT_TEXT)
                    .debug_selector(move || sel.into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.eq_op(op, cx);
                        }),
                    )
            };
        div()
            .h(px(FOOT_H))
            .flex_none()
            .relative()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(10.))
            .bg(rgb(0x14151a))
            .border_t_1()
            .border_color(rgb(LINE))
            .child(tool("Sketch", sketching, "eq-sketch", EqOp::Sketch, cx))
            .child(
                bar_button("Piano", piano, ACCENT_TEXT)
                    .debug_selector(|| "eq-piano".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.eq_op(EqOp::Piano, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .h(px(26.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .bg(rgb(0x1c1d23))
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT_3))
                    .child("Zero Latency"),
            )
            .child(
                bar_button(
                    format!("Character · {}", eq.character.name()),
                    eq.character != tempo_engine::project::Character::Clean,
                    AMBER,
                )
                .debug_selector(|| "eq-character".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let now = ws.shown_eq().map(|(_, e)| e.character);
                        let items = tempo_engine::project::Character::ALL
                            .iter()
                            .map(|&c| {
                                (
                                    format!(
                                        "{}{}",
                                        if Some(c) == now { "✓ " } else { "" },
                                        c.name()
                                    ),
                                    MenuAction::Eq(EqOp::Character(c)),
                                )
                            })
                            .collect();
                        ws.eq_menu(e.position, items, cx);
                    }),
                ),
            )
            .child(div().flex_1())
            .child(tool("Instances", listing, "eq-instances", EqOp::List, cx))
            .child(tool(
                "Match",
                matching,
                "eq-match-open",
                if matching {
                    EqOp::MatchCancel
                } else {
                    EqOp::Match(None)
                },
                cx,
            ))
            .child(div().flex_1())
            .child(
                bar_button(
                    match (a.pre, a.post, a.freeze) {
                        (_, _, true) => "Analyzer · frozen",
                        (true, true, _) => "Analyzer · Pre + Post",
                        (false, true, _) => "Analyzer · Post",
                        (true, false, _) => "Analyzer · Pre",
                        _ => "Analyzer · Off",
                    },
                    popup == Some(Popup::Analyzer) || (analyzing && a.freeze),
                    ACCENT_TEXT,
                )
                .debug_selector(|| "eq-analyzer".into())
                .when(
                    analyzing && !a.freeze && popup != Some(Popup::Analyzer),
                    |d| d.text_color(rgb(TEXT)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.eq_op(EqOp::Popup(Popup::Analyzer), cx);
                    }),
                ),
            )
            .child(div().w(px(1.)).h(px(18.)).bg(rgb(LINE_STRONG)))
            .child(
                bar_button(
                    if eq.bypass { "Bypassed" } else { "Bypass" },
                    eq.bypass,
                    RECORD,
                )
                .debug_selector(|| "eq-bypass".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.eq_op(EqOp::Bypass, cx);
                    }),
                ),
            )
            .child(
                bar_button(
                    format!("Output {}", EqKnob::Output.text(eq.output_db)),
                    popup == Some(Popup::Output),
                    ACCENT_TEXT,
                )
                .debug_selector(|| "eq-output".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.eq_op(EqOp::Popup(Popup::Output), cx);
                    }),
                ),
            )
            .child(
                div()
                    .absolute()
                    .right(px(0.))
                    .bottom(px(0.))
                    .size(px(14.))
                    .cursor(gpui::CursorStyle::ResizeUpLeftDownRight)
                    .debug_selector(|| "eq-resize".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let Some(v) = ws.eq_view.as_ref() else { return };
                            let start = v.size;
                            ws.begin_drag(
                                Drag::EqWindow {
                                    from: pt(e.position),
                                    start,
                                    resize: true,
                                },
                                cx,
                            );
                        }),
                    ),
            )
    }

    /// The analyzer's settings or the output's, over the bottom bar.
    pub(super) fn eq_popup(&self, eq: &Eq, cx: &mut Context<Self>) -> Option<Div> {
        let view = self.eq_view.as_ref()?;
        let popup = view.popup?;
        let a = view.analysis;
        let panel = div()
            .absolute()
            .bottom(px(FOOT_H + 6.0))
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .rounded(px(10.))
            .bg(rgb(0x18191f))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        Some(match popup {
            Popup::Analyzer => panel
                .right(px(180.))
                .debug_selector(|| "eq-analyzer-panel".into())
                .child(
                    div()
                        .flex()
                        .gap(px(6.))
                        .child(
                            bar_button("Pre", a.pre, ACCENT_TEXT)
                                .debug_selector(|| "eq-pre".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| ws.eq_op(EqOp::Pre, cx)),
                                ),
                        )
                        .child(
                            bar_button("Post", a.post, ACCENT_TEXT)
                                .debug_selector(|| "eq-post".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| ws.eq_op(EqOp::Post, cx)),
                                ),
                        )
                        .child(div().flex_1())
                        .child(
                            bar_button("Freeze", a.freeze, SKY)
                                .debug_selector(|| "eq-freeze".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| ws.eq_op(EqOp::Freeze, cx)),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            div()
                                .w(px(74.))
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_3))
                                .child("Compare with"),
                        )
                        .child(
                            dropdown(
                                view.ext
                                    .and_then(|id| {
                                        self.session.project.tracks.iter().find(|t| t.id == id)
                                    })
                                    .map_or("No other track".to_string(), |t| t.name.clone()),
                                24.,
                            )
                            .debug_selector(|| "eq-ext".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    let now = ws.eq_view.as_ref().and_then(|v| v.ext);
                                    let own = ws.eq_view.as_ref().and_then(|v| match v.ch {
                                        Channel::Track(i) => {
                                            ws.session.project.tracks.get(i).map(|t| t.id)
                                        }
                                        _ => None,
                                    });
                                    let tick = |on: bool| if on { "✓ " } else { "" };
                                    let mut items = vec![(
                                        format!("{}None", tick(now.is_none())),
                                        MenuAction::Eq(EqOp::Ext(None)),
                                    )];
                                    items.extend(
                                        ws.session
                                            .project
                                            .tracks
                                            .iter()
                                            .filter(|t| Some(t.id) != own)
                                            .map(|t| {
                                                (
                                                    format!(
                                                        "{}{}",
                                                        tick(now == Some(t.id)),
                                                        t.name
                                                    ),
                                                    MenuAction::Eq(EqOp::Ext(Some(t.id))),
                                                )
                                            }),
                                    );
                                    ws.eq_menu(e.position, items, cx);
                                }),
                            ),
                        )
                        .child(div().flex_1())
                        .child(
                            bar_button("Collisions", a.collisions, RECORD)
                                .debug_selector(|| "eq-collisions".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| ws.eq_op(EqOp::Collisions, cx)),
                                ),
                        )
                        .child(
                            bar_button("Grab", a.grab, ACCENT_TEXT)
                                .debug_selector(|| "eq-grab".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| ws.eq_op(EqOp::Grab, cx)),
                                ),
                        ),
                )
                .child(choices(
                    "Range",
                    &[("60 dB", 60u8), ("90 dB", 90), ("120 dB", 120)],
                    a.range,
                    EqOp::AnalyzerRange,
                    cx,
                ))
                .child(choices(
                    "Resolution",
                    &[("Low", 0u8), ("Medium", 1), ("High", 2), ("Maximum", 3)],
                    a.resolution,
                    EqOp::Resolution,
                    cx,
                ))
                .child(choices(
                    "Speed",
                    &[
                        ("Very slow", 0u8),
                        ("Slow", 1),
                        ("Medium", 2),
                        ("Fast", 3),
                        ("Very fast", 4),
                    ],
                    a.speed,
                    EqOp::Speed,
                    cx,
                ))
                .child(choices(
                    "Tilt",
                    &[
                        ("0", 0u8),
                        ("1.5", 1),
                        ("3", 2),
                        ("4.5", 3),
                        ("6 dB/oct", 4),
                    ],
                    (a.tilt / 1.5).round() as u8,
                    EqOp::Tilt,
                    cx,
                )),
            Popup::Output => panel
                .right(px(10.))
                .w(px(340.))
                .debug_selector(|| "eq-output-panel".into())
                .child(
                    div()
                        .flex()
                        .justify_around()
                        .child(self.panel_value(EqKnob::Output, eq.output_db, cx))
                        .child(self.panel_value(EqKnob::Pan, eq.pan, cx))
                        .child(self.panel_value(EqKnob::Scale, eq.scale, cx)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_3))
                                .child("Pan"),
                        )
                        .child(
                            tray()
                                .rounded(px(7.))
                                .child(
                                    seg("L / R", !eq.pan_ms, 22.)
                                        .debug_selector(|| "eq-pan-lr".into())
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|ws, _, _, cx| {
                                                ws.eq_op(EqOp::PanMs(false), cx)
                                            }),
                                        ),
                                )
                                .child(
                                    seg("M / S", eq.pan_ms, 22.)
                                        .debug_selector(|| "eq-pan-ms".into())
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|ws, _, _, cx| {
                                                ws.eq_op(EqOp::PanMs(true), cx)
                                            }),
                                        ),
                                ),
                        )
                        .child(div().flex_1())
                        .child(
                            bar_button("Ø Phase", eq.invert, SKY)
                                .debug_selector(|| "eq-invert".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| ws.eq_op(EqOp::Invert, cx)),
                                ),
                        )
                        .child(
                            bar_button("Auto Gain", eq.auto_gain, AMBER)
                                .debug_selector(|| "eq-auto-gain".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| ws.eq_op(EqOp::AutoGain, cx)),
                                ),
                        ),
                )
                .child(div().text_size(px(10.5)).text_color(rgb(TEXT_4)).child(
                    "Gain scale scales every band's gain: 0% flat, 200% twice as much. \
                             Auto gain makes up for how much louder or quieter the bands make it.",
                )),
        })
    }

    /// One of the EQ's own knobs (not a band's), in a pop-up.
    fn panel_value(&self, k: EqKnob, v: f32, cx: &mut Context<Self>) -> Div {
        let pos = k.pos(v);
        let shown = if k.bipolar() { pos * 2.0 - 1.0 } else { pos };
        let field = self.name_field(crate::workspace::Name::Eq(0, k));
        div()
            .w(px(84.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.))
            .child(
                div()
                    .text_size(px(9.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT_4))
                    .child(k.label()),
            )
            .child(
                div()
                    .cursor_ns_resize()
                    .debug_selector(move || {
                        format!("eq-knob-{}", k.label().to_lowercase().replace(' ', "-"))
                    })
                    .child(knob(shown, 48., ACCENT, k.bipolar()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.press_knob(0, k, e, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .h(px(16.))
                    .font_family(MONO)
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_2))
                    .map(|el| match field {
                        Some(input) => el.w(px(72.)).bg(rgb(FIELD)).child(input),
                        None => el.child(k.text(v)),
                    }),
            )
    }
}
