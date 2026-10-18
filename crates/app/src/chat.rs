//! The Agent panel, beside the tracks: a conversation with Claude about the song. What it's
//! thinking, saying and doing shows as it happens and its edits land in the song as it makes
//! them; a `tempo ask` from a terminal shows here too, and so does any other agent's editing.

use std::time::{Duration, Instant};

use futures::StreamExt;
use gpui::{
    Animation, AnimationExt, Context, Div, Entity, Focusable, FontWeight, MouseButton,
    ScrollHandle, Subscription, Window, div, prelude::*, px, rgb,
};
use serde_json::Value;

use crate::{
    agent::{self, Agent, Event, Kind, Readiness},
    menu::{Menu, MenuAction},
    text::{TextEvent, TextInput},
    theme::*,
    workspace::Workspace,
};

/// How a tool call went.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Running,
    Done,
    Failed(String),
}

/// A line of the conversation.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// What was asked.
    You(String),
    /// Thinking, until it says or does something.
    Thinking,
    /// What it said.
    Words(String),
    Tool {
        id: String,
        name: String,
        input: Value,
        step: Step,
    },
    /// An edit by an agent outside the conversation.
    Outside(String),
    /// The end of an answer: how long it took, how many steps, whether it was stopped, or what
    /// went wrong.
    End {
        seconds: f64,
        steps: usize,
        stopped: bool,
        error: Option<String>,
    },
}

/// Ideas for a first request.
const STARTERS: [&str; 4] = [
    "Make a 4-bar drum groove that fits this song",
    "Find samples that fit this song's key and tempo",
    "Write a bassline that follows the chords",
    "Balance the mix and give it some space",
];

pub struct Chat {
    pub items: Vec<Item>,
    /// The agent to work with, as last chosen.
    pub kind: Kind,
    /// Whether each agent can work here, once asked.
    pub ready: Vec<(Kind, Readiness)>,
    agent: Option<Agent>,
    /// Which conversation is running, so the end of an old one doesn't end a new one.
    run: u32,
    pub busy: bool,
    since: Option<Instant>,
    pub model: Option<String>,
    /// A `tempo ask` in a terminal is the one asking.
    pub remote: bool,
    pub input: Entity<TextInput>,
    pub scroll: ScrollHandle,
    /// tempo's critic's notes on the song.
    pub critic: crate::producer::Critic,
    /// The song as it was when the agent was last asked: Undo this turn puts it back.
    pub before: Option<tempo_engine::project::Project>,
    /// Whether what's selected goes along with what's asked.
    pub use_selection: bool,
    _sent: Subscription,
}

impl Chat {
    pub fn new(cx: &mut Context<Workspace>) -> Self {
        let input = cx.new(|cx| TextInput::search("Ask Claude to make, change or mix…", cx));
        let sent = cx.subscribe(&input, |ws, input, e: &TextEvent, cx| match e {
            TextEvent::Confirm(text) if !text.is_empty() && !ws.chat.busy => {
                let text = text.clone();
                input.update(cx, |i, cx| i.set_text("", cx));
                ws.ask_agent(text, cx);
            }
            TextEvent::Cancel => input.update(cx, |i, cx| i.set_text("", cx)),
            _ => {}
        });
        // Asking each agent whether it's signed in takes a moment: in the background (and not
        // in tests, which leave the person's agents alone).
        let check = cx.background_spawn(async {
            match cfg!(test) {
                true => Vec::new(),
                false => agent::AGENTS
                    .iter()
                    .map(|d| (d.kind, d.kind.readiness()))
                    .collect(),
            }
        });
        cx.spawn(async move |this, cx| {
            let ready = check.await;
            this.update(cx, |ws, cx| {
                ws.chat.ready = ready;
                cx.notify();
            })
            .ok();
        })
        .detach();
        Chat {
            items: Vec::new(),
            kind: if cfg!(test) {
                Kind::ClaudeCode
            } else {
                agent::chosen()
            },
            ready: Vec::new(),
            agent: None,
            run: 0,
            busy: false,
            since: None,
            model: None,
            remote: false,
            input,
            scroll: ScrollHandle::new(),
            critic: Default::default(),
            before: None,
            use_selection: true,
            _sent: sent,
        }
    }

