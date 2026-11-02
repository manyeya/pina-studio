//! Asking the agent: the panel opened, a question sent once the agent is ready, its answer heard, stopped.

use super::*;

/// Asks each agent whether it can work (installed, signed in), in the background as it takes a
/// moment; then, if a question was waiting on it, asks that. Tests leave the person's agents alone
/// and keep what they set.
pub fn check_agents(then: Option<String>, cx: &mut Context<Workspace>) {
    let check = cx.background_spawn(async {
        match cfg!(test) {
            true => None,
            false => Some(
                agent::AGENTS
                    .iter()
                    .map(|d| (d.kind, d.kind.readiness()))
                    .collect::<Vec<_>>(),
            ),
        }
    });
    cx.spawn(async move |this, cx| {
        let ready = check.await;
        this.update(cx, |ws, cx| {
            if let Some(ready) = ready {
                ws.chat.ready = ready;
            }
            if let Some(text) = then {
                ws.status = None;
                ws.ask_agent_checked(text, true, cx);
            }
            cx.notify();
        })
        .ok();
    })
    .detach();
}

impl Workspace {
    /// Shows or hides the Agent panel, where the Samples panel would be.
    pub fn toggle_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_agent = !self.show_agent;
        if self.show_agent {
            // Signed in or installed since it was last asked: it says so now.
            check_agents(None, cx);
            self.show_browser = false;
            self.view = crate::workspace::View::Arrange;
            let focus = self.chat.input.read(cx).focus_handle(cx);
            focus.focus(window, cx);
        }
        cx.notify();
    }

    /// Asks the agent something, starting it on the song if it isn't running.
    pub fn ask_agent(&mut self, text: String, cx: &mut Context<Self>) {
        self.ask_agent_checked(text, false, cx);
    }

    /// Asks, if the agent can work. Not ready when last asked, it's asked again first (it may
    /// have been installed or signed in since); still not, it says what to do.
    pub fn ask_agent_checked(&mut self, text: String, checked: bool, cx: &mut Context<Self>) {
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
        if problem.is_some() && !checked {
            self.status = Some(format!("Checking {}…", def.name));
            check_agents(Some(text), cx);
            cx.notify();
            return;
        }
        if let Some(problem) = problem {
            self.chat.asked(text);
            self.chat.end(0.0, false, Some(problem));
            self.agent_unavailable(cx);
            cx.notify();
            return;
        }
        // Another model or effort picked since it started: Claude Code starts again on them,
        // carrying on the conversation; Codex takes them with its next question.
        let mut resume = None;
        if let Some(a) = &mut self.chat.agent
            && a.tuning != self.chat.tuning
        {
            match self.chat.kind {
                agent::Kind::ClaudeCode => {
                    resume = a.session();
                    self.chat.agent = None;
                }
                _ => a.tuning = self.chat.tuning.clone(),
            }
        }
        // Carrying one on from before: as the agent knows it, or else it's told what was said.
        let mut told = None;
        if self.chat.agent.is_none() {
            if resume.is_none() {
                resume = self.chat.session.clone();
                self.chat.carrying = resume.is_some().then(|| text.clone());
            }
            if resume.is_none() && self.chat.has_questions() {
                told = Some(self.chat.recap());
            }
            // A new conversation hears the palette afresh.
            if resume.is_none() {
                self.chat.palette_heard = None;
            }
            let tuning = self.chat.tuning.clone();
            #[cfg(test)]
            if let Some(c) = &self.chat.command {
                // SAFETY: only the test with the stand-in asks an agent while it's set.
                unsafe { std::env::set_var("PINA_CLAUDE", c) };
            }
            match Agent::start(self.chat.kind, &self.session.dir, tuning, resume) {
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
                    self.agent_unavailable(cx);
                    cx.notify();
                    return;
                }
            }
        }
        // What's selected goes along, so "make this hit harder" knows what this is; and the
        // sounds picked for it, when they've changed.
        let mut full = match self.selection_note().filter(|_| self.chat.use_selection) {
            Some((_, said)) => format!("(Selected in tempo: {said}.)\n\n{text}"),
            None => text.clone(),
        };
        if let Some(sounds) = self.palette_note() {
            full = format!("{sounds}\n\n{full}");
        }
        let given = self.midi_note();
        if let Some((note, _)) = &given {
            full = format!("{note}\n\n{full}");
        }
        if let Some(said) = told {
            full = format!(
                "(You and the person have talked about this song before. What was said:\n\n{said}\n\nCarry on from there.)\n\n{full}"
            );
        }
        if !self.chat.busy {
            self.chat.before = Some(self.session.project.clone());
        }
        let sent = self.chat.agent.as_mut().map(|a| a.ask(&full));
        self.chat.asked(text);
        for path in given.into_iter().flat_map(|g| g.1) {
            self.chat.items.push(Item::Midi { path, given: true });
        }
        if let Some(Err(e)) = sent {
            self.chat.agent = None;
            if self.carry_on_without_it(cx) {
                return;
            }
            self.chat.end(0.0, false, Some(e));
        }
        self.keep_chat();
        if keep {
            self.chat.scroll.scroll_to_bottom();
        }
        cx.notify();
    }

    /// Takes in an event from Claude, keeping the end of the transcript in view.
    pub(crate) fn heard_claude(&mut self, e: Event, cx: &mut Context<Self>) {
        let keep = self.chat.at_end();
        let over = matches!(e, Event::Done { .. });
        match &e {
            Event::Done { error: Some(_), .. } if self.carry_on_without_it(cx) => return,
            Event::Text { .. } | Event::ToolStart { .. } | Event::Tool { .. } => {
                self.chat.carrying = None;
            }
            _ => {}
        }
        let wrote = self.heard_midi(&e);
        if over {
            self.note_changes();
        }
        self.chat.apply(e);
        if let Some(path) = wrote {
            self.chat.items.push(Item::Midi { path, given: false });
        }
        if over {
            self.agent_turn_over(cx);
            self.keep_chat();
        }
        if keep {
            self.chat.scroll.scroll_to_bottom();
        }
        cx.notify();
    }

    /// Carrying a conversation on, the agent turned out not to remember it (the song's moved, or
    /// it's too old): the question goes again to one told what was said. Whether that's how it
    /// was. The agent says so only by failing, and any other failure comes round again and
    /// is shown then.
    fn carry_on_without_it(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(text) = self.chat.carrying.take() else {
            return false;
        };
        let asked = self
            .chat
            .items
            .iter()
            .rposition(|i| matches!(i, Item::You(_)));
        self.chat.items.truncate(asked.unwrap_or(0));
        self.chat.run += 1;
        self.chat.agent = None;
        self.chat.session = None;
        self.chat.busy = false;
        self.ask_agent(text, cx);
        true
    }

    /// An event from a `pina ask` in a terminal: the panel opens to show it.
    pub fn heard_agent(&mut self, e: Event, cx: &mut Context<Self>) {
        if let Event::Asked { .. } = e {
            self.chat.remote = true;
            self.show_agent = true;
            self.show_browser = false;
        }
        self.heard_claude(e, cx);
    }

    /// The last question, asked again after it went wrong.
    pub(crate) fn retry_agent(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.chat.last_question() else {
            return;
        };
        // A conversation that never got going starts again, so it hears Pina Studio's
        // instructions with the question.
        if self
            .chat
            .agent
            .as_ref()
            .is_some_and(|a| a.session().is_none())
        {
            self.chat.agent = None;
        }
        // What went with it goes with it again.
        self.chat.attached = self.chat.last_given();
        self.ask_agent(text, cx);
    }

    /// The last question asked of the other agent instead, in a new conversation with it.
    pub(crate) fn ask_other_agent(&mut self, cx: &mut Context<Self>) {
        let (Some(text), Some(other)) = (self.chat.last_question(), self.chat.other_kind()) else {
            return;
        };
        let given = self.chat.last_given();
        self.pick_agent(other, cx);
        self.chat.attached = given;
        self.ask_agent(text, cx);
    }

    pub(crate) fn stop_agent(&mut self, cx: &mut Context<Self>) {
        if let Some(a) = self.chat.agent.as_mut() {
            a.interrupt();
        }
        cx.notify();
    }

    /// A new conversation: the one showing is kept, to come back to, and ends if it's running.
    pub(crate) fn new_chat(&mut self, cx: &mut Context<Self>) {
        self.keep_chat();
        self.clear_chat();
        self.chat.page = ChatPage::Chat;
        cx.notify();
    }

    /// Leaves a new conversation showing.
    pub(crate) fn clear_chat(&mut self) {
        self.chat.forget();
    }
}
