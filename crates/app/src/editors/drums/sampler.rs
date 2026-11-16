//! The picked pad's Drum Sampler, in the rack panel's Sampler tab, laid out on a fixed grid:
//! the waveform across the top (start, length and gain, reverse, and the sounds like it under
//! it), then two rows of three cards, each the same height: Envelope (attack, hold, decay;
//! Trigger or Gate), Pitch (transpose and detune), Playback (the effect, on an X/Y pad and two
//! knobs); Filter (its curve, frequency and resonance or a peak's gain), Output (volume, pan,
//! velocity), Modulation (velocity or slide, what it moves, how far). Its layers (sounds stacked
//! on the pad) are tabs in the panel's header.

use pina_engine::song::project::{
    DrumModSource, PAD_LAYERS, SampleCharacter, SampleMode, SamplerVoice,
};

use super::*;
use crate::editors::sampler::{Field, pill};
use crate::workspace::{Target, pt};

/// Each card's colour.
const SAMPLE: u32 = VIOLET;
const FX: u32 = AMBER;
const FILT: u32 = SKY;
const GLOBAL: u32 = MINT;

/// The two rows of cards' height.
pub(crate) const CARD_H: f32 = 112.0;

/// The playback effect's two settings: the first moved up and down on the X/Y pad, the second
/// across.
fn fx_fields(fx: PlaybackFx) -> [(Field, &'static str); 2] {
    match fx {
        PlaybackFx::Stretch => [
            (Field::StretchFactor, "Factor"),
            (Field::GrainSize, "Grain"),
        ],
        PlaybackFx::Loop => [(Field::LoopOffset, "Offset"), (Field::LoopMs, "Length")],
        PlaybackFx::PitchEnv => [(Field::PitchEnv, "Amount"), (Field::PitchDecay, "Decay")],
        PlaybackFx::Punch => [(Field::Punch, "Amount"), (Field::PunchMs, "Release")],
        PlaybackFx::EightBit => [(Field::CrushKhz, "Rate"), (Field::CrushDecay, "Decay")],
        PlaybackFx::Fm => [(Field::Fm, "Amount"), (Field::OscRatio, "Freq")],
        PlaybackFx::RingMod => [(Field::Ring, "Amount"), (Field::OscRatio, "Freq")],
        PlaybackFx::SubOsc => [(Field::Sub, "Amount"), (Field::SubHz, "Freq")],
        PlaybackFx::Noise => [(Field::Noise, "Amount"), (Field::NoiseColor, "Color")],
    }
}

/// A voice switched to a playback effect: the last one silent, this one's settings as they
/// were if they make a sound, else where it's heard well.
pub(crate) fn set_fx(v: &mut SamplerVoice, fx: PlaybackFx) {
    if let Some(was) = v.shown_fx().filter(|&w| w != fx) {
        was.silence(v);
    }
    if !fx.sounds(v) {
        match fx {
            PlaybackFx::Stretch => {
                v.mode = SampleMode::Stretch;
                v.speed = 1.0;
            }
            PlaybackFx::Loop => {
                v.loop_ms = 60.0;
                v.loop_offset = 0.0;
            }
            PlaybackFx::PitchEnv => {
                v.pitch_env = 12.0;
                v.pitch_decay_ms = 60.0;
            }
            PlaybackFx::Punch => v.punch = 0.5,
            PlaybackFx::EightBit => {
                v.character = SampleCharacter::Vintage;
                v.crush_khz = 8.0;
                v.bits = 8;
            }
            PlaybackFx::Fm => {
                v.fm = 0.3;
                v.osc_ratio = 1.0;
            }
            PlaybackFx::RingMod => {
                v.ring = 0.5;
                v.osc_ratio = 2.0;
            }
            PlaybackFx::SubOsc => {
                v.sub = 0.5;
                v.sub_hz = 60.0;
            }
            PlaybackFx::Noise => v.noise = 0.3,
        }
    }
    v.fx = Some(fx);
    v.fx_off = false;
}

/// A filter's name on its button, short: its kind and slope.
fn short_filter(m: FilterMode) -> &'static str {
    match m {
        FilterMode::LowPass => "LP 12",
        FilterMode::Ladder => "LP 24",
        FilterMode::HighPass24 => "HP 24",
        FilterMode::HighPass => "HP 12",
        FilterMode::Peak => "Peak",
        m => m.name(),
    }
}

