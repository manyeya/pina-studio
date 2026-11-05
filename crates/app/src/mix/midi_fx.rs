//! A track's MIDI effects: its strip's MIDI FX slot, and the panel it opens, a card for each
//! effect (its settings, bypass, order) and one of each kind to add.

use std::rc::Rc;

use gpui::{
    Div, FontWeight, MouseButton, MouseDownEvent, Pixels, Point, anchored, deferred, div,
    prelude::*, px, rgb,
};
use pina_engine::song::project::{ArpOrder, ChordShape, MidiFx, MidiFxSlot};

use crate::{ui::theme::*, workspace::Workspace};

/// The MIDI FX panel open: its track, and where its slot was clicked.
pub struct MidiFxView {
    pub track: usize,
    pub at: Point<Pixels>,
}

/// A change to one effect's settings.
type Set = Rc<dyn Fn(&mut MidiFx)>;

fn set(f: impl Fn(&mut MidiFx) + 'static) -> Set {
    Rc::new(f)
}

/// The arpeggiator's and repeater's rates, in beats.
const RATES: [(&str, f32); 6] = [
    ("1/4", 1.0),
    ("1/8", 0.5),
    ("1/8T", 1.0 / 3.0),
    ("1/16", 0.25),
    ("1/16T", 1.0 / 6.0),
    ("1/32", 0.125),
];

const ROOTS: [&str; 12] = [
    "C", "C♯", "D", "E♭", "E", "F", "F♯", "G", "A♭", "A", "B♭", "B",
];

fn label(s: impl Into<gpui::SharedString>) -> Div {
    div()
        .w(px(64.))
        .flex_none()
        .text_size(px(11.))
        .text_color(rgb(TEXT_3))
        .child(s.into())
}

/// A row's chip: picked or not.
fn chip(text: String, on: bool) -> Div {
    div()
        .h(px(20.))
        .px(px(7.))
        .flex()
        .items_center()
        .rounded(px(5.))
        .text_size(px(10.5))
        .cursor_pointer()
        .bg(if on {
            alpha(ACCENT, 0.2)
        } else {
            rgb(FIELD).into()
        })
        .text_color(rgb(if on { TEXT } else { TEXT_2 }))
        .when(!on, |d| d.hover(|s| s.bg(rgb(RAISED))))
        .child(text)
}

impl Workspace {
    /// Changes one of a track's MIDI effects, as an edit.
    fn change_midi_fx(&mut self, t: usize, i: usize, f: &Set, cx: &mut gpui::Context<Self>) {
        self.edit(cx, |ws| {
            if let Some(s) = ws.session.project.tracks[t].midi_fx.get_mut(i) {
                f(&mut s.fx);
            }
        });
    }

    /// Changes a track's MIDI effects themselves (adds, takes out, moves, bypasses), as an edit.
    fn change_midi_fx_list(
        &mut self,
        t: usize,
        cx: &mut gpui::Context<Self>,
        f: impl FnOnce(&mut Vec<MidiFxSlot>),
    ) {
        self.edit(cx, |ws| f(&mut ws.session.project.tracks[t].midi_fx));
    }

