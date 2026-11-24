//! A song as a composer model reads it, and what it writes back. The score is MIDI-GPT's: every
//! part's bars, each bar's notes from its start, the shape its server takes and answers in. The
//! bars asked for are written; every other part the song plays is heard, so what's written fits it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    music::compose::heard_notes,
    song::project::{Note, PPQ, Project},
};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Score {
    /// Ticks in a quarter note.
    pub resolution: u32,
    /// Microseconds in a quarter note.
    pub tempo: u32,
    pub tracks: Vec<Track>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Track {
    /// Its General MIDI program: the instrument the model hears it as.
    pub instrument: u8,
    /// "melodic" or "drum".
    pub track_type: String,
    pub bars: Vec<Bar>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Bar {
    pub ts_numerator: u32,
    pub ts_denominator: u32,
    /// Quarter notes in the bar.
    #[serde(default = "four")]
    pub beat_length: f64,
    #[serde(default)]
    pub notes: Vec<ScoreNote>,
}

fn four() -> f64 {
    4.0
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ScoreNote {
    pub pitch: u8,
    pub velocity: u8,
    /// From the bar's start, in the score's ticks.
    pub onset_ticks: i64,
    pub duration_ticks: i64,
    /// Off the grid by this many ticks (models that play off it).
    #[serde(default)]
    pub delta: i64,
}

/// What a part plays, as General MIDI names it: the instrument a model hears it as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    Drums,
    Bass,
    Keys,
    Pad,
    Lead,
    Guitar,
    Strings,
}

impl Part {
    pub const ALL: [Part; 7] = [
        Part::Drums,
        Part::Bass,
        Part::Keys,
        Part::Pad,
        Part::Lead,
        Part::Guitar,
        Part::Strings,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Part::Drums => "Drums",
            Part::Bass => "Bass",
            Part::Keys => "Keys",
            Part::Pad => "Pad",
            Part::Lead => "Lead",
            Part::Guitar => "Guitar",
            Part::Strings => "Strings",
        }
    }

    pub fn named(s: &str) -> Option<Part> {
        Part::ALL
            .into_iter()
            .find(|p| p.name().eq_ignore_ascii_case(s))
    }

    /// Its General MIDI program (drums are told apart by their track's type instead).
    pub fn program(self) -> u8 {
        match self {
            Part::Drums | Part::Keys => 0,
            Part::Bass => 33,
            Part::Pad => 89,
            Part::Lead => 81,
            Part::Guitar => 25,
            Part::Strings => 48,
        }
    }

    /// What `notes` sound like they are, by the notes (never a name): low and one at a time a
    /// bass, three or more at once keys, otherwise a lead. None to go by: keys.
    pub fn heard(notes: &[Note]) -> Part {
        if notes.is_empty() {
            return Part::Keys;
        }
        let low = notes.iter().map(|n| n.pitch as f64).sum::<f64>() / (notes.len() as f64) < 48.0;
        let most = notes
            .iter()
            .map(|n| {
                notes
                    .iter()
                    .filter(|m| m.start <= n.start && n.start < m.start + m.length as i64)
                    .count()
            })
            .max()
            .unwrap_or(1);
        match (low, most) {
            (true, 1) => Part::Bass,
            (_, m) if m >= 3 => Part::Keys,
            _ => Part::Lead,
        }
    }
}

/// How a melodic part plays: chords, one note at a time, or as the model hears it. A model
/// carries on what a track already plays (a bass line stays a bass line), so chords are asked
/// for: MIDI-GPT's polyphony is how many notes start together, its level the count itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Plays {
    Any,
    Single,
    Chords,
}

impl Plays {
    pub const ALL: [Plays; 3] = [Plays::Any, Plays::Chords, Plays::Single];