    /// Takes in what Claude did.
    pub fn apply(&mut self, e: Event) {
        let thinking = self.items.last() == Some(&Item::Thinking);
        if thinking && !matches!(e, Event::Thinking | Event::Started { .. }) {
            self.items.pop();
        }
        match e {
            Event::Asked { text } => self.asked(text),
            Event::Started { model } => self.model = Some(model),
            Event::Thinking if !thinking => self.items.push(Item::Thinking),
            Event::Thinking => {}
            Event::Text { text } => match self.items.last_mut() {
                Some(Item::Words(w)) => w.push_str(&text),
                _ => self.items.push(Item::Words(text.trim_start().to_string())),
            },
            Event::ToolStart { id, name } => self.items.push(Item::Tool {
                id,
                name,
                input: Value::Null,
                step: Step::Running,
            }),
            Event::Tool { id, name, input } => match self.tool(&id) {
                Some(Item::Tool { input: i, .. }) => *i = input,
                _ => self.items.push(Item::Tool {
                    id,
                    name,
                    input,
                    step: Step::Running,
                }),
            },
            Event::ToolDone { id, ok, output } => {
                if let Some(Item::Tool { step, .. }) = self.tool(&id) {
                    *step = match ok {
                        true => Step::Done,
                        false => Step::Failed(output.lines().next().unwrap_or("").to_string()),
                    };
                }
            }
            Event::Done {
                seconds,
                stopped,
                error,
            } => self.end(seconds, stopped, error),
        }
    }

    fn asked(&mut self, text: String) {
        self.items.push(Item::You(text));
        self.busy = true;
        self.since = Some(Instant::now());
    }

    fn end(&mut self, seconds: f64, stopped: bool, error: Option<String>) {
        let asked = self
            .items
            .iter()
            .rposition(|i| matches!(i, Item::You(_)))
            .unwrap_or(0);
        let mut steps = 0;
        for item in &mut self.items[asked..] {
            if let Item::Tool { step, .. } = item {
                steps += 1;
                if *step == Step::Running {
                    *step = Step::Done;
                }
            }
        }
        // Claude says how long it took; when it couldn't, the clock does.
        let seconds = match (seconds, self.since) {
            (0.0, Some(t)) => t.elapsed().as_secs_f64(),
            (s, _) => s,
        };
        self.items.push(Item::End {
            seconds,
            steps,
            stopped,
            error,
        });
        self.busy = false;
        self.remote = false;
        self.since = None;
    }

    fn tool(&mut self, id: &str) -> Option<&mut Item> {
        self.items
            .iter_mut()
            .rev()
            .find(|i| matches!(i, Item::Tool { id: x, .. } if x == id))
    }

    /// Whether `kind` can work here; None until asked.
    pub fn readiness(&self, kind: Kind) -> Option<&Readiness> {
        self.ready.iter().find(|(k, _)| *k == kind).map(|(_, r)| r)
    }

    /// Ends the conversation, for another song.
    pub fn forget(&mut self) {
        self.run += 1;
        self.agent = None;
        self.items.clear();
        self.busy = false;
        self.remote = false;
        self.since = None;
    }

    /// Notes an edit by an agent outside the conversation.
    pub fn outside(&mut self, what: &str) {
        self.items.push(Item::Outside(what.to_string()));
        if self.items.len() > 400 {
            self.items.remove(0);
        }
    }

    /// Whether the transcript is scrolled to (nearly) its end, so new lines should keep it there.
    fn at_end(&self) -> bool {
        let (at, max) = (self.scroll.offset().y, self.scroll.max_offset().y);
        f32::from(-at) >= f32::from(max) - 48.0
    }
}

