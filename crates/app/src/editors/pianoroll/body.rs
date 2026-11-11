//! The roll itself: keys, grid, notes and lanes, drawn.

use super::*;

impl Workspace {
    pub fn roll_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (ppt, sx, sy, kh) = (
            self.px_per_tick(),
            self.scroll_x,
            self.roll_scroll_y,
            self.roll.key_h,
        );
        let (beat_ticks, bar_ticks) = (self.session.project.beat(), self.session.project.bar());
        let at = self.roll_region();
        let color = at.map_or(ACCENT, |at| self.color(at.0));
        let project = &self.session.project;
        let span = at.map(|at| {
            let r = project.region(at);
            (r.start, r.start + r.length)
        });
        let track = at.map(|at| project.tracks[at.0].id);
        let key = self.roll.scale.then(|| project.song_key()).flatten();
        // Names spelled by the key, scale shown or not.
        let spelling = project.song_key();
        let hover = self.roll_hover;
        let by_velocity = self.roll.by_velocity;

        // Pitches sounding now: the region's notes under the playhead, and a key held down.
        let tick = self.playhead();
        let mut sounding = [false; 128];
        if let (Some(at), Some((start, _))) = (at, span)
            && self.player.playing()
            && let Content::Midi { notes, .. } = &project.region(at).content
        {
            for n in notes {
                let on = start as i64 + n.start;
                if (on..on + n.length as i64).contains(&(tick as i64)) {
                    sounding[n.pitch as usize] = true;
                }
            }
        }
        match &self.drag {
            Some(Drag::Key { pitch, .. }) => sounding[*pitch as usize] = true,
            Some(Drag::Paint { .. }) => self
                .painting
                .iter()
                .for_each(|&p| sounding[p as usize] = true),
            _ => {}
        }

