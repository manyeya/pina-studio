//! Stacks: their own row above their tracks, picking one, and its menu.

use super::*;

impl Workspace {
    /// The colour of the stack track `i` plays into (its first track's), if it's in one.
    pub fn stack_color(&self, i: usize) -> Option<u32> {
        let bus = self.session.project.stack_of(i)?.bus;
        let first = self
            .session
            .project
            .tracks
            .iter()
            .position(|t| t.output == Some(bus))?;
        Some(self.color(first))
    }

    /// A stack's own row, above its tracks: fold them away, its name and how many it has, its
    /// mute, solo (its tracks') and level (drag it). A click picks the stack: its tracks, and its
    /// channel in the inspector and mixer; a right-click, its menu.
    pub(crate) fn stack_row(&self, a: usize, first: usize, cx: &mut Context<Self>) -> Div {
        let p = &self.session.project;
        let aux = &p.auxes[a];
        let (bus, folded, mute, volume) = (aux.bus, aux.folded, aux.mute, aux.volume_db);
        let members: Vec<usize> = (0..p.tracks.len())
            .filter(|&i| p.tracks[i].output == Some(bus))
            .collect();
        let solo = members.iter().all(|&i| p.tracks[i].solo);
        let color = self.color(first);
        let selected = self.channel == Channel::Aux(a);
        let top = self.row_top(first) - STACK_H;
        let count = match members.len() {
            1 => "1 track".to_string(),
            n => format!("{n} tracks"),
        };
        let readout = match volume {
            v if v <= strip::SILENT + 0.01 => "−∞".to_string(),
            v => format!("{v:+.1}").replace('-', "−"),
        };
        let pos = strip::fader_pos(volume);
        div()
            .absolute()
            .top(px(top))
            .left_0()
            .w_full()
            .h(px(STACK_H))
            .flex()
            .items_center()
            .gap(px(8.))
            .pl(px(8.))
            .pr(px(10.))
            .bg(rgb(if selected { HEADER_SELECTED } else { HEADERS }))
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .debug_selector(move || format!("stack-{a}"))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(3.))
                    .bg(alpha(color, if selected { 0.95 } else { 0.6 })),
            )
            // Down into its tracks, unless they're folded away.
            .when(!folded, |d| {
                d.child(
                    div()
                        .absolute()
                        .left(px(12.))
                        .top(px(STACK_H * 0.5 + 9.))
                        .bottom(px(-1.))
                        .w(px(2.))
                        .bg(alpha(color, 0.55)),
                )
            })
            .child(
                div()
                    .size(px(20.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(RAISED)))
                    .debug_selector(move || format!("stack-fold-{a}"))
                    .child(
                        icon(
                            if folded {
                                Icon::ChevronRight
                            } else {
                                Icon::Chevron
                            },
                            12.,
                        )
                        .text_color(rgb(TEXT_2)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.track_op(crate::workspace::context::TrackOp::Fold(bus), cx);
                        }),
                    ),
            )
            .child(
                div()
                    .size(px(24.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .bg(alpha(color, 0.16))
                    .border_1()
                    .border_color(alpha(color, 0.4))
                    .child(icon(Icon::Stack, 14.).text_color(rgb(color))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .when(mute, |d| d.opacity(0.55))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(aux.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .child(match folded {
                                true => format!("Stack · {count} folded"),
                                false => format!("Stack · {count}"),
                            }),
                    ),
            )
            .child(
                strip_button("M", mute, ACCENT, 0xffffff)
                    .debug_selector(move || format!("stack-mute-{a}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit(cx, |ws| ws.session.project.auxes[a].mute ^= true);
                        }),
                    ),
            )
            .child({
                let members = members.clone();
                strip_button("S", solo, SOLO, BG)
                    .debug_selector(move || format!("stack-solo-{a}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit(cx, |ws| {
                                for &m in &members {
                                    ws.session.project.tracks[m].solo = !solo;
                                }
                            });
                        }),
                    )
            })
            // Its level, an LCD over how far up its fader is: drag it up or down.
            .child(
                div()
                    .w(px(58.))
                    .h(px(20.))
                    .flex_none()
                    .relative()
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .justify_end()
                    .px(px(6.))
                    .rounded(px(4.))
                    .bg(rgb(0x0b0b0e))
                    .border_1()
                    .border_color(rgb(0x22222a))
                    .font_family(MONO)
                    .text_size(px(10.))
                    .text_color(rgb(TEXT_2))
                    .cursor_ns_resize()
                    .debug_selector(move || format!("stack-level-{a}"))
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .bottom_0()
                            .w(gpui::relative(pos))
                            .bg(alpha(color, 0.18)),
                    )
                    .child(readout)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.modifiers.alt {
                                ws.edit(cx, |ws| ws.session.project.auxes[a].volume_db = 0.0);
                                return;
                            }
                            ws.begin_drag(
                                Drag::Fader {
                                    channel: Channel::Aux(a),
                                    y: f32::from(e.position.y),
                                    pos,
                                    height: 160.,
                                },
                                cx,
                            );
                        }),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                    if e.click_count == 2 {
                        ws.start_naming(Name::Aux(a), window, cx);
                        return;
                    }
                    ws.pick_stack(a);
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.pick_stack(a);
                    ws.stack_menu(a, e.position, cx);
                }),
            )
    }

    /// A stack picked: its tracks, and its channel.
    pub fn pick_stack(&mut self, a: usize) {
        let p = &self.session.project;
        let Some(bus) = p.auxes.get(a).map(|x| x.bus) else {
            return;
        };
        let ids: Vec<u64> = p
            .tracks
            .iter()
            .filter(|t| t.output == Some(bus))
            .map(|t| t.id)
            .collect();
        if let Some(first) = p.tracks.iter().position(|t| t.output == Some(bus)) {
            self.track = first;
        }
        self.tracks_picked = ids;
        self.channel = Channel::Aux(a);
        self.region = None;
    }

    /// A stack's menu: fold its tracks away or show them, rename it, flatten it.
    pub(crate) fn stack_menu(&mut self, a: usize, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(aux) = self.session.project.auxes.get(a) else {
            return;
        };
        let op = |label: &str, o: crate::workspace::context::TrackOp| {
            (label.to_string(), MenuAction::Track(o))
        };
        let items = vec![
            match aux.folded {
                true => op(
                    "Show Its Tracks",
                    crate::workspace::context::TrackOp::Fold(aux.bus),
                ),
                false => op(
                    "Fold Its Tracks Away",
                    crate::workspace::context::TrackOp::Fold(aux.bus),
                ),
            },
            (
                format!("Rename {}…", aux.name),
                MenuAction::Rename(Name::Aux(a)),
            ),
            (String::new(), MenuAction::Nothing),
            op(
                "Flatten Stack",
                crate::workspace::context::TrackOp::Flatten(aux.bus),
            ),
        ];
        self.open_menu(at, items, cx);
    }
}
