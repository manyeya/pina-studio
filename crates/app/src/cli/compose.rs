//! `pina composers` and `pina compose`: composer models (MIDI-GPT and its kind) the person
//! downloads (Pina Studio ships none), and bars of a song written by one with the rest of the
//! song heard, each take saved as a MIDI file beside the song to hear and drag in.

use std::{path::Path, sync::atomic::AtomicBool};

use pina_engine::{
    composer::{
        self,
        catalog::{KNOWN, Known},
        score::{self, Ask, Part},
        server,
    },
    song::midi_file::{MidiFile, Part as MidiPart},
};
use serde_json::{Value, json};

/// Every known model: downloaded or not, its size and licence, whether its Python's installed,
/// and whether the person's signed in for the ones that ask.
pub fn list() -> Result<Value, String> {
    let dir = composer::folder();
    let have = composer::installed(&dir);
    Ok(json!({
        "folder": dir,
        "signed_in": composer::hf_token().is_some(),
        "models": KNOWN.iter().map(|k| json!({
            "id": k.id,
            "name": k.name,
            "maker": k.maker,
            "about": k.about,
            "megabytes": (k.bytes as f64 / 1e6).round(),
            "licence": k.licence,
            "licence_url": k.licence_url,
            "gated": k.gated,
            "page": k.page,
            "bars": k.bars,
            "microtiming": k.microtiming,
            "downloaded": have.iter().any(|h| h.known.id == k.id),
            "ready": server::ready(k.family, &dir),
        })).collect::<Vec<_>>(),
    }))
}

fn known(id: &str) -> Result<&'static Known, String> {
    Known::named(id).ok_or_else(|| format!("no composer model {id}: `pina composers` lists them"))
}

/// `id` downloaded (with the person's token, where its makers ask them to sign in), and its
/// family's Python installed. What's happening goes to stderr as it happens.
pub fn get(id: &str) -> Result<Value, String> {
    let k = known(id)?;
    let dir = composer::folder();
    let stop = AtomicBool::new(false);
    let file = match k.file_in(&dir) {
        f if f.is_file() => f,
        _ => {
            eprintln!("Downloading {} ({:.0} MB)", k.name, k.bytes as f64 / 1e6);
            let token = composer::hf_token();
            composer::catalog::download(k, &dir, token.as_deref(), |_| {}, &stop)?
        }
    };
    if !server::ready(k.family, &dir) {
        server::install(k.family, &dir, &mut |line| eprintln!("{line}"), &stop)?;
    }
    Ok(json!({ "model": k.id, "file": file, "ready": true }))
}

/// A downloaded model deleted, and its family's server stopped (the next ask starts it again).
pub fn remove(id: &str) -> Result<Value, String> {
    let k = known(id)?;
    let dir = composer::folder();
    server::stop_server(k.family, &dir);
    composer::remove(k, &dir)?;
    Ok(json!({ "removed": k.id }))
}

/// The person's Hugging Face token, saved where `hf auth login` keeps it.
pub fn token(t: &str) -> Result<Value, String> {
    composer::save_hf_token(t)?;
    Ok(json!({ "signed_in": true }))
}

const COMPOSE_USAGE: &str = "pina compose <project> --track N[,N] --bars 9-16 [--part bass] [--model id] [--takes 3] [--temperature 0.9] [--seed N] [--set name=level] [--any-key] [--name <name>]";

/// Bars of the song at `dir` written by a downloaded composer model, each take a MIDI file in
/// the song's MIDI folder. `args` as `COMPOSE_USAGE` says; bars count from 1, both ends kept.
pub fn compose(dir: &Path, args: &[&str]) -> Result<Value, String> {
    compose_parts(dir, args, &[])
}