        let bounds = self.roll_bounds.clone();
        let grid = canvas(
            move |b, _, _| bounds.set(b),
            move |b, _, window, _| {
                let w = f32::from(b.size.width);
                let quad = |window: &mut Window, x: f32, y: f32, ww: f32, hh: f32, c: Hsla| {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x + px(x), b.origin.y + px(y)),
                            size(px(ww), px(hh)),
                        ),
                        c,
                    ));
                };
                let first = (sy / kh) as i32;
                let last = ((sy + f32::from(b.size.height)) / kh) as i32 + 1;
                for row in first..=last.min(127) {
                    let pitch = (127 - row) as u8;
                    let y = row as f32 * kh - sy;
                    let bg = match key {
                        Some(k) if k.is_root(pitch) => Hsla::from(rgb(0x15202c)),
                        Some(k) if k.contains(pitch) => rgb(0x131318).into(),
                        Some(_) => rgb(0x0d0d10).into(),
                        None if black(pitch) => rgb(0x0f0f12).into(),
                        None => rgb(0x141418).into(),
                    };
                    quad(window, 0.0, y, w, kh, bg);
                    if hover == Some(pitch) {
                        quad(window, 0.0, y, w, kh, alpha(0xffffff, 0.035));
                    }
                    let line = if pitch.is_multiple_of(12) {
                        0x24242b
                    } else {
                        0x17171c
                    };
                    quad(window, 0.0, y + kh - 1.0, w, 1.0, rgb(line).into());
                }
                let bar_px = ppt * bar_ticks as f32;
                if bar_px > 0.0 {
                    let h = f32::from(b.size.height);
                    for bar in (sx / bar_px) as u64..=((sx + w) / bar_px) as u64 + 1 {
                        if bar % 2 == 1 {
                            let x = bar as f32 * bar_px - sx;
                            quad(window, x, 0.0, bar_px, h, alpha(0xffffff, 0.014));
                        }
                    }
                }
                let sixteenth = ppt * (PPQ / 4) as f32;
                let fine = sixteenth >= 6.0;
                let step = if fine { PPQ / 4 } else { beat_ticks };
                let step_px = ppt * step as f32;
                let first = (sx / step_px) as u64;
                let last = ((sx + w) / step_px) as u64 + 1;
                for k in first..=last {
                    let t = k * step;
                    let color = if t % bar_ticks == 0 {
                        0x34343d
                    } else if t % beat_ticks == 0 {
                        0x24242b
                    } else {
                        0x17171c
                    };
                    let x = (t as f32 * ppt - sx).round();
                    quad(
                        window,
                        x,
                        0.0,
                        1.0,
                        f32::from(b.size.height),
                        rgb(color).into(),
                    );
                }
                // Shade what's outside the region.
                if let Some((start, end)) = span {
                    let (x0, x1) = (start as f32 * ppt - sx, end as f32 * ppt - sx);
                    let shade = alpha(0x000000, 0.4);
                    if x0 > 0.0 {
                        quad(window, 0.0, 0.0, x0, f32::from(b.size.height), shade);
                    }
                    if x1 < w {
                        quad(
                            window,
                            x1.max(0.0),
                            0.0,
                            w - x1.max(0.0),
                            f32::from(b.size.height),
                            shade,
                        );
                    }
                }
            },
        )
        .absolute()
        .size_full();

        // Another track's notes, faintly behind.
        let mut ghosts = Vec::new();
        if let Some(ghost) = self.roll.ghost.filter(|&g| Some(g) != track)
            && let Some(t) = project.tracks.iter().find(|t| t.id == ghost)
        {
            let width = f32::from(self.roll_bounds.get().size.width).max(400.0);
            for r in &t.regions {
                let Some(notes) = r.content.notes(r.length) else {
                    continue;
                };
                for n in notes.iter() {
                    let x = (r.start as i64 + n.start) as f32 * ppt - sx;
                    let w = (n.length as f32 * ppt).max(3.0);
                    if x + w < 0.0 || x > width {
                        continue;
                    }
                    ghosts.push(
                        div()
                            .absolute()
                            .left(px(x))
                            .top(px((127 - n.pitch) as f32 * kh - sy + 1.))
                            .w(px(w - 1.0))
                            .h(px(kh - 2.))
                            .rounded(px(3.))
                            .border_1()
                            .border_dashed()
                            .border_color(alpha(TEXT, 0.22))
                            .bg(alpha(TEXT, 0.035)),
                    );
                }
            }
        }

        let mut notes = Vec::new();
        let mut stems = Vec::new();
        let mut tip = None;
        let lane = self.roll.lane;
        let mut curve: Vec<(f32, f32)> = Vec::new();
        let mut curve_now = None;
        // Each note's own expression: its line in the lane, and its bend over it in the grid.
        let mut owns: Vec<NoteLine> = Vec::new();
        let mut glides: Vec<NoteLine> = Vec::new();
        if let (Some(at), Some((start, end))) = (at, span) {
            let Content::Midi {
                notes: list,
                controls,
            } = &project.region(at).content
            else {
                unreachable!()
            };
            for (i, n) in list.iter().enumerate() {
                let x = (start as i64 + n.start) as f32 * ppt - sx;
                let w = (n.length as f32 * ppt).max(3.0);
                let y = (127 - n.pitch) as f32 * kh - sy;
                let picked = self.picked.contains(&i);
                let n = n.clone();
                let ink = if by_velocity {
                    velocity_color(n.velocity)
                } else {
                    color
                };
                let strength = if by_velocity {
                    0.92
                } else {
                    0.55 + 0.45 * n.velocity as f32 / 127.0
                } * (0.4 + 0.6 * n.chance.min(100) as f32 / 100.0);
                let label = w >= 26.0 && kh >= 10.0;
                let odds =
                    (n.chance < 100 && w >= 46.0 && kh >= 10.0).then(|| format!("{}%", n.chance));
                notes.push(
                    div()
                        .absolute()
                        .left(px(x))
                        .top(px(y + 1.))
                        .w(px(w - 1.0))
                        .h(px(kh - 2.))
                        .pl(px(5.))
                        .rounded(px(3.))
                        .overflow_hidden()
                        .bg(match (picked, n.muted) {
                            (_, true) => alpha(ink, 0.08),
                            (true, _) => Hsla::from(rgb(tint(ink, 0.55))),
                            _ => alpha(ink, strength),
                        })
                        .when(n.muted, |d| {
                            d.border_1().border_dashed().border_color(alpha(ink, 0.7))
                        })
                        .when(picked, |d| d.border_1().border_color(rgb(0xffffff)))
                        .font_family(MONO)
                        .text_size(px(8.5))
                        .line_height(px(kh - 2.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(if n.muted { ink } else { BG }))
                        .whitespace_nowrap()
                        .cursor(match self.tool {
                            Tool::Eraser => gpui::CursorStyle::OperationNotAllowed,
                            Tool::Scissors => gpui::CursorStyle::IBeam,
                            // Its middle bends it.
                            _ if lane == Lane::Pitch => gpui::CursorStyle::Crosshair,
                            _ => gpui::CursorStyle::OpenHand,
                        })
                        .when(label, |d| d.child(name_in(spelling, n.pitch)))
                        .when_some(odds, |d, odds| {
                            d.child(
                                div()
                                    .absolute()
                                    .right(px(6.))
                                    .top_0()
                                    .opacity(0.8)
                                    .child(odds),
                            )
                        })
                        // Its velocity, as a line along its foot as long as it's hard.
                        .when(kh >= 9.0 && w > 8.0, |d| {
                            d.child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .bottom_0()
                                    .h(px(2.))
                                    .w(px((w - 1.0) * n.velocity as f32 / 127.0))
                                    .bg(alpha(0x000000, 0.35)),
                            )
                        })
                        .debug_selector(move || format!("note-{i}"))
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .w(px(4.))
                                .h_full()
                                .cursor_ew_resize()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    grab_note(at, i, Edge::Start, cx),
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .right_0()
                                .top_0()
                                .w(px(5.))
                                .h_full()
                                .cursor_ew_resize()
                                .on_mouse_down(MouseButton::Left, grab_note(at, i, Edge::End, cx)),
                        )
                        .on_mouse_down(MouseButton::Left, grab_note(at, i, Edge::Move, cx))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.roll_menu(e, Some(i), cx);
                            }),
                        ),
                );
                if lane.per_note() {
                    let h = match lane {
                        Lane::Chance => n.chance.min(100) as f32 / 100.0 * VELOCITY_H,
                        Lane::Release => n.release.min(127) as f32 / 127.0 * VELOCITY_H,
                        _ => n.velocity as f32 / 127.0 * VELOCITY_H,
                    };
                    if lane == Lane::Velocity && n.spread > 0 {
                        let s = n.spread as f32 / 127.0 * VELOCITY_H;
                        let (lo, hi) = ((h - s).max(0.0), (h + s).min(VELOCITY_H));
                        stems.push(
                            div()
                                .absolute()
                                .left(px(x - 2.))
                                .bottom(px(5.0 + lo))
                                .w(px(6.))
                                .h(px(hi - lo))
                                .rounded(px(3.))
                                .bg(alpha(ink, 0.3)),
                        );
                    }
                    let dot = if picked {
                        Hsla::from(rgb(0xffffff))
                    } else {
                        rgb(ink).into()
                    };
                    stems.push(
                        div()
                            .absolute()
                            .left(px(x))
                            .bottom(px(5.))
                            .w(px(2.))
                            .h(px(h))
                            .bg(dot),
                    );
                    stems.push(
                        div()
                            .absolute()
                            .left(px(x - 3.))
                            .bottom(px(5.0 + h - 4.0))
                            .size(px(8.))
                            .rounded(px(4.))
                            .bg(dot)
                            .cursor_ns_resize()
                            .debug_selector(move || format!("velocity-{i}"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    // The dot's note and, if it's picked, every picked note.
                                    if !ws.picked.contains(&i) {
                                        ws.picked = vec![i];
                                    }
                                    let notes = ws
                                        .session
                                        .project
                                        .region(at)
                                        .content
                                        .notes(0)
                                        .map(|n| n.to_vec())
                                        .unwrap_or_default();
                                    ws.held = ws
                                        .picked
                                        .iter()
                                        .filter_map(|&k| notes.get(k).map(|n| (k, n.clone())))
                                        .collect();
                                    let y = f32::from(e.position.y);
                                    let drag = match ws.roll.lane {
                                        Lane::Chance => Drag::NoteField {
                                            at,
                                            y,
                                            field: NoteField::Chance,
                                        },
                                        Lane::Release => Drag::NoteField {
                                            at,
                                            y,
                                            field: NoteField::Release,
                                        },
                                        _ => Drag::Velocity { at, y },
                                    };
                                    ws.begin_drag(drag, cx);
                                }),
                            ),
                    );
                }
            }
            // The tooltip, for the note grabbed (or picked alone).
            let lead = match &self.drag {
                Some(Drag::Notes { lead, .. }) => Some(*lead),
                _ if self.picked.len() == 1 => self.picked.first().copied(),
                _ => None,
            };
            if let Some(n) = lead.and_then(|i| list.get(i)) {
                let x = (start as i64 + n.start) as f32 * ppt - sx;
                let w = (n.length as f32 * ppt).max(3.0);
                let y = (127 - n.pitch) as f32 * kh - sy;
                let at_tick = (start as i64 + n.start).max(0) as u64;
                tip = Some(
                    div()
                        .absolute()
                        .left(px(x + w + 6.))
                        .top(px(y - 2.))
                        .px(px(6.))
                        .py(px(1.))
                        .rounded(px(4.))
                        .bg(rgb(TEXT))
                        .text_color(rgb(BG))
                        .font_family(MONO)
                        .text_size(px(10.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .whitespace_nowrap()
                        .child(format!(
                            "{} · {} · {} · vel {}",
                            name_in(spelling, n.pitch),
                            self.position(at_tick),
                            length_name(n.length),
                            n.velocity
                        )),
                );
            }
            let shown_w = f32::from(self.roll_bounds.get().size.width).max(400.0);
            let some_picked = !self.picked.is_empty();
            for (i, n) in list.iter().enumerate() {
                let x0 = (start as i64 + n.start) as f32 * ppt - sx;
                let x1 = x0 + n.length as f32 * ppt;
                if x1 < 0.0 || x0 > shown_w {
                    continue;
                }
                let line = |d: Dim| -> Vec<(f32, f32, f32)> {
                    n.expression
                        .get(d)
                        .iter()
                        .map(|b| {
                            let x = (start as i64 + n.start + b.at as i64) as f32 * ppt - sx;
                            (x, b.value, b.curve)
                        })
                        .collect()
                };
                let picked = self.picked.contains(&i);
                let own = |points| NoteLine {
                    from: x0,
                    to: x1,
                    mid: (127 - n.pitch) as f32 * kh - sy + kh / 2.0,
                    points,
                    ink: if by_velocity {
                        velocity_color(n.velocity)
                    } else {
                        color
                    },
                    picked,
                    bright: picked || !some_picked,
                };
                let bends = line(Dim::Pitch);
                if !bends.is_empty() {
                    glides.push(own(bends));
                }
                if let Some(d) = lane.dim() {
                    owns.push(own(line(d)));
                }
            }
            // The lane's controller, as a held line.
            if !lane.per_note() && !lane.expression() {
                let mut points: Vec<&ControlPoint> =
                    controls.iter().filter(|c| lane.holds(&c.kind)).collect();
                points.sort_by_key(|c| c.start);
                for c in &points {
                    curve.push((
                        (start as i64 + c.start) as f32 * ppt - sx,
                        lane.level(&c.kind),
                    ));
                }
                let now = tick as i64 - start as i64;
                curve_now = points
                    .iter()
                    .rev()
                    .find(|c| c.start <= now)
                    .map(|c| lane.say(&c.kind));
                let _ = end;
            }
        }

        // Notes bent by their own expression: the bend as a line over each, a row a semitone;
        // with the Pitch lane showing, its points to take hold of.
        let editing = lane == Lane::Pitch;
        let bent = (!glides.is_empty()).then(|| {
            canvas(
                |_, _, _| {},
                move |b, _, window, _| {
                    let (ox, oy) = (f32::from(b.origin.x), f32::from(b.origin.y));
                    for g in &glides {
                        let points: Vec<(f32, f32, f32)> = g
                            .points
                            .iter()
                            .map(|&(x, v, bow)| (ox + x, oy + g.mid - v * kh, bow))
                            .collect();
                        let (ink, width) = match (editing, g.bright) {
                            (false, _) => (alpha(0xffffff, 0.85), 1.5),
                            (true, true) => (Hsla::from(rgb(0xffffff)), 2.0),
                            (true, false) => (alpha(0xffffff, 0.45), 1.5),
                        };
                        paint_line(window, &points, (ox + g.from, ox + g.to), width, ink);
                        if editing {
                            dots(window, &points, g.ink, g.picked);
                        }
                    }
                },
            )
            .absolute()
            .size_full()
        });

        // The rubber band.
        let band = match &self.drag {
            Some(Drag::Marquee { from, to, .. }) => {
                let o = self.roll_bounds.get().origin;
                let (x0, y0) = (
                    from.x.min(to.x) - f32::from(o.x),
                    from.y.min(to.y) - f32::from(o.y),
                );
                let (w, h) = ((from.x - to.x).abs(), (from.y - to.y).abs());
                Some(
                    div()
                        .absolute()
                        .left(px(x0))
                        .top(px(y0))
                        .w(px(w))
                        .h(px(h))
                        .rounded(px(2.))
                        .border_1()
                        .border_color(alpha(ACCENT, 0.9))
                        .bg(alpha(ACCENT, 0.12)),
                )
            }
            _ => None,
        };

        // The keyboard: click a key to hear it.
        let keys = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let w = f32::from(b.size.width);
                let first = (sy / kh) as i32;
                let last = ((sy + f32::from(b.size.height)) / kh) as i32 + 1;
                window.paint_quad(fill(b, rgb(0xd9d9df)));
                for row in first..=last.min(127) {
                    let pitch = (127 - row) as u8;
                    let y = b.origin.y + px(row as f32 * kh - sy);
                    let line = |window: &mut Window, at: f32| {
                        window.paint_quad(fill(
                            Bounds::new(point(b.origin.x, y + px(at)), size(px(w), px(1.))),
                            rgb(0xb0b0ba),
                        ));
                    };
                    if black(pitch) {
                        line(window, kh / 2.0);
                    } else {
                        if sounding[pitch as usize] {
                            window.paint_quad(fill(
                                Bounds::new(point(b.origin.x, y), size(px(w), px(kh))),
                                rgb(color),
                            ));
                        } else if hover == Some(pitch) {
                            window.paint_quad(fill(
                                Bounds::new(point(b.origin.x, y), size(px(w), px(kh))),
                                rgb(0xc8c8d0),
                            ));
                        }
                        if pitch > 0 && !black(pitch - 1) {
                            line(window, kh - 0.5);
                        }
                    }
                }
                for row in first..=last.min(127) {
                    let pitch = (127 - row) as u8;
                    // The song's scale down the keys' edge: its notes marked, its tonic bright.
                    if let Some(k) = key.filter(|k| k.contains(pitch)) {
                        let y = b.origin.y + px(row as f32 * kh - sy + 1.0);
                        let mark = Bounds::new(
                            point(b.origin.x + px(w - 4.0), y),
                            size(px(4.), px(kh - 2.)),
                        );
                        let c = match k.is_root(pitch) {
                            true => Hsla::from(rgb(ACCENT)),
                            false => alpha(ACCENT, 0.45),
                        };
                        window.paint_quad(fill(mark, c));
                    }
                    if black(pitch) {
                        let y = b.origin.y + px(row as f32 * kh - sy + 1.0);
                        let key = Bounds::new(point(b.origin.x, y), size(px(w * 0.6), px(kh - 2.)));
                        let c = match (sounding[pitch as usize], hover == Some(pitch)) {
                            (true, _) => rgb(color),
                            (_, true) => rgb(0x34343c),
                            _ => rgb(0x18181c),
                        };
                        window.paint_quad(fill(key, c).corner_radii(gpui::Corners {
                            top_left: px(0.),
                            bottom_left: px(0.),
                            top_right: px(3.),
                            bottom_right: px(3.),
                        }));
                    }
                }
            },
        )
        .absolute()
        .size_full();
        // Every C, and the song's tonic, named; and the pitch under the mouse.
        let tonic = |p: u8| key.is_some_and(|k| k.is_root(p));
        let labels = (0..=127u8)
            .filter(|&p| (p % 12 == 0 || (tonic(p) && !black(p)) || hover == Some(p)) && kh >= 9.0)
            .map(|p| {
                div()
                    .absolute()
                    .right(px(7.))
                    .top(px((127 - p) as f32 * kh - sy + (kh - 10.0) / 2.0))
                    .font_family(MONO)
                    .text_size(px(9.))
                    .line_height(px(10.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(match (tonic(p), hover == Some(p)) {
                        (true, _) => ACCENT,
                        (_, true) if black(p) => TEXT,
                        _ => 0x5a5a64,
                    }))
                    .child(name_in(spelling, p))
            });
        let keyboard = div()
            .w(px(KEYS_W))
            .flex_none()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(ROLL_RULER))
                    .flex_none()
                    .bg(rgb(WELL))
                    .border_b_1()
                    .border_color(rgb(LINE)),
            )
            .child(
                div()
                    .flex_1()
                    .relative()
                    .overflow_hidden()
                    .child(keys)
                    .children(labels)
                    .when_some(track, |d, track| {
                        d.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                let pitch = ws.pitch_at(f32::from(e.position.y));
                                // ⌘-click picks every note of the key's pitch; ⇧-click (as
                                // Logic's) adds them to those picked, key by key.
                                if e.modifiers.platform || e.modifiers.shift {
                                    let had = ws.picked.clone();
                                    ws.roll_op(RollOp::SelectPitch(pitch), cx);
                                    if e.modifiers.shift {
                                        let mut all = had;
                                        all.extend(ws.picked.iter().copied());
                                        all.sort_unstable();
                                        all.dedup();
                                        ws.picked = all;
                                    }
                                    return;
                                }
                                ws.player.preview(track, pitch, 100);
                                ws.begin_drag(Drag::Key { track, pitch }, cx);
                            }),
                        )
                    })
                    .on_mouse_move(
                        cx.listener(|ws, e: &MouseMoveEvent, _, cx| ws.hover_roll(e, cx)),
                    )
                    .on_scroll_wheel(cx.listener(|ws, e, _, cx| ws.roll_scroll(e, cx))),
            );

        let bar_px = ppt * bar_ticks as f32;
        let width = f32::from(self.roll_bounds.get().size.width).max(400.0);
        let every = (40.0 / bar_px).ceil().max(1.0) as u64;
        let bars = ((sx / bar_px) as u64..=((sx + width) / bar_px) as u64 + 1)
            .filter(|b| b % every == 0)
            .map(|b| {
                div()
                    .absolute()
                    .left(px(b as f32 * bar_px - sx + 5.))
                    .top(px(4.))
                    .font_family(MONO)
                    .text_size(px(10.))
                    .line_height(px(12.))
                    .text_color(rgb(TEXT_2))
                    .child(format!("{}", b + 1))
            });
        // The chords the region's notes play, beat by beat, named in the song's key.
        let chords: Vec<Div> = match (at, span) {
            (Some(at), Some((start, end))) => {
                let notes = project
                    .region(at)
                    .content
                    .notes(0)
                    .map(|n| n.to_vec())
                    .unwrap_or_default();
                let length = (end - start) as i64;
                pina_engine::music::harmony::read_chords(&notes, (0, length), beat_ticks)
                    .into_iter()
                    .enumerate()
                    .filter_map(|(k, (from, to, c))| {
                        let x = (start as i64 + from) as f32 * ppt - sx;
                        let w = (to - from) as f32 * ppt;
                        (x + w > 0.0 && x < width).then(|| {
                            div()
                                .absolute()
                                .left(px(x + 1.))
                                .top(px(19.))
                                .w(px((w - 2.).max(4.)))
                                .h(px(14.))
                                .px(px(4.))
                                .rounded(px(3.))
                                .bg(alpha(color, 0.18))
                                .text_size(px(10.))
                                .line_height(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(TEXT_2))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .debug_selector(move || format!("roll-chord-{k}"))
                                .child(c.name_in(spelling.as_ref()))
                        })
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        let playhead = tick as f32 * ppt - sx;
        let tools = self.roll_panel(at, cx);

        // The lane: a tab for each thing it can show, the notes' own over the controllers'.
        let lane_tab = |l: Lane, cx: &mut Context<Self>| {
            seg(l.short(), lane == l, 20.)
                .flex_1()
                .px(px(0.))
                .text_size(px(10.5))
                .debug_selector(move || format!("lane-{}", l.short()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.roll.lane = l;
                        cx.notify();
                    }),
                )
        };
        let lane_tabs = div()
            .flex()
            .flex_col()
            .gap(px(3.))
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(7.))
                    .children(Lane::NOTES.map(|l| lane_tab(l, cx))),
            )
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(7.))
                    .children(Lane::CONTROLLERS.map(|l| lane_tab(l, cx))),
            );
        // What the lane does, said where there's room for it.
        let lane_tip = match lane {
            Lane::Pitch => Some(
                "Draw here or on the notes (another lane moves them). A click adds a point, a double-click takes one out. ⌥ bows a line, ⌥-drag rubs out, ⇧ scales, ⌘ lands between semitones.",
            ),
            Lane::Slide | Lane::Press => Some(
                "A click adds a point, a drag draws, a double-click takes one out. ⌥ bows a line, ⌥-drag rubs out, ⇧ scales. The MPE tab has shapes.",
            ),
            _ => None,
        };
        // The lane's scale, at its right.
        let lane_marks: Vec<Div> = match lane.dim() {
            Some(dim) => {
                let marks: Vec<(f32, &str)> = match dim {
                    Dim::Pitch => vec![(12.0, "+12"), (0.0, "0"), (-12.0, "−12")],
                    _ => vec![(1.0, "100%"), (0.5, "50%"), (0.0, "0")],
                };
                let h = lane.height();
                marks
                    .into_iter()
                    .map(|(v, text)| {
                        let b = Bounds::new(point(px(0.), px(0.)), size(px(10.), px(h)));
                        div()
                            .absolute()
                            .right(px(6.))
                            .top(px(lane_y(b, lane_level(dim, v)) - 7.0))
                            .text_size(px(9.5))
                            .font_family(MONO)
                            .text_color(rgb(TEXT_4))
                            .child(text)
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        // What the hand on a note's line reads, by it.
        let readout = self.bend_readout.clone().map(|(p, text)| {
            let o = self.roll_bounds.get().origin;
            div()
                .absolute()
                .left(px(p.x - f32::from(o.x) + 12.))
                .top(px(p.y - f32::from(o.y) + ROLL_RULER - 26.))
                .px(px(6.))
                .py(px(1.))
                .rounded(px(4.))
                .bg(rgb(TEXT))
                .text_color(rgb(BG))
                .font_family(MONO)
                .text_size(px(10.5))
                .font_weight(FontWeight::SEMIBOLD)
                .whitespace_nowrap()
                .debug_selector(|| "bend-readout".into())
                .child(text)
        });
        let lane_bounds = self.lane_bounds.clone();
        let lane_view = canvas(
            move |b, _, _| lane_bounds.set(b),
            move |b, _, window, _| {
                if lane.per_note() {
                    return;
                }
                let (x0, y0) = (f32::from(b.origin.x), f32::from(b.origin.y));
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                let floor = y0 + h - 5.0;
                let y_of = |level: f32| floor - level * VELOCITY_H;
                // Each note's own: guides, a band along it at rest, and its line with its points
                // (the picked notes' bright, the rest faint).
                if let Some(dim) = lane.dim() {
                    let guide = |window: &mut Window, v: f32, strong: bool| {
                        let y = lane_y(b, lane_level(dim, v));
                        window.paint_quad(fill(
                            Bounds::new(point(px(x0), px(y)), size(px(w), px(1.))),
                            alpha(0xffffff, if strong { 0.12 } else { 0.035 }),
                        ));
                    };
                    match dim {
                        Dim::Pitch => {
                            for st in -12..=12 {
                                guide(window, st as f32, st % 12 == 0);
                            }
                        }
                        _ => {
                            for k in 0..=4 {
                                guide(window, k as f32 / 4.0, k % 2 == 0);
                            }
                        }
                    }
                    let rest = lane_y(b, lane_level(dim, dim.rest()));
                    for o in &owns {
                        let (a, z) = (o.from.max(0.0), o.to.min(w));
                        if z <= a {
                            continue;
                        }
                        window.paint_quad(fill(
                            Bounds::new(point(px(x0 + a), px(rest - 1.5)), size(px(z - a), px(3.))),
                            alpha(o.ink, if o.bright { 0.3 } else { 0.12 }),
                        ));
                        let points: Vec<(f32, f32, f32)> = o
                            .points
                            .iter()
                            .map(|&(x, v, bow)| (x0 + x, lane_y(b, lane_level(dim, v)), bow))
                            .collect();
                        let ink = match o.bright {
                            true => Hsla::from(rgb(o.ink)),
                            false => alpha(o.ink, 0.35),
                        };
                        paint_line(window, &points, (x0 + o.from, x0 + o.to), 2.0, ink);
                        dots(window, &points, o.ink, o.picked);
                    }
                    return;
                }
                if lane == Lane::Bend {
                    window.paint_quad(fill(
                        Bounds::new(point(px(x0), px(y_of(0.5))), size(px(w), px(1.))),
                        alpha(0xffffff, 0.12),
                    ));
                }
                for (k, &(x, level)) in curve.iter().enumerate() {
                    let next = curve.get(k + 1).map_or(w, |c| c.0).min(w);
                    let (left, right) = (x.max(0.0), next);
                    if right <= left {
                        continue;
                    }
                    let base = if lane == Lane::Bend { y_of(0.5) } else { floor };
                    let top = y_of(level);
                    let (a, bb) = (top.min(base), top.max(base));
                    window.paint_quad(fill(
                        Bounds::new(
                            point(px(x0 + left), px(a)),
                            size(px(right - left), px((bb - a).max(1.0))),
                        ),
                        alpha(color, 0.22),
                    ));
                    window.paint_quad(fill(
                        Bounds::new(
                            point(px(x0 + left), px(top - 1.0)),
                            size(px(right - left), px(2.)),
                        ),
                        rgb(color),
                    ));
                    if x >= 0.0 {
                        window.paint_quad(
                            fill(
                                Bounds::new(
                                    point(px(x0 + x - 2.0), px(top - 3.0)),
                                    size(px(5.), px(5.)),
                                ),
                                rgb(0xffffff),
                            )
                            .corner_radii(gpui::Corners::all(px(2.5))),
                        );
                    }
                }
            },
        )
        .absolute()
        .size_full();

        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .child(
                div()
                    .w(px(self.header_w))
                    .flex_none()
                    .h_full()
                    .flex()
                    .flex_col()
                    .bg(rgb(SURFACE))
                    .border_r_1()
                    .border_color(rgb(LINE))
                    .child(div().flex_1().min_h(px(0.)).flex().child(tools).child(keyboard))
                    .child(
                        div()
                            .h(px(lane.height()))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .justify_center()
                            .gap(px(4.))
                            .px(px(12.))
                            .border_t_1()
                            .border_color(rgb(LINE))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(div().font_weight(FontWeight::MEDIUM).child(lane.name()))
                                    .child(div().flex_1().text_size(px(11.)).text_color(rgb(TEXT_3)).whitespace_nowrap().overflow_hidden().child(
                                        match (lane, &curve_now) {
                                            (l, _) if l.per_note() => "drag a dot, or draw across".to_string(),
                                            (l, _) if l.expression() => "each note's own (MPE)".to_string(),
                                            (_, Some(v)) => format!("now {v} · draw, ⌥ to erase"),
                                            _ => "draw it, ⌥ to erase".to_string(),
                                        },
                                    )),
                            )
                            .when_some(lane_tip, |d, tip| {
                                d.child(div().text_size(px(10.5)).line_height(px(14.)).text_color(rgb(TEXT_3)).child(tip))
                            })
                            .child(lane_tabs),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .relative()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(ROLL_RULER))
                            .flex_none()
                            .relative()
                            .overflow_hidden()
                            .bg(rgb(WELL))
                            .border_b_1()
                            .border_color(rgb(LINE))
                            .cursor_pointer()
                            .children(bars)
                            .children(chords)
                            .debug_selector(|| "roll-ruler".into())
                            // Click the ruler to move the playhead there.
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                    let x = f32::from(e.position.x) - f32::from(ws.roll_bounds.get().origin.x);
                                    let tick = ((x + ws.scroll_x) / ws.px_per_tick()).max(0.0);
                                    let tick = ws.snap(tick as f64);
                                    ws.locate_tick(tick, cx);
                                }),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h(px(0.))
                            .relative()
                            .overflow_hidden()
                            .debug_selector(|| "roll-grid".into())
                            .child(grid)
                            .children(ghosts)
                            .children(notes)
                            .children(bent)
                            .children(band)
                            .children(tip)
                            .when(at.is_none(), |d| {
                                d.child(
                                    div()
                                        .absolute()
                                        .top(px(20.))
                                        .left(px(20.))
                                        .text_color(rgb(TEXT_3))
                                        .child("Select a MIDI region to edit its notes, or draw one with the pencil."),
                                )
                            })
                            .on_mouse_down(MouseButton::Left, cx.listener(|ws, e, _, cx| ws.roll_click(e, cx)))
                            .on_mouse_down(MouseButton::Right, cx.listener(|ws, e: &MouseDownEvent, _, cx| ws.roll_menu(e, None, cx)))
                            .on_mouse_move(cx.listener(|ws, e: &MouseMoveEvent, _, cx| ws.hover_roll(e, cx)))
                            .on_scroll_wheel(cx.listener(|ws, e, _, cx| ws.roll_scroll(e, cx))),
                    )
                    .child(
                        div()
                            .h(px(lane.height()))
                            .flex_none()
                            .relative()
                            .overflow_hidden()
                            .bg(rgb(WELL))
                            .border_t_1()
                            .border_color(rgb(LINE))
                            .debug_selector(|| "roll-lane".into())
                            .child(lane_view)
                            .children(stems)
                            .children(lane_marks)
                            .on_mouse_down(MouseButton::Left, cx.listener(|ws, e, _, cx| ws.lane_down(e, cx))),
                    )
                    .child(div().absolute().left(px(playhead)).top_0().w(px(1.)).h_full().bg(rgb(TEXT)))
                    .children(readout),
            )
    }
}
