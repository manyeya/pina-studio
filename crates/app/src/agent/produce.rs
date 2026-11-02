//! A producer's tools for agents: the critic, the person's playbooks, chord suggestions, a reference
//! song to aim at, sounds for a part, a mix by role, and a master to a loudness. Edits go through
//! `cli::edit_with`, so they land live in the app when it has the song open.

use std::path::{Path, PathBuf};

use pina_engine::{
    music::Key,
    music::analysis,
    music::critique::{self, Heard},
    music::playbook,
    music::theory::{self, PROGRESSIONS},
    plugin::scan,
    song::commands::{ChannelRef, Command},
    song::project::{
        Comp, Eq, Format, Fx, Instrument, Limiter, PluginRef, Preset, PresetSource, Project,
        Reference, Saturator, chain,
    },
};
use serde_json::{Value, json};

use crate::cli;

/// A channel to set a compressor for by listening: a track's, or the master's (None).
pub fn comp_heard(p: &Project, track: Option<usize>) -> (Project, Option<(u64, u64)>) {
    let mut q = p.clone();
    // Past no master: the master's own chain is what's being set, or isn't the track's to hear.
    q.master_inserts.clear();
    q.master_more.clear();
    (q.master_comp, q.master_sat, q.master_volume_db) = (None, None, 0.0);
    match track {
        Some(t) => {
            // As its compressor hears it: after its plugins and EQ, before its compressor,
            // saturator and fader.
            let tr = &mut q.tracks[t];
            (tr.comp, tr.sat, tr.volume_db, tr.pan) = (None, None, 0.0, 0.0);
            tr.automation.volume.clear();
            tr.sends.clear();
            (cli::alone(&q, t), played(p, t))
        }
        None => (q, None),
    }
}

/// A chain cut short at its limiter: what's after it (and it) taken out.
#[allow(clippy::too_many_arguments)]
fn before_limiter(
    order: &mut Vec<Fx>,
    inserts: &mut Vec<PluginRef>,
    more: &mut Vec<pina_engine::song::project::Unit>,
    eq: &mut Option<Eq>,
    comp: &mut Option<Comp>,
    sat: &mut Option<Saturator>,
    limit: &mut Option<Limiter>,
) {
    let full = chain(order, inserts.len(), more.len());
    let at = full
        .iter()
        .position(|f| *f == Fx::Limit)
        .unwrap_or(full.len());
    for fx in &full[at..] {
        match fx {
            Fx::Eq => *eq = None,
            Fx::Comp => *comp = None,
            Fx::Sat => *sat = None,
            _ => {}
        }
    }
    inserts.truncate(full[..at].iter().filter(|f| **f == Fx::Plugin).count());
    more.truncate(full[..at].iter().filter(|f| **f == Fx::More).count());
    *order = full[..at].to_vec();
    *limit = None;
}

/// A channel to set a limiter for by listening, as it hears it, through its chain up to the
/// limiter, the fader (which comes after) at 0 dB: a track's (alone, eight bars from where it
/// first plays), an aux's (all that's sent or routed to it, the whole song) or the master's (the
/// whole song). None for the range: all of it.
pub fn limit_heard(p: &Project, ch: ChannelRef) -> (Project, Option<(u64, u64)>) {
    let mut q = p.clone();
    // Past anything but the master: the master's own chain isn't the channel's to hear.
    if ch != ChannelRef::Master {
        q.master_inserts.clear();
        q.master_more.clear();
        (q.master_eq, q.master_comp, q.master_sat, q.master_limit) = (None, None, None, None);
    }
    q.master_volume_db = 0.0;
    match ch {
        ChannelRef::Track(t) => {
            let tr = &mut q.tracks[t];
            before_limiter(
                &mut tr.chain,
                &mut tr.inserts,
                &mut tr.more,
                &mut tr.eq,
                &mut tr.comp,
                &mut tr.sat,
                &mut tr.limit,
            );
            (tr.volume_db, tr.pan) = (0.0, 0.0);
            tr.automation.volume.clear();
            tr.sends.clear();
            (cli::alone(&q, t), played(p, t))
        }
        ChannelRef::Aux(a) => {
            let bus = q.auxes[a].bus;
            // Every track's own output into a silent bus (its sends going on as they were),
            // every other aux silent: what's heard is only what reaches this one.
            let silent = q.add_aux();
            for t in &mut q.tracks {
                if t.output != Some(bus) {
                    t.output = Some(silent);
                }
            }
            for (i, x) in q.auxes.iter_mut().enumerate() {
                x.mute = i != a;
            }
            let x = &mut q.auxes[a];
            before_limiter(
                &mut x.chain,
                &mut x.inserts,
                &mut x.more,
                &mut x.eq,
                &mut x.comp,
                &mut x.sat,
                &mut x.limit,
            );
            (x.volume_db, x.pan) = (0.0, 0.0);
            (q, None)
        }
        ChannelRef::Master => {
            before_limiter(
                &mut q.master_chain,
                &mut q.master_inserts,
                &mut q.master_more,
                &mut q.master_eq,
                &mut q.master_comp,
                &mut q.master_sat,
                &mut q.master_limit,
            );
            (q, None)
        }
    }
}

