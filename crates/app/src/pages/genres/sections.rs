//! A genre's sections and parts.

use super::*;

impl Workspace {
    /// The picked section's name, length and energy, where it goes, and taking it out.
    pub(crate) fn section_controls(
        &mut self,
        i: usize,
        s: &Section,
        count: usize,
        color: u32,
        cx: &mut Context<Self>,
    ) -> Div {
        let name = self.genres.fields.as_ref().and_then(|f| f.section.clone());
        let small = |label: Icon,
                     sel: &'static str,
                     cx: &mut Context<Self>,
                     f: Box<dyn Fn(&mut Playbook)>| {
            let f = std::rc::Rc::new(f);
            div()
                .size(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.))
                .border_1()
                .border_color(rgb(LINE))
                .text_color(rgb(TEXT_2))
                .cursor_pointer()
                .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                .child(icon(label, 12.).text_color(rgb(TEXT_2)))
                .debug_selector(move || sel.into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        let f = f.clone();
                        ws.edit_genre(true, cx, move |p| f(p))
                    }),
                )
        };
        let energy: Vec<_> = (1..=5u8)
            .map(|e| {
                let on = e <= s.energy;
                div()
                    .w(px(14.))
                    .h(px(6.0 + e as f32 * 3.0))
                    .rounded(px(3.))
                    .cursor_pointer()
                    .bg(if on {
                        rgb(color).into()
                    } else {
                        alpha(color, 0.15)
                    })
                    .debug_selector(move || format!("energy-{e}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.edit_genre(false, cx, move |p| {
                                if let Some(s) = p.sections.get_mut(i) {
                                    s.energy = e
                                }
                            })
                        }),
                    )
            })
            .collect();
        div()
            .mt(px(8.))
            .pt(px(12.))
            .border_t_1()
            .border_color(rgb(LINE_SOFT))
            .flex()
            .items_center()
            .gap(px(14.))
            .child(div().w(px(180.)).child(field_box(name, "section-name")))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(small(
                        Icon::Minus,
                        "bars-down",
                        cx,
                        Box::new(move |p| {
                            if let Some(s) = p.sections.get_mut(i) {
                                s.bars = s.bars.saturating_sub(4).max(4)
                            }
                        }),
                    ))
                    .child(
                        div()
                            .min_w(px(58.))
                            .flex()
                            .justify_center()
                            .font_family(MONO)
                            .text_size(px(12.5))
                            .child(format!("{} bars", s.bars)),
                    )
                    .child(small(
                        Icon::Plus,
                        "bars-up",
                        cx,
                        Box::new(move |p| {
                            if let Some(s) = p.sections.get_mut(i) {
                                s.bars = (s.bars + 4).min(128)
                            }
                        }),
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(4.))
                    .child(
                        div()
                            .mr(px(4.))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child("Energy"),
                    )
                    .children(energy),
            )
            .child(div().flex_1())
            .when(i > 0, |d| {
                d.child(small(
                    Icon::Back,
                    "section-left",
                    cx,
                    Box::new(move |p| {
                        p.sections.swap(i, i - 1);
                    }),
                ))
            })
            .when(i + 1 < count, |d| {
                d.child(small(
                    Icon::Forward,
                    "section-right",
                    cx,
                    Box::new(move |p| {
                        p.sections.swap(i, i + 1);
                    }),
                ))
            })
            .child(small(
                Icon::Close,
                "section-remove",
                cx,
                Box::new(move |p| {
                    if i < p.sections.len() {
                        p.sections.remove(i);
                    }
                }),
            ))
    }

    /// Each part: what it sounds like and how it's written.
    pub(crate) fn genre_parts(&mut self, p: &Playbook, cx: &mut Context<Self>) -> Div {
        let fields = self.genres.fields.as_ref();
        let cards: Vec<_> = p
            .parts
            .iter()
            .enumerate()
            .map(|(i, part)| {
                let rc = role_color(&part.role);
                let sound = fields.and_then(|f| f.sounds.get(i).cloned());
                let write = fields.and_then(|f| f.writes.get(i).cloned());
                div()
                    .w(px(330.))
                    .flex_grow(1.0)
                    .p(px(14.))
                    .rounded(px(14.))
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(LINE))
                    .flex()
                    .flex_col()
                    .gap(px(9.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(div().w(px(4.)).h(px(18.)).rounded(px(2.)).bg(rgb(rc)))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(capital(&part.role)),
                            )
                            .child(
                                div()
                                    .id(("part-remove", i))
                                    .h(px(24.))
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .gap(px(5.))
                                    .rounded(px(7.))
                                    .border_1()
                                    .border_color(rgb(LINE))
                                    .text_size(px(11.5))
                                    .text_color(rgb(TEXT_2))
                                    .cursor_pointer()
                                    .hover(|d| {
                                        d.bg(alpha(RECORD, 0.14)).border_color(alpha(RECORD, 0.5))
                                    })
                                    .child(icon(Icon::Close, 10.).text_color(rgb(TEXT_2)))
                                    .child("Remove")
                                    .debug_selector(move || format!("part-remove-{i}"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            ws.edit_genre(true, cx, move |p| {
                                                if i < p.parts.len() {
                                                    let role = p.parts.remove(i).role;
                                                    for s in &mut p.sections {
                                                        s.parts.retain(|x| *x != role);
                                                    }
                                                }
                                            })
                                        }),
                                    ),
                            ),
                    )
                    .child(label("Sounds"))
                    .child(
                        field_box(sound, "part-sound")
                            .debug_selector(move || format!("part-sound-{i}")),
                    )
                    .child(label("How to write it"))
                    .child(
                        field_box(write, "part-write")
                            .debug_selector(move || format!("part-write-{i}")),
                    )
            })
            .collect();
        let missing: Vec<&str> = ROLES
            .iter()
            .copied()
            .filter(|r| !p.parts.iter().any(|x| x.role == *r))
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(heading(
                "Parts",
                "Who plays, the sounds to look for, how to write them",
            ))
            .child(div().flex().flex_wrap().gap(px(12.)).children(cards))
            .when(!missing.is_empty(), |d| {
                d.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(rgb(TEXT_3))
                                .child("Add"),
                        )
                        .children(missing.into_iter().map(|role| {
                            let rc = role_color(role);
                            div()
                                .h(px(24.))
                                .px(px(9.))
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .rounded(px(12.))
                                .border_1()
                                .border_color(rgb(LINE))
                                .text_size(px(11.5))
                                .text_color(rgb(TEXT_2))
                                .cursor_pointer()
                                .hover(|d| d.bg(alpha(rc, 0.14)).border_color(alpha(rc, 0.5)))
                                .child(div().size(px(6.)).rounded(px(3.)).bg(rgb(rc)))
                                .child(role)
                                .debug_selector(move || format!("add-part-{role}"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        ws.edit_genre(true, cx, |p| {
                                            p.parts.push(Part {
                                                role: role.into(),
                                                sound: String::new(),
                                                write: String::new(),
                                            })
                                        })
                                    }),
                                )
                        })),
                )
            })
    }
}
