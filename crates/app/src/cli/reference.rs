//! References, for the command line, agents and the Song Study page: a song's blueprint (from its
//! audio, by way of its study, or from a picture of its spectrogram), kept so it's made once; a
//! new song set up to be like one, for the agent to write every note of; and a song's render
//! compared with its reference, section by section, with what a revision asked for in words
//! aims at and whether it got there. Files stay on this Mac.

use std::path::{Path, PathBuf};

use pina_engine::{
    music::{Key, arrange},
    song::project::Project,
    study::reference::{
        Blueprint, Calibration, Closeness, Like, Source,
        audio::{self, Shape},
        compare::{self, Measure, Report},
        image,
    },
};
use serde_json::{Value, json};

use crate::cli::study::{cache_dir, file_hash};

/// Where blueprints are kept: beside the studies, in a folder of their own.
fn kept_dir() -> PathBuf {
    cache_dir().join("blueprints")
}

/// The file a blueprint of `path` is kept in: a picture's under its own name (one a picture, the
/// latest reading), audio's by its study's version too.
fn kept_file(path: &Path, picture: bool) -> Option<PathBuf> {
    let tag = match picture {
        true => "pic",
        false => "audio",
    };
    Some(kept_dir().join(format!(
        "{:016x}-{tag}{}.json",
        file_hash(path)?,
        pina_engine::study::reference::VERSION * 100 + pina_engine::study::VERSION
    )))
}

fn keep(path: &Path, picture: bool, bp: &Blueprint) {
    if let Some(f) = kept_file(path, picture) {
        let _ = std::fs::create_dir_all(kept_dir());
        let _ = std::fs::write(f, serde_json::to_vec(bp).unwrap_or_default());
    }
}

fn kept(path: &Path, picture: bool) -> Option<Blueprint> {
    let b = std::fs::read(kept_file(path, picture)?).ok()?;
    serde_json::from_slice::<Blueprint>(&b)
        .ok()
        .filter(|bp| bp.version == pina_engine::study::reference::VERSION)
}

/// A reference's blueprint: a picture's read as `given` says (and kept, with that reading), or
/// audio's from its study (both kept).
pub fn blueprint(path: &Path, given: &Calibration) -> Result<Blueprint, String> {
    if !path.is_file() {
        return Err(format!("{} isn't a file", path.display()));
    }
    if image::is_picture(path) {
        if let Some(bp) = kept(path, true)
            && is_reading(&bp, given)
        {
            return Ok(bp);
        }
        return reread(path, given);
    }
    if let Some(bp) = kept(path, false) {
        return Ok(bp);
    }
    let study = crate::cli::study::study(path, false)?;
    let (mut sides, rate) = pina_engine::audio::engine::read_audio(path)?;
    let l = sides.remove(0);
    let r = sides.pop().unwrap_or_else(|| l.clone());
    let bp = audio::from_audio(
        &l,
        &r,
        rate as f32,
        Source::Audio,
        path.to_path_buf(),
        Some(&Shape::of_study(&study)),
        Some(&study),
    );
    keep(path, false, &bp);
    Ok(bp)
}

/// A picture read afresh as `given` says (what it leaves out found, or unknown), and kept: a
/// reading changed by hand, a value taken out among them.
pub fn reread(path: &Path, given: &Calibration) -> Result<Blueprint, String> {
    let bp = image::from_image(path, given)?;
    keep(path, true, &bp);
    Ok(bp)
}

/// Whether a kept picture's blueprint was read as `given` asks (what it doesn't say, found).
fn is_reading(bp: &Blueprint, given: &Calibration) -> bool {
    let Some(had) = &bp.calibration else {
        return false;
    };
    let same =
        |a: Option<f32>, b: Option<f32>| b.is_none_or(|b| a.is_some_and(|a| (a - b).abs() < 1e-3));
    same(had.seconds, given.seconds)
        && same(had.low_hz, given.low_hz)
        && same(had.high_hz, given.high_hz)
        && same(had.db_range, given.db_range)
        && given.scale.is_none_or(|s| had.scale == Some(s))
        && given.colors.is_none_or(|c| had.colors == Some(c))
        && given.crop.is_none_or(|c| had.crop == Some(c))
}

