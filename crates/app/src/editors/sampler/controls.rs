//! The sampler's control bar and knobs.

use super::*;

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
                .child(toggle("Mono", v.mono, "sampler-mono").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.edit_sampler(t, cx, |s| s.voice.mono ^= true)
                    }),
                ))
                .child(toggle("Legato", v.legato, "sampler-legato").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.edit_sampler(t, cx, |s| {
                            s.voice.legato ^= true;
                            s.voice.mono |= s.voice.legato;
                        })
                    }),
                ))
                .child(toggle("Slice…", false, "sampler-slice").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                        let items = vec![
                            (
                                "At its hits (up to 8)".to_string(),
                                MenuAction::Slice(Slicing::Hits(8)),
                            ),
                            (
                                "At its hits (up to 16)".to_string(),
                                MenuAction::Slice(Slicing::Hits(16)),
                            ),
                            (
                                "At its hits (up to 32)".to_string(),
                                MenuAction::Slice(Slicing::Hits(32)),
                            ),
                            (
                                "Into 4 equal parts".to_string(),
                                MenuAction::Slice(Slicing::Equal(4)),
                            ),
                            (
                                "Into 8 equal parts".to_string(),
                                MenuAction::Slice(Slicing::Equal(8)),
                            ),
                            (
                                "Into 16 equal parts".to_string(),
                                MenuAction::Slice(Slicing::Equal(16)),
                            ),
                        ];
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
                ),
        )
    }

    /// A knob for a sampler setting: turned by dragging up and down.
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
        let bipolar = matches!(
            f,
            Field::FilterEnv | Field::Transpose | Field::Tune(_) | Field::Pan(_)
        );
        let shown = if bipolar { value * 2.0 - 1.0 } else { value };
        let text = field_text(&s, f, self.session.project.song_key());
        let target = Target::Sampler { track: t, field: f };
        div()
            .w(px(50.))
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2.))
            .cursor_ns_resize()
            .debug_selector(move || format!("sampler-{f:?}"))
            .child(self.modded(self.mod_key(target), 28., knob(shown, 28., color, bipolar)))
            .child(
                div()
                    .text_size(px(9.5))
                    .text_color(rgb(TEXT_3))
                    .whitespace_nowrap()
                    .child(label),
            )
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
                    let value = ws.target(target);
                    let y = pt(e.position).y;
                    ws.begin_drag(Drag::Knob { target, y, value }, cx);
                }),
            )
    }
}