/// As `compose`, each track (in the order `--track` gives them) heard as the part `parts`
/// names, where it names one: the Studio Agent's `compose`.
pub fn compose_parts(dir: &Path, args: &[&str], parts: &[&str]) -> Result<Value, String> {
    let mut ask = Ask::default();
    let mut model = None;
    let mut name = None;
    let mut it = args.iter().copied();
    while let Some(flag) = it.next() {
        let mut value = || {
            it.next()
                .ok_or_else(|| format!("{flag} needs a value: {COMPOSE_USAGE}"))
        };
        let number = |v: &str| -> Result<f64, String> {
            v.parse().map_err(|_| format!("{flag}: {v} isn't a number"))
        };
        match flag {
            "--track" => {
                for t in value()?.split(',') {
                    let t = t.trim().parse().map_err(|_| format!("{t} isn't a track"))?;
                    ask.tracks.push((t, None));
                }
            }
            "--part" => {
                let v = value()?;
                let part = Part::named(v).ok_or_else(|| {
                    let all: Vec<&str> = Part::ALL.iter().map(|p| p.name()).collect();
                    format!("{v} isn't a part: {}", all.join(", "))
                })?;
                ask.tracks.iter_mut().for_each(|t| t.1 = Some(part));
            }
            "--bars" => {
                let v = value()?;
                let (a, b) = match v.split_once('-') {
                    Some((a, b)) => (number(a)?, number(b)?),
                    None => (number(v)?, number(v)?),
                };
                if a < 1.0 || b < a {
                    return Err(format!("--bars {v}: from bar 1, the first before the last"));
                }
                ask.bars = (a as u32 - 1, b as u32);
            }
            "--model" => model = Some(value()?),
            "--takes" => ask.takes = number(value()?)? as u32,
            "--temperature" => ask.temperature = number(value()?)? as f32,
            "--seed" => ask.seed = Some(number(value()?)? as i64),
            "--set" => {
                let v = value()?;
                let (k, level) = v
                    .split_once('=')
                    .ok_or_else(|| format!("--set {v}: name=level"))?;
                ask.attributes.insert(k.to_string(), number(level)? as i64);
            }
            "--any-key" => ask.in_key = false,
            "--name" => name = Some(value()?.to_string()),
            _ => return Err(format!("{flag}? {COMPOSE_USAGE}")),
        }
    }
    for (t, name) in ask.tracks.iter_mut().zip(parts) {
        if !name.is_empty() {
            t.1 = Some(Part::named(name).ok_or_else(|| format!("{name} isn't a part"))?);
        }
    }
    let folder = composer::folder();
    let have = composer::installed(&folder);
    let m = match model {
        Some(id) => have
            .iter()
            .find(|h| h.known.id == id)
            .ok_or_else(|| format!("{id} isn't downloaded: `pina composers get {id}`"))?,
        None => have.first().ok_or(
            "no composer model downloaded: `pina composers get midigpt-yellow-small` (after `pina composers token <token>`)",
        )?,
    };
    let p = super::project(dir)?;
    let stop = AtomicBool::new(false);
    let running = server::start(m, &folder, "auto", &mut |line| eprintln!("{line}"), &stop)?;
    let caps = server::info(&running.url)?;
    let prompt = score::prompt(&p, &ask, &caps)?;
    let count = ask.takes.clamp(1, 16);
    let mut takes = Vec::new();
    for k in 0..count {
        eprintln!("{}: take {} of {count}", m.known.name, k + 1);
        let answer = server::generate(
            &running.url,
            &prompt.take(k, ask.seed),
            &mut |pr, _| eprint!("\r{} notes written", pr.notes),
            &stop,
        )?;
        eprintln!();
        takes.extend(score::takes(&answer, &prompt, &ask)?);
    }
    // Each take from its first bar, so it lines up wherever it's dropped.
    let from = ask.bars.0 as i64 * prompt.bar as i64;
    let out = dir.join(super::midi::MIDI_DIR);
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let base = name.unwrap_or_else(|| {
        let parts: Vec<&str> = ask
            .tracks
            .iter()
            .filter_map(|(t, _)| p.tracks.get(*t).map(|tr| tr.name.as_str()))
            .collect();
        format!(
            "{} bars {}-{}",
            parts.join(" and "),
            ask.bars.0 + 1,
            ask.bars.1
        )
    });
    let base: String = base
        .chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '-' } else { c })
        .collect();
    let mut written = Vec::new();
    for (k, take) in takes.iter().enumerate() {
        let parts = take
            .notes
            .iter()
            .enumerate()
            .map(|(c, (&t, notes))| {
                let i = prompt.order.iter().position(|&o| o == t).unwrap_or(0);
                let sounds = &prompt.body["score"]["tracks"][i];
                let drums = sounds["track_type"] == "drum";
                MidiPart {
                    name: p.tracks[t].name.clone(),
                    channel: if drums { 9 } else { (c % 9) as u8 },
                    program: (!drums).then(|| sounds["instrument"].as_u64().unwrap_or(0) as u8),
                    notes: notes
                        .iter()
                        .map(|n| pina_engine::song::project::Note {
                            start: n.start - from,
                            ..n.clone()
                        })
                        .collect(),
                    controls: Vec::new(),
                }
            })
            .collect();
        let file = MidiFile {
            parts,
            tempo: Some(p.tempo),
            tempos: Vec::new(),
            time_signature: Some(p.time_signature),
            key: p.song_key(),
            name: Some(base.clone()),
        };
        let path = (1..)
            .map(|n| match (takes.len(), n) {
                (1, 1) => out.join(format!("{base}.mid")),
                (1, n) => out.join(format!("{base} {n}.mid")),
                (_, 1) => out.join(format!("{base} take {}.mid", k + 1)),
                (_, n) => out.join(format!("{base} take {} {n}.mid", k + 1)),
            })
            .find(|f| !f.exists())
            .unwrap_or_default();
        std::fs::write(&path, file.to_bytes()).map_err(|e| format!("{}: {e}", path.display()))?;
        written.push(json!({
            "file": path,
            "seed": take.seed,
            "notes": take.notes.values().map(Vec::len).sum::<usize>(),
        }));
    }
    Ok(json!({
        "model": m.known.id,
        "bars": [ask.bars.0 + 1, ask.bars.1],
        "takes": written,
    }))
}
