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
        if self.chat.agent.is_none() {
            // A new conversation hears the palette afresh.
            if resume.is_none() {
                self.chat.palette_heard = None;
            }
            let tuning = self.chat.tuning.clone();
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
    pub(crate) fn heard_claude(&mut self, e: Event, cx: &mut Context<Self>) {
        let keep = self.chat.at_end();
        self.chat.apply(e);
        if keep {
            self.chat.scroll.scroll_to_bottom();
        }
        cx.notify();
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

    pub(crate) fn stop_agent(&mut self, cx: &mut Context<Self>) {
        if let Some(a) = self.chat.agent.as_mut() {
            a.interrupt();
        }
        cx.notify();
    }

    /// A new conversation: the one running ends.
    pub(crate) fn new_chat(&mut self, cx: &mut Context<Self>) {
        self.chat.run += 1;
        self.chat.agent = None;
        self.chat.palette_heard = None;
        self.chat.items.clear();
        self.chat.busy = false;
        self.chat.remote = false;
        self.chat.since = None;
        cx.notify();
    }
}
