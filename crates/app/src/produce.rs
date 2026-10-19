//! A producer's tools for agents: the critic, genre playbooks, chord suggestions, a reference
//! song to aim at, sounds for a part, a mix by role, and a master to a loudness. Edits go through
//! `cli::edit_with`, so they land live in the app when it has the song open.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tempo_engine::{
    analysis,
    arrange::role,
    commands::{AutoParam, ChannelRef, Command, ParamRef, PointIn},
    critique::{self, Heard},
    music::Key,
    playbook,
    project::{Format, Instrument, PluginRef, Preset, PresetSource, Project, Reference, Which},
    scan,
    theory::{self, PROGRESSIONS},
};

use crate::cli;

/// What's wrong with a song and how to fix it: from its notes and layout, and (unless `quick`)
/// a render of it, over bars `range` or all of it.
pub fn critique(dir: &Path, quick: bool, range: Option<(u64, u64)>) -> Result<Value, String> {
    let p = cli::project(dir)?;
    let issues = if quick {
        critique::critique(&p, None)
    } else {
        let ticks = range.map(|(a, b)| cli::bars(&p, a, b)).transpose()?;
        let report = cli::report(&p, dir, ticks, true)?;
        let sections: Vec<(String, analysis::Analysis)> = report
            .sections
            .iter()
            .map(|(n, _, a)| (n.clone(), a.clone()))
            .collect();
        critique::critique(
            &p,
            Some(&Heard {
                mix: &report.mix,
                tracks: &report.tracks,
                sections: &sections,
            }),
        )
    };
    let count = |l: &str| issues.iter().filter(|i| i.level == l).count();
    let verdict = match (count("fix"), count("improve")) {
        (0, 0) => {
            "nothing to fix: listen to it (render) and trust your ears over the numbers".to_string()
        }
        (0, n) => format!("nothing wrong, {n} things between it and a finished record"),
        (f, n) => format!("{f} things sound wrong (fix them first), {n} more to improve"),
    };
    Ok(json!({
        "verdict": verdict,
        "genre": p.genre,
        "heard": !quick,
        "issues": issues,
    }))
}

/// A genre's playbook, or every genre's id and name.
pub fn playbook(genre: Option<&str>) -> Result<Value, String> {
    match genre {
        None => {
            let (all, problems) = playbook::load();
            let genres: Vec<Value> = all
                .iter()
                .map(|b| json!({ "id": b.id, "name": b.name, "tempo": b.tempo }))
                .collect();
            let mut out = json!({ "genres": genres, "edit": playbook::dir() });
            if !problems.is_empty() {
                out["unreadable"] = json!(problems);
            }
            Ok(out)
        }
        Some(g) => {
            let b = playbook::find(g).ok_or(format!(
                "no playbook for {g:?}; there are {}",
                playbook::ids().join(", ")
            ))?;
            let key = Key {
                root: 9,
                minor: b.minor,
            };
            let mut out = serde_json::to_value(&b).map_err(|e| e.to_string())?;
            out["progressions"] = json!(suggestions(&key, &b.harmony, 4));
            if let Some(d) = playbook::dir() {
                out["file"] = json!(d.join(format!("{}.json", b.id)));
            }
            out["how"] = json!(WORKFLOW);
            Ok(out)
        }
    }
}

/// Opens the playbooks folder (made the first time, a copy of tempo's), or one genre's file.
pub fn edit_playbooks(genre: Option<&str>) -> Result<(), String> {
    let (_, problems) = playbook::load();
    let dir = playbook::dir().ok_or("no home folder")?;
    let target = match genre {
        None => dir,
        Some(g) => {
            let id = playbook::find(g).map_or(g.to_string(), |b| b.id);
            let file = dir.join(format!("{id}.json"));
            if !file.exists() {
                return Err(format!("no {id}.json in {}", dir.display()));
            }
            file
        }
    };
    for p in problems {
        eprintln!("{p}");
    }
    let mut open = std::process::Command::new("open");
    if target.is_file() {
        open.arg("-t");
    }
    open.arg(&target).status().map_err(|e| e.to_string())?;
    println!("{}", target.display());
    Ok(())
}

