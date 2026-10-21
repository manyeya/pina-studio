//! Track headers: name, icon, buttons, fader, pan and meters, a row per track.

use super::*;

/// A header's button: dark until on, then lit in its colour.
pub(crate) fn strip_button(label: &'static str, on: bool, glow: u32, ink: u32) -> Div {
    div()
        .w(px(24.))
        .h(px(18.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .border_1()
        .text_size(px(9.5))
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .when(on, |d| {
            d.bg(rgb(glow))
                .border_color(rgb(glow))
                .text_color(rgb(ink))
                .shadow(vec![BoxShadow {
                    color: alpha(glow, 0.55),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(8.),
                    spread_radius: px(0.),
                    inset: false,
                }])
        })
        .when(!on, |d| {
            d.bg(rgb(HEADERS))
                .border_color(rgb(RAISED))
                .text_color(rgb(0x6a6a75))
                .hover(|s| s.text_color(rgb(TEXT_2)))
        })
        .child(label)
}

/// The instrument's kind, in a word or two: for a track set in under its stack, where a name
/// won't fit.
pub(crate) fn instrument_kind(i: Option<&Instrument>) -> String {
    match i {
        Some(i @ Instrument::Sampler(_)) if i.is_kit() => "Kit".into(),
        Some(Instrument::Sampler(_)) => "Sampler".into(),
        _ => instrument_name(i),
    }
}

/// The instrument's name, under a track's.
pub fn instrument_name(i: Option<&Instrument>) -> String {
    match i {
        None => "Audio".into(),
        Some(Instrument::Synth) => "Sine Synth".into(),
        Some(Instrument::Drums) => "Drum Kit".into(),
        Some(Instrument::Sampler(s)) => sampler_name(s),
        Some(Instrument::Plugin(r)) => r.class.clone(),
    }
}

/// A pan knob, drawn as its mode pans: balance a dot round the dial; stereo pan the arc its two
/// sides span; binaural the dial with a head at its middle.
pub(crate) fn pan_dial(mode: PanMode, pan: f32, spread: f32, color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let c = b.center();
            let r = f32::from(b.size.width.min(b.size.height)) / 2.0 - 1.5;
            let at = |deg: f32, rr: f32| {
                point(
                    c.x + px(rr * deg.to_radians().cos()),
                    c.y + px(rr * deg.to_radians().sin()),
                )
            };
            let arc = |from: f32, to: f32, ink: Hsla, window: &mut Window| {
                if (to - from).abs() < 0.5 {
                    return;
                }
                let mut p = gpui::PathBuilder::stroke(px(2.));
                let steps = ((to - from).abs() / 6.0).ceil().max(2.0) as usize;
                for k in 0..=steps {
                    let d = from + (to - from) * k as f32 / steps as f32;
                    match k {
                        0 => p.move_to(at(d, r)),
                        _ => p.line_to(at(d, r)),
                    }
                }
                if let Ok(p) = p.build() {
                    window.paint_path(p, ink);
                }
            };
            let dot = |p: Point<Pixels>, d: f32, ink: Hsla, window: &mut Window| {
                window.paint_quad(
                    fill(Bounds::centered_at(p, size(px(d), px(d))), ink).corner_radii(px(d / 2.0)),
                );
            };
            let body = r * 0.62;
            let knob = |window: &mut Window| {
                window.paint_quad(
                    fill(
                        Bounds::centered_at(c, size(px(body * 2.0), px(body * 2.0))),
                        rgb(0x2e2e36),
                    )
                    .corner_radii(px(body))
                    .border_widths(px(1.))
                    .border_color(rgb(0x0b0b0e)),
                );
            };
            // -1 to 1 round the top, 135° either way; binaural all the way round.
            let deg = |p: f32| 270.0 + p.clamp(-1.0, 1.0) * 135.0;
            let ink: Hsla = rgb(color).into();
            match mode {
                PanMode::Binaural => {
                    arc(0.0, 360.0, rgb(0x26262d).into(), window);
                    knob(window);
                    dot(c, body * 0.7, rgb(0x4a4a58).into(), window);
                    dot(
                        at(270.0 + pan.clamp(-1.0, 1.0) * 180.0, r),
                        5.0,
                        ink,
                        window,
                    );
                }
                PanMode::Stereo => {
                    arc(deg(-1.0), deg(1.0), rgb(0x26262d).into(), window);
                    let (a, z) = (deg(pan - spread.abs()), deg(pan + spread.abs()));
                    arc(a.min(z), a.max(z), ink, window);
                    knob(window);
                    for d in [deg(pan - spread), deg(pan + spread)] {
                        let mut p = gpui::PathBuilder::stroke(px(1.6));
                        p.move_to(at(d, body * 0.35));
                        p.line_to(at(d, body * 0.85));
                        if let Ok(p) = p.build() {
                            window.paint_path(p, Hsla::from(rgb(TEXT)));
                        }
                    }
                }
                PanMode::Balance => {
                    arc(deg(-1.0), deg(1.0), rgb(0x26262d).into(), window);
                    let (a, z) = (deg(0.0), deg(pan));
                    arc(a.min(z), a.max(z), ink, window);
                    // The middle, marked.
                    dot(
                        at(270.0, r + 0.2),
                        2.4,
                        if pan.abs() < 0.005 {
                            ink
                        } else {
                            rgb(0x3a3a44).into()
                        },
                        window,
                    );
                    knob(window);
                    let mut p = gpui::PathBuilder::stroke(px(1.6));
                    p.move_to(at(deg(pan), body * 0.3));
                    p.line_to(at(deg(pan), body * 0.85));
                    if let Ok(p) = p.build() {
                        window.paint_path(p, Hsla::from(rgb(TEXT)));
                    }
                }
            }
        },
    )
    .size_full()
}

