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

impl Workspace {
    /// The conversation, as it's drawn: what was asked, what the agent said, what it showed, and
    /// its steps folded into a line a run.
    pub(crate) fn transcript(&mut self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let items = std::mem::take(&mut self.chat.items);
        let mut paint = Painter::new(self.chat.pick);
        let out = self.lines(&items, &mut paint, cx);
        self.chat.items = items;
        *self.chat.shown.borrow_mut() = paint.shown;
        out
    }

    /// Opens or folds something in the conversation, by where it is.
    pub(crate) fn toggle_opened(&mut self, n: usize, cx: &mut Context<Self>) {
        if !self.chat.opened.remove(&n) {
            self.chat.opened.insert(n);
        }
        cx.notify();
    }

    fn lines(
        &self,
        items: &[Item],
        paint: &mut Painter,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let bar = self.session.project.bar();
        let kind = self.chat.kind;
        let last_changes = items
            .iter()
            .rposition(|i| matches!(i, Item::Changes { .. }))
            .filter(|&c| !items[c..].iter().any(|i| matches!(i, Item::You(_))));
        let mut out = Vec::new();
        let mut steps: Vec<usize> = Vec::new();
        for (n, item) in items.iter().enumerate() {
            match item {
                Item::Tool { name, .. } if is_card(name) => {}
                Item::Tool { .. } | Item::Outside(_) => {
                    steps.push(n);
                    continue;
                }
                _ => {}
            }
            if !steps.is_empty() {
                let run = std::mem::take(&mut steps);
                out.push(self.activity(items, &run, bar, cx).into_any_element());
            }
            match item {
                Item::Tool { input, .. } => {
                    out.extend(self.card(n, input, items, cx).map(|d| d.into_any_element()));
                }
                Item::Midi { path, .. } => {
                    let at = self.chat.shelf.iter().position(|m| m.path == *path);
                    let card = at.and_then(|i| self.midi_card(i, cx));
                    out.extend(card.map(|d| d.into_any_element()));
                }
                Item::You(text) => out.push(self.asked_line(paint, text, n, cx).into_any_element()),
                Item::Words(text) => out.push(self.said(paint, text, n, cx).into_any_element()),
                Item::Thinking => out.push(
                    div()
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
                ),
                Item::Changes { lines } => out.push(
                    self.changes_card(n, lines, last_changes == Some(n), cx)
                        .into_any_element(),
                ),
                Item::End {
                    seconds,
                    steps,
                    stopped,
                    error: None,
                } => {
                    let answer = answer_of(items, n);
                    let group: gpui::SharedString = format!("end-{n}").into();
                    let copy = self.copy_button(format!("answer-{n}"), answer, cx);
                    out.push(
                        div()
                            .group(group.clone())
                            .debug_selector(move || format!("end-{n}"))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .h(px(22.))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child(match stopped {
                                true => icon(Icon::Stop, 12.).text_color(rgb(TEXT_3)),
                                false => icon(Icon::Check, 12.).text_color(rgb(METER_LOW)),
                            })
                            .child(format!(
                                "{} {}",
                                if *stopped { "Stopped after" } else { "Done in" },
                                duration(*seconds),
                            ))
                            .when(*steps > 0, |d| {
                                d.child(format!(
                                    "· {steps} step{}",
                                    if *steps == 1 { "" } else { "s" }
                                ))
                            })
                            .child(div().flex_1())
                            .child(
                                div()
                                    .invisible()
                                    .group_hover(group, |d| d.visible())
                                    .child(copy),
                            )
                            .into_any_element(),
                    );
                }
                // What it means first; what the agent said, small, under it.
                Item::End { error: Some(e), .. } => out.push(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .p(px(10.))
                        .rounded(px(8.))
                        .bg(alpha(RECORD, 0.1))
                        .border_1()
                        .border_color(alpha(RECORD, 0.35))
                        .text_size(px(12.))
                        .line_height(px(17.))
                        .text_color(rgb(TEXT_2))
                        .debug_selector(move || format!("agent-error-{n}"))
                        .map(|d| match kind.explain(e) {
                            Some(plain) => d.child(plain).child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_4))
                                    .child(e.clone()),
                            ),
                            None => d.child(e.clone()),
                        })
                        .into_any_element(),
                ),
                Item::Compared { reference, lines } => out.push(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(5.))
                        .px(px(11.))
                        .py(px(9.))
                        .rounded(px(10.))
                        .bg(rgb(CARD))
                        .border_1()
                        .border_color(rgb(LINE))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .text_size(px(11.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(TEXT))
                                .child(icon(Icon::Wave, 12.).text_color(rgb(AGENT_TEXT)))
                                .child(format!("Compared with {reference}")),
                        )
                        .children(lines.iter().enumerate().map(|(i, l)| {
                            div()
                                .text_size(px(12.))
                                .line_height(px(16.))
                                .text_color(rgb(if i == 0 { TEXT } else { TEXT_2 }))
                                .when(i == 0, |d| d.font_family(MONO).text_size(px(11.)))
                                .child(match i {
                                    0 => l.clone(),
                                    _ => format!("· {l}"),
                                })
                        }))
                        .into_any_element(),
                ),
                Item::Outside(_) => {}
            }
        }
        if !steps.is_empty() {
            out.push(self.activity(items, &steps, bar, cx).into_any_element());
        }
        out
    }

    /// What was asked, to the right, with a way to copy it.
    fn asked_line(&self, paint: &mut Painter, text: &str, n: usize, cx: &mut Context<Self>) -> Div {
        let group: gpui::SharedString = format!("you-{n}").into();
        div()
            .group(group.clone())
            .debug_selector(move || format!("you-{n}"))
            .flex()
            .justify_end()
            .items_start()
            .gap(px(4.))
            .child(
                div()
                    .mt(px(3.))
                    .invisible()
                    .group_hover(group, |d| d.visible())
                    .child(self.copy_button(format!("you-{n}"), text.to_string(), cx)),
            )
            .child(
                div()
                    .max_w(gpui::relative(0.86))
                    .px(px(11.))
                    .py(px(7.))
                    .rounded(px(12.))
                    .bg(rgb(RAISED))
                    .text_size(px(13.))
                    .line_height(px(19.))
                    .text_color(rgb(TEXT))
                    .child(paint.text(text.to_string(), Vec::new(), "\n\n")),
            )
    }

    /// A run of steps, folded to a line: what it's doing now, or what it did. A click opens it.
    fn activity(&self, items: &[Item], run: &[usize], bar: u64, cx: &mut Context<Self>) -> Div {
        let key = run[0];
        let open = self.chat.opened.contains(&key);
        let step_of = |n: usize| match &items[n] {
            Item::Tool { step, .. } => step.clone(),
            _ => Step::Done,
        };
        let failed = run
            .iter()
            .filter(|&&n| matches!(step_of(n), Step::Failed(_)))
            .count();
        let now = run.iter().rev().find_map(|&n| match &items[n] {
            Item::Tool {
                name,
                input,
                step: Step::Running,
                ..
            } => Some(agent::label(name, input, bar)),
            _ => None,
        });
        let live = now.is_some();
        let (line, detail) = match now {
            Some((what, detail)) => (what, detail),
            None => (summary(items, run), String::new()),
        };
        let mark = match (live, failed) {
            (true, _) => div()
                .size(px(7.))
                .rounded(px(4.))
                .bg(rgb(AGENT))
                .with_animation(
                    ("step-now", key),
                    Animation::new(Duration::from_millis(1000))
                        .repeat()
                        .with_easing(gpui::pulsating_between(0.25, 1.0)),
                    |d, t| d.opacity(t),
                )
                .into_any_element(),
            (false, 0) => icon(Icon::Check, 12.)
                .text_color(rgb(TEXT_4))
                .into_any_element(),
            _ => div()
                .text_size(px(11.))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(RECORD))
                .child("!")
                .into_any_element(),
        };
        let rows: Vec<Div> = run
            .iter()
            .filter(|_| open)
            .map(|&n| match &items[n] {
                Item::Tool {
                    name, input, step, ..
                } => {
                    let (what, detail) = agent::label(name, input, bar);
                    step_row(tool_icon(name), what, detail, step, n)
                }
                Item::Outside(what) => step_row(
                    Icon::Agent,
                    "An agent edited".into(),
                    what.clone(),
                    &Step::Done,
                    n,
                ),
                _ => div(),
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .rounded(px(8.))
            .bg(rgb(WELL))
            .border_1()
            .border_color(rgb(LINE_SOFT))
            .child(
                div()
                    .h(px(30.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .debug_selector(move || format!("activity-{key}"))
                    .child(div().w(px(13.)).flex().justify_center().child(mark))
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(12.))
                            .text_color(rgb(if live { TEXT } else { TEXT_2 }))
                            .child(line),
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
                    .when(failed > 0, |d| {
                        d.child(
                            div()
                                .flex_none()
                                .text_size(px(10.5))
                                .text_color(rgb(METER_HIGH))
                                .child(format!("{failed} failed")),
                        )
                    })
                    .when(live, |d| {
                        d.child(
                            div()
                                .flex_none()
                                .font_family(MONO)
                                .text_size(px(10.))
                                .text_color(rgb(TEXT_4))
                                .child(run.len().to_string()),
                        )
                    })
                    .child(
                        icon(
                            if open {
                                Icon::Chevron
                            } else {
                                Icon::ChevronRight
                            },
                            11.,
                        )
                        .flex_none()
                        .text_color(rgb(TEXT_4)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.toggle_opened(key, cx);
                        }),
                    ),
            )
            .when(open, |d| {
                d.child(
                    div()
                        .pb(px(4.))
                        .border_t_1()
                        .border_color(rgb(LINE_SOFT))
                        .pt(px(3.))
                        .children(rows),
                )
            })
    }
}

