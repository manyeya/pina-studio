//! The Agent panel, and what it offers before the first question.

use super::*;

impl Workspace {
    pub fn agent_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let tuning = self.tuning_label();
        let c = &self.chat;
        let def = c.kind.def();
        let status = match (c.busy, c.since, c.remote) {
            (true, Some(t), true) => format!("From the terminal · {}s", t.elapsed().as_secs()),
            (true, Some(t), false) => format!("Working · {}s", t.elapsed().as_secs()),
            _ => c
                .model
                .as_deref()
                .filter(|m| !m.is_empty())
                .map(model_name)
                .unwrap_or_default(),
        };
        let header = div()
            .h(px(TOP_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .pl(px(12.))
            .pr(px(7.))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(badge(c.kind, 20., c.busy))
            // Its name picks another.
            .child(
                div()
                    .id("agent-pick")
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .px(px(4.))
                    .py(px(2.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(RAISED)))
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(def.name)
                    .child(icon(Icon::Chevron, 12.).text_color(rgb(TEXT_3)))
                    .debug_selector(|| "agent-pick".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &gpui::MouseDownEvent, _, cx| {
                            check_agents(None, cx);
                            ws.menu = Some(ws.agent_menu(e.position));
                            cx.notify();
                        }),
                    ),
            )
            // Working: for how long; else its model and effort, to pick another.
            .when(c.busy, |d| {
                d.child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .font_family(MONO)
                        .text_size(px(11.))
                        .text_color(rgb(AGENT_TEXT))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(status),
                )
            })
            .when(!c.busy, |d| {
                d.child(
                    div().flex_1().min_w(px(0.)).flex().child(
                        div()
                            .min_w(px(0.))
                            .h(px(24.))
                            .px(px(6.))
                            .flex()
                            .items_center()
                            .gap(px(3.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_3))
                            .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT_2)))
                            .debug_selector(|| "agent-tuning".into())
                            .child(
                                div()
                                    .min_w(px(0.))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(tuning),
                            )
                            .child(icon(Icon::Chevron, 11.).text_color(rgb(TEXT_4)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, e: &gpui::MouseDownEvent, _, cx| {
                                    ws.menu = Some(ws.tuning_menu(e.position));
                                    cx.notify();
                                }),
                            ),
                    ),
                )
            })
            .child(
                icon_seg(Icon::Plus, false, 26.)
                    .debug_selector(|| "new-chat".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.new_chat(cx)),
                    ),
            )
            .child(
                icon_seg(Icon::Close, false, 26.)
                    .debug_selector(|| "close-agent".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.show_agent = false;
                            cx.notify();
                        }),
                    ),
            );
        let bar = self.session.project.bar();
        let body = match c.items.is_empty() {
            true => self.agent_welcome(cx).into_any_element(),
            false => div()
                .id("agent-transcript")
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .track_scroll(&c.scroll)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .p(px(12.))
                        .children(transcript(&c.items, bar))
                        .when(
                            !c.busy
                                && matches!(c.items.last(), Some(Item::End { .. }))
                                && c.before
                                    .as_ref()
                                    .is_some_and(|b| *b != self.session.project),
                            |d| {
                                d.child(
                                    div().flex().child(
                                        div()
                                            .id("undo-turn")
                                            .h(px(26.))
                                            .px(px(10.))
                                            .flex()
                                            .items_center()
                                            .gap(px(6.))
                                            .rounded(px(7.))
                                            .border_1()
                                            .border_color(rgb(LINE_STRONG))
                                            .text_size(px(11.5))
                                            .text_color(rgb(TEXT_2))
                                            .cursor_pointer()
                                            .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                                            .child(icon(Icon::Back, 11.).text_color(rgb(TEXT_2)))
                                            .child("Undo this turn")
                                            .debug_selector(|| "undo-turn".into())
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|ws, _, _, cx| ws.undo_turn(cx)),
                                            ),
                                    ),
                                )
                            },
                        ),
                )
                .into_any_element(),
        };
        let (busy, remote) = (c.busy, c.remote);
        let empty = c.input.read(cx).text().trim().is_empty();
        // Send, a solid circle when there's something to send; Stop, the same circle, while it
        // works.
        let send = div()
            .size(px(30.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(15.))
            .map(|d| match busy {
                true => d
                    .bg(rgb(TEXT))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(0xffffff)))
                    .child(div().size(px(10.)).rounded(px(2.)).bg(rgb(BG)))
                    .debug_selector(|| "agent-stop".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.stop_agent(cx)),
                    ),
                false => d
                    .when(empty, |d| d.bg(rgb(RAISED)))
                    .when(!empty, |d| {
                        d.bg(rgb(TEXT))
                            .cursor_pointer()
                            .hover(|d| d.bg(rgb(0xffffff)))
                    })
                    .child(icon(Icon::Send, 14.).text_color(rgb(if empty { TEXT_4 } else { BG })))
                    .debug_selector(|| "agent-send".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.chat.input.update(cx, |i, cx| i.confirm(cx));
                        }),
                    ),
            })
            .when(busy && remote, |d| d.opacity(0.4));
        // What's selected, as context that goes along: a chip to take out, or (taken out) a
        // dashed one to put back.
        let selection =
            self.selection_note().map(|(label, _)| {
                let on = c.use_selection;
                div()
                    .id("agent-selection")
                    .min_w(px(0.))
                    .flex_shrink(1.)
                    .h(px(26.))
                    .pl(px(8.))
                    .pr(px(if on { 4. } else { 9. }))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(8.))
                    .border_1()
                    .cursor_pointer()
                    .text_size(px(11.5))
                    .when(on, |d| {
                        d.bg(rgb(RAISED))
                            .border_color(rgb(LINE_STRONG))
                            .text_color(rgb(TEXT_2))
                            .hover(|d| d.border_color(rgb(TEXT_4)))
                    })
                    .when(!on, |d| {
                        d.border_dashed()
                            .border_color(rgb(LINE_STRONG))
                            .text_color(rgb(TEXT_4))
                            .hover(|d| d.text_color(rgb(TEXT_3)))
                    })
                    .child(
                        icon(if on { Icon::Pointer } else { Icon::Plus }, 11.)
                            .text_color(rgb(if on { ACCENT_TEXT } else { TEXT_4 })),
                    )
                    .child(
                        div()
                            .min_w(px(0.))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(label),
                    )
                    .when(on, |d| {
                        d.child(
                            div()
                                .size(px(18.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(5.))
                                .hover(|d| d.bg(rgb(LINE_STRONG)))
                                .child(icon(Icon::Close, 9.).text_color(rgb(TEXT_3))),
                        )
                    })
                    .debug_selector(|| "agent-selection".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.chat.use_selection ^= true;
                            cx.notify();
                        }),
                    )
            });
        let critic = self.critic_chip(cx);
        let (palette, start) = (self.palette_card(cx), self.palette_start(cx));

        let c = &self.chat;
        // One card: what's typed, and under it what goes with it and the button that sends it.
        let composer = div().flex_none().px(px(10.)).pb(px(10.)).pt(px(4.)).child(
            div()
                .flex()
                .flex_col()
                .rounded(px(14.))
                .bg(rgb(FIELD))
                .border_1()
                .border_color(rgb(LINE_STRONG))
                .shadow_lg()
                .children(palette)
                .children(start)
                .child(
                    div()
                        .px(px(13.))
                        .pt(px(12.))
                        .pb(px(6.))
                        .debug_selector(|| "agent-input".into())
                        .child(c.input.clone()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .pl(px(8.))
                        .pr(px(7.))
                        .pt(px(4.))
                        .pb(px(7.))
                        .children(selection)
                        .child(critic)
                        .child(div().flex_1())
                        .child(send),
                ),
        );
        div()
            .w(px(self.panel_width()))
            .flex_none()
            .h_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(SURFACE))
            .border_l_1()
            .border_color(rgb(LINE))
            .child(header)
            .child(self.agent_context(cx))
            .child(body)
            .children(self.critic_sheet(cx))
            .child(composer)
            .child(self.panel_edge(cx))
    }

    /// Before the first request: what it does, and some things to ask.
    pub(crate) fn agent_welcome(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let def = self.chat.kind.def();
        let problem = match self.chat.readiness(self.chat.kind) {
            Some(Readiness::Missing) => Some(format!(
                "{} isn't installed on this Mac. Install it and sign in ({}), or pick another agent above.",
                def.name, def.sign_in
            )),
            Some(Readiness::SignIn) => Some(format!(
                "{} isn't signed in. Run {} in a terminal, then come back.",
                def.name, def.sign_in
            )),
            _ => None,
        };
        // Scrolls, top first, when the critic's sheet leaves it little room.
        div()
            .id("agent-welcome")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(14.))
            .px(px(18.))
            .pt(px(22.))
            .pb(px(12.))
            .child(badge(self.chat.kind, 36., false))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(17.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("Make music with {}", def.name)),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .line_height(px(18.))
                            .text_color(rgb(TEXT_3))
                            .child(format!(
                                "Ask for anything. {} edits this song while you watch: every \
                                     step shows here, and every edit lands in the tracks as it's \
                                     made.",
                                def.name
                            )),
                    ),
            )
            .when_some(problem, |d, problem| {
                d.child(
                    div()
                        .p(px(10.))
                        .rounded(px(8.))
                        .bg(alpha(RECORD, 0.1))
                        .border_1()
                        .border_color(alpha(RECORD, 0.35))
                        .text_size(px(12.))
                        .line_height(px(17.))
                        .text_color(rgb(TEXT_2))
                        .child(problem),
                )
            })
            .child(
                div().flex().flex_col().gap(px(6.)).children(
                    self.suggestions()
                        .into_iter()
                        .enumerate()
                        .map(|(i, (s, ask))| {
                            div()
                                .id(("starter", i))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .px(px(10.))
                                .py(px(8.))
                                .rounded(px(8.))
                                .bg(rgb(CARD))
                                .border_1()
                                .border_color(rgb(LINE))
                                .text_size(px(12.5))
                                .text_color(rgb(TEXT_2))
                                .cursor_pointer()
                                .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                                .child(icon(Icon::Forward, 12.).flex_none().text_color(rgb(AGENT)))
                                .child(div().flex_1().min_w(px(0.)).line_height(px(17.)).child(s))
                                .debug_selector(move || format!("starter-{i}"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| ws.ask_agent(ask.clone(), cx)),
                                )
                        }),
                ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .line_height(px(16.))
                    .text_color(rgb(TEXT_4))
                    .child(
                        "Or from a terminal: pina ask <song> \"…\" shows here too. Any agent \
                         can drive Pina Studio with pina mcp.",
                    ),
            )
    }
}
