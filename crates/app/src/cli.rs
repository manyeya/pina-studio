//! What agents and scripts run: each returns JSON (or text), and goes to the running app when it
//! has the project open, so edits land on screen and can be undone there.

use std::path::{Path, PathBuf};

use pina_engine::{
    analysis,
    commands::{self, ChannelRef, Command, PluginSource},
    engine::{Engine, load_plugin},
    plugin::{Plugin, PluginApi, Processor, named_values},
    project::{Content, Project, Region, Track, Which},
    scan,
    session::Session,
};
use serde_json::{Value, json};

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
            p.resolve_sidechains();
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
            "Pina Studio doesn't have {} open: open it with `pina {0}`",
            dir.display()
        ))
    })
}

/// An audio file (WAV, AIFF, FLAC or MP3) as left and right, and its rate.
pub fn audio(path: &Path) -> Result<(Vec<f32>, Vec<f32>, f64), String> {
    let (mut channels, rate) = pina_engine::engine::read_audio(path)?;
    let l = channels.remove(0);
    let r = channels.pop().unwrap_or_else(|| l.clone());
    Ok((l, r, rate))
}

/// How long a project plays, tail included, in samples.
fn length(p: &Project) -> u64 {
    let rate = p.sample_rate as f64;
    p.samples(p.end(), rate) + (TAIL * rate) as u64
}

/// `p` rendered for `samples`.
fn mixed(p: &Project, dir: &Path, samples: u64) -> Result<[Vec<f32>; 2], String> {
    let (mut engine, _plugins) = Engine::load(p, dir, p.sample_rate as f64)?;
    Ok(engine.render(samples))
}

/// What a render of a project measures: the mix, each marked section in it, and (soloed) each
/// track alone, by index.
pub struct Report {
    pub mix: analysis::Analysis,
    pub sections: Vec<(String, u64, analysis::Analysis)>,
    pub tracks: Vec<analysis::Analysis>,
}

/// Bars `first..=last` (from 1) as ticks.
pub fn bars(p: &Project, first: u64, last: u64) -> Result<(u64, u64), String> {
    if first == 0 || last < first {
        return Err("bars run from 1, the last no earlier than the first".into());
    }
    Ok(((first - 1) * p.bar(), last * p.bar()))
}

/// `p` rendered from tick `from` to `to` (the whole song and its tail by default), a bar played
/// first and dropped so effects are going.
pub fn rendered(
    p: &Project,
    dir: &Path,
    range: Option<(u64, u64)>,
) -> Result<[Vec<f32>; 2], String> {
    let Some((from, to)) = range else {
        return mixed(p, dir, length(p));
    };
    let rate = p.sample_rate as f64;
    let lead = p.samples(from.min(p.bar()), rate);
    let start = p.samples(from, rate) - lead;
    let frames = p.samples(to, rate) - start;
    let (mut engine, _plugins) = Engine::load(p, dir, rate)?;
    let [mut l, mut r] = engine.render_from(start, frames);
    l.drain(..lead as usize);
    r.drain(..lead as usize);
    Ok([l, r])
}

/// `p` with only track `i` playing, through the auxes and master it plays into. The others stay
/// as silent placeholders (their regions empty, so the song ends and loops run as before), so
/// their plugins and audio don't run for nothing.
pub fn alone(p: &Project, i: usize) -> Project {
    let mut alone = p.clone();
    for (j, other) in alone.tracks.iter_mut().enumerate() {
        if j == i {
            other.solo = false;
            other.mute = false;
            continue;
        }
        *other = Track {
            id: other.id,
            mute: true,
            regions: other
                .regions
                .iter()
                .map(|r| Region {
                    content: Content::default(),
                    ..r.clone()
                })
                .collect(),
            ..Track::default()
        };
    }
    alone
}