impl Workspace {
    /// Shows or hides the Agent panel, where the Samples panel would be.
    pub fn toggle_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_agent = !self.show_agent;
        if self.show_agent {
            self.show_samples = false;
            self.view = crate::workspace::View::Arrange;
            let focus = self.chat.input.read(cx).focus_handle(cx);
            focus.focus(window, cx);
        }
        cx.notify();
    }

    /// Asks the agent something, starting it on the song if it isn't running.
    pub fn ask_agent(&mut self, text: String, cx: &mut Context<Self>) {
        let keep = self.chat.at_end();
        let def = self.chat.kind.def();
        let problem = match self.chat.readiness(self.chat.kind) {
            Some(Readiness::Missing) => Some(format!("{} isn't installed on this Mac.", def.name)),
            Some(Readiness::SignIn) => Some(format!(
                "{} isn't signed in. Run {} in a terminal, then ask again.",
                def.name, def.sign_in
            )),
            _ => None,
        };
        if let Some(problem) = problem {
            self.chat.asked(text);
            self.chat.end(0.0, false, Some(problem));
            cx.notify();
            return;
        }
        if self.chat.agent.is_none() {
            match Agent::start(self.chat.kind, &self.session.dir) {
                Ok((agent, mut events)) => {
                    self.chat.run += 1;
                    let run = self.chat.run;
                    self.chat.agent = Some(agent);
                    cx.spawn(async move |this, cx| {
                        while let Some(e) = events.next().await {
                            let more = this.update(cx, |ws, cx| {
                                if ws.chat.run == run {
                                    ws.heard_claude(e, cx);
                                }
                            });
                            if more.is_err() {
                                return;
                            }
                        }
                        // It's gone: a new question starts it again.
                        this.update(cx, |ws, cx| {
                            if ws.chat.run == run {
                                ws.chat.agent = None;
                                if ws.chat.busy {
                                    let name = ws.chat.kind.def().name;
                                    ws.chat.end(0.0, false, Some(format!("{name} stopped.")));
                                }
                                cx.notify();
                            }
                        })
                        .ok();
                    })
                    .detach();
                }
                Err(e) => {
                    self.chat.asked(text);
                    self.chat.end(0.0, false, Some(e));
                    cx.notify();
                    return;
                }
            }
        }
        // What's selected goes along, so "make this hit harder" knows what this is.
        let full = match self.selection_note().filter(|_| self.chat.use_selection) {
            Some((_, said)) => format!("(Selected in tempo: {said}.)\n\n{text}"),
            None => text.clone(),
        };
        if !self.chat.busy {
            self.chat.before = Some(self.session.project.clone());
        }
        let sent = self.chat.agent.as_mut().map(|a| a.ask(&full));
        self.chat.asked(text);
        if let Some(Err(e)) = sent {
            self.chat.agent = None;
            self.chat.end(0.0, false, Some(e));
        }
        if keep {
            self.chat.scroll.scroll_to_bottom();
        }
        cx.notify();
    }

    /// Takes in an event from Claude, keeping the end of the transcript in view.
    fn heard_claude(&mut self, e: Event, cx: &mut Context<Self>) {
        let keep = self.chat.at_end();
        self.chat.apply(e);
        if keep {
            self.chat.scroll.scroll_to_bottom();
        }
        cx.notify();
    }

    /// An event from a `tempo ask` in a terminal: the panel opens to show it.
    pub fn heard_agent(&mut self, e: Event, cx: &mut Context<Self>) {
        if let Event::Asked { .. } = e {
            self.chat.remote = true;
            self.show_agent = true;
            self.show_samples = false;
        }
        self.heard_claude(e, cx);
    }

    fn stop_agent(&mut self, cx: &mut Context<Self>) {
        if let Some(a) = self.chat.agent.as_mut() {
            a.interrupt();
        }
        cx.notify();
    }

    /// Works with another agent from now on, in a new conversation.
    pub fn pick_agent(&mut self, kind: Kind, cx: &mut Context<Self>) {
        if kind != self.chat.kind {
            self.chat.kind = kind;
            self.chat.model = None;
            agent::choose(kind);
            self.new_chat(cx);
        }
        cx.notify();
    }

    /// The agents to choose from: each one's name, and what it needs if it can't work yet.
    pub fn agent_menu(&self, at: gpui::Point<gpui::Pixels>) -> Menu {
        let items = agent::AGENTS
            .iter()
            .map(|d| {
                let mark = if d.kind == self.chat.kind {
                    "✓ "
                } else {
                    "    "
                };
                let how = match self.chat.readiness(d.kind) {
                    None => String::new(),
                    Some(Readiness::Ready(on)) if on.is_empty() => String::new(),
                    Some(Readiness::Ready(on)) => format!(" · {on}"),
                    Some(Readiness::Missing) => " · not installed".into(),
                    Some(Readiness::SignIn) => format!(" · sign in: {}", d.sign_in),
                };
                (
                    format!("{mark}{}  {}{how}", d.logo, d.name),
                    MenuAction::Agent(d.kind),
                )
            })
            .collect();
        Menu {
            at,
            items,
            ..Default::default()
        }
    }

    /// A new conversation: the one running ends.
    fn new_chat(&mut self, cx: &mut Context<Self>) {
        self.chat.run += 1;
        self.chat.agent = None;
        self.chat.items.clear();
        self.chat.busy = false;
        self.chat.remote = false;
        self.chat.since = None;
        cx.notify();
    }

    pub fn agent_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
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
            .h(px(40.))
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
                            ws.menu = Some(ws.agent_menu(e.position));
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(rgb(if c.busy { AGENT_TEXT } else { TEXT_3 }))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(status),
            )
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
                .child(
                    div()
                        .min_h(px(30.))
                        .px(px(13.))
                        .pt(px(12.))
                        .pb(px(4.))
                        .flex()
                        .items_center()
                        .text_size(px(13.))
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
    fn agent_welcome(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
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
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .children(STARTERS.iter().enumerate().map(|(i, s)| {
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
                            .child(div().flex_1().min_w(px(0.)).line_height(px(17.)).child(*s))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| ws.ask_agent(s.to_string(), cx)),
                            )
                    })),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .line_height(px(16.))
                    .text_color(rgb(TEXT_4))
                    .child(
                        "Or from a terminal: tempo ask <song> \"…\" shows here too. Any agent \
                         can drive tempo with tempo mcp.",
                    ),
            )
    }
}

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