/// Every picture studied, the latest first.
pub fn pictures() -> Vec<Blueprint> {
    let Ok(entries) = std::fs::read_dir(kept_dir()) else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, Blueprint)> = entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains("-pic"))
        .filter_map(|e| {
            let when = e.metadata().ok()?.modified().ok()?;
            let bp: Blueprint = serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok()?;
            (bp.version == pina_engine::study::reference::VERSION && bp.file.is_file())
                .then_some((when, bp))
        })
        .collect();
    found.sort_by_key(|a| std::cmp::Reverse(a.0));
    found.into_iter().map(|(_, bp)| bp).collect()
}

/// A picture forgotten: its kept reading gone (the picture stays).
pub fn forget_picture(path: &Path) {
    if let Some(f) = kept_file(path, true) {
        let _ = std::fs::remove_file(f);
    }
}

/// The song's tempo, meter, bars (seconds) and sections (its markers) up to `seconds`.
fn shape_of(p: &Project, seconds: f32) -> Shape {
    let rate = p.sample_rate as f64;
    let secs = |tick: u64| (p.samples(tick, rate) as f64 / rate) as f32;
    let bar = p.bar().max(1);
    let bars: Vec<f32> = (0..)
        .map(|i| secs(i * bar))
        .take_while(|&t| t < seconds - 0.05)
        .collect();
    let mut letters: Vec<String> = Vec::new();
    let sections = p
        .markers
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let end = p
                .markers
                .get(i + 1)
                .map_or(seconds, |n| secs(n.tick))
                .min(seconds);
            let base = m
                .name
                .trim_end_matches(|c: char| c.is_ascii_digit() || c == ' ')
                .to_lowercase();
            let k = match letters.iter().position(|b| *b == base) {
                Some(k) => k,
                None => {
                    letters.push(base);
                    letters.len() - 1
                }
            };
            let label = ((b'A' + (k as u8).min(25)) as char).to_string();
            (m.name.clone(), label, secs(m.tick).min(seconds), end)
        })
        .filter(|s| s.3 > s.2 + 0.5)
        .collect();
    Shape {
        bpm: p.tempo as f32,
        meter: [p.time_signature[0] as u32, p.time_signature[1] as u32],
        bars,
        sections,
    }
}

/// The song's render as a blueprint, its tempo, bars and sections as its project has them.
pub fn rendered(p: &Project, dir: &Path) -> Result<Blueprint, String> {
    let [l, r] = crate::cli::rendered(p, dir, None)?;
    if l.len() < p.sample_rate as usize {
        return Err("the song is under a second long: write it first".into());
    }
    let seconds = l.len() as f32 / p.sample_rate as f32;
    let shape = shape_of(p, seconds);
    Ok(audio::from_audio(
        &l,
        &r,
        p.sample_rate as f32,
        Source::Render,
        dir.to_path_buf(),
        Some(&shape),
        None,
    ))
}

/// The person's choices for a song like a reference: a style in their words, and how closely to
/// follow its rhythm, form and sound.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Choices {
    pub title: Option<String>,
    pub style: String,
    pub rhythm: Closeness,
    pub form: Closeness,
    pub sound: Closeness,
}

