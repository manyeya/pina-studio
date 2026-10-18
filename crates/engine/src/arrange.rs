//! A loop made into a song: sections laid out from it with the parts each one has, bars copied,
//! and the transitions between sections (risers, reverse cymbals, impacts, fills, gaps), their
//! sounds made here so they don't need a sample library.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    look::{self, Family},
    playbook,
    project::{
        AUDIO_DIR, AutoPoint, ChordSpan, Content, Instrument, Marker, Note, Project, Region, Track,
    },
};

/// What part a track plays in a song: what it's called, else what's written on it, else what
/// it sounds like.
pub fn role(t: &Track) -> &'static str {
    let name = t.name.to_lowercase();
    let says = |words: &[&str]| {
        words.iter().any(|w| {
            name.split(|c: char| !c.is_alphanumeric()).any(|x| x == *w)
                || name.contains(w) && w.len() > 3
        })
    };
    const BY_NAME: &[(&str, &[&str])] = &[
        (
            "fx",
            &["fx", "sfx", "riser", "impact", "sweep", "transition"],
        ),
        (
            "perc",
            &[
                "perc",
                "percussion",
                "conga",
                "bongo",
                "shaker",
                "tamb",
                "cowbell",
            ],
        ),
        (
            "drums",
            &[
                "drums", "drum", "kit", "beat", "kick", "snare", "hats", "hat", "clap", "break",
            ],
        ),
        ("bass", &["bass", "808", "sub", "log"]),
        ("vocals", &["vocal", "vocals", "vox", "voice", "choir"]),
        (
            "pad",
            &["pad", "pads", "strings", "string", "atmos", "atmosphere"],
        ),
        ("arp", &["arp", "arpeggio", "sequence"]),
        ("counter", &["counter"]),
        (
            "chords",
            &[
                "chords", "chord", "keys", "piano", "rhodes", "ep", "organ", "guitar", "stab",
                "stabs",
            ],
        ),
        (
            "lead",
            &[
                "lead", "melody", "hook", "bell", "bells", "pluck", "flute", "horn", "sax",
                "trumpet",
            ],
        ),
        (
            "texture",
            &["texture", "vinyl", "noise", "rain", "ambience", "foley"],
        ),
    ];
    if let Some((role, _)) = BY_NAME.iter().find(|(_, words)| says(words)) {
        return role;
    }
    // What tempo wrote on it.
    match t.regions.first().map(|r| r.name.as_str()) {
        Some("Chords") => return "chords",
        Some("Bass") => return "bass",
        Some("Melody") => return "lead",
        Some("Counter") => return "counter",
        Some("Arp") => return "arp",
        Some("Drums") => return "drums",
        _ => {}
    }
    if t.instrument.as_ref().is_some_and(Instrument::is_kit) {
        return "drums";
    }
    match look::look(t, &|_| None).family {
        Family::Drums => "drums",
        Family::Percussion => "perc",
        Family::Bass => "bass",
        Family::Keys | Family::Guitar => "chords",
        Family::Strings | Family::Pad => "pad",
        Family::Vocals => "vocals",
        Family::Fx => "fx",
        Family::Texture => "texture",
        Family::Brass | Family::Woodwind | Family::Synth | Family::Bells => "lead",
        Family::Other => "other",
    }
}

