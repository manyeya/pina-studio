//! The Agent panel, beside the tracks: a conversation with Claude about the song. What it's
//! thinking, saying and doing shows as it happens and its edits land in the song as it makes
//! them; a `pina ask` from a terminal shows here too, and so does any other agent's editing.

mod asking;
mod panel;
mod transcript;
mod tuning;

pub use asking::*;
pub use transcript::*;
pub(crate) use tuning::*;

use std::time::{Duration, Instant};

use futures::StreamExt;
use gpui::{
    Animation, AnimationExt, Context, Div, Entity, Focusable, FontWeight, MouseButton,
    ScrollHandle, Subscription, Window, div, prelude::*, px, rgb,
};
use serde_json::Value;

use crate::{
    agent::{self, Agent, Event, Kind, Readiness},
    ui::menu::{Menu, MenuAction},
    ui::text::{TextEvent, TextInput},
    ui::theme::*,
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
    /// A `pina ask` in a terminal is the one asking.
    pub remote: bool,
    pub input: Entity<TextInput>,
    pub scroll: ScrollHandle,
    /// Pina Studio's critic's notes on the song.
    pub critic: crate::agent::producer::Critic,
    /// The song as it was when the agent was last asked: Undo this turn puts it back.
    pub before: Option<pina_engine::song::project::Project>,
    /// Whether what's selected goes along with what's asked.
    pub use_selection: bool,
    /// The model and effort it runs with, as last picked for it.
    pub tuning: agent::Tuning,
    /// The palette as the agent was last told it, so it's told again only when it changes.
    pub palette_heard: Option<pina_engine::song::project::Palette>,
    _sent: Subscription,
}

impl Chat {
    pub fn new(cx: &mut Context<Workspace>) -> Self {
        // It wraps and grows as it's written in, up to 8 lines; ⇧Enter starts a new line.
        let input = cx.new(|cx| {
            TextInput::writing("Ask Claude to make, change or mix…", 8, cx).typed(
                13.,
                19.,
                FontWeight::NORMAL,
            )
        });
        let sent = cx.subscribe(&input, |ws, input, e: &TextEvent, cx| match e {
            TextEvent::Confirm(text) if !text.is_empty() && !ws.chat.busy => {
                let text = text.clone();
                input.update(cx, |i, cx| i.set_text("", cx));
                ws.ask_agent(text, cx);
            }
            TextEvent::Cancel => input.update(cx, |i, cx| i.set_text("", cx)),
            _ => {}
        });
        check_agents(None, cx);
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
            palette_heard: None,
            tuning: agent::tuning(agent::chosen()),
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

#[cfg(test)]
mod tests;
