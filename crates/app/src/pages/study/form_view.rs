//! The song's form: its sections, and each one up close.

use super::*;

/// The form in a sentence: how it's built.
pub(crate) fn form_note(s: &Study) -> String {
    let chorus = s
        .sections
        .iter()
        .filter(|x| x.name.starts_with("Chorus"))
        .count();
    let verse = s
        .sections
        .iter()
        .filter(|x| x.name.starts_with("Verse"))
        .count();
    let first = s
        .sections
        .iter()
        .find(|x| x.name.starts_with("Chorus"))
        .map(|x| x.start);
    let mut out = match (verse, chorus) {
        (0, 0) => "No section comes back the same: built as a groove that moves".to_string(),
        (v, c) => format!(
            "{v} verse{}, {c} chorus{}",
            if v == 1 { "" } else { "s" },
            if c == 1 { "" } else { "es" }
        ),
    };
    if let Some(t) = first {
        out.push_str(&format!(" · first chorus at {}", clock(t)));
    }
    if s.sections.iter().any(|x| x.name == "Bridge") {
        out.push_str(" · a bridge");
    }
    out
}

/// The section picked, opened up: where it is, how loud, its chords as numerals, what plays in
/// it, and its first lines.
pub(crate) fn section_detail(s: &Study, i: usize, x: &study::SectionInfo) -> Div {
    let c = letter_color(&x.label);
    let heard: Vec<String> = s
        .instruments
        .iter()
        .filter(|e| e.sections.contains(&i))
        .map(|e| e.name.clone())
        .collect();
    let lines: Vec<String> = s
        .lyrics
        .as_ref()
        .map(|l| {
            l.lines
                .iter()
                .filter(|l| l.section == Some(i))
                .take(4)
                .map(|l| l.text.clone())
                .collect()
        })
        .unwrap_or_default();
    let fact = |name: &'static str, value: String| {
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .text_size(px(10.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT_3))
                    .child(name.to_uppercase()),
            )
            .child(div().font_family(MONO).text_size(px(15.)).child(value))
    };
    div()
        .mt(px(4.))
        .p(px(18.))
        .rounded(px(14.))
        .bg(rgb(CARD))
        .border_1()
        .border_color(rgb(LINE))
        .border_l_4()
        .border_color(rgb(c))
        .flex()
        .gap(px(28.))
        .debug_selector(|| "study-section-detail".into())
        .child(
            div()
                .w(px(180.))
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(
                    div()
                        .text_size(px(20.))
                        .font_weight(FontWeight::BOLD)
                        .child(x.name.clone()),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(18.))
                        .child(fact("Bars", format!("{}–{}", x.bar + 1, x.bar + x.bars)))
                        .child(fact("At", clock(x.start))),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(18.))
                        .child(fact("LUFS", format!("{:.1}", x.lufs)))
                        .child(fact("Energy", format!("{:.0}%", x.energy * 100.0))),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(label("Goes round"))
                        .child(
                            div()
                                .font_family(MONO)
                                .text_size(px(17.))
                                .child(x.chords.clone()),
                        )
                        .child(
                            div()
                                .font_family(MONO)
                                .text_size(px(12.5))
                                .text_color(rgb(tint(c, 0.3)))
                                .child(x.numerals.clone()),
                        ),
                )
                .when(!heard.is_empty(), |d| {
                    d.child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.))
                            .children(heard.into_iter().map(|h| {
                                div()
                                    .px(px(8.))
                                    .py(px(3.))
                                    .rounded(px(7.))
                                    .bg(rgb(RAISED))
                                    .text_size(px(11.5))
                                    .text_color(rgb(TEXT_2))
                                    .child(h)
                            })),
                    )
                })
                .when(!lines.is_empty(), |d| {
                    d.child(
                        div()
                            .pl(px(12.))
                            .border_l_2()
                            .border_color(alpha(c, 0.5))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .children(lines.into_iter().map(|l| {
                                div().text_size(px(12.5)).text_color(rgb(TEXT_2)).child(l)
                            })),
                    )
                }),
        )
}

impl Workspace {
    /// The form: a block a section, as wide as it's long, the same colour for the same music;
    /// under it, each one's chords; the one picked, opened up.
    pub(crate) fn study_form(&mut self, s: &Study, cx: &mut Context<Self>) -> Div {
        let picked = self
            .studies
            .section
            .or_else(|| s.sections.iter().position(|x| x.name.starts_with("Chorus")))
            .unwrap_or(0)
            .min(s.sections.len().saturating_sub(1));
        let blocks = s.sections.iter().enumerate().map(|(i, x)| {
            let c = letter_color(&x.label);
            let on = i == picked;
            div()
                .id(("section", i))
                .debug_selector(move || format!("study-section-{i}"))
                .flex_grow(x.bars.max(1) as f32)
                .flex_basis(px(0.))
                .min_w(px(0.))
                .h(px(64.))
                .px(px(8.))
                .py(px(7.))
                .flex()
                .flex_col()
                .justify_between()
                .rounded(px(8.))
                .cursor_pointer()
                .bg(alpha(c, if on { 0.3 } else { 0.08 + 0.14 * x.energy }))
                .border_t_2()
                .border_color(rgb(c))
                .when(on, |d| d.border_1().border_color(rgb(c)))
                .hover(|d| d.bg(alpha(c, 0.26)))
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(x.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .child(format!("{} bars", x.bars)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.studies.section = Some(i);
                        cx.notify();
                    }),
                )
        });
        let chords = s.sections.iter().map(|x| {
            div()
                .flex_grow(x.bars.max(1) as f32)
                .flex_basis(px(0.))
                .min_w(px(0.))
                .px(px(2.))
                .font_family(MONO)
                .text_size(px(10.5))
                .text_color(rgb(TEXT_3))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(x.chords.clone())
        });
        let detail = s.sections.get(picked).map(|x| section_detail(s, picked, x));
        // The form as letters: "A B C B D C".
        let letters: Vec<&str> = s.sections.iter().map(|x| x.label.as_str()).collect();
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(heading(
                "Structure",
                "Where each section starts and how hard it hits; one colour, one music",
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .children(letters.iter().map(|l| {
                        div()
                            .size(px(20.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.))
                            .bg(alpha(letter_color(l), 0.18))
                            .text_size(px(11.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(letter_color(l)))
                            .child(l.to_string())
                    }))
                    .child(
                        div()
                            .pl(px(6.))
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_3))
                            .child(form_note(s)),
                    ),
            )
            .child(div().flex().gap(px(3.)).children(blocks))
            .child(div().flex().gap(px(3.)).children(chords))
            .children(detail)
    }
}