/// A limiter set by listening to what reaches it, to come out at `target` LUFS (see
/// `dsp::limit::learn`). None when nothing plays there.
pub fn learn_limit(
    p: &Project,
    dir: &Path,
    ch: ChannelRef,
    target: f32,
) -> Result<Option<pina_engine::dsp::limit::learn::Learned>, String> {
    let (heard, range) = limit_heard(p, ch);
    if matches!(ch, ChannelRef::Track(_)) && range.is_none() {
        return Ok(None);
    }
    let render = cli::rendered(&heard, dir, range)?;
    let current = match ch {
        ChannelRef::Track(t) => p.tracks[t].limit,
        ChannelRef::Aux(a) => p.auxes[a].limit,
        ChannelRef::Master => p.master_limit,
    }
    .unwrap_or_default();
    let at = range.map_or(0, |r| r.0);
    let rate = p.sample_rate as f64;
    Ok(pina_engine::dsp::limit::learn::learn(
        &render,
        rate,
        p.tempo_at(at),
        &current,
        target,
    ))
}

/// `pina learn --limit`: a channel's limiter as listening sets it for `target` LUFS, and the
/// loudness it comes out at, as JSON (not set: whoever asked sets it).
pub fn learn_limiter(dir: &Path, ch: ChannelRef, target: f32) -> Result<Value, String> {
    let p = cli::project(dir)?;
    match ch {
        ChannelRef::Track(t) if t >= p.tracks.len() => return Err(format!("no track {t}")),
        ChannelRef::Aux(a) if a >= p.auxes.len() => return Err(format!("no aux {a}")),
        _ => {}
    }
    if !(-30.0..=-4.0).contains(&target) {
        return Err("lufs is -30 to -4".into());
    }
    Ok(match learn_limit(&p, dir, ch, target)? {
        Some(l) => json!({ "limit": l.limit, "lufs": l.lufs }),
        None => Value::Null,
    })
}

/// Eight bars of a track from where it first plays (its first note, or its first audio): what
/// listening to it hears.
fn played(p: &Project, t: usize) -> Option<(u64, u64)> {
    use pina_engine::song::project::Content;
    let notes = pina_engine::music::compose::heard_notes(p, t);
    let note = notes.iter().map(|n| n.start.max(0) as u64).min();
    let audio = p.tracks[t]
        .regions
        .iter()
        .filter(|r| matches!(r.content, Content::Audio { .. } | Content::Takes { .. }))
        .map(|r| r.start)
        .min();
    let first = note.into_iter().chain(audio).min()?;
    let bar = p.bar();
    let from = first / bar * bar;
    Some((from, from + 8 * bar))
}

/// A compressor set by listening to a track (or the master, None): rendered as its compressor
/// hears it, measured, and set for what it plays (see `dsp::comp::learn`). None when nothing
/// plays there.
pub fn learn_comp(
    p: &Project,
    dir: &Path,
    track: Option<usize>,
) -> Result<Option<pina_engine::song::project::Comp>, String> {
    use pina_engine::dsp::comp::learn::{Material, learn};
    let (heard, range) = comp_heard(p, track);
    if track.is_some() && range.is_none() {
        return Ok(None);
    }
    let render = cli::rendered(&heard, dir, range)?;
    let material = match track {
        Some(t) => Material::of_part(pina_engine::music::arrange::role(&p.tracks[t])),
        None => Material::Bus,
    };
    let at = range.map_or(0, |r| r.0);
    Ok(learn(
        &render,
        p.sample_rate as f64,
        p.tempo_at(at),
        material,
    ))
}

