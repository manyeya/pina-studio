//! The sampler editor itself: zones, the waveform and its marks, drawn.

use super::*;

impl Workspace {
    pub fn sampler_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(t) = self.sampler_track() else {
            return self.sampler_none(cx).into_any_element();
        };
        let s = self.sampler_of(t).cloned().unwrap_or_default();
        if s.zones.is_empty() {
            return self.sampler_empty(cx).into_any_element();
        }
        let picked = self.sampler_ui.zone.min(s.zones.len() - 1);
        self.sampler_ui.zone = picked;
        let z = s.zones[picked].clone();
        let wave = self.wave(&z.file, cx);
        let key = self.session.project.song_key();
        let color = zone_color(picked);
        let track_id = self.session.project.tracks[t].id;

        // The zones, down the left.
        let rows = s.zones.iter().enumerate().map(|(i, zone)| {
            let on = i == picked;
            let range = match zone.high {
                Some(h) => format!("{}–{}", name_in(key, zone.note), name_in(key, h)),
                None => name_in(key, zone.note),
            };
            let vel = zone
                .velocities
                .map(|(a, b)| format!(" · v{a}–{b}"))
                .unwrap_or_default();
            div()
                .id(("zone", i))
                .debug_selector(move || format!("zone-{i}"))
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(8.))
                .py(px(5.))
                .rounded(px(7.))
                .cursor_pointer()
                .when(on, |d| d.bg(rgb(RAISED)))
                .when(!on, |d| d.hover(|d| d.bg(rgb(OFF))))
                .child(
                    div()
                        .size(px(8.))
                        .flex_none()
                        .rounded(px(2.))
                        .bg(rgb(zone_color(i))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(12.))
                        .text_color(rgb(if on { TEXT } else { TEXT_2 }))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(zone_name(zone)),
                )
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(10.))
                        .text_color(rgb(TEXT_3))
                        .whitespace_nowrap()
                        .child(format!("{range}{vel}")),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.sampler_ui.zone = i;
                        let pitch = ws.sampler_of(t).and_then(|s| s.zones.get(i)).map(|z| {
                            z.root
                                .unwrap_or(z.note)
                                .clamp(z.note, z.high.unwrap_or(z.note))
                        });
                        if let Some(p) = pitch {
                            ws.player.preview(track_id, p, 100);
                            ws.begin_drag(
                                Drag::Key {
                                    track: track_id,
                                    pitch: p,
                                },
                                cx,
                            );
                        }
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |ws, _, _, cx| {
                        ws.sampler_ui.zone = 0;
                        ws.edit_sampler(t, cx, |s| {
                            if i < s.zones.len() {
                                s.zones.remove(i);
                            }
                        });
                        ws.status = Some("Zone removed (⌘Z puts it back)".into());
                    }),
                )
        });
        let left = div()
            .w(px(self.header_w))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .px(px(12.))
                    .pt(px(10.))
                    .pb(px(6.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT_3))
                            .child(format!("ZONES · {}", s.zones.len())),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_4))
                            .child("right-click removes"),
                    ),
            )
            .child(
                div()
                    .id("zones")
                    .flex_1()
                    .min_h(px(0.))
                    .px(px(6.))
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .overflow_y_scroll()
                    .children(rows),
            )
            .child(
                div()
                    .m(px(8.))
                    .h(px(40.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(9.))
                    .border_1()
                    .border_dashed()
                    .border_color(if self.file_drag.is_some() {
                        rgb(ACCENT).into()
                    } else {
                        alpha(0xffffff, 0.14)
                    })
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT_3))
                    .cursor_pointer()
                    .hover(|d| d.bg(alpha(0xffffff, 0.04)))
                    .child("Drop samples, or click")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.pick_samples(cx)),
                    ),
            );

        // The waveform, with the zone's marks and the other slices of its sample.
        let siblings: Vec<(usize, f32, u8)> = s
            .zones
            .iter()
            .enumerate()
            .filter(|(i, o)| *i != picked && o.file == z.file)
            .map(|(i, o)| (i, o.start, o.note))
            .collect();
        let (start, end) = (z.start, z.end.unwrap_or(1.0));
        let looping = z.loop_mode != LoopMode::Off;
        let (ls, le) = (z.loop_start.unwrap_or(start), z.loop_end.unwrap_or(end));
        let wave_for_paint = wave.clone();
        let reverse = z.reverse;
        let drawn = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (x0, y0) = (f32::from(b.origin.x), f32::from(b.origin.y));
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                let quad = |window: &mut Window, x: f32, y: f32, ww: f32, hh: f32, c: Hsla| {
                    window.paint_quad(fill(
                        Bounds::new(point(px(x), px(y)), size(px(ww), px(hh))),
                        c,
                    ));
                };
                quad(window, x0, y0 + h / 2.0, w, 1.0, alpha(0xffffff, 0.06));
                let Some(wave) = &wave_for_paint else { return };
                let mid = y0 + h / 2.0;
                let cols = w.max(1.0) as usize;
                for c in 0..cols {
                    let (a, bb) = (
                        c * wave.len / cols,
                        ((c + 1) * wave.len / cols).max(c * wave.len / cols + 1),
                    );
                    let (lo, hi) = wave.peaks.range(a, bb);
                    let frac = c as f32 / cols as f32;
                    let inside = frac >= start && frac <= end;
                    let ink = if inside {
                        alpha(color, 0.85)
                    } else {
                        alpha(0xffffff, 0.16)
                    };
                    let (top, bottom) =
                        (mid - hi.max(0.0) * h * 0.46, mid - lo.min(0.0) * h * 0.46);
                    quad(
                        window,
                        x0 + c as f32,
                        top,
                        1.0,
                        (bottom - top).max(1.0),
                        ink,
                    );
                }
                // Outside what plays, shaded; the loop lit.
                quad(window, x0, y0, w * start, h, alpha(0x000000, 0.35));
                quad(
                    window,
                    x0 + w * end,
                    y0,
                    w * (1.0 - end),
                    h,
                    alpha(0x000000, 0.35),
                );
                if looping {
                    quad(
                        window,
                        x0 + w * ls,
                        y0,
                        w * (le - ls),
                        h,
                        alpha(color, 0.08),
                    );
                    quad(
                        window,
                        x0 + w * ls,
                        y0,
                        w * (le - ls),
                        3.0,
                        rgb(color).into(),
                    );
                }
                let _ = reverse;
            },
        )
        .absolute()
        .size_full();
        let mark = |ws: &Self,
                    at: f32,
                    which: Mark,
                    label: &'static str,
                    tone: u32,
                    cx: &mut Context<Self>| {
            let _ = ws;
            div()
                .absolute()
                .left(gpui::relative(at))
                .top_0()
                .bottom_0()
                .w(px(9.))
                .ml(px(-4.))
                .cursor_ew_resize()
                .debug_selector(move || format!("mark-{which:?}"))
                .child(
                    div()
                        .absolute()
                        .left(px(4.))
                        .top_0()
                        .bottom_0()
                        .w(px(1.5))
                        .bg(rgb(tone)),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(4.))
                        .top(px(if matches!(which, Mark::LoopStart | Mark::LoopEnd) {
                            4.
                        } else {
                            0.
                        }))
                        .px(px(4.))
                        .rounded_r(px(3.))
                        .bg(rgb(tone))
                        .text_size(px(9.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(BG))
                        .child(label),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let b = ws.sampler_wave_bounds.get();
                        let (left, width) = (f32::from(b.origin.x), f32::from(b.size.width));
                        let _ = e;
                        ws.begin_drag(
                            Drag::Mark {
                                track: t,
                                zone: picked,
                                mark: which,
                                left,
                                width,
                            },
                            cx,
                        );
                    }),
                )
        };
        let wave_bounds = self.sampler_wave_bounds.clone();
        let header = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(div().size(px(9.)).rounded(px(2.)).bg(rgb(color)))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(zone_name(&z)),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .child(match &wave {
                        Some(w) => format!(
                            "{:.2} s · plays {:.0}–{:.0}%",
                            w.seconds,
                            start * 100.0,
                            end * 100.0
                        ),
                        None => "reading…".into(),
                    }),
            )
            .child(div().flex_1())
            .child(self.zone_switches(t, picked, &z, cx));
        let wave_view = div()
            .flex_1()
            .min_h(px(90.))
            .relative()
            .rounded(px(10.))
            .bg(rgb(WELL))
            .border_1()
            .border_color(rgb(LINE))
            .overflow_hidden()
            .debug_selector(|| "sampler-wave".into())
            .child(
                canvas(move |b, _, _| wave_bounds.set(b), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(drawn)
            .children(siblings.iter().map(|&(i, at, note)| {
                div()
                    .absolute()
                    .left(gpui::relative(at))
                    .top_0()
                    .bottom_0()
                    .w(px(1.))
                    .bg(alpha(zone_color(i), 0.7))
                    .child(
                        div()
                            .absolute()
                            .left(px(3.))
                            .bottom(px(3.))
                            .font_family(MONO)
                            .text_size(px(9.))
                            .whitespace_nowrap()
                            .text_color(rgb(zone_color(i)))
                            .child(name_in(None, note)),
                    )
            }))
            .child(mark(self, start, Mark::Start, "S", MINT, cx))
            .child(mark(self, end, Mark::End, "E", CORAL, cx))
            .when(looping, |d| {
                d.child(mark(self, ls, Mark::LoopStart, "L", AMBER, cx))
                    .child(mark(self, le, Mark::LoopEnd, "L", AMBER, cx))
            });

        // The keys, each zone over the notes it plays; click one to hear it.
        let keymap = self.sampler_keys(t, &s, picked, cx);

        // How it plays.
        let card = |title: &'static str, side: Option<Div>| {
            div()
                .flex_none()
                .px(px(10.))
                .py(px(7.))
                .rounded(px(10.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(LINE))
                .flex()
                .flex_col()
                .gap(px(5.))
                .child(
                    div()
                        .h(px(20.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .text_size(px(10.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(TEXT_3))
                                .child(title),
                        )
                        .child(div().flex_1())
                        .children(side),
                )
        };
        let row = || div().flex().items_start().gap(px(2.));
        let zone_card = card("ZONE", None).child(
            row()
                .child(self.sampler_knob(
                    t,
                    Field::Note(picked),
                    if z.high.is_some() { "Low" } else { "Note" },
                    color,
                    cx,
                ))
                .when(z.high.is_some(), |d| {
                    d.child(self.sampler_knob(t, Field::High(picked), "High", color, cx))
                        .child(self.sampler_knob(t, Field::Root(picked), "Root", color, cx))
                })
                .child(self.sampler_knob(t, Field::Tune(picked), "Fine", color, cx))
                .child(self.sampler_knob(t, Field::Gain(picked), "Level", color, cx))
                .child(self.sampler_knob(t, Field::Pan(picked), "Pan", color, cx))
                .child(self.sampler_knob(t, Field::VelLow(picked), "Vel lo", color, cx))
                .child(self.sampler_knob(t, Field::VelHigh(picked), "Vel hi", color, cx)),
        );
        let v = s.voice;
        let amp_card = card(
            "AMP",
            Some(envelope(v.attack_ms, v.decay_ms, v.sustain, v.release_ms, MINT).w(px(110.))),
        )
        .child(
            row()
                .child(self.sampler_knob(t, Field::Attack, "Attack", MINT, cx))
                .child(self.sampler_knob(t, Field::Decay, "Decay", MINT, cx))
                .child(self.sampler_knob(t, Field::Sustain, "Sustain", MINT, cx))
                .child(self.sampler_knob(t, Field::Release, "Release", MINT, cx))
                .child(self.sampler_knob(t, Field::Velocity, "Velocity", MINT, cx)),
        );
        let modes = [
            (FilterMode::Off, "Off"),
            (FilterMode::LowPass, "LP"),
            (FilterMode::HighPass, "HP"),
            (FilterMode::BandPass, "BP"),
        ]
        .map(|(m, label)| {
            seg(label, v.filter == m, 17.)
                .px(px(7.))
                .text_size(px(10.))
                .debug_selector(move || format!("filter-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.edit_sampler(t, cx, |s| s.voice.filter = m)),
                )
        });
        let filter_card = card(
            "FILTER",
            Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(tray().p(px(1.)).rounded(px(6.)).children(modes))
                    .child(
                        envelope(
                            v.filter_attack_ms,
                            v.filter_decay_ms,
                            v.filter_sustain,
                            v.filter_release_ms,
                            SKY,
                        )
                        .w(px(110.)),
                    ),
            ),
        )
        .child(
            row()
                .child(self.sampler_knob(t, Field::Cutoff, "Cutoff", SKY, cx))
                .child(self.sampler_knob(t, Field::Resonance, "Reso", SKY, cx))
                .child(self.sampler_knob(t, Field::FilterEnv, "Env", SKY, cx))
                .child(self.sampler_knob(t, Field::FilterAttack, "F att", SKY, cx))
                .child(self.sampler_knob(t, Field::FilterDecay, "F dec", SKY, cx))
                .child(self.sampler_knob(t, Field::FilterSustain, "F sus", SKY, cx))
                .child(self.sampler_knob(t, Field::FilterRelease, "F rel", SKY, cx))
                .child(self.sampler_knob(t, Field::KeyTrack, "Key trk", SKY, cx)),
        );
        let pitch_card = card("PITCH & LFO", None).child(
            row()
                .child(self.sampler_knob(t, Field::Transpose, "Transpose", VIOLET, cx))
                .child(self.sampler_knob(t, Field::Glide, "Glide", VIOLET, cx))
                .child(self.sampler_knob(t, Field::LfoRate, "LFO rate", VIOLET, cx))
                .child(self.sampler_knob(t, Field::Vibrato, "Vibrato", VIOLET, cx))
                .child(self.sampler_knob(t, Field::LfoFilter, "Wah", VIOLET, cx))
                .child(self.sampler_knob(t, Field::Tremolo, "Tremolo", VIOLET, cx))
                .child(self.sampler_knob(t, Field::Volume, "Volume", VIOLET, cx)),
        );
        let params = div()
            .flex_none()
            .flex()
            .flex_wrap()
            .gap(px(8.))
            .child(zone_card)
            .child(amp_card)
            .child(filter_card)
            .child(pitch_card);

        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .bg(rgb(LANE))
            .child(left)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(header)
                    .child(wave_view)
                    .child(keymap)
                    .child(params),
            )
            .into_any_element()
    }
}