impl Workspace {
    pub(crate) fn headers(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let armed = self.armed();
        let mut rows: Vec<Div> = Vec::new();
        for i in 0..self.session.project.tracks.len() {
            if let Some(a) = self.stack_above(i) {
                rows.push(self.stack_row(a, i, cx));
            }
            if self.folded(i) {
                continue;
            }
            let selected = self.is_picked(i);
            let color = self.color(i);
            let glyph = self.shown(i).look.icon;
            let picking = self.picker.as_ref().is_some_and(|p| p.track == i);
            // In a stack, it's set in under the stack's row, in the stack's colour.
            let nested = self.stack_color(i);
            let top = self.row_top(i);
            rows.push(self.header_row(i, top, selected, color, glyph, picking, nested, armed, cx));
            if self.show_automation {
                rows.extend(self.auto_headers(i, cx));
            }
        }
        let width = self.header_w;
        div()
            .w(px(self.header_w))
            .flex_none()
            .h_full()
            .relative()
            .overflow_hidden()
            .bg(rgb(HEADERS))
            .border_r_1()
            .border_color(rgb(LINE))
            .children(rows)
            // Its right edge: drag the column wider or narrower; double-click for its usual width.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right(px(-1.))
                    .w(px(6.))
                    .h_full()
                    .cursor_col_resize()
                    .hover(|d| d.bg(alpha(ACCENT, 0.35)))
                    .when(
                        matches!(self.drag, Some(Drag::Width { side: false, .. })),
                        |d| d.bg(alpha(ACCENT, 0.6)),
                    )
                    .debug_selector(|| "headers-edge".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.click_count == 2 {
                                ws.header_w = HEADER_W;
                                cx.notify();
                                return;
                            }
                            ws.begin_drag(
                                Drag::Width {
                                    side: false,
                                    x: f32::from(e.position.x),
                                    width,
                                },
                                cx,
                            );
                        }),
                    ),
            )
            .on_scroll_wheel(self.on_scroll(cx))
    }

    /// A track's header, as a channel strip: its colour down its edge and its number; its icon
    /// (click for its colour and icon) and name (double-click to rename) over what it plays;
    /// mute, solo and record; and, tall enough, its fader with its meter running inside it, its
    /// level, and its pan knob, drawn as its mode pans (balance, a spread, around the head):
    /// right-click it for the modes and the pan law. Right-click the header for everything else;
    /// drag its bottom edge to make the track taller or shorter.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn header_row(
        &mut self,
        i: usize,
        top: f32,
        selected: bool,
        color: u32,
        glyph: &'static str,
        picking: bool,
        nested: Option<u32>,
        armed: Option<usize>,
        cx: &mut Context<Self>,
    ) -> Div {
        let lh = self.lane_h(i);
        let t = &self.session.project.tracks[i];
        // Where its automation has them, if it plays any.
        let (name, mute, solo, volume, pan, mode, spread) = (
            t.name.clone(),
            self.muted_now(i),
            t.solo,
            self.now_value(i, Param::Volume),
            self.now_value(i, Param::Pan),
            t.pan_mode,
            t.spread.unwrap_or(1.0),
        );
        let kind = match nested {
            Some(_) => instrument_kind(t.instrument.as_ref()),
            None => instrument_name(t.instrument.as_ref()),
        };
        let level = self.levels.get(&Some(t.id)).copied().unwrap_or_default();
        let shown = self.settings.clone();
        let roomy = lh >= 64.0 && shown.header_strip;
        let pos = strip::fader_pos(volume);
        let pan_text = {
            let side = strip::pan_label;
            match mode {
                PanMode::Balance => side(pan),
                PanMode::Stereo => format!(
                    "{}↔{:.0}",
                    if pan.abs() < 0.005 {
                        String::new()
                    } else {
                        side(pan)
                    },
                    spread * 100.0
                ),
                PanMode::Binaural => crate::editors::binaural::azimuth_label(pan),
            }
        };
        // The readout shows the level, or the pan while it's turned.
        let panning = matches!(
            self.drag,
            Some(Drag::Pan { channel: Channel::Track(t), .. } | Drag::Spread { track: t, .. }) if t == i
        );
        let readout_text = match panning {
            true => pan_text,
            false if volume <= strip::SILENT + 0.01 => "−∞".to_string(),
            false => format!("{volume:+.1}").replace('-', "−"),
        };
        // Automated, the readout lights in its mode's colour.
        let marked = match panning {
            true => self.auto_mark(i, Param::Pan),
            false => self.auto_mark(i, Param::Volume),
        };
        let lit = marked.unwrap_or(tint(color, 0.55));

        let button = strip_button;
        let buttons = div()
            .flex()
            .gap(px(4.))
            .child(
                button("M", mute, ACCENT, 0xffffff)
                    .debug_selector(move || format!("mute-{i}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit(cx, |ws| {
                                let m = !ws.muted_now(i);
                                ws.session.project.tracks[i].mute = m;
                                ws.moved_once(i, Param::Mute, m as u8 as f32);
                            });
                        }),
                    ),
            )
            .child(
                button("S", solo, SOLO, BG)
                    .debug_selector(move || format!("solo-{i}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit(cx, |ws| ws.session.project.tracks[i].solo ^= true);
                        }),
                    ),
            )
            .when(shown.header_record, |d| {
                d.child(
                    button("R", armed == Some(i), RECORD, 0xffffff)
                        .debug_selector(move || format!("arm-{i}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.toggle_arm(i);
                                cx.notify();
                            }),
                        ),
                )
            });
        // The level, left over right, as LEDs: green to -7 dB, amber, then red at 0.
        let (now, held) = (level.now, level.held);
        let leds = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                const AT: [f32; 12] = [
                    -42.0, -36.0, -30.0, -24.0, -18.0, -14.0, -10.0, -7.0, -4.5, -2.5, -1.0, 0.0,
                ];
                let w = f32::from(b.size.width);
                let seg = (w - 11.0) / 12.0;
                for (row, (l, h)) in [(now[0], held[0]), (now[1], held[1])]
                    .into_iter()
                    .enumerate()
                {
                    let (db, peak) = (20.0 * l.max(1e-9).log10(), 20.0 * h.max(1e-9).log10());
                    let top = (AT.iter().rposition(|&a| peak >= a)).filter(|_| h > 1e-6);
                    for (k, &at) in AT.iter().enumerate() {
                        let on = db >= at || top == Some(k);
                        let ink = match (on, k) {
                            (false, _) => rgb(0x2a2a31),
                            (true, 0..=7) => rgb(METER_LOW),
                            (true, 8..=9) => rgb(METER_MID),
                            _ => rgb(METER_HIGH),
                        };
                        window.paint_quad(fill(
                            Bounds::new(
                                point(
                                    b.origin.x + px(k as f32 * (seg + 1.0)),
                                    b.origin.y + px(row as f32 * 5.0),
                                ),
                                size(px(seg), px(3.)),
                            ),
                            ink,
                        ));
                    }
                }
            },
        )
        .flex_1()
        .h(px(8.));

        // The fader: a groove filled in the track's colour, a scale under it, and a ridged
        // cap. Drag it (⇧ for fine), swipe sideways over it, double-click to type a level,
        // ⌥-click for 0 dB.
        const FADER_W: f32 = 98.0;
        let ticks = [strip::SILENT, -24.0, -12.0, -6.0, 0.0, 6.0].map(|db| {
            div()
                .absolute()
                .left(px(
                    (FADER_W * strip::fader_pos(db) - 0.5).clamp(0.0, FADER_W - 1.0)
                ))
                .top(px(16.))
                .w(px(1.))
                .h(px(4.))
                .bg(rgb(if db == 0.0 { TEXT_3 } else { 0x3a3a44 }))
        });
        let fader = div()
            .id(("fader", i))
            .w(px(FADER_W))
            .h(px(22.))
            .flex_none()
            .relative()
            .cursor_ew_resize()
            .debug_selector(move || format!("track-fader-{i}"))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(9.))
                    .h(px(3.))
                    .rounded(px(1.5))
                    .bg(rgb(0x0b0b0e)),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(9.))
                    .w(px(FADER_W * pos))
                    .h(px(3.))
                    .rounded(px(1.5))
                    .bg(alpha(color, if mute { 0.35 } else { 0.8 })),
            )
            .children(ticks)
            .child(self.modded_fader(i, FADER_W, 22.))
            // The cap, as the mixer's are, stood on end.
            .child(
                div()
                    .absolute()
                    .left(px((FADER_W * pos - 7.0).clamp(0.0, FADER_W - 14.0)))
                    .top(px(1.))
                    .w(px(14.))
                    .h(px(20.))
                    .rounded(px(4.))
                    .bg(rgb(0x3a3a44))
                    .border_1()
                    .border_color(rgb(0x4e4e5a))
                    .shadow_md()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().w(px(1.)).h(px(12.)).bg(rgb(TEXT))),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    ws.select_track(i);
                    if e.modifiers.alt {
                        ws.edit(cx, |ws| {
                            ws.set_volume(Channel::Track(i), 0.0);
                            ws.moved_once(i, Param::Volume, 0.0);
                        });
                        return;
                    }
                    if e.click_count == 2 {
                        ws.start_naming(Name::Volume(i), window, cx);
                        return;
                    }
                    let pos = strip::fader_pos(ws.now_value(i, Param::Volume));
                    let x = f32::from(e.position.x);
                    ws.begin_drag(
                        Drag::HFader {
                            track: i,
                            x,
                            pos,
                            width: FADER_W,
                        },
                        cx,
                    );
                }),
            )
            .on_scroll_wheel(swipe(Name::Volume(i), cx));
        // The readout, an LCD: the level (double-click to type one), or the pan while it's
        // turned; a pan typed shows here too.
        let field = self
            .name_field(Name::Volume(i))
            .or_else(|| self.name_field(Name::Pan(i)));
        let readout = div()
            .w(px(40.))
            .h(px(18.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .bg(rgb(0x0b0b0e))
            .border_1()
            .border_color(match marked {
                Some(c) => alpha(c, 0.45),
                None => rgb(0x22222a).into(),
            })
            .font_family(MONO)
            .text_size(px(10.))
            .text_color(rgb(lit))
            .whitespace_nowrap()
            .overflow_hidden()
            .debug_selector(move || format!("track-db-{i}"))
            .child(match field {
                Some(field) => div().w_full().px(px(2.)).child(field),
                None => div().child(readout_text),
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                    if e.click_count == 2 {
                        cx.stop_propagation();
                        ws.start_naming(Name::Volume(i), window, cx);
                    }
                }),
            );
        // The pan knob, its LED ring showing where: drag up or right to turn it right (⇧ fine;
        // ⌘ spreads a stereo pan), swipe sideways, double-click to type it (binaural: its
        // panner), ⌥-click for the middle, right-click for its mode and the pan law.
        let pan_knob = div()
            .id(("pan", i))
            .size(px(24.))
            .flex_none()
            .cursor_ns_resize()
            .debug_selector(move || format!("track-pan-{i}"))
            .child(self.modded(
                Some((i, pina_engine::dsp::modulate::ModTarget::Pan)),
                24.,
                pan_dial(mode, pan, spread, color),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    ws.select_track(i);
                    let shown_pan = ws.now_value(i, Param::Pan);
                    let t = &ws.session.project.tracks[i];
                    if e.modifiers.alt {
                        ws.edit(cx, |ws| {
                            ws.set_pan(Channel::Track(i), 0.0);
                            ws.moved_once(i, Param::Pan, 0.0);
                            let t = &mut ws.session.project.tracks[i];
                            (t.spread, t.elevation, t.distance) = (None, None, None);
                        });
                        return;
                    }
                    if e.click_count == 2 {
                        match t.pan_mode {
                            PanMode::Binaural => ws.open_panner(i, cx),
                            _ => ws.start_naming(Name::Pan(i), window, cx),
                        }
                        return;
                    }
                    let (x, y) = (f32::from(e.position.x), f32::from(e.position.y));
                    let drag = match (e.modifiers.platform, t.pan_mode) {
                        (true, PanMode::Stereo) => Drag::Spread {
                            track: i,
                            x,
                            y,
                            spread: t.spread.unwrap_or(1.0),
                        },
                        _ => Drag::Pan {
                            channel: Channel::Track(i),
                            x,
                            y,
                            pan: shown_pan,
                        },
                    };
                    ws.begin_drag(drag, cx);
                    cx.notify();
                }),
            )
            .on_scroll_wheel(swipe(Name::Pan(i), cx))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.pan_menu(i, e.position, cx);
                }),
            );

        // The strip: a raised panel, its buttons and LEDs over its fader, readout and knob.
        let strip_panel = div()
            .w(px(190.))
            .h(px(if roomy { 60. } else { 28. }))
            .flex_none()
            .mr(px(8.))
            .px(px(8.))
            .py(px(if roomy { 6. } else { 4. }))
            .flex()
            .flex_col()
            .justify_between()
            .rounded(px(8.))
            .border_1()
            .border_color(rgb(RAISED))
            .bg(linear_gradient(
                180.,
                linear_color_stop(rgb(0x1e1e24), 0.),
                linear_color_stop(rgb(0x17171c), 1.),
            ))
            .shadow(vec![BoxShadow {
                color: alpha(0xffffff, 0.05),
                offset: point(px(0.), px(1.)),
                blur_radius: px(0.),
                spread_radius: px(0.),
                inset: true,
            }])
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(buttons)
                    .when(shown.header_meter, |d| d.child(leds)),
            )
            .when(roomy, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(fader)
                        .child(readout)
                        .child(pan_knob),
                )
            });

        // Who it is: its icon (click for its colour and icon), number and name (double-click to
        // rename), and what it plays.
        let icon_size = if roomy { 36. } else { 26. };
        let identity = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .items_center()
            .gap(px(10.))
            .pl(px(if nested.is_some() { 25. } else { 13. }))
            .pr(px(8.))
            .when(mute, |d| d.opacity(0.55))
            .when(shown.header_icons, |d| {
                d.child(
                    div()
                        .id(("track-look", i))
                        .size(px(icon_size))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(icon_size / 4.0))
                        .bg(alpha(color, 0.14))
                        .border_1()
                        .border_color(alpha(color, if picking { 0.8 } else { 0.34 }))
                        .cursor_pointer()
                        .hover(|d| d.bg(alpha(color, 0.26)))
                        .child(
                            crate::ui::looks::glyph(glyph, icon_size * 0.52).text_color(rgb(color)),
                        )
                        .debug_selector(move || format!("track-look-{i}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                ws.select_track(i);
                                let at = e.position + gpui::point(gpui::px(24.), gpui::px(-12.));
                                ws.open_picker(i, at, window, cx);
                            }),
                        ),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(6.))
                            .min_w(px(0.))
                            .when(shown.header_numbers, |d| {
                                d.child(
                                    div()
                                        .flex_none()
                                        .font_family(MONO)
                                        .text_size(px(10.))
                                        .text_color(rgb(if selected { TEXT_3 } else { TEXT_4 }))
                                        .child(format!("{:02}", i + 1)),
                                )
                            })
                            .child(match self.name_field(Name::Track(i)) {
                                Some(field) => div().flex_1().child(field),
                                None => div()
                                    .min_w(px(0.))
                                    .text_size(px(13.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .line_height(px(17.))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .text_color(rgb(if selected { 0xffffff } else { 0xe6e6ea }))
                                    .child(name)
                                    .debug_selector(move || format!("track-name-{i}"))
                                    // Double-click the name to rename the track.
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                                            if e.click_count == 2 {
                                                cx.stop_propagation();
                                                ws.select_track(i);
                                                ws.start_naming(Name::Track(i), window, cx);
                                            }
                                        }),
                                    ),
                            }),
                    )
                    .when(lh >= 52.0 && shown.header_kind, |d| {
                        d.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .text_size(px(10.5))
                                .line_height(px(13.))
                                .text_color(rgb(TEXT_3))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .child(
                                    div()
                                        .min_w(px(0.))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(kind),
                                ),
                        )
                    }),
            );

        let height = lh;
        div()
            .absolute()
            .top(px(top))
            .left_0()
            .w_full()
            .h(px(lh))
            .flex()
            .items_center()
            .bg(rgb(if selected { HEADER_SELECTED } else { HEADERS }))
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .debug_selector(move || format!("header-{i}"))
            // Its colour down its edge.
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(3.))
                    .bg(alpha(color, if selected { 0.95 } else { 0.5 })),
            )
            // Its stack's, down from the stack's row.
            .when_some(nested, |d, c| {
                d.child(
                    div()
                        .absolute()
                        .left(px(12.))
                        .top(px(-1.))
                        .bottom(px(-1.))
                        .w(px(2.))
                        .bg(alpha(c, 0.55)),
                )
            })
            .child(identity)
            .child(strip_panel)
            // The bottom edge: drag the track taller or shorter; double-click for its usual size.
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(px(-2.))
                    .h(px(6.))
                    .cursor_row_resize()
                    .hover(|d| d.bg(alpha(ACCENT, 0.35)))
                    .debug_selector(move || format!("track-edge-{i}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.click_count == 2 {
                                ws.session.project.tracks[i].height = None;
                                ws.session.edited = true;
                                cx.notify();
                                return;
                            }
                            ws.begin_drag(
                                Drag::LaneHeight {
                                    track: i,
                                    y: f32::from(e.position.y),
                                    height,
                                },
                                cx,
                            );
                        }),
                    ),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    // Within the selection, it stays selected: the menu acts on all of it.
                    match ws.is_picked(i) {
                        true => {
                            ws.track = i;
                            ws.channel = Channel::Track(i);
                        }
                        false => ws.select_track(i),
                    }
                    ws.track_menu(i, e.position, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    let m = e.modifiers;
                    match m.platform || m.shift {
                        true => ws.pick_track(i, m.shift),
                        false => ws.select_track(i),
                    }
                    ws.region = None;
                    cx.notify();
                }),
            )
    }
}
