//! What agents and scripts run: each returns JSON (or text), and goes to the running app when it
//! has the project open, so edits land on screen and can be undone there.

use std::path::Path;

use serde_json::{Value, json};
use tempo_engine::{
    analysis::{self, Analysis},
    commands::{self, ChannelRef, Command, PluginSource},
    engine::{Engine, load_plugin},
    plugin::{Plugin, PluginApi, Processor, named_values},
    project::{Project, Which},
    scan,
    session::Session,
};

use crate::server::{self, Request};

/// Seconds after the last region, for releases and effect tails.
pub const TAIL: f64 = 2.0;

fn json<T: serde::Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

/// The project in `dir`: as the app has it, if it's open there, else as saved.
pub fn project(dir: &Path) -> Result<Project, String> {
    match server::ask(dir, &Request::Project) {
        Some(reply) => {
            let mut p: Project = serde_json::from_value(reply?).map_err(|e| e.to_string())?;
            p.assign_ids();
            Ok(p)
        }
        None => Project::load(dir),
    }
}

pub fn show(dir: &Path) -> Result<Value, String> {
    json(project(dir)?)
}

/// Applies a command given as JSON.
pub fn edit(dir: &Path, command: &str) -> Result<Value, String> {
    let command: Command =
        serde_json::from_str(command).map_err(|e| format!("not a command: {e}"))?;
    edit_with(dir, command)
}

/// Applies a command: in the app if it has the project open, else to the folder on disk (making
/// the project if there's none).
pub fn edit_with(dir: &Path, command: Command) -> Result<Value, String> {
    let request = Request::Edit { command };
    if let Some(reply) = server::ask(dir, &request) {
        return reply;
    }
    let Request::Edit { command } = request else {
        unreachable!()
    };
    let mut session = Session::open(dir)?;
    let mut plugins = Offline { dir, loaded: None };
    let command = commands::resolve(&session.project, command, &mut plugins)?;
    let result = commands::apply(&mut session.project, dir, command)?;
    session.save()?;
    Ok(result)
}

/// One plugin of the project saved in `dir`, loaded as the project sets it.
fn load_one(
    dir: &Path,
    p: &Project,
    channel: ChannelRef,
    which: Which,
) -> Result<(Plugin, Processor), String> {
    let r = commands::plugin_ref(p, channel, which).ok_or("no plugin there")?;
    load_plugin(r, dir, p.sample_rate as f64)
}

/// Plugins loaded from the folder, as `resolve` asks for them.
struct Offline<'a> {
    dir: &'a Path,
    loaded: Option<(Plugin, Processor)>,
}

impl PluginSource for Offline<'_> {
    fn plugin(
        &mut self,
        project: &Project,
        channel: ChannelRef,
        which: Which,
    ) -> Result<&dyn PluginApi, String> {
        let loaded = load_one(self.dir, project, channel, which)?;
        Ok(&*self.loaded.insert(loaded).0)
    }
}

/// Something only the running app can do: transport, save, undo.
pub fn app(dir: &Path, request: Request) -> Result<Value, String> {
    server::ask(dir, &request).unwrap_or_else(|| {
        Err(format!(
            "tempo doesn't have {} open: open it with `tempo {0}`",
            dir.display()
        ))
    })
}

/// How long a project plays, tail included, in samples.
fn length(p: &Project) -> u64 {
    let rate = p.sample_rate as f64;
    p.samples(p.end(), rate) + (TAIL * rate) as u64
}

fn measure(p: &Project, dir: &Path) -> Result<Analysis, String> {
    let rate = p.sample_rate as f64;
    let (mut engine, _plugins) = Engine::load(p, dir, rate)?;
    let [l, r] = engine.render(length(p));
    Ok(analysis::analyze(&l, &r, rate))
}

