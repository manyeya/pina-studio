//! `tempo samples`: the sample library for agents and scripts, as JSON. Finding samples by words
//! and fit, what's similar, kits that go together, and what a sample is.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tempo_engine::samples::{
    Kind, Sample,
    library::Library,
    note_name, place,
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
    if done == of || done.is_multiple_of(50) {
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

/// Puts a sample into a song, made to fit it. A loop goes on an audio track (`track`, else a new
/// one) at `at`, stretched to the song's tempo, moved into its key if a small move fits, and
/// looping; a one-shot goes into a sampler (`track`'s, else a new sampler track) as a zone that
/// plays it in tune: a drum on its note, a pitched sound across the keys from its own note.
pub fn put(file: &Path, dir: &Path, track: Option<usize>, at: u64) -> Result<Value, String> {
    use tempo_engine::{
        commands::Command,
        project::{Instrument, Sampler},
    };
    let lib = library();
    let path = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    let sample = match lib.get(&path) {
        Some(s) => s.clone(),
        None => Sample {
            path: path.clone(),
            name: path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            folder: String::new(),
            modified: 0,
            size: 0,
            analysis: tempo_engine::samples::analyze::analyze_file(&path)?,
        },
    };
    let p = crate::cli::project(dir)?;
    let edit = |c: Command| crate::cli::edit_with(dir, c);
    let a = &sample.analysis;
    let mut done = Vec::new();
    if a.kind == Kind::Loop {
        let t = match track.filter(|&t| p.tracks.get(t).is_some_and(|t| t.instrument.is_none())) {
            Some(t) => t,
            None => {
                let made = edit(Command::AddTrack {
                    name: sample.name.clone(),
                    instrument: None,
                    index: None,
                })?;
                made["track"].as_u64().ok_or("no track made")? as usize
            }
        };
        let r = edit(Command::AddAudioRegion {
            track: t,
            file: path.clone(),
            start: at,
            offset: 0,
            length: None,
        })?;
        let region = r["region"][1].as_u64().ok_or("no region made")? as usize;
        // In time: its beats at the song's tempo.
        if let (Some(bpm), Some(length)) = (a.bpm, place::loop_ticks(a, p.tempo)) {
            edit(Command::FlexRegion {
                track: t,
                region,
                length,
            })?;
            let how = match place::feel(bpm as f64, p.tempo) / bpm as f64 {
                f if f < 1.0 => " as half time",
                f if f > 1.0 => " as double time",
                _ => "",
            };
            done.push(format!(
                "stretched from {bpm:.0} to {:.0} BPM{how}",
                p.tempo
            ));
        }
        // In key: the small move that fits it, if there is one.
        let fit = p.song_key().and_then(|key| {
            let q = search::Query {
                key: Some(key.name()),
                limit: usize::MAX,
                ..Default::default()
            };
            let index = lib.samples.iter().position(|s| s.path == path)?;
            search::find(&lib.samples, &q)
                .into_iter()
                .find(|h| h.index == index)
                .and_then(|h| h.transpose.map(|t| (t, key)))
        });
        // Looping, and following the song's tempo from here on.
        let mut set = serde_json::json!({
            "op": "set_region", "track": t, "region": region, "loop": true, "follow": true
        });
        if let Some((semitones, key)) = fit {
            set["transpose"] = json!(semitones);
            done.push(format!("moved {semitones:+} semitones into {}", key.name()));
        }
        edit(serde_json::from_value(set).map_err(|e| e.to_string())?)?;
        return Ok(json!({ "track": t, "region": [t, region], "done": done }));
    }
    let sampler = |i: Option<&Instrument>| match i {
        Some(Instrument::Sampler(s)) => Some(s.clone()),
        _ => None,
    };
    // Kept off the notes the sampler's other zones play (a full-range sound makes room).
    match track.and_then(|t| sampler(p.tracks.get(t)?.instrument.as_ref()).map(|s| (t, s))) {
        Some((t, mut s)) => {
            place::place_zone(&mut s, &sample, None)?;
            let zone = s.zones.last().unwrap();
            let (note, high) = (zone.note, zone.high);
            edit(Command::SetInstrument {
                track: t,
                instrument: Instrument::Sampler(s),
            })?;
            Ok(json!({ "track": t, "note": note, "high": high }))
        }
        None => {
            let mut s = Sampler::default();
            place::place_zone(&mut s, &sample, None)?;
            let (note, high) = (s.zones[0].note, s.zones[0].high);
            let made = edit(Command::AddTrack {
                name: sample.name.clone(),
                instrument: Some(Instrument::Sampler(s)),
                index: None,
            })?;
            Ok(json!({ "track": made["track"], "note": note, "high": high }))
        }
    }
}

/// What the person picked for the song: each sound as the library knows it (what it is, its note
/// or key, tempo), each instrument and effect with how to put it in; what each is for, and
/// whether it's in the song yet.
pub fn palette(dir: &Path) -> Result<Value, String> {
    let p = crate::cli::project(dir)?;
    let lib = Library::load();
    let sounds: Vec<Value> = p
        .palette
        .sounds
        .iter()
        .map(|ps| {
            let found = lib.get(&ps.path).cloned().map(Ok).unwrap_or_else(|| {
                tempo_engine::samples::analyze::analyze_file(&ps.path).map(|analysis| Sample {
                    path: ps.path.clone(),
                    name: ps
                        .path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    folder: String::new(),
                    modified: 0,
                    size: 0,
                    analysis,
                })
            });
            let mut v = match found {
                Ok(s) => sample_json(&s),
                Err(e) => json!({ "path": ps.path, "error": e }),
            };
            let map = v.as_object_mut().unwrap();
            map.insert("role".into(), json!(ps.role));
            map.insert("in_song".into(), json!(p.uses(&ps.path)));
            v
        })
        .collect();
    let devices = |instrument: bool| -> Vec<Value> {
        p.palette
            .devices
            .iter()
            .filter(|d| d.instrument == instrument)
            .map(|d| {
                let mut v = json!(d);
                let map = v.as_object_mut().unwrap();
                map.insert("use".into(), json!(crate::palette::how_to_use(d)));
                map.insert("in_song".into(), json!(p.uses_device(d)));
                v
            })
            .collect()
    };
    let how = match (p.palette.none(), p.palette.only) {
        (true, _) => {
            "The person hasn't picked anything: choose your own sounds, instruments and effects."
        }
        (false, true) => {
            "The person picked these and asked for these only: make the song with them, each \
             for its role (sounds with samples_put, one-shots for the same kit on one sampler \
             track by passing its `track`; instruments with set_instrument or add_track; effects \
             with set_insert, or tempo's own with `use`), and add no other sounds, instruments \
             or effects of a kind they picked. Parts none of them suits: ask, or leave out."
        }
        (false, false) => {
            "The person picked these: start with them, each for its role (none: what it suits; \
             sounds with samples_put, one-shots for the same kit on one sampler track by passing \
             its `track`; instruments with set_instrument or add_track; effects with set_insert, \
             or tempo's own with `use`), then fill what's missing with your own."
        }
    };
    Ok(json!({
        "only": p.palette.only,
        "sounds": sounds,
        "instruments": devices(true),
        "effects": devices(false),
        "how": how,
    }))
}

/// `tempo samples …` from the command line.
pub fn command(args: &[&str]) -> Result<Value, String> {
    // Options anywhere: --for <project>, --limit <n>, --track <n>, --at <tick>.
    let mut rest = Vec::new();
    let (mut project, mut limit, mut track, mut at) = (None, None, None, 0);
    let mut it = args.iter();
    while let Some(&a) = it.next() {
        match a {
            "--for" => project = it.next().map(PathBuf::from),
            "--track" => {
                track = Some(
                    it.next()
                        .and_then(|n| n.parse().ok())
                        .ok_or("--track takes a number")?,
                )
            }
            "--at" => {
                at = it
                    .next()
                    .and_then(|n| n.parse().ok())
                    .ok_or("--at takes a tick")?
            }
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
        ["put", file, dir] => put(Path::new(file), Path::new(dir), track, at),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palette_says_what_each_sound_is_its_part_and_whether_its_in() {
        let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/demo");
        let dir =
            std::env::temp_dir().join(format!("tempo-test-palette-tool-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Audio Files")).unwrap();
        for f in ["project.json", "Audio Files/drums.wav"] {
            std::fs::copy(demo.join(f), dir.join(f)).unwrap();
        }
        let drums = dir.join("Audio Files/drums.wav");
        let other = dir.join("loop.wav");
        std::fs::copy(&drums, &other).unwrap();
        let set = format!(
            r#"{{"op": "set_palette", "sounds": [{{"path": "{}", "role": "drum loop"}}, {{"path": "{}"}}],
                "devices": [{{"name": "Room", "plugin": {{"class": "Room", "path": "/p/Room.vst3"}}, "role": "vocal"}},
                {{"name": "Drum Kit", "instrument": true, "own": "drums"}}], "only": true}}"#,
            drums.display(),
            other.display()
        );
        crate::cli::edit_with(&dir, serde_json::from_str(&set).unwrap()).unwrap();
        let v = palette(&dir).unwrap();
        assert_eq!(v["only"], true);
        assert!(v["how"].as_str().unwrap().contains("no other"));
        let sounds = v["sounds"].as_array().unwrap();
        assert_eq!(sounds[0]["role"], "drum loop");
        assert_eq!(sounds[0]["in_song"], true); // the demo's drums play it
        assert_eq!(sounds[1]["in_song"], false);
        assert!(sounds[1]["role"].is_null() && sounds[1]["kind"].is_string());
        // Instruments and effects: how each goes in, and whether it has (the demo has drums).
        let (inst, fx) = (&v["instruments"][0], &v["effects"][0]);
        assert_eq!(inst["use"], "tempo's own: \"instrument\": \"drums\"");
        assert_eq!(fx["role"], "vocal");
        assert_eq!(fx["in_song"], false);
        assert!(fx["use"].as_str().unwrap().contains(r#""class":"Room""#));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
