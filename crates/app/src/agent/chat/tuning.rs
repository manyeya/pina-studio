//! Which agent, model and effort answer, picked from the panel's menus.

use super::*;

/// "claude-opus-5-5" as "Opus 5.5".
pub(crate) fn model_name(id: &str) -> String {
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

impl Workspace {
    /// Works with another agent from now on, in a new conversation.
    pub fn pick_agent(&mut self, kind: Kind, cx: &mut Context<Self>) {
        if kind != self.chat.kind {
            self.chat.kind = kind;
            self.chat.model = None;
            self.chat.tuning = agent::tuning(kind);
            agent::choose(kind);
            self.new_chat(cx);
        }
        cx.notify();
    }

    /// A model picked for the agent, from the next question on; an effort the new model doesn't
    /// take goes back to its own.
    pub fn pick_model(&mut self, model: Option<String>, cx: &mut Context<Self>) {
        let kind = self.chat.kind;
        let t = &mut self.chat.tuning;
        t.model = model;
        if t.effort
            .as_ref()
            .is_some_and(|e| !kind.efforts(t.model.as_deref()).contains(e))
        {
            t.effort = None;
        }
        agent::tune(kind, t);
        self.status = Some(format!("{} from the next question on", self.tuning_label()));
        cx.notify();
    }

    pub fn pick_effort(&mut self, effort: Option<String>, cx: &mut Context<Self>) {
        self.chat.tuning.effort = effort;
        agent::tune(self.chat.kind, &self.chat.tuning);
        self.status = Some(format!("{} from the next question on", self.tuning_label()));
        cx.notify();
    }

    /// The model and effort, as the chip says them: those picked, else the model it last ran
    /// as (its own), and "Default" before it has.
    pub fn tuning_label(&self) -> String {
        let (kind, t) = (self.chat.kind, &self.chat.tuning);
        let model = match &t.model {
            Some(m) => agent::model_name(kind, m),
            None => kind
                .default_model()
                .map(|m| agent::model_name(kind, &m))
                .or_else(|| {
                    self.chat
                        .model
                        .as_deref()
                        .filter(|m| !m.is_empty())
                        .map(model_name)
                })
                .unwrap_or_else(|| "Default model".into()),
        };
        match &t.effort {
            Some(e) => format!("{model} · {}", agent::effort_name(e)),
            None => model,
        }
    }

    /// The models to pick from, and the efforts the picked one takes.
    pub fn tuning_menu(&self, at: gpui::Point<gpui::Pixels>) -> Menu {
        let (kind, t) = (self.chat.kind, &self.chat.tuning);
        let tick = |on: bool| if on { "✓ " } else { "" };
        let own = match kind.default_model() {
            Some(m) => format!("Its own ({})", agent::model_name(kind, &m)),
            None => "Its own".into(),
        };
        let mut items = vec![
            ("Model".to_string(), MenuAction::Nothing),
            (
                format!("{}{own}", tick(t.model.is_none())),
                MenuAction::AgentModel(None),
            ),
        ];
        items.extend(kind.models().into_iter().map(|m| {
            (
                format!(
                    "{}{}",
                    tick(t.model.as_deref() == Some(m.id.as_str())),
                    m.name
                ),
                MenuAction::AgentModel(Some(m.id)),
            )
        }));
        items.push((String::new(), MenuAction::Nothing));
        items.push(("Effort".to_string(), MenuAction::Nothing));
        items.push((
            format!("{}Its own", tick(t.effort.is_none())),
            MenuAction::AgentEffort(None),
        ));
        items.extend(kind.efforts(t.model.as_deref()).into_iter().map(|e| {
            (
                format!(
                    "{}{}",
                    tick(t.effort.as_deref() == Some(e.as_str())),
                    agent::effort_name(&e)
                ),
                MenuAction::AgentEffort(Some(e)),
            )
        }));
        Menu {
            at,
            items,
            ..Default::default()
        }
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
}
