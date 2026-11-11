//! Synth-0's sounds for an agent: the library the synth's window shows (the factory's, those
//! saved, packs), searched; a track's sound saved into it, with a name, a part, words and a line;
//! one loaded onto a track; a pack made; a sound moved into one. Through the app when it has the
//! song open, so the window and its library follow at once; else on the files.

use std::path::{Path, PathBuf};

use pina_engine::song::{
    commands::Command,
    project::{Instrument, Project},
};
use serde_json::{Value, json};

use super::{edit_with, server};
use crate::editors::poly::library::{
    FACTORY, Library, PARTS, PackInfo, SavedSound, now, sounds_dir,
};

/// What an agent asks of the library, by `op`: list, save, load, make_pack, move.
pub fn synth_sounds(args: &Value) -> Result<Value, String> {
    let dir = args
        .get("project")
        .and_then(Value::as_str)
        .map(PathBuf::from);
    if let Some(dir) = &dir
        && let Some(reply) = server::ask(dir, &server::Request::SynthSounds { args: args.clone() })
    {
        return reply;
    }
    let mut lib = Library::read(&sounds_dir());
    if let Some(done) = library_op(&mut lib, args) {
        return done;
    }
    let dir = dir.ok_or("`project`: the song's folder, for a track's sound")?;
    let t = track(args)?;
    match op(args) {
        "save" => {
            let project = Project::load(&dir)?;
            let poly = synth_on(&project, t)?;
            let (sound, pack) = to_save(args, poly, "")?;
            let key = lib.save(sound, pack.as_deref())?;
            set_patch(&dir, t, &key)?;
            Ok(json!({ "saved": key }))
        }
        "load" => {
            let key = key(args)?;
            let e = lib.find(&key).ok_or_else(|| missing(&key))?.clone();
            let mut poly = e.sound.poly;
            poly.patch = e.key.clone();
            let settings = serde_json::to_value(&poly).map_err(|e| e.to_string())?;
            edit_with(
                &dir,
                Command::SetSynth {
                    track: t,
                    patch: None,
                    settings: Some(settings),
                },
            )?;
            Ok(json!({ "loaded": e.key, "track": t }))
        }
        other => Err(format!(
            "no op {other}: list, save, load, make_pack or move"
        )),
    }
}

fn op(args: &Value) -> &str {
    args.get("op").and_then(Value::as_str).unwrap_or("list")
}

fn track(args: &Value) -> Result<usize, String> {
    args.get("track")
        .and_then(Value::as_u64)
        .map(|t| t as usize)
        .ok_or_else(|| "`track`: which track's Synth-0".into())
}

fn key(args: &Value) -> Result<String, String> {
    args.get("key")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "`key`: a sound as `list` gives it (\"Pack/Name\", a factory name)".into())
}

fn missing(key: &str) -> String {
    format!("no sound {key}: `list` gives every key")
}

fn synth_on(project: &Project, t: usize) -> Result<pina_engine::song::project::Poly, String> {
    match project.tracks.get(t).and_then(|tr| tr.instrument.as_ref()) {
        Some(Instrument::Poly(p)) => Ok((**p).clone()),
        Some(_) => Err(format!("track {t} doesn't play Synth-0")),
        None => Err(format!("no track {t}, or it has no instrument")),
    }
}

