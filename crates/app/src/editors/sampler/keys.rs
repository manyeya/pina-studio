//! The picked zone's bar (its trim, how it plays and loops, its choke), and the keys under
//! the zones.

use super::*;

impl Workspace {
    /// The picked zone's bar, under its sample, in its colour: where it's trimmed (its start and
    /// length, to drag) and which way it plays, whether it plays out or stops with its note,
    /// its loop and the loop's crossfade, whether it plays as its note starts or as it's let go,
    /// and its choke group.
    pub(crate) fn zone_bar(&mut self, t: usize, i: usize, z: &Zone, cx: &mut Context<Self>) -> Div {
        let color = zone_color(i);
        let set = move |ws: &mut Workspace, cx: &mut Context<Workspace>, f: fn(&mut Zone)| {
            ws.edit_sampler(t, cx, |s| {
                if let Some(z) = s.zones.get_mut(i) {
                    f(z);
                }
            });
        };
        let switch = |label: &'static str,
                      on: bool,
                      sel: &'static str,
                      f: fn(&mut Zone),
                      cx: &mut Context<Self>| {
            pill(label, on, color)
                .debug_selector(move || sel.into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        set(ws, cx, f);
                    }),
                )
        };
        let looping = z.loop_mode;
        let loops = [
            (LoopMode::Off, "Off"),
            (LoopMode::Forward, "Loop"),
            (LoopMode::PingPong, "Ping-pong"),
        ]
        .map(|(m, label)| {
            pill(label, looping == m, color)
                .debug_selector(move || format!("loop-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.edit_sampler(t, cx, |s| {
                            if let Some(z) = s.zones.get_mut(i) {
                                z.loop_mode = m;
                            }
                        })
                    }),
                )
        });
        let choke = dropdown(
            z.choke.map_or("None".to_string(), |g| format!("Group {g}")),
            28.,
        )
        .debug_selector(|| "zone-choke".into())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                let items = [None, Some(1), Some(2), Some(3), Some(4)]
                    .map(|group| {
                        let label = group.map_or("None".to_string(), |g| format!("Group {g}"));
                        let op = SamplerOp::Choke {
                            track: t,
                            zone: i,
                            group,
                        };
                        (label, MenuAction::Sampler(op))
                    })
                    .to_vec();
                ws.menu = Some(Menu {
                    at: e.position,
                    items,
                    ..Default::default()
                });
                cx.notify();
            }),
        );
        let tray_of = || tray().p(px(2.)).gap(px(1.)).rounded(px(8.));
        div()
            .flex()
            .flex_wrap()
            .items_start()
            .gap(px(12.))
            .child(captioned(
                "TRIM",
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(self.sampler_value(t, Field::Start(i), "Start", color, cx))
                    .child(self.sampler_value(t, Field::Span(i), "Length", color, cx))
                    .child(tray_of().child(switch(
                        "Reverse",
                        z.reverse,
                        "zone-reverse",
                        |z| z.reverse ^= true,
                        cx,
                    ))),
            ))
            .child(captioned(
                "PLAYS",
                tray_of()
                    .child(switch(
                        "One-shot",
                        !z.gate,
                        "zone-oneshot",
                        |z| z.gate = false,
                        cx,
                    ))
                    .child(switch("Gate", z.gate, "zone-gate", |z| z.gate = true, cx)),
            ))
            .child(captioned(
                "LOOP",
                div()
                    .flex()
                    .gap(px(4.))
                    .child(tray_of().children(loops))
                    .when(looping != LoopMode::Off, |d| {
                        d.child(self.sampler_value(t, Field::LoopFade(i), "Fade", color, cx))
                    }),
            ))
            .child(captioned(
                "TRIGGER",
                tray_of()
                    .child(switch(
                        "Note on",
                        z.trigger == Trigger::Attack,
                        "zone-attack",
                        |z| z.trigger = Trigger::Attack,
                        cx,
                    ))
                    .child(switch(
                        "Release",
                        z.trigger == Trigger::Release,
                        "zone-release",
                        |z| z.trigger = Trigger::Release,
                        cx,
                    )),
            ))
            .child(captioned("CHOKE", choke))
    }

    /// Four octaves and more of keys, the zones over the notes they play.
    pub(crate) fn sampler_keys(
        &mut self,
        t: usize,
        s: &Sampler,
        picked: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let (lo, hi) = {
            let used_lo = s.zones.iter().map(|z| z.note).min().unwrap_or(36);
            let used_hi = s
                .zones
                .iter()
                .map(|z| z.high.unwrap_or(z.note))
                .max()
                .unwrap_or(84);
            let lo = (used_lo.min(36) / 12) * 12;
            let hi = ((used_hi.max(lo + 47) / 12) * 12 + 11).min(127);
            (lo, hi)
        };
        let track = self.session.project.tracks[t].id;
        let keys = (lo..=hi).map(|p| {
            let black = matches!(p % 12, 1 | 3 | 6 | 8 | 10);
            let zone = s.zones.iter().enumerate().find(|(_, z)| z.covers(p));
            let c = zone.map(|(i, _)| zone_color(i));
            let mine = zone.is_some_and(|(i, _)| i == picked);
            let root = s
                .zones
                .get(picked)
                .is_some_and(|z| z.high.is_some() && z.root.unwrap_or(z.note) == p);
            div()
                .flex_1()
                .min_w(px(0.))
                .h_full()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .h(px(6.))
                        .rounded(px(2.))
                        .when_some(c, |d, c| d.bg(alpha(c, if mine { 0.95 } else { 0.4 }))),
                )
                .child(
                    div()
                        .when(black, |d| d.h(px(14.)))
                        .when(!black, |d| d.flex_1())
                        .rounded_b(px(3.))
                        .bg(rgb(if black { 0x1c1c22 } else { 0xd9d9df }))
                        .when(root, |d| d.border_b_2().border_color(rgb(AMBER)))
                        .cursor_pointer()
                        .when(p % 12 == 0 && !black, |d| {
                            d.child(
                                div()
                                    .pt(px(6.))
                                    .flex()
                                    .justify_center()
                                    .text_size(px(8.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0x6a6a74))
                                    .child(name_in(None, p)),
                            )
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                if let Some(i) = ws
                                    .sampler_of(t)
                                    .and_then(|s| s.zones.iter().position(|z| z.covers(p)))
                                {
                                    ws.sampler_ui.zone = i;
                                }
                                ws.player.preview(track, p, 100);
                                ws.begin_drag(Drag::Key { track, pitch: p }, cx);
                            }),
                        ),
                )
        });
        div()
            .h(px(32.))
            .flex_none()
            .flex()
            .gap(px(1.))
            .debug_selector(|| "sampler-keys".into())
            .children(keys)
    }
}
