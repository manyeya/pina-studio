//! A genre's mix targets, and its notes.

use super::*;

impl Workspace {
    /// Where its mixes sit: loudness, and the balance of six bands, dragged.
    pub(crate) fn genre_mix(&mut self, p: &Playbook, color: u32, cx: &mut Context<Self>) -> Div {
        let lufs = p.lufs;
        let bands: Vec<_> = (0..6)
            .map(|i| {
                let v = p.spectrum[i];
                let level = ((v - FLOOR) / -FLOOR).clamp(0.0, 1.0);
                let set = move |ws: &mut Workspace,
                                y: f32,
                                b: Bounds<Pixels>,
                                save: bool,
                                cx: &mut Context<Workspace>| {
                    let top = f32::from(b.origin.y);
                    let h = f32::from(b.size.height).max(1.0);
                    let at = (1.0 - ((y - top) / h).clamp(0.0, 1.0)) * -FLOOR + FLOOR;
                    let v = (at * 2.0).round() / 2.0;
                    match save {
                        true => ws.edit_genre(false, cx, move |p| p.spectrum[i] = v),
                        false => {
                            if let Some(p) = ws.genres.draft.as_mut() {
                                p.spectrum[i] = v;
                            }
                            cx.notify();
                        }
                    }
                };
                let bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::<Pixels>::default()));
                let (b1, b2, b3) = (bounds.clone(), bounds.clone(), bounds.clone());
                div()
                    .id(("band", i))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(12.))
                            .text_color(rgb(TEXT))
                            .flex()
                            .justify_center()
                            .child(format!("{v:.1}")),
                    )
                    .child(
                        div()
                            .id(("band-col", i))
                            .h(px(150.))
                            .relative()
                            .rounded(px(9.))
                            .bg(rgb(WELL))
                            .cursor_ns_resize()
                            .debug_selector(move || format!("band-{i}"))
                            .child(
                                canvas(move |b, _, _| b1.set(b), |_, _, _, _| {})
                                    .absolute()
                                    .size_full(),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .bottom_0()
                                    .left_0()
                                    .right_0()
                                    .h(gpui::relative(level))
                                    .rounded(px(9.))
                                    .bg(alpha(color, 0.16 + 0.3 * level))
                                    .border_t_2()
                                    .border_color(rgb(color)),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    set(ws, f32::from(e.position.y), b2.get(), true, cx)
                                }),
                            )
                            .on_mouse_move(cx.listener(move |ws, e: &MouseMoveEvent, _, cx| {
                                if e.pressed_button == Some(MouseButton::Left) {
                                    set(ws, f32::from(e.position.y), b3.get(), false, cx)
                                }
                            }))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    if let Some(p) = ws.genres.draft.clone() {
                                        ws.save_genre(&p, cx);
                                    }
                                }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(rgb(TEXT_2))
                                    .child(BANDS[i].0),
                            )
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(9.5))
                                    .text_color(rgb(TEXT_4))
                                    .child(BANDS[i].1),
                            ),
                    )
            })
            .collect();
        let step = |sign: f32, cx: &mut Context<Self>| {
            div()
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .border_1()
                .border_color(rgb(LINE))
                .text_color(rgb(TEXT_2))
                .cursor_pointer()
                .hover(|d| d.bg(rgb(RAISED)))
                .child(
                    icon(if sign < 0.0 { Icon::Minus } else { Icon::Plus }, 13.)
                        .text_color(rgb(TEXT_2)),
                )
                .debug_selector(move || format!("lufs-{}", if sign < 0.0 { "down" } else { "up" }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.edit_genre(false, cx, move |p| {
                            p.lufs = (p.lufs + sign * 0.5).clamp(-24.0, -4.0)
                        })
                    }),
                )
        };
        // Loudness, on a scale from quiet (-20) to loud (-6).
        let fill = ((lufs + 20.0) / 14.0).clamp(0.0, 1.0);
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(heading(
                "Mix target",
                "What a finished master sounds like: the critic and master aim here",
            ))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(
                        tile(
                            "Spectrum",
                            "Each band's share of the energy, dB of the total: drag a band",
                        )
                        .flex_1()
                        .child(div().flex().gap(px(10.)).children(bands)),
                    )
                    .child(
                        tile("Loudness", "Integrated LUFS of the master")
                            .w(px(220.))
                            .flex_none()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .child(step(-1.0, cx))
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex()
                                            .justify_center()
                                            .font_family(MONO)
                                            .text_size(px(30.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(format!("{lufs:.1}")),
                                    )
                                    .child(step(1.0, cx)),
                            )
                            .child(
                                div().h(px(8.)).rounded(px(4.)).bg(rgb(WELL)).child(
                                    div()
                                        .h_full()
                                        .w(gpui::relative(fill))
                                        .rounded(px(4.))
                                        .bg(rgb(color)),
                                ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .font_family(MONO)
                                    .text_size(px(10.))
                                    .text_color(rgb(TEXT_4))
                                    .child("quiet −20")
                                    .child("−6 loud"),
                            ),
                    ),
            )
    }

    /// What the agent should know about it, a line each.
    pub(crate) fn genre_notes(&mut self, p: &Playbook, cx: &mut Context<Self>) -> Div {
        let fields = self.genres.fields.as_ref();
        let notes: Vec<_> = (0..p.notes.len())
            .map(|i| {
                let f = fields.and_then(|f| f.notes.get(i).cloned());
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .w(px(22.))
                            .flex_none()
                            .font_family(MONO)
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_4))
                            .child(format!("{:02}", i + 1)),
                    )
                    .child(div().flex_1().min_w(px(0.)).child(field_box(f, "note")))
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                heading(
                    "Notes for the agent",
                    "What makes it sound like itself, and what sounds amateur",
                )
                .child(
                    div()
                        .id("add-note")
                        .h(px(26.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .rounded(px(7.))
                        .border_1()
                        .border_color(rgb(LINE))
                        .text_size(px(12.))
                        .text_color(rgb(TEXT_2))
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(OFF)))
                        .child(icon(Icon::Plus, 12.).text_color(rgb(TEXT_2)))
                        .child("Note")
                        .debug_selector(|| "add-note".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                ws.edit_genre(true, cx, |p| p.notes.push(String::new()))
                            }),
                        ),
                ),
            )
            .child(
                div()
                    .p(px(14.))
                    .rounded(px(14.))
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(LINE))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .children(notes)
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_4))
                            .child("Clear a note to take it out."),
                    ),
            )
    }
}