/// A saturator set by listening to a track (or the master, None): rendered as it hears it
/// (after its plugins, EQ and compressor), measured, and set for its part in the song's genre
/// (see `dsp::sat::learn`). None when nothing plays there.
pub fn learn_sat(
    p: &Project,
    dir: &Path,
    track: Option<usize>,
) -> Result<Option<(Saturator, Vec<String>)>, String> {
    use pina_engine::dsp::sat::learn::{Ear, learn};
    let mut q = p.clone();
    q.master_inserts.clear();
    q.master_more.clear();
    (q.master_sat, q.master_limit, q.master_volume_db) = (None, None, 0.0);
    let (heard, range) = match track {
        Some(t) => {
            (q.master_eq, q.master_comp) = (None, None);
            let tr = &mut q.tracks[t];
            (tr.sat, tr.volume_db, tr.pan) = (None, 0.0, 0.0);
            tr.automation.volume.clear();
            tr.sends.clear();
            let Some(range) = played(p, t) else {
                return Ok(None);
            };
            (cli::alone(&q, t), Some(range))
        }
        None => (q, None),
    };
    let render = cli::rendered(&heard, dir, range)?;
    let genre = p.genre.clone().unwrap_or_default();
    let ear = Ear {
        part: track.map_or("bus", |t| pina_engine::music::arrange::role(&p.tracks[t])),
        genre: &genre,
        lowest_hz: track.and_then(|t| crate::agent::mixing::lowest_hz(p, t)),
        key: p.song_key(),
    };
    Ok(learn(&render, p.sample_rate as f64, &ear))
}

/// One built-in effect on a track (or the master) set by listening, as the app's Learn sets it:
/// `comp`, `sat` or `limit` (to `lufs`, -14 if not given). What it would set (and, for the
/// saturator, why); set too with `apply`.
pub fn learn_effect(
    dir: &Path,
    track: Option<usize>,
    effect: &str,
    lufs: Option<f32>,
    apply: bool,
) -> Result<Value, String> {
    let p = cli::project(dir)?;
    if let Some(t) = track.filter(|&t| t >= p.tracks.len()) {
        return Err(format!("no track {t}"));
    }
    let channel = match track {
        Some(t) => json!({ "track": t }),
        None => json!("master"),
    };
    let (answer, set) = match effect {
        "comp" => {
            let c = learn_comp(&p, dir, track)?;
            (
                json!({ "comp": c }),
                c.map(|c| json!({ "op": "set_comp", "comp": c })),
            )
        }
        "sat" => match learn_sat(&p, dir, track)? {
            Some((s, why)) => (
                json!({ "sat": s, "why": why }),
                Some(json!({ "op": "set_saturator", "sat": s })),
            ),
            None => (json!({ "sat": null }), None),
        },
        "limit" => {
            let target = lufs.unwrap_or(-14.0);
            if !(-30.0..=-4.0).contains(&target) {
                return Err("lufs is -30 to -4".into());
            }
            let ch = track.map_or(ChannelRef::Master, ChannelRef::Track);
            match learn_limit(&p, dir, ch, target)? {
                Some(l) => (
                    json!({ "limit": l.limit, "lufs": l.lufs }),
                    Some(json!({ "op": "set_limiter", "limit": l.limit })),
                ),
                None => (json!({ "limit": null }), None),
            }
        }
        _ => return Err("`effect` is comp, sat or limit".into()),
    };
    let mut answer = answer;
    match (set, apply) {
        (Some(mut c), true) => {
            c["channel"] = channel;
            cli::edit(dir, &c.to_string())?;
            answer["set"] = json!(true);
        }
        (None, _) => answer["note"] = json!("nothing plays there to listen to"),
        _ => answer["set"] = json!(false),
    }
    Ok(answer)
}

/// `pina learn --sat`: a channel's saturator as listening sets it, and why, as JSON.
pub fn learn_saturator(dir: &Path, track: Option<usize>) -> Result<Value, String> {
    let p = cli::project(dir)?;
    if let Some(t) = track.filter(|&t| t >= p.tracks.len()) {
        return Err(format!("no track {t}"));
    }
    Ok(match learn_sat(&p, dir, track)? {
        Some((sat, why)) => json!({ "sat": sat, "why": why }),
        None => Value::Null,
    })
}

