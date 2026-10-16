//! `tempo samples`: the sample library for agents and scripts, as JSON. Finding samples by words
//! and fit, what's similar, kits that go together, and what a sample is.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tempo_engine::samples::{
    Kind, Sample,
    library::Library,
    note_name,
    search::{self, Aspects, Hit, KitSeed, Query},
};

/// The library, scanned first if it's never been (so a first search finds something).
fn library() -> Library {
    let mut lib = Library::load();
    if lib.samples.is_empty() {
        eprintln!("analysing the sample library for the first time…");
        lib.scan(&progress);
    }
    lib
}

fn progress(done: usize, of: usize) {
    if done == of || done % 50 == 0 {
        eprintln!("analysed {done} of {of}");
    }
}

/// A hit as JSON: what the sample is, how well it matched and why, and how a sampler would
/// play it.
pub fn hit_json(lib: &Library, hit: &Hit) -> Value {
    let mut v = sample_json(&lib.samples[hit.index]);
    let map = v.as_object_mut().unwrap();
    map.insert("score".into(), json!((hit.score * 100.0).round() / 100.0));
    map.insert("reasons".into(), json!(hit.reasons));
    if let Some(t) = hit.transpose {
        map.insert("transpose".into(), json!(t));
    }
    v
}

/// A sample as JSON, its analysis in brief (no embedding or outline).
pub fn sample_json(s: &Sample) -> Value {
    let a = &s.analysis;
    let mut v = json!({
        "path": s.path,
        "name": s.name,
        "folder": s.folder,
        "about": search::describe(s),
        "kind": a.kind,
        "category": a.category,
        "confidence": (a.confidence * 100.0).round() / 100.0,
        "seconds": (a.seconds * 1000.0).round() / 1000.0,
        "traits": a.traits,
    });
    let map = v.as_object_mut().unwrap();
    if let Some(p) = &a.pitch {
        map.insert("note".into(), json!(note_name(p.note)));
        map.insert("midi".into(), json!(p.note));
        map.insert("cents".into(), json!(p.cents.round()));
    }
    if let Some(k) = &a.key {
        map.insert("key".into(), json!(k.name));
    }
    if let Some(bpm) = a.bpm {
        map.insert("bpm".into(), json!((bpm * 10.0).round() / 10.0));
    }
    if let Some(bars) = a.bars {
        map.insert("bars".into(), json!(bars));
    }
    // How to play it: a sampler zone for a one-shot (for `add_track`'s sampler), an audio
    // region for a loop.
    if a.kind == Kind::OneShot {
        map.insert("zone".into(), json!(search::zone(s, None)));
    }
    v
}

/// Fills a query's key and tempo from a project, where it doesn't say them.
fn from_project(
    project: Option<&Path>,
    key: &mut Option<String>,
    bpm: &mut Option<f32>,
) -> Result<(), String> {
    let Some(dir) = project else {
        return Ok(());
    };
    let p = crate::cli::project(dir)?;
    if key.is_none() {
        *key = p.song_key().map(|k| k.name());
    }
    if bpm.is_none() {
        *bpm = Some(p.tempo as f32);
    }
    Ok(())
}

/// A query from the command line: JSON (`{"text": …, "key": …}`), or words.
fn query(arg: &str) -> Result<Query, String> {
    match arg.trim_start().starts_with('{') {
        true => serde_json::from_str(arg).map_err(|e| format!("not a query: {e}")),
        false => Ok(Query {
            text: arg.to_string(),
            ..Default::default()
        }),
    }
}

pub fn find(mut q: Query, project: Option<&Path>) -> Result<Value, String> {
    from_project(project, &mut q.key, &mut q.bpm)?;
    let lib = library();
    let hits = search::find(&lib.samples, &q);
    Ok(Value::Array(
        hits.iter().map(|h| hit_json(&lib, h)).collect(),
    ))
}

pub fn similar(path: &Path, aspects: Aspects, limit: usize) -> Result<Value, String> {
    let lib = library();
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if lib.get(&path).is_none() {
        return Err(format!(
            "{} isn't in the library (see `tempo samples`)",
            path.display()
        ));
    }
    let hits = search::similar(&lib.samples, &path, aspects, limit);
    Ok(Value::Array(
        hits.iter().map(|h| hit_json(&lib, h)).collect(),
    ))
}

