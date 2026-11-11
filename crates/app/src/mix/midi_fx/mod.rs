//! A track's MIDI effects: its strip's MIDI FX slot, and the window it opens: the chain down its
//! left (each one on or off, moved, taken out), the one picked on the right, drawn as it plays,
//! with its dials and choices; a picker of every kind to add; and a chord to hear them by.

mod dials;
mod pictures;

use std::rc::Rc;

use gpui::{
    Div, FontWeight, MouseButton, MouseDownEvent, Pixels, Point, Stateful, anchored, deferred, div,
    point, prelude::*, px, rgb,
};
use pina_engine::song::project::{ArpOrder, ChordShape, MidiFx, MidiFxSlot};

pub use dials::Dial;

use crate::{
    mix::eq::{bar_button, dial, value_line},
    ui::theme::*,
    workspace::{Drag, Workspace, pt},
};

/// The MIDI FX window open: its track, where it is, the effect picked, and whether the kinds
/// to add are showing.
pub struct MidiFxView {
    pub track: usize,
    pub at: Point<Pixels>,
    pub picked: usize,
    pub adding: bool,
    /// A chord being played through the effects, to hear them.
    pub hearing: bool,
}

impl MidiFxView {
    pub fn new(track: usize, at: Point<Pixels>) -> MidiFxView {
        MidiFxView {
            track,
            at,
            picked: 0,
            adding: false,
            hearing: false,
        }
    }
}

const W: f32 = 920.0;
const H: f32 = 580.0;
const HEAD_H: f32 = 48.0;
const CHAIN_W: f32 = 236.0;
const PICTURE_H: f32 = 210.0;
const DIAL: f32 = 56.0;

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

fn rate_name(r: f32) -> &'static str {
    RATES
        .iter()
        .find(|(_, v)| (v - r).abs() < 1e-3)
        .map_or("—", |(n, _)| n)
}

fn kebab(s: &str) -> String {
    s.to_lowercase().replace(' ', "-")
}

/// Each kind's mark.
fn kind_icon(fx: &MidiFx) -> Icon {
    match fx {
        MidiFx::Arpeggiator { .. } => Icon::Note,
        MidiFx::ChordTrigger { .. } => Icon::Keys,
        MidiFx::ScaleQuantizer { .. } => Icon::Scale,
        MidiFx::Transposer { .. } => Icon::Send,
        MidiFx::Velocity { .. } => Icon::Pulse,
        MidiFx::NoteRepeater { .. } => Icon::Loop,
        MidiFx::Randomizer { .. } => Icon::Dice,
        MidiFx::Modifier { .. } => Icon::Settings,
        MidiFx::MpeShaper { .. } => Icon::Curve,
        MidiFx::Plugin { .. } => Icon::Plug,
    }
}

/// What it's set to, in a few words, for the chain.
fn summary(fx: &MidiFx) -> String {
    match *fx {
        MidiFx::Arpeggiator {
            rate,
            order,
            octaves,
            latch,
            ..
        } => format!(
            "{} · {} · {octaves} oct{}",
            rate_name(rate),
            order.name(),
            if latch { " · latch" } else { "" }
        ),
        MidiFx::ChordTrigger { shape } => shape.name().to_string(),
        MidiFx::ScaleQuantizer { root: None, .. } => "The song's key".into(),
        MidiFx::ScaleQuantizer {
            root: Some(r),
            minor,
        } => format!(
            "{} {}",
            ROOTS[r as usize % 12],
            if minor { "minor" } else { "major" }
        ),
        MidiFx::Transposer { .. } => Dial::Semitones.text(fx),
        MidiFx::Velocity { scale, .. } => format!("{scale} % · {}", Dial::VelAdd.text(fx)),
        MidiFx::NoteRepeater { count, rate, .. } => format!("{count} × {}", rate_name(rate)),
        MidiFx::Randomizer { .. } => format!(
            "{} · {}",
            Dial::RandPitch.text(fx),
            Dial::RandChance.text(fx)
        ),
        MidiFx::Modifier { from, to, .. } => match to {
            0 => format!("CC {from} → off"),
            to => format!("CC {from} → CC {to}"),
        },
        MidiFx::MpeShaper { bend, swap, .. } => {
            format!("Bend {bend} %{}", if swap { " · swapped" } else { "" })
        }
        MidiFx::Plugin { .. } => "Plugin".into(),
    }
}

/// A slot's line in the chain: a plugin's format, else its settings.
fn slot_summary(slot: &MidiFxSlot) -> String {
    match &slot.plugin {
        Some(r) => format!("{} plugin", r.format.name()),
        None => summary(&slot.fx),
    }
}

/// The place of the chain's `i`th slot among its plugins (if it's one), as the engine knows it.
fn plugin_place(list: &[MidiFxSlot], i: usize) -> Option<usize> {
    list.get(i)?.plugin.as_ref()?;
    Some(list[..i].iter().filter(|s| s.plugin.is_some()).count())
}

/// A button of a grid of choices, lit when it's the one.
fn choice(text: &str, on: bool) -> Div {
    div()
        .h(px(28.))
        .px(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(7.))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .cursor_pointer()
        .map(|d| match on {
            true => d.bg(rgb(RAISED)).text_color(rgb(TEXT)),
            false => d
                .bg(rgb(0x17181d))
                .text_color(rgb(TEXT_3))
                .hover(|d| d.bg(rgb(OFF)).text_color(rgb(TEXT_2))),
        })
        .child(text.to_string())
}

