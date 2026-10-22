//! The conversation drawn: each answer's words and steps.

use super::*;

/// An agent's mark, as modisa draws it: its logo in its brand colour (white for a monochrome
/// brand), breathing while it works.
pub fn badge(kind: Kind, size: f32, busy: bool) -> impl IntoElement {
    let def = kind.def();
    let color = def.color.unwrap_or(TEXT);
    let dot = div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(size * 0.3))
        .bg(alpha(color, 0.14))
        .border_1()
        .border_color(alpha(color, 0.28))
        .child(logo(kind, size * 0.6));
    match busy {
        true => dot
            .with_animation(
                "agent-breathe",
                Animation::new(Duration::from_millis(1600))
                    .repeat()
                    .with_easing(gpui::pulsating_between(0.45, 1.0)),
                |d, t| d.opacity(t),
            )
            .into_any_element(),
        false => dot.into_any_element(),
    }
}

/// An agent's logo, from modisa's logo font, in its brand colour.
pub fn logo(kind: Kind, size: f32) -> Div {
    let def = kind.def();
    div()
        .font_family(agent::MARKS)
        .text_size(px(size))
        .line_height(px(size))
        .text_color(rgb(def.color.unwrap_or(TEXT)))
        .child(def.logo.to_string())
}

/// The conversation's lines: what was asked, what Claude said, and its steps, run together in
/// a card.
pub(crate) fn transcript(items: &[Item], bar: u64) -> Vec<gpui::AnyElement> {
    let mut out = Vec::new();
    let mut steps: Vec<Div> = Vec::new();
    let flush = |steps: &mut Vec<Div>, out: &mut Vec<gpui::AnyElement>| {
        if !steps.is_empty() {
            out.push(
                div()
                    .flex()
                    .flex_col()
                    .py(px(3.))
                    .rounded(px(8.))
                    .bg(rgb(WELL))
                    .border_1()
                    .border_color(rgb(LINE_SOFT))
                    .children(std::mem::take(steps))
                    .into_any_element(),
            );
        }
    };
    for (n, item) in items.iter().enumerate() {
        match item {
            Item::Tool {
                name, input, step, ..
            } => {
                let (what, detail) = agent::label(name, input, bar);
                steps.push(step_row(tool_icon(name), what, detail, step, n));
                continue;
            }
            Item::Outside(what) => {
                steps.push(step_row(
                    Icon::Agent,
                    "An agent edited".into(),
                    what.clone(),
                    &Step::Done,
                    n,
                ));
                continue;
            }
            _ => flush(&mut steps, &mut out),
        }
        out.push(match item {
            Item::You(text) => div()
                .flex()
                .justify_end()
                .child(
                    div()
                        .max_w(gpui::relative(0.88))
                        .px(px(11.))
                        .py(px(7.))
                        .rounded(px(12.))
                        .bg(rgb(RAISED))
                        .text_size(px(13.))
                        .line_height(px(18.))
                        .text_color(rgb(TEXT))
                        .child(text.clone()),
                )
                .into_any_element(),
            Item::Thinking => div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(12.5))
                .text_color(rgb(AGENT_TEXT))
                .child(div().size(px(6.)).rounded(px(3.)).bg(rgb(AGENT)))
                .child("Thinking…")
                .with_animation(
                    ("thinking", n),
                    Animation::new(Duration::from_millis(1400))
                        .repeat()
                        .with_easing(gpui::pulsating_between(0.4, 1.0)),
                    |d, t| d.opacity(t),
                )
                .into_any_element(),
            Item::Words(text) => words(text).into_any_element(),
            Item::End {
                seconds,
                steps,
                stopped,
                error,
            } => match error {
                Some(e) => div()
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(alpha(RECORD, 0.1))
                    .border_1()
                    .border_color(alpha(RECORD, 0.35))
                    .text_size(px(12.))
                    .line_height(px(17.))
                    .text_color(rgb(TEXT_2))
                    .child(e.clone())
                    .into_any_element(),
                None => div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(match stopped {
                        true => icon(Icon::Stop, 12.).text_color(rgb(TEXT_3)),
                        false => icon(Icon::Check, 12.).text_color(rgb(METER_LOW)),
                    })
                    .child(format!(
                        "{} {} · {steps} step{}",
                        if *stopped { "Stopped after" } else { "Done in" },
                        duration(*seconds),
                        if *steps == 1 { "" } else { "s" }
                    ))
                    .into_any_element(),
            },
            Item::Tool { .. } | Item::Outside(_) => unreachable!(),
        });
    }
    flush(&mut steps, &mut out);
    out
}

