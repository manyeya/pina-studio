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

    /// A stack's own row, above its tracks, built as a track's header is: fold them away, its
    /// name and how many it has, and a strip of its mute, solo (its tracks'), meter and level, with
    /// its fader and pan once it's tall enough. Its bottom edge drags it taller; a double-click
    /// there is its usual size. A click picks the stack: its tracks, and its channel in the
    /// inspector and mixer; a right-click, its menu; a double-click renames it.
    pub(crate) fn stack_row(&self, a: usize, first: usize, cx: &mut Context<Self>) -> Div {
        let p = &self.session.project;
        let aux = &p.auxes[a];
        let (bus, folded, mute, volume, pan) =
            (aux.bus, aux.folded, aux.mute, aux.volume_db, aux.pan);
        let members: Vec<usize> = (0..p.tracks.len())
            .filter(|&i| p.tracks[i].output == Some(bus))
            .collect();
        let solo = members.iter().all(|&i| p.tracks[i].solo);
        let color = self.color(first);
        let selected = self.channel == Channel::Aux(a);
        let height = self.stack_h(a);
        let top = self.row_top(first) - height;
        let shown = &self.settings;
        let roomy = height >= 64.0 && shown.header_strip;
        let tracks = members.len();
        let count = match tracks {
            1 => "1 track".to_string(),
            n => format!("{n} tracks"),
        };
        let readout = match volume {
            v if v <= strip::SILENT + 0.01 => "−∞".to_string(),
            v => format!("{v:+.1}").replace('-', "−"),
        };
        let pos = strip::fader_pos(volume);
        let level = self.levels.get(&Some(aux.id)).copied().unwrap_or_default();
        let buttons = div()
            .flex()
            .gap(px(4.))
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
            .child(
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
                    ),
            );
        // Its level: drag it up or down, double-click to type one, ⌥-click for 0 dB.
        let lcd = header_lcd(None, tint(color, 0.55))
            .cursor_ns_resize()
            .debug_selector(move || format!("stack-level-{a}"))
            .child(match self.name_field(Name::AuxVolume(a)) {
                Some(field) => div().w_full().px(px(2.)).child(field),
                None => div().child(readout),
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    if e.modifiers.alt {
                        ws.edit(cx, |ws| ws.set_volume(Channel::Aux(a), 0.0));
                        return;
                    }
                    if e.click_count == 2 {
                        ws.start_naming(Name::AuxVolume(a), window, cx);
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
            );
        let row = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(buttons)
            .when(shown.header_meter, |d| {
                d.child(level_leds(level.now, level.held))
            });
        let panel = match roomy {
            false => header_panel(false).child(row.child(lcd)),
            true => {
                let fader = header_fader(pos, color, mute, div())
                    .id(("stack-fader", a))
                    .debug_selector(move || format!("stack-fader-{a}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            if e.modifiers.alt {
                                ws.edit(cx, |ws| ws.set_volume(Channel::Aux(a), 0.0));
                                return;
                            }
                            if e.click_count == 2 {
                                ws.start_naming(Name::AuxVolume(a), window, cx);
                                return;
                            }
                            let x = f32::from(e.position.x);
                            let drag = Drag::HFader {
                                channel: Channel::Aux(a),
                                x,
                                pos,
                                width: FADER_W,
                            };
                            ws.begin_drag(drag, cx);
                        }),
                    )
                    .on_scroll_wheel(swipe(Name::AuxVolume(a), cx));
                let knob = div()
                    .id(("stack-pan", a))
                    .size(px(24.))
                    .flex_none()
                    .cursor_ns_resize()
                    .debug_selector(move || format!("stack-pan-{a}"))
                    .child(pan_dial(PanMode::Balance, pan, 1.0, color))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.modifiers.alt {
                                ws.edit(cx, |ws| ws.set_pan(Channel::Aux(a), 0.0));
                                return;
                            }
                            let (x, y) = (f32::from(e.position.x), f32::from(e.position.y));
                            let drag = Drag::Pan {
                                channel: Channel::Aux(a),
                                x,
                                y,
                                pan,
                            };
                            ws.begin_drag(drag, cx);
                        }),
                    );
                header_panel(true).child(row).child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(fader)
                        .child(lcd)
                        .child(knob),
                )
            }
        };
        let tile = if roomy { 36. } else { 26. };
        let identity = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(3.))
            .pr(px(8.))
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
            .when(shown.header_icons, |d| {
                d.child(
                    div()
                        .size(px(tile))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(tile / 4.0))
                        .bg(alpha(color, 0.14))
                        .border_1()
                        .border_color(alpha(color, 0.34))
                        .when(mute, |d| d.opacity(0.55))
                        .child(icon(Icon::Stack, tile * 0.52).text_color(rgb(color))),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .ml(px(4.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .when(mute, |d| d.opacity(0.55))
                    .child(match self.name_field(Name::Aux(a)) {
                        Some(field) => div().child(field),
                        None => div()
                            .text_size(px(13.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .line_height(px(17.))
                            .text_color(rgb(if selected { 0xffffff } else { 0xe6e6ea }))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(aux.name.clone()),
                    })
                    .when(height >= 44.0 && shown.header_kind, |d| {
                        d.child(
                            div()
                                .text_size(px(10.5))
                                .line_height(px(13.))
                                .text_color(rgb(TEXT_3))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(match folded {
                                    true => format!("{tracks} folded"),
                                    false => count,
                                }),
                        )
                    }),
            );
        div()
            .absolute()
            .top(px(top))
            .left_0()
            .w_full()
            .h(px(height))
            .flex()
            .items_center()
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
                        .top(px(height * 0.5 + 9.))
                        .bottom(px(-1.))
                        .w(px(2.))
                        .bg(alpha(color, 0.55)),
                )
            })
            .child(identity)
            .child(panel)
            // The bottom edge: drag it taller or shorter; double-click for its usual size.
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(px(-2.))
                    .h(px(6.))
                    .cursor_row_resize()
                    .hover(|d| d.bg(alpha(ACCENT, 0.35)))
                    .debug_selector(move || format!("stack-edge-{a}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.click_count == 2 {
                                ws.session.project.auxes[a].height = None;
                                ws.session.edited = true;
                                cx.notify();
                                return;
                            }
                            let y = f32::from(e.position.y);
                            ws.begin_drag(Drag::StackHeight { aux: a, y, height }, cx);
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
