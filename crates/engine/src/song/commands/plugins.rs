//! Commands on plugins: parameters and presets by name, and sidechains.

use super::*;

/// A plugin parameter, by its id or its name.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum ParamRef {
    Id(u32),
    Name(String),
}

/// An automation point: its value, or its value as the plugin shows it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PointIn {
    pub tick: u64,
    #[serde(default)]
    pub value: Option<f32>,
    #[serde(default)]
    pub text: Option<String>,
    /// How the line bends on to the next point, -1 to 1.
    #[serde(default)]
    pub curve: Option<f32>,
    /// Holds its value up to the next point.
    #[serde(default)]
    pub step: Option<bool>,
    /// Eases out and into the next point: an S.
    #[serde(default)]
    pub ease: Option<bool>,
}

/// A preset, by name or where it comes from.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum PresetChoice {
    Name(String),
    Found(Preset),
    From(PresetSource),
}

/// What `resolve` asks of the plugins a command names: one, loaded.
pub trait PluginSource {
    fn plugin(
        &mut self,
        project: &Project,
        channel: ChannelRef,
        which: Which,
    ) -> Result<&dyn PluginApi, String>;
}

/// Puts what a command says in words into what Pina Studio keeps: a parameter's name into its id, a
/// value as the plugin shows it ("850 Hz") into its value, a preset's name into the preset.
/// Asks `plugins` for a plugin only when the command needs one.
pub fn resolve(
    p: &Project,
    cmd: Command,
    plugins: &mut dyn PluginSource,
) -> Result<Command, String> {
    Ok(match cmd {
        Command::SetPluginParam {
            channel,
            plugin,
            param,
            value,
            text,
        } if matches!(param, ParamRef::Name(_)) || text.is_some() => {
            let found = plugins.plugin(p, channel, plugin)?;
            let info = find_param(&found.params(), &param)?;
            let value = match text {
                Some(t) => Some(value_from_text(found, &info, &t)?),
                None => value,
            };
            Command::SetPluginParam {
                channel,
                plugin,
                param: ParamRef::Id(info.id),
                value,
                text: None,
            }
        }
        Command::SetAutomation {
            track,
            param,
            send,
            plugin: Some(which),
            id: Some(id),
            name,
            modulator,
            setting,
            points,
        } if matches!(id, ParamRef::Name(_)) || points.iter().any(|p| p.text.is_some()) => {
            let found = plugins.plugin(p, ChannelRef::Track(track), which)?;
            let info = find_param(&found.params(), &id)?;
            let points = points
                .into_iter()
                .map(|pt| {
                    Ok(PointIn {
                        value: match &pt.text {
                            Some(t) => Some(value_from_text(found, &info, t)? as f32),
                            None => pt.value,
                        },
                        text: None,
                        ..pt
                    })
                })
                .collect::<Result<_, String>>()?;
            Command::SetAutomation {
                track,
                param,
                send,
                plugin: Some(which),
                id: Some(ParamRef::Id(info.id)),
                name: name.or(Some(info.name)),
                modulator,
                setting,
                points,
            }
        }
        Command::SetPreset {
            channel,
            plugin,
            preset: Some(choice),
        } if !matches!(choice, PresetChoice::Found(_)) => {
            let found = plugins.plugin(p, channel, plugin)?;
            let presets = found.presets();
            let preset = match choice {
                PresetChoice::Name(name) => find_named(&presets, &name, |p| &p.name, "preset")?,
                PresetChoice::From(source) => presets
                    .into_iter()
                    .find(|p| p.source == source)
                    .unwrap_or_else(|| Preset {
                        name: match &source {
                            PresetSource::File { file } => file
                                .file_stem()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                            PresetSource::Program { program, .. } => format!("Program {program}"),
                        },
                        source,
                    }),
                PresetChoice::Found(p) => p,
            };
            Command::SetPreset {
                channel,
                plugin,
                preset: Some(PresetChoice::Found(preset)),
            }
        }
        Command::SetSidechain {
            channel,
            insert,
            from: Some(from),
        } => {
            let found = plugins.plugin(p, channel, Which::Insert(insert))?;
            if !found.sidechain() {
                return Err(format!("{} has no sidechain input", found.name()));
            }
            Command::SetSidechain {
                channel,
                insert,
                from: Some(from),
            }
        }
        other => other,
    })
}

