//! A channel strip, drawn.

use super::*;

pub fn strip(ws: &Workspace, ch: Channel, layout: Layout, cx: &mut Context<Workspace>) -> Div {
    let project = &ws.session.project;
    let mixer = matches!(layout, Layout::Mixer { .. });
    let rows = match layout {
        Layout::Mixer { rows, .. } => rows,
        Layout::Compact { .. } => [true; 3],
    };
    let (name, color) = ws.channel_look(ch);
    let (pan, mute, solo, id) = match ch {
        Channel::Track(i) => {
            let t = &project.tracks[i];
            let pan = ws.now_value(i, Param::Pan);
            (Some(pan), Some(ws.muted_now(i)), Some(t.solo), Some(t.id))
        }
        Channel::Aux(i) => {
            let a = &project.auxes[i];
            (Some(a.pan), Some(a.mute), None, Some(a.id))
        }
        Channel::Master => (None, None, None, None),
    };
    let volume = ws.volume(ch);
    let level = ws.levels.get(&id).copied().unwrap_or_default();
    let pos = fader_pos(volume);
    let selected = mixer
        && match ch {
            Channel::Track(t) => {
                ws.channel == ch || (ws.is_picked(t) && ws.tracks_picked.len() > 1)
            }
            _ => ws.channel == ch,
        };
    let slot_h = 22.;

    // The instrument, or what feeds the strip.
    let source = match ch {
        Channel::Track(i) => {
            let t = &project.tracks[i];
            let dot = Some(color);
            match &t.instrument {
                None => {
                    let input = t.input.unwrap_or(Input {
                        channel: 0,
                        stereo: true,
                    });
                    dotted(input.name(), true, 24., dot)
                        .debug_selector(move || format!("input-{i}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.menu = Some(ws.input_menu(i, e.position));
                                cx.notify();
                            }),
                        )
                }
                Some(Instrument::Synth) => plugin_slot(
                    "Sine Synth".into(),
                    true,
                    ch,
                    Which::Instrument,
                    24.,
                    dot,
                    cx,
                ),
                Some(Instrument::Sampler(_)) => {
                    plugin_slot("Sampler".into(), true, ch, Which::Instrument, 24., dot, cx)
                }
                Some(Instrument::Drums) => {
                    plugin_slot("Drum Kit".into(), true, ch, Which::Instrument, 24., dot, cx)
                }
                Some(Instrument::Plugin(r)) => {
                    plugin_slot(r.class.clone(), true, ch, Which::Instrument, 24., dot, cx)
                }
            }
        }
        Channel::Aux(i) => info(format!("Bus {}", project.auxes[i].bus), 24.),
        Channel::Master => info("Output 1–2", 24.),
    };

    // Effect slots: the built-ins it has, its plugins, and an empty one to add another.
    let (eq, comp) = ws.strip_dsp(ch);
    let mut effects = Vec::new();
    if eq.is_some() {
        effects.push(builtin_slot(
            "Channel EQ",
            ch,
            Builtin::Eq,
            slot_h,
            if eq.is_some_and(|e| e.bypass) {
                TEXT_4
            } else {
                color
            },
            cx,
        ));
    }
    if comp.is_some() {
        effects.push(builtin_slot(
            "Compressor",
            ch,
            Builtin::Comp,
            slot_h,
            if comp.is_some_and(|c| c.bypass) {
                TEXT_4
            } else {
                color
            },
            cx,
        ));
    }
    if let Some(sat) = ws.sat(ch) {
        let color = if sat.bypass { TEXT_4 } else { color };
        effects.push(builtin_slot(
            "Saturator",
            ch,
            Builtin::Sat,
            slot_h,
            color,
            cx,
        ));
    }
    let inserts = ws.inserts(ch);
    for (n, r) in inserts.iter().enumerate() {
        effects.push(plugin_slot(
            r.class.clone(),
            true,
            ch,
            Which::Insert(n),
            slot_h,
            Some(TEXT_4),
            cx,
        ));
    }
    {
        effects.push(plugin_slot(
            String::new(),
            false,
            ch,
            Which::Insert(inserts.len()),
            slot_h,
            None,
            cx,
        ));
    }
    let (fit_effects, fit_sends) = match layout {
        Layout::Mixer { .. } => (MIXER_INSERTS, MIXER_SENDS),
        Layout::Compact { effects, sends } => (effects, sends),
    };
    if mixer {
        effects.truncate(MIXER_INSERTS);
    }
    while effects.len() < fit_effects {
        effects.push(blank(slot_h));
    }

    // Sends, for tracks.
    let mut sends = Vec::new();
    if let Channel::Track(track) = ch {
        let list = &project.tracks[track].sends;
        for (index, s) in list.iter().enumerate() {
            let (level, pre) = (ws.now_value(track, Param::Send(s.bus)), s.pre);
            let send_bus = s.bus;
            let bus = project
                .auxes
                .iter()
                .find(|a| a.bus == s.bus)
                .map_or(format!("Bus {}", s.bus), |a| a.name.clone());
            // Before the fader: amber, its dot and its knob.
            let tone = if pre { AMBER } else { TEXT };
            sends.push(
                dotted(bus, true, slot_h, pre.then_some(AMBER))
                    .justify_between()
                    .pr(px(3.))
                    .cursor_ns_resize()
                    .child(ws.modded(
                        Some((track, pina_engine::dsp::modulate::ModTarget::Send(index))),
                        16.,
                        knob(fader_pos(level), 16., tone, false),
                    ))
                    .debug_selector(move || format!("send-{track}-{index}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.modifiers.alt {
                                ws.edit(cx, |ws| {
                                    ws.session.project.tracks[track].sends[index].level_db = 0.0;
                                    ws.moved_once(track, Param::Send(send_bus), 0.0);
                                });
                            } else {
                                let y = f32::from(e.position.y);
                                ws.begin_drag(
                                    Drag::Send {
                                        track,
                                        index,
                                        y,
                                        level,
                                    },
                                    cx,
                                );
                            }
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.menu = Some(ws.send_options(track, index, e.position));
                            cx.notify();
                        }),
                    ),
            );
        }
        if !mixer || sends.len() < MIXER_SENDS {
            sends.push(
                slot("Send", false, slot_h)
                    .justify_center()
                    .debug_selector(move || format!("add-send-{track}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.menu = Some(ws.send_menu(track, e.position));
                            cx.notify();
                        }),
                    ),
            );
        }
    }
    if mixer {
        sends.truncate(MIXER_SENDS);
    }
    while sends.len() < fit_sends {
        sends.push(blank(slot_h));
    }

    // Where it plays, in the mixer.
    let output = match ch {
        Channel::Track(track) => {
            let bus = project.tracks[track].output;
            let to = bus.and_then(|b| project.auxes.iter().find(|a| a.bus == b));
            let label = to.map_or("Stereo Out".to_string(), |a| a.name.clone());
            pill(label)
                .cursor_pointer()
                .hover(|s| s.border_color(rgb(LINE_STRONG)))
                .debug_selector(move || format!("mixer-output-{track}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        ws.menu = Some(ws.output_menu(track, e.position));
                        cx.notify();
                    }),
                )
        }
        Channel::Aux(_) => info("Stereo Out", 22.).justify_center().rounded(px(11.)),
        Channel::Master => info("Out 1–2", 22.).justify_center().rounded(px(11.)),
    };

    let pan_row = div()
        .flex_none()
        .h(px(if mixer { 36. } else { 28. }))
        .flex()
        .items_center()
        .justify_between()
        .px(px(2.))
        .child(match pan {
            Some(pan) => div()
                .cursor_ns_resize()
                .child(ws.modded(
                    match ch {
                        Channel::Track(t) => Some((t, pina_engine::dsp::modulate::ModTarget::Pan)),
                        _ => None,
                    },
                    if mixer { 32. } else { 26. },
                    knob(pan, if mixer { 32. } else { 26. }, TEXT, true),
                ))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        if e.modifiers.alt {
                            ws.edit(cx, |ws| {
                                ws.set_pan(ch, 0.0);
                                if let Channel::Track(i) = ch {
                                    ws.moved_once(i, Param::Pan, 0.0);
                                }
                            });
                        } else {
                            let y = f32::from(e.position.y);
                            ws.begin_drag(
                                Drag::Pan {
                                    channel: ch,
                                    x: f32::from(e.position.x),
                                    y,
                                    pan,
                                },
                                cx,
                            );
                        }
                    }),
                )
                .into_any_element(),
            None => div().into_any_element(),
        })
        .child(
            div()
                .font_family(MONO)
                .text_size(px(11.))
                .text_color(rgb(TEXT_3))
                .child(match pan {
                    Some(p) if p.abs() < 0.005 => "C".to_string(),
                    Some(p) => format!("{}{:.0}", if p < 0.0 { "L" } else { "R" }, p.abs() * 64.0),
                    None => String::new(),
                }),
        );

    let peak_db = 20.0 * level.held[0].max(level.held[1]).max(1e-9).log10();
    let readout = |text: String, color: u32| {
        div()
            .flex_1()
            .h(px(18.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .bg(rgb(WELL))
            .font_family(MONO)
            .text_size(px(10.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(color))
            .child(text)
    };
    let readouts = div()
        .flex_none()
        .flex()
        .gap(px(4.))
        .child(readout(
            db_text(volume),
            match ch {
                Channel::Track(i) => ws.auto_mark(i, Param::Volume).unwrap_or(TEXT_2),
                _ => TEXT_2,
            },
        ))
        .child(readout(
            db_text(if peak_db < -100.0 { SILENT } else { peak_db }),
            if peak_db > 0.0 {
                METER_HIGH
            } else if peak_db > -6.0 {
                METER_MID
            } else {
                TEXT_2
            },
        ));

    let fader_h = layout.fader_h();
    let (track_x, cap_w, scale_x, meter_x) = if mixer {
        (13.0, 30.0, 31.0, 54.0)
    } else {
        (38.0, 26.0, 0.0, 66.0)
    };
    let usable = fader_h - 16.0;
    let at = |p: f32| 8.0 + (1.0 - p) * usable;
    let labels = [6.0f32, 0.0, -6.0, -12.0, -24.0, -40.0].map(|db| {
        div()
            .absolute()
            .left(px(scale_x))
            .top(px(at(fader_pos(db)) - 5.0))
            .w(px(20.))
            .when(mixer, |d| d.flex().justify_center())
            .when(!mixer, |d| d.flex().justify_end())
            .font_family(MONO)
            .text_size(px(9.))
            .line_height(px(10.))
            .text_color(rgb(TEXT_3))
            .child(if db > 0.0 {
                format!("+{db:.0}")
            } else {
                format!("{:.0}", db.abs())
            })
    });
    let fader = div()
        .flex_none()
        .relative()
        .h(px(fader_h))
        .cursor_ns_resize()
        .debug_selector(move || format!("fader-{ch:?}"))
        .child(
            div()
                .absolute()
                .left(px(track_x))
                .top(px(8.))
                .w(px(4.))
                .h(px(usable))
                .rounded(px(2.))
                .bg(rgb(0x0a0a0d)),
        )
        .children(labels)
        .child(
            div()
                .absolute()
                .left(px(meter_x))
                .top(px(8.))
                .h(px(usable))
                .child(meter(
                    level.now,
                    level.held,
                    if mixer { 5. } else { 4. },
                    2.,
                )),
        )
        .child(
            div()
                .absolute()
                .left(px(track_x + 2.0 - cap_w / 2.0))
                .top(px(at(pos) - if mixer { 10. } else { 7. }))
                .w(px(cap_w))
                .h(px(if mixer { 20. } else { 14. }))
                .rounded(px(if mixer { 5. } else { 4. }))
                .bg(rgb(0x3a3a44))
                .border_1()
                .border_color(rgb(0x4e4e5a))
                .shadow_md()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(2.))
                .when(mixer, |d| {
                    d.child(div().w(px(18.)).h(px(1.)).bg(rgb(TEXT_3)))
                })
                .child(
                    div()
                        .w(px(if mixer { 18. } else { 16. }))
                        .h(px(1.))
                        .bg(rgb(TEXT)),
                )
                .when(mixer, |d| {
                    d.child(div().w(px(18.)).h(px(1.)).bg(rgb(TEXT_3)))
                }),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                if e.modifiers.alt {
                    ws.edit(cx, |ws| {
                        ws.set_volume(ch, 0.0);
                        if let Channel::Track(i) = ch {
                            ws.moved_once(i, Param::Volume, 0.0);
                        }
                    });
                } else {
                    let y = f32::from(e.position.y);
                    ws.begin_drag(
                        Drag::Fader {
                            channel: ch,
                            y,
                            pos,
                            height: usable,
                        },
                        cx,
                    );
                }
            }),
        );

    let buttons = div()
        .flex_none()
        .h(px(22.))
        .flex()
        .gap(px(3.))
        .when_some(mute, |d, on| {
            d.child(
                toggle("M", on, TEXT, BG, 22.)
                    .flex_1()
                    .debug_selector(move || format!("strip-mute-{ch:?}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit(cx, |ws| match ch {
                                Channel::Track(i) => {
                                    let m = !ws.muted_now(i);
                                    ws.session.project.tracks[i].mute = m;
                                    ws.moved_once(i, Param::Mute, m as u8 as f32);
                                }
                                Channel::Aux(i) => ws.session.project.auxes[i].mute ^= true,
                                Channel::Master => {}
                            });
                        }),
                    ),
            )
        })
        .when_some(
            solo.zip(if let Channel::Track(i) = ch {
                Some(i)
            } else {
                None
            }),
            |d, (on, i)| {
                d.child(toggle("S", on, SOLO, BG, 22.).flex_1().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.edit(cx, |ws| ws.session.project.tracks[i].solo ^= true);
                    }),
                ))
            },
        )
        .when(mixer, |d| {
            d.when_some(
                if let Channel::Track(i) = ch {
                    Some(i)
                } else {
                    None
                },
                |d, i| {
                    d.child(
                        toggle("R", ws.armed() == Some(i), RECORD, 0xffffff, 22.)
                            .flex_1()
                            .debug_selector(move || format!("strip-arm-{i}"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.toggle_arm(i);
                                    cx.notify();
                                }),
                            ),
                    )
                },
            )
        });

    let plate = div()
        .flex_none()
        .h(px(if mixer { 28. } else { 22. }))
        .px(px(4.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(if mixer { 7. } else { 6. }))
        .text_size(px(if mixer { 12. } else { 11. }))
        .font_weight(FontWeight::SEMIBOLD)
        .whitespace_nowrap()
        .overflow_hidden()
        .bg(if selected {
            rgb(color).into()
        } else {
            alpha(color, 0.14)
        })
        .text_color(rgb(if selected { BG } else { tint(color, 0.5) }));
    // Double-click a track's or bus's name to rename it.
    let naming = match ch {
        Channel::Track(i) => Some(Name::Track(i)),
        Channel::Aux(i) => Some(Name::Aux(i)),
        Channel::Master => None,
    };
    let plate = match naming.and_then(|n| ws.name_field(n)) {
        Some(field) => plate.child(field),
        None => plate.child(name).when_some(naming, |d, what| {
            d.debug_selector(move || format!("plate-{ch:?}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                        if e.click_count == 2 {
                            cx.stop_propagation();
                            ws.start_naming(what, window, cx);
                        }
                    }),
                )
        }),
    };

    // A track's automation mode: lit in its colour, click for the
    // others.
    let auto_button = match ch {
        Channel::Track(i) => {
            let mode = project.tracks[i].automation.mode;
            let c = crate::arrange::automation::mode_color(mode);
            let on = mode != pina_engine::song::project::AutoMode::Off;
            Some(
                div()
                    .flex_none()
                    .h(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(5.))
                    .rounded(px(5.))
                    .bg(alpha(c, if on { 0.12 } else { 0.05 }))
                    .border_1()
                    .border_color(alpha(c, if on { 0.35 } else { 0.15 }))
                    .text_size(px(10.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(c))
                    .cursor_pointer()
                    .hover(|d| d.bg(alpha(c, 0.2)))
                    .debug_selector(move || format!("strip-auto-{i}"))
                    .child(div().size(px(5.)).rounded(px(3.)).bg(rgb(c)))
                    .child(mode.name())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.mode_menu(i, e.position, cx);
                        }),
                    ),
            )
        }
        // Buses have none; the space keeps their faders level with the tracks'.
        _ => Some(div().flex_none().h(px(18.))),
    };

    let reduction = ws.reductions.get(&id).copied().unwrap_or(0.0);
    let header = mixer.then(|| {
        let (i, number) = match ch {
            Channel::Track(i) => (
                crate::ui::looks::glyph(ws.shown(i).look.icon, 14.),
                format!("{}", i + 1),
            ),
            Channel::Aux(i) => (icon(Icon::Bus, 14.), format!("B{}", i + 1)),
            Channel::Master => (icon(Icon::Output, 14.), String::new()),
        };
        div()
            .flex_none()
            .h(px(28.))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.))
                    .bg(alpha(color, 0.14))
                    .text_color(rgb(color))
                    .child(i.text_color(rgb(color))),
            )
            .when(reduction > 0.3, |d| {
                d.child(
                    div()
                        .font_family(MONO)
                        .text_size(px(9.5))
                        .text_color(rgb(METER_MID))
                        .child(format!("GR {:.0}", reduction)),
                )
            })
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(number),
            )
    });

    div()
        .debug_selector(move || format!("strip-{ch:?}"))
        .when(mixer, |d| d.w(px(90.)).flex_none())
        .when(!mixer, |d| d.flex_1().min_w(px(0.)))
        .flex()
        .flex_col()
        .gap(px(if mixer { 8. } else { 6. }))
        .p(px(if mixer { 8. } else { 7. }))
        .rounded(px(if mixer { 12. } else { 10. }))
        .border_1()
        .bg(rgb(if selected {
            0x1c1c23
        } else if mixer {
            0x141418
        } else {
            STRIP
        }))
        .border_color(rgb(if selected { 0x3a3a44 } else { 0x1e1e24 }))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                let m = e.modifiers;
                match ch {
                    // ⌘ or ⇧: more tracks into the selection, as in the headers.
                    Channel::Track(i) if m.platform || m.shift => ws.pick_track(i, m.shift),
                    Channel::Track(i) => ws.select_track(i),
                    _ => ws.channel = ch,
                }
                cx.notify();
            }),
        )
        .children(header)
        .when(rows[0], |d| {
            d.child(eq_thumbnail(
                ws,
                ch,
                if mixer { 44. } else { 34. },
                color,
                cx,
            ))
        })
        .child(source)
        .when(rows[1], |d| {
            d.child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .children(effects),
            )
        })
        .when(rows[2] && (mixer || !sends.is_empty()), |d| {
            d.child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .children(sends),
            )
        })
        .when(mixer, |d| d.child(output))
        .children(auto_button)
        .child(pan_row)
        .child(readouts)
        .child(fader)
        .child(buttons)
        .child(plate)
}