    pub fn name(self) -> &'static str {
        match self {
            Plays::Any => "As the model hears it",
            Plays::Single => "Single notes",
            Plays::Chords => "Chords",
        }
    }

    pub fn named(s: &str) -> Option<Plays> {
        match s.to_ascii_lowercase().as_str() {
            "any" => Some(Plays::Any),
            "single" | "single notes" | "notes" => Some(Plays::Single),
            "chords" => Some(Plays::Chords),
            _ => None,
        }
    }

    /// `steer` (a part's controls) set to play this way: chords start three notes or more
    /// together, single notes one at a time.
    pub fn apply(self, steer: &mut BTreeMap<String, i64>) {
        steer.remove("min_polyphony");
        steer.remove("max_polyphony");
        match self {
            Plays::Any => {}
            Plays::Chords => {
                steer.insert("min_polyphony".into(), 3);
            }
            Plays::Single => {
                steer.insert("max_polyphony".into(), 1);
            }
        }
    }

    /// How `steer` has a part play.
    pub fn of(steer: &BTreeMap<String, i64>) -> Plays {
        match (steer.get("min_polyphony"), steer.get("max_polyphony")) {
            (Some(&m), _) if m >= 3 => Plays::Chords,
            (_, Some(&m)) if m <= 1 => Plays::Single,
            _ => Plays::Any,
        }
    }
}

/// What's asked of a model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ask {
    /// The song's tracks to write, each with what it plays (none: as its notes sound).
    pub tracks: Vec<(usize, Option<Part>)>,
    /// Which bars, from 0: from the first up to (not including) the second.
    pub bars: (u32, u32),
    /// How far it may stray from the likeliest note: 1 as trained, lower safer, higher wilder.
    pub temperature: f32,
    /// How many takes, each from its own seed.
    pub takes: u32,
    /// A take written again: its seed.
    pub seed: Option<i64>,
    /// Melodic parts kept to the song's key.
    pub in_key: bool,
    /// The model's own controls, by the names its `info` gives, each at a level: for every part,
    /// and for one part (by its song track) over those.
    pub attributes: BTreeMap<String, i64>,
    pub steer: BTreeMap<usize, BTreeMap<String, i64>>,
}

impl Default for Ask {
    fn default() -> Ask {
        Ask {
            tracks: Vec::new(),
            bars: (0, 4),
            temperature: 1.0,
            takes: 1,
            seed: None,
            in_key: true,
            attributes: BTreeMap::new(),
            steer: BTreeMap::new(),
        }
    }
}

/// A request ready for the model: its body, and which song track each of its parts is.
pub struct Prompt {
    pub body: Value,
    /// `order[i]` is the song track at the score's track `i`.
    pub order: Vec<usize>,
    /// Ticks in one of the song's bars.
    pub bar: u64,
}