/// The part of a region from `from` to `to` (song ticks), as a region of its own that plays just
/// what it did there: notes moved, a pattern's steps turned so they line up, audio's offset on.
/// None if it has nothing there.
pub fn clip(p: &Project, r: &Region, from: u64, to: u64) -> Option<Region> {
    let (s, e) = (r.start.max(from), (r.start + r.length).min(to));
    if s >= e {
        return None;
    }
    let skip = s - r.start;
    let length = e - s;
    let content = match &r.content {
        Content::Midi { notes, controls } => Content::Midi {
            notes: notes
                .iter()
                .filter(|n| n.start >= skip as i64 && n.start < (skip + length) as i64)
                .map(|n| Note {
                    start: n.start - skip as i64,
                    ..*n
                })
                .collect(),
            // ponytail: controller moves are cut, not chased to where they stood at `from`.
            controls: controls
                .iter()
                .filter(|c| c.start >= skip as i64 && c.start < (skip + length) as i64)
                .map(|c| crate::project::ControlPoint {
                    start: c.start - skip as i64,
                    ..*c
                })
                .collect(),
        },
        Content::Pattern { step, rows, swing } => {
            let steps = rows.iter().map(|r| r.cells.len()).max().unwrap_or(1).max(1);
            let turn = ((skip / (*step).max(1)) as usize) % steps;
            Content::Pattern {
                step: *step,
                rows: rows
                    .iter()
                    .map(|row| {
                        let mut cells = row.cells.clone();
                        cells.resize(steps, 0);
                        cells.rotate_left(turn);
                        crate::project::PatternRow {
                            pitch: row.pitch,
                            cells,
                        }
                    })
                    .collect(),
                swing: *swing,
            }
        }
        Content::Audio {
            offset, stretch, ..
        } => {
            let mut c = r.content.clone();
            if let Content::Audio { offset: o, .. } = &mut c {
                *o = offset + (p.samples(skip, p.sample_rate as f64) as f64 / stretch) as u64;
            }
            c
        }
        // A take folder is kept whole or not at all.
        Content::Takes { .. } if skip == 0 && length == r.length => r.content.clone(),
        Content::Takes { .. } => return None,
    };
    Some(Region {
        start: s,
        length,
        link: None,
        looped: false,
        content,
        ..r.clone()
    })
}

/// Takes `from..to` out of a track: regions inside go, regions across its edges are cut back to
/// what's outside it.
pub fn clear(p: &mut Project, t: usize, from: u64, to: u64) {
    let regions = std::mem::take(&mut p.tracks[t].regions);
    let mut kept = Vec::new();
    for r in regions {
        let end = r.start + r.length;
        if end <= from || r.start >= to {
            kept.push(r);
            continue;
        }
        kept.extend(clip(p, &r, r.start, from));
        kept.extend(clip(p, &r, to, end));
    }
    p.tracks[t].regions = kept;
}

/// A track's regions from `from` for `length` ticks, cut to fit, their starts made relative.
fn take(p: &Project, t: usize, from: u64, length: u64) -> Vec<Region> {
    p.tracks[t]
        .regions
        .iter()
        .filter_map(|r| clip(p, r, from, from + length))
        .map(|r| Region {
            start: r.start - from,
            ..r
        })
        .collect()
}

/// Lays `pieces` (relative starts, a loop `length` long) from `at` to `end`, looping, the last
/// time cut short.
fn lay(
    p: &mut Project,
    t: usize,
    pieces: &[Region],
    length: u64,
    at: u64,
    end: u64,
    velocity: i8,
) -> usize {
    let mut placed = 0;
    let mut k = at;
    while k < end {
        for r in pieces {
            let moved = Region {
                start: k + r.start,
                ..r.clone()
            };
            if let Some(mut r) = clip(p, &moved, k, end) {
                r.velocity = (r.velocity as i16 + velocity as i16).clamp(-64, 64) as i8;
                p.tracks[t].regions.push(r);
                placed += 1;
            }
        }
        k += length.max(1);
    }
    placed
}

/// The chord track's chords from `from` for `length`, relative.
fn take_chords(p: &Project, from: u64, length: u64) -> Vec<ChordSpan> {
    p.chords
        .iter()
        .filter(|c| c.start < from + length && c.start + c.length > from)
        .map(|c| {
            let s = c.start.max(from);
            let e = (c.start + c.length).min(from + length);
            ChordSpan {
                start: s - from,
                length: e - s,
                chord: c.chord.clone(),
            }
        })
        .collect()
}

fn lay_chords(p: &mut Project, chords: &[ChordSpan], length: u64, at: u64, end: u64) {
    p.chords
        .retain(|c| c.start + c.length <= at || c.start >= end);
    let mut k = at;
    while k < end {
        for c in chords {
            let s = k + c.start;
            if s < end {
                p.chords.push(ChordSpan {
                    start: s,
                    length: c.length.min(end - s),
                    chord: c.chord.clone(),
                });
            }
        }
        k += length.max(1);
    }
    p.chords.sort_by_key(|c| c.start);
}