/// A new song set up to be like a reference, for the agent to write: its tempo, key and meter
/// where the reference says them (else left for the agent to choose), the reference and the
/// choices kept with it, and audio's mix as its mix reference. No notes: those are the agent's.
/// Answers its folder and the blueprint.
pub fn create(
    path: &Path,
    given: &Calibration,
    choices: &Choices,
    root: &Path,
) -> Result<(PathBuf, Blueprint), String> {
    let bp = blueprint(path, given)?;
    let title = choices.title.clone().unwrap_or_else(|| {
        let stem = path
            .file_stem()
            .map_or("Reference".into(), |s| s.to_string_lossy().into_owned());
        format!(
            "Like {}",
            stem.split(['(', '[']).next().unwrap_or("").trim()
        )
    });
    let meter = bp.meter.unwrap_or([4, 4]);
    let dir = crate::pages::projects::create(
        root,
        &title,
        bp.tempo
            .map_or(100.0, |t| (t as f64).round())
            .clamp(20.0, 999.0),
        // A picture's tonal centre is a guess: the agent chooses the key.
        bp.key
            .as_deref()
            .filter(|_| bp.source == Source::Audio)
            .and_then(Key::parse),
        [meter[0] as u64, meter[1] as u64],
        crate::pages::projects::Template::Empty,
    )?;
    let mut p = Project::load(&dir)?;
    p.like = Some(Like {
        file: path.to_path_buf(),
        source: bp.source,
        calibration: bp.calibration.clone(),
        style: choices.style.clone(),
        rhythm: choices.rhythm,
        form: choices.form,
        sound: choices.sound,
    });
    if bp.source == Source::Audio
        && let Ok(s) = crate::cli::study::study(path, false)
    {
        p.reference = Some(crate::pages::study::reference(&s));
    }
    p.save(&dir)?;
    Ok((dir, bp))
}

/// What the agent is asked, to write a song like its reference: the person's choices, the
/// blueprint's brief, how to plan and write it (every note its own), and to compare and revise.
pub fn compose_prompt(dir: &Path, like: &Like, bp: &Blueprint) -> String {
    let how = |c: Closeness| match c {
        Closeness::Loose => "loosely (its spirit)",
        Closeness::Close => "closely (its shape)",
        Closeness::Tight => "tightly (as near as new notes can be)",
    };
    let source = match like.source {
        Source::Image => {
            "a picture of its spectrogram: what the brief marks estimated is a guess, and unknown is yours to choose"
        }
        _ => "its recording",
    };
    let style = match like.style.trim() {
        "" => "yours to choose, near the reference's".to_string(),
        s => s.to_string(),
    };
    // A recording's study says what plays in it and how; its genre is the agent's to hear
    // (the person's playbooks are theirs, read only when they ask).
    let what = match like.source {
        Source::Image => String::new(),
        _ => crate::cli::study::study(&like.file, false)
            .map(|s| {
                let plays: Vec<String> = s
                    .instruments
                    .iter()
                    .filter(|e| e.confidence >= 0.4)
                    .map(|e| format!("{} ({})", e.name, e.role))
                    .collect();
                let tagged = match s.identity.genre.trim() {
                    "" => String::new(),
                    g => format!(" Its tags say {g}."),
                };
                format!(
                    "What the reference is: first `study` its file (online: its tags, genre and \
                     lyrics) and name its genre yourself.{tagged} Heard in it (estimates): {}; \
                     {:.0}% swing; key {} (fit {:.2}), {:.0} BPM, {:.1} LUFS. Its mix is the \
                     song's mix reference already.\n\n",
                    plays.join(", "),
                    s.groove.swing,
                    s.key.name,
                    s.key.fit,
                    s.tempo.bpm,
                    s.mix.overall.lufs,
                )
            })
            .unwrap_or_default(),
    };
    format!(
        "Write a new song in the project at {dir} like the reference {file}, from {source}. \
         Style: {style}. Follow its rhythm {r}, its form {f}, its sound {s}.\n\n{what}\
         Work as a producer, as for any song you write here:\n\
         1. Plan: its sections (bars and energy, from the brief's form), the parts each section \
         has (fewer where its energy is low), the motifs, and a chord progression of your own. \
         The brief's chords are a machine's guesses from the audio and often wrong: take their \
         mood and how often they change, never copy them, and keep the progression in the \
         song's key unless a section's pitch classes plainly leave it.\n\
         2. Sounds: real instruments for its genre and parts (`sounds` with the genre and each \
         role, the library's samples, the person's palette), nothing placeholder, and a kit \
         whose drums suit the genre.\n\
         3. Write the whole song, every note yourself: new parts and notes, never a copy of the \
         reference's (a reference, not a source), with fills, variation and transitions between \
         sections. Build the drums on each section's `steps`, and the bass and the melody on its \
         `bass` and `melody` (estimated from the reference: their rhythm and the way they move \
         to write your own on, in the song's key), as closely as its rhythm says. Mark each section (add_marker) with its name, so the comparison lines them \
         up. `critique` (quick) after each part and fix what it says.\n\
         4. Finish it: `critique` with a render and fix it, then `mix` and `master` toward the \
         reference.\n\
         5. Then `reference_compare` with the project, and fix its biggest differences, at most \
         two passes, changing only what they name.\n\n\
         The reference's brief:\n{brief}",
        dir = dir.display(),
        file = like.file.display(),
        r = how(like.rhythm),
        f = how(like.form),
        s = how(like.sound),
        brief = serde_json::to_string_pretty(&bp.brief()).unwrap_or_default(),
    )
}

