//! The pad picked: what it is and how it's choked; its sound, to step through others like it,
//! and its layers; its waveform (drag its start and end); and its shaping, module by module.

use super::*;
use crate::editors::sampler::{
    Field,
    modules::{AMP, FILTER, MODS, PLAY, knobs, module},
};
use pina_engine::song::project::PAD_LAYERS;

/// The sound module's title, as a layer's: its level, pan and tuning are that layer's own.
const LAYER_TITLES: [&str; PAD_LAYERS] = [
    "LAYER 1", "LAYER 2", "LAYER 3", "LAYER 4", "LAYER 5", "LAYER 6", "LAYER 7", "LAYER 8",
];

impl Workspace {
    pub(crate) fn pad_editor(&mut self, kit: usize, cx: &mut Context<Self>) -> Div {
        let note = self.drums.pad;
        let body = div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .overflow_hidden();
        let Some((t, s)) = self.drum_pad(kit, note).map(|(t, s)| (t, s.clone())) else {
            return body.child(self.empty_pad(kit, note, cx));
        };
        let color = self.pad_color(kit, note);
        let layer = self.pad_layer(kit, note);
        let wave = s.zones.get(layer).and_then(|z| {
            let file = z.file.clone();
            self.wave(&file, cx)
        });
        let k = |ws: &Self, f: Field, label: &'static str, c: u32, cx: &mut Context<Self>| {
            ws.sampler_knob(t, f, label, c, cx)
        };
        let reversed = s.zones.get(layer).is_some_and(|z| z.reverse);
        let reverse = crate::editors::sampler::pill("Reverse", reversed, color)
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
        let first = div()
            .flex()
            .gap(px(8.))
            .child(
                module(
                    match s.zones.len() {
                        1 => "SOUND",
                        _ => LAYER_TITLES.get(layer).copied().unwrap_or("SOUND"),
                    },
                    PLAY,
                    reverse,
                )
                .flex_1()
                .child(
                    knobs()
                        .child(k(self, Field::Transpose, "Tune", PLAY, cx))
                        .child(k(self, Field::Tune(layer), "Fine", PLAY, cx))
                        .child(k(self, Field::Gain(layer), "Level", PLAY, cx))
                        .child(k(self, Field::Pan(layer), "Pan", PLAY, cx)),
                ),
            )
            .child(
                module("ENVELOPE", AMP, div()).flex_1().child(
                    knobs()
                        .child(k(self, Field::Attack, "Attack", AMP, cx))
                        .child(k(self, Field::Length, "Length", AMP, cx))
                        .child(k(self, Field::Velocity, "Velocity", AMP, cx)),
                ),
            )
            .child(
                module("PITCH DROP", MODS, div()).flex_1().child(
                    knobs()
                        .child(k(self, Field::PitchEnv, "Drop", MODS, cx))
                        .child(k(self, Field::PitchDecay, "Time", MODS, cx)),
                ),
            );
        let v = s.voice;
        let second = div()
            .flex()
            .gap(px(8.))
            .child(
                module("TONE", FILTER, self.pad_filter_kind(t, v.filter, cx))
                    .flex_1()
                    .child(
                        knobs()
                            .child(k(self, Field::Cutoff, "Cutoff", FILTER, cx))
                            .child(k(self, Field::Resonance, "Reso", FILTER, cx))
                            .child(k(self, Field::Drive, "Drive", FILTER, cx)),
                    ),
            )
            .child(
                module("CHARACTER", CORAL, div()).flex_1().child(
                    knobs()
                        .child(k(self, Field::Punch, "Punch", CORAL, cx))
                        .child(k(self, Field::Sub, "Sub", CORAL, cx))
                        .child(k(self, Field::Noise, "Noise", CORAL, cx))
                        .child(k(self, Field::Color, "Colour", CORAL, cx)),
                ),
            );
        body.child(self.pad_head(kit, note, t, color, cx))
            .child(self.pad_sound(kit, note, color, cx))
            .child(
                div()
                    .h(px(84.))
                    .flex_none()
                    .rounded(px(10.))
                    .overflow_hidden()
                    .child(self.sampler_wave(t, &s, layer, wave, cx)),
            )
            .child(first)
            .child(second)
    }

    /// Its filter's kind, a menu.
    fn pad_filter_kind(
        &self,
        t: usize,
        mode: pina_engine::song::project::FilterMode,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
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
            .text_color(rgb(match mode {
                pina_engine::song::project::FilterMode::Off => TEXT_3,
                _ => FILTER,
            }))
            .debug_selector(|| "pad-filter-kind".into())
            .child(mode.name())
            .child(icon(Icon::Chevron, 9.).text_color(rgb(TEXT_4)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let items = pina_engine::song::project::FilterMode::ALL
                        .map(|mode| {
                            let op = crate::editors::sampler::SamplerOp::Filter { track: t, mode };
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
            )
    }

    /// Its name, kind and note; its choke group; mute and solo; its channel and full sampler.
    fn pad_head(&self, kit: usize, note: u8, t: usize, color: u32, cx: &mut Context<Self>) -> Div {
        let track = &self.session.project.tracks[t];
        let (name, muted, solo) = (track.name.clone(), track.mute, track.solo);
        let choke = track.pad.and_then(|p| p.choke);
        let role = self.pad_role(kit, note);
        let chokes = tray().p(px(2.)).rounded(px(7.)).children(
            [None, Some(1), Some(2), Some(3), Some(4)].map(|g| {
                let label = g.map_or("Off".to_string(), |g| g.to_string());
                seg(label.clone(), choke == g, 20.)
                    .debug_selector(move || format!("pad-choke-{label}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.drum_op(DrumOp::Choke { note, group: g }, cx);
                        }),
                    )
            }),
        );
        let switch = |label: &'static str, on: bool, lit: u32, cx: &mut Context<Self>| {
            toggle(label, on, lit, 0x101014, 22.)
                .debug_selector(move || format!("pad-{}", label.to_lowercase()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.edit(cx, |ws| {
                            let track = &mut ws.session.project.tracks[t];
                            match label {
                                "M" => track.mute ^= true,
                                _ => track.solo ^= true,
                            }
                        });
                    }),
                )
        };
        let open = |label: &'static str, sel: &'static str, to: Bottom, cx: &mut Context<Self>| {
            button(label, false)
                .debug_selector(move || sel.into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.select_track(t);
                        ws.show_bottom(to, cx);
                    }),
                )
        };
        div()
            .h(px(30.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(div().size(px(12.)).rounded(px(4.)).bg(rgb(color)))
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight::BOLD)
                    .whitespace_nowrap()
                    .child(name),
            )
            .child(chip(role.name(), color))
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(format!("{} · {note}", note_name(note))),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child("Choke"),
            )
            .child(chokes)
            .child(
                div()
                    .flex()
                    .gap(px(3.))
                    .child(switch("M", muted, TEXT_2, cx))
                    .child(switch("S", solo, SOLO, cx)),
            )
            .child(open("Sampler", "pad-sampler", Bottom::Sampler, cx))
            .child(
                icon_seg(Icon::Trash, false, 26.)
                    .debug_selector(|| "pad-remove".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.drum_op(DrumOp::Remove { note }, cx);
                        }),
                    ),
            )
    }

    /// The pad's sound, its picked layer's: stepped through (the folder's others, then the
    /// library's that sound like it) or picked from them. One sound shows its name and folder;
    /// layered, a tab for each, by name. Then its layers: adding one, how they play, taking one
    /// out. A sample dropped on it is layered on.
    fn pad_sound(&mut self, kit: usize, note: u8, color: u32, cx: &mut Context<Self>) -> Div {
        let layer = self.pad_layer(kit, note);
        let (list, at) = self.pad_lane(kit, note, layer);
        let samples = &self.browser.library.samples;
        // Each layer's name and colour, and the picked one's folder.
        let mut folder = "Not in the library".to_string();
        let mut names: Vec<(String, u32)> = Vec::new();
        let (zones, layering, blends) = match self.drum_pad(kit, note) {
            Some((_, s)) => (s.zones.clone(), s.layering(), s.blends()),
            None => (Vec::new(), None, false),
        };
        for (i, z) in zones.iter().enumerate() {
            let from = z.from.as_ref().or(z.file.is_absolute().then_some(&z.file));
            match from.and_then(|f| samples.iter().find(|s| &s.path == f)) {
                Some(s) => {
                    if i == layer {
                        folder = s.folder.clone();
                    }
                    names.push((s.name.clone(), category_color(s.analysis.category)));
                }
                None => names.push((
                    z.file
                        .file_stem()
                        .map_or(String::new(), |n| n.to_string_lossy().into_owned()),
                    color,
                )),
            }
        }
        let dropping = match self.drag {
            Some(Drag::Sample {
                at, moved: true, ..
            }) => self.strip_under(at) == Some(note),
            _ => false,
        };
        let step = |i: Icon, by: i32, sel: &'static str, cx: &mut Context<Self>| {
            icon_seg(i, false, 26.)
                .debug_selector(move || sel.into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.step_pad(note, by, cx);
                    }),
                )
        };
        let count = match list.len() {
            0 => "No others like it".to_string(),
            n => format!("{} of {n}", at + 1),
        };
        let text = |s: String, size: f32, c: u32| {
            div()
                .min_w(px(0.))
                .text_size(px(size))
                .text_color(rgb(c))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(s)
        };
        // Clicked, the picked sound's menu; a tab of another layer picks it instead.
        let sound = div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .items_center()
            .gap(px(4.))
            .cursor_pointer()
            .debug_selector(|| "pad-sound".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.sound_menu(kit, note, e.position, cx);
                }),
            );
        let sound = match names.len() {
            _ if dropping => sound
                .gap(px(8.))
                .child(icon(Icon::Stack, 11.).text_color(rgb(ACCENT)))
                .child(
                    text("Layer it on this pad".into(), 12.5, TEXT)
                        .flex_none()
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .child(text("It plays with what's on it".into(), 11., TEXT_3)),
            0 | 1 => {
                let name = names.pop().map_or(String::new(), |n| n.0);
                sound
                    .gap(px(8.))
                    .child(div().size(px(7.)).rounded(px(4.)).bg(rgb(color)))
                    .child(
                        text(name, 12.5, TEXT)
                            .flex_none()
                            .max_w(px(220.))
                            .font_weight(FontWeight::SEMIBOLD),
                    )
                    .child(text(folder, 11., TEXT_3))
            }
            n => sound
                .children(names.into_iter().enumerate().map(|(i, (name, c))| {
                    let picked = i == layer;
                    div()
                        .h(px(24.))
                        .min_w(px(34.))
                        .max_w(px(150.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .rounded(px(6.))
                        .border_1()
                        .map(|d| match picked {
                            true => d.bg(rgb(RAISED)).border_color(alpha(c, 0.55)),
                            false => d
                                .border_color(gpui::transparent_black())
                                .hover(|d| d.bg(rgb(FIELD))),
                        })
                        .debug_selector(move || format!("pad-layer-{i}"))
                        .child(div().flex_none().size(px(6.)).rounded(px(3.)).bg(rgb(c)))
                        .child(
                            text(name, 11.5, if picked { TEXT } else { TEXT_3 }).font_weight(
                                match picked {
                                    true => FontWeight::SEMIBOLD,
                                    false => FontWeight::MEDIUM,
                                },
                            ),
                        )
                        .when(!picked, |d| {
                            d.on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.drums.layer = (note, i);
                                    cx.notify();
                                }),
                            )
                        })
                }))
                .when(n < PAD_LAYERS, |d| {
                    d.child(self.add_layer(icon_seg(Icon::Plus, false, 22.), kit, note, cx))
                }),
        };
        let strip = self.drums.strip.clone();
        div()
            .relative()
            .h(px(36.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(6.))
            .rounded(px(10.))
            .bg(match dropping {
                true => alpha(ACCENT, 0.12),
                false => rgb(0x131318).into(),
            })
            .border_1()
            .border_color(rgb(if dropping { ACCENT } else { LINE_SOFT }))
            .child(
                canvas(move |b, _, _| strip.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(step(Icon::Back, -1, "pad-back", cx))
            .child(sound)
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .whitespace_nowrap()
                    .child(count),
            )
            .child(step(Icon::Forward, 1, "pad-next", cx))
            .child(div().w(px(1.)).h(px(20.)).bg(rgb(LINE_SOFT)))
            .child(self.pad_layers(
                kit,
                note,
                (layer, zones.len()),
                (layering, blends),
                color,
                cx,
            ))
    }

    /// A way to layer another sound on a pad: what goes with it, from a menu.
    fn add_layer(
        &self,
        d: Div,
        kit: usize,
        note: u8,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        d.debug_selector(|| "pad-add-layer".into())
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
            )
    }

    /// How a pad's layers play (and, by velocity, whether they blend), and taking the picked
    /// one out. With one, a way to layer another.
    fn pad_layers(
        &self,
        kit: usize,
        note: u8,
        (layer, layers): (usize, usize),
        (layering, blends): (Option<Layering>, bool),
        color: u32,
        cx: &mut Context<Self>,
    ) -> Div {
        let row = div().flex_none().flex().items_center().gap(px(6.));
        if layers < 2 {
            let label = div()
                .h(px(26.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(7.))
                .bg(rgb(FIELD))
                .hover(|d| d.bg(rgb(RAISED)))
                .cursor_pointer()
                .text_size(px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(TEXT_2))
                .child(icon(Icon::Plus, 10.).text_color(rgb(TEXT_3)))
                .child("Layer");
            return row.child(self.add_layer(label, kit, note, cx));
        }
        let help = |l: Layering| match l {
            Layering::Stack => "Stack: every layer plays at once",
            Layering::Cycle => {
                "Cycle: they take turns, a round robin, so no two hits in a row match"
            }
            Layering::Velocity => "Velocity: soft hits play the first layer, hard ones the last",
        };
        let how = tray()
            .p(px(2.))
            .rounded(px(7.))
            .children(Layering::ALL.map(|l| {
                seg(l.name(), layering == Some(l), 22.)
                    .debug_selector(move || format!("pad-layering-{}", l.name().to_lowercase()))
                    .help(help(l), cx)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.drum_op(DrumOp::Layering { note, how: l }, cx);
                        }),
                    )
            }));
        let blend = crate::editors::sampler::pill("Blend", blends, color)
            .debug_selector(|| "pad-blend".into())
            .help(
                "Blend: velocity layers fade into each other, rather than switching at a hardness",
                cx,
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.drum_op(DrumOp::Blend { note, on: !blends }, cx);
                }),
            );
        let remove = icon_seg(Icon::Close, false, 22.)
            .debug_selector(|| "pad-remove-layer".into())
            .help("Take the picked layer off the pad", cx)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.drum_op(DrumOp::Unlayer { note, layer }, cx);
                }),
            );
        row.child(how)
            .when(layering == Some(Layering::Velocity), |d| d.child(blend))
            .child(remove)
    }

    /// An empty pad: what it'd be, and how to give it a sound.
    fn empty_pad(&mut self, kit: usize, note: u8, cx: &mut Context<Self>) -> Div {
        let role = gm_role(note);
        let color = category_color(role);
        div().flex_1().flex().items_center().justify_center().child(
            div()
                .w(px(380.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(10.))
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(12.))
                        .text_color(rgb(TEXT_3))
                        .child(format!("{} · {note}", note_name(note))),
                )
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::BOLD)
                        .child("An empty pad"),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(TEXT_3))
                        .text_center()
                        .child(
                            "Drop a sample on it, from the browser or the Finder, or pick \
                                 one from the library.",
                        ),
                )
                .child(
                    button(format!("Pick a {}", role.name().to_lowercase()), false)
                        .child(div().size(px(7.)).rounded(px(4.)).bg(rgb(color)))
                        .flex_row_reverse()
                        .debug_selector(|| "pad-pick".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.sound_menu(kit, note, e.position, cx);
                            }),
                        ),
                ),
        )
    }
}