/// A section asked for: its name, bars, and the parts in it, by role ("drums", "lead") or track
/// number ("3").
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SectionIn {
    pub name: String,
    pub bars: u64,
    pub parts: Vec<String>,
    /// 1 to 5: how hard it plays, which sets its parts' velocity (4 as written).
    #[serde(default)]
    pub energy: Option<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Arrangement {
    /// Where the loop the song's made from starts (ticks): 0 by default.
    #[serde(default)]
    pub from: Option<u64>,
    /// How many bars it is: what's there, up to 16, by default.
    #[serde(default)]
    pub bars: Option<u64>,
    /// The sections, in order; the genre playbook's by default.
    #[serde(default)]
    pub sections: Option<Vec<SectionIn>>,
    /// The part a track plays, where tempo would guess wrong: {"3": "lead"}.
    #[serde(default)]
    pub roles: Option<BTreeMap<String, String>>,
}

pub fn arrange(p: &mut Project, a: Arrangement) -> Result<Value, String> {
    let bar = p.bar();
    let from = a.from.unwrap_or(0);
    let loop_bars = match a.bars {
        Some(b) if b > 0 => b,
        Some(_) => return Err("bars must be at least 1".into()),
        None => p.end().saturating_sub(from).div_ceil(bar).clamp(1, 16),
    };
    let length = loop_bars * bar;
    if !p.tracks.iter().any(|t| {
        t.regions
            .iter()
            .any(|r| r.start < from + length && r.start + r.length > from)
    }) {
        return Err("nothing's there to arrange: write a loop first".into());
    }
    // A track in no section's parts: with the genre's sections, it plays in the loud ones (its
    // part's one the genre doesn't name); with sections given, it's silent.
    let fallback = a.sections.is_none();
    let sections: Vec<SectionIn> = match a.sections {
        Some(s) if !s.is_empty() => s,
        Some(_) => return Err("no sections".into()),
        None => {
            let genre = p
                .genre
                .as_deref()
                .ok_or("give sections, or set_genre so the genre's are used")?;
            let book =
                playbook::find(genre).ok_or(format!("no playbook for {genre:?}: give sections"))?;
            book.sections
                .iter()
                .map(|s| SectionIn {
                    name: s.name.into(),
                    bars: s.bars,
                    parts: s.parts.iter().map(|x| x.to_string()).collect(),
                    energy: Some(s.energy),
                })
                .collect()
        }
    };
    if sections.iter().any(|s| s.bars == 0) {
        return Err("a section needs at least a bar".into());
    }
    let roles: Vec<String> = (0..p.tracks.len())
        .map(|t| {
            a.roles
                .as_ref()
                .and_then(|r| r.get(&t.to_string()))
                .cloned()
                .unwrap_or_else(|| role(&p.tracks[t]).to_string())
        })
        .collect();
    let named: Vec<&str> = sections
        .iter()
        .flat_map(|s| s.parts.iter().map(String::as_str))
        .collect();
    let in_section = |s: &SectionIn, t: usize| -> bool {
        let unnamed =
            !named.contains(&roles[t].as_str()) && !named.contains(&t.to_string().as_str());
        s.parts
            .iter()
            .any(|x| *x == roles[t] || *x == t.to_string())
            || fallback && unnamed && s.energy.unwrap_or(4) >= 4
    };
    // The loop, then the room it goes in.
    let pieces: Vec<Vec<Region>> = (0..p.tracks.len())
        .map(|t| take(p, t, from, length))
        .collect();
    let chords = take_chords(p, from, length);
    let total: u64 = sections.iter().map(|s| s.bars * bar).sum();
    for t in 0..p.tracks.len() {
        clear(p, t, from, from + total);
    }
    p.markers
        .retain(|m| m.tick < from || m.tick >= from + total);
    let mut at = from;
    let mut laid = Vec::new();
    // Each track's level, section by section, where its sections' energies differ: a synth
    // that ignores velocity still plays a quiet intro quietly, and a limiter can't flatten it.
    let mut levels: Vec<Vec<AutoPoint>> = vec![Vec::new(); p.tracks.len()];
    for s in &sections {
        let end = at + s.bars * bar;
        let energy = s.energy.unwrap_or(4).clamp(1, 5);
        let velocity = (energy as i8 - 4) * 4;
        let db = [-8.0, -5.0, -3.0, -1.5, 0.0][energy as usize - 1];
        for t in (0..p.tracks.len()).filter(|&t| in_section(s, t) && !pieces[t].is_empty()) {
            let v = p.tracks[t].volume_db + db;
            levels[t].push(AutoPoint { tick: at, value: v });
            levels[t].push(AutoPoint {
                tick: end - 1,
                value: v,
            });
        }
        let mut playing = Vec::new();
        for t in 0..p.tracks.len() {
            if in_section(s, t) && !pieces[t].is_empty() {
                lay(p, t, &pieces[t], length, at, end, velocity);
                playing.push(format!("{} ({})", p.tracks[t].name, roles[t]));
            }
        }
        if !chords.is_empty() {
            lay_chords(p, &chords, length, at, end);
        }
        p.markers.push(Marker {
            tick: at,
            name: s.name.clone(),
        });
        laid.push(
            json!({ "name": s.name, "bar": at / bar + 1, "bars": s.bars, "playing": playing }),
        );
        at = end;
    }
    p.markers.sort_by_key(|m| m.tick);
    for (t, points) in levels.into_iter().enumerate() {
        let first = points.first().map(|x| x.value);
        if points.iter().all(|x| Some(x.value) == first) {
            continue;
        }
        let auto = &mut p.tracks[t].automation.volume;
        auto.retain(|x| x.tick < from || x.tick >= from + total);
        auto.extend(points);
        auto.sort_by_key(|x| x.tick);
    }
    let unused: Vec<String> = (0..p.tracks.len())
        .filter(|&t| !pieces[t].is_empty() && !sections.iter().any(|s| in_section(s, t)))
        .map(|t| format!("{} ({})", p.tracks[t].name, roles[t]))
        .collect();
    let mut out = json!({ "sections": laid, "bars": total / bar });
    if !unused.is_empty() {
        out["silent"] = json!(unused);
    }
    Ok(out)
}

/// Copies bars `from..from+bars` of some tracks (all by default) to `to`, replacing what's there.
pub fn copy_bars(
    p: &mut Project,
    from: u64,
    bars: u64,
    to: u64,
    tracks: Option<Vec<usize>>,
) -> Result<Value, String> {
    let length = bars.max(1) * p.bar();
    let tracks = tracks.unwrap_or_else(|| (0..p.tracks.len()).collect());
    if let Some(t) = tracks.iter().find(|&&t| t >= p.tracks.len()) {
        return Err(format!("no track {t}"));
    }
    let mut placed = 0;
    for &t in &tracks {
        let pieces = take(p, t, from, length);
        clear(p, t, to, to + length);
        placed += lay(p, t, &pieces, length, to, to + length, 0);
    }
    let chords = take_chords(p, from, length);
    if !chords.is_empty() {
        lay_chords(p, &chords, length, to, to + length);
    }
    Ok(json!({ "regions": placed }))
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Move {
    /// Noise and a tone sweeping up, ending on the bar.
    Riser,
    /// A cymbal played backwards, swelling into the bar.
    Reverse,
    /// A low boom on the bar.
    Impact,
    /// A sweep falling away from the bar.
    Downlifter,
    /// A drum fill in the bar before, a crash on the bar.
    Fill,
    /// Everything but the FX stops for the beat before the bar.
    Gap,
    /// A riser, a fill, a beat's gap and an impact: into a drop or chorus.
    Build,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    /// The downbeat it leads into (ticks): a section's start.
    pub at: u64,
    #[serde(rename = "kind")]
    pub kind: Move,
    /// How many bars a riser, reverse or downlifter lasts (2, 1, 2 by default); a gap's beats
    /// (1).
    #[serde(default)]
    pub length: Option<u64>,
    /// The track: the drums for a fill, else where the FX go ("FX", made if there's none).
    #[serde(default)]
    pub track: Option<usize>,
}

/// The track the FX go on: one called FX that plays audio, made at the end if there's none.
fn fx_track(p: &mut Project) -> usize {
    if let Some(t) = p
        .tracks
        .iter()
        .position(|t| t.instrument.is_none() && role(t) == "fx")
    {
        return t;
    }
    let at = p.tracks.len();
    p.add_track(
        at,
        Track {
            name: "FX".into(),
            volume_db: -6.0,
            ..Default::default()
        },
    );
    at
}

pub fn transition(p: &mut Project, dir: &Path, tr: Transition) -> Result<Value, String> {
    let (bar, beat) = (p.bar(), p.beat());
    let at = tr.at;
    let mut done = Vec::new();
    let kinds: &[Move] = match tr.kind {
        Move::Build => &[Move::Riser, Move::Fill, Move::Gap, Move::Impact],
        k => std::slice::from_ref(match k {
            Move::Riser => &Move::Riser,
            Move::Reverse => &Move::Reverse,
            Move::Impact => &Move::Impact,
            Move::Downlifter => &Move::Downlifter,
            Move::Fill => &Move::Fill,
            _ => &Move::Gap,
        }),
    };
    for &kind in kinds {
        let given = if tr.kind == Move::Build {
            None
        } else {
            tr.length
        };
        match kind {
            Move::Fill => {
                let t = match tr.track.filter(|_| tr.kind == Move::Fill) {
                    Some(t) => t,
                    None => (0..p.tracks.len())
                        .find(|&t| {
                            role(&p.tracks[t]) == "drums" && p.tracks[t].instrument.is_some()
                        })
                        .ok_or("no drum track for a fill: give track")?,
                };
                fill(p, t, at)?;
                done.push(format!("fill on {}", p.tracks[t].name));
            }
            Move::Gap => {
                let beats = given.unwrap_or(1).max(1);
                let from = at.saturating_sub(beats * beat);
                for t in 0..p.tracks.len() {
                    if role(&p.tracks[t]) != "fx" {
                        clear(p, t, from, at);
                    }
                }
                done.push(format!("{beats} beat gap"));
            }
            sound => {
                let bars = given.unwrap_or(match sound {
                    Move::Reverse => 1,
                    Move::Impact => 2,
                    _ => 2,
                });
                let (start, length) = match sound {
                    Move::Riser | Move::Reverse => {
                        (at.saturating_sub(bars * bar), (bars * bar).min(at))
                    }
                    _ => (at, bars * bar),
                };
                if length == 0 {
                    return Err("a riser needs bars before it: at is the song's start".into());
                }
                let file = make_fx(p, dir, sound, length)?;
                let t = match tr.track.filter(|_| tr.kind != Move::Build) {
                    Some(t) if t < p.tracks.len() && p.tracks[t].instrument.is_none() => t,
                    Some(t) => return Err(format!("track {t} isn't an audio track for FX")),
                    None => fx_track(p),
                };
                p.tracks[t].regions.push(Region {
                    name: format!("{sound:?}"),
                    start,
                    length,
                    content: Content::Audio {
                        file,
                        offset: 0,
                        stretch: 1.0,
                        flex: Default::default(),
                        follow: false,
                        formants: false,
                        warps: Vec::new(),
                    },
                    ..Default::default()
                });
                done.push(format!(
                    "{} on {}",
                    format!("{sound:?}").to_lowercase(),
                    p.tracks[t].name
                ));
            }
        }
    }
    Ok(json!({ "done": done }))
}

/// A fill in the bar before `at` on a drum track: its back half a snare run rising (toms at the
/// end, if the kit has them), then a crash and a kick on `at`.
fn fill(p: &mut Project, t: usize, at: u64) -> Result<(), String> {
    let (bar, beat) = (p.bar(), p.beat());
    let from = at.checked_sub(bar).ok_or("a fill needs a bar before it")?;
    let to = at + beat;
    // What plays there now, as notes in a region of its own.
    let mut notes: Vec<Note> = Vec::new();
    for r in &p.tracks[t].regions {
        if let Some(c) = clip(p, r, from, to)
            && let Some(list) = c.content.notes(c.length)
        {
            notes.extend(list.iter().map(|n| c.played(n)).map(|n| Note {
                start: n.start + (c.start - from) as i64,
                ..n
            }));
        }
    }
    clear(p, t, from, to);
    let half = (bar / 2) as i64;
    notes.retain(|n| n.start < half || n.start >= bar as i64);
    let has = |pitch: u8| match &p.tracks[t].instrument {
        Some(Instrument::Sampler(s)) => s
            .zones
            .iter()
            .any(|z| (z.note..=z.high.unwrap_or(z.note)).contains(&pitch)),
        _ => true,
    };
    let snare = [38, 39, 40, 37].into_iter().find(|&n| has(n)).unwrap_or(38);
    let toms: Vec<u8> = [48, 45, 41].into_iter().filter(|&n| has(n)).collect();
    let sixteenth = (beat / 4) as i64;
    let n = half / sixteenth;
    for k in 0..n {
        let pitch = match (toms.len(), k >= n - 4) {
            (2.., true) => toms[((k - (n - 4)) as usize * toms.len() / 4).min(toms.len() - 1)],
            _ => snare,
        };
        notes.push(Note {
            start: half + k * sixteenth,
            length: sixteenth as u64,
            pitch,
            velocity: (60 + k * 60 / n.max(1)) as u8,
        });
    }
    for pitch in [36, 49] {
        if has(pitch) {
            notes.push(Note {
                start: bar as i64,
                length: beat,
                pitch,
                velocity: 116,
            });
        }
    }
    notes.sort_by_key(|n| (n.start, n.pitch));
    p.tracks[t].regions.push(Region {
        name: "Fill".into(),
        start: from,
        length: to - from,
        content: Content::Midi {
            notes,
            controls: Vec::new(),
        },
        ..Default::default()
    });
    Ok(())
}

/// Makes a transition's sound, `ticks` long at the song's tempo, in the project's audio folder
/// (once: the same sound and length is the same file).
fn make_fx(p: &Project, dir: &Path, kind: Move, ticks: u64) -> Result<PathBuf, String> {
    let rate = p.sample_rate;
    let frames = p.samples(ticks, rate as f64) as usize;
    let name = format!(
        "{AUDIO_DIR}/tempo {} {}.wav",
        format!("{kind:?}").to_lowercase(),
        frames
    );
    let path = dir.join(&name);
    if path.exists() {
        return Ok(PathBuf::from(name));
    }
    let [l, r] = synth_fx(kind, frames, rate as f32);
    std::fs::create_dir_all(dir.join(AUDIO_DIR)).map_err(|e| e.to_string())?;
    crate::analysis::write_wav(&path, &l, &r, rate)?;
    Ok(PathBuf::from(name))
}

/// A state-variable filter (Simper's): low, band and high pass at once.
#[derive(Default, Clone, Copy)]
struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    fn run(&mut self, x: f32, hz: f32, q: f32, rate: f32) -> (f32, f32, f32) {
        let g = (std::f32::consts::PI * hz.clamp(20.0, rate * 0.45) / rate).tan();
        let k = 1.0 / q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let (a2, a3) = (g * a1, g * g * a1);
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, v1, x - k * v1 - v2)
    }
}