/// A card: its name and (on the right) its switch over what it holds, centred, all the same
/// height; `grow` its share of the row's width, and of the least it takes before its row wraps.
fn card(title: &'static str, color: u32, side: impl IntoElement, body: Div, grow: f32) -> Div {
    div()
        .flex_grow(grow)
        .flex_shrink(1.)
        .flex_basis(px(grow * 145.))
        .min_w(px(0.))
        .h(px(CARD_H))
        .flex()
        .flex_col()
        .gap(px(6.))
        .px(px(10.))
        .py(px(8.))
        .rounded(px(7.))
        .bg(rgb(0x16161b))
        .border_1()
        .border_color(rgb(LINE_SOFT))
        .child(
            div()
                .h(px(20.))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(div().size(px(6.)).rounded(px(3.)).bg(rgb(color)))
                .child(
                    div()
                        .text_size(px(9.5))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(TEXT_2))
                        .whitespace_nowrap()
                        .child(title),
                )
                .child(div().flex_1())
                .child(side),
        )
        .child(
            body.flex_1()
                .min_h(px(0.))
                .flex()
                .items_center()
                .justify_center(),
        )
}

impl Workspace {
    /// The Sampler tab: the picked pad's Drum Sampler, or what goes on an empty pad.
    pub(crate) fn sampler_view(&mut self, kit: usize, cx: &mut Context<Self>) -> Div {
        let note = self.drums.pad;
        let Some((t, _)) = self.drum_pad(kit, note) else {
            return self.no_sampler(note, cx);
        };
        let Some(s) = self.sampler_of(t).cloned() else {
            return div();
        };
        let layer = self
            .pad_layer(kit, note)
            .min(s.zones.len().saturating_sub(1));
        let v = s.voice;
        // Too narrow for three across, a row's cards wrap under each other.
        let row = |a: Div, b: Div, c: Div| {
            div()
                .flex_none()
                .flex()
                .flex_wrap()
                .gap(px(6.))
                .child(a)
                .child(b)
                .child(c)
        };
        let top = row(
            self.ds_envelope(t, &s, layer, cx),
            self.ds_pitch(t, layer, cx),
            self.ds_fx(note, t, &v, cx),
        );
        let bottom = row(
            self.ds_filter(note, t, &v, cx),
            self.ds_output(t, cx),
            self.ds_mod(note, t, &v, cx),
        );
        // At least the panel's height (the sample takes what's spare); taller, it scrolls.
        div()
            .w_full()
            .min_h(gpui::relative(1.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(self.ds_sample(kit, note, t, &s, layer, cx))
            .child(top)
            .child(bottom)
    }

    /// A pad with no sound: what its note is, and how to give it one.
    fn no_sampler(&mut self, note: u8, cx: &mut Context<Self>) -> Div {
        let what = gm_name(note).map_or(String::new(), |n| format!(" · {n}"));
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(10.))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT_2))
                    .child(format!("{}{what}", note_name(note))),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT_3))
                    .child("No sound on this pad yet. Drop a sample on it, or pick one."),
            )
            .child(
                button("Pick a sound", false)
                    .h(px(28.))
                    .child(icon(Icon::Shuffle, 12.).text_color(rgb(TEXT_3)))
                    .flex_row_reverse()
                    .debug_selector(|| "drum-sampler-empty-swap".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.drums.tab = RackTab::Swap;
                            ws.drums.swap_rack = false;
                            cx.notify();
                        }),
                    ),
            )
    }

    /// The pad's layers as tabs (one sound, none shown): each picked to edit, × on the picked
    /// one, + for another, and how they play.
    pub(crate) fn layer_tabs(&mut self, kit: usize, cx: &mut Context<Self>) -> Div {
        let note = self.drums.pad;
        let mut row = div().flex().items_center().gap(px(2.));
        let Some(s) = self.drum_pad(kit, note).map(|(_, s)| s.clone()) else {
            return row;
        };
        let layer = self.pad_layer(kit, note);
        let n = s.zones.len();
        if n > 1 {
            row = row
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(rgb(TEXT_3))
                        .mr(px(2.))
                        .child("Layers"),
                )
                .children((0..n).map(|i| {
                    seg((i + 1).to_string(), i == layer, 20.)
                        .debug_selector(move || format!("pad-layer-{i}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.drums.layer = (note, i);
                                cx.notify();
                            }),
                        )
                }))
                .child(
                    icon_seg(Icon::Close, false, 20.)
                        .debug_selector(|| "pad-remove-layer".into())
                        .help("Take the picked layer off the pad", cx)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.drum_op(DrumOp::Unlayer { note, layer }, cx);
                            }),
                        ),
                )
                .child(
                    dropdown(s.layering().map_or("Mixed", |l| l.name()).to_string(), 20.)
                        .debug_selector(|| "pad-layering".into())
                        .help(
                            "How its layers play: all at once, in turn, or by how hard it's hit",
                            cx,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                let blends =
                                    ws.drum_pad(kit, note).is_some_and(|(_, s)| s.blends());
                                let mut items: Vec<(String, MenuAction)> = Layering::ALL
                                    .map(|how| {
                                        (
                                            how.name().to_string(),
                                            MenuAction::Drums(DrumOp::Layering { note, how }),
                                        )
                                    })
                                    .to_vec();
                                items.push((
                                    match blends {
                                        true => "Velocity layers switch".into(),
                                        false => "Velocity layers blend".into(),
                                    },
                                    MenuAction::Drums(DrumOp::Blend { note, on: !blends }),
                                ));
                                ws.menu = Some(Menu {
                                    at: e.position,
                                    items,
                                    ..Default::default()
                                });
                                cx.notify();
                            }),
                        ),
                );
        }
        if n < PAD_LAYERS {
            row = row.child(
                button("Layer", false)
                    .h(px(22.))
                    .child(icon(Icon::Plus, 10.).text_color(rgb(TEXT_3)))
                    .flex_row_reverse()
                    .debug_selector(|| "pad-add-layer".into())
                    .help(
                        "Layer another sound on the pad: a clap on a snare, a sub under a kick",
                        cx,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.layer_menu(kit, note, e.position, cx);
                        }),
                    ),
            );
        }
        row
    }

    /// The sample: its waveform (drag its start and end; drop a sample on it to layer it), and
    /// along its foot its start, length and gain, reverse, the sounds like it either side, and
    /// picking another.
    fn ds_sample(
        &mut self,
        kit: usize,
        note: u8,
        t: usize,
        s: &Sampler,
        layer: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let z = s.zones.get(layer).cloned().unwrap_or_default();
        let wave = {
            let file = z.file.clone();
            self.wave(&file, cx)
        };
        let dropping = match self.drag {
            Some(Drag::Sample {
                at, moved: true, ..
            }) => self.strip_under(at) == Some(note),
            _ => false,
        };
        let strip = self.drums.strip.clone();
        let waveform = div()
            .relative()
            .flex_1()
            .min_h(px(64.))
            .rounded(px(6.))
            .overflow_hidden()
            .bg(rgb(WELL))
            .border_1()
            .border_color(rgb(if dropping { ACCENT } else { LINE_SOFT }))
            .debug_selector(|| "pad-layers".into())
            .child(
                canvas(move |b, _, _| strip.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            // Pinned to the box: the waveform takes the room it's given, no more.
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .flex_col()
                    .child(self.sampler_wave(t, s, layer, wave, cx)),
            )
            .when(dropping, |d| {
                d.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(alpha(ACCENT, 0.2))
                        .text_size(px(11.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Drop to layer it on this pad"),
                )
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.sample_menu(kit, note, e.position, cx);
                }),
            );
        let step =
            |i: Icon, by: i32, sel: &'static str, help: &'static str, cx: &mut Context<Self>| {
                icon_seg(i, false, 24.)
                    .debug_selector(move || sel.into())
                    .help(help, cx)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.step_pad(note, by, cx);
                        }),
                    )
            };
        let reverse = pill("Reverse", z.reverse, SAMPLE)
            .debug_selector(|| "pad-reverse".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.edit_sampler(t, cx, |s| {
                        if let Some(z) = s.zones.get_mut(layer) {
                            z.reverse ^= true;
                        }
                    });
                }),
            );
        let foot = div()
            .min_h(px(26.))
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(4.))
            .child(self.sampler_value(t, Field::Start(layer), "Start", SAMPLE, cx))
            .child(self.sampler_value(t, Field::Span(layer), "Length", SAMPLE, cx))
            .child(self.sampler_value(t, Field::Gain(layer), "Gain", SAMPLE, cx))
            .child(div().flex_1())
            .child(reverse)
            .child(div().w(px(1.)).h(px(16.)).bg(rgb(LINE)).mx(px(2.)))
            .child(step(
                Icon::Back,
                -1,
                "pad-back",
                "The previous sound like this one",
                cx,
            ))
            .child(step(
                Icon::Forward,
                1,
                "pad-next",
                "The next sound like this one",
                cx,
            ))
            .child(
                button("Swap", false)
                    .h(px(24.))
                    .child(icon(Icon::Shuffle, 11.).text_color(rgb(TEXT_3)))
                    .flex_row_reverse()
                    .debug_selector(|| "pad-hot-swap".into())
                    .help("Hot-swap: the library's sounds for this pad", cx)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.drums.tab = RackTab::Swap;
                            ws.drums.swap_rack = false;
                            cx.notify();
                        }),
                    ),
            );
        div()
            .flex_1()
            .min_h(px(132.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(waveform)
            .child(foot)
    }

    /// Envelope: attack, hold and decay; Trigger (plays out its hold and decay) or Gate (holds
    /// while the note does, then decays).
    fn ds_envelope(&mut self, t: usize, s: &Sampler, layer: usize, cx: &mut Context<Self>) -> Div {
        let gate = s.zones.get(layer).is_some_and(|z| z.gate);
        let mode = tray().p(px(2.)).rounded(px(6.)).children(
            [("Trigger", false), ("Gate", true)].map(|(label, g)| {
                seg(label, gate == g, 18.)
                    .debug_selector(move || format!("pad-{}", label.to_lowercase()))
                    .help(
                        match g {
                            false => "Trigger: the sound plays out its hold and decay, however short the note",
                            true => "Gate: the sound holds while the note does, then decays",
                        },
                        cx,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit_sampler(t, cx, |s| {
                                for z in &mut s.zones {
                                    z.gate = g;
                                }
                                if g {
                                    s.voice.sustain = 1.0;
                                    s.voice.release_ms = s.voice.decay_ms.max(1.0);
                                }
                            });
                        }),
                    )
            }),
        );
        let hold = match gate {
            true => div()
                .w(px(48.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(3.))
                .opacity(0.35)
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(rgb(TEXT_3))
                        .child("Hold"),
                )
                .child(
                    div()
                        .size(px(30.))
                        .rounded_full()
                        .border_2()
                        .border_color(rgb(LINE)),
                )
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(rgb(TEXT_3))
                        .child("Gate"),
                ),
            false => self.sampler_knob(t, Field::Hold, "Hold", SAMPLE, cx),
        };
        card(
            "ENVELOPE",
            SAMPLE,
            mode,
            div()
                .gap(px(4.))
                .child(self.sampler_knob(t, Field::Attack, "Attack", SAMPLE, cx))
                .child(hold)
                .child(self.sampler_knob(t, Field::DrumDecay, "Decay", SAMPLE, cx)),
            1.35,
        )
    }

    /// Pitch: transpose and detune.
    fn ds_pitch(&mut self, t: usize, layer: usize, cx: &mut Context<Self>) -> Div {
        card(
            "PITCH",
            SAMPLE,
            div(),
            div()
                .gap(px(14.))
                .child(self.sampler_knob(t, Field::Transpose, "Transpose", SAMPLE, cx))
                .child(self.sampler_knob(t, Field::Tune(layer), "Detune", SAMPLE, cx)),
            0.85,
        )
    }

    /// Playback: on, which effect, its two settings on an X/Y pad and two knobs (greyed while
    /// there's none, or it's off).
    fn ds_fx(&mut self, note: u8, t: usize, v: &SamplerVoice, cx: &mut Context<Self>) -> Div {
        let fx = v.shown_fx();
        let on = fx.is_some() && !v.fx_off;
        let switch = pill("On", on, FX)
            .debug_selector(|| "pad-fx-on".into())
            .help("Playback effect on or off", cx)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.edit_sampler(t, cx, |s| match s.voice.shown_fx() {
                        Some(_) => s.voice.fx_off ^= true,
                        None => set_fx(&mut s.voice, PlaybackFx::PitchEnv),
                    });
                }),
            );
        let kind = dropdown(fx.map_or("None", |f| f.name()), 20.)
            .debug_selector(|| "pad-fx-kind".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let items = PlaybackFx::ALL
                        .map(|fx| {
                            (
                                fx.name().to_string(),
                                MenuAction::Drums(DrumOp::Fx { note, fx }),
                            )
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
        let head = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(switch)
            .child(kind);
        let [(f1, l1), (f2, l2)] = fx_fields(fx.unwrap_or(PlaybackFx::PitchEnv));
        let body = div()
            .gap(px(8.))
            .opacity(if on { 1.0 } else { 0.35 })
            .child(self.xy_pad(t, f2, f1, on, cx))
            .child(self.sampler_knob(t, f1, l1, FX, cx))
            .child(self.sampler_knob(t, f2, l2, FX, cx));
        card("PLAYBACK", FX, head, body, 1.3)
    }

    /// An X/Y pad: across moves `x`, up moves `y`; drag anywhere on it.
    fn xy_pad(&mut self, t: usize, x: Field, y: Field, on: bool, cx: &mut Context<Self>) -> Div {
        let (px_, py_) = (self.sampler_pos(t, x), self.sampler_pos(t, y));
        let area: Rc<Cell<Option<Bounds<Pixels>>>> = Default::default();
        let seen = area.clone();
        let (tx, ty) = (
            Target::Sampler { track: t, field: x },
            Target::Sampler { track: t, field: y },
        );
        let dot = canvas(
            move |b, _, _| seen.set(Some(b)),
            move |b, _, window, _| {
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                for i in 1..4 {
                    let f = i as f32 / 4.0;
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x + px(w * f), b.origin.y),
                            size(px(1.), px(h)),
                        ),
                        rgb(LINE_SOFT),
                    ));
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x, b.origin.y + px(h * f)),
                            size(px(w), px(1.)),
                        ),
                        rgb(LINE_SOFT),
                    ));
                }
                let (cx_, cy_) = (w * px_, h * (1.0 - py_));
                window.paint_quad(fill(
                    Bounds::new(point(b.origin.x + px(cx_), b.origin.y), size(px(1.), px(h))),
                    alpha(FX, 0.35),
                ));
                window.paint_quad(fill(
                    Bounds::new(point(b.origin.x, b.origin.y + px(cy_)), size(px(w), px(1.))),
                    alpha(FX, 0.35),
                ));
                window.paint_quad(gpui::quad(
                    Bounds::new(
                        point(b.origin.x + px(cx_ - 5.0), b.origin.y + px(cy_ - 5.0)),
                        size(px(10.), px(10.)),
                    ),
                    px(5.),
                    rgb(if on { FX } else { TEXT_3 }),
                    px(2.),
                    rgb(BG),
                    Default::default(),
                ));
            },
        )
        .size_full();
        div()
            .size(px(82.))
            .flex_none()
            .rounded(px(5.))
            .bg(rgb(WELL))
            .border_1()
            .border_color(rgb(LINE_SOFT))
            .overflow_hidden()
            .cursor_crosshair()
            .debug_selector(|| "pad-fx-xy".into())
            .child(dot)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let Some(b) = area.get() else {
                        return;
                    };
                    let drag = Drag::Xy {
                        x: tx,
                        y: ty,
                        left: f32::from(b.origin.x),
                        top: f32::from(b.origin.y),
                        w: f32::from(b.size.width),
                        h: f32::from(b.size.height),
                    };
                    ws.begin_drag(drag, cx);
                    ws.drag_to(pt(e.position), cx);
                }),
            )
    }

    /// Filter: on, its kind, its curve, its frequency and resonance (a peak's gain and width).
    fn ds_filter(&mut self, note: u8, t: usize, v: &SamplerVoice, cx: &mut Context<Self>) -> Div {
        let on = v.filter != FilterMode::Off && !v.filter_off;
        let mode = match v.filter {
            FilterMode::Off => FilterMode::LowPass,
            m => m,
        };
        let switch = pill("On", on, FILT)
            .debug_selector(|| "pad-filter-on".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.edit_sampler(t, cx, |s| {
                        let v = &mut s.voice;
                        if v.filter == FilterMode::Off {
                            v.filter = FilterMode::LowPass;
                            v.filter_off = false;
                        } else {
                            v.filter_off ^= true;
                        }
                    });
                }),
            );
        let kind = dropdown(short_filter(mode), 20.)
            .debug_selector(|| "pad-filter-kind".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let items = FilterMode::DRUM
                        .map(|mode| {
                            (
                                mode.drum_name().to_string(),
                                MenuAction::Drums(DrumOp::Filter { note, mode }),
                            )
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
        let (cutoff, reso, peak) = (v.cutoff_hz, v.resonance, v.peak_db);
        let curve = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                let mut path = gpui::PathBuilder::stroke(px(1.5));
                for i in 0..=48 {
                    let x = i as f32 / 48.0;
                    let hz = 20.0 * 1000f32.powf(x);
                    let db = crate::editors::sampler::response_peak(mode, cutoff, reso, peak, hz);
                    let y = (0.45 - db / 48.0).clamp(0.0, 1.0) * h;
                    let p = point(b.origin.x + px(x * w), b.origin.y + px(y));
                    match i {
                        0 => path.move_to(p),
                        _ => path.line_to(p),
                    }
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, if on { rgb(FILT) } else { rgb(TEXT_4) });
                }
            },
        )
        .w(px(78.))
        .h(px(56.));
        let knobs = div()
            .flex()
            .gap(px(2.))
            .child(self.sampler_knob(t, Field::Cutoff, "Freq", FILT, cx))
            .child(match mode {
                FilterMode::Peak => self.sampler_knob(t, Field::PeakDb, "Gain", FILT, cx),
                _ => self.sampler_knob(t, Field::Resonance, "Res", FILT, cx),
            })
            .when(mode == FilterMode::Peak, |d| {
                d.child(self.sampler_knob(t, Field::Resonance, "Width", FILT, cx))
            });
        let head = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(switch)
            .child(kind);
        card(
            "FILTER",
            FILT,
            head,
            div()
                .gap(px(8.))
                .opacity(if on { 1.0 } else { 0.35 })
                .child(
                    div()
                        .rounded(px(5.))
                        .bg(rgb(WELL))
                        .border_1()
                        .border_color(rgb(LINE_SOFT))
                        .overflow_hidden()
                        .child(curve),
                )
                .child(knobs),
            1.25,
        )
    }

    /// Output: volume, pan, and how much velocity changes its level.
    fn ds_output(&mut self, t: usize, cx: &mut Context<Self>) -> Div {
        card(
            "OUTPUT",
            GLOBAL,
            div(),
            div()
                .gap(px(4.))
                .child(self.sampler_knob(t, Field::Volume, "Volume", GLOBAL, cx))
                .child(self.sampler_knob(t, Field::VoicePan, "Pan", GLOBAL, cx))
                .child(self.sampler_knob(t, Field::Velocity, "Vel>Vol", GLOBAL, cx)),
            1.0,
        )
    }

    /// Modulation: velocity or slide, what it moves, how far.
    fn ds_mod(&mut self, note: u8, t: usize, v: &SamplerVoice, cx: &mut Context<Self>) -> Div {
        let source = tray().p(px(2.)).rounded(px(6.)).children(
            [
                ("Vel", DrumModSource::Velocity),
                ("Slide", DrumModSource::Slide),
            ]
            .map(|(label, src)| {
                seg(label, v.drum_source == src, 18.)
                    .debug_selector(move || format!("pad-mod-{}", label.to_lowercase()))
                    .help(
                        match src {
                            DrumModSource::Velocity => "Modulation from how hard it's hit",
                            DrumModSource::Slide => "Modulation from the slide (MPE)",
                        },
                        cx,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit_sampler(t, cx, |s| s.voice.drum_source = src);
                        }),
                    )
            }),
        );
        let dest = dropdown(v.drum_dest.name(), 22.)
            .debug_selector(|| "pad-mod-dest".into())
            .help("What the modulation moves", cx)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let items = DrumModDest::ALL
                        .map(|dest| {
                            (
                                dest.name().to_string(),
                                MenuAction::Drums(DrumOp::ModDest { note, dest }),
                            )
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
        card(
            "MODULATION",
            GLOBAL,
            source,
            div()
                .gap(px(12.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(
                            div()
                                .text_size(px(10.))
                                .text_color(rgb(TEXT_3))
                                .child("Moves"),
                        )
                        .child(dest),
                )
                .child(self.sampler_knob(t, Field::DrumAmount, "Amount", GLOBAL, cx)),
            1.25,
        )
    }

    /// The waveform's menu: the sounds like it in hot-swap, the one it started from back, or it
    /// kept as where likeness is measured from.
    fn sample_menu(
        &mut self,
        kit: usize,
        note: u8,
        at: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let _ = kit;
        let layer = self.drums.layer.1;
        let items = vec![
            (
                "Show Similar Files".to_string(),
                MenuAction::Drums(DrumOp::Similar { note }),
            ),
            (
                "Return to Reference".to_string(),
                MenuAction::Drums(DrumOp::Reference { note, layer }),
            ),
            (
                "Save as Similarity Reference".to_string(),
                MenuAction::Drums(DrumOp::KeepReference { note, layer }),
            ),
            (
                "Reverse".to_string(),
                MenuAction::Drums(DrumOp::Reverse { note, layer }),
            ),
        ];
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    /// A menu of the notes a pad can move to: the free ones, and those with pads (they trade).
    pub(crate) fn pad_note_menu(
        &mut self,
        kit: usize,
        note: u8,
        at: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let p = &self.session.project;
        let items: Vec<(String, MenuAction)> = (0..128u8)
            .rev()
            .filter(|&n| n != note)
            .map(|n| {
                let label = match p.pad_on(kit, n) {
                    Some(t) => format!("{}  ·  trade with {}", note_name(n), p.tracks[t].name),
                    None => note_name(n),
                };
                (label, MenuAction::Drums(DrumOp::Note { note, to: n }))
            })
            .collect();
        self.menu = Some(Menu {
            at,
            items,
            searchable: true,
            ..Default::default()
        });
        cx.notify();
    }
}