/// The song as a score, and `ask` as the model's request. `caps` is what the model said it can
/// do (`server::info`): only the controls it has are asked for.
pub fn prompt(p: &Project, ask: &Ask, caps: &Value) -> Result<Prompt, String> {
    let (b0, b1) = ask.bars;
    if b0 >= b1 {
        return Err("no bars to write: the first must come before the last".into());
    }
    if ask.tracks.is_empty() {
        return Err("no part to write: pick a track".into());
    }
    for &(t, _) in &ask.tracks {
        match p.tracks.get(t) {
            None => return Err(format!("there's no track {t}")),
            Some(tr) if tr.instrument.is_none() => {
                return Err(format!("{} plays audio: a composer writes notes", tr.name));
            }
            _ => {}
        }
    }
    let bar = p.bar().max(1);
    let bars = (p.end().div_ceil(bar) as u32).max(b1) as usize;
    let target = |t: usize| {
        ask.tracks
            .iter()
            .find(|(x, _)| *x == t)
            .map(|(_, part)| *part)
    };
    // Every part that plays notes, heard; and those to write, even empty. Those asked to play
    // otherwise than they do (chords of a melody or bass line, single notes of chords) don't hear
    // their own other bars, or they'd carry on as they were.
    let mut order = Vec::new();
    let mut line = Vec::new();
    let mut tracks = Vec::new();
    for (t, tr) in p.tracks.iter().enumerate() {
        let Some(inst) = &tr.instrument else { continue };
        let notes = heard_notes(p, t);
        let asked = target(t);
        if asked.is_none() && notes.is_empty() {
            continue;
        }
        let drums = inst.is_kit() || asked.flatten() == Some(Part::Drums);
        // Chords asked of a part that sounds like a lead or a bass: heard as keys, an
        // instrument chords are written for (as a lead, the model mostly keeps to one note); and
        // the other way round. Only what the model hears: the track sounds as it does.
        let chords = ask.steer.get(&t).map(Plays::of) == Some(Plays::Chords);
        // Asked to play otherwise than it does (chords of a line, single notes of chords).
        let plays = ask.steer.get(&t).map_or(Plays::Any, Plays::of);
        let otherwise = match (plays, Part::heard(&notes)) {
            (Plays::Chords, Part::Lead | Part::Bass) => true,
            (Plays::Single, Part::Keys) => !notes.is_empty(),
            _ => false,
        };
        if otherwise && !drums {
            line.push(t);
        }
        let part = asked
            .flatten()
            .unwrap_or_else(|| match (drums, Part::heard(&notes)) {
                (true, _) => Part::Drums,
                (false, Part::Lead | Part::Bass) if chords => Part::Keys,
                // Single notes of what sounds like keys: heard as a lead, which plays them.
                (false, Part::Keys) if plays == Plays::Single => Part::Lead,
                (false, heard) => heard,
            });
        let [n, d] = p.time_signature;
        let mut score_bars: Vec<Bar> = (0..bars)
            .map(|_| Bar {
                ts_numerator: n as u32,
                ts_denominator: d as u32,
                beat_length: 4.0 * n as f64 / d.max(1) as f64,
                notes: Vec::new(),
            })
            .collect();
        for note in notes.iter().filter(|n| n.start >= 0) {
            let b = note.start as u64 / bar;
            if let Some(sb) = score_bars.get_mut(b as usize) {
                sb.notes.push(ScoreNote {
                    pitch: note.pitch,
                    velocity: note.velocity,
                    onset_ticks: (note.start as u64 - b * bar) as i64,
                    duration_ticks: note.length.max(1) as i64,
                    delta: 0,
                });
            }
        }
        order.push(t);
        tracks.push(Track {
            instrument: part.program(),
            track_type: if drums { "drum" } else { "melodic" }.into(),
            bars: score_bars,
        });
    }
    let score = Score {
        resolution: PPQ as u32,
        tempo: (60_000_000.0 / p.tempo.max(1.0)).round() as u32,
        tracks,
    };
    let key_mask = ask.in_key.then(|| pitch_mask(p, caps)).flatten();
    let prompts: Vec<Value> = order
        .iter()
        .enumerate()
        .map(|(i, &t)| {
            if target(t).is_none() {
                return json!({ "id": i, "bars": [] });
            }
            let drum = score.tracks[i].track_type == "drum";
            let mut asked = ask.attributes.clone();
            asked.extend(ask.steer.get(&t).cloned().unwrap_or_default());
            let mut tp = json!({
                "id": i,
                "bars": (b0..b1).collect::<Vec<u32>>(),
                "attributes": attributes(&asked, caps, drum),
            });
            if line.contains(&t) {
                tp["mask_bars"] = json!(
                    (0..bars as u32)
                        .filter(|b| !(b0..b1).contains(b))
                        .collect::<Vec<u32>>()
                );
            }
            if let (Some(mask), false) = (&key_mask, drum) {
                tp["controls"] = json!({ "pitch_mask": mask });
            }
            tp
        })
        .collect();
    let body = json!({
        "score": score,
        "request": {
            "tracks": prompts,
            "config": {
                "temperature": ask.temperature.clamp(0.1, 2.0),
                "seed": -1,
                "model_dim": "auto",
            },
        },
        "stream": true,
    });
    Ok(Prompt { body, order, bar })
}

impl Prompt {
    /// The request for take `k` (from 0). Takes are asked one at a time, each streamed as it's
    /// written (the server streams only one take per request), so the first can be heard while
    /// the next is writing. A seed gives the first take and the rest follow from it, so the same
    /// seed writes the same takes again.
    pub fn take(&self, k: u32, seed: Option<i64>) -> Value {
        let mut body = self.body.clone();
        body["request"]["config"]["seed"] = json!(seed.map_or(-1, |s| s + k as i64));
        body
    }
}