/// All an answer said, for copying: from the question before the end of a turn at `end`.
fn answer_of(items: &[Item], end: usize) -> String {
    let from = items[..end]
        .iter()
        .rposition(|i| matches!(i, Item::You(_)))
        .unwrap_or(0);
    let said = items[from..end].iter().filter_map(|i| match i {
        Item::Words(t) => Some(t.trim()),
        _ => None,
    });
    said.collect::<Vec<_>>().join("\n\n")
}

/// A run of steps in words: how many, and of what.
fn summary(items: &[Item], run: &[usize]) -> String {
    let (mut edits, mut checks, mut finds) = (0, 0, 0);
    for &n in run {
        let tool = match &items[n] {
            Item::Tool { name, .. } => name.strip_prefix("mcp__pina__").unwrap_or(name),
            _ => "edit",
        };
        match tool {
            "edit" | "samples_put" | "mix" | "master" | "midi_write" => edits += 1,
            "analyze" | "listen" | "critique" | "render" => checks += 1,
            t if t.starts_with("samples") || t == "sounds" || t == "theory" => finds += 1,
            _ => {}
        }
    }
    let many = |n: usize, one: &str, more: &str| match n {
        0 => None,
        1 => Some(format!("1 {one}")),
        n => Some(format!("{n} {more}")),
    };
    let parts: Vec<String> = [
        many(edits, "edit", "edits"),
        many(checks, "check", "checks"),
        many(finds, "search", "searches"),
    ]
    .into_iter()
    .flatten()
    .collect();
    match (parts.is_empty(), run.len()) {
        (true, 1) => "1 step".to_string(),
        (true, n) => format!("{n} steps"),
        _ => parts.join(" · "),
    }
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