pub(crate) fn duration(s: f64) -> String {
    match s as u64 {
        0..60 => format!("{s:.0}s"),
        n => format!("{}m {:02}s", n / 60, n % 60),
    }
}

/// A step: what it's doing and to what, and how it went.
pub(crate) fn step_row(i: Icon, what: String, detail: String, step: &Step, n: usize) -> Div {
    let state = match step {
        Step::Running => div()
            .size(px(7.))
            .rounded(px(4.))
            .bg(rgb(AGENT))
            .with_animation(
                ("step", n),
                Animation::new(Duration::from_millis(1000))
                    .repeat()
                    .with_easing(gpui::pulsating_between(0.25, 1.0)),
                |d, t| d.opacity(t),
            )
            .into_any_element(),
        Step::Done => icon(Icon::Check, 12.)
            .text_color(rgb(TEXT_4))
            .into_any_element(),
        Step::Failed(_) => div()
            .text_size(px(11.))
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(RECORD))
            .child("!")
            .into_any_element(),
    };
    div()
        .flex()
        .flex_col()
        .px(px(9.))
        .py(px(4.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(icon(i, 13.).text_color(rgb(if *step == Step::Running {
                    AGENT
                } else {
                    TEXT_3
                })))
                .child(
                    div()
                        .flex_none()
                        .text_size(px(12.))
                        .text_color(rgb(TEXT_2))
                        .child(what),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .font_family(MONO)
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(detail),
                )
                .child(
                    div()
                        .flex_none()
                        .w(px(12.))
                        .flex()
                        .justify_center()
                        .child(state),
                ),
        )
        .when_some(
            match step {
                Step::Failed(e) => Some(e.clone()),
                _ => None,
            },
            |d, e| {
                d.child(
                    div()
                        .pl(px(21.))
                        .text_size(px(11.))
                        .line_height(px(15.))
                        .text_color(rgb(METER_HIGH))
                        .child(e),
                )
            },
        )
}

pub(crate) fn tool_icon(name: &str) -> Icon {
    match name.strip_prefix("mcp__pina__").unwrap_or(name) {
        "edit" => Icon::Pencil,
        "analyze" => Icon::Wave,
        "render" => Icon::Export,
        "transport" => Icon::Play,
        "plugins" | "params" | "presets" => Icon::Plug,
        n if n.starts_with("samples") => Icon::Samples,
        _ => Icon::Search,
    }
}

/// What Claude said, in paragraphs, its lists as lists and its bold as plain words.
pub(crate) fn words(text: &str) -> Div {
    let line = |s: &str| s.replace("**", "").replace('`', "");
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .text_size(px(13.))
        .line_height(px(19.))
        .text_color(rgb(TEXT))
        .children(text.trim().split("\n\n").map(|para| {
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .children(para.lines().map(|l| {
                    let t = l.trim_start();
                    match t
                        .strip_prefix("- ")
                        .or_else(|| t.strip_prefix("* "))
                        .or_else(|| t.strip_prefix("• "))
                    {
                        Some(item) => div()
                            .flex()
                            .gap(px(7.))
                            .child(div().flex_none().text_color(rgb(TEXT_3)).child("•"))
                            .child(div().flex_1().min_w(px(0.)).child(line(item))),
                        None => match t.trim_start_matches('#').len() < t.len() {
                            true => div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(line(t.trim_start_matches('#').trim())),
                            false => div().child(line(l)),
                        },
                    }
                }))
        }))
}