/// The song's key as the model's pitch mask, if it has a preset for the key's scale.
fn pitch_mask(p: &Project, caps: &Value) -> Option<Value> {
    let key = p.song_key()?;
    let presets: Vec<&str> = caps["capabilities"]["pitch_mask_scale_presets"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let names: &[&str] = match key.minor {
        true => &["minor", "natural_minor", "aeolian"],
        false => &["major", "ionian"],
    };
    let scale = names.iter().find(|n| presets.contains(n))?;
    Some(json!({ "scale": scale, "root": key.root % 12 }))
}

/// The controls asked for that the model has for this kind of part, each kept within its levels.
fn attributes(asked: &BTreeMap<String, i64>, caps: &Value, drum: bool) -> Value {
    let mut out = serde_json::Map::new();
    for (name, &level) in asked {
        let Some(levels) = caps["attributes"][name].as_i64() else {
            continue;
        };
        let fits = match caps["attribute_track_types"][name].as_str() {
            Some("drum") => drum,
            Some("melodic") => !drum,
            _ => true,
        };
        if fits {
            out.insert(name.clone(), json!(level.clamp(0, (levels - 1).max(0))));
        }
    }
    Value::Object(out)
}

/// Notes streamed while a take is written, as the song's notes: by its track, at their ticks in
/// the song. `resolution` is the ticks a quarter they come in: what the model writes its answers
/// at (its score's resolution), which isn't `info`'s (the model's own grid).
pub fn written(
    notes: &[super::server::Written],
    prompt: &Prompt,
    resolution: u32,
) -> Vec<(usize, Note)> {
    let scale = PPQ as f64 / resolution.max(1) as f64;
    notes
        .iter()
        .filter_map(|n| {
            let t = *prompt.order.get(n.track)?;
            Some((
                t,
                Note {
                    start: (n.bar * prompt.bar) as i64
                        + (n.onset_ticks as f64 * scale).round() as i64,
                    length: ((n.duration_ticks as f64 * scale).round() as u64).max(1),
                    pitch: n.pitch,
                    velocity: n.velocity.clamp(1, 127),
                    ..Default::default()
                },
            ))
        })
        .collect()
}

/// One take: the notes written, by the song's track, at their ticks in the song.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Take {
    pub seed: i64,
    pub notes: BTreeMap<usize, Vec<Note>>,
}

/// The model's answer as takes: for each part asked for, the bars it wrote.
pub fn takes(answer: &Value, prompt: &Prompt, ask: &Ask) -> Result<Vec<Take>, String> {
    let scores: Vec<(Value, i64)> = match answer["candidates"].as_array() {
        Some(c) => c
            .iter()
            .filter(|c| !c["score"].is_null())
            .map(|c| (c["score"].clone(), c["seed"].as_i64().unwrap_or(-1)))
            .collect(),
        None => vec![(
            answer["score"].clone(),
            answer["seed"].as_i64().unwrap_or(-1),
        )],
    };
    if scores.iter().all(|(s, _)| s.is_null()) {
        return Err(answer["candidates"][0]["error"]
            .as_str()
            .unwrap_or("the model wrote nothing")
            .to_string());
    }
    let mut out = Vec::new();
    for (score, seed) in scores {
        let score: Score = serde_json::from_value(score).map_err(|e| format!("its answer: {e}"))?;
        let scale = PPQ as f64 / score.resolution.max(1) as f64;
        let mut take = Take {
            seed,
            notes: BTreeMap::new(),
        };
        for (i, &t) in prompt.order.iter().enumerate() {
            if !ask.tracks.iter().any(|(x, _)| *x == t) {
                continue;
            }
            let wrote: Vec<u64> = match answer["generated_bars"][i.to_string()].as_array() {
                Some(b) => b.iter().filter_map(Value::as_u64).collect(),
                None => (ask.bars.0 as u64..ask.bars.1 as u64).collect(),
            };
            let Some(track) = score.tracks.get(i) else {
                continue;
            };
            let mut notes = Vec::new();
            for b in wrote {
                let Some(bar) = track.bars.get(b as usize) else {
                    continue;
                };
                for n in &bar.notes {
                    let at = (n.onset_ticks + n.delta) as f64 * scale;
                    notes.push(Note {
                        start: (b * prompt.bar) as i64 + at.round() as i64,
                        length: ((n.duration_ticks as f64 * scale).round() as u64).max(1),
                        pitch: n.pitch,
                        velocity: n.velocity.clamp(1, 127),
                        ..Default::default()
                    });
                }
            }
            notes.sort_by_key(|n| (n.start, n.pitch));
            take.notes.insert(t, notes);
        }
        out.push(take);
    }
    Ok(out)
}