/// The plugin on a channel, if there's one there.
pub fn plugin_ref(p: &Project, channel: ChannelRef, which: Which) -> Option<&PluginRef> {
    match (channel, which) {
        (ChannelRef::Track(i), w) => p.tracks.get(i)?.plugin(w),
        (ChannelRef::Aux(i), Which::Insert(n)) => p.auxes.get(i)?.inserts.get(n),
        (ChannelRef::Master, Which::Insert(n)) => p.master_inserts.get(n),
        _ => None,
    }
}

/// A parameter by id, or by name: the same name ignoring case, else the only one whose name has
/// it in.
pub fn find_param(params: &[PluginParam], which: &ParamRef) -> Result<PluginParam, String> {
    match which {
        ParamRef::Id(id) => params
            .iter()
            .find(|p| p.id == *id)
            .cloned()
            .ok_or_else(|| format!("no parameter {id}")),
        ParamRef::Name(name) => find_named(params, name, |p| &p.name, "parameter"),
    }
}

pub(crate) fn find_named<T: Clone>(
    items: &[T],
    name: &str,
    of: impl Fn(&T) -> &String,
    what: &str,
) -> Result<T, String> {
    let lower = name.trim().to_lowercase();
    if let Some(t) = items.iter().find(|t| of(t).to_lowercase() == lower) {
        return Ok(t.clone());
    }
    let near: Vec<&T> = items
        .iter()
        .filter(|t| of(t).to_lowercase().contains(&lower))
        .collect();
    match near[..] {
        [one] => Ok(one.clone()),
        [] => Err(format!("no {what} called {name:?}")),
        _ => Err(format!(
            "{} {what}s have {name:?} in their names: {}",
            near.len(),
            near.iter()
                .take(8)
                .map(|t| format!("{:?}", of(t)))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// The plugin on a channel: a track's instrument or insert, an aux's or the master's insert.
pub(crate) fn plugin_mut(
    p: &mut Project,
    channel: ChannelRef,
    which: Which,
) -> Result<&mut PluginRef, String> {
    match channel {
        ChannelRef::Track(i) => track(p, i)?.plugin_mut(which),
        ChannelRef::Aux(i) => match which {
            Which::Insert(n) => aux(p, i)?.inserts.get_mut(n),
            Which::Instrument => None,
        },
        ChannelRef::Master => match which {
            Which::Insert(n) => p.master_inserts.get_mut(n),
            Which::Instrument => None,
        },
    }
    .ok_or_else(|| "no plugin there".to_string())
}

/// The plugins commands.
pub(super) fn run(p: &mut Project, _dir: &Path, cmd: Command) -> Result<Value, String> {
    match cmd {
        Command::SetPluginParam {
            channel,
            plugin,
            param,
            value,
            text,
        } => {
            let ParamRef::Id(param) = param else {
                return Err(format!(
                    "a parameter by name needs the plugin: {UNRESOLVED}"
                ));
            };
            if text.is_some() {
                return Err(format!("a value as text needs the plugin: {UNRESOLVED}"));
            }
            let r = plugin_mut(p, channel, plugin)?;
            match value {
                Some(v) if v.is_finite() => drop(r.params.insert(param, v.clamp(0.0, 1.0))),
                Some(_) => return Err("value must be a number from 0 to 1".into()),
                None => drop(r.params.remove(&param)),
            }
        }
        Command::SetPreset {
            channel,
            plugin,
            preset,
        } => {
            let preset = match preset {
                Some(PresetChoice::Found(preset)) => Some(preset),
                Some(_) => {
                    return Err(format!(
                        "a preset needs the plugin to be found: {UNRESOLVED}"
                    ));
                }
                None => None,
            };
            let r = plugin_mut(p, channel, plugin)?;
            // A preset is a whole new sound: what was set over the old one goes, and the old
            // settings with it.
            if preset.is_some() {
                r.params.clear();
                r.state = None;
            }
            r.preset = preset;
        }
        Command::SetSidechain {
            channel,
            insert,
            from,
        } => {
            let id = match from {
                Some(t) => Some(track(p, t)?.id),
                None => None,
            };
            plugin_mut(p, channel, Which::Insert(insert))?.sidechain = id;
        }
        _ => unreachable!("not one of the plugins commands"),
    }
    Ok(json!({}))
}
