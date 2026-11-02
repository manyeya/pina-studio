//! The sampler's modules along the bottom: Play (what its mode needs), Filter (its response
//! drawn), Amp (its envelope drawn) and Modulation (the LFOs, the envelopes, the matrix, and
//! what plays with the sample), each a card.

use super::*;

pub(crate) const PLAY: u32 = VIOLET;
pub(crate) const FILTER: u32 = SKY;
pub(crate) const AMP: u32 = MINT;
pub(crate) const MODS: u32 = AMBER;

/// A module's card: its name (and what goes beside it) over what it holds.
pub(crate) fn module(title: &'static str, color: u32, side: impl IntoElement) -> Div {
    div()
        .flex_none()
        .h_full()
        .flex()
        .flex_col()
        .gap(px(8.))
        .px(px(12.))
        .py(px(10.))
        .rounded(px(12.))
        .bg(rgb(0x131318))
        .border_1()
        .border_color(rgb(LINE_SOFT))
        .child(
            div()
                .h(px(22.))
                .flex()
                .items_center()
                .gap(px(8.))
                .child(div().size(px(7.)).rounded(px(4.)).bg(rgb(color)))
                .child(
                    div()
                        .text_size(px(10.5))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(TEXT_2))
                        .child(title),
                )
                .child(div().flex_1())
                .child(side),
        )
}

pub(crate) fn knobs() -> Div {
    div().flex().items_start().gap(px(2.))
}