/// Measures a project (as the app has it, if open) over `range` (ticks; all of it by default):
/// the mix, its sections, and with `solo` each track alone, through the auxes and master it
/// plays into.
pub fn report(
    p: &Project,
    dir: &Path,
    range: Option<(u64, u64)>,
    solo: bool,
) -> Result<Report, String> {
    let rate = p.sample_rate as f64;
    let [l, r] = rendered(p, dir, range)?;
    let mix = analysis::analyze(&l, &r, rate);
    let origin = range.map_or(0, |r| r.0);
    let at = |tick: u64| (p.samples(tick.saturating_sub(origin), rate) as usize).min(l.len());
    let sections = p
        .markers
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            let next = p.markers.get(i + 1).map(|n| n.tick);
            let (a, b) = (at(m.tick.max(origin)), next.map_or(l.len(), at));
            (b > a && range.is_none_or(|(_, to)| m.tick < to)).then(|| {
                (
                    m.name.clone(),
                    m.tick,
                    analysis::analyze(&l[a..b], &r[a..b], rate),
                )
            })
        })
        .collect();
    let mut tracks = Vec::new();
    for i in (0..p.tracks.len()).filter(|_| solo) {
        let [l, r] = rendered(&alone(p, i), dir, range)?;
        tracks.push(analysis::analyze(&l, &r, rate));
    }
    Ok(Report {
        mix,
        sections,
        tracks,
    })
}

/// Measures an audio file, or a project: the mix, each section (marker to marker), then each
/// track soloed; for a project, over bars `first..=last` if given.
pub fn analyze(path: &Path, bars_given: Option<(u64, u64)>) -> Result<Value, String> {
    if path.is_file() {
        let (l, r, rate) = audio(path)?;
        return json(analysis::analyze(&l, &r, rate));
    }
    let p = project(path)?;
    let range = bars_given.map(|(a, b)| bars(&p, a, b)).transpose()?;
    let report = report(&p, path, range, true)?;
    let sections: Vec<Value> = report
        .sections
        .iter()
        .map(|(name, tick, a)| json!({ "name": name, "tick": tick, "analysis": a }))
        .collect();
    let tracks: Vec<Value> = report
        .tracks
        .iter()
        .enumerate()
        .map(|(i, a)| json!({ "track": i, "name": p.tracks[i].name, "analysis": a }))
        .collect();
    let mut out = json!({ "mix": report.mix, "sections": sections, "tracks": tracks });
    if let Some((a, b)) = bars_given {
        out["bars"] = json!(format!("{a}-{b}"));
    }
    Ok(out)
}

/// Bounces the project (as the app has it, if open) to a WAV file.
pub fn render(dir: &Path, out: &Path, bars_given: Option<(u64, u64)>) -> Result<Value, String> {
    let p = project(dir)?;
    if let Some((a, b)) = bars_given {
        let range = bars(&p, a, b)?;
        let [l, r] = rendered(&p, dir, Some(range))?;
        analysis::write_wav(out, &l, &r, p.sample_rate)?;
        let seconds = l.len() as f64 / p.sample_rate as f64;
        return Ok(
            json!({ "file": out, "seconds": seconds, "bars": format!("{a}-{b}"), "sample_rate": p.sample_rate }),
        );
    }
    bounce(&p, dir, out)?;
    let rate = p.sample_rate as f64;
    let seconds = length(&p) as f64 / rate;
    Ok(json!({ "file": out, "seconds": seconds, "sample_rate": p.sample_rate }))
}