pub fn kit(mut seed: KitSeed, project: Option<&Path>) -> Result<Value, String> {
    from_project(project, &mut seed.key, &mut seed.bpm)?;
    let lib = library();
    let kit = search::kit(&lib.samples, &seed);
    let slots: Vec<Value> = kit
        .slots
        .iter()
        .map(|slot| {
            json!({
                "role": slot.role,
                "pick": slot.pick.as_ref().map(|h| hit_json(&lib, h)),
                "alternatives": slot.alternatives.iter().map(|h| hit_json(&lib, h)).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!({ "key": seed.key, "bpm": seed.bpm, "slots": slots }))
}

pub fn info(path: &Path) -> Result<Value, String> {
    let lib = library();
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    match lib.get(&path) {
        Some(s) => {
            let mut v = sample_json(s);
            let a = &s.analysis;
            let map = v.as_object_mut().unwrap();
            map.insert(
                "analysis".into(),
                json!({
                    "tonal": a.tonal,
                    "chroma": a.chroma,
                    "loudness": a.loudness,
                    "peak_db": a.peak_db,
                    "envelope": a.envelope,
                    "spectrum": a.spectrum,
                    "width": a.width,
                    "pitch": a.pitch,
                    "key": a.key,
                }),
            );
            Ok(v)
        }
        // Not in the library: analyse it on the spot.
        None => {
            let analysis = tempo_engine::samples::analyze::analyze_file(&path)?;
            Ok(json!({ "path": path, "analysis": analysis }))
        }
    }
}

/// The library's folders, and what's in it.
pub fn status() -> Result<Value, String> {
    let lib = Library::load();
    let mut kinds = serde_json::Map::new();
    for s in &lib.samples {
        let key = s.analysis.category.name().to_string();
        let n = kinds.get(&key).and_then(Value::as_u64).unwrap_or(0);
        kinds.insert(key, json!(n + 1));
    }
    Ok(json!({
        "folders": lib.folders,
        "samples": lib.samples.len(),
        "loops": lib.samples.iter().filter(|s| s.analysis.kind == Kind::Loop).count(),
        "categories": kinds,
    }))
}

pub fn scan() -> Result<Value, String> {
    let mut lib = Library::load();
    let report = lib.scan(&progress);
    serde_json::to_value(report).map_err(|e| e.to_string())
}

pub fn add_folder(dir: &Path) -> Result<Value, String> {
    let mut lib = Library::load();
    lib.add_folder(dir)?;
    let report = lib.scan(&progress);
    serde_json::to_value(report).map_err(|e| e.to_string())
}

pub fn remove_folder(dir: &Path) -> Result<Value, String> {
    let mut lib = Library::load();
    lib.remove_folder(dir);
    lib.save()?;
    status()
}

/// `tempo samples …` from the command line.
pub fn command(args: &[&str]) -> Result<Value, String> {
    // Options anywhere: --for <project>, --limit <n>.
    let mut rest = Vec::new();
    let (mut project, mut limit) = (None, None);
    let mut it = args.iter();
    while let Some(&a) = it.next() {
        match a {
            "--for" => project = it.next().map(PathBuf::from),
            "--limit" => {
                limit = Some(
                    it.next()
                        .and_then(|n| n.parse().ok())
                        .ok_or("--limit takes a number")?,
                )
            }
            _ => rest.push(a),
        }
    }
    let project = project.as_deref();
    match rest.as_slice() {
        [] => status(),
        ["scan"] => scan(),
        ["add", dir] => add_folder(Path::new(dir)),
        ["remove", dir] => remove_folder(Path::new(dir)),
        ["info", path] => info(Path::new(path)),
        ["similar", path] => similar(Path::new(path), Aspects::default(), limit.unwrap_or(20)),
        ["find", words @ ..] => {
            let mut q = query(&words.join(" "))?;
            if let Some(n) = limit {
                q.limit = n;
            }
            find(q, project)
        }
        ["kit", words @ ..] => {
            let joined = words.join(" ");
            let seed = match joined.trim_start().starts_with('{') {
                true => {
                    serde_json::from_str(&joined).map_err(|e| format!("not a kit seed: {e}"))?
                }
                false => KitSeed {
                    text: joined,
                    ..Default::default()
                },
            };
            kit(seed, project)
        }
        _ => Err(crate::USAGE.to_string()),
    }
}
