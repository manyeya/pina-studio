//! Each agent's models and efforts, and the ones picked, kept between runs.

use super::*;

/// The agent chosen last, in `~/Library/Application Support/Pina Studio/agent`; Claude Code at first.
/// How an agent runs: the model it uses and how hard it thinks (its effort). None leaves either
/// to the agent's own settings.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Tuning {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

/// A model to pick: its id as the agent takes it, its name, and the effort levels it takes.
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub efforts: Vec<String>,
}

/// A model's name for people: its own, or (not one listed) its id.
pub fn model_name(kind: Kind, id: &str) -> String {
    kind.models()
        .into_iter()
        .find(|m| m.id == id)
        .map_or_else(|| id.to_string(), |m| m.name)
}

/// An effort level's name for people.
pub fn effort_name(effort: &str) -> String {
    match effort {
        "xhigh" => "Extra high".into(),
        "max" => "Max".into(),
        e => {
            let mut c = e.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        }
    }
}

pub(crate) fn codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".codex")
        })
}

/// Codex's models, from the list it keeps (`models_cache.json`): those it shows, in its order.
pub(crate) fn codex_models() -> Vec<Model> {
    let Ok(text) = std::fs::read_to_string(codex_home().join("models_cache.json")) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let mut models: Vec<(i64, Model)> = v["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["visibility"] == "list")
        .filter_map(|m| {
            let id = m["slug"].as_str()?.to_string();
            let efforts = m["supported_reasoning_levels"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|l| l["effort"].as_str().map(String::from))
                .collect();
            Some((
                m["priority"].as_i64().unwrap_or(i64::MAX),
                Model {
                    name: m["display_name"].as_str().unwrap_or(&id).to_string(),
                    id,
                    efforts,
                },
            ))
        })
        .collect();
    models.sort_by_key(|m| m.0);
    models.into_iter().map(|m| m.1).collect()
}

/// The model and effort last picked for an agent, kept for every song.
pub fn tuning(kind: Kind) -> Tuning {
    let all: serde_json::Map<String, Value> = std::fs::read_to_string(tuning_file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    all.get(kind.def().id)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

pub fn tune(kind: Kind, tuning: &Tuning) {
    let file = tuning_file();
    let mut all: serde_json::Map<String, Value> = std::fs::read_to_string(&file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    all.insert(kind.def().id.into(), json!(tuning));
    let _ = std::fs::create_dir_all(file.parent().unwrap());
    let _ = std::fs::write(file, Value::Object(all).to_string());
}

pub(crate) fn tuning_file() -> PathBuf {
    match cfg!(test) {
        // Tests never touch the person's.
        true => std::env::temp_dir().join(format!(
            "pina-test-tuning-{}-{:?}.json",
            std::process::id(),
            thread::current().id()
        )),
        false => choice_file().with_file_name("agent-tuning.json"),
    }
}

pub fn chosen() -> Kind {
    std::fs::read_to_string(choice_file())
        .ok()
        .and_then(|s| Kind::from_id(&s))
        .unwrap_or_default()
}

pub fn choose(kind: Kind) {
    let file = choice_file();
    let _ = std::fs::create_dir_all(file.parent().unwrap());
    let _ = std::fs::write(file, kind.def().id);
}

pub(crate) fn choice_file() -> PathBuf {
    pina_engine::paths::support().join("agent")
}

impl Kind {
    /// The models it offers: Claude Code's by their aliases (each the latest of its line), Codex's
    /// as its own model list has them (the ones it lists, in its order).
    pub fn models(self) -> Vec<Model> {
        match self {
            Kind::ClaudeCode => [
                ("fable", "Fable"),
                ("opus", "Opus"),
                ("sonnet", "Sonnet"),
                ("haiku", "Haiku"),
            ]
            .into_iter()
            .map(|(id, name)| Model {
                id: id.into(),
                name: name.into(),
                efforts: CLAUDE_EFFORTS.map(String::from).to_vec(),
            })
            .collect(),
            Kind::Codex => codex_models(),
        }
    }

    /// The effort levels it takes with a model (or, with none picked, its default's).
    pub fn efforts(self, model: Option<&str>) -> Vec<String> {
        let models = self.models();
        let id = model.map(str::to_string).or_else(|| self.default_model());
        match id.and_then(|id| models.into_iter().find(|m| m.id == id)) {
            Some(m) => m.efforts,
            None => match self {
                Kind::ClaudeCode => CLAUDE_EFFORTS.map(String::from).to_vec(),
                Kind::Codex => ["low", "medium", "high"].map(String::from).to_vec(),
            },
        }
    }

    /// The model it uses when none is picked, if its settings say (Codex's config does).
    pub fn default_model(self) -> Option<String> {
        match self {
            Kind::ClaudeCode => None,
            Kind::Codex => {
                let config = std::fs::read_to_string(codex_home().join("config.toml")).ok()?;
                // The top level's `model = "…"`, before any [table].
                config
                    .lines()
                    .take_while(|l| !l.trim_start().starts_with('['))
                    .find_map(|l| {
                        let (k, v) = l.split_once('=')?;
                        (k.trim() == "model").then(|| v.trim().trim_matches('"').to_string())
                    })
            }
        }
    }
}