/// Bounces a project (as the app has it, if open) as `spec` says, for `pina bounce` and agents:
/// `what` ("mix", "stems" or "sections"), `tracks` (stems: indices; all with regions by default),
/// `formats` (wav, aiff, flac, mp3, aac, alac, midi), `bits` (16, 24, 32), `rate`, `lufs` or `peak`
/// (dBFS) to level to (`ceiling`, -1 by default, under a loudness), `tail` (true), `master` (stems
/// through the master), `from_bar`/`to_bar`, `out` (a folder: the song's Bounces by default),
/// `name` (the song's by default), `kbps` (MP3 and AAC).
pub fn bounce_spec(dir: &Path, spec: &Value) -> Result<Value, String> {
    use pina_engine::bounce::{self, Bits, Format, Job, Level, Options};
    let p = project(dir)?;
    let num = |k: &str| spec.get(k).and_then(Value::as_f64);
    let kbps = num("kbps").map(|k| k as u32);
    let formats: Vec<Format> = match spec.get("formats").and_then(Value::as_array) {
        Some(list) => list
            .iter()
            .filter_map(Value::as_str)
            .map(|f| match f.to_lowercase().as_str() {
                "wav" => Ok(Format::Wav),
                "aiff" | "aif" => Ok(Format::Aiff),
                "flac" => Ok(Format::Flac),
                "mp3" => Ok(Format::Mp3 {
                    kbps: kbps.unwrap_or(320),
                }),
                "aac" | "m4a" => Ok(Format::Aac {
                    kbps: kbps.unwrap_or(256),
                }),
                "alac" => Ok(Format::Alac),
                "midi" | "mid" => Ok(Format::Midi),
                other => Err(format!(
                    "{other} isn't a format: wav, aiff, flac, mp3, aac, alac or midi"
                )),
            })
            .collect::<Result<_, _>>()?,
        None => vec![Format::Wav],
    };
    let bits = match num("bits").map(|b| b as u32) {
        None | Some(24) => Bits::I24,
        Some(16) => Bits::I16,
        Some(32) => Bits::F32,
        Some(b) => return Err(format!("{b} bits: 16, 24 or 32")),
    };
    let ceiling = num("ceiling").unwrap_or(-1.0) as f32;
    let level = match (num("lufs"), num("peak")) {
        (Some(l), _) => Level::Loudness {
            lufs: l as f32,
            ceiling,
        },
        (None, Some(db)) => Level::Peak { db: db as f32 },
        _ => Level::Off,
    };
    let opts = Options {
        formats,
        bits,
        rate: num("rate").map_or(p.sample_rate, |r| r as u32),
        level,
        tail: spec.get("tail").and_then(Value::as_bool).unwrap_or(true),
        dither: true,
    };
    let bar = |k: &str| spec.get(k).and_then(Value::as_u64);
    let range = match (bar("from_bar"), bar("to_bar")) {
        (None, None) => None,
        (a, b) => Some(bars(
            &p,
            a.unwrap_or(1),
            b.unwrap_or_else(|| p.end().div_ceil(p.bar().max(1))),
        )?),
    };
    let song = dir
        .file_name()
        .map_or("Bounce".into(), |n| n.to_string_lossy().into_owned());
    let base = bounce::file_safe(spec.get("name").and_then(Value::as_str).unwrap_or(&song));
    let master = spec.get("master").and_then(Value::as_bool).unwrap_or(false);
    let jobs: Vec<Job> = match spec.get("what").and_then(Value::as_str).unwrap_or("mix") {
        "mix" => vec![Job {
            name: base.clone(),
            track: None,
            range,
            master: true,
        }],
        "stems" => {
            let tracks: Vec<usize> = match spec.get("tracks").and_then(Value::as_array) {
                Some(t) => t
                    .iter()
                    .filter_map(Value::as_u64)
                    .map(|t| t as usize)
                    .collect(),
                None => bounce::tracks_with_regions(&p),
            };
            tracks
                .into_iter()
                .map(|t| {
                    let name = p
                        .tracks
                        .get(t)
                        .map(|x| x.name.clone())
                        .ok_or(format!("there's no track {t}"))?;
                    Ok(Job {
                        name: format!("{base} - {}", bounce::file_safe(&name)),
                        track: Some(t),
                        range,
                        master,
                    })
                })
                .collect::<Result<_, String>>()?
        }
        "sections" => bounce::sections(&p)
            .into_iter()
            .enumerate()
            .map(|(k, (name, from, to))| Job {
                name: format!("{base} - {:02} {}", k + 1, bounce::file_safe(&name)),
                track: None,
                range: Some((from, to)),
                master: true,
            })
            .collect(),
        other => return Err(format!("{other}: mix, stems or sections")),
    };
    if jobs.is_empty() {
        return Err("nothing to bounce: no tracks with regions, or no markers for sections".into());
    }
    let out = spec
        .get("out")
        .and_then(Value::as_str)
        .map_or(dir.join("Bounces"), PathBuf::from);
    let mut results = Vec::new();
    for job in &jobs {
        let done = bounce::bounce(&p, dir, job, &opts, &out, &|_| {})?;
        let a = &done.analysis;
        results.push(json!({
            "name": done.name,
            "files": done.files,
            "seconds": (done.seconds * 100.0).round() / 100.0,
            "lufs": (a.lufs as f64 * 10.0).round() / 10.0,
            "peak_db": (a.peak_db[0].max(a.peak_db[1]) as f64 * 10.0).round() / 10.0,
            "gain_db": (done.gain_db as f64 * 10.0).round() / 10.0,
        }));
    }
    Ok(json!({ "folder": out, "bounced": results }))
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
