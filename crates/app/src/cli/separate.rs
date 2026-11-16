//! `pina models` and `pina separate`: the separation models the person has added (Pina Studio
//! comes with none), and a file parted into its stems by one. `pina lyrics`: the lines sung in a
//! file, by a Whisper model the person has downloaded.

use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

use pina_engine::separate;
use serde_json::{Value, json};

/// The models in the Models folder, and the folders there that aren't one, with why.
pub fn models() -> Result<Value, String> {
    let dir = separate::folder();
    let (found, wrong) = separate::models(&dir);
    Ok(json!({
        "folder": dir,
        "runs_here": separate::available(),
        "models": found.iter().map(|m| json!({
            "name": m.about.name,
            "stems": m.about.stems,
            "rest": m.about.rest,
            "input": m.about.input,
            "rate": m.about.rate,
            "licence": m.about.licence,
            "source": m.about.source,
            "dir": m.dir,
        })).collect::<Vec<_>>(),
        "problems": wrong,
    }))
}

/// `pina models inspect <file.onnx>`: its inputs and outputs, to write its model.json from.
pub fn inspect(file: &Path) -> Result<Value, String> {
    let (ins, outs) = separate::inspect(file)?;
    let list = |v: Vec<(String, Vec<i64>)>| -> Vec<Value> {
        v.into_iter()
            .map(|(name, shape)| json!({"name": name, "shape": shape}))
            .collect()
    };
    Ok(json!({"inputs": list(ins), "outputs": list(outs)}))
}

/// `pina models get <name>`: a model that's been run, downloaded from where its maker keeps it
/// into Models; without a name, the ones there are to get.
pub fn get(name: Option<&str>) -> Result<Value, String> {
    use separate::catalog::KNOWN;
    let list = || -> Value {
        KNOWN
            .iter()
            .map(|k| json!({"id": k.id, "name": k.about().name, "bytes": k.bytes, "licence": k.licence, "heard": k.heard}))
            .collect()
    };
    let Some(name) = name else {
        return Ok(json!({ "to_get": list() }));
    };
    let n = name.to_lowercase();
    let known = KNOWN
        .iter()
        .find(|k| k.id == n || k.about().name.to_lowercase().starts_with(&n))
        .ok_or(format!(
            "No model called {name} to get: pina models get lists them"
        ))?;
    let mut shown = -1;
    let folder = separate::catalog::download(
        known,
        &separate::folder(),
        |b| {
            let pct = (b * 100 / known.bytes.max(1)) as i64;
            if pct != shown {
                shown = pct;
                eprint!("\r{}: {pct}%", known.id);
                let _ = std::io::stderr().flush();
            }
        },
        &AtomicBool::new(false),
    )?;
    eprintln!();
    Ok(json!({ "model": known.about().name, "folder": folder, "licence": known.licence }))
}