/// The library's progressions that suit `words`, best first, written in `key`.
fn suggestions(key: &Key, words: &str, n: usize) -> Vec<Value> {
    let words: Vec<String> = words
        .to_lowercase()
        .replace('-', "_")
        .split([' ', ','])
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    let mut ranked: Vec<(usize, &theory::Progression)> = PROGRESSIONS
        .iter()
        .filter(|p| p.minor == key.minor)
        .map(|p| {
            (
                p.suits
                    .iter()
                    .filter(|s| {
                        words
                            .iter()
                            .any(|w| s.starts_with(w.as_str()) || w.starts_with(**s))
                    })
                    .count(),
                p,
            )
        })
        .collect();
    ranked.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    ranked
        .into_iter()
        .take(n)
        .map(|(_, p)| {
            let chords: Vec<String> = p
                .numerals
                .split('|')
                .map(|bar| {
                    bar.split_whitespace()
                        .filter_map(|c| theory::numeral(c, key).map(|c| c.name_in(Some(key))))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect();
            json!({ "numerals": p.numerals, "chords": chords.join(" | "), "suits": p.suits })
        })
        .collect()
}

/// Chords for a key: progressions that suit the words, and the key's own chords by degree.
pub fn theory(key: &str, words: &str) -> Result<Value, String> {
    let k = Key::parse(key).ok_or(format!(
        "{key:?} isn't a key: try \"A minor\" or \"Eb major\""
    ))?;
    let degrees: &[&str] = if k.minor {
        &["i7", "iiø7", "IIImaj7", "iv7", "v7", "VImaj7", "VII7"]
    } else {
        &["Imaj7", "ii7", "iii7", "IVmaj7", "V7", "vi7", "viiø7"]
    };
    let own: Vec<String> = degrees
        .iter()
        .filter_map(|d| theory::numeral(d, &k).map(|c| format!("{d} {}", c.name_in(Some(&k)))))
        .collect();
    let notes: Vec<&str> = (0..12u8)
        .filter(|p| k.contains(*p))
        .map(|p| {
            [
                "C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
            ][p as usize]
        })
        .collect();
    Ok(json!({
        "key": k.name(),
        "scale": notes,
        "chords": own,
        "progressions": suggestions(&k, words, 6),
        "write": "set_chords with the numerals or chords; then write_chords, write_bass and write_melody follow them",
    }))
}

/// Measures a finished song to aim at; with a project, sets it as the song's reference and says
/// how the song differs.
pub fn reference(file: &Path, dir: Option<&Path>) -> Result<Value, String> {
    let (l, r, rate) = cli::audio(file)?;
    let a = analysis::analyze(&l, &r, rate);
    let s = &a.spectrum_db;
    let reference = Reference {
        file: file.to_path_buf(),
        lufs: a.lufs,
        crest_db: a.crest_db,
        spectrum: [s.sub, s.bass, s.low_mid, s.mid, s.presence, s.air],
        width_db: a.stereo.width_db,
        key: a.notes.key.clone(),
    };
    let mut out = json!({ "reference": reference });
    if let Some(dir) = dir {
        cli::edit_with(
            dir,
            Command::SetReference {
                reference: Some(reference.clone()),
            },
        )?;
        let p = cli::project(dir)?;
        let song = cli::report(&p, dir, None, false)?.mix;
        let t = &song.spectrum_db;
        let mine = [t.sub, t.bass, t.low_mid, t.mid, t.presence, t.air];
        let names = ["sub", "bass", "low_mid", "mid", "presence", "air"];
        let bands: serde_json::Map<String, Value> = names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                (
                    n.to_string(),
                    json!(((mine[i] - reference.spectrum[i]) * 10.0).round() / 10.0),
                )
            })
            .collect();
        out["song_minus_reference"] = json!({
            "lufs": ((song.lufs - reference.lufs) * 10.0).round() / 10.0,
            "crest_db": ((song.crest_db - reference.crest_db) * 10.0).round() / 10.0,
            "width_db": ((song.stereo.width_db - reference.width_db) * 10.0).round() / 10.0,
            "bands_db": bands,
        });
        out["set"] = json!("it's the song's reference now: critique aims the mix at it");
    }
    Ok(out)
}

/// Words that say a preset is for a part, in its name or the folders it's in: how plugins file
/// them ("Basses/", "Pads/", "KEY Rhodes", "LD Saw").
fn role_words(role: &str) -> &'static [&'static str] {
    match role {
        "bass" => &["bass", "808", "sub", "reese", "bs "],
        "chords" => &[
            "key", "piano", "ep ", "rhodes", "wurli", "organ", "chord", "poly", "guitar", "stab",
            "clav",
        ],
        "pad" => &[
            "pad", "atmo", "ambient", "string", "choir", "drone", "swell", "texture",
        ],
        "lead" => &[
            "lead", "ld ", "pluck", "bell", "brass", "horn", "flute", "wind", "sax", "whistle",
            "solo",
        ],
        "counter" | "arp" => &["arp", "seq", "pluck", "bell", "mallet", "marimba", "rhythm"],
        "fx" => &["fx", "sfx", "riser", "noise", "sweep", "impact"],
        "texture" => &["atmo", "texture", "noise", "drone", "ambient"],
        "drums" | "perc" => &["drum", "kit", "perc", "kick", "snare", "hat", "tom"],
        "vocals" => &["vox", "vocal", "choir", "voice", "vocoder"],
        _ => &[],
    }
}

