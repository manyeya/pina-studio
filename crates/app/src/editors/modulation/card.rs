//! A modulator's card: its name, what it is and how it's set, and where it goes.

use super::*;

impl Workspace {
    pub(crate) fn mod_card(&self, t: usize, k: usize, cx: &mut Context<Self>) -> Div {
        let track = &self.session.project.tracks[t];
        // One its stack shares: the stack's, with this track's routes.
        let m = Modulator {
            kind: self
                .session
                .project
                .mod_kind(t, k)
                .unwrap_or(&track.modulators[k].kind)
                .clone(),
            routes: track.modulators[k].routes.clone(),
        };
        let color = MOD_COLORS[k % MAX_MODS];
        let picked = self.mod_edit == Some((t, k));
        let value = self.mod_values.get(&track.id).map_or(0.0, |v| v[k]);
        let name = self.mod_name(t, k);
        // One its stack shares: named as the stack's in its title, the stack on its picture.
        let stack = match track.modulators[k].kind {
            ModKind::Shared { .. } => self.session.project.stack_of(t).map(|a| a.name.clone()),
            _ => None,
        };
        let name = match &stack {
            Some(s) => name
                .strip_prefix(&format!("{s} · "))
                .unwrap_or(&name)
                .to_string(),
            None => name,
        };
        let menu = |label: String,
                    items: Vec<(String, MenuAction)>,
                    name: String,
                    cx: &mut Context<Self>| {
            div()
                .h(px(22.))
                .px(px(7.))
                .flex()
                .items_center()
                .rounded(px(5.))
                .bg(rgb(FIELD))
                .font_family(MONO)
                .text_size(px(10.5))
                .text_color(rgb(TEXT_2))
                .whitespace_nowrap()
                .cursor_pointer()
                .hover(|d| d.bg(rgb(RAISED)))
                .debug_selector(move || name.clone())
                .child(label)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        ws.menu = Some(Menu {
                            at: e.position,
                            items: items.clone(),
                            ..Default::default()
                        });
                        cx.notify();
                    }),
                )
        };
        let change = |c: ModChange| MenuAction::Mod(t, k, c);
        let rates = |now: ModRate| -> Vec<(String, MenuAction)> {
            let mut items: Vec<(String, MenuAction)> = SYNCED
                .iter()
                .map(|&b| {
                    let r = ModRate::Beats(b);
                    let tick = if now == r { "✓ " } else { "" };
                    (format!("{tick}{}", r.label()), change(ModChange::Rate(r)))
                })
                .collect();
            items.push((String::new(), MenuAction::Nothing));
            items.extend(FREE.iter().map(|&h| {
                let r = ModRate::Hz(h);
                let tick = if now == r { "✓ " } else { "" };
                (format!("{tick}{}", r.label()), change(ModChange::Rate(r)))
            }));
            items
        };
        let mini = |label: &'static str, field: ModField, text: String, cx: &mut Context<Self>| {
            let v = self.mod_field(t, k, field);
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(1.))
                .cursor_ns_resize()
                .debug_selector(move || format!("mod-{k}-{field:?}"))
                .child(div().size(px(24.)).child(knob(v, 24., color, false)))
                .child(
                    div()
                        .text_size(px(9.5))
                        .text_color(rgb(TEXT_3))
                        .child(label),
                )
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(9.5))
                        .text_color(rgb(TEXT_2))
                        .child(text),
                )
                // Right-click: automate it.
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let Some(s) = setting_of(field) else {
                            return;
                        };
                        let param = Param::Mod(k as u8, s);
                        let id = ws.session.project.tracks[t].id;
                        let has = !ws.session.project.tracks[t]
                            .automation
                            .points(param)
                            .is_empty();
                        let label = match has {
                            true => format!("Show {}'s automation", s.name()),
                            false => format!("Automate {}", s.name()),
                        };
                        ws.menu = Some(Menu {
                            at: e.position,
                            items: vec![(
                                label,
                                MenuAction::Auto(
                                    id,
                                    crate::arrange::automation::AutoOp::Automate(param),
                                ),
                            )],
                            ..Default::default()
                        });
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let y = pt(e.position).y;
                        ws.begin_drag(
                            Drag::ModValue {
                                track: t,
                                index: k,
                                field,
                                y,
                                value: v,
                            },
                            cx,
                        );
                    }),
                )
        };
        let ms = |v: f32| match v {
            v if v >= 1000.0 => format!("{:.1} s", v / 1000.0),
            v => format!("{v:.0} ms"),
        };
        // What it is, and how it's set.
        let controls: Div = match &m.kind {
            ModKind::Lfo {
                shape,
                rate,
                phase,
                unipolar,
            } => {
                let shapes = LfoShape::ALL
                    .iter()
                    .map(|&s| {
                        let tick = if s == *shape { "✓ " } else { "" };
                        (format!("{tick}{}", s.name()), change(ModChange::Shape(s)))
                    })
                    .collect();
                let uni = *unipolar;
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(menu(
                        shape.name().into(),
                        shapes,
                        format!("mod-{k}-shape"),
                        cx,
                    ))
                    .child(menu(
                        rate.label(),
                        rates(*rate),
                        format!("mod-{k}-rate"),
                        cx,
                    ))
                    .child(
                        div()
                            .h(px(22.))
                            .px(px(7.))
                            .flex()
                            .items_center()
                            .rounded(px(5.))
                            .bg(if uni {
                                alpha(color, 0.25)
                            } else {
                                rgb(FIELD).into()
                            })
                            .text_size(px(10.5))
                            .text_color(rgb(if uni { TEXT } else { TEXT_3 }))
                            .cursor_pointer()
                            .child(if uni { "0 → 1" } else { "±" })
                            .debug_selector(move || format!("mod-{k}-uni"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.mod_change(t, k, ModChange::Unipolar(!uni), cx)
                                }),
                            ),
                    )
                    .child(div().flex_1())
                    .child(mini(
                        "Phase",
                        ModField::Phase,
                        format!("{:.0}°", phase * 360.0),
                        cx,
                    ))
            }
            ModKind::Steps {
                values,
                rate,
                glide,
            } => {
                let counts = [4usize, 8, 16, 32]
                    .map(|n| {
                        let tick = if n == values.len() { "✓ " } else { "" };
                        (format!("{tick}{n} steps"), change(ModChange::Count(n)))
                    })
                    .to_vec();
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(menu(
                        rate.label(),
                        rates(*rate),
                        format!("mod-{k}-rate"),
                        cx,
                    ))
                    .child(menu(
                        format!("{} steps", values.len()),
                        counts,
                        format!("mod-{k}-count"),
                        cx,
                    ))
                    .child(menu(
                        "Dice".into(),
                        vec![("Random values".into(), change(ModChange::Scramble))],
                        format!("mod-{k}-dice"),
                        cx,
                    ))
                    .child(div().flex_1())
                    .child(mini(
                        "Glide",
                        ModField::Glide,
                        format!("{:.0}%", glide * 100.0),
                        cx,
                    ))
            }
            ModKind::Envelope {
                attack_ms,
                decay_ms,
                sustain,
                release_ms,
            } => div()
                .flex()
                .justify_between()
                .child(mini("Attack", ModField::Attack, ms(*attack_ms), cx))
                .child(mini("Decay", ModField::Decay, ms(*decay_ms), cx))
                .child(mini(
                    "Sustain",
                    ModField::Sustain,
                    format!("{:.0}%", sustain * 100.0),
                    cx,
                ))
                .child(mini("Release", ModField::Release, ms(*release_ms), cx)),
            ModKind::Follower {
                source,
                attack_ms,
                release_ms,
            } => {
                let sources: Vec<(String, MenuAction)> = self
                    .session
                    .project
                    .tracks
                    .iter()
                    .filter(|o| o.id != track.id)
                    .map(|o| {
                        let tick = if o.id == *source { "✓ " } else { "" };
                        (format!("{tick}{}", o.name), change(ModChange::Source(o.id)))
                    })
                    .collect();
                let heard = self
                    .session
                    .project
                    .tracks
                    .iter()
                    .find(|o| o.id == *source)
                    .map_or("a track".to_string(), |o| o.name.clone());
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_3))
                            .child("Hears"),
                    )
                    .child(menu(heard, sources, format!("mod-{k}-source"), cx))
                    .child(div().flex_1())
                    .child(mini("Attack", ModField::Attack, ms(*attack_ms), cx))
                    .child(mini("Release", ModField::Release, ms(*release_ms), cx))
            }
            ModKind::Random { rate, smooth } => {
                let on = *smooth;
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(menu(
                        rate.label(),
                        rates(*rate),
                        format!("mod-{k}-rate"),
                        cx,
                    ))
                    .child(
                        div()
                            .h(px(22.))
                            .px(px(7.))
                            .flex()
                            .items_center()
                            .rounded(px(5.))
                            .bg(if on {
                                alpha(color, 0.25)
                            } else {
                                rgb(FIELD).into()
                            })
                            .text_size(px(10.5))
                            .text_color(rgb(if on { TEXT } else { TEXT_3 }))
                            .cursor_pointer()
                            .child(if on { "Smooth" } else { "Stepped" })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.mod_change(t, k, ModChange::Smooth(!on), cx)
                                }),
                            ),
                    )
            }
            ModKind::Macro { value } => div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .child("Turn it, or automate it"),
                )
                .child(div().flex_1())
                .child(mini(
                    "Value",
                    ModField::Value,
                    format!("{:.0}%", value * 100.0),
                    cx,
                )),
            // The track left its stack, or the stack the modulator.
            ModKind::Shared { .. } => div()
                .text_size(px(10.5))
                .text_color(rgb(TEXT_3))
                .child("Its stack no longer shares this: it moves nothing"),
        };
        // Where it goes: each route's depth dragged sideways (⇧ finer), × to take it off.
        let routes: Vec<Div> = m
            .routes
            .iter()
            .map(|r| {
                let (mt, depth) = (r.target, r.depth);
                let range = modulate::depth_range(mt);
                let share = (depth / range).clamp(-1.0, 1.0);
                div()
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(7.))
                    .rounded(px(6.))
                    .bg(rgb(0x141419))
                    .relative()
                    .overflow_hidden()
                    .cursor_ew_resize()
                    .debug_selector(move || format!("mod-{k}-route-{mt:?}"))
                    // How far, as a bar from the middle.
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(gpui::relative(0.5 + share.min(0.0) / 2.0))
                            .w(gpui::relative(share.abs() / 2.0))
                            .bg(alpha(color, 0.22)),
                    )
                    .child(div().text_color(rgb(color)).text_size(px(11.)).child("→"))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(11.))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(self.target_name(t, mt)),
                    )
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_2))
                            .child(modulate::depth_text(mt, depth)),
                    )
                    .child(
                        div()
                            .size(px(16.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_3))
                            .cursor_pointer()
                            .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                            .child("✕")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.unmap(t, k, mt, cx);
                                }),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let p = pt(e.position);
                            ws.begin_drag(
                                Drag::ModDepth {
                                    track: t,
                                    index: k,
                                    target: mt,
                                    x: p.x,
                                    y: p.y,
                                    depth,
                                },
                                cx,
                            );
                        }),
                    )
            })
            .collect();
        let none = m.routes.is_empty();
        div()
            .w(px(248.))
            .flex_none()
            .h_full()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(10.))
            .rounded(px(12.))
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(if picked { color } else { LINE }))
            .when(picked, |d| {
                d.shadow(vec![gpui::BoxShadow {
                    color: alpha(color, 0.35),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(14.),
                    spread_radius: px(0.),
                    inset: false,
                }])
            })
            .debug_selector(move || format!("mod-card-{k}"))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    ws.mod_edit = match ws.mod_edit == Some((t, k)) {
                        true => None,
                        false => Some((t, k)),
                    };
                    cx.notify();
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(
                        div()
                            .flex_none()
                            .size(px(9.))
                            .rounded(px(5.))
                            .bg(rgb(color)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(name),
                    )
                    .child(div().flex_1())
                    // Its handle: drag it onto a knob.
                    .child(
                        div()
                            .h(px(22.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .rounded(px(6.))
                            .bg(alpha(color, 0.18))
                            .border_1()
                            .border_color(alpha(color, 0.5))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT))
                            .cursor(gpui::CursorStyle::ClosedHand)
                            .debug_selector(move || format!("mod-{k}-map"))
                            .child("↗")
                            .child(div().text_size(px(10.)).child("Map"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.mod_drag_at = Some(pt(e.position));
                                    ws.begin_drag(Drag::Modulator { track: t, index: k }, cx);
                                }),
                            ),
                    )
                    // Or learn it: whatever's turned next in a plugin's own window.
                    .child({
                        let learning = self
                            .mod_learn
                            .as_ref()
                            .is_some_and(|l| (l.track, l.index) == (t, k));
                        div()
                            .h(px(22.))
                            .px(px(7.))
                            .flex()
                            .items_center()
                            .rounded(px(6.))
                            .bg(if learning {
                                alpha(color, 0.45)
                            } else {
                                rgb(FIELD).into()
                            })
                            .text_size(px(10.))
                            .text_color(rgb(if learning { TEXT } else { TEXT_2 }))
                            .cursor_pointer()
                            .hover(|d| d.bg(rgb(RAISED)))
                            .debug_selector(move || format!("mod-{k}-learn"))
                            .child(if learning { "Learning…" } else { "Learn" })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.learn(t, k, cx);
                                }),
                            )
                    })
                    // Or pick where from a list.
                    .child({
                        div()
                            .size(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .cursor_pointer()
                            .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                            .debug_selector(move || format!("mod-{k}-to"))
                            .child("⋯")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    // Made when asked for: it names every parameter of the
                                    // track's plugins.
                                    let items = ws.target_menu(t, k);
                                    ws.menu = Some(Menu::search(e.position, items));
                                    cx.notify();
                                }),
                            )
                    })
                    .child(
                        div()
                            .size(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .cursor_pointer()
                            .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                            .debug_selector(move || format!("mod-{k}-remove"))
                            .child("✕")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.remove_modulator(t, k, cx);
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .relative()
                    .child(self.mod_visual(t, k, &m.kind, color, cx))
                    // Where it is now.
                    .child(
                        div()
                            .absolute()
                            .top(px(6.))
                            .right(px(8.))
                            .font_family(MONO)
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_3))
                            .child(format!("{value:+.2}")),
                    )
                    .when_some(stack, |d, s| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(6.))
                                .left(px(8.))
                                .px(px(6.))
                                .h(px(16.))
                                .flex()
                                .items_center()
                                .rounded(px(4.))
                                .bg(alpha(color, 0.18))
                                .text_size(px(9.5))
                                .text_color(rgb(TEXT_2))
                                .child(format!("Shared · {s}")),
                        )
                    }),
            )
            .child(controls.flex_none())
            .child(
                div()
                    .id(("mod-routes", k))
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .children(routes)
                    .when(none, |d| {
                        d.child(
                            div()
                                .h(px(24.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.))
                                .border_1()
                                .border_dashed()
                                .border_color(rgb(LINE_STRONG))
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_3))
                                .child("drag ↗ Map onto a knob"),
                        )
                    }),
            )
    }
}