/// The project's last comparison, kept to judge the next by.
fn last_file(dir: &Path) -> PathBuf {
    let full = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in full.as_os_str().as_encoded_bytes() {
        h = (h ^ *b as u64).wrapping_mul(0x100_0000_01b3);
    }
    kept_dir().join(format!("like-{h:016x}.json"))
}

/// The song (as `p` has it) rendered and compared with its reference.
pub fn report(p: &Project, dir: &Path) -> Result<(Report, Blueprint, Blueprint), String> {
    if p.tracks.iter().all(|t| t.regions.is_empty()) {
        return Err("the song has nothing in it yet: write it, then compare".into());
    }
    let like = p.like.clone().ok_or(
        "the song has no reference to be like: make it with Create a song like this, or \
         reference_create",
    )?;
    let reference = blueprint(&like.file, &like.calibration.clone().unwrap_or_default())?;
    let song = rendered(p, dir)?;
    Ok((compare::compare(&reference, &song), reference, song))
}

/// The song's bars (from 1) a span of its render's frames covers.
fn bars_of(song: &Blueprint, span: (usize, usize)) -> Option<(usize, usize)> {
    if song.bars.is_empty() {
        return None;
    }
    let at = |f: usize| {
        song.bars
            .iter()
            .rposition(|&b| b <= f as f32 + 0.5)
            .unwrap_or(0)
    };
    Some((at(span.0) + 1, at(span.1.saturating_sub(1)) + 1))
}

/// The song's tracks that play the parts named (by what each plays).
fn tracks_for(p: &Project, parts: &[String]) -> Vec<Value> {
    let roles: Vec<&str> = parts
        .iter()
        .flat_map(|part| match part.as_str() {
            "kick" | "snare" | "hats" | "clap" | "cymbals" | "drums" => &["drums", "perc"][..],
            "shaker" | "perc" => &["perc", "drums"][..],
            "bass" | "808" => &["bass"][..],
            "chords" | "keys" => &["chords", "keys"][..],
            "pad" => &["pad"][..],
            "lead" => &["lead", "arp", "counter"][..],
            "vocals" => &["vocals"][..],
            _ => &[][..],
        })
        .copied()
        .collect();
    p.tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| t.instrument.is_some() || !t.regions.is_empty())
        .filter(|(_, t)| roles.contains(&arrange::role(t)))
        .map(|(i, t)| json!({ "track": i, "name": t.name }))
        .collect()
}