/// A small square button with an icon, in a bar.
fn square(glyph: Icon, sel: String, danger: bool) -> Stateful<Div> {
    div()
        .id(gpui::SharedString::from(sel.clone()))
        .group("midi-fx-square")
        .size(px(28.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(7.))
        .cursor_pointer()
        .bg(rgb(0x1c1d23))
        .hover(move |d| match danger {
            true => d.bg(alpha(RECORD, 0.16)),
            false => d.bg(rgb(RAISED)),
        })
        .debug_selector(move || sel.clone())
        .child(
            icon(glyph, 14.)
                .text_color(rgb(TEXT_2))
                .group_hover("midi-fx-square", move |d| {
                    d.text_color(rgb(if danger { 0xff7070 } else { TEXT }))
                }),
        )
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
        let n = self.session.project.tracks[t].midi_fx.len();
        if let Some(v) = self.midi_fx_view.as_mut() {
            v.picked = v.picked.min(n.saturating_sub(1));
        }
    }

    /// The chord the window hears and draws the effects by: the song's key's (C major's).
    fn midi_fx_chord(&self) -> Vec<u8> {
        let key = self.session.project.song_key();
        let root = 60 + key.map_or(0, |k| k.root % 12);
        ChordShape::InKey.pitches(root, key)
    }

    /// A track's MIDI FX slot on its strip: what it runs, or a place to add one.
    pub(crate) fn midi_fx_slot(&self, t: usize, h: f32, cx: &mut gpui::Context<Self>) -> Div {
        let list = &self.session.project.tracks[t].midi_fx;
        let names: Vec<&str> = list
            .iter()
            .filter(|s| !s.bypass)
            .map(|s| s.name())
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
        // The "+" sits in the middle, as the effects' does.
        .when(!used, |d| d.justify_center())
        .debug_selector(move || format!("midi-fx-slot-{t}"))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                ws.menu = None;
                // Beside the slot, where there's room.
                let at = point(
                    e.position.x + px(16.),
                    (e.position.y - px(40.)).max(px(60.)),
                );
                let mut v = MidiFxView::new(t, at);
                v.adding = ws.session.project.tracks[t].midi_fx.is_empty();
                ws.midi_fx_view = Some(v);
                cx.notify();
            }),
        )
    }

    /// The MIDI FX window, over everything.
    pub fn midi_fx_window(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let v = self.midi_fx_view.as_ref()?;
        let (t, at, picked, adding, hearing) = (v.track, v.at, v.picked, v.adding, v.hearing);
        let track = self.session.project.tracks.get(t)?;
        let list = track.midi_fx.clone();
        let picked = picked.min(list.len().saturating_sub(1));
        let color = self.color(t);
        let body = match (adding || list.is_empty(), list.get(picked)) {
            (false, Some(slot)) => self
                .midi_fx_editor(t, picked, &list, slot.clone(), color, cx)
                .into_any_element(),
            _ => self
                .midi_fx_picker(t, !list.is_empty(), cx)
                .into_any_element(),
        };
        let frame = div()
            .id("midi-fx-window")
            .occlude()
            .w(px(W))
            .h(px(H))
            .flex()
            .flex_col()
            .rounded(px(12.))
            .overflow_hidden()
            .bg(rgb(0x101115))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .font_family(SANS)
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .debug_selector(|| "midi-fx-window".into())
            .child(self.midi_fx_head(t, list.len(), hearing, color, cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .child(self.midi_fx_chain(t, &list, picked, adding, color, cx))
                    .child(div().flex_1().min_w(px(0.)).flex().flex_col().child(body)),
            );
        Some(deferred(anchored().position(at).snap_to_window().child(frame)).with_priority(2))
    }

    /// Along the top: what it is, whose, a chord to hear it by, adding one, and closing. Drag it
    /// to move the window.
    fn midi_fx_head(
        &self,
        t: usize,
        n: usize,
        hearing: bool,
        color: u32,
        cx: &mut gpui::Context<Self>,
    ) -> Stateful<Div> {
        let track = &self.session.project.tracks[t];
        let instrument = crate::arrange::tracks::instrument_name(track.instrument.as_ref());
        div()
            .id("midi-fx-head")
            .h(px(HEAD_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(14.))
            .bg(rgb(0x14151a))
            .border_b_1()
            .border_color(rgb(LINE))
            .cursor(gpui::CursorStyle::OpenHand)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    let Some(start) = ws.midi_fx_view.as_ref().map(|v| v.at) else {
                        return;
                    };
                    ws.begin_drag(
                        Drag::MidiFxWindow {
                            from: pt(e.position),
                            start,
                        },
                        cx,
                    );
                }),
            )
            .child(
                div()
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .bg(alpha(color, 0.16))
                    .child(icon(Icon::MidiIn, 16.).text_color(rgb(color))),
            )
            .child(
                div()
                    .text_size(px(13.5))
                    .font_weight(FontWeight::BOLD)
                    .child("MIDI FX"),
            )
            .child(
                div()
                    .h(px(26.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(6.))
                    .bg(rgb(0x1c1d23))
                    .text_size(px(11.5))
                    .font_weight(FontWeight::MEDIUM)
                    .child(div().size(px(7.)).rounded(px(4.)).bg(rgb(color)))
                    .child(track.name.clone()),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT_3))
                    .whitespace_nowrap()
                    .child(match n {
                        0 => format!("before {instrument}"),
                        1 => format!("1 effect, before {instrument}"),
                        n => format!("{n} effects, top to bottom, before {instrument}"),
                    }),
            )
            .child(div().flex_1())
            .child(
                bar_button(if hearing { "Playing…" } else { "Hear It" }, hearing, color)
                    .debug_selector(|| "midi-fx-hear".into())
                    .child(icon(Icon::Play, 11.).text_color(rgb(if hearing {
                        color
                    } else {
                        TEXT_3
                    })))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.hear_midi_fx(t, cx);
                        }),
                    ),
            )
            .child(
                square(Icon::Close, "midi-fx-close".into(), false).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.midi_fx_view = None;
                        cx.notify();
                    }),
                ),
            )
    }

    /// Down the left: the effects in the order they change the notes, the one picked lit, and
    /// a way to add one.
    fn midi_fx_chain(
        &self,
        t: usize,
        list: &[MidiFxSlot],
        picked: usize,
        adding: bool,
        color: u32,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let rows = list.iter().enumerate().map(|(i, slot)| {
            let on = !slot.bypass;
            let here = i == picked && !adding;
            div()
                .id(("midi-fx-row", i))
                .h(px(54.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(10.))
                .rounded(px(9.))
                .cursor_pointer()
                .border_1()
                .map(|d| match here {
                    true => d.bg(rgb(0x1c1e26)).border_color(alpha(color, 0.55)),
                    false => d
                        .border_color(gpui::transparent_black())
                        .hover(|d| d.bg(rgb(0x16171c))),
                })
                .debug_selector(move || format!("midi-fx-row-{i}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        if let Some(v) = ws.midi_fx_view.as_mut() {
                            v.picked = i;
                            v.adding = false;
                        }
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .w(px(14.))
                        .flex_none()
                        .font_family(MONO)
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_4))
                        .child((i + 1).to_string()),
                )
                .child(
                    div()
                        .size(px(30.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(8.))
                        .bg(if on {
                            alpha(color, 0.16)
                        } else {
                            rgb(0x1c1d23).into()
                        })
                        .child(icon(kind_icon(&slot.fx), 15.).text_color(rgb(if on {
                            color
                        } else {
                            TEXT_4
                        }))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(if on { TEXT } else { TEXT_3 }))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(slot.name().to_string()),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_3))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(if on { slot_summary(slot) } else { "Off".into() }),
                        ),
                )
                .child(
                    switch(on)
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
        });
        div()
            .w(px(CHAIN_W))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(4.))
            .p(px(10.))
            .bg(rgb(0x0d0e12))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .px(px(4.))
                    .pb(px(4.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT_3))
                    .child("CHAIN"),
            )
            .child(
                div()
                    .id("midi-fx-chain-rows")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .children(rows),
            )
            .when(list.is_empty(), |d| {
                d.child(
                    div()
                        .px(px(4.))
                        .py(px(10.))
                        .text_size(px(11.5))
                        .text_color(rgb(TEXT_3))
                        .child("Nothing yet: pick one to add."),
                )
            })
            .child(
                div()
                    .flex_none()
                    .mt(px(6.))
                    .h(px(36.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(7.))
                    .rounded(px(9.))
                    .border_1()
                    .border_dashed()
                    .cursor_pointer()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .map(|d| match adding {
                        true => d
                            .border_color(alpha(ACCENT, 0.7))
                            .bg(alpha(ACCENT, 0.1))
                            .text_color(rgb(ACCENT_TEXT)),
                        false => d
                            .border_color(rgb(LINE_STRONG))
                            .text_color(rgb(TEXT_2))
                            .hover(|d| d.border_color(rgb(0x4a4a55)).text_color(rgb(TEXT))),
                    })
                    .debug_selector(|| "midi-fx-add-row".into())
                    .child(icon(Icon::Plus, 13.).text_color(rgb(TEXT_3)))
                    .child("Add MIDI Effect")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            if let Some(v) = ws.midi_fx_view.as_mut() {
                                v.adding = true;
                            }
                            cx.notify();
                        }),
                    ),
            )
    }

    /// Every kind, as tiles: what it is and what it does; a click adds it after the rest.
    fn midi_fx_picker(&self, t: usize, back: bool, cx: &mut gpui::Context<Self>) -> Stateful<Div> {
        let tiles = MidiFx::KINDS.map(|kind| {
            let fx = MidiFx::fresh(kind).expect("every kind has a fresh one");
            div()
                .id(kind)
                .group("midi-fx-tile")
                .min_h(px(96.))
                .p(px(12.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .rounded(px(10.))
                .cursor_pointer()
                .bg(rgb(0x15161b))
                .border_1()
                .border_color(rgb(0x24252c))
                .hover(|d| d.bg(rgb(0x1b1c22)).border_color(rgb(0x3a3b45)))
                .debug_selector(move || format!("midi-fx-pick-{kind}"))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(9.))
                        .child(
                            div()
                                .size(px(28.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(7.))
                                .bg(rgb(0x1f2028))
                                .child(icon(kind_icon(&fx), 15.).text_color(rgb(TEXT_2))),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(kind),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(TEXT_3))
                        .child(dials::about(&fx)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        let mut at = 0;
                        ws.change_midi_fx_list(t, cx, |list| {
                            list.push(MidiFxSlot::new(fx));
                            at = list.len() - 1;
                        });
                        if let Some(v) = ws.midi_fx_view.as_mut() {
                            v.picked = at;
                            v.adding = false;
                        }
                        cx.notify();
                    }),
                )
        });
        div()
            .id("midi-fx-picker")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Add a MIDI effect"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(rgb(TEXT_3))
                                    .child("It changes the notes after the ones above it, before the instrument hears them."),
                            ),
                    )
                    .when(back, |d| {
                        d.child(
                            bar_button("Back", false, ACCENT)
                                .debug_selector(|| "midi-fx-picker-back".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| {
                                        if let Some(v) = ws.midi_fx_view.as_mut() {
                                            v.adding = false;
                                        }
                                        cx.notify();
                                    }),
                                ),
                        )
                    }),
            )
            .child(div().grid().grid_cols(3).gap(px(10.)).children(tiles))
            .child(self.midi_fx_plugins(t, cx))
    }

    /// Plugins as MIDI FX: those that say they are (MIDI processors, note effects) as tiles, and
    /// any instrument that plays notes out, as Scaler does, from a list to search.
    fn midi_fx_plugins(&self, t: usize, cx: &mut gpui::Context<Self>) -> Div {
        let found = self.plugins.clone().unwrap_or_default();
        let midi: Vec<_> = found.iter().filter(|p| p.midi_effect()).cloned().collect();
        let tiles = midi.into_iter().take(9).map(|p| {
            let r = p.plugin_ref();
            let sel = format!("midi-fx-plugin-{}", p.class);
            div()
                .id(gpui::SharedString::from(sel.clone()))
                .h(px(44.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(9.))
                .rounded(px(9.))
                .cursor_pointer()
                .bg(rgb(0x15161b))
                .border_1()
                .border_color(rgb(0x24252c))
                .hover(|d| d.bg(rgb(0x1b1c22)).border_color(rgb(0x3a3b45)))
                .debug_selector(move || sel.clone())
                .child(icon(Icon::Plug, 14.).text_color(rgb(TEXT_2)))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(p.class.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_3))
                                .child(format!("{} · {}", p.vendor, p.format.name())),
                        ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.add_midi_fx_plugin(t, r.clone(), cx);
                    }),
                )
        });
        let any = bar_button("Any Plugin…", false, ACCENT)
            .debug_selector(|| "midi-fx-any-plugin".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let found = ws.plugins.clone().unwrap_or_default();
                    let mut picks: Vec<_> = found
                        .iter()
                        .filter(|p| p.midi_effect() || p.instrument)
                        .collect();
                    picks.sort_by_key(|p| (!p.midi_effect(), p.class.to_lowercase()));
                    let items: Vec<_> = picks
                        .into_iter()
                        .map(|p| {
                            (
                                format!("{} · {} ({})", p.class, p.vendor, p.format.name()),
                                crate::ui::menu::MenuAction::MidiFxPlugin(t, p.plugin_ref()),
                            )
                        })
                        .collect();
                    if items.is_empty() {
                        ws.status = Some("No plugins found yet: they're still being looked for, or none are installed".into());
                        return cx.notify();
                    }
                    ws.menu = Some(crate::ui::menu::Menu::search(e.position, items));
                    cx.notify();
                }),
            );
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .pt(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Plugins"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(rgb(TEXT_3))
                                    .child("MIDI effects, or an instrument that plays notes out (Scaler): its notes go on down the chain."),
                            ),
                    )
                    .child(any),
            )
            .child(div().grid().grid_cols(3).gap(px(8.)).children(tiles))
    }

    /// A plugin slot's panel: it, and its own window to set it up in.
    fn midi_fx_plugin_panel(
        &self,
        t: usize,
        place: usize,
        r: &pina_engine::song::project::PluginRef,
        color: u32,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let instrument = crate::arrange::tracks::instrument_name(
            self.session.project.tracks[t].instrument.as_ref(),
        );
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(14.))
            .rounded(px(10.))
            .bg(rgb(0x0b0c10))
            .border_1()
            .border_color(rgb(0x1f2027))
            .child(
                div()
                    .size(px(64.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(16.))
                    .bg(alpha(color, 0.14))
                    .child(icon(Icon::Plug, 30.).text_color(rgb(color))),
            )
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(FontWeight::BOLD)
                    .child(r.class.clone()),
            )
            .child(
                div()
                    .max_w(px(460.))
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_3))
                    .text_center()
                    .child(format!(
                        "It hears the notes above it in the chain; what it plays goes on to {instrument}. Set it up in its own window."
                    )),
            )
            .child(
                div()
                    .h(px(36.))
                    .px(px(18.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(9.))
                    .cursor_pointer()
                    .bg(rgb(ACCENT))
                    .text_color(rgb(0xffffff))
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .hover(|d| d.bg(rgb(tint(ACCENT, 0.12))))
                    .debug_selector(|| "midi-fx-open-plugin".into())
                    .child(icon(Icon::Expand, 14.).text_color(rgb(0xffffff)))
                    .child(format!("Open {}", r.class))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.open_editor(
                                crate::mix::strip::Channel::Track(t),
                                pina_engine::song::project::Which::MidiFx(place),
                                cx,
                            );
                        }),
                    ),
            )
    }

    /// A plugin put at the end of a track's MIDI FX, picked, its window opening once it's loaded.
    pub fn add_midi_fx_plugin(
        &mut self,
        t: usize,
        r: pina_engine::song::project::PluginRef,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut at = 0;
        self.change_midi_fx_list(t, cx, |list| {
            list.push(MidiFxSlot::plugin(r));
            at = list.len() - 1;
        });
        if let Some(v) = self.midi_fx_view.as_mut() {
            v.picked = at;
            v.adding = false;
        }
        let list = &self.session.project.tracks[t].midi_fx;
        if let Some(place) = plugin_place(list, at) {
            self.open_editor_when_loaded(
                crate::mix::strip::Channel::Track(t),
                pina_engine::song::project::Which::MidiFx(place),
                cx,
            );
        }
        cx.notify();
    }

    /// The effect picked: its name and what it does, on or off, moved, taken out; drawn as it
    /// plays; its dials and its choices.
    fn midi_fx_editor(
        &self,
        t: usize,
        i: usize,
        list: &[MidiFxSlot],
        slot: MidiFxSlot,
        color: u32,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let n = list.len();
        let on = !slot.bypass;
        let tone = if on { color } else { TEXT_4 };
        let fx = slot.fx;
        let list_op = |glyph: Icon,
                       name: &'static str,
                       show: bool,
                       danger: bool,
                       f: fn(&mut Vec<MidiFxSlot>, usize) -> usize,
                       cx: &mut gpui::Context<Self>| {
            square(glyph, format!("midi-fx-{i}-{name}"), danger)
                .when(!show, |d| d.invisible())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        let mut to = i;
                        ws.change_midi_fx_list(t, cx, |list| to = f(list, i));
                        if let Some(v) = ws.midi_fx_view.as_mut() {
                            v.picked = to;
                        }
                        cx.notify();
                    }),
                )
        };
        let head = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(12.))
            .child(
                switch(on)
                    .debug_selector(move || format!("midi-fx-{i}-power"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.change_midi_fx_list(t, cx, |list| {
                                if let Some(s) = list.get_mut(i) {
                                    s.bypass ^= true;
                                }
                            });
                        }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .items_baseline()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::BOLD)
                            .child(slot.name().to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(match &slot.plugin {
                                Some(r) => format!("{} · {}", r.format.name(), dials::about(&fx)),
                                None => dials::about(&fx).to_string(),
                            }),
                    ),
            )
            .child(list_op(
                Icon::ChevronUp,
                "up",
                i > 0,
                false,
                |l, i| {
                    l.swap(i - 1, i);
                    i - 1
                },
                cx,
            ))
            .child(list_op(
                Icon::Chevron,
                "down",
                i + 1 < n,
                false,
                |l, i| {
                    l.swap(i, i + 1);
                    i + 1
                },
                cx,
            ))
            .child(list_op(
                Icon::Trash,
                "remove",
                true,
                true,
                |l, i| {
                    l.remove(i);
                    i.saturating_sub(1)
                },
                cx,
            ));
        // A plugin: its own window is where it's set up.
        if let (Some(r), Some(place)) = (&slot.plugin, plugin_place(list, i)) {
            return div()
                .flex_1()
                .min_h(px(0.))
                .flex()
                .flex_col()
                .gap(px(14.))
                .p(px(18.))
                .child(head)
                .child(self.midi_fx_plugin_panel(t, place, r, tone, cx));
        }
        let picture = div()
            .flex_none()
            .h(px(PICTURE_H))
            .flex()
            .gap(px(10.))
            .when(!on, |d| d.opacity(0.45))
            .children(self.midi_fx_picture(t, i, fx, tone, cx));
        let dials: Vec<Div> = Dial::of(&fx)
            .iter()
            .map(|&d| self.midi_fx_dial(t, i, fx, d, tone, cx))
            .collect();
        let many = dials.len() > 4;
        let side = self.midi_fx_choices(t, i, fx, cx);
        let controls = div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .items_center()
            .gap(px(20.))
            .when(!dials.is_empty(), |d| {
                d.child(
                    div()
                        .flex_none()
                        .when(many, |d| d.w(px(330.)).flex().flex_wrap().gap(px(10.)))
                        .when(!many, |d| d.flex().gap(px(22.)))
                        .children(dials),
                )
            })
            .when_some(side.filter(|_| true), |d, side| {
                d.when(!Dial::of(&fx).is_empty(), |d| {
                    d.child(div().w(px(1.)).h(px(110.)).bg(rgb(LINE)))
                })
                .child(div().flex_1().min_w(px(0.)).child(side))
            });
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .p(px(18.))
            .child(head)
            .child(picture)
            .child(controls)
    }

    /// What the effect does, drawn.
    fn midi_fx_picture(
        &self,
        t: usize,
        i: usize,
        fx: MidiFx,
        color: u32,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let chord = self.midi_fx_chord();
        let names = |ps: &[u8]| {
            ps.iter()
                .map(|&p| ROOTS[p as usize % 12])
                .collect::<Vec<_>>()
                .join(" ")
        };
        let one = |caption: String, el: gpui::AnyElement| {
            vec![pictures::frame(caption, el).into_any_element()]
        };
        match fx {
            MidiFx::Arpeggiator {
                rate,
                order,
                octaves,
                latch,
                ..
            } => one(
                format!(
                    "{} held · {} · {} · {octaves} octave{}{}",
                    names(&chord),
                    rate_name(rate),
                    order.name(),
                    if octaves == 1 { "" } else { "s" },
                    if latch { " · latched" } else { "" }
                ),
                pictures::arp(fx, chord, color).into_any_element(),
            ),
            MidiFx::ChordTrigger { shape } => {
                let key = self.session.project.song_key();
                let root = chord[0];
                let plays = shape.pitches(root, key);
                one(
                    format!("{} plays {}", ROOTS[root as usize % 12], names(&plays)),
                    pictures::chord(plays, 60, 83, color).into_any_element(),
                )
            }
            MidiFx::ScaleQuantizer { root, minor } => {
                vec![
                    self.scale_keys(t, i, root, minor, color, cx)
                        .into_any_element(),
                ]
            }
            MidiFx::Transposer { semitones } => one(
                format!(
                    "{} becomes {}",
                    names(&chord),
                    names(
                        &chord
                            .iter()
                            .map(|&p| (p as i32 + semitones as i32).clamp(0, 127) as u8)
                            .collect::<Vec<_>>()
                    )
                ),
                pictures::moved(chord, semitones as i32, color).into_any_element(),
            ),
            MidiFx::Velocity { .. } => one(
                "Velocity in, along · out, up".into(),
                pictures::velocity(fx, color).into_any_element(),
            ),
            MidiFx::NoteRepeater { count, rate, .. } => one(
                format!("A note, then {count} more, {} apart", rate_name(rate)),
                pictures::repeats(fx, color).into_any_element(),
            ),
            MidiFx::Randomizer { .. } => one(
                "Sixteen notes as it might play them · hollow: left out".into(),
                pictures::random(fx, color).into_any_element(),
            ),
            MidiFx::Modifier { from, to, scale } => one(
                format!("Its moves, at {scale} %"),
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(28.))
                    .child(cc_card(from, "From", TEXT_2))
                    .child(icon(Icon::ArrowRight, 26.).text_color(rgb(color)))
                    .child(cc_card(to, "Sent as", color))
                    .into_any_element(),
            ),
            MidiFx::Plugin { .. } => Vec::new(),
            MidiFx::MpeShaper { slide, press, .. } => vec![
                pictures::frame("Slide".into(), pictures::shape(slide, color)).into_any_element(),
                pictures::frame("Pressure".into(), pictures::shape(press, color))
                    .into_any_element(),
            ],
        }
    }

    /// A scale quantizer's octave, its notes lit: a key clicked is its root.
    fn scale_keys(
        &self,
        t: usize,
        i: usize,
        root: Option<u8>,
        minor: bool,
        color: u32,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let key = match root {
            Some(r) => Some(pina_engine::music::Key { root: r, minor }),
            None => self.session.project.song_key(),
        };
        let caption = match (root, key) {
            (None, Some(k)) => format!("The song's key: {}", k.name()),
            (None, None) => "The song has no key yet: pick a root".into(),
            (Some(r), _) => format!(
                "{} {} · click a key for another root",
                ROOTS[r as usize],
                if minor { "minor" } else { "major" }
            ),
        };
        let lit = |p: u8| key.is_some_and(|k| k.contains(p));
        let is_root = |p: u8| key.is_some_and(|k| k.root % 12 == p % 12);
        let key_div = |p: u8, black: bool, cx: &mut gpui::Context<Self>| {
            let (on, first) = (lit(p), is_root(p));
            div()
                .id(("scale-key", p as u32))
                .rounded_b(px(6.))
                .cursor_pointer()
                .flex()
                .flex_col()
                .justify_end()
                .items_center()
                .pb(px(8.))
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .map(|d| match (black, on, first) {
                    (_, _, true) => d.bg(rgb(tint(color, 0.15))).text_color(rgb(0x0c0c0f)),
                    (_, true, _) => d.bg(alpha(color, 0.75)).text_color(rgb(0x0c0c0f)),
                    (false, false, _) => d.bg(rgb(0xd9d9de)).text_color(rgb(0x6a6b75)),
                    (true, false, _) => d.bg(rgb(0x1a1b21)).text_color(rgb(0x6a6b75)),
                })
                .hover(|d| d.opacity(0.85))
                .debug_selector(move || format!("midi-fx-{i}-root-{}", ROOTS[p as usize % 12]))
                .child(ROOTS[p as usize % 12])
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        let r = p % 12;
                        ws.change_midi_fx(
                            t,
                            i,
                            &set(move |f| {
                                if let MidiFx::ScaleQuantizer { root, .. } = f {
                                    *root = Some(r)
                                }
                            }),
                            cx,
                        );
                    }),
                )
        };
        let whites = [0u8, 2, 4, 5, 7, 9, 11];
        let blacks = [(1u8, 0usize), (3, 1), (6, 3), (8, 4), (10, 5)];
        let keys = div()
            .relative()
            .size_full()
            .flex()
            .gap(px(3.))
            .children(whites.map(|p| key_div(60 + p, false, cx).flex_1().h_full()))
            .children(blacks.map(|(p, after)| {
                key_div(60 + p, true, cx)
                    .absolute()
                    .top_0()
                    .h(gpui::relative(0.6))
                    .w(gpui::relative(0.09))
                    .left(gpui::relative((after as f32 + 1.0) / 7.0 - 0.045))
            }));
        pictures::frame(caption, keys)
    }

    /// One of the effect's numbers, as a dial: drag it up or down; ⌘-click puts it back.
    fn midi_fx_dial(
        &self,
        t: usize,
        i: usize,
        fx: MidiFx,
        d: Dial,
        color: u32,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let pos = d.pos(&fx);
        let shown = if d.bipolar() { pos * 2.0 - 1.0 } else { pos };
        let size = if Dial::of(&fx).len() > 4 { 46.0 } else { DIAL };
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.))
            .min_w(px(size + 18.0))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .whitespace_nowrap()
                    .child(d.name()),
            )
            .child(
                div()
                    .cursor_ns_resize()
                    .debug_selector(move || format!("midi-fx-{i}-dial-{}", kebab(d.name())))
                    .child(dial(
                        shown,
                        size,
                        (size / 9.0).clamp(4.0, 7.0),
                        color,
                        d.bipolar(),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.modifiers.platform {
                                let Some(kind) = ws.session.project.tracks[t]
                                    .midi_fx
                                    .get(i)
                                    .map(|s| s.fx.name())
                                else {
                                    return;
                                };
                                let fresh = MidiFx::fresh(kind).and_then(|f| d.get(&f));
                                if let Some(v) = fresh {
                                    ws.change_midi_fx(t, i, &set(move |f| d.set(f, v)), cx);
                                }
                                return;
                            }
                            let y = pt(e.position).y;
                            ws.begin_drag(
                                Drag::MidiFxDial {
                                    track: t,
                                    slot: i,
                                    dial: d,
                                    y,
                                    value: pos,
                                },
                                cx,
                            );
                        }),
                    ),
            )
            .child(value_line(
                d.text(&fx),
                if size >= 56.0 { 16.0 } else { 13.0 },
                None,
            ))
    }

    /// A dial dragged: its effect's number from where the drag began.
    pub fn drag_midi_fx_dial(
        &mut self,
        (track, slot): (usize, usize),
        dial: Dial,
        y: f32,
        value: f32,
        p: Point<f32>,
    ) {
        let pos = value - (p.y - y) / 150.0;
        if let Some(s) = self
            .session
            .project
            .tracks
            .get_mut(track)
            .and_then(|t| t.midi_fx.get_mut(slot))
        {
            dial.set_pos(&mut s.fx, pos);
        }
    }

    /// Its choices, in rows: a name, then a grid of buttons.
    fn midi_fx_choices(
        &self,
        t: usize,
        i: usize,
        fx: MidiFx,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Div> {
        let row = |name: &'static str,
                   cols: u16,
                   items: Vec<(String, bool, Set)>,
                   cx: &mut gpui::Context<Self>| {
            div()
                .flex()
                .items_start()
                .gap(px(12.))
                .child(
                    div()
                        .w(px(52.))
                        .flex_none()
                        .pt(px(7.))
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_3))
                        .child(name),
                )
                .child(
                    div()
                        .flex_1()
                        .grid()
                        .grid_cols(cols)
                        .gap(px(4.))
                        .p(px(3.))
                        .rounded(px(9.))
                        .bg(rgb(0x0e0f13))
                        .border_1()
                        .border_color(rgb(0x1f2027))
                        .children(items.into_iter().map(|(text, on, f)| {
                            let sel = format!("midi-fx-{i}-{}-{}", kebab(name), kebab(&text));
                            choice(&text, on)
                                .debug_selector(move || sel.clone())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        cx.stop_propagation();
                                        ws.change_midi_fx(t, i, &f, cx);
                                    }),
                                )
                        })),
                )
        };
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
        let col = div().flex().flex_col().gap(px(10.));
        Some(match fx {
            MidiFx::Arpeggiator {
                rate, order, latch, ..
            } => col
                .child(row(
                    "Rate",
                    6,
                    rates(rate, |f, r| {
                        if let MidiFx::Arpeggiator { rate, .. } = f {
                            *rate = r
                        }
                    }),
                    cx,
                ))
                .child(row(
                    "Order",
                    3,
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
                ))
                .child(row(
                    "Latch",
                    2,
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
                )),
            MidiFx::ChordTrigger { shape } => col.child(row(
                "Chord",
                6,
                ChordShape::ALL
                    .iter()
                    .map(|&c| {
                        let pick = set(move |f| *f = MidiFx::ChordTrigger { shape: c });
                        (c.name().to_string(), c == shape, pick)
                    })
                    .collect(),
                cx,
            )),
            MidiFx::ScaleQuantizer { root, minor } => col
                .child(row(
                    "Key",
                    2,
                    [("Song's", None), ("Its own", Some(root.unwrap_or(0)))]
                        .map(|(name, r)| {
                            let pick = set(move |f| {
                                if let MidiFx::ScaleQuantizer { root, .. } = f {
                                    *root = r
                                }
                            });
                            (name.to_string(), r.is_some() == root.is_some(), pick)
                        })
                        .to_vec(),
                    cx,
                ))
                .when(root.is_some(), |d| {
                    d.child(row(
                        "Scale",
                        2,
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
                    ))
                }),
            MidiFx::Transposer { semitones } => col.child(row(
                "Jump",
                4,
                [-12i8, -7, -5, 0, 5, 7, 12, 24]
                    .map(|by| {
                        let label = match by {
                            0 => "0".to_string(),
                            by if by > 0 => format!("+{by}"),
                            by => format!("−{}", -by),
                        };
                        let pick = set(move |f| *f = MidiFx::Transposer { semitones: by });
                        (label, by == semitones, pick)
                    })
                    .to_vec(),
                cx,
            )),
            MidiFx::Velocity { .. } => col.child(row(
                "Start",
                2,
                [
                    ("As played", (100u8, 0i8, 1u8, 127u8)),
                    ("Softer", (70, 0, 1, 127)),
                    ("Harder", (100, 25, 1, 127)),
                    ("Even", (25, 75, 1, 127)),
                ]
                .map(|(name, (scale, add, min, max))| {
                    let now = MidiFx::Velocity {
                        scale,
                        add,
                        min,
                        max,
                    };
                    let pick = set(move |f| *f = now);
                    (name.to_string(), fx == now, pick)
                })
                .to_vec(),
                cx,
            )),
            MidiFx::NoteRepeater { rate, .. } => col.child(row(
                "Rate",
                3,
                rates(rate, |f, r| {
                    if let MidiFx::NoteRepeater { rate, .. } = f {
                        *rate = r
                    }
                }),
                cx,
            )),
            MidiFx::Modifier { to, .. } => col.child(row(
                "Send as",
                3,
                [1u8, 11, 7, 74, 71, 0]
                    .map(|cc| {
                        let pick = set(move |f| {
                            if let MidiFx::Modifier { to, .. } = f {
                                *to = cc
                            }
                        });
                        (dials::cc_name(cc).to_string(), cc == to, pick)
                    })
                    .to_vec(),
                cx,
            )),
            MidiFx::MpeShaper { swap, .. } => col.child(row(
                "Swap",
                1,
                [("Off", false), ("Slide ⇄ press", true)]
                    .map(|(name, on)| {
                        let pick = set(move |f| {
                            if let MidiFx::MpeShaper { swap, .. } = f {
                                *swap = on
                            }
                        });
                        (name.to_string(), on == swap, pick)
                    })
                    .to_vec(),
                cx,
            )),
            MidiFx::Randomizer { .. } | MidiFx::Plugin { .. } => return None,
        })
    }

    /// The song key's chord held through the track's effects for two bars, to hear them.
    pub fn hear_midi_fx(&mut self, t: usize, cx: &mut gpui::Context<Self>) {
        let Some(id) = self.session.project.tracks.get(t).map(|t| t.id) else {
            return;
        };
        let chord = self.midi_fx_chord();
        if self.midi_fx_view.as_ref().is_some_and(|v| v.hearing) {
            for &p in &chord {
                self.player.preview(id, p, 0);
            }
            if let Some(v) = self.midi_fx_view.as_mut() {
                v.hearing = false;
            }
            return cx.notify();
        }
        for &p in &chord {
            self.player.preview(id, p, 100);
        }
        if let Some(v) = self.midi_fx_view.as_mut() {
            v.hearing = true;
        }
        let p = &self.session.project;
        let secs = p.seconds_at((p.bar() * 2) as f64) - p.seconds_at(0.0);
        let ms = (secs * 1000.0).clamp(800.0, 8000.0) as u64;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(ms))
                .await;
            this.update(cx, |ws, cx| {
                if ws.midi_fx_view.as_ref().is_some_and(|v| v.hearing) {
                    for &p in &chord {
                        ws.player.preview(id, p, 0);
                    }
                    if let Some(v) = ws.midi_fx_view.as_mut() {
                        v.hearing = false;
                    }
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
}

/// A controller as a card: its number big, its name under it.
fn cc_card(cc: u8, label: &'static str, color: u32) -> Div {
    div()
        .w(px(150.))
        .p(px(14.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(4.))
        .rounded(px(10.))
        .bg(rgb(0x14151a))
        .border_1()
        .border_color(rgb(0x24252c))
        .child(
            div()
                .text_size(px(11.))
                .text_color(rgb(TEXT_3))
                .child(label),
        )
        .child(
            div()
                .text_size(px(22.))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(color))
                .child(match cc {
                    0 => "Off".to_string(),
                    cc => format!("CC {cc}"),
                }),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(rgb(TEXT_2))
                .child(match dials::cc_name(cc) {
                    "" => "Controller".to_string(),
                    name => name.to_string(),
                }),
        )
}
