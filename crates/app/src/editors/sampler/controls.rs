//! The sampler's control bar, its knobs and its little switches.

use super::*;
use crate::editors::pianoroll::{More, more_button};

/// A switch in a tray: lit when on.
pub(crate) fn pill(label: &'static str, on: bool, color: u32) -> Div {
    div()
        .h(px(22.))
        .px(px(9.))
        .flex()
        .items_center()
        .rounded(px(6.))
        .cursor_pointer()
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .when(on, |d| d.bg(alpha(color, 0.18)).text_color(rgb(color)))
        .when(!on, |d| {
            d.text_color(rgb(TEXT_3))
                .hover(|d| d.bg(rgb(OFF)).text_color(rgb(TEXT_2)))
        })
        .child(label)
}

impl Workspace {
    /// The sampler pane's own controls in the editor's bar.
    pub fn sampler_controls(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let t = self.sampler_track()?;
        let s = self.sampler_of(t)?.clone();
        let v = s.voice;
        let toggle = |label: &'static str, on: bool, sel: &'static str| {
            div()
                .h(px(26.))
                .px(px(10.))
                .flex()
                .items_center()
                .rounded(px(7.))
                .cursor_pointer()
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .bg(if on {
                    alpha(ACCENT, 0.16)
                } else {
                    rgb(0x23232a).into()
                })
                .text_color(rgb(if on { ACCENT_TEXT } else { TEXT_2 }))
                .hover(|d| d.bg(rgb(RAISED)))
                .debug_selector(move || sel.into())
                .child(label)
        };
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(toggle("Slice…", false, "sampler-slice").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                        let items = [
                            ("At its hits (up to 8)", Slicing::Hits(8)),
                            ("At its hits (up to 16)", Slicing::Hits(16)),
                            ("At its hits (up to 32)", Slicing::Hits(32)),
                            ("Into 4 equal parts", Slicing::Equal(4)),
                            ("Into 8 equal parts", Slicing::Equal(8)),
                            ("Into 16 equal parts", Slicing::Equal(16)),
                        ]
                        .map(|(label, how)| (label.to_string(), MenuAction::Slice(how)))
                        .to_vec();
                        ws.menu = Some(Menu {
                            at: e.position,
                            items,
                            ..Default::default()
                        });
                        cx.notify();
                    }),
                ))
                .child(
                    button("+ Sample", false)
                        .debug_selector(|| "sampler-add".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.pick_samples(cx)),
                        ),
                )
                .child(more_button(
                    v.mono || v.legato,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        let tick = |on: bool, label: &str| match on {
                            true => format!("✓ {label}"),
                            false => label.to_string(),
                        };
                        let items = vec![
                            (tick(v.mono, "Mono"), MenuAction::More(More::Mono(t))),
                            (tick(v.legato, "Legato"), MenuAction::More(More::Legato(t))),
                        ];
                        ws.menu = Some(Menu {
                            at: e.position,
                            items,
                            ..Default::default()
                        });
                        cx.notify();
                    }),
                )),
        )
    }

    /// A knob for a sampler setting: its name over it, its value under it. Drag it up and
    /// down; ⌘-click puts it back to how it comes.
    pub(crate) fn sampler_knob(
        &self,
        t: usize,
        f: Field,
        label: &'static str,
        color: u32,
        cx: &mut Context<Self>,
    ) -> Div {
        let s = self.sampler_of(t).cloned().unwrap_or_default();
        let value = field_pos(&s, f);
        let shown = if bipolar(f) { value * 2.0 - 1.0 } else { value };
        let text = field_text(&s, f, self.session.project.song_key());
        let target = Target::Sampler { track: t, field: f };
        // As it comes: a new sampler's, its zones as they are but this setting.
        let reset = {
            let mut fresh = Sampler {
                zones: s.zones.iter().map(|_| Zone::default()).collect(),
                ..Sampler::default()
            };
            if let Field::Route(i) = f {
                fresh.voice.matrix = s.voice.matrix;
                fresh.voice.matrix.slots_mut()[i.min(7)].amount = 0.0;
            }
            field_pos(&fresh, f)
        };
        div()
            .w(px(48.))
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(3.))
            .cursor_ns_resize()
            .debug_selector(move || format!("sampler-{f:?}"))
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(TEXT_3))
                    .whitespace_nowrap()
                    .child(label),
            )
            .child(self.modded(
                self.mod_key(target),
                30.,
                dial(shown, 30., 4.0, color, bipolar(f)),
            ))
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(10.))
                    .text_color(rgb(TEXT))
                    .whitespace_nowrap()
                    .child(text),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if e.modifiers.platform {
                        ws.edit(cx, |ws| ws.set_target(target, reset));
                        return;
                    }
                    let value = ws.target(target);
                    let y = pt(e.position).y;
                    ws.begin_drag(Drag::Knob { target, y, value }, cx);
                }),
            )
    }

    /// A setting as a little value to drag up and down: its name and value side by side.
    pub(crate) fn sampler_value(
        &self,
        t: usize,
        f: Field,
        label: &'static str,
        color: u32,
        cx: &mut Context<Self>,
    ) -> Div {
        let s = self.sampler_of(t).cloned().unwrap_or_default();
        let text = field_text(&s, f, None);
        let target = Target::Sampler { track: t, field: f };
        div()
            .h(px(26.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(7.))
            .bg(rgb(FIELD))
            .cursor_ns_resize()
            .hover(|d| d.bg(rgb(RAISED)))
            .debug_selector(move || format!("sampler-{f:?}"))
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .child(label),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(rgb(color))
                    .whitespace_nowrap()
                    .child(text),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let value = ws.target(target);
                    let y = pt(e.position).y;
                    ws.begin_drag(Drag::Knob { target, y, value }, cx);
                }),
            )
    }

    /// A row of choices in a tray, the one the sampler has lit; picking one is an edit.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sampler_choices<T: Copy + PartialEq + 'static>(
        &self,
        t: usize,
        sel: &'static str,
        options: &[(&'static str, T)],
        now: T,
        color: u32,
        cx: &mut Context<Self>,
        set: impl Fn(&mut Sampler, T) + Copy + 'static,
    ) -> Div {
        tray()
            .p(px(2.))
            .gap(px(1.))
            .rounded(px(8.))
            .children(options.iter().map(|&(label, value)| {
                pill(label, value == now, color)
                    .debug_selector(move || {
                        format!("{sel}-{}", label.to_lowercase().replace(' ', "-"))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit_sampler(t, cx, |s| set(s, value));
                        }),
                    )
            }))
    }
}
