//! The step sequencer itself: pads, the grid, lanes and modulation, drawn.

use super::*;

impl Workspace {
    /// The step sequencer: the sounds as pads (tap one to hear it and
    /// pick it), every sound's steps in its own colour, the picked one opened on its velocities,
    /// and what can be done to it in a bar under them.
    pub fn steps_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(at) = self.pattern() else {
            return self.steps_none(cx).into_any_element();
        };
        let region = self.session.project.region(at).clone();
        let Content::Pattern { step, rows, .. } = &region.content else {
            unreachable!()
        };
        let steps = rows
            .iter()
            .map(|r| r.cells.len())
            .max()
            .unwrap_or(16)
            .max(1);
        let instrument = self.session.project.tracks[at.0].instrument.clone();
        let key = self.session.project.song_key();
        let track_id = self.session.project.tracks[at.0].id;
        let picked_row = self.steps_row.min(rows.len().saturating_sub(1));
        let names: Vec<String> = rows
            .iter()
            .map(|r| row_name(instrument.as_ref(), key, r.pitch))
            .collect();
        let colors: Vec<u32> = names
            .iter()
            .enumerate()
            .map(|(i, n)| row_color(n, i))
            .collect();
        // The step under the playhead, while playing in the region (each row on its own loop).
        let tick = self.playhead();
        let now = (self.player.playing()
            && (region.start..region.start + region.length).contains(&tick))
        .then(|| ((tick - region.start) / (*step).max(1)) as usize);
        let hit_now = |r: usize| {
            let cells = &rows[r].cells;
            now.is_some_and(|n| !cells.is_empty() && cells[n % cells.len()] > 0)
        };

