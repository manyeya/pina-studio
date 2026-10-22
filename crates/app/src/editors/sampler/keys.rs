//! The zone switches and the keys under the zones.

use super::*;

impl Workspace {
    /// The zone's switches: how it ends, loops and which way it plays.
    pub(crate) fn zone_switches(
        &mut self,
        t: usize,
        i: usize,
        z: &Zone,
        cx: &mut Context<Self>,
    ) -> Div {
        let looping = z.loop_mode;
        let loops = [
            (LoopMode::Off, "No loop"),
            (LoopMode::Forward, "Loop"),
            (LoopMode::PingPong, "Ping-pong"),
        ]
        .map(|(m, label)| {
            seg(label, looping == m, 22.)
                .px(px(8.))
                .text_size(px(11.))
                .debug_selector(move || format!("loop-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.edit_sampler(t, cx, |s| {
                            if let Some(z) = s.zones.get_mut(i) {
                                z.loop_mode = m;
                            }
                        })
                    }),
                )
        });
        let switch =
            |label: &'static str, on: bool| seg(label, on, 22.).px(px(8.)).text_size(px(11.));
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(7.))
                    .child(
                        switch("One-shot", !z.gate)
                            .debug_selector(|| "zone-oneshot".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.edit_sampler(t, cx, |s| {
                                        if let Some(z) = s.zones.get_mut(i) {
                                            z.gate = false
                                        }
                                    })
                                }),
                            ),
                    )
                    .child(
                        switch("Gate", z.gate)
                            .debug_selector(|| "zone-gate".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.edit_sampler(t, cx, |s| {
                                        if let Some(z) = s.zones.get_mut(i) {
                                            z.gate = true
                                        }
                                    })
                                }),
                            ),
                    ),
            )
            .child(tray().p(px(2.)).rounded(px(7.)).children(loops))
            .child(
                tray().p(px(2.)).rounded(px(7.)).child(
                    switch("Reverse", z.reverse)
                        .debug_selector(|| "zone-reverse".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                ws.edit_sampler(t, cx, |s| {
                                    if let Some(z) = s.zones.get_mut(i) {
                                        z.reverse ^= true
                                    }
                                })
                            }),
                        ),
                ),
            )
            .child(
                tray().p(px(2.)).rounded(px(7.)).child(
                    switch(
                        match z.choke {
                            Some(g) => ["Choke 1", "Choke 2", "Choke 3", "Choke 4"]
                                [(g as usize).saturating_sub(1).min(3)],
                            None => "No choke",
                        },
                        z.choke.is_some(),
                    )
                    .debug_selector(|| "zone-choke".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.edit_sampler(t, cx, |s| {
                                if let Some(z) = s.zones.get_mut(i) {
                                    z.choke = match z.choke {
                                        None => Some(1),
                                        Some(g) if g < 4 => Some(g + 1),
                                        _ => None,
                                    };
                                }
                            })
                        }),
                    ),
                ),
            )
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
                        .h(px(8.))
                        .rounded(px(2.))
                        .when_some(c, |d, c| d.bg(alpha(c, if mine { 0.95 } else { 0.4 }))),
                )
                .child(
                    div()
                        .when(black, |d| d.h(px(20.)))
                        .when(!black, |d| d.flex_1())
                        .rounded_b(px(3.))
                        .bg(rgb(if black { 0x1c1c22 } else { 0xd9d9df }))
                        .when(root, |d| d.border_b_2().border_color(rgb(AMBER)))
                        .cursor_pointer()
                        .when(p % 12 == 0 && !black, |d| {
                            d.child(
                                div()
                                    .pt(px(10.))
                                    .flex()
                                    .justify_center()
                                    .text_size(px(8.5))
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
            .h(px(44.))
            .flex_none()
            .flex()
            .gap(px(1.))
            .debug_selector(|| "sampler-keys".into())
            .children(keys)
    }
}