/// "claude-opus-5-5" as "Opus 5.5".
fn model_name(id: &str) -> String {
    let rest = id.strip_prefix("claude-").unwrap_or(id);
    let mut parts = rest.split('-');
    let family = parts.next().unwrap_or_default();
    let version: Vec<&str> = parts.take_while(|p| p.len() <= 2).collect();
    let mut name = family.to_string();
    if let Some(f) = name.get_mut(0..1) {
        f.make_ascii_uppercase();
    }
    match version.is_empty() {
        true => name,
        false => format!("{name} {}", version.join(".")),
    }
}

/// The conversation's lines: what was asked, what Claude said, and its steps, run together in
/// a card.
fn transcript(items: &[Item], bar: u64) -> Vec<gpui::AnyElement> {
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

fn duration(s: f64) -> String {
    match s as u64 {
        0..60 => format!("{s:.0}s"),
        n => format!("{}m {:02}s", n / 60, n % 60),
    }
}

/// A step: what it's doing and to what, and how it went.
fn step_row(i: Icon, what: String, detail: String, step: &Step, n: usize) -> Div {
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

fn tool_icon(name: &str) -> Icon {
    match name.strip_prefix("mcp__tempo__").unwrap_or(name) {
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
fn words(text: &str) -> Div {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_ids_read_as_names() {
        assert_eq!(model_name("claude-opus-5-5"), "Opus 5.5");
        assert_eq!(model_name("claude-sonnet-4-5-20250929"), "Sonnet 4.5");
        assert_eq!(model_name("custom"), "Custom");
    }
}