impl Workspace {
    /// What the mode needs: Repitch its pitch, speed and drop; Stretch its speed and grains;
    /// Grains its cloud. Unison under them all.
    pub(crate) fn play_module(&self, t: usize, v: &SamplerVoice, cx: &mut Context<Self>) -> Div {
        let k =
            |ws: &Self, f, label, cx: &mut Context<Self>| ws.sampler_knob(t, f, label, PLAY, cx);
        let freeze = pill("Freeze", v.freeze, PLAY)
            .debug_selector(|| "sampler-freeze".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.edit_sampler(t, cx, |s| s.voice.freeze ^= true);
                }),
            );
        let (title, first, second): (&'static str, Div, Div) = match v.mode {
            SampleMode::Repitch => (
                "REPITCH",
                knobs()
                    .child(k(self, Field::Transpose, "Transpose", cx))
                    .child(k(self, Field::Speed, "Speed", cx))
                    .child(k(self, Field::Glide, "Glide", cx))
                    .child(k(self, Field::Volume, "Volume", cx)),
                knobs()
                    .child(k(self, Field::Unison, "Unison", cx))
                    .child(k(self, Field::Detune, "Detune", cx))
                    .child(k(self, Field::Spread, "Width", cx))
                    .child(k(self, Field::Velocity, "Velocity", cx)),
            ),
            SampleMode::Stretch => (
                "STRETCH",
                knobs()
                    .child(k(self, Field::Transpose, "Transpose", cx))
                    .child(k(self, Field::Speed, "Speed", cx))
                    .child(k(self, Field::GrainSize, "Grain", cx))
                    .child(k(self, Field::Position, "Start", cx)),
                knobs()
                    .child(k(self, Field::Unison, "Unison", cx))
                    .child(k(self, Field::Detune, "Detune", cx))
                    .child(k(self, Field::Spread, "Width", cx))
                    .child(k(self, Field::Volume, "Volume", cx)),
            ),
            SampleMode::Grains => (
                "GRAINS",
                knobs()
                    .child(k(self, Field::Position, "Position", cx))
                    .child(k(self, Field::GrainSize, "Size", cx))
                    .child(k(self, Field::Density, "Density", cx))
                    .child(k(self, Field::Spray, "Spray", cx)),
                knobs()
                    .child(k(self, Field::Speed, "Scan", cx))
                    .child(k(self, Field::Transpose, "Transpose", cx))
                    .child(k(self, Field::GrainPitch, "Detune", cx))
                    .child(k(self, Field::GrainReverse, "Reverse", cx)),
            ),
        };
        let side = match v.mode {
            SampleMode::Repitch => div(),
            _ => div().child(freeze),
        };
        module(title, PLAY, side)
            .debug_selector(|| "sampler-play".into())
            .child(first)
            .child(second)
    }

    /// The filter: its kind (a menu), its response drawn (drag its point), and its knobs.
    pub(crate) fn filter_module(&self, t: usize, v: &SamplerVoice, cx: &mut Context<Self>) -> Div {
        let k =
            |ws: &Self, f, label, cx: &mut Context<Self>| ws.sampler_knob(t, f, label, FILTER, cx);
        let kind = div()
            .h(px(22.))
            .px(px(9.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(6.))
            .bg(rgb(FIELD))
            .cursor_pointer()
            .hover(|d| d.bg(rgb(RAISED)))
            .text_size(px(11.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(if v.filter == FilterMode::Off {
                TEXT_3
            } else {
                FILTER
            }))
            .debug_selector(|| "sampler-filter-kind".into())
            .child(v.filter.name())
            .child(icon(Icon::Chevron, 9.).text_color(rgb(TEXT_4)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let items = FilterMode::ALL
                        .map(|mode| {
                            let op = SamplerOp::Filter { track: t, mode };
                            (mode.name().to_string(), MenuAction::Sampler(op))
                        })
                        .to_vec();
                    ws.menu = Some(Menu {
                        at: e.position,
                        items,
                        ..Default::default()
                    });
                    cx.notify();
                }),
            );
        module("FILTER", FILTER, kind)
            .w(px(216.))
            .child(self.filter_graph(t, FILTER, 68., cx))
            .child(
                knobs()
                    .child(k(self, Field::Cutoff, "Cutoff", cx))
                    .child(k(self, Field::Resonance, "Reso", cx))
                    .child(k(self, Field::Drive, "Drive", cx))
                    .child(k(self, Field::FilterEnv, "Env", cx)),
            )
    }

    /// The amp envelope, drawn (drag its points), and its knobs.
    pub(crate) fn amp_module(&self, t: usize, cx: &mut Context<Self>) -> Div {
        let k = |ws: &Self, f, label, cx: &mut Context<Self>| ws.sampler_knob(t, f, label, AMP, cx);
        module("AMP", AMP, div())
            .w(px(216.))
            .child(self.env_graph(t, EnvKind::Amp, AMP, 68., cx))
            .child(
                knobs()
                    .child(k(self, Field::Attack, "Attack", cx))
                    .child(k(self, Field::Decay, "Decay", cx))
                    .child(k(self, Field::Sustain, "Sustain", cx))
                    .child(k(self, Field::Release, "Release", cx)),
            )
    }

    /// Modulation: LFO 1 (and its own routes), LFO 2, the filter and modulation envelopes, the
    /// matrix, and what plays with the sample; a page each.
    pub(crate) fn mod_module(&self, t: usize, v: &SamplerVoice, cx: &mut Context<Self>) -> Div {
        let tab = self.sampler_ui.tab;
        let tabs = tray().p(px(2.)).gap(px(1.)).rounded(px(8.)).children(
            [
                (ModTab::Lfo1, "LFO 1"),
                (ModTab::Lfo2, "LFO 2"),
                (ModTab::Env, "Env"),
                (ModTab::Matrix, "Matrix"),
                (ModTab::Layers, "Layers"),
            ]
            .map(|(which, label)| {
                pill(label, tab == which, MODS)
                    .px(px(7.))
                    .debug_selector(move || {
                        format!("sampler-tab-{}", label.to_lowercase().replace(' ', ""))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.sampler_ui.tab = which;
                            cx.notify();
                        }),
                    )
            }),
        );
        let k = |ws: &Self, f, label, color, cx: &mut Context<Self>| {
            ws.sampler_knob(t, f, label, color, cx)
        };
        let body =
            match tab {
                ModTab::Lfo1 | ModTab::Lfo2 => {
                    let second = tab == ModTab::Lfo2;
                    let (wave, beats, retrigger) = match second {
                        false => (v.lfo_wave, v.lfo_beats, v.lfo_retrigger),
                        true => (v.lfo2.wave, v.lfo2.beats, v.lfo2.retrigger),
                    };
                    let waves = div().flex().gap(px(3.)).children(LfoWave::ALL.map(|w| {
                        wave_icon(w, w == wave, MODS)
                            .debug_selector(move || {
                                format!("lfo-wave-{}", w.name().to_lowercase().replace(' ', "-"))
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.edit_sampler(t, cx, |s| match second {
                                        false => s.voice.lfo_wave = w,
                                        true => s.voice.lfo2.wave = w,
                                    });
                                }),
                            )
                    }));
                    let sync = pill("Sync", beats > 0.0, MODS)
                        .debug_selector(|| "lfo-sync".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.edit_sampler(t, cx, |s| {
                                    let b = match second {
                                        false => &mut s.voice.lfo_beats,
                                        true => &mut s.voice.lfo2.beats,
                                    };
                                    *b = if *b > 0.0 { 0.0 } else { 1.0 };
                                });
                            }),
                        );
                    let retrig = pill("Retrig", retrigger, MODS)
                        .debug_selector(|| "lfo-retrigger".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.edit_sampler(t, cx, |s| match second {
                                    false => s.voice.lfo_retrigger ^= true,
                                    true => s.voice.lfo2.retrigger ^= true,
                                });
                            }),
                        );
                    let rate = match second {
                        false => Field::LfoRate,
                        true => Field::Lfo2Rate,
                    };
                    let row = knobs().child(k(self, rate, "Rate", MODS, cx));
                    let row = match second {
                        false => row
                            .child(k(self, Field::Vibrato, "Vibrato", MODS, cx))
                            .child(k(self, Field::LfoFilter, "Wah", MODS, cx))
                            .child(k(self, Field::Tremolo, "Tremolo", MODS, cx)),
                        true => row.child(
                            div()
                                .flex_1()
                                .pt(px(16.))
                                .pl(px(6.))
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_3))
                                .child("Moves what the matrix routes it to."),
                        ),
                    };
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(7.))
                        .child(waves)
                        .child(row)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .child(sync)
                                .child(retrig)
                                .child(div().flex_1().child(wave_graph(wave, MODS, 26.))),
                        )
                }
                ModTab::Env => {
                    let mod_env = self.sampler_ui.mod_env;
                    let which = tray().p(px(2.)).gap(px(1.)).rounded(px(8.)).children(
                        [(false, "Filter env"), (true, "Mod env")].map(|(m, label)| {
                            pill(label, mod_env == m, if m { MODS } else { FILTER })
                                .debug_selector(move || format!("sampler-env-{m}"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        cx.stop_propagation();
                                        ws.sampler_ui.mod_env = m;
                                        cx.notify();
                                    }),
                                )
                        }),
                    );
                    let (env, color, fields) = match mod_env {
                        false => (
                            EnvKind::Filter,
                            FILTER,
                            [
                                Field::FilterAttack,
                                Field::FilterDecay,
                                Field::FilterSustain,
                                Field::FilterRelease,
                            ],
                        ),
                        true => (
                            EnvKind::Mod,
                            MODS,
                            [
                                Field::ModAttack,
                                Field::ModDecay,
                                Field::ModSustain,
                                Field::ModRelease,
                            ],
                        ),
                    };
                    let row = knobs()
                        .child(k(self, fields[0], "Attack", color, cx))
                        .child(k(self, fields[1], "Decay", color, cx))
                        .child(k(self, fields[2], "Sustain", color, cx))
                        .child(k(self, fields[3], "Release", color, cx))
                        .when(!mod_env, |d| {
                            d.child(k(self, Field::KeyTrack, "Key", FILTER, cx))
                        });
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(
                            div().flex().gap(px(8.)).items_center().child(which).child(
                                div().flex_1().child(self.env_graph(t, env, color, 34., cx)),
                            ),
                        )
                        .child(row)
                }
                ModTab::Matrix => div().flex_1().min_h(px(0.)).child(
                    div()
                        .id("sampler-matrix")
                        .size_full()
                        .overflow_y_scroll()
                        .child(self.sampler_matrix(t, MODS, cx)),
                ),
                ModTab::Layers => div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        knobs()
                            .child(k(self, Field::Sub, "Sub", MODS, cx))
                            .child(k(self, Field::Noise, "Noise", MODS, cx))
                            .child(k(self, Field::NoiseColor, "Color", MODS, cx))
                            .child(k(self, Field::Punch, "Punch", MODS, cx)),
                    )
                    .child(
                        knobs()
                            .child(k(self, Field::PitchEnv, "Drop", MODS, cx))
                            .child(k(self, Field::PitchDecay, "Drop time", MODS, cx))
                            .child(k(self, Field::Fm, "FM", MODS, cx))
                            .child(k(self, Field::Ring, "Ring", MODS, cx))
                            .child(k(self, Field::OscRatio, "Ratio", MODS, cx)),
                    ),
            };
        div()
            .flex_1()
            .min_w(px(272.))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .px(px(12.))
            .py(px(10.))
            .rounded(px(12.))
            .bg(rgb(0x131318))
            .border_1()
            .border_color(rgb(LINE_SOFT))
            .child(
                div()
                    .h(px(22.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().size(px(7.)).rounded(px(4.)).bg(rgb(MODS)))
                    .child(tabs),
            )
            .child(body)
    }
}