/// `pina learn`: a channel's compressor as listening sets it, as JSON (not set: the app, or
/// whoever asked, sets it).
pub fn learn(dir: &Path, track: Option<usize>) -> Result<Value, String> {
    let p = cli::project(dir)?;
    if let Some(t) = track.filter(|&t| t >= p.tracks.len()) {
        return Err(format!("no track {t}"));
    }
    let comp = learn_comp(&p, dir, track)?;
    serde_json::to_value(comp).map_err(|e| e.to_string())
}

/// What's wrong with a song and how to fix it: from its notes and layout, and (unless `quick`)
/// a render of it, over bars `range` or all of it.
pub fn critique(dir: &Path, quick: bool, range: Option<(u64, u64)>) -> Result<Value, String> {
    let p = cli::project(dir)?;
    let mut issues = if quick {
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
    // A song written to be like a reference: its render against the reference's spectrum, the
    // biggest differences as notes to fix, every time it's heard (not only when compared).
    let mut likeness = Value::Null;
    if !quick && range.is_none() && p.like.is_some() {
        match cli::reference::report(&p, dir) {
            Ok((report, reference, song)) => {
                let d = cli::reference::describe(&report, &reference, &song, &p);
                likeness = d["likeness"].clone();
                for x in d["differences"].as_array().into_iter().flatten().take(5) {
                    let tracks: Vec<String> = x["tracks"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|t| Some(format!("{} \"{}\"", t["track"], t["name"].as_str()?)))
                        .collect();
                    issues.push(critique::Issue {
                        level: "improve",
                        area: "reference",
                        what: x["says"].as_str().unwrap_or_default().to_string(),
                        at: Some(match x["bars"].as_str() {
                            Some(b) => format!("{}, bars {b}", x["section"].as_str().unwrap_or("")),
                            None => x["section"].as_str().unwrap_or("").to_string(),
                        }),
                        fix: match tracks.is_empty() {
                            true => x["ask"].as_str().unwrap_or_default().to_string(),
                            false => format!(
                                "{} Tracks: {}.",
                                x["ask"].as_str().unwrap_or_default(),
                                tracks.join(", ")
                            ),
                        },
                    });
                }
            }
            Err(e) => issues.push(critique::Issue {
                level: "note",
                area: "reference",
                what: format!("couldn't compare it with its reference: {e}"),
                at: None,
                fix: "reference_compare once the song plays".into(),
            }),
        }
    }
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
        "like_reference": likeness,
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
            let b = playbook::find(g).ok_or(match playbook::ids() {
                ids if ids.is_empty() => format!("no playbook for {g:?}: the person has none"),
                ids => format!("no playbook for {g:?}; theirs are {}", ids.join(", ")),
            })?;
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

/// Opens the person's playbooks folder (made if it isn't there), or one genre's file.
pub fn edit_playbooks(genre: Option<&str>) -> Result<(), String> {
    let (_, problems) = playbook::load();
    let dir = playbook::dir().ok_or("no home folder")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
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
    crate::platform::open(&target).map_err(|e| e.to_string())?;
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
        "write": "set_chords with the numerals or chords; then write the parts on them yourself",
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
        "bass" => &["bass", "808", "sub", "reese", "bs ", "contrabass"],
        "chords" => &[
            "key",
            "piano",
            "ep ",
            "rhodes",
            "wurli",
            "organ",
            "chord",
            "poly",
            "guitar",
            "stab",
            "clav",
            "harpsichord",
            "accordion",
        ],
        "pad" => &[
            "pad", "atmo", "ambient", "string", "choir", "drone", "swell", "texture", "ensemble",
            "aahs", "oohs",
        ],
        "lead" => &[
            "lead", "ld ", "pluck", "bell", "brass", "horn", "flute", "wind", "sax", "whistle",
            "solo", "trumpet", "trombone", "clarinet", "oboe", "violin", "cello", "ocarina",
        ],
        "counter" | "arp" => &[
            "arp",
            "seq",
            "pluck",
            "bell",
            "mallet",
            "marimba",
            "rhythm",
            "vibraphone",
            "celesta",
            "kalimba",
            "harp",
            "pizzicato",
        ],
        "fx" => &["fx", "sfx", "riser", "noise", "sweep", "impact"],
        "texture" => &["atmo", "texture", "noise", "drone", "ambient"],
        "drums" | "perc" => &[
            "drum",
            "kit",
            "perc",
            "kick",
            "snare",
            "hat",
            "tom",
            "timpani",
            "taiko",
            "agogo",
            "woodblock",
        ],
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

/// Where an instrument's presets are kept in the cache. The `2|` is how they're listed: the
/// General MIDI synth's programs came in with it, so what was kept before is listed again.
fn preset_key(p: &scan::PluginInfo) -> String {
    format!(
        "2|{:?}|{}|{}|{}",
        p.format,
        p.path.display(),
        p.class,
        p.uid.as_deref().unwrap_or("")
    )
}

fn presets_cache() -> PathBuf {
    pina_engine::paths::caches().join("presets.json")
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
                let key = preset_key(p);
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
/// own: in a process of its own, so one that crashes or hangs doesn't take Pina Studio with it. Kept
/// until the plugin changes.
fn instrument_presets(
    p: &scan::PluginInfo,
    cache: &mut serde_json::Map<String, Value>,
) -> Vec<Preset> {
    let key = preset_key(p);
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

/// Sounds for a part: every installed instrument's presets that might suit it, auditioned (each
/// played a phrase for the part and heard, so they're ranked by how they sound, not what they're
/// called; what's heard is kept), and samples from the library, for `role` in `genre`, matching
/// `words`; with `track`, the commands are for it.
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
    // The character asked for, as the library names it (dark, warm, punchy, wide…), and any
    // instrument named ("rhodes", "brass", "strings").
    let (asked_sounds, asked) = pina_engine::samples::search::named(words);
    // The words that name an instrument: a preset called by one ("Rhodes Mk1") is likelier it.
    let naming: Vec<String> = words
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !pina_engine::samples::search::named(w).0.is_empty())
        .map(str::to_string)
        .collect();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let plugins = scan::scan(&exe, &scan::everything(), &scan::cache_file());
    let rank = |f: Format| match f {
        Format::Clap => 0,
        Format::Vst3 => 1,
        Format::Au => 2,
        Format::Vst2 => 3,
    };
    let file = presets_cache();
    let mut cache: serde_json::Map<String, Value> = std::fs::read(&file)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    // Each instrument once, in the format that lists its presets (a CLAP build may list none
    // where its Audio Unit lists its every program), else the one it plays best in.
    let mut classes: Vec<(&scan::PluginInfo, Vec<Preset>)> = Vec::new();
    for p in plugins.iter().filter(|p| p.instrument) {
        let presets = instrument_presets(p, &mut cache);
        match classes.iter_mut().find(|(c, _)| c.class == p.class) {
            Some((c, kept))
                if presets.len() > kept.len()
                    || presets.len() == kept.len() && rank(p.format) < rank(c.format) =>
            {
                *c = p;
                *kept = presets;
            }
            Some(_) => {}
            None => classes.push((p, presets)),
        }
    }
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
        let _ = std::fs::write(&file, serde_json::to_vec(&cache).unwrap_or_default());
    }
    let t = track.unwrap_or(0);
    let mut kept = crate::sounds::audition::Kept::open();
    // New auditions this time, at most: what isn't heard now is next time.
    let mut budget = 40usize;
    let mut instruments: Vec<(f32, Value)> = Vec::new();
    let mut best: Vec<(f32, Value)> = Vec::new();
    for (p, all) in &classes {
        let plugin = p.plugin_ref();
        // Those whose names suit the part first; a bank of unnamed programs (an FM synth's
        // voices) is heard a few at a time.
        let mut named: Vec<(usize, &Preset)> = all
            .iter()
            .map(|x| (fit(x, role_name, &wanted), x))
            .collect();
        named.sort_by_key(|(f, _)| std::cmp::Reverse(*f));
        let mut candidates: Vec<(usize, &Preset)> = named
            .iter()
            .filter(|(f, _)| *f > 0)
            .take(8)
            .copied()
            .collect();
        if candidates.len() < 4 && all.len() <= 160 {
            for &(f, x) in named.iter().filter(|(f, _)| *f == 0) {
                if candidates.len() >= 8 {
                    break;
                }
                candidates.push((f, x));
            }
        }
        let unheard: Vec<Preset> = candidates
            .iter()
            .filter(|(_, x)| kept.get(&plugin, x, role_name).is_none())
            .map(|(_, x)| (*x).clone())
            .take(budget)
            .collect();
        budget -= unheard.len();
        for (x, h) in unheard
            .iter()
            .zip(crate::sounds::audition::hear(&plugin, &unheard, role_name))
        {
            kept.put(&plugin, x, role_name, &h);
        }
        let set_instrument = json!({ "op": "set_instrument", "track": t, "instrument": Instrument::Plugin(plugin.clone()) });
        let mut heard: Vec<(f32, Value)> = candidates
            .iter()
            .filter_map(|(f, x)| {
                let load = json!({ "op": "set_preset", "channel": { "track": t }, "plugin": "instrument", "preset": x.source });
                // Its name on one scale for every instrument: a part's word or two and one asked.
                let by_name = (*f as f32 / 7.0).min(1.0);
                match kept.get(&plugin, x, role_name) {
                    // Silent, or it wouldn't play.
                    Some(None) => None,
                    Some(Some(h)) => {
                        let (score, why) =
                            crate::sounds::audition::judge(role_name, &h, &asked, &asked_sounds);
                        let called = naming.iter().any(|w| x.name.to_lowercase().contains(w.as_str()));
                        let score = score * 0.8 + by_name * 0.1 + if called { 0.1 } else { 0.0 };
                        Some((score, json!({ "name": x.name, "sounds": crate::sounds::audition::say(&h), "fit": (score as f64 * 100.0).round() / 100.0, "watch": why, "load": load })))
                    }
                    None => Some((by_name * 0.3, json!({ "name": x.name, "sounds": "not heard yet: ask again", "load": load }))),
                }
            })
            .collect();
        heard.sort_by(|a, b| b.0.total_cmp(&a.0));
        heard.truncate(5);
        for (score, v) in heard.iter().take(2) {
            best.push((*score, json!({ "instrument": p.class, "preset": v["name"], "sounds": v["sounds"], "fit": v.get("fit"), "commands": [set_instrument, v["load"]] })));
        }
        let top = heard.first().map_or(0.0, |h| h.0);
        instruments.push((
            top,
            json!({
                "name": p.class,
                "vendor": p.vendor,
                "presets_in_all": all.len(),
                "set_instrument": set_instrument,
                "presets": heard.into_iter().map(|(_, v)| v).collect::<Vec<_>>(),
            }),
        ));
    }
    kept.save();
    // Pina Studio's own synth: its patches made for the part (or named by the words), heard
    // playing it as the presets are.
    let suits: &[&str] = match role_name {
        "bass" => &["bass"],
        "lead" | "vocals" => &["lead"],
        "pad" | "texture" => &["pad"],
        "chords" => &["pad", "keys", "pluck"],
        "keys" => &["keys", "pad"],
        "arp" => &["pluck", "lead"],
        "counter" => &["lead", "pluck"],
        _ => &[],
    };
    let mut patches: Vec<(f32, Value)> = pina_engine::song::project::POLY_PATCHES
        .iter()
        .filter(|(name, part, about)| {
            let said =
                |w: &String| name.to_lowercase().contains(w.as_str()) || about.contains(w.as_str());
            suits.contains(part) || naming.iter().any(said)
        })
        .take(8)
        .filter_map(|&(name, _, about)| {
            let patch = pina_engine::song::project::poly_patch(name)?;
            let h = crate::sounds::audition::hear_synth(&patch, role_name)?;
            let (score, why) = crate::sounds::audition::judge(role_name, &h, &asked, &asked_sounds);
            let called = naming
                .iter()
                .any(|w| name.to_lowercase().contains(w.as_str()) || about.contains(w.as_str()));
            let score = score * 0.85 + if called { 0.15 } else { 0.0 };
            let load = json!({ "op": "set_synth", "track": t, "patch": name });
            Some((
                score,
                json!({ "name": name, "about": about, "sounds": crate::sounds::audition::say(&h),
                    "fit": (score as f64 * 100.0).round() / 100.0, "watch": why, "load": load }),
            ))
        })
        .collect();
    patches.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (score, v) in patches.iter().take(2) {
        best.push((
            *score,
            json!({ "instrument": "Pina Studio's synth", "preset": v["name"], "sounds": v["sounds"],
                "fit": v.get("fit"), "commands": [v["load"]] }),
        ));
    }
    if let Some(top) = patches.first().map(|p| p.0) {
        instruments.push((
            top,
            json!({
                "name": "Pina Studio's synth",
                "about": "built in: three wavetable oscillators with warps and unison, a sub and noise, two analog-modelled filters, four envelopes, eight LFOs and a matrix that can reach the track's effects; set_synth shapes any patch",
                "presets": patches.into_iter().map(|(_, v)| v).collect::<Vec<_>>(),
            }),
        ));
    }
    instruments.sort_by(|a, b| b.0.total_cmp(&a.0));
    best.sort_by(|a, b| b.0.total_cmp(&a.0));
    best.truncate(6);
    // The library's search wants few words: what's asked, else the genre's first sound, else
    // the part.
    let first = from_book.split(',').next().unwrap_or("").trim().to_string();
    let samples = [words.trim(), first.as_str(), role_name]
        .into_iter()
        .filter(|q| !q.is_empty())
        .find_map(|q| {
            let query = pina_engine::samples::search::Query {
                text: q.into(),
                limit: 8,
                ..Default::default()
            };
            crate::cli::samples::find(query, dir)
                .ok()
                .filter(|v| v.as_array().is_some_and(|a| !a.is_empty()))
        })
        .unwrap_or(json!([]));
    Ok(json!({
        "role": role_name,
        "looking_for": from_book,
        "best": best.into_iter().map(|(_, v)| v).collect::<Vec<_>>(),
        "instruments": instruments.into_iter().map(|(_, v)| v).collect::<Vec<_>>(),
        "samples": samples,
        "how": "each preset was played a phrase for the part and heard by the sample library's model (`sounds`: what it heard, its character, attack and brightness; `fit` 0-1 for the part and the character asked; `watch`: what may be wrong). Run a best one's commands, then `listen` to the track in the song; if it isn't right, try the next. `presets` lists everything an instrument has.",
    }))
}

/// Mixes tracks (all, or `only`) by measuring them: see `mixing`.
pub fn mix_role(
    dir: &Path,
    only: Option<usize>,
    as_role: Option<&str>,
    levels: bool,
) -> Result<Value, String> {
    crate::agent::mixing::mix(dir, only, as_role, levels)
}

/// Masters the song. Its tonal balance goes toward what it aims at, then the master's own limiter
/// is set by listening (its style, its release to the tempo, its gain found with the limiter
/// itself) so the song's loudness is `lufs`: the reference's, else that of the person's playbook
/// the genre is, else -10. True peaks stay under -1 dBTP, and it's measured again as it plays.
pub fn master(dir: &Path, lufs: Option<f32>) -> Result<Value, String> {
    let p = cli::project(dir)?;
    let target = lufs
        .or(p.reference.as_ref().map(|r| r.lufs))
        .or(p.genre.as_deref().and_then(playbook::find).map(|b| b.lufs))
        .unwrap_or(-10.0);
    if !(-30.0..=-4.0).contains(&target) {
        return Err("lufs is -30 to -4".into());
    }
    // Its tonal balance towards what it aims at, before the limiter hears it.
    let tonal = crate::agent::mixing::tonal_balance(dir)?;
    // macOS's own limiter, which mastering used to put on, makes way for the built-in one; and
    // the fader after it goes back to 0 dB, the limiter's ceiling being what nothing passes.
    let mut replaced = 0;
    while let Some(index) = cli::project(dir)?
        .master_inserts
        .iter()
        .position(|r| r.class == "AUPeakLimiter")
    {
        cli::edit_with(
            dir,
            Command::SetInsert {
                channel: ChannelRef::Master,
                index,
                plugin: None,
            },
        )?;
        replaced += 1;
    }
    cli::edit_with(
        dir,
        Command::SetMaster {
            volume_db: Some(0.0),
            pan_law: None,
        },
    )?;
    let mut p = cli::project(dir)?;
    p.master_limit = Some(Limiter {
        ceiling_db: -1.0,
        true_peak: true,
        ..p.master_limit.unwrap_or_default()
    });
    let Some(learned) = learn_limit(&p, dir, ChannelRef::Master, target)? else {
        return Err("the song is silent: nothing to master".into());
    };
    let mut limit = learned.limit;
    let set = |l: &Limiter| {
        let limit = serde_json::to_value(l).map_err(|e| e.to_string())?;
        cli::edit_with(
            dir,
            Command::SetLimiter {
                channel: ChannelRef::Master,
                limit: Some(limit),
                which: 0,
            },
        )
    };
    set(&limit)?;
    // Heard as it plays: what comes after the limiter, if anything, moves it a little.
    let mut end = cli::report(&cli::project(dir)?, dir, None, false)?.mix;
    let off = target - end.lufs;
    if off.abs() > 0.3 && limit.gain_db + off > 0.0 && limit.gain_db + off < 30.0 {
        limit.gain_db = ((limit.gain_db + off * 1.2) * 10.0).round() / 10.0;
        set(&limit)?;
        end = cli::report(&cli::project(dir)?, dir, None, false)?.mix;
    }
    let peak = end.peak_db[0].max(end.peak_db[1]);
    let short = target - end.lufs;
    Ok(json!({
        "note": if short > 1.0 { format!("{short:.1} dB short: the mix is too quiet for the limiter to reach it cleanly; mix again (mix) and master") } else { String::new() },
        "target_lufs": target,
        "lufs": (end.lufs * 10.0).round() / 10.0,
        "peak_db": (peak * 10.0).round() / 10.0,
        "crest_db": (end.crest_db * 10.0).round() / 10.0,
        "tonal_balance": tonal,
        "limiter": format!(
            "the master's limiter: {}, +{:.1} dB into a {:.1} dBTP ceiling, release {} ms (auto)",
            limit.style.name(),
            limit.gain_db,
            limit.ceiling_db,
            limit.release_ms
        ),
        "replaced": if replaced > 0 { "macOS's peak limiter, which mastering used before" } else { "" },
    }))
}

/// How an agent makes a song that sounds finished, in any style: by writing it, as a producer
/// would. Nothing in Pina Studio writes parts for it.
pub const WORKFLOW: &str = "Nothing is precoded: you write the song, every note, in whatever style \
the person asks for. Playbooks are the person's own: follow one only when they ask for it \
(playbook <name>). \
0. Hear what they want: the style, mood and songs they name (study <file> takes one apart: take \
its tempo, key, form and groove as ideas, not a template). \
1. Plan the whole song before writing: tempo, key, chords, and a map of its sections (bars, \
what plays, how much energy), where it builds and where it lets go. set_tempo, set_key, \
set_chords, and markers for the sections. \
2. Sounds: the person's palette first (palette), then samples_find, samples_kit and sounds for \
the rest; real instruments and samples, or Pina Studio's own synth (set_synth: a patch, then its wavetables, warps, unison, filters, envelopes, LFOs and matrix shaped for the part), never the sine synth. Drums and percussion as sampler \
zones, the pitched ones tuned to the key. \
3. Write every part yourself, a section at a time, with add_midi_region (or add_pattern_region \
for steps): drums with a feel (your own swing, accents, ghost notes, velocities that vary, \
timing nudged a little), a bass that sits with the kick and leads into the next chord, chords \
voiced to move little, a melody from a motif you repeat, vary and answer. Give each section its \
own: what plays, how dense, fills and turnarounds into the next, call and response; copy_bars \
only for bars you then change. Never one loop pasted through the song. \
4. Transitions: write the fills, rolls and kick drops yourself; transition for riser, reverse, \
impact and gap sounds, timed to land on the downbeat. \
5. critique quick after each part and fix every \"fix\"; render a bar range and analyze it. \
6. Mix: levels by part, EQ what clashes, space (reverb, delay) and movement (automation), with \
mix as the start (it sets each channel by listening); then shape each part's built-in effects \
for the song with all they have: the saturator's bands and styles (tape, tube, transformer, \
crunch, heavy, rectify, foldback, lo-fi), drive, tone, dynamics and feedback on the key; the \
compressor's style, parallel mix and spectral; the EQ's dynamic, spectral and mid/side bands, \
sidechains, character and phase; stack them where a part wants it (\"which\": 1 adds a second: \
an EQ after a saturator, a fast compressor then a slow one); set_fx stacks the tools: filter for \
sweeps and wahs, motion for chorus, flanger, phaser, tremolo, shaper for a hit's punch or tail, \
gate to close what rings, de_esser and tune on anything sung, stereo for each band's width, \
multiband to glue a bus, pitch for harmonies, tape_stop to throw a transition; learn sets one \
effect by listening. \
Then critique (it renders) \
and fix what it hears. \
7. master. 8. critique once more; say what you made and what's left. \
Asked for files (a master, stems, an MP3): bounce, with the formats, bits, rate and loudness asked.";