/// `pina separate <file> [--model <name>] [--out <folder>]`: a WAV a stem, in `<file> stems`
/// here unless `--out` says where.
pub fn command(file: &Path, rest: &[&str]) -> Result<Value, String> {
    let mut model = None;
    let mut out = None;
    let mut args = rest.iter();
    while let Some(a) = args.next() {
        match *a {
            "--model" => model = args.next().copied(),
            "--out" => out = args.next().map(PathBuf::from),
            other => return Err(format!("pina separate: what's {other}?")),
        }
    }
    if !file.is_file() {
        return Err(format!("{}: no such file", file.display()));
    }
    let model = separate::find(&separate::folder(), model)?;
    let stem = file
        .file_stem()
        .map_or("audio".into(), |s| s.to_string_lossy());
    let out = out.unwrap_or_else(|| PathBuf::from(format!("{stem} stems")));
    let started = std::time::Instant::now();
    let mut shown = -1;
    let files = separate::separate(
        file,
        &model,
        &out,
        |p| {
            let pct = (p * 100.0) as i32;
            if pct != shown {
                shown = pct;
                eprint!("\r{}: {pct}%", model.about.name);
                let _ = std::io::stderr().flush();
            }
        },
        &AtomicBool::new(false),
    )?;
    eprintln!();
    Ok(json!({
        "model": model.about.name,
        "seconds": (started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
        "stems": files,
    }))
}

/// `pina lyrics <file> [--model base|small|turbo]`: the lines sung in it, each when it starts
/// and ends (seconds) and how many times it's sung; by the best Whisper model downloaded, or
/// the one named.
pub fn lyrics(file: &Path, rest: &[&str]) -> Result<Value, String> {
    use pina_engine::lyrics;
    let want = match rest {
        [] => None,
        ["--model", m] => Some(m.to_lowercase()),
        _ => return Err("pina lyrics <file> [--model base|small|turbo]".into()),
    };
    let dir = lyrics::folder();
    let have = lyrics::installed(&dir);
    let model = match &want {
        None => have.first(),
        Some(m) => have
            .iter()
            .find(|w| w.name.to_lowercase().contains(m.as_str())),
    }
    .ok_or(format!(
        "No Whisper model{} in {}: get one in Settings › Stem models",
        want.map(|m| format!(" called {m}")).unwrap_or_default(),
        dir.display()
    ))?;
    let (sides, rate) = pina_engine::audio::engine::read_audio(file)?;
    let started = std::time::Instant::now();
    let lines = lyrics::hear(&dir.join(model.file), &sides, rate)?;
    Ok(json!({
        "model": model.name,
        "seconds": (started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
        "lines": lines,
    }))
}

/// The audio region `region` on track `track` of the song in `dir`, as the app has it if it's
/// open: its file.
fn region_file(dir: &Path, track: usize, region: usize) -> Result<PathBuf, String> {
    let p = crate::cli::project(dir)?;
    let r = p
        .tracks
        .get(track)
        .and_then(|t| t.regions.get(region))
        .ok_or(format!("no region {region} on track {track}"))?;
    match &r.content {
        pina_engine::song::project::Content::Audio { file, .. } => Ok(file.clone()),
        _ => Err(format!("region {region} on track {track} isn't audio")),
    }
}

/// The agent's `stems`: an audio region parted by a separation model (`model`, else the first
/// there is) into stems kept in the song's Audio Files/Stems (made once a file and model), each
/// then on a track of its own in a stack (`add_stems`), live in the app if it's open.
pub fn stems_tool(
    dir: &Path,
    track: usize,
    region: usize,
    model: Option<&str>,
) -> Result<Value, String> {
    let file = region_file(dir, track, region)?;
    let model = separate::find(&separate::folder(), model)?;
    let folder = separate::stems_folder(&file, &model.about.name);
    let names: Vec<String> = model
        .about
        .stems
        .iter()
        .chain(&model.about.rest)
        .cloned()
        .collect();
    let stems: Vec<(String, PathBuf)> = names
        .iter()
        .map(|n| (n.clone(), folder.join(format!("{n}.wav"))))
        .collect();
    let started = std::time::Instant::now();
    if !stems.iter().all(|(_, f)| dir.join(f).is_file()) {
        separate::separate(
            &dir.join(&file),
            &model,
            &dir.join(&folder),
            |_| {},
            &AtomicBool::new(false),
        )?;
    }
    let added = crate::cli::edit_with(
        dir,
        pina_engine::song::commands::Command::AddStems {
            track,
            region,
            stems: stems.clone(),
        },
    )?;
    Ok(json!({
        "model": model.about.name,
        "seconds": started.elapsed().as_secs(),
        "stems": stems.iter().map(|(n, f)| json!({"name": n, "file": f})).collect::<Vec<_>>(),
        "tracks": added["tracks"],
    }))
}

/// The agent's `lyrics`: the lines sung in an audio region's file, by the best Whisper model
/// downloaded; a song's vocals separated first (`lyrics::hear_file`), kept.
pub fn lyrics_tool(dir: &Path, track: usize, region: usize) -> Result<Value, String> {
    use pina_engine::lyrics;
    let file = region_file(dir, track, region)?;
    let model = lyrics::installed(&lyrics::folder())
        .first()
        .copied()
        .ok_or("No Whisper model: the person gets one in Settings › Stems & lyrics")?;
    let heard = lyrics::hear_file(dir, &file, model, false, |_| {}, &AtomicBool::new(false))?;
    let separated = heard.file != file;
    Ok(json!({
        "model": model.name,
        "file": heard.file,
        "from_vocals_stem": separated,
        "lines": heard.lines.iter().map(|l| json!({
            "text": l.text,
            "from": l.start,
            "to": l.end,
            "sung": l.sung,
            "words": l.words.iter().map(|w| json!({"text": w.text, "from": w.start, "to": w.end})).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    }))
}

/// `pina slices <audio file> [--voice]`: it cut at its hits, each slice named for what it sounds
/// like (Smart Slice, as it is); `--voice` also says how much of it is a voice, as the best
/// vocals model hears it.
pub fn slices(file: &Path, rest: &[&str]) -> Result<Value, String> {
    use pina_engine::smart::{self, InIt};
    if !rest.is_empty() {
        return Err("pina slices <audio file>".into());
    }
    let started = std::time::Instant::now();
    let (sides, rate) = pina_engine::audio::engine::read_audio(file)?;
    let stop = AtomicBool::new(false);
    // What's in it, by its sound: as the quickest vocals model hears it, else the analysis.
    let (heard, shares) = match pina_engine::lyrics::quickest_vocals_model(&separate::folder()) {
        Some(model) => {
            let (moments, sound) = separate::vocal_share(&model, &sides, rate, |_| {}, &stop)?;
            (InIt::of(moments, sound), Some((moments, sound)))
        }
        None => match pina_engine::lyrics::voice_alone(&sides, rate, "") {
            true => (InIt::Voice, None),
            false => (InIt::Beat, None),
        },
    };
    let listened = started.elapsed().as_secs_f64();
    let cut = smart::slices(&sides, rate, heard == InIt::Voice, |_| {}, &stop);
    Ok(json!({
        "heard": heard.name(),
        "voice_moments": shares.map(|s| s.0),
        "voice_sound": shares.map(|s| s.1),
        "seconds_to_listen": (listened * 10.0).round() / 10.0,
        "count": cut.len(),
        "slices": cut,
        "seconds": (started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
    }))
}

/// `pina flips <song> <track> <region>`: an agent (`--agent`, Claude Code by default) finds
/// flips in the lines sung in an audio region (heard as `lyrics` hears them), each printed as a
/// line of JSON as it's found; `--style` one of the styles, `--vibe` what to make, `--count`
/// how many.
pub fn flips(dir: &Path, track: usize, region: usize, rest: &[&str]) -> Result<Value, String> {
    use crate::agent::{Event, Kind, ask_once, tuning};
    use pina_engine::lyrics::{self, flips};
    let (mut style, mut vibe, mut count, mut kind) = ("any", String::new(), 6, Kind::ClaudeCode);
    let mut args = rest.iter();
    while let Some(a) = args.next() {
        let mut value = || args.next().copied().ok_or(format!("{a} needs a value"));
        match *a {
            "--style" => style = value()?,
            "--vibe" => vibe = value()?.to_string(),
            "--count" => count = value()?.parse().map_err(|_| "--count takes a number")?,
            "--agent" => kind = Kind::from_id(value()?).ok_or("--agent claude-code or codex")?,
            _ => {
                return Err(format!(
                    "pina flips <song> <track> <region> [--style {}] [--vibe text] [--count n] [--agent claude-code|codex]",
                    flips::STYLES
                        .iter()
                        .map(|s| s.0)
                        .collect::<Vec<_>>()
                        .join("|")
                ));
            }
        }
    }
    let p = crate::cli::project(dir)?;
    let file = region_file(dir, track, region)?;
    let model = lyrics::installed(&lyrics::folder())
        .first()
        .copied()
        .ok_or("No Whisper model: get one in Settings › Stems & lyrics")?;
    let heard = lyrics::hear_file(dir, &file, model, false, |_| {}, &AtomicBool::new(false))?;
    let bpb = (p.bar() / pina_engine::song::project::PPQ).max(1) as u32;
    let key = p.song_key().map(|k| k.name());
    let words = flips::numbered(&heard.lines);
    let prompt = flips::prompt(
        &heard.lines,
        p.tempo,
        bpb,
        key.as_deref(),
        style,
        &vibe,
        count,
        &[],
    );
    let cmd = kind
        .command()
        .ok_or(format!("{} isn't installed", kind.def().name))?;
    let started = std::time::Instant::now();
    let (_asked, events) = ask_once(kind, &cmd, &tuning(kind), flips::SYSTEM, &prompt)?;
    let mut reading = flips::Reading::default();
    let mut found = Vec::new();
    let say = |f: &flips::Flip| println!("{}", serde_json::to_string(f).unwrap_or_default());
    for e in futures::executor::block_on_stream(events) {
        match e {
            Event::Text { text } => {
                for f in reading.add(&text, &words, bpb) {
                    say(&f);
                    found.push(f);
                }
            }
            Event::Done { error: Some(e), .. } => return Err(kind.explain(&e).unwrap_or(e)),
            Event::Done { .. } => break,
            _ => {}
        }
    }
    for f in reading.end(&words, bpb) {
        say(&f);
        found.push(f);
    }
    Ok(json!({
        "agent": kind.def().name,
        "seconds": started.elapsed().as_secs(),
        "flips": found.len(),
        "file": heard.file,
    }))
}

/// The agent's `slices`: an audio region's file Smart Sliced (the stretch the region plays, as
/// it is or `unvoiced`), each slice's seconds and what it sounds like; kept, so asking again is
/// instant.
pub fn slices_tool(
    dir: &Path,
    track: usize,
    region: usize,
    unvoiced: bool,
) -> Result<Value, String> {
    use pina_engine::{smart, song::project::Timing};
    let p = crate::cli::project(dir)?;
    let file = region_file(dir, track, region)?;
    let r = p.region((track, region));
    let rate = p.sample_rate as f64;
    let stretch = Timing::of(&r.content, rate).map(|t| {
        let at = |tick: u64| (p.audio_frame(&t, r.start as f64, tick as f64) / rate) as f32;
        (at(r.start), at(r.start + r.length))
    });
    let cut = smart::slice_file(
        dir,
        &file,
        stretch,
        unvoiced,
        false,
        |_| {},
        &AtomicBool::new(false),
    )?;
    Ok(json!({
        "file": cut.file,
        "slices": cut.slices.iter().map(|s| json!({
            "from": s.start, "to": s.end, "name": s.name, "voice": s.voice,
        })).collect::<Vec<_>>(),
    }))
}

/// The agent's `stem_models`: the separation and Whisper models there are, and those to get.
pub fn stem_models_tool() -> Result<Value, String> {
    use pina_engine::lyrics;
    let mut out = models()?;
    let dir = lyrics::folder();
    out["whisper"] = lyrics::installed(&dir)
        .iter()
        .map(|w| json!(w.name))
        .collect::<Vec<_>>()
        .into();
    out["to_get"] = get(None)?["to_get"].clone();
    out["whisper_to_get"] = lyrics::WHISPERS
        .iter()
        .map(|w| json!({"name": w.name, "bytes": w.bytes, "heard": w.heard}))
        .collect::<Vec<_>>()
        .into();
    Ok(out)
}