    /// A row of choices, the one an effect has picked.
    fn fx_choices(
        &self,
        (t, i): (usize, usize),
        name: &'static str,
        items: Vec<(String, bool, Set)>,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        div()
            .flex()
            .items_start()
            .gap(px(6.))
            .child(label(name).pt(px(3.)))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_wrap()
                    .gap(px(3.))
                    .children(items.into_iter().map(|(text, on, f)| {
                        let id = format!("midi-fx-{i}-{name}-{text}");
                        chip(text, on)
                            .debug_selector(move || id.clone())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.change_midi_fx(t, i, &f, cx);
                                }),
                            )
                    })),
            )
    }

    /// A number to step down and up.
    fn fx_stepper(
        &self,
        (t, i): (usize, usize),
        name: &'static str,
        value: String,
        down: Set,
        up: Set,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let step = |sign: &'static str, f: Set, cx: &mut gpui::Context<Self>| {
            div()
                .size(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .bg(rgb(FIELD))
                .text_color(rgb(TEXT_2))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(RAISED)))
                .debug_selector(move || format!("midi-fx-{i}-{name}-{sign}"))
                .child(sign)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.change_midi_fx(t, i, &f, cx);
                    }),
                )
        };
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(label(name))
            .child(
                div()
                    .w(px(60.))
                    .h(px(20.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .bg(rgb(WELL))
                    .font_family(MONO)
                    .text_size(px(11.))
                    .child(value),
            )
            .child(step("−", down, cx))
            .child(step("+", up, cx))
    }

    /// An effect's settings, as controls.
    fn midi_fx_settings(
        &self,
        t: usize,
        i: usize,
        fx: MidiFx,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let at = (t, i);
        let rates = |now: f32, pick: fn(&mut MidiFx, f32)| -> Vec<(String, bool, Set)> {
            RATES
                .iter()
                .map(|&(name, r)| {
                    (
                        name.to_string(),
                        (now - r).abs() < 1e-3,
                        set(move |f| pick(f, r)),
                    )
                })
                .collect()
        };
        let body = div().flex().flex_col().gap(px(5.));
        match fx {
            MidiFx::Arpeggiator {
                rate,
                order,
                octaves,
                gate,
                latch,
            } => body
                .child(self.fx_choices(
                    at,
                    "Rate",
                    rates(rate, |f, r| {
                        if let MidiFx::Arpeggiator { rate, .. } = f {
                            *rate = r
                        }
                    }),
                    cx,
                ))
                .child(
                    self.fx_choices(
                        at,
                        "Order",
                        ArpOrder::ALL
                            .iter()
                            .map(|&o| {
                                let pick = set(move |f| {
                                    if let MidiFx::Arpeggiator { order, .. } = f {
                                        *order = o
                                    }
                                });
                                (o.name().to_string(), o == order, pick)
                            })
                            .collect(),
                        cx,
                    ),
                )
                .child(
                    self.fx_choices(
                        at,
                        "Octaves",
                        (1..=4u8)
                            .map(|n| {
                                let pick = set(move |f| {
                                    if let MidiFx::Arpeggiator { octaves, .. } = f {
                                        *octaves = n
                                    }
                                });
                                (n.to_string(), n == octaves, pick)
                            })
                            .collect(),
                        cx,
                    ),
                )
                .child(self.fx_stepper(
                    at,
                    "Gate",
                    format!("{:.0}%", gate * 100.0),
                    set(|f| {
                        if let MidiFx::Arpeggiator { gate, .. } = f {
                            *gate = (*gate - 0.1).max(0.1)
                        }
                    }),
                    set(|f| {
                        if let MidiFx::Arpeggiator { gate, .. } = f {
                            *gate = (*gate + 0.1).min(1.0)
                        }
                    }),
                    cx,
                ))
                .child(
                    self.fx_choices(
                        at,
                        "Latch",
                        [("Off", false), ("On", true)]
                            .map(|(name, on)| {
                                let pick = set(move |f| {
                                    if let MidiFx::Arpeggiator { latch, .. } = f {
                                        *latch = on
                                    }
                                });
                                (name.to_string(), on == latch, pick)
                            })
                            .to_vec(),
                        cx,
                    ),
                ),
            MidiFx::ChordTrigger { shape } => body.child(
                self.fx_choices(
                    at,
                    "Chord",
                    ChordShape::ALL
                        .iter()
                        .map(|&c| {
                            let pick = set(move |f| *f = MidiFx::ChordTrigger { shape: c });
                            (c.name().to_string(), c == shape, pick)
                        })
                        .collect(),
                    cx,
                ),
            ),
            MidiFx::ScaleQuantizer { root, minor } => body
                .child(
                    self.fx_choices(
                        at,
                        "Key",
                        std::iter::once(("Song's".to_string(), root.is_none(), None))
                            .chain(
                                (0..12u8).map(|r| {
                                    (ROOTS[r as usize].to_string(), root == Some(r), Some(r))
                                }),
                            )
                            .map(|(name, on, r)| {
                                let pick = set(move |f| {
                                    if let MidiFx::ScaleQuantizer { root, .. } = f {
                                        *root = r
                                    }
                                });
                                (name, on, pick)
                            })
                            .collect(),
                        cx,
                    ),
                )
                .when(root.is_some(), |d| {
                    d.child(
                        self.fx_choices(
                            at,
                            "Scale",
                            [("Major", false), ("Minor", true)]
                                .map(|(name, m)| {
                                    let pick = set(move |f| {
                                        if let MidiFx::ScaleQuantizer { minor, .. } = f {
                                            *minor = m
                                        }
                                    });
                                    (name.to_string(), m == minor, pick)
                                })
                                .to_vec(),
                            cx,
                        ),
                    )
                }),
            MidiFx::Transposer { semitones } => body.child(self.fx_stepper(
                at,
                "Semitones",
                format!("{semitones:+}"),
                set(|f| {
                    if let MidiFx::Transposer { semitones } = f {
                        *semitones = (*semitones - 1).max(-48)
                    }
                }),
                set(|f| {
                    if let MidiFx::Transposer { semitones } = f {
                        *semitones = (*semitones + 1).min(48)
                    }
                }),
                cx,
            )),
            MidiFx::Velocity {
                scale,
                add,
                min,
                max,
            } => body
                .child(self.fx_stepper(
                    at,
                    "Scale",
                    format!("{scale}%"),
                    set(|f| {
                        if let MidiFx::Velocity { scale, .. } = f {
                            *scale = scale.saturating_sub(10)
                        }
                    }),
                    set(|f| {
                        if let MidiFx::Velocity { scale, .. } = f {
                            *scale = (*scale + 10).min(200)
                        }
                    }),
                    cx,
                ))
                .child(self.fx_stepper(
                    at,
                    "Add",
                    format!("{add:+}"),
                    set(|f| {
                        if let MidiFx::Velocity { add, .. } = f {
                            *add = add.saturating_sub(5).max(-127)
                        }
                    }),
                    set(|f| {
                        if let MidiFx::Velocity { add, .. } = f {
                            *add = add.saturating_add(5)
                        }
                    }),
                    cx,
                ))
                .child(self.fx_stepper(
                    at,
                    "Lowest",
                    min.to_string(),
                    set(|f| {
                        if let MidiFx::Velocity { min, .. } = f {
                            *min = min.saturating_sub(5).max(1)
                        }
                    }),
                    set(|f| {
                        if let MidiFx::Velocity { min, max, .. } = f {
                            *min = (*min + 5).min(*max)
                        }
                    }),
                    cx,
                ))
                .child(self.fx_stepper(
                    at,
                    "Highest",
                    max.to_string(),
                    set(|f| {
                        if let MidiFx::Velocity { min, max, .. } = f {
                            *max = max.saturating_sub(5).max(*min)
                        }
                    }),
                    set(|f| {
                        if let MidiFx::Velocity { max, .. } = f {
                            *max = (*max + 5).min(127)
                        }
                    }),
                    cx,
                )),
            MidiFx::NoteRepeater {
                count,
                rate,
                decay,
                transpose,
            } => body
                .child(self.fx_stepper(
                    at,
                    "Repeats",
                    count.to_string(),
                    set(|f| {
                        if let MidiFx::NoteRepeater { count, .. } = f {
                            *count = count.saturating_sub(1).max(1)
                        }
                    }),
                    set(|f| {
                        if let MidiFx::NoteRepeater { count, .. } = f {
                            *count = (*count + 1).min(16)
                        }
                    }),
                    cx,
                ))
                .child(self.fx_choices(
                    at,
                    "Rate",
                    rates(rate, |f, r| {
                        if let MidiFx::NoteRepeater { rate, .. } = f {
                            *rate = r
                        }
                    }),
                    cx,
                ))
                .child(self.fx_stepper(
                    at,
                    "Each",
                    format!("{:.0}%", decay * 100.0),
                    set(|f| {
                        if let MidiFx::NoteRepeater { decay, .. } = f {
                            *decay = (*decay - 0.1).max(0.1)
                        }
                    }),
                    set(|f| {
                        if let MidiFx::NoteRepeater { decay, .. } = f {
                            *decay = (*decay + 0.1).min(1.0)
                        }
                    }),
                    cx,
                ))
                .child(self.fx_stepper(
                    at,
                    "Moves",
                    format!("{transpose:+}"),
                    set(|f| {
                        if let MidiFx::NoteRepeater { transpose, .. } = f {
                            *transpose = (*transpose - 1).max(-24)
                        }
                    }),
                    set(|f| {
                        if let MidiFx::NoteRepeater { transpose, .. } = f {
                            *transpose = (*transpose + 1).min(24)
                        }
                    }),
                    cx,
                )),
            MidiFx::Randomizer {
                pitch,
                velocity,
                chance,
            } => body
                .child(self.fx_stepper(
                    at,
                    "Pitch",
                    format!("±{pitch}"),
                    set(|f| {
                        if let MidiFx::Randomizer { pitch, .. } = f {
                            *pitch = pitch.saturating_sub(1)
                        }
                    }),
                    set(|f| {
                        if let MidiFx::Randomizer { pitch, .. } = f {
                            *pitch = (*pitch + 1).min(24)
                        }
                    }),
                    cx,
                ))
                .child(self.fx_stepper(
                    at,
                    "Velocity",
                    format!("±{velocity}"),
                    set(|f| {
                        if let MidiFx::Randomizer { velocity, .. } = f {
                            *velocity = velocity.saturating_sub(5)
                        }
                    }),
                    set(|f| {
                        if let MidiFx::Randomizer { velocity, .. } = f {
                            *velocity = (*velocity + 5).min(127)
                        }
                    }),
                    cx,
                ))
                .child(self.fx_stepper(
                    at,
                    "Chance",
                    format!("{chance}%"),
                    set(|f| {
                        if let MidiFx::Randomizer { chance, .. } = f {
                            *chance = chance.saturating_sub(5)
                        }
                    }),
                    set(|f| {
                        if let MidiFx::Randomizer { chance, .. } = f {
                            *chance = (*chance + 5).min(100)
                        }
                    }),
                    cx,
                )),
            MidiFx::Modifier { from, to, scale } => body
                .child(self.fx_stepper(
                    at,
                    "From CC",
                    from.to_string(),
                    set(|f| {
                        if let MidiFx::Modifier { from, .. } = f {
                            *from = from.saturating_sub(1)
                        }
                    }),
                    set(|f| {
                        if let MidiFx::Modifier { from, .. } = f {
                            *from = (*from + 1).min(127)
                        }
                    }),
                    cx,
                ))
                .child(self.fx_stepper(
                    at,
                    "To CC",
                    match to {
                        0 => "none".to_string(),
                        n => n.to_string(),
                    },
                    set(|f| {
                        if let MidiFx::Modifier { to, .. } = f {
                            *to = to.saturating_sub(1)
                        }
                    }),
                    set(|f| {
                        if let MidiFx::Modifier { to, .. } = f {
                            *to = (*to + 1).min(127)
                        }
                    }),
                    cx,
                ))
                .child(self.fx_stepper(
                    at,
                    "Scale",
                    format!("{scale}%"),
                    set(|f| {
                        if let MidiFx::Modifier { scale, .. } = f {
                            *scale = scale.saturating_sub(10)
                        }
                    }),
                    set(|f| {
                        if let MidiFx::Modifier { scale, .. } = f {
                            *scale = (*scale + 10).min(200)
                        }
                    }),
                    cx,
                )),
        }
    }

    /// One effect's card: its name, bypass, up and down, out; its settings.
    fn midi_fx_card(
        &self,
        t: usize,
        i: usize,
        n: usize,
        slot: MidiFxSlot,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let button = |sign: &'static str,
                      show: bool,
                      cx: &mut gpui::Context<Self>,
                      f: fn(&mut Vec<MidiFxSlot>, usize)| {
            div()
                .size(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .text_color(rgb(TEXT_3))
                .when(!show, |d| d.invisible())
                .cursor_pointer()
                .hover(|s| s.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                .debug_selector(move || format!("midi-fx-{i}-{sign}"))
                .child(sign)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.change_midi_fx_list(t, cx, |list| f(list, i));
                    }),
                )
        };
        div()
            .p(px(10.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .rounded(px(9.))
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(LINE))
            .when(slot.bypass, |d| d.opacity(0.55))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(if slot.bypass {
                        TEXT_4
                    } else {
                        ACCENT
                    })))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(slot.fx.name()),
                    )
                    .child(
                        switch(!slot.bypass)
                            .debug_selector(move || format!("midi-fx-{i}-on"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.change_midi_fx_list(t, cx, |list| {
                                        if let Some(s) = list.get_mut(i) {
                                            s.bypass ^= true;
                                        }
                                    });
                                }),
                            ),
                    )
                    .child(button("↑", i > 0, cx, |list, i| list.swap(i - 1, i)))
                    .child(button("↓", i + 1 < n, cx, |list, i| list.swap(i, i + 1)))
                    .child(button("×", true, cx, |list, i| {
                        list.remove(i);
                    })),
            )
            .child(self.midi_fx_settings(t, i, slot.fx, cx))
    }

    /// A track's MIDI FX slot on its strip: what it runs, or a place to add one.
    pub(crate) fn midi_fx_slot(&self, t: usize, h: f32, cx: &mut gpui::Context<Self>) -> Div {
        let list = &self.session.project.tracks[t].midi_fx;
        let names: Vec<&str> = list
            .iter()
            .filter(|s| !s.bypass)
            .map(|s| s.fx.name())
            .collect();
        let used = !list.is_empty();
        crate::mix::strip::dotted(
            match (used, names.is_empty()) {
                (false, _) => "+".to_string(),
                (true, true) => "Bypassed".to_string(),
                (true, false) => names.join(" · "),
            },
            used,
            h,
            used.then_some(ACCENT),
        )
        .debug_selector(move || format!("midi-fx-slot-{t}"))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                ws.menu = None;
                ws.midi_fx_view = Some(MidiFxView {
                    track: t,
                    at: e.position,
                });
                cx.notify();
            }),
        )
    }

    /// The MIDI FX panel, over everything, where its slot was clicked.
    pub fn midi_fx_window(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let v = self.midi_fx_view.as_ref()?;
        let (t, at) = (v.track, v.at);
        let track = self.session.project.tracks.get(t)?;
        let list = track.midi_fx.clone();
        let name = track.name.clone();
        let n = list.len();
        let cards: Vec<Div> = list
            .into_iter()
            .enumerate()
            .map(|(i, slot)| self.midi_fx_card(t, i, n, slot, cx))
            .collect();
        let add = div()
            .flex()
            .flex_wrap()
            .gap(px(4.))
            .children(MidiFx::KINDS.map(|kind| {
                chip(format!("+ {kind}"), false)
                    .debug_selector(move || format!("midi-fx-add-{kind}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(fx) = MidiFx::fresh(kind) {
                                ws.change_midi_fx_list(t, cx, |list| {
                                    list.push(MidiFxSlot { fx, bypass: false })
                                });
                            }
                        }),
                    )
            }));
        Some(
            deferred(
                anchored().position(at).snap_to_window().child(
                    div()
                        .id("midi-fx-panel")
                        .w(px(400.))
                        .max_h(px(620.))
                        .overflow_y_scroll()
                        .p(px(10.))
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .rounded(px(12.))
                        .bg(rgb(0x1c1c22))
                        .border_1()
                        .border_color(rgb(LINE_STRONG))
                        .shadow_lg()
                        .on_mouse_down_out(cx.listener(|ws, _, _, cx| {
                            ws.midi_fx_view = None;
                            cx.notify();
                        }))
                        .child(
                            div()
                                .flex()
                                .items_baseline()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("MIDI FX"),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(TEXT_3))
                                        .child(format!("{name} · they change its notes in this order, before it plays them")),
                                ),
                        )
                        .children(cards)
                        .when(n == 0, |d| {
                            d.child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .child("None yet: add one below."),
                            )
                        })
                        .child(add),
                ),
            )
            .with_priority(2),
        )
    }
}
