//! The song's words: lines, rhymes, syllables and hooks.

use super::*;

impl Workspace {
    /// The words: lines by section, each with its rhyme's letter and syllables; the hooks, the
    /// voice, the words it keeps coming back to; and the agent, for what they mean.
    pub(crate) fn study_words(&mut self, s: &Study, color: u32, cx: &mut Context<Self>) -> Div {
        let ask = div()
            .h(px(32.))
            .px(px(13.))
            .flex()
            .items_center()
            .gap(px(7.))
            .rounded(px(9.))
            .bg(alpha(AGENT, 0.12))
            .border_1()
            .border_color(alpha(AGENT, 0.35))
            .text_size(px(12.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(AGENT_TEXT))
            .cursor_pointer()
            .hover(|d| d.bg(alpha(AGENT, 0.2)))
            .debug_selector(|| "study-ask".into())
            .child(icon(Icon::Agent, 13.).text_color(rgb(AGENT)))
            .child("Ask the agent what it means")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| ws.ask_about_study(cx)),
            );
        let meaning = tile(
            "Theme & meaning",
            "Its core message, its metaphors and imagery, and why it works: read by the agent from the lyrics and this study",
        )
        .child(div().flex().child(ask));
        let Some(l) = s.lyrics.as_ref() else {
            let who = match s.identity.title.is_empty() {
                true => "this file".to_string(),
                false => format!("“{}”", s.identity.title),
            };
            return div()
                .flex()
                .flex_col()
                .gap(px(14.))
                .child(heading("Lyrics & poetic devices", "Rhyme schemes, scansion and hooks"))
                .child(
                    div()
                        .flex()
                        .gap(px(16.))
                        .child(
                            tile("No lyrics found", "Looked up on LRCLIB by the title and artist in the file's tags")
                                .flex_1()
                                .child(div().text_size(px(12.5)).text_color(rgb(TEXT_2)).child(format!(
                                    "None were found for {who}. If its file name or tags spell the title and artist, \
                                     studying it again looks once more; an instrumental has none."
                                ))),
                        )
                        .child(meaning.flex_1()),
                );
        };
        let hooks = l.hooks.clone();
        // Lines grouped by section, in order.
        let mut groups: Vec<(Option<usize>, Vec<&study::words::Line>)> = Vec::new();
        for line in &l.lines {
            match groups.last_mut() {
                Some((sec, lines)) if *sec == line.section => lines.push(line),
                _ => groups.push((line.section, vec![line])),
            }
        }
        let scheme_of = |i: usize| {
            l.schemes
                .iter()
                .find(|(k, _)| *k == i)
                .map(|(_, v)| v.clone())
        };
        let lines = groups.into_iter().map(|(sec, lines)| {
            let section = sec.and_then(|i| s.sections.get(i));
            let c = section.map_or(SLATE, |x| letter_color(&x.label));
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .pb(px(2.))
                        .child(div().w(px(3.)).h(px(12.)).rounded(px(2.)).bg(rgb(c)))
                        .child(
                            div()
                                .text_size(px(11.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(tint(c, 0.3)))
                                .child(
                                    section.map_or("Lyrics".to_string(), |x| x.name.to_uppercase()),
                                ),
                        )
                        .when_some(sec.and_then(scheme_of), |d, sch| {
                            d.child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_4))
                                    .child(sch),
                            )
                        }),
                )
                .children(lines.into_iter().map(|line| {
                    let hook = hooks.contains(&line.text);
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            div()
                                .size(px(18.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(5.))
                                .bg(alpha(rhyme_color(line.rhyme), 0.16))
                                .font_family(MONO)
                                .text_size(px(10.5))
                                .text_color(rgb(rhyme_color(line.rhyme)))
                                .child(line.rhyme.to_string()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_size(px(13.))
                                .text_color(rgb(if hook { tint(color, 0.35) } else { TEXT_2 }))
                                .when(hook, |d| d.font_weight(FontWeight::SEMIBOLD))
                                .child(line.text.clone()),
                        )
                        .child(
                            div()
                                .w(px(26.))
                                .flex_none()
                                .flex()
                                .justify_end()
                                .font_family(MONO)
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_4))
                                .child(line.syllables.to_string()),
                        )
                }))
        });
        let top = l.words.first().map_or(1, |w| w.1).max(1);
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(heading(
                "Lyrics & poetic devices",
                "Each line's rhyme (same letter, same sound) and syllables; its hooks lit",
            ))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(16.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .p(px(18.))
                            .rounded(px(14.))
                            .bg(rgb(CARD))
                            .border_1()
                            .border_color(rgb(LINE))
                            .flex()
                            .flex_col()
                            .gap(px(18.))
                            .children(lines)
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_4))
                                    .child(format!("Lyrics: {}", l.source)),
                            ),
                    )
                    .child(
                        div()
                            .w(px(320.))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap(px(16.))
                            .child(meaning)
                            .child(
                                tile("Voice", "Who sings, and to whom")
                                    .child(div().text_size(px(14.)).child(capital(&l.voice))),
                            )
                            .when(!l.hooks.is_empty(), |d| {
                                d.child(tile("Hooks", "Lines sung three times or more").children(
                                    l.hooks.iter().take(4).map(|h| {
                                        div()
                                            .pl(px(10.))
                                            .border_l_2()
                                            .border_color(rgb(color))
                                            .text_size(px(13.))
                                            .text_color(rgb(TEXT))
                                            .child(format!("“{h}”"))
                                    }),
                                ))
                            })
                            .when(!l.words.is_empty(), |d| {
                                d.child(tile("Its words", "What it keeps coming back to").child(
                                    div().flex().flex_wrap().gap(px(6.)).children(
                                        l.words.iter().take(14).map(|(w, n)| {
                                            let k = *n as f32 / top as f32;
                                            div()
                                                .px(px(8.))
                                                .py(px(3.))
                                                .rounded(px(7.))
                                                .bg(alpha(color, 0.06 + 0.18 * k))
                                                .text_size(px(11.0 + 5.0 * k))
                                                .text_color(rgb(if k > 0.5 {
                                                    TEXT
                                                } else {
                                                    TEXT_2
                                                }))
                                                .child(format!("{w} {n}"))
                                        }),
                                    ),
                                ))
                            }),
                    ),
            )
    }
}
