//! The Studio Agent panel, and what it offers before the first question.

use super::*;
use crate::workspace::Help;

impl Workspace {
    pub fn agent_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        self.shelf_sync();
        self.past_sync(cx);
        let page = self.chat.page;
        let items = match page == ChatPage::Chat && !self.chat.items.is_empty() {
            true => self.transcript(cx),
            false => Vec::new(),
        };
        let tuning = self.tuning_label();
        let c = &self.chat;
        let def = c.kind.def();
        let status = match (c.busy, c.since, c.remote) {
            (true, Some(t), true) => format!("From the terminal · {}s", t.elapsed().as_secs()),
            (true, Some(t), false) => format!("Working · {}s", t.elapsed().as_secs()),
            _ => String::new(),
        };
        let asked = c.has_questions();
        let subtitle = match (asked, c.title.is_empty()) {
            (false, _) => "New conversation".to_string(),
            (_, true) => title_of(&c.items),
            _ => c.title.clone(),
        };
        let header = div()
            .h(px(TOP_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(9.))
            .pl(px(12.))
            .pr(px(7.))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(badge(c.kind, 24., c.busy))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(13.))
                            .line_height(px(17.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Studio Agent"),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .line_height(px(14.))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .debug_selector(|| "agent-title".into())
                            .child(subtitle),
                    ),
            )
            .when(asked, |d| {
                d.child(
                    icon_seg(Icon::More, false, 26.)
                        .debug_selector(|| "agent-more".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, e: &gpui::MouseDownEvent, _, cx| {
                                ws.menu = Some(ws.this_chat_menu(e.position));
                                cx.notify();
                            }),
                        ),
                )
            })
            .child(
                icon_seg(Icon::Plus, false, 26.)
                    .debug_selector(|| "new-chat".into())
                    .help("New conversation: this one stays in History", cx)
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
        // Under what's typed: who answers and how, or how long it's been at it.
        let engine = div()
            .flex_none()
            .h(px(22.))
            .px(px(5.))
            .flex()
            .items_center()
            .gap(px(5.))
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|d| d.bg(rgb(RAISED)))
            .text_size(px(11.))
            .text_color(rgb(TEXT_3))
            .debug_selector(|| "agent-pick".into())
            .child(logo(c.kind, 11.))
            .child(def.name)
            .child(icon(Icon::Chevron, 10.).text_color(rgb(TEXT_4)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &gpui::MouseDownEvent, _, cx| {
                    check_agents(None, cx);
                    ws.menu = Some(ws.agent_menu(e.position));
                    cx.notify();
                }),
            );
        let how = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .justify_end()
            .when(c.busy, |d| {
                d.child(
                    div()
                        .min_w(px(0.))
                        .pr(px(5.))
                        .font_family(MONO)
                        .text_size(px(10.5))
                        .text_color(rgb(AGENT_TEXT))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(status),
                )
            })
            .when(!c.busy, |d| {
                d.child(
                    div()
                        .min_w(px(0.))
                        .h(px(22.))
                        .px(px(5.))
                        .flex()
                        .items_center()
                        .gap(px(3.))
                        .rounded(px(6.))
                        .cursor_pointer()
                        .text_size(px(11.))
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
                        .child(icon(Icon::Chevron, 10.).flex_none().text_color(rgb(TEXT_4)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, e: &gpui::MouseDownEvent, _, cx| {
                                ws.menu = Some(ws.tuning_menu(e.position));
                                cx.notify();
                            }),
                        ),
                )
            });
        let tabs = self.agent_tabs(cx);
        let body = match (page, c.items.is_empty()) {
            (ChatPage::Midi, _) => self.agent_midi_page(cx).into_any_element(),
            (ChatPage::History, _) => self.agent_history(cx).into_any_element(),
            (_, true) => self.agent_welcome(cx).into_any_element(),
            _ => div()
                .id("agent-transcript")
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .track_scroll(&c.scroll)
                .debug_selector(|| "agent-transcript".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, e: &gpui::MouseDownEvent, _, cx| ws.chat_press(e, cx)),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|ws, e: &gpui::MouseDownEvent, _, cx| {
                        if ws.chat.picked_text().is_some() {
                            cx.stop_propagation();
                            let items =
                                vec![("Copy".to_string(), MenuAction::Chat(ChatOp::Picked))];
                            ws.menu = Some(Menu {
                                at: e.position,
                                items,
                                ..Default::default()
                            });
                            cx.notify();
                        }
                    }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .p(px(12.))
                        .children(items)
                        // It went wrong: ask again, or ask the other agent.
                        .when(c.failed(), |d| {
                            let other = c.other_kind();
                            let button = |id: &'static str, label: String| {
                                div()
                                    .id(id)
                                    .h(px(26.))
                                    .px(px(10.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(7.))
                                    .border_1()
                                    .border_color(rgb(LINE_STRONG))
                                    .text_size(px(11.5))
                                    .text_color(rgb(TEXT_2))
                                    .cursor_pointer()
                                    .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                                    .child(label)
                                    .debug_selector(move || id.into())
                            };
                            d.child(
                                div()
                                    .flex()
                                    .gap(px(6.))
                                    .child(button("agent-retry", "Try again".into()).on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, _, _, cx| ws.retry_agent(cx)),
                                    ))
                                    .when_some(other, |d, k| {
                                        d.child(
                                            button("agent-other", format!("Ask {}", k.def().name))
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|ws, _, _, cx| {
                                                        ws.ask_other_agent(cx)
                                                    }),
                                                ),
                                        )
                                    }),
                            )
                        })
                        .when(
                            !c.busy
                                && matches!(c.items.last(), Some(Item::End { .. }))
                                && !c
                                    .items
                                    .iter()
                                    .rev()
                                    .take_while(|i| !matches!(i, Item::You(_)))
                                    .any(|i| matches!(i, Item::Changes { .. }))
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
        let c = &self.chat;
        let (busy, remote) = (c.busy, c.remote);
        let empty = c.input.read(cx).text().trim().is_empty() && c.attached.is_empty();
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
        let attached = self.midi_attached(cx);
        let attach = self.midi_attach_button(cx);
        let drop_hint = self.midi_drop_hint();
        let c = &self.chat;
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
                .children(attached)
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
                        .child(attach)
                        .children(selection)
                        .child(critic)
                        .child(div().flex_1())
                        .child(send),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .px(px(5.))
                        .py(px(3.))
                        .border_t_1()
                        .border_color(rgb(LINE_SOFT))
                        .child(engine)
                        .child(how),
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
            .child(tabs)
            .child(body)
            .children(self.critic_sheet(cx))
            .child(composer)
            .children(drop_hint)
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
            .px(px(14.))
            .pt(px(14.))
            .pb(px(12.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(17.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("What are we making?"),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .line_height(px(18.))
                            .text_color(rgb(TEXT_3))
                            .child(
                                "The Studio Agent works in this song while you watch: every edit \
                                 lands in the tracks as it's made, and you can undo a whole turn.",
                            ),
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
            .children(self.recent_chats(cx))
    }
}
