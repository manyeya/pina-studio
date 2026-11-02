//! The sampler editor laid out: the zones and the picked one's settings down the left; its mode,
//! character and switches along the top; its sample, the keys, and its modules.

use super::*;

/// The zones' column's width.
const ZONES_W: f32 = 224.0;

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
        let v = s.voice;
        let left = self.zone_column(t, &s, picked, &wave, cx);
        let header = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .child(self.sampler_choices(
                t,
                "sampler-mode",
                &SampleMode::ALL.map(|m| (m.name(), m)),
                v.mode,
                VIOLET,
                cx,
                |s, m| s.voice.mode = m,
            ))
            .when(v.mode == SampleMode::Grains, |d| {
                d.child(self.sampler_value(t, Field::GrainSpread, "Width", VIOLET, cx))
                    .child(self.sampler_value(t, Field::GrainShape, "Shape", VIOLET, cx))
            })
            .child(self.character_controls(t, &v, cx))
            .child(div().flex_1())
            .child(self.zone_switches(t, picked, &z, cx));
        let wave_view = self.sampler_wave(t, &s, picked, wave, cx);
        let keymap = self.sampler_keys(t, &s, picked, cx);
        let modules = div()
            .id("sampler-modules")
            .h(px(194.))
            .flex_none()
            .flex()
            .gap(px(8.))
            .overflow_x_scroll()
            .child(self.play_module(t, &v, cx))
            .child(self.filter_module(t, &v, cx))
            .child(self.amp_module(t, cx))
            .child(self.mod_module(t, &v, cx));
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
                    .child(modules),
            )
            .into_any_element()
    }

    /// The character: clean, tape (its warmth) or vintage (its rate, bits, output filter, and
    /// a menu of the classic ones).
    fn character_controls(&self, t: usize, v: &SamplerVoice, cx: &mut Context<Self>) -> Div {
        let picker = self.sampler_choices(
            t,
            "sampler-character",
            &SampleCharacter::ALL.map(|c| (c.name(), c)),
            v.character,
            CORAL,
            cx,
            |s, c| s.voice.character = c,
        );
        let extra = match v.character {
            SampleCharacter::Clean => div(),
            SampleCharacter::Tape => {
                div().child(self.sampler_value(t, Field::Color, "Warmth", CORAL, cx))
            }
            SampleCharacter::Vintage => div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(
                    div()
                        .h(px(26.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .rounded(px(7.))
                        .bg(rgb(FIELD))
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(RAISED)))
                        .text_size(px(11.))
                        .text_color(rgb(CORAL))
                        .debug_selector(|| "sampler-vintage".into())
                        .child("Classics")
                        .child(icon(Icon::Chevron, 9.).text_color(rgb(TEXT_4)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                let items = [
                                    ("Boom bap drum machine · 26 kHz, 12-bit", 26.04, 12, false),
                                    ("Beat machine · 40 kHz, 12-bit", 40.0, 12, false),
                                    ("Rack sampler, dark · 32 kHz, 12-bit", 32.0, 12, true),
                                    ("Early digital · 22 kHz, 8-bit", 22.05, 8, true),
                                    ("Toy · 11 kHz, 6-bit", 11.0, 6, false),
                                ]
                                .map(|(label, khz, bits, filter)| {
                                    let op = SamplerOp::Vintage {
                                        track: t,
                                        khz,
                                        bits,
                                        filter,
                                    };
                                    (label.to_string(), MenuAction::Sampler(op))
                                })
                                .to_vec();
                                ws.menu = Some(Menu {
                                    at: e.position,
                                    items,
                                    ..Default::default()
                                });
                                cx.notify();
                            }),
                        ),
                )
                .child(self.sampler_value(t, Field::CrushKhz, "Rate", CORAL, cx))
                .child(self.sampler_value(t, Field::Bits, "Bits", CORAL, cx))
                .child(
                    pill("Smooth", v.crush_filter, CORAL)
                        .debug_selector(|| "sampler-crush-filter".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.edit_sampler(t, cx, |s| s.voice.crush_filter ^= true);
                            }),
                        ),
                ),
        };
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(picker)
            .child(extra)
    }

    /// The zones, the picked one's settings under them, and where to drop more.
    fn zone_column(
        &mut self,
        t: usize,
        s: &Sampler,
        picked: usize,
        wave: &Option<Arc<Wave>>,
        cx: &mut Context<Self>,
    ) -> Div {
        let key = self.session.project.song_key();
        let track_id = self.session.project.tracks[t].id;
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
                .h(px(28.))
                .flex_none()
                .rounded(px(7.))
                .cursor_pointer()
                .when(on, |d| d.bg(rgb(RAISED)))
                .when(!on, |d| d.hover(|d| d.bg(rgb(OFF))))
                .child(
                    div()
                        .w(px(3.))
                        .h(px(16.))
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
                .when(zone.trigger == Trigger::Release, |d| {
                    d.child(div().text_size(px(9.)).text_color(rgb(AMBER)).child("REL"))
                })
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
                        if ws.sampler_ui.zone != i {
                            ws.sampler_ui.view = None;
                        }
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
        let rows: Vec<_> = rows.collect();
        let z = &s.zones[picked];
        let color = zone_color(picked);
        let ranged = z.high.is_some();
        let k =
            |ws: &Self, f, label, cx: &mut Context<Self>| ws.sampler_knob(t, f, label, color, cx);
        let hearing = self.sampler_ui.hearing == Some(picked);
        let hear = button(
            if hearing {
                "Listening…"
            } else {
                "Hear its root"
            },
            hearing,
        )
        .debug_selector(|| "zone-hear-root".into())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                if ws.sampler_ui.hearing.is_none() {
                    ws.detect_root(t, picked, cx);
                }
            }),
        );
        let settings = div()
            .flex_none()
            .m(px(8.))
            .mt(px(0.))
            .p(px(10.))
            .rounded(px(12.))
            .bg(rgb(0x131318))
            .border_1()
            .border_color(rgb(LINE_SOFT))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(color)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(zone_name(z)),
                    )
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_3))
                            .child(match wave {
                                Some(w) => format!("{:.2} s", w.seconds),
                                None => "reading…".into(),
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(2.))
                    .child(k(
                        self,
                        Field::Note(picked),
                        if ranged { "Low" } else { "Note" },
                        cx,
                    ))
                    .when(ranged, |d| {
                        d.child(k(self, Field::High(picked), "High", cx)).child(k(
                            self,
                            Field::Root(picked),
                            "Root",
                            cx,
                        ))
                    })
                    .child(k(self, Field::Tune(picked), "Fine", cx))
                    .child(k(self, Field::Gain(picked), "Level", cx))
                    .child(k(self, Field::Pan(picked), "Pan", cx))
                    .child(k(self, Field::VelLow(picked), "Vel low", cx))
                    .child(k(self, Field::VelHigh(picked), "Vel high", cx))
                    .child(k(self, Field::VelFade(picked), "Vel fade", cx))
                    .child(k(self, Field::LoopFade(picked), "Loop fade", cx)),
            )
            .child(div().flex().child(hear));
        div()
            .w(px(ZONES_W))
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
                            .text_size(px(10.5))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(TEXT_3))
                            .child(format!("ZONES · {}", s.zones.len())),
                    )
                    .child(
                        div()
                            .h(px(20.))
                            .px(px(7.))
                            .flex()
                            .items_center()
                            .rounded(px(5.))
                            .cursor_pointer()
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_3))
                            .hover(|d| d.bg(rgb(OFF)).text_color(rgb(TEXT)))
                            .child("+ Add")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| ws.pick_samples(cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .id("zones")
                    .flex_1()
                    .min_h(px(40.))
                    .px(px(6.))
                    .pb(px(8.))
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .overflow_y_scroll()
                    .border_color(if self.file_drag.is_some() {
                        rgb(ACCENT)
                    } else {
                        rgb(SURFACE)
                    })
                    .children(rows),
            )
            .child(settings)
    }
}
