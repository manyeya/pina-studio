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
    project::{Format, Instrument, PluginRef, Project, Reference, Which},
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
        None => Ok(json!(
            playbook::PLAYBOOKS
                .iter()
                .map(|b| json!({ "id": b.id, "name": b.name, "tempo": b.tempo }))
                .collect::<Vec<_>>()
        )),
        Some(g) => {
            let b = playbook::find(g).ok_or(format!(
                "no playbook for {g:?}; there are {}",
                playbook::ids().join(", ")
            ))?;
            let key = Key {
                root: 9,
                minor: b.minor,
            };
            let mut out = serde_json::to_value(b).map_err(|e| e.to_string())?;
            out["progressions"] = json!(suggestions(&key, b.harmony, 4));
            out["how"] = json!(WORKFLOW);
            Ok(out)
        }
    }
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

/// Surge XT's patch folders, by the part they're for.
fn patch_folders(role: &str) -> &'static [&'static str] {
    match role {
        "bass" => &["Basses"],
        "chords" => &["Keys", "Chords", "Polysynths", "Organs", "Guitars"],
        "pad" => &["Pads", "Atmospheres", "Strings"],
        "lead" => &["Leads", "Plucks", "Bells", "Brass", "Winds"],
        "counter" | "arp" => &["Sequences", "Plucks", "Bells", "Keys", "Rhythms"],
        "fx" => &["FX"],
        "texture" => &["Atmospheres", "FX"],
        "drums" | "perc" => &["Percussion", "Drums"],
        "vocals" => &["Vocoder", "Vox"],
        _ => &["Leads", "Keys", "Pads"],
    }
}

fn surge_dir() -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_default();
    Path::new(&home).join("Library/Application Support/Surge XT")
}

/// Every Surge XT patch (factory and third party) in the role's folders, as (name, path).
fn patches(role: &str) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for root in ["patches_factory", "patches_3rdparty"] {
        let base = surge_dir().join(root);
        let mut dirs: Vec<PathBuf> = patch_folders(role).iter().map(|f| base.join(f)).collect();
        // Third-party patches sit in a folder per author, then by kind.
        if let Ok(authors) = std::fs::read_dir(&base) {
            for a in authors.flatten() {
                dirs.extend(patch_folders(role).iter().map(|f| a.path().join(f)));
            }
        }
        for d in dirs {
            let Ok(files) = std::fs::read_dir(&d) else {
                continue;
            };
            for f in files.flatten() {
                let path = f.path();
                if path.extension().is_some_and(|e| e == "fxp") {
                    let name = path
                        .file_stem()
                        .map_or(String::new(), |s| s.to_string_lossy().to_string());
                    out.push((name, path));
                }
            }
        }
    }
    out
}

/// How well a name says what's wanted: the words of `wanted` it holds.
fn score(name: &str, wanted: &[String]) -> usize {
    let n = name.to_lowercase();
    wanted
        .iter()
        .filter(|w| w.len() > 2 && n.contains(w.as_str()))
        .count()
}

/// Sounds for a part: Surge XT patches, instrument plugins, and samples from the library, for
/// `role` in `genre`, matching `words`; with a project, ready-made commands for `track`.
pub fn sounds(
    role_name: &str,
    genre: Option<&str>,
    words: &str,
    dir: Option<&Path>,
    track: Option<usize>,
) -> Result<Value, String> {
    let book = genre.and_then(playbook::find);
    let from_book = book
        .and_then(|b| b.parts.iter().find(|p| p.role == role_name))
        .map_or("", |p| p.sound);
    let wanted: Vec<String> = format!("{words} {from_book}")
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    let mut found = patches(role_name);
    found.sort_by_key(|(n, _)| std::cmp::Reverse(score(n, &wanted)));
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let plugins = scan::scan(&exe, &scan::everything(), &scan::cache_file());
    let surge = plugins
        .iter()
        .filter(|p| p.class == "Surge XT")
        .min_by_key(|p| match p.format {
            Format::Clap => 0,
            Format::Vst3 => 1,
            _ => 2,
        });
    let t = track.unwrap_or(0);
    let patches: Vec<Value> = found
        .iter()
        .take(14)
        .map(|(name, path)| {
            let mut v = json!({ "name": name, "file": path });
            if dir.is_some() && surge.is_some() {
                v["load"] = json!({ "op": "set_preset", "channel": { "track": t }, "plugin": "instrument", "preset": { "file": path } });
            }
            v
        })
        .collect();
    let instruments: Vec<Value> = plugins
        .iter()
        .filter(|p| p.instrument)
        .map(|p| json!({ "name": p.class, "vendor": p.vendor, "format": p.format, "instrument": { "plugin": p.plugin_ref() } }))
        .collect();
    // The library's search wants few words: what's asked, else the genre's first sound, else
    // the part.
    let first = from_book.split(',').next().unwrap_or("").trim();
    let samples = [words.trim(), first, role_name]
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
    let mut out = json!({
        "role": role_name,
        "looking_for": from_book,
        "surge_xt_patches": patches,
        "instrument_plugins": instruments,
        "samples": samples,
    });
    if let Some(s) = surge {
        let inst = Instrument::Plugin(s.plugin_ref());
        out["surge_xt"] = json!({ "op": "set_instrument", "track": t, "instrument": inst });
        out["how"] = json!(
            "set_instrument Surge XT on the track (surge_xt), then a patch's `load`; render a bar or two of it and try another if it isn't right"
        );
    }
    Ok(out)
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
                    })
                    .collect();
                cli::edit_with(
                    dir,
                    Command::SetAutomation {
                        track: t,
                        param: AutoParam::Volume,
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
    cli::edit_with(dir, Command::SetMaster { volume_db: -0.5 })?;
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
pub const WORKFLOW: &str = "1. playbook <genre>, then set_genre, set_tempo and set_key from it. \
2. Chords first: theory for progressions, set_chords. \
3. A loop of 4-8 bars: tracks for its parts; sounds <role> for each (Surge XT patches, plugins, \
samples) instead of the sine synth or built-in kit; write_drums, write_bass (style kick for an \
808), write_chords, write_melody. \
4. critique quick: fix every \"fix\"; render a bar range and listen-check with analyze. \
5. arrange (the genre's sections), then transition into each section (build into choruses and \
drops, fill or reverse into verses). Vary sections: write_melody with another seed or style in \
the chorus, take parts out of verses. \
6. mix_role, then critique (it renders): fix mix issues. \
7. master. 8. critique once more; say what you made and what's left.";