/// A comparison as an agent or a person reads it: each section's likeness by measure, with its
/// bars in the song; the differences, biggest first, with the tracks to change. With `request`
/// ("closer drums", "brighter chorus"), what it aims at and how the song moved since the last
/// comparison: reached, better (another pass may help) or stalled (stop).
pub fn compare(dir: &Path, request: Option<&str>) -> Result<Value, String> {
    let p = crate::cli::project(dir)?;
    let (report, reference, song) = report(&p, dir)?;
    let before: Option<Report> = std::fs::read(last_file(dir))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let _ = std::fs::create_dir_all(kept_dir());
    let _ = std::fs::write(
        last_file(dir),
        serde_json::to_vec(&report).unwrap_or_default(),
    );
    let mut out = describe(&report, &reference, &song, &p);
    if let Some(request) = request.filter(|r| !r.trim().is_empty()) {
        let t = compare::target(request, &report);
        out["target"] = json!({
            "request": request,
            "measures": t.measures.iter().map(|m| m.name()).collect::<Vec<_>>(),
            "sections": t.sections.iter().filter_map(|&i| report.sections.get(i).map(|s| s.name.clone())).collect::<Vec<_>>(),
            "likeness": compare::score(&report, &t),
        });
        if let Some(before) = before {
            let pr = compare::progress(&before, &report, &t);
            out["progress"] = json!(pr);
            out["next"] = json!(match pr.verdict {
                compare::Verdict::Reached => "Reached: stop revising this.",
                compare::Verdict::Better =>
                    "Better: another pass on the same differences may help.",
                compare::Verdict::Stalled =>
                    "No better: stop, and say so; undo the pass if the rest got worse.",
            });
        }
    }
    Ok(out)
}

/// A report as JSON, with the song's bars and tracks.
pub fn describe(report: &Report, reference: &Blueprint, song: &Blueprint, p: &Project) -> Value {
    let round = |v: Option<f32>| v.map(|v| (v * 100.0).round() / 100.0);
    let scores = |s: &compare::Scores| {
        json!({
            "groove": round(s.groove), "tonal": round(s.tonal),
            "balance": round(s.balance), "energy": round(s.energy),
        })
    };
    let bars = |span: (usize, usize)| bars_of(song, span).map(|(a, b)| format!("{a}-{b}"));
    let sections: Vec<Value> = report
        .sections
        .iter()
        .map(|s| {
            json!({
                "name": s.name, "letter": s.label, "bars": bars(s.song),
                "likeness": scores(&s.scores),
                "energy_db": (s.energy_db * 10.0).round() / 10.0,
                "balance_db": s.balance_db.map(|b| b.map(|v| (v * 10.0).round() / 10.0)),
            })
        })
        .collect();
    let differences: Vec<Value> = report
        .differences
        .iter()
        .map(|d| {
            let s = &report.sections[d.section];
            json!({
                "section": s.name, "bars": bars(s.song), "measure": d.measure.name(),
                "size": (d.size * 100.0).round() / 100.0, "says": d.says,
                "parts": d.parts, "tracks": tracks_for(p, &d.parts), "ask": d.ask,
            })
        })
        .collect();
    json!({
        "reference": reference.file,
        "from": reference.source.name(),
        "relative": report.relative,
        "likeness": scores(&report.overall),
        "sections": sections,
        "differences": differences,
        "note": match report.relative {
            true => "The reference is a picture with no level scale: every measure is relative, and \
                     balance isn't compared without its frequency axis.",
            false => "Every measure is relative to each song's own whole, so levels don't need to match.",
        },
    })
}

