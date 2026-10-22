//! A band's dynamics: threshold, ratio, timing, and what it listens to.

use super::*;

impl Workspace {
    /// A dynamic band's own settings, in a tray under the rest: its threshold (its own unless
    /// set; the level it's hearing shown), attack and release (their own unless turned), the
    /// whole band or spectral (its density and tilt); what it listens to (what it covers, or
    /// between two frequencies; or the sidechain, from a track picked); bypassed, or cleared.
    pub(crate) fn dynamics(&self, d: &Drawn, cx: &mut Context<Self>) -> Div {
        let id = d.id;
        let color = d.color;
        let dynamic = d.band.dynamic.unwrap_or_default();
        let level = match self.player.playing() {
            true => self.player.eq_tap().level(d.index),
            false => -120.0,
        };
        let auto = dynamic.threshold_db.is_none();
        let threshold = dynamic.threshold_db.unwrap_or(-30.0);
        let share = |db: f32| (db.clamp(-80.0, 0.0) + 80.0) / 80.0;
        let free = dynamic.trigger.is_some();
        let source = self
            .shown_eq()
            .and_then(|(_, eq)| eq.sidechain)
            .and_then(|id| self.session.project.tracks.iter().find(|t| t.id == id))
            .map(|t| t.name.clone());
        let toggle =
            |label: String, sel: &'static str, on: bool, op: EqOp, cx: &mut Context<Self>| {
                div()
                    .h(px(24.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(11.))
                    .whitespace_nowrap()
                    .when(on, |s| s.bg(alpha(color, 0.18)).text_color(rgb(color)))
                    .when(!on, |s| {
                        s.bg(rgb(FIELD))
                            .text_color(rgb(TEXT_3))
                            .hover(|s| s.bg(rgb(RAISED)).text_color(rgb(TEXT_2)))
                    })
                    .debug_selector(move || sel.into())
                    .child(label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.eq_op(op, cx);
                        }),
                    )
            };
        let caption = |s: &'static str| div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(s);
        // Threshold: what it hears, filling the bar; where it's set, a line (drag it).
        let thresh = div()
            .w(px(150.))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(caption("Threshold"))
                    .child(
                        div()
                            .h(px(16.))
                            .px(px(6.))
                            .flex()
                            .items_center()
                            .rounded(px(4.))
                            .cursor_pointer()
                            .text_size(px(10.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .when(auto, |s| s.bg(alpha(color, 0.18)).text_color(rgb(color)))
                            .when(!auto, |s| {
                                s.bg(rgb(FIELD))
                                    .text_color(rgb(TEXT_3))
                                    .hover(|s| s.text_color(rgb(TEXT_2)))
                            })
                            .debug_selector(|| "eq-auto-threshold".into())
                            .child("Auto")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.eq_op(EqOp::AutoThreshold, cx);
                                }),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(11.))
                            .text_color(rgb(if auto { TEXT_3 } else { TEXT }))
                            .child(if auto {
                                "own".to_string()
                            } else {
                                EqKnob::Threshold.text(threshold)
                            }),
                    ),
            )
            .child(
                div()
                    .relative()
                    .h(px(8.))
                    .rounded(px(4.))
                    .bg(rgb(0x24252c))
                    .cursor_ns_resize()
                    .debug_selector(|| "eq-threshold".into())
                    .child(
                        div()
                            .absolute()
                            .left(px(0.))
                            .top(px(0.))
                            .h(px(8.))
                            .w(gpui::relative(share(level)))
                            .rounded(px(4.))
                            .bg(alpha(color, 0.45)),
                    )
                    .when(!auto, |el| {
                        el.child(
                            div()
                                .absolute()
                                .left(gpui::relative(share(threshold)))
                                .top(px(-3.))
                                .w(px(2.))
                                .h(px(14.))
                                .rounded(px(1.))
                                .bg(rgb(TEXT)),
                        )
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.press_knob(id, EqKnob::Threshold, e, cx);
                        }),
                    ),
            );
        div()
            .mx(px(10.))
            .mb(px(10.))
            .p(px(10.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .rounded(px(10.))
            .bg(rgb(BG))
            .border_1()
            .border_color(rgb(LINE))
            .debug_selector(|| "eq-dynamics".into())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(thresh)
                    .child(self.pill_value(
                        id,
                        EqKnob::Attack,
                        "Attack",
                        dynamic.attack_ms.is_none(),
                        color,
                        cx,
                    ))
                    .child(self.pill_value(
                        id,
                        EqKnob::Release,
                        "Release",
                        dynamic.release_ms.is_none(),
                        color,
                        cx,
                    ))
                    .child(div().flex_1())
                    // The whole band moving, or only what stands out in it.
                    .child(
                        tray()
                            .child(
                                seg("Whole", !dynamic.spectral, 22.)
                                    .debug_selector(|| "eq-whole-band".into())
                                    .when(dynamic.spectral, |s| {
                                        s.on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|ws, _, _, cx| {
                                                cx.stop_propagation();
                                                ws.eq_op(EqOp::Spectral, cx);
                                            }),
                                        )
                                    }),
                            )
                            .child(
                                seg("Spectral", dynamic.spectral, 22.)
                                    .when(dynamic.spectral, |s| {
                                        s.bg(rgb(color)).text_color(rgb(BG))
                                    })
                                    .debug_selector(|| "eq-spectral".into())
                                    .when(!dynamic.spectral, |s| {
                                        s.on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|ws, _, _, cx| {
                                                cx.stop_propagation();
                                                ws.eq_op(EqOp::Spectral, cx);
                                            }),
                                        )
                                    }),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .when(dynamic.spectral, |el| {
                        el.child(self.pill_value(id, EqKnob::Density, "Density", false, color, cx))
                            .child(toggle(
                                "Tilt".into(),
                                "eq-spectral-tilt",
                                dynamic.tilt,
                                EqOp::SpectralTilt,
                                cx,
                            ))
                            .child(div().w(px(1.)).h(px(16.)).mx(px(4.)).bg(rgb(LINE_STRONG)))
                    })
                    .child(caption("Listens to"))
                    .child(toggle(
                        "Band".into(),
                        "eq-trigger-band",
                        !free,
                        EqOp::Trigger,
                        cx,
                    ))
                    .child(toggle(
                        "Free".into(),
                        "eq-trigger-free",
                        free,
                        EqOp::Trigger,
                        cx,
                    ))
                    .when(free, |el| {
                        el.child(self.small_value(id, EqKnob::Low, cx))
                            .child(self.small_value(id, EqKnob::High, cx))
                    })
                    .child(
                        toggle(
                            match &source {
                                Some(name) => format!("Ext · {name}"),
                                None => "Ext".into(),
                            },
                            "eq-external",
                            dynamic.external,
                            EqOp::External,
                            cx,
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.eq_sidechain_menu(e.position, cx);
                            }),
                        ),
                    )
                    .child(
                        div()
                            .size(px(24.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .cursor_pointer()
                            .bg(rgb(FIELD))
                            .hover(|s| s.bg(rgb(RAISED)))
                            .debug_selector(|| "eq-sidechain".into())
                            .child(icon(Icon::Chevron, 11.).text_color(rgb(TEXT_3)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.eq_sidechain_menu(e.position, cx);
                                }),
                            ),
                    )
                    .child(div().flex_1())
                    .child(toggle(
                        "Bypass".into(),
                        "eq-dyn-bypass",
                        dynamic.bypass,
                        EqOp::DynBypass,
                        cx,
                    ))
                    .child(toggle(
                        "Clear".into(),
                        "eq-dyn-clear",
                        false,
                        EqOp::DynClear,
                        cx,
                    )),
            )
    }

    /// The tracks the EQ's sidechain can be: what its dynamic bands listen to with Ext on.
    pub(crate) fn eq_sidechain_menu(
        &mut self,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some((ch, eq)) = self.shown_eq() else {
            return;
        };
        let own = match ch {
            crate::mix::strip::Channel::Track(i) => {
                self.session.project.tracks.get(i).map(|t| t.id)
            }
            _ => None,
        };
        let tick = |on: bool| if on { "✓ " } else { "" };
        let mut items = vec![("Sidechain from".to_string(), MenuAction::Nothing)];
        items.push((
            format!("{}None", tick(eq.sidechain.is_none())),
            MenuAction::Eq(EqOp::Sidechain(None)),
        ));
        items.extend(
            self.session
                .project
                .tracks
                .iter()
                .filter(|t| Some(t.id) != own)
                .map(|t| {
                    (
                        format!("{}{}", tick(eq.sidechain == Some(t.id)), t.name),
                        MenuAction::Eq(EqOp::Sidechain(Some(t.id))),
                    )
                }),
        );
        self.eq_menu(at, items, cx);
    }
}
