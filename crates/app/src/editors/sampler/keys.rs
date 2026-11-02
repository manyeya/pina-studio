//! The zone's switches, and the keys under the zones.

use super::*;

impl Workspace {
    /// The zone's switches: how it ends, loops, which way it plays, its choke group, and when
    /// it plays (as its note starts, or as it's let go).
    pub(crate) fn zone_switches(
        &mut self,
        t: usize,
        i: usize,
        z: &Zone,
        cx: &mut Context<Self>,
    ) -> Div {
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
            pill(label, on, TEXT)
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
            (LoopMode::Off, "No loop"),
            (LoopMode::Forward, "Loop"),
            (LoopMode::PingPong, "Ping-pong"),
        ]
        .map(|(m, label)| {
            pill(label, looping == m, AMBER)
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
        let choke = match z.choke {
            Some(g) => {
                ["Choke 1", "Choke 2", "Choke 3", "Choke 4"][(g as usize).saturating_sub(1).min(3)]
            }
            None => "No choke",
        };
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .child(
                tray()
                    .p(px(2.))
                    .gap(px(1.))
                    .rounded(px(8.))
                    .child(switch(
                        "One-shot",
                        !z.gate,
                        "zone-oneshot",
                        |z| z.gate = false,
                        cx,
                    ))
                    .child(switch("Gate", z.gate, "zone-gate", |z| z.gate = true, cx)),
            )
            .child(tray().p(px(2.)).gap(px(1.)).rounded(px(8.)).children(loops))
            .child(
                tray()
                    .p(px(2.))
                    .gap(px(1.))
                    .rounded(px(8.))
                    .child(switch(
                        "Reverse",
                        z.reverse,
                        "zone-reverse",
                        |z| z.reverse ^= true,
                        cx,
                    ))
                    .child(switch(
                        "On release",
                        z.trigger == Trigger::Release,
                        "zone-release",
                        |z| {
                            z.trigger = match z.trigger {
                                Trigger::Attack => Trigger::Release,
                                Trigger::Release => Trigger::Attack,
                            }
                        },
                        cx,
                    ))
                    .child(switch(
                        choke,
                        z.choke.is_some(),
                        "zone-choke",
                        |z| {
                            z.choke = match z.choke {
                                None => Some(1),
                                Some(g) if g < 4 => Some(g + 1),
                                _ => None,
                            };
                        },
                        cx,
                    )),
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