/// A sound to save from `args` and the synth's settings: its name, part (one of the library's),
/// words, line and pack (none: just yours), made by `by`.
pub(crate) fn to_save(
    args: &Value,
    poly: pina_engine::song::project::Poly,
    by: &str,
) -> Result<(SavedSound, Option<String>), String> {
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or("`name`: what the sound's called")?
        .to_string();
    let part = args.get("part").and_then(Value::as_str).unwrap_or("Lead");
    let part = PARTS
        .iter()
        .find(|p| p.eq_ignore_ascii_case(part))
        .ok_or_else(|| format!("`part` is one of {}", PARTS.join(", ")))?
        .to_string();
    let words = args
        .get("words")
        .and_then(Value::as_array)
        .map(|w| {
            w.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let about = args
        .get("about")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let pack = args.get("pack").and_then(Value::as_str).map(str::to_string);
    if pack.as_deref() == Some(FACTORY) {
        return Err("the factory's sounds stay as they are: save into a pack of your own".into());
    }
    Ok((
        SavedSound {
            name,
            part,
            words,
            about,
            by: by.to_string(),
            made: now(),
            poly,
        },
        pack,
    ))
}

/// The track's sound now named by `key`, so its window says what it is.
fn set_patch(dir: &Path, t: usize, key: &str) -> Result<(), String> {
    edit_with(
        dir,
        Command::SetSynth {
            track: t,
            patch: None,
            settings: Some(json!({ "patch": key })),
        },
    )
    .map(drop)
}

/// What needs only the library (list, make_pack, move); None for what needs a track.
pub(crate) fn library_op(lib: &mut Library, args: &Value) -> Option<Result<Value, String>> {
    let text = |k: &str| {
        args.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Some(match op(args) {
        "list" => {
            let words: Vec<String> = text("query")
                .split_whitespace()
                .map(str::to_lowercase)
                .collect();
            let (part, pack) = (text("part"), text("pack"));
            let sounds: Vec<Value> = lib
                .sounds
                .iter()
                .filter(|e| part.is_empty() || e.sound.part.eq_ignore_ascii_case(&part))
                .filter(|e| pack.is_empty() || e.pack.as_deref() == Some(pack.as_str()))
                .filter(|e| {
                    let all = format!(
                        "{} {} {} {} {}",
                        e.sound.name,
                        e.sound.part,
                        e.sound.about,
                        e.sound.words.join(" "),
                        e.pack.as_deref().unwrap_or_default()
                    )
                    .to_lowercase();
                    words.iter().all(|w| all.contains(w))
                })
                .take(200)
                .map(|e| {
                    json!({ "key": e.key, "name": e.sound.name, "part": e.sound.part,
                        "words": e.sound.words, "about": e.sound.about,
                        "pack": e.pack.clone().unwrap_or_else(|| "none: yours".into()) })
                })
                .collect();
            let packs: Vec<Value> = lib
                .packs
                .iter()
                .map(|p| json!({ "name": p.name, "sounds": lib.count(&p.name), "imported": p.imported, "about": p.about }))
                .collect();
            Ok(json!({ "sounds": sounds, "packs": packs, "parts": PARTS }))
        }
        "make_pack" => lib
            .make_pack(PackInfo {
                name: text("name"),
                about: text("about"),
                ..Default::default()
            })
            .map(|name| json!({ "pack": name })),
        "move" => match key(args) {
            Ok(k) => {
                let to = args.get("pack").and_then(Value::as_str);
                lib.move_sound(&k, to).map(|key| json!({ "moved": key }))
            }
            Err(e) => Err(e),
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agent_lists_saves_into_a_pack_and_finds_its_sounds() {
        let dir = sounds_dir();
        let mut lib = Library::read(&dir);
        let made = library_op(
            &mut lib,
            &json!({ "op": "make_pack", "name": "Night Drive" }),
        )
        .unwrap()
        .unwrap();
        assert_eq!(made["pack"], "Night Drive");
        let poly = pina_engine::song::project::poly_patch("Evolving Pad").unwrap();
        let (sound, pack) = to_save(
            &json!({ "name": "Glass Rain", "part": "pad", "words": ["Glassy", "Wide"], "about": "slow glass", "pack": "Night Drive" }),
            poly,
            "agent",
        )
        .unwrap();
        assert_eq!(sound.part, "Pad");
        lib.save(sound, pack.as_deref()).unwrap();
        let found = library_op(&mut lib, &json!({ "op": "list", "query": "glass" }))
            .unwrap()
            .unwrap();
        let keys: Vec<&str> = found["sounds"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|s| s["key"].as_str())
            .collect();
        assert!(keys.contains(&"Night Drive/Glass Rain"), "{keys:?}");
        assert!(
            to_save(
                &json!({ "name": "x", "part": "kazoo" }),
                Default::default(),
                ""
            )
            .is_err()
        );
        assert!(
            to_save(
                &json!({ "name": "x", "pack": FACTORY }),
                Default::default(),
                ""
            )
            .is_err()
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