/// Measures a WAV file, or a project: the mix, then each track soloed.
pub fn analyze(path: &Path) -> Result<Value, String> {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
    {
        let (l, r, rate) = analysis::read_wav(path)?;
        return json(analysis::analyze(&l, &r, rate));
    }
    let p = project(path)?;
    let mix = measure(&p, path)?;
    let mut tracks = Vec::new();
    for (i, t) in p.tracks.iter().enumerate() {
        let mut alone = p.clone();
        alone
            .tracks
            .iter_mut()
            .enumerate()
            .for_each(|(j, t)| t.solo = j == i);
        tracks.push(json!({ "track": i, "name": t.name, "analysis": measure(&alone, path)? }));
    }
    Ok(json!({ "mix": mix, "tracks": tracks }))
}

/// Bounces the project (as the app has it, if open) to a WAV file.
pub fn render(dir: &Path, out: &Path) -> Result<Value, String> {
    let p = project(dir)?;
    bounce(&p, dir, out)?;
    let rate = p.sample_rate as f64;
    let seconds = length(&p) as f64 / rate;
    Ok(json!({ "file": out, "seconds": seconds, "sample_rate": p.sample_rate }))
}

/// Bounces a project, whose files are in `dir`, to a WAV file.
pub fn bounce(p: &Project, dir: &Path, out: &Path) -> Result<(), String> {
    let rate = p.sample_rate as f64;
    let (mut engine, _plugins) = Engine::load(p, dir, rate)?;
    engine.bounce(out, length(p))
}

/// A plugin's parameters as JSON: id, name, value (0 to 1) and how the plugin shows it, its
/// default, unit and steps, and for a choice of a few, what each step is.
pub fn params_json(plugin: &dyn PluginApi) -> Value {
    let list: Vec<Value> = plugin
        .params()
        .into_iter()
        .map(|p| {
            let mut v = json!({
                "id": p.id,
                "name": p.name,
                "value": p.value,
                "text": plugin.param_text(p.id, p.value),
                "default": p.default,
            });
            let map = v.as_object_mut().unwrap();
            if !p.units.is_empty() {
                map.insert("units".into(), p.units.clone().into());
            }
            if p.steps > 0 {
                map.insert("steps".into(), p.steps.into());
            }
            // A choice: what each value is called, and a value that picks it.
            if let Some(named) = named_values(plugin, &p) {
                let options: Vec<Value> = named
                    .into_iter()
                    .map(|(name, value)| json!({ "text": name, "value": value }))
                    .collect();
                map.insert("options".into(), options.into());
            }
            v
        })
        .collect();
    Value::Array(list)
}

/// A plugin's presets as JSON: name, and its program or file.
pub fn presets_json(plugin: &dyn PluginApi) -> Value {
    json!(plugin.presets())
}

/// A plugin's parameters, for automating them or setting them: from the app if it has the project
/// open, else by loading the plugin.
pub fn params(dir: &Path, channel: ChannelRef, plugin: Which) -> Result<Value, String> {
    if let Some(reply) = server::ask(dir, &Request::Params { channel, plugin }) {
        return reply;
    }
    let p = Project::load(dir)?;
    let (loaded, _processor) = load_one(dir, &p, channel, plugin)?;
    Ok(params_json(&*loaded))
}

/// What a plugin can load: its programs and preset files. From the app if it has the project
/// open, else by loading the plugin.
pub fn presets(dir: &Path, channel: ChannelRef, plugin: Which) -> Result<Value, String> {
    if let Some(reply) = server::ask(dir, &Request::Presets { channel, plugin }) {
        return reply;
    }
    let p = Project::load(dir)?;
    let (loaded, _processor) = load_one(dir, &p, channel, plugin)?;
    Ok(presets_json(&*loaded))
}

/// Saves a plugin's settings as a preset file of its format, where its presets live.
pub fn save_preset(
    dir: &Path,
    channel: ChannelRef,
    plugin: Which,
    name: &str,
) -> Result<Value, String> {
    let request = Request::SavePreset {
        channel,
        plugin,
        name: name.to_string(),
    };
    if let Some(reply) = server::ask(dir, &request) {
        return reply;
    }
    let p = Project::load(dir)?;
    let (loaded, _processor) = load_one(dir, &p, channel, plugin)?;
    Ok(json!({ "file": loaded.save_preset(name)? }))
}

/// The plugins installed, scanning what's new.
pub fn plugins() -> Result<Value, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    json(scan::scan(&exe, &scan::everything(), &scan::cache_file()))
}