/// How well a preset suits a part: the part's words in its name or folders count most, then
/// the words asked for.
fn fit(preset: &Preset, role: &str, wanted: &[String]) -> usize {
    let mut text = format!("{} ", preset.name.to_lowercase());
    if let PresetSource::File { file } = &preset.source {
        // The folders it's filed in, nearest first.
        for part in file.iter().rev().skip(1).take(3) {
            text.push_str(&part.to_string_lossy().to_lowercase());
            text.push(' ');
        }
    }
    let part = role_words(role)
        .iter()
        .filter(|w| text.contains(*w))
        .count();
    let asked = wanted
        .iter()
        .filter(|w| w.len() > 2 && text.contains(w.as_str()))
        .count();
    part * 3 + asked
}

fn presets_cache() -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_default();
    Path::new(&home).join("Library/Caches/tempo/presets.json")
}

/// Each instrument's presets, as the sounds tool lists them: those kept from before at once, and
/// with `ask` the rest asked of each instrument (a process each, so it can take a while), kept for
/// next time.
pub fn presets_of(plugins: &[scan::PluginInfo], ask: bool) -> Vec<Vec<Preset>> {
    let file = presets_cache();
    let mut cache: serde_json::Map<String, Value> = std::fs::read(&file)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let out = plugins
        .iter()
        .map(|p| match ask {
            true => instrument_presets(p, &mut cache),
            false => {
                let key = format!(
                    "{:?}|{}|{}|{}",
                    p.format,
                    p.path.display(),
                    p.class,
                    p.uid.as_deref().unwrap_or("")
                );
                cache
                    .get(&key)
                    .and_then(|k| serde_json::from_value(k["presets"].clone()).ok())
                    .unwrap_or_default()
            }
        })
        .collect();
    if ask {
        let _ = std::fs::create_dir_all(file.parent().unwrap_or(Path::new(".")));
        let _ = std::fs::write(&file, serde_json::to_vec(&cache).unwrap_or_default());
    }
    out
}