/// White noise, the same every time.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

/// A transition's sound, `frames` long, peaking at -3 dBFS.
fn synth_fx(kind: Move, frames: usize, rate: f32) -> [Vec<f32>; 2] {
    let mut out = [vec![0f32; frames], vec![0f32; frames]];
    let (mut nl, mut nr) = (Noise(0x1234_5678), Noise(0x9abc_def1));
    let (mut fl, mut fr, mut tone_f) = (Svf::default(), Svf::default(), Svf::default());
    let n = frames.max(1) as f32;
    let mut phase = 0f32;
    let [out_l, out_r] = &mut out;
    for (i, (ol, or)) in out_l.iter_mut().zip(out_r.iter_mut()).enumerate() {
        let x = i as f32 / n;
        let secs = i as f32 / rate;
        let (a, b) = (nl.next(), nr.next());
        let (l, r) = match kind {
            Move::Riser => {
                // Band-passed noise rising 200 Hz to 12 kHz, widening, with a saw sweeping up
                // two octaves under it.
                let hz = 200.0 * 60f32.powf(x * x);
                let width = 0.2 + 0.8 * x;
                let mid = (a + b) * 0.5;
                let (_, bl, _) = fl.run(mid + (a - mid) * width, hz, 2.5, rate);
                let (_, br, _) = fr.run(mid + (b - mid) * width, hz, 2.5, rate);
                phase = (phase + 110.0 * 4f32.powf(x) / rate).fract();
                let (saw, _, _) = tone_f.run(phase * 2.0 - 1.0, 400.0 + 4000.0 * x, 0.7, rate);
                let env = x.powf(2.2);
                ((bl * 1.6 + saw * 0.25) * env, (br * 1.6 + saw * 0.25) * env)
            }
            Move::Downlifter => {
                let hz = 9000.0 * (1.0 / 45.0f32).powf(x.sqrt());
                let (_, bl, _) = fl.run(a, hz, 2.0, rate);
                let (_, br, _) = fr.run(b, hz, 2.0, rate);
                let env = (1.0 - x).powf(2.0);
                (bl * env * 1.6, br * env * 1.6)
            }
            Move::Reverse => {
                // A cymbal's wash (high-passed noise decaying), heard backwards: so it swells.
                let back = 1.0 - x;
                let (_, _, hl) = fl.run(a, 5000.0, 0.7, rate);
                let (_, _, hr) = fr.run(b, 5000.0, 0.7, rate);
                let env = (-back * 5.0).exp() * x.min(0.98).powf(0.3) * (1.0 - x).min(0.02) * 50.0;
                (hl * env, hr * env)
            }
            _ => {
                // An impact: a sine dropping 70 to 32 Hz, a short noise thump, and a dark tail.
                let hz = 32.0 + 38.0 * (-secs * 6.0).exp();
                phase = (phase + hz / rate).fract();
                let boom = (phase * std::f32::consts::TAU).sin() * (-secs * 1.4).exp();
                let (ll, _, _) = fl.run(a, 900.0 * (-secs * 3.0).exp() + 120.0, 0.7, rate);
                let (lr, _, _) = fr.run(b, 900.0 * (-secs * 3.0).exp() + 120.0, 0.7, rate);
                let thump = (-secs * 9.0).exp() * 1.4 + (-secs * 1.2).exp() * 0.25;
                (boom + ll * thump, boom + lr * thump)
            }
        };
        (*ol, *or) = (l, r);
    }
    let peak = out.iter().flatten().fold(0f32, |m, v| m.max(v.abs()));
    if peak > 0.0 {
        let gain = 0.708 / peak;
        out.iter_mut().flatten().for_each(|v| *v *= gain);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose::{self, ChordPart, DrumPart};

    fn sketch() -> Project {
        let mut p = Project::new(48_000);
        p.key = Some("A minor".into());
        p.genre = Some("trap".into());
        for (name, instrument) in [
            ("Drums", Instrument::Drums),
            ("808", Instrument::Synth),
            ("Bells", Instrument::Synth),
            ("Pad", Instrument::Synth),
        ] {
            p.tracks.push(Track {
                name: name.into(),
                instrument: Some(instrument),
                ..Default::default()
            });
        }
        compose::set_chords(&mut p, Some("i | VI | III | VII"), None, None).unwrap();
        compose::write_drums(
            &mut p,
            DrumPart {
                track: 0,
                start: None,
                bars: None,
                style: None,
                fill_every: Some(0),
                seed: Some(1),
            },
        )
        .unwrap();
        for t in 1..4 {
            compose::write_chords(
                &mut p,
                ChordPart {
                    track: t,
                    start: None,
                    bars: None,
                    chords: None,
                    voicing: None,
                    rhythm: None,
                    register: None,
                    seed: None,
                },
            )
            .unwrap();
        }
        p
    }

    #[test]
    fn tracks_know_their_parts() {
        let p = sketch();
        let roles: Vec<&str> = p.tracks.iter().map(role).collect();
        assert_eq!(roles, ["drums", "bass", "lead", "pad"]);
    }

    #[test]
    fn clipping_a_pattern_keeps_its_steps_in_place() {
        let p = sketch();
        let r = &p.tracks[0].regions[0];
        let half = clip(&p, r, 3840 * 2, 3840 * 4).unwrap();
        let whole = r.content.notes(r.length).unwrap();
        let later: Vec<(i64, u8)> = whole
            .iter()
            .filter(|n| n.start >= 7680)
            .map(|n| (n.start - 7680, n.pitch))
            .collect();
        let clipped: Vec<(i64, u8)> = half
            .content
            .notes(half.length)
            .unwrap()
            .iter()
            .map(|n| (n.start, n.pitch))
            .collect();
        assert_eq!(clipped, later);
    }

    #[test]
    fn a_loop_becomes_a_song() {
        let mut p = sketch();
        let out = arrange(
            &mut p,
            Arrangement {
                from: None,
                bars: None,
                sections: None,
                roles: None,
            },
        )
        .unwrap();
        // Trap's playbook: 84 bars, its sections marked.
        assert_eq!(out["bars"], 84);
        assert_eq!(p.markers.len(), 7);
        assert_eq!(p.markers[1].name, "Hook");
        assert_eq!(p.markers[1].tick, 8 * 3840);
        // The intro has no drums; the hook does, all 16 bars, and the chords run the whole song.
        let drums_at = |p: &Project, bar: u64| {
            p.tracks[0]
                .regions
                .iter()
                .any(|r| r.start <= bar * 3840 && bar * 3840 < r.start + r.length)
        };
        assert!(!drums_at(&p, 2));
        assert!(drums_at(&p, 8) && drums_at(&p, 23));
        assert_eq!(p.chords.last().map(|c| c.start + c.length), Some(84 * 3840));
        // The song's quiet parts play softer, and lower.
        let intro = p.tracks[2].regions.iter().find(|r| r.start == 0).unwrap();
        assert!(intro.velocity < 0);
        let level = |t: usize, tick: u64| {
            p.tracks[t]
                .automation
                .volume
                .iter()
                .rev()
                .find(|x| x.tick <= tick)
                .unwrap()
                .value
        };
        assert!(level(2, 0) < level(2, 8 * 3840));
        // Sections can be given, by role or track.
        let out = arrange(
            &mut p,
            Arrangement {
                from: Some(8 * 3840),
                bars: Some(4),
                sections: Some(vec![
                    SectionIn {
                        name: "A".into(),
                        bars: 4,
                        parts: vec!["0".into()],
                        energy: None,
                    },
                    SectionIn {
                        name: "B".into(),
                        bars: 8,
                        parts: vec!["drums".into(), "lead".into()],
                        energy: Some(5),
                    },
                ]),
                roles: None,
            },
        )
        .unwrap();
        assert_eq!(out["sections"][1]["playing"].as_array().unwrap().len(), 2);
        assert_eq!(out["silent"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn transitions_lead_into_a_bar() {
        let dir = std::env::temp_dir().join(format!("tempo-arrange-{}", std::process::id()));
        let mut p = sketch();
        let out = transition(
            &mut p,
            &dir,
            Transition {
                at: 2 * 3840,
                kind: Move::Build,
                length: None,
                track: None,
            },
        )
        .unwrap();
        assert_eq!(out["done"].as_array().unwrap().len(), 4);
        let fx = p.tracks.iter().position(|t| t.name == "FX").unwrap();
        let riser = &p.tracks[fx].regions[0];
        assert_eq!((riser.start, riser.length), (0, 2 * 3840));
        let Content::Audio { file, .. } = &riser.content else {
            panic!()
        };
        let (l, _, _) = crate::analysis::read_wav(&dir.join(file)).unwrap();
        // It rises: the last quarter is louder than the first.
        let energy = |s: &[f32]| s.iter().map(|v| v * v).sum::<f32>();
        assert!(energy(&l[l.len() * 3 / 4..]) > energy(&l[..l.len() / 4]) * 10.0);
        // The beat before the bar is silent but for the FX, and the drums fill the bar before.
        let gap = 2 * 3840 - 960;
        for t in 0..4 {
            assert!(
                compose::heard_notes(&p, t)
                    .iter()
                    .all(|n| n.start < gap as i64 || n.start >= 2 * 3840),
                "{t}"
            );
        }
        let fill = p.tracks[0]
            .regions
            .iter()
            .find(|r| r.name == "Fill")
            .unwrap();
        assert_eq!(fill.start, 3840);
        std::fs::remove_dir_all(&dir).ok();
    }
}