        // The pads: a sound each, sixteen to a page; tap to hear it and pick it.
        let pad_h = 52.0;
        let pads = (0..16).map(|p| {
            let Some(row) = rows.get(p) else {
                return div()
                    .h(px(pad_h))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(9.))
                    .border_1()
                    .border_dashed()
                    .border_color(rgb(LINE_STRONG))
                    .text_color(rgb(TEXT_4))
                    .text_size(px(16.))
                    .cursor_pointer()
                    .hover(|d| d.text_color(rgb(TEXT_2)).border_color(rgb(TEXT_4)))
                    .debug_selector(move || format!("pad-{p}"))
                    .child("+")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            ws.add_sound_menu(at, e, cx)
                        }),
                    );
            };
            let (color, picked, hit) = (colors[p], p == picked_row, hit_now(p));
            let pitch = row.pitch;
            let mute = row.mute;
            div()
                .h(px(pad_h))
                .p(px(7.))
                .flex()
                .flex_col()
                .justify_between()
                .rounded(px(9.))
                .border_color(if picked {
                    rgb(TEXT).into()
                } else {
                    alpha(color, 0.45)
                })
                .when(picked, |d| d.border_2())
                .when(!picked, |d| d.border_1())
                .bg(alpha(
                    color,
                    if hit {
                        0.45
                    } else if mute {
                        0.06
                    } else {
                        0.16
                    },
                ))
                .when(hit, |d| {
                    d.shadow(vec![gpui::BoxShadow {
                        color: alpha(color, 0.5),
                        offset: gpui::point(px(0.), px(0.)),
                        blur_radius: px(14.),
                        spread_radius: px(0.),
                        inset: false,
                    }])
                })
                .cursor_pointer()
                .hover(|d| d.bg(alpha(color, 0.26)))
                .debug_selector(move || format!("pad-{p}"))
                .child(
                    div()
                        .text_size(px(11.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(if mute { TEXT_4 } else { TEXT }))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(names[p].clone()),
                )
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(9.5))
                        .text_color(rgb(TEXT_3))
                        .child(crate::editors::pianoroll::name_in(None, pitch)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.steps_row = p;
                        ws.player.preview(track_id, pitch, 100);
                        ws.begin_drag(
                            Drag::Key {
                                track: track_id,
                                pitch,
                            },
                            cx,
                        );
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| ws.row_menu(at, p, e, cx)),
                )
        });
        let pads = div()
            .w(px(self.header_w))
            .flex_none()
            .h_full()
            .p(px(10.))
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(div().grid().grid_cols(4).gap(px(7.)).children(pads));

        // A row's steps, and the picked row's velocities under them, in the same columns.
        const LABEL_W: f32 = 120.0;
        let beat_gap = |k: usize, d: Div| d.when(k.is_multiple_of(4) && k > 0, |d| d.ml(px(6.)));
        let mut grid = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            let (color, picked, mute, len) =
                (colors[r], r == picked_row, row.mute, row.cells.len());
            let label = div()
                .w(px(LABEL_W))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(6.))
                .pr(px(6.))
                .child(
                    div()
                        .size(px(8.))
                        .flex_none()
                        .rounded(px(2.))
                        .bg(rgb(color)),
                )
                .child(
                    div()
                        .id(("row", r))
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(if mute {
                            TEXT_4
                        } else if picked {
                            TEXT
                        } else {
                            TEXT_2
                        }))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .cursor_pointer()
                        .debug_selector(move || format!("row-{r}"))
                        .child(names[r].clone())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                ws.steps_row = r;
                                if let Some(pitch) = ws.row_pitch(at, r) {
                                    ws.player.preview(track_id, pitch, 100);
                                    ws.begin_drag(
                                        Drag::Key {
                                            track: track_id,
                                            pitch,
                                        },
                                        cx,
                                    );
                                }
                                cx.notify();
                            }),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                ws.row_menu(at, r, e, cx)
                            }),
                        ),
                )
                .when(len != steps, |d| {
                    d.child(
                        div()
                            .flex_none()
                            .px(px(4.))
                            .rounded(px(3.))
                            .bg(alpha(AMBER, 0.16))
                            .font_family(MONO)
                            .text_size(px(9.5))
                            .text_color(rgb(AMBER))
                            .child(format!("{len}")),
                    )
                })
                .child(
                    div()
                        .size(px(18.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.))
                        .text_size(px(9.5))
                        .font_weight(FontWeight::BOLD)
                        .cursor_pointer()
                        .bg(rgb(if mute { ACCENT } else { 0x23232a }))
                        .text_color(rgb(if mute { 0xffffff } else { TEXT_3 }))
                        .debug_selector(move || format!("row-mute-{r}"))
                        .child("M")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.edit_rows(at, cx, |rows| {
                                    if let Some(row) = rows.get_mut(r) {
                                        row.mute ^= true;
                                    }
                                });
                            }),
                        ),
                );
            let cells = (0..steps).map(|k| {
                let inside = k < len;
                let v = row.cells.get(k % len.max(1)).copied().unwrap_or(0);
                let playing = now.is_some_and(|n| n % len.max(1) == k % len.max(1)) && inside;
                let base = if (k / 4) % 2 == 0 { 0x19191f } else { 0x202027 };
                let strength = 0.45 + 0.55 * v as f32 / 127.0;
                let cell = div()
                    .flex_1()
                    .h(px(22.))
                    .relative()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(if playing {
                        alpha(0xffffff, 0.6)
                    } else {
                        alpha(0xffffff, 0.0)
                    })
                    .bg(match (v > 0, inside) {
                        (false, true) => rgb(base).into(),
                        (false, false) => rgb(0x141418).into(),
                        (true, true) if mute => alpha(color, 0.25),
                        (true, true) => alpha(color, strength),
                        (true, false) => alpha(color, 0.22),
                    })
                    .when(v >= ACCENTED && inside, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(3.))
                                .right(px(3.))
                                .size(px(5.))
                                .rounded(px(2.5))
                                .bg(rgb(0xffffff)),
                        )
                    })
                    // Its own settings, marked: a ratchet split, a chance faded, a pitch moved.
                    .when(v > 0 && inside, |d| {
                        let i = k % len.max(1);
                        let (n, chance, t) =
                            (row.repeat_at(i), row.chance_at(i), row.transpose_at(i));
                        d.when(chance < 100, |d| {
                            d.opacity(0.45 + 0.55 * chance as f32 / 100.0)
                        })
                        .when(n > 1, |d| {
                            d.child(div().absolute().inset_0().flex().children((0..n).map(|j| {
                                div().flex_1().h_full().when(j > 0, |d| {
                                    d.border_l_1().border_color(alpha(0x000000, 0.55))
                                })
                            })))
                        })
                        .when(chance < 100 || t != 0, |d| {
                            d.child(
                                div()
                                    .absolute()
                                    .bottom(px(1.))
                                    .left(px(3.))
                                    .right(px(3.))
                                    .flex()
                                    .justify_between()
                                    .font_family(MONO)
                                    .text_size(px(8.5))
                                    .text_color(rgb(0xffffff))
                                    .child(if chance < 100 {
                                        format!("{chance}%")
                                    } else {
                                        String::new()
                                    })
                                    .child(if t != 0 {
                                        format!("{t:+}")
                                    } else {
                                        String::new()
                                    }),
                            )
                        })
                    })
                    .debug_selector(move || format!("step-{r}-{k}"))
                    .when(inside, |d| {
                        d.cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.press_step(at, r, k, e.modifiers.alt, cx);
                                }),
                            )
                            .on_mouse_move(cx.listener(move |ws, e: &MouseMoveEvent, _, cx| {
                                if e.pressed_button == Some(MouseButton::Left) {
                                    ws.paint_step(r, k, cx);
                                }
                            }))
                    });
                beat_gap(k, cell)
            });
            let mut block = div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .px(px(6.))
                .py(px(3.))
                .rounded(px(8.))
                .when(picked, |d| d.bg(rgb(0x17171c)))
                .child(
                    div()
                        .h(px(24.))
                        .flex()
                        .items_center()
                        .gap(px(3.))
                        .child(label)
                        .children(cells),
                );
            if picked {
                // Its lane: each step's velocity, chance, pitch, ratchet, gate or nudge, a bar a
                // step, drawn with the mouse; and where the modulators on the sound move it.
                let mode = self.steps_lane_mode;
                let lane_bounds = self.steps_lane.clone();
                let targets = self.mod_targets.clone();
                let param = mode.sound_param();
                let target = param.map(|p| ModTarget::Sound(r, p));
                let mods = self.session.project.mods_of(at.0);
                let tempo = self.session.project.tempo;
                // Each modulator moving this lane: its colour, depth and kind.
                let moving: Vec<(usize, u32, f32, ModKind)> = mods
                    .iter()
                    .enumerate()
                    .flat_map(|(k, m)| {
                        m.routes
                            .iter()
                            .filter(|route| Some(route.target) == target)
                            .map(move |route| {
                                (
                                    k,
                                    crate::editors::modulation::MOD_COLORS[k % 8],
                                    route.depth,
                                    m.kind.clone(),
                                )
                            })
                    })
                    .collect();
                let offering = matches!(self.drag, Some(Drag::Modulator { track, .. }) if track == at.0)
                    && target.is_some();
                let bars = (0..steps).map(|k| {
                    let inside = k < len;
                    let i = k % len.max(1);
                    let (h, bipolar, text) = mode.shown(row, i);
                    let on = row.cells.get(i).copied().unwrap_or(0) > 0;
                    let lit = alpha(
                        color,
                        if inside {
                            if on { 0.9 } else { 0.4 }
                        } else {
                            0.25
                        },
                    );
                    let room = 34.0;
                    let bar = match bipolar {
                        false => div()
                            .absolute()
                            .bottom_0()
                            .left(gpui::relative(0.2))
                            .w(gpui::relative(0.6))
                            .h(px((h * room).max(2.0)))
                            .rounded_t(px(2.))
                            .bg(if h > 0.0 { lit } else { rgb(RAISED).into() }),
                        true => div()
                            .absolute()
                            .left(gpui::relative(0.2))
                            .w(gpui::relative(0.6))
                            .when(h >= 0.0, |d| d.bottom(px(room / 2.0)))
                            .when(h < 0.0, |d| d.top(px(room / 2.0 + 2.0)))
                            .h(px((h.abs() * room / 2.0).max(1.0)))
                            .rounded(px(1.))
                            .bg(if h != 0.0 { lit } else { rgb(RAISED).into() }),
                    };
                    // Where each modulator has this step, as it falls in the song.
                    let beats = (region.start + k as u64 * *step) as f64 / PPQ as f64;
                    let dots: Vec<Div> = moving
                        .iter()
                        .filter_map(|(_, c, depth, kind)| {
                            let v = pina_engine::dsp::modulate::value_at(kind, beats, tempo)?;
                            let (h0, _, _) = mode.shown(row, i);
                            let by = depth * v;
                            let to = match mode {
                                LaneMode::Velocity => (h0 + by / 127.0).clamp(0.0, 1.0),
                                LaneMode::Chance => (h0 + by / 100.0).clamp(0.0, 1.0),
                                _ => (h0 + by / 12.0).clamp(-1.0, 1.0),
                            };
                            let y = match bipolar {
                                false => to * room,
                                true => room / 2.0 + to * room / 2.0,
                            };
                            Some(
                                div()
                                    .absolute()
                                    .left(gpui::relative(0.5))
                                    .ml(px(-3.))
                                    .bottom(px(y - 3.0))
                                    .size(px(6.))
                                    .rounded(px(3.))
                                    .bg(rgb(*c))
                                    .border_1()
                                    .border_color(rgb(0x0b0b0e)),
                            )
                        })
                        .collect();
                    let cell = div()
                        .flex_1()
                        .h_full()
                        .relative()
                        .debug_selector(move || format!("step-velocity-{k}"))
                        .child(bar)
                        .children(dots)
                        .children(text.filter(|_| inside).map(|t| {
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .right_0()
                                .flex()
                                .justify_center()
                                .font_family(MONO)
                                .text_size(px(8.5))
                                .text_color(rgb(TEXT_2))
                                .child(t)
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                ws.begin_drag(Drag::StepVelocity, cx);
                                ws.draw_step_velocity(at, k, f32::from(e.position.y), cx);
                            }),
                        )
                        .on_mouse_move(cx.listener(move |ws, e: &MouseMoveEvent, _, cx| {
                            if e.pressed_button == Some(MouseButton::Left)
                                && matches!(ws.drag, Some(Drag::StepVelocity))
                            {
                                ws.draw_step_velocity(at, k, f32::from(e.position.y), cx);
                            }
                        }));
                    beat_gap(k, cell)
                });
                let tabs = LaneMode::ALL.map(|m| {
                    seg(m.name(), m == mode, 20.)
                        .px(px(7.))
                        .text_size(px(10.5))
                        .debug_selector(move || format!("lane-{}", m.name()))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.steps_lane_mode = m;
                                cx.notify();
                            }),
                        )
                });
                let chips: Vec<Div> = moving
                    .iter()
                    .map(|(k, c, depth, _)| {
                        let (k, mt) = (*k, target.unwrap_or(ModTarget::Volume));
                        div()
                            .h(px(20.))
                            .px(px(6.))
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .rounded(px(5.))
                            .bg(alpha(*c, 0.16))
                            .border_1()
                            .border_color(alpha(*c, 0.45))
                            .text_size(px(10.5))
                            .child(div().size(px(6.)).rounded(px(3.)).bg(rgb(*c)))
                            .child(self.mod_name(at.0, k))
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_color(rgb(TEXT_2))
                                    .child(pina_engine::dsp::modulate::depth_text(mt, *depth)),
                            )
                            .child(
                                div()
                                    .text_color(rgb(TEXT_3))
                                    .cursor_pointer()
                                    .hover(|d| d.text_color(rgb(TEXT)))
                                    .child("✕")
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            cx.stop_propagation();
                                            ws.unmap(at.0, k, mt, cx);
                                        }),
                                    ),
                            )
                            .cursor_ew_resize()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener({
                                    let depth = *depth;
                                    move |ws, e: &MouseDownEvent, _, cx| {
                                        let p = crate::workspace::pt(e.position);
                                        ws.begin_drag(
                                            Drag::ModDepth {
                                                track: at.0,
                                                index: k,
                                                target: mt,
                                                x: p.x,
                                                y: p.y,
                                                depth,
                                            },
                                            cx,
                                        );
                                    }
                                }),
                            )
                    })
                    .collect();
                let hint = match (param, moving.is_empty()) {
                    (Some(_), true) => "drag a modulator's ↗ here to move it",
                    (Some(_), false) => "",
                    (None, _) => "draw it",
                };
                block = block
                    .child(
                        div()
                            .h(px(22.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .w(px(LABEL_W))
                                    .flex_none()
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_3))
                                    .child("Each step's"),
                            )
                            .child(tray().p(px(2.)).rounded(px(6.)).children(tabs))
                            .children(chips)
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_4))
                                    .child(hint),
                            ),
                    )
                    .child(
                        div()
                            .h(px(40.))
                            .flex()
                            .gap(px(3.))
                            .child(div().w(px(LABEL_W)).flex_none())
                            .child(
                                div()
                                    .flex_1()
                                    .h_full()
                                    .relative()
                                    .flex()
                                    .gap(px(3.))
                                    .rounded(px(5.))
                                    .when(offering, |d| {
                                        d.bg(alpha(ACCENT, 0.1))
                                            .border_1()
                                            .border_color(alpha(ACCENT, 0.7))
                                    })
                                    .debug_selector(|| "steps-lane".into())
                                    .child(
                                        gpui::canvas(
                                            move |b: Bounds<Pixels>, _, _| {
                                                lane_bounds.set(b);
                                                if let Some(mt) = target {
                                                    targets.borrow_mut().push((at.0, mt, b));
                                                }
                                            },
                                            |_, _, _, _| {},
                                        )
                                        .absolute()
                                        .top_0()
                                        .left_0()
                                        .size_full(),
                                    )
                                    .children(bars),
                            ),
                    );
            }
            grid.push(block);
        }
        let add = button("+ Sound", false)
            .debug_selector(|| "add-sound".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| ws.add_sound_menu(at, e, cx)),
            );
        let step_beats = *step as f32 / PPQ as f32;
        let add_param = button("+ Parameter row", false)
            .debug_selector(|| "add-param".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    let t = at.0;
                    let k = ws.session.project.tracks[t].modulators.len();
                    let kind = ModKind::Steps {
                        values: vec![0.5; steps],
                        rate: pina_engine::dsp::modulate::ModRate::Beats(step_beats),
                        glide: 0.0,
                    };
                    ws.add_modulator(t, kind, cx);
                    if ws.session.project.tracks[t].modulators.len() > k {
                        ws.menu = Some(Menu {
                            at: e.position,
                            items: ws.target_menu(t, k),
                            ..Default::default()
                        });
                        ws.status =
                            Some("Pick what the row steps through; then paint its steps.".into());
                    }
                    cx.notify();
                }),
            );

        // The track's modulators, to drag onto a sound's lane (or any knob of the track); and
        // its parameter rows (steps modulators, as automation rows), painted in time with
        // the pattern.
        let t = at.0;
        let track_mods = self.session.project.mods_of(t);
        let mod_chips: Vec<Div> = track_mods
            .iter()
            .enumerate()
            .map(|(k, _)| {
                let c = crate::editors::modulation::MOD_COLORS[k % 8];
                div()
                    .h(px(22.))
                    .px(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .rounded(px(6.))
                    .bg(alpha(c, 0.14))
                    .border_1()
                    .border_color(alpha(c, 0.45))
                    .text_size(px(11.))
                    .cursor(gpui::CursorStyle::ClosedHand)
                    .debug_selector(move || format!("steps-mod-{k}"))
                    .child(div().size(px(7.)).rounded(px(3.5)).bg(rgb(c)))
                    .child(self.mod_name(t, k))
                    .child(div().text_color(rgb(TEXT_2)).child("↗"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.click_count == 2 {
                                ws.show_bottom(Bottom::Modulation, cx);
                                return;
                            }
                            ws.mod_drag_at = Some(crate::workspace::pt(e.position));
                            ws.begin_drag(Drag::Modulator { track: t, index: k }, cx);
                        }),
                    )
            })
            .collect();
        let quick = |label: &'static str, kind: ModKind, cx: &mut Context<Self>| {
            button(format!("+ {label}"), false)
                .debug_selector(move || format!("steps-add-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.add_modulator(t, kind.clone(), cx)),
                )
        };
        let strip = div()
            .h(px(30.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(5.))
            .px(px(6.))
            .child(
                div()
                    .w(px(LABEL_W - 6.0))
                    .flex_none()
                    .text_size(px(10.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT_3))
                    .child("MODULATE"),
            )
            .children(mod_chips)
            .child(quick("LFO", ModKind::lfo(), cx))
            .child(quick("Random", ModKind::random(), cx))
            .child(add_param)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child("drag a modulator's ↗ onto a sound's lane, or any knob of the track"),
            );
        let param_rows: Vec<Div> = track_mods
            .iter()
            .enumerate()
            .filter_map(|(k, m)| match &m.kind {
                ModKind::Steps { values, .. } => Some((k, values.clone(), m.routes.clone())),
                _ => None,
            })
            .map(|(k, values, routes)| {
                let c = crate::editors::modulation::MOD_COLORS[k % 8];
                let n = values.len().max(1);
                let label = match routes.first() {
                    Some(r) => format!(
                        "{} {}",
                        self.target_name(t, r.target),
                        pina_engine::dsp::modulate::depth_text(r.target, r.depth)
                    ),
                    None => "Map to…".to_string(),
                };
                let id = self.session.project.tracks[t].id;
                let live = self.mod_values.get(&id).map_or(0.0, |v| v[k]);
                let now_step = now;
                let bounds = self.mod_steps.clone();
                let cells = (0..steps).map(|i| {
                    let v = values[i % n];
                    let playing = now_step.is_some_and(|p| p % n == i % n);
                    let cell = div()
                        .flex_1()
                        .h_full()
                        .relative()
                        .rounded(px(3.))
                        .bg(rgb(if (i / 4) % 2 == 0 { 0x17171c } else { 0x1c1c22 }))
                        .when(playing, |d| d.border_1().border_color(alpha(0xffffff, 0.5)))
                        .child(
                            div()
                                .absolute()
                                .bottom_0()
                                .left(gpui::relative(0.15))
                                .w(gpui::relative(0.7))
                                .h(gpui::relative(v.clamp(0.0, 1.0)))
                                .rounded_t(px(2.))
                                .bg(alpha(c, if i < n { 0.75 } else { 0.3 })),
                        );
                    beat_gap(i, cell)
                });
                div()
                    .h(px(36.))
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .px(px(6.))
                    .py(px(3.))
                    .child(
                        div()
                            .w(px(LABEL_W))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .pr(px(6.))
                            .child(div().size(px(8.)).flex_none().rounded(px(2.)).bg(rgb(c)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .text_size(px(11.))
                                    .text_color(rgb(if routes.is_empty() { AMBER } else { TEXT_2 }))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .cursor_pointer()
                                    .debug_selector(move || format!("param-row-{k}"))
                                    .child(label)
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                            cx.stop_propagation();
                                            ws.menu = Some(Menu {
                                                at: e.position,
                                                items: ws.target_menu(t, k),
                                                ..Default::default()
                                            });
                                            cx.notify();
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(9.5))
                                    .text_color(rgb(TEXT_3))
                                    .child(format!("{live:.2}")),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .relative()
                            .flex()
                            .gap(px(3.))
                            .cursor_crosshair()
                            .debug_selector(move || format!("param-cells-{k}"))
                            .child(
                                gpui::canvas(
                                    move |b: Bounds<Pixels>, _, _| {
                                        bounds.borrow_mut().insert((t, k), b);
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .top_0()
                                .left_0()
                                .size_full(),
                            )
                            .children(cells)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.begin_drag(Drag::ModSteps { track: t, index: k }, cx);
                                    ws.paint_mod_step(t, k, crate::workspace::pt(e.position));
                                    ws.changed(cx);
                                }),
                            ),
                    )
            })
            .collect();

        // What can be done to the picked row, all in sight.
        let r = picked_row;
        let (pick_name, pick_color, pick_len) = (
            names.get(r).cloned().unwrap_or_default(),
            colors.get(r).copied().unwrap_or(TEXT_3),
            rows.get(r).map_or(steps, |row| row.cells.len()),
        );
        let op_button = |label: String, op: RowOp, name: &'static str, cx: &mut Context<Self>| {
            button(label, false)
                .debug_selector(move || format!("row-op-{name}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.row_op(at, r, op, cx)),
                )
        };
        let menu_button = |label: String,
                           name: &'static str,
                           items: Vec<(String, MenuAction)>,
                           cx: &mut Context<Self>| {
            button(label, false)
                .debug_selector(move || format!("row-op-{name}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        ws.menu = Some(Menu {
                            at: e.position,
                            items: items.clone(),
                            ..Default::default()
                        });
                        cx.notify();
                    }),
                )
        };
        let row_action = |op: RowOp| MenuAction::Row { at, row: r, op };
        let euclid: Vec<(String, MenuAction)> = (1..=pick_len.min(12))
            .map(|h| {
                (
                    format!("{h} hits over {pick_len}"),
                    row_action(RowOp::Euclid(h)),
                )
            })
            .collect();
        let mut lengths: Vec<(String, MenuAction)> = [3usize, 5, 6, 7, 12]
            .into_iter()
            .filter(|&l| l < steps)
            .map(|l| {
                (
                    format!("{l} steps (against {steps})"),
                    row_action(RowOp::Length(l)),
                )
            })
            .collect();
        lengths.push((format!("{steps} steps"), row_action(RowOp::Length(steps))));
        let label = |t: &'static str| div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(t);
        let bar = (!rows.is_empty()).then(|| {
            div()
                .h(px(42.))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(5.))
                .px(px(12.))
                .border_t_1()
                .border_color(rgb(LINE))
                .bg(rgb(SURFACE))
                .overflow_hidden()
                .child(
                    div()
                        .size(px(9.))
                        .flex_none()
                        .rounded(px(3.))
                        .bg(rgb(pick_color)),
                )
                .child(
                    div()
                        .mr(px(8.))
                        .text_size(px(12.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .whitespace_nowrap()
                        .child(pick_name),
                )
                .child(label("Fill"))
                .child(op_button("1".into(), RowOp::Fill(1), "fill-1", cx))
                .child(op_button("2".into(), RowOp::Fill(2), "fill-2", cx))
                .child(op_button("4".into(), RowOp::Fill(4), "fill-4", cx))
                .child(op_button("8".into(), RowOp::Fill(8), "fill-8", cx))
                .child(div().w(px(1.)).h(px(18.)).mx(px(3.)).bg(rgb(LINE)))
                .child(op_button("←".into(), RowOp::Shift(-1), "earlier", cx))
                .child(op_button("→".into(), RowOp::Shift(1), "later", cx))
                .child(op_button("Reverse".into(), RowOp::Reverse, "reverse", cx))
                .child(op_button(
                    "Randomize".into(),
                    RowOp::Randomize,
                    "randomize",
                    cx,
                ))
                .child(menu_button("Euclid ▾".into(), "euclid", euclid, cx))
                .child(div().flex_1())
                .child(menu_button(
                    format!("Length {pick_len} ▾"),
                    "length",
                    lengths,
                    cx,
                ))
                .child(op_button("Clear".into(), RowOp::Clear, "clear", cx))
                .child(op_button("Remove".into(), RowOp::Remove, "remove", cx))
        });

        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .bg(rgb(LANE))
            .child(
                div().flex_1().min_h(px(0.)).flex().child(pads).child(
                    div()
                        .id("steps")
                        .flex_1()
                        .min_w(px(0.))
                        .h_full()
                        .overflow_y_scroll()
                        .p(px(6.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(strip)
                        .children(grid)
                        .children(param_rows)
                        .child(div().pt(px(4.)).pl(px(6.)).flex().child(add)),
                ),
            )
            .children(bar)
            .into_any_element()
    }
}