/// An instrument's presets (its own programs and its files), as it lists them loaded on its
/// own: in a process of its own, so one that crashes or hangs doesn't take tempo with it. Kept
/// until the plugin changes.
fn instrument_presets(
    p: &scan::PluginInfo,
    cache: &mut serde_json::Map<String, Value>,
) -> Vec<Preset> {
    let key = format!(
        "{:?}|{}|{}|{}",
        p.format,
        p.path.display(),
        p.class,
        p.uid.as_deref().unwrap_or("")
    );
    let modified = std::fs::metadata(&p.path)
        .and_then(|m| m.modified())
        .map_or(0, |t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
        });
    if let Some(kept) = cache.get(&key).filter(|k| k["modified"] == json!(modified))
        && let Ok(list) = serde_json::from_value(kept["presets"].clone())
    {
        return list;
    }
    let listed = (|| -> Option<Vec<Preset>> {
        let exe = std::env::current_exe().ok()?;
        let mut child = std::process::Command::new(exe)
            .arg("__presets")
            .arg(serde_json::to_string(&p.plugin_ref()).ok()?)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()?;
        // Read as it writes (a long list fills the pipe), and give up on one that hangs.
        let mut stdout = child.stdout.take()?;
        let reader = std::thread::spawn(move || {
            let mut out = String::new();
            std::io::Read::read_to_string(&mut stdout, &mut out).map(|_| out)
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while child.try_wait().ok()?.is_none() {
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        serde_json::from_str(&reader.join().ok()?.ok()?).ok()
    })();
    // One that couldn't be asked is asked again next time.
    let Some(listed) = listed else {
        return Vec::new();
    };
    cache.insert(key, json!({ "modified": modified, "presets": listed }));
    listed
}

/// Sounds for a part: every installed instrument's presets that suit it, a few from each, and
/// samples from the library, for `role` in `genre`, matching `words`; with `track`, the
/// commands are for it.
pub fn sounds(
    role_name: &str,
    genre: Option<&str>,
    words: &str,
    dir: Option<&Path>,
    track: Option<usize>,
) -> Result<Value, String> {
    let book = genre.and_then(playbook::find);
    let from_book = book
        .as_ref()
        .and_then(|b| b.parts.iter().find(|p| p.role == role_name))
        .map_or(String::new(), |p| p.sound.clone());
    let wanted: Vec<String> = format!("{words} {from_book}")
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let plugins = scan::scan(&exe, &scan::everything(), &scan::cache_file());
    // Each instrument once, in the format it plays best in here.
    let rank = |f: Format| match f {
        Format::Clap => 0,
        Format::Vst3 => 1,
        Format::Au => 2,
        Format::Vst2 => 3,
    };
    let mut chosen: Vec<&scan::PluginInfo> = Vec::new();
    for p in plugins.iter().filter(|p| p.instrument) {
        match chosen.iter_mut().find(|c| c.class == p.class) {
            Some(c) if rank(p.format) < rank(c.format) => *c = p,
            Some(_) => {}
            None => chosen.push(p),
        }
    }
    let file = presets_cache();
    let mut cache: serde_json::Map<String, Value> = std::fs::read(&file)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let t = track.unwrap_or(0);
    let mut instruments: Vec<(usize, Value)> = chosen
        .iter()
        .map(|p| {
            let all = instrument_presets(p, &mut cache);
            let mut fitting: Vec<(usize, &Preset)> = all
                .iter()
                .map(|x| (fit(x, role_name, &wanted), x))
                .filter(|(f, _)| *f > 0)
                .collect();
            fitting.sort_by_key(|(f, _)| std::cmp::Reverse(*f));
            let best = fitting.first().map_or(0, |f| f.0);
            // A few from each, so no one instrument's library crowds out the rest.
            let presets: Vec<Value> = fitting
                .iter()
                .take(6)
                .map(|(_, x)| json!({ "name": x.name, "load": { "op": "set_preset", "channel": { "track": t }, "plugin": "instrument", "preset": x.source } }))
                .collect();
            let v = json!({
                "name": p.class,
                "vendor": p.vendor,
                "presets_in_all": all.len(),
                "set_instrument": { "op": "set_instrument", "track": t, "instrument": Instrument::Plugin(p.plugin_ref()) },
                "presets": presets,
            });
            (best, v)
        })
        .collect();
    instruments.sort_by_key(|(best, _)| std::cmp::Reverse(*best));
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
        let _ = std::fs::write(&file, serde_json::to_vec(&cache).unwrap_or_default());
    }
    // The library's search wants few words: what's asked, else the genre's first sound, else
    // the part.
    let first = from_book.split(',').next().unwrap_or("").trim().to_string();
    let samples = [words.trim(), first.as_str(), role_name]
        .into_iter()
        .filter(|q| !q.is_empty())
        .find_map(|q| {
            let query = tempo_engine::samples::search::Query {
                text: q.into(),
                limit: 8,
                ..Default::default()
            };
            crate::samples_cli::find(query, dir)
                .ok()
                .filter(|v| v.as_array().is_some_and(|a| !a.is_empty()))
        })
        .unwrap_or(json!([]));
    Ok(json!({
        "role": role_name,
        "looking_for": from_book,
        "instruments": instruments.into_iter().map(|(_, v)| v).collect::<Vec<_>>(),
        "samples": samples,
        "how": "an instrument's set_instrument on the track, then one of its presets' load; or a sample with samples_put. Render a bar or two and try another if it isn't right. `presets` lists everything an instrument has.",
    }))
}

/// A channel strip for a part: EQ, compressor, saturator, pan, and how much reverb.
struct Strip {
    low_cut: f32,
    low_mid: (f32, f32, f32),
    high_mid: (f32, f32, f32),
    shelf: (f32, f32),
    comp: Option<(f32, f32, f32, f32)>,
    sat: Option<(f32, f32, f32)>,
    /// Loudness alone, LUFS (a bass's RMS, as LUFS hardly hears sub), against drums at -20:
    /// the mix leaves the master room.
    lufs: f32,
    pan: f32,
    reverb: Option<f32>,
}

fn strip_for(role: &str) -> Strip {
    let s = |low_cut, low_mid, high_mid, shelf, comp, sat, lufs, pan, reverb| Strip {
        low_cut,
        low_mid,
        high_mid,
        shelf,
        comp,
        sat,
        lufs,
        pan,
        reverb,
    };
    match role {
        // The saturator soft-clips the hits a compressor lets through.
        "drums" => s(
            30.0,
            (400.0, -2.0, 1.0),
            (5000.0, 0.0, 1.0),
            (10000.0, 0.0),
            Some((-18.0, 4.0, 8.0, 90.0)),
            Some((6.0, 0.0, 0.7)),
            -20.0,
            0.0,
            None,
        ),
        "perc" => s(
            150.0,
            (400.0, -1.5, 1.0),
            (4000.0, 1.0, 1.0),
            (10000.0, 1.0),
            Some((-18.0, 2.5, 10.0, 80.0)),
            None,
            -28.0,
            0.35,
            Some(-16.0),
        ),
        // Body at 100 Hz and harmonics from the saturator, so it's heard on small speakers and
        // fills 60-250 Hz rather than living in the sub alone.
        "bass" => s(
            28.0,
            (100.0, 2.5, 0.8),
            (800.0, 2.0, 1.0),
            (8000.0, -2.0),
            Some((-18.0, 4.0, 10.0, 120.0)),
            Some((9.0, 0.15, 0.55)),
            -21.0,
            0.0,
            None,
        ),
        "chords" => s(
            150.0,
            (350.0, -2.5, 1.0),
            (3000.0, -1.0, 1.0),
            (10000.0, 0.0),
            Some((-20.0, 2.5, 20.0, 150.0)),
            Some((3.0, -0.1, 0.35)),
            -25.0,
            0.0,
            Some(-14.0),
        ),
        "pad" => s(
            200.0,
            (400.0, -3.0, 0.9),
            (2500.0, -2.0, 1.0),
            (10000.0, 0.0),
            Some((-22.0, 2.0, 30.0, 200.0)),
            None,
            -29.0,
            0.0,
            Some(-9.0),
        ),
        "lead" => s(
            180.0,
            (300.0, -2.0, 1.0),
            (3000.0, 1.0, 1.0),
            (10000.0, 1.0),
            Some((-18.0, 3.0, 5.0, 100.0)),
            Some((2.0, 0.1, 0.25)),
            -25.0,
            0.0,
            Some(-12.0),
        ),
        "counter" | "arp" => s(
            200.0,
            (350.0, -2.0, 1.0),
            (2000.0, -1.0, 1.0),
            (10000.0, 0.5),
            Some((-20.0, 2.5, 10.0, 120.0)),
            None,
            -28.0,
            -0.35,
            Some(-10.0),
        ),
        "vocals" => s(
            90.0,
            (300.0, -2.0, 1.0),
            (3500.0, 2.0, 1.0),
            (12000.0, 2.0),
            Some((-20.0, 4.0, 3.0, 80.0)),
            Some((2.0, 0.1, 0.2)),
            -22.0,
            0.0,
            Some(-13.0),
        ),
        "fx" => s(
            60.0,
            (400.0, 0.0, 1.0),
            (3000.0, 0.0, 1.0),
            (10000.0, 0.0),
            None,
            None,
            -28.0,
            0.0,
            Some(-12.0),
        ),
        "texture" => s(
            200.0,
            (400.0, -2.0, 1.0),
            (3000.0, -2.0, 1.0),
            (8000.0, -3.0),
            None,
            None,
            -36.0,
            0.0,
            None,
        ),
        _ => s(
            120.0,
            (350.0, -2.0, 1.0),
            (3000.0, 0.0, 1.0),
            (10000.0, 0.0),
            Some((-20.0, 2.5, 15.0, 120.0)),
            None,
            -26.0,
            0.0,
            Some(-14.0),
        ),
    }
}

const REVERB: (&str, &str) = ("AUReverb2", "aufx rvb2 appl");
const LIMITER: (&str, &str) = ("AUPeakLimiter", "aufx lmtr appl");

fn au(class: &str, uid: &str) -> PluginRef {
    PluginRef {
        format: Format::Au,
        class: class.into(),
        uid: Some(uid.into()),
        ..Default::default()
    }
}

fn param(channel: ChannelRef, plugin: Which, name: &str, text: &str) -> Command {
    Command::SetPluginParam {
        channel,
        plugin,
        param: ParamRef::Name(name.into()),
        value: None,
        text: Some(text.into()),
    }
}

/// The song's reverb aux, "Verb", made (a plate-like AUReverb2, fully wet, its low end cut) if
/// there's none. Its bus.
fn reverb_bus(dir: &Path, p: &Project, first: usize) -> Result<u32, String> {
    if let Some(a) = p.auxes.iter().find(|a| a.name == "Verb") {
        return Ok(a.bus);
    }
    let made = cli::edit_with(
        dir,
        Command::AddSend {
            track: first,
            bus: None,
            level_db: -60.0,
            pre: false,
        },
    )?;
    let bus = made["bus"].as_u64().ok_or("no bus made")? as u32;
    let p = cli::project(dir)?;
    let aux = p
        .auxes
        .iter()
        .position(|a| a.bus == bus)
        .ok_or("no aux made")?;
    let ch = ChannelRef::Aux(aux);
    let edits = [
        Command::SetAux {
            aux,
            name: Some("Verb".into()),
            volume_db: Some(0.0),
            pan: None,
            mute: None,
        },
        Command::SetInsert {
            channel: ch,
            index: 0,
            plugin: Some(au(REVERB.0, REVERB.1)),
        },
        param(ch, Which::Insert(0), "Dry/Wet Mix", "100"),
        param(ch, Which::Insert(0), "Low Freq Decay Time", "1.8"),
        param(ch, Which::Insert(0), "High Freq Decay Time", "1.0"),
        Command::SetEq {
            channel: ch,
            eq: Some(json!({ "low_cut": 250, "high_shelf": { "freq": 8000, "gain_db": -3 } })),
        },
    ];
    for e in edits {
        cli::edit_with(dir, e)?;
    }
    Ok(bus)
}

/// Mixes tracks by their parts (all, or `only`): EQ, compression, saturation, pan, a reverb send,
/// then (unless `levels` is off) each one's level, measured alone, to where its part sits.
pub fn mix_role(
    dir: &Path,
    only: Option<usize>,
    as_role: Option<&str>,
    levels: bool,
) -> Result<Value, String> {
    let p = cli::project(dir)?;
    let tracks: Vec<usize> = match only {
        Some(t) if t < p.tracks.len() => vec![t],
        Some(t) => return Err(format!("no track {t}")),
        None => (0..p.tracks.len())
            .filter(|&t| !p.tracks[t].regions.is_empty())
            .collect(),
    };
    if tracks.is_empty() {
        return Err("no tracks with anything on them to mix".into());
    }
    let lofi = p
        .genre
        .as_deref()
        .is_some_and(|g| matches!(g, "lofi" | "boom_bap" | "neo_soul"));
    // Where the 808 or log drum carries the song, the melodic parts sit under it.
    let sub_led = p
        .genre
        .as_deref()
        .is_some_and(|g| matches!(g, "trap" | "drill" | "amapiano" | "reggaeton"));
    let mut done = Vec::new();
    let mut side = 1.0f32;
    let mut bus = None;
    for &t in &tracks {
        let r = as_role.unwrap_or_else(|| role(&p.tracks[t]));
        let s = strip_for(r);
        let ch = ChannelRef::Track(t);
        let shelf = if lofi && !matches!(r, "drums" | "perc") {
            s.shelf.1 - 3.0
        } else {
            s.shelf.1
        };
        cli::edit_with(
            dir,
            Command::SetEq {
                channel: ch,
                eq: Some(json!({
                    "low_cut": s.low_cut,
                    "low_mid": { "freq": s.low_mid.0, "gain_db": s.low_mid.1, "q": s.low_mid.2 },
                    "high_mid": { "freq": s.high_mid.0, "gain_db": s.high_mid.1, "q": s.high_mid.2 },
                    "high_shelf": { "freq": s.shelf.0, "gain_db": shelf },
                })),
            },
        )?;
        cli::edit_with(dir, Command::SetComp { channel: ch, comp: s.comp.map(|(threshold, ratio, attack, release)| json!({
            "threshold_db": threshold, "ratio": ratio, "attack_ms": attack, "release_ms": release, "makeup_db": 0, "rms": r == "pad",
        })) })?;
        let sat = s.sat.map(|(drive, tone, mix)| json!({ "drive_db": if lofi { drive + 3.0 } else { drive }, "tone": tone, "mix": mix }));
        cli::edit_with(dir, Command::SetSaturator { channel: ch, sat })?;
        let pan = if s.pan != 0.0 {
            side = -side;
            s.pan * side
        } else {
            0.0
        };
        cli::edit_with(
            dir,
            Command::SetTrack {
                track: t,
                name: None,
                color: None,
                icon: None,
                volume_db: None,
                pan: Some(pan),
                pan_mode: None,
                spread: None,
                elevation: None,
                distance: None,
                mute: None,
                solo: None,
            },
        )?;
        if let Some(level) = s.reverb {
            let b = match bus {
                Some(b) => b,
                None => *bus.insert(reverb_bus(dir, &cli::project(dir)?, t)?),
            };
            let now = cli::project(dir)?;
            match now.tracks[t].sends.iter().position(|x| x.bus == b) {
                Some(send) => cli::edit_with(
                    dir,
                    Command::SetSend {
                        track: t,
                        send,
                        level_db: Some(level),
                        pre: None,
                    },
                )?,
                None => cli::edit_with(
                    dir,
                    Command::AddSend {
                        track: t,
                        bus: Some(b),
                        level_db: level,
                        pre: false,
                    },
                )?,
            };
        }
        done.push(json!({ "track": t, "name": p.tracks[t].name, "role": r }));
    }
    // Levels: each part alone, moved to its part's loudness.
    if levels {
        let p = cli::project(dir)?;
        // Before the master's chain: levels are the mix's, and a limiter there would make every
        // part read loud.
        let mut unmastered = p.clone();
        unmastered.master_inserts.clear();
        (
            unmastered.master_eq,
            unmastered.master_comp,
            unmastered.master_sat,
        ) = (None, None, None);
        unmastered.master_volume_db = 0.0;
        let report = cli::report(&unmastered, dir, None, true)?;
        for (i, &t) in tracks.iter().enumerate() {
            let r = as_role.unwrap_or_else(|| role(&p.tracks[t]));
            let a = &report.tracks[t];
            let measured = if r == "bass" {
                (a.rms_db[0] + a.rms_db[1]) / 2.0
            } else {
                a.lufs
            };
            if measured < -60.0 {
                done[i]["level"] = json!("silent: no level set");
                continue;
            }
            let under = if sub_led && matches!(r, "lead" | "chords" | "pad" | "counter" | "arp") {
                3.0
            } else {
                0.0
            };
            let wanted = p.tracks[t].volume_db + strip_for(r).lufs - under - measured;
            let volume = wanted.clamp(-30.0, 12.0);
            cli::edit_with(
                dir,
                Command::SetTrack {
                    track: t,
                    name: None,
                    color: None,
                    icon: None,
                    volume_db: Some((volume * 10.0).round() / 10.0),
                    pan: None,
                    pan_mode: None,
                    spread: None,
                    elevation: None,
                    distance: None,
                    mute: None,
                    solo: None,
                },
            )?;
            // Its level automation (a section's ups and downs) moves with it: it's what plays.
            let moved = volume - p.tracks[t].volume_db;
            let auto = &p.tracks[t].automation.volume;
            if !auto.is_empty() && moved.abs() > 0.05 {
                let points = auto
                    .iter()
                    .map(|x| PointIn {
                        tick: x.tick,
                        value: Some(x.value + moved),
                        text: None,
                        curve: Some(x.curve),
                        step: Some(x.step),
                        ease: Some(x.ease),
                    })
                    .collect();
                cli::edit_with(
                    dir,
                    Command::SetAutomation {
                        track: t,
                        param: AutoParam::Volume,
                        send: None,
                        plugin: None,
                        id: None,
                        name: None,
                        points,
                    },
                )?;
            }
            done[i]["volume_db"] = json!((volume * 10.0).round() / 10.0);
            if wanted > 12.0 {
                done[i]["level"] = json!(format!(
                    "needed {wanted:.0} dB: turn its instrument (or sample) up and mix again"
                ));
            }
        }
    }
    Ok(json!({ "mixed": done, "next": "critique to hear what's left; master when it's right" }))
}

/// Masters the song: a limiter on the master and its gain set so the song's loudness is `lufs`
/// (the reference's, else the genre's, else -10), measured again until it's there.
pub fn master(dir: &Path, lufs: Option<f32>) -> Result<Value, String> {
    let p = cli::project(dir)?;
    let target = lufs
        .or(p.reference.as_ref().map(|r| r.lufs))
        .or(p.genre.as_deref().and_then(playbook::find).map(|b| b.lufs))
        .unwrap_or(-10.0);
    if !(-30.0..=-4.0).contains(&target) {
        return Err("lufs is -30 to -4".into());
    }
    let slot = match p.master_inserts.iter().position(|i| i.class == LIMITER.0) {
        Some(i) => i,
        None => {
            cli::edit_with(
                dir,
                Command::SetInsert {
                    channel: ChannelRef::Master,
                    index: p.master_inserts.len(),
                    plugin: Some(au(LIMITER.0, LIMITER.1)),
                },
            )?;
            p.master_inserts.len()
        }
    };
    let set_gain = |db: f32| {
        cli::edit_with(
            dir,
            param(
                ChannelRef::Master,
                Which::Insert(slot),
                "Pre-Gain",
                &format!("{db:.1} dB"),
            ),
        )
    };
    // Unlimited first, so the gain is from where the mix is. The whole song is measured at the
    // start and the end; between, its loudest section (up to 16 bars), its loudness offset from
    // the whole's taken as staying put.
    let mut gain = 0.0f32;
    set_gain(gain)?;
    // Loud genres clip softly before the limiter (the master's saturator runs before its
    // inserts): the kick's and bass's peaks rounded off, so the limiter needn't crush them.
    let mut clip = match target {
        t if t > -9.5 => 6.0f32,
        t if t > -11.0 => 3.0,
        _ => 0.0,
    };
    let set_clip = |drive: f32| {
        let sat = (drive > 0.0)
            .then(|| json!({ "drive_db": drive, "tone": 0, "mix": 1, "output_db": 0 }));
        cli::edit_with(
            dir,
            Command::SetSaturator {
                channel: ChannelRef::Master,
                sat,
            },
        )
    };
    set_clip(clip)?;
    // The fader comes after the limiter (which stops at 0 dBFS): it sets the ceiling, and the
    // loudness is measured under it.
    cli::edit_with(
        dir,
        Command::SetMaster {
            volume_db: Some(-0.5),
            pan_law: None,
        },
    )?;
    let p = cli::project(dir)?;
    let whole = cli::report(&p, dir, None, false)?;
    if whole.mix.lufs < -60.0 {
        return Err("the song is silent: nothing to master".into());
    }
    let loudest = whole
        .sections
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.2.lufs.total_cmp(&b.1.2.lufs))
        .map(|(i, (_, tick, a))| {
            let end = p
                .markers
                .get(i + 1)
                .map_or(p.end(), |n| n.tick)
                .min(tick + 16 * p.bar());
            ((*tick, end), a.lufs - whole.mix.lufs)
        });
    let measure = |_gain: f32| -> Result<f32, String> {
        let p = cli::project(dir)?;
        Ok(match loudest {
            Some((range, offset)) => cli::report(&p, dir, Some(range), false)?.mix.lufs - offset,
            None => cli::report(&p, dir, None, false)?.mix.lufs,
        })
    };
    let mut lufs = whole.mix.lufs;
    let mut steps = vec![json!({ "gain_db": 0.0, "lufs": (lufs * 10.0).round() / 10.0 })];
    // A limiter gives back less than it's pushed: push the rest a little harder.
    let push = |gain: &mut f32, off: f32| -> Result<(), String> {
        *gain = (*gain + off * if *gain > 0.0 { 1.2 } else { 1.0 }).clamp(-20.0, 24.0);
        set_gain(*gain).map(drop)
    };
    // Near it by the section (the offset drifts as the loud parts are limited harder), then
    // exact by the whole.
    for _ in 0..3 {
        let off = target - lufs;
        if off.abs() < 1.0 {
            break;
        }
        push(&mut gain, off)?;
        lufs = measure(gain)?;
        steps.push(json!({ "gain_db": (gain * 10.0).round() / 10.0, "lufs": (lufs * 10.0).round() / 10.0 }));
    }
    let mut end = cli::report(&cli::project(dir)?, dir, None, false)?.mix;
    for _ in 0..3 {
        let off = target - end.lufs;
        if off.abs() < 0.4 {
            break;
        }
        let before = end.lufs;
        push(&mut gain, off)?;
        end = cli::report(&cli::project(dir)?, dir, None, false)?.mix;
        // Pushed and no louder: the limiter's at its end. Clip harder.
        if off > 0.0 && end.lufs - before < 0.3 && clip < 9.0 {
            clip += 3.0;
            set_clip(clip)?;
            end = cli::report(&cli::project(dir)?, dir, None, false)?.mix;
        }
        steps.push(json!({ "gain_db": (gain * 10.0).round() / 10.0, "clip_db": clip, "lufs": (end.lufs * 10.0).round() / 10.0, "whole": true }));
    }
    let peak = end.peak_db[0].max(end.peak_db[1]);
    let short = target - end.lufs;
    Ok(json!({
        "note": if short > 1.0 { format!("{short:.1} dB short: the mix is too quiet for the limiter to reach it cleanly; mix again (mix) and master") } else { String::new() },
        "target_lufs": target,
        "lufs": (end.lufs * 10.0).round() / 10.0,
        "peak_db": (peak * 10.0).round() / 10.0,
        "crest_db": (end.crest_db * 10.0).round() / 10.0,
        "limiter": format!("master insert {slot}, pre-gain {gain:.1} dB"),
        "clipper": if clip > 0.0 { format!("the master's saturator, drive {clip} dB") } else { "none".into() },
        "steps": steps,
    }))
}

/// How an agent makes a song that sounds finished, with these tools.
pub const WORKFLOW: &str = "0. Asked for a song like an existing one: study <file> with playbook true, \
and use the playbook it saves as the genre. \
1. playbook <genre>, then set_genre, set_tempo and set_key from it. \
2. Chords first: theory for progressions, set_chords. \
3. A loop of 4-8 bars: tracks for its parts; the person's palette first (palette: the sounds \
they picked, samples_put each for its role), then sounds <role> for the rest (any installed \
instrument's presets, or samples) instead of the sine synth or built-in kit; write_drums, write_bass \
(style kick for an 808), write_chords, write_melody. \
4. critique quick: fix every \"fix\"; render a bar range and listen-check with analyze. \
5. arrange (the genre's sections), then transition into each section (build into choruses and \
drops, fill or reverse into verses). Vary sections: write_melody with another seed or style in \
the chorus, take parts out of verses. \
6. mix_role, then critique (it renders): fix mix issues. \
7. master. 8. critique once more; say what you made and what's left. \
Asked for files (a master, stems, an MP3): bounce, with the formats, bits, rate and loudness asked.";