/// What the agent is asked, to revise toward the reference as the person asked: the differences
/// the request aims at, the bars and tracks they're in, and to change nothing else.
pub fn revise_prompt(
    dir: &Path,
    p: &Project,
    request: &str,
    t: &compare::Target,
    report: &Report,
    song: &Blueprint,
) -> String {
    let aimed: Vec<String> = report
        .differences
        .iter()
        .filter(|d| t.measures.contains(&d.measure) && t.sections.contains(&d.section))
        .map(|d| {
            let s = &report.sections[d.section];
            let bars =
                bars_of(song, s.song).map_or(String::new(), |(a, b)| format!(" (bars {a}-{b})"));
            let tracks = tracks_for(p, &d.parts)
                .iter()
                .filter_map(|t| Some(format!("{} \"{}\"", t["track"], t["name"].as_str()?)))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "- {}{bars}: {} {}{}",
                s.name,
                d.says,
                d.ask,
                match tracks.is_empty() {
                    true => String::new(),
                    false => format!(" Tracks: {tracks}."),
                }
            )
        })
        .collect();
    let names: Vec<&str> = t.measures.iter().map(|m| m.name()).collect();
    let sections: Vec<&str> = t
        .sections
        .iter()
        .filter_map(|&i| report.sections.get(i).map(|s| s.name.as_str()))
        .collect();
    format!(
        "Revise the song at {dir} toward its reference, as I asked: \"{request}\". This aims at its \
         {measures} in {sections}.\n\n{list}\n\nChange only what these name (those sections, those \
         parts); leave every other track and bar as it is, including anything I've edited. Write \
         new notes, don't copy the reference. Then call `reference_compare` with the project and \
         `request` \"{request}\", and say what moved.",
        dir = dir.display(),
        measures = names.join(" and "),
        sections = sections.join(", "),
        list = match aimed.is_empty() {
            true => "The comparison finds no big difference there now; make the change asked for, \
                     gently."
                .to_string(),
            false => aimed.join("\n"),
        },
    )
}

/// What a measure is called in a sentence.
pub fn measure_word(m: Measure) -> &'static str {
    match m {
        Measure::Groove => "groove",
        Measure::Tonal => "notes and held lines",
        Measure::Balance => "balance",
        Measure::Energy => "energy",
    }
}

/// A calibration from command-line flags: `--crop x,y,w,h`, `--seconds 180`, `--hz 20-22050`,
/// `--scale log`, `--colors magma`, `--db 80`.
pub fn calibration_from_args(args: &[&str]) -> Result<Calibration, String> {
    let mut c = Calibration::default();
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1).copied();
        let need = || value.ok_or(format!("{} needs a value", args[i]));
        match args[i] {
            "--crop" => {
                let v: Vec<u32> = need()?
                    .split(',')
                    .filter_map(|x| x.trim().parse().ok())
                    .collect();
                c.crop = Some(
                    v.try_into()
                        .map_err(|_| "--crop is x,y,width,height".to_string())?,
                );
            }
            "--seconds" => c.seconds = need()?.parse().ok(),
            "--hz" => {
                let v = need()?;
                let (a, b) = v.split_once('-').ok_or("--hz is low-high, like 20-22050")?;
                (c.low_hz, c.high_hz) = (a.trim().parse().ok(), b.trim().parse().ok());
            }
            "--scale" => {
                c.scale = Some(
                    pina_engine::study::reference::FreqScale::parse(need()?)
                        .ok_or("--scale is linear, log or mel")?,
                )
            }
            "--colors" | "--colours" => {
                c.colors = Some(pina_engine::study::reference::Colors::parse(need()?).ok_or(
                    "--colors is gray, gray_inverted, viridis, magma, inferno, plasma or jet",
                )?)
            }
            "--db" => c.db_range = need()?.parse().ok(),
            _ => {
                i += 1;
                continue;
            }
        }
        i += 2;
    }
    Ok(c)
}

/// A calibration from JSON (an agent's call): `crop` [x, y, w, h], `seconds`, `low_hz`,
/// `high_hz`, `scale`, `colors`, `db_range`.
pub fn calibration_from_json(v: &Value) -> Result<Calibration, String> {
    serde_json::from_value(json!({
        "crop": v.get("crop"),
        "seconds": v.get("seconds"),
        "low_hz": v.get("low_hz"),
        "high_hz": v.get("high_hz"),
        "scale": v.get("scale"),
        "colors": v.get("colors"),
        "db_range": v.get("db_range"),
    }))
    .map_err(|e| format!("the picture's reading: {e}"))
}

/// A blueprint as an agent reads it: its brief, its reading (a picture's), and what's needed.
pub fn for_agent(bp: &Blueprint) -> Value {
    let mut out = bp.brief();
    if let Some(c) = &bp.calibration {
        out["reading"] = json!(c);
    }
    out
}
