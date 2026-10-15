//! A project: what's on the timeline and how it's mixed. It's a folder, like Logic's `.logicx`:
//! `project.json` plus the audio it uses under `Audio Files/`. The JSON is the source of truth, so
//! people and agents can read, diff and edit it.

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

/// Ticks per quarter note, as in Logic.
pub const PPQ: u64 = 960;
/// Beats to a bar: 4/4 throughout, for now.
pub const BEATS_PER_BAR: u64 = 4;

pub const FILE: &str = "project.json";
pub const AUDIO_DIR: &str = "Audio Files";
pub const STATES_DIR: &str = "Plugin States";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Project {
    /// Beats per minute.
    pub tempo: f64,
    pub sample_rate: u32,
    #[serde(default)]
    pub master_volume_db: f32,
    pub tracks: Vec<Track>,
    /// Aux channels: tracks send to them by bus number, and they play into the master.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auxes: Vec<Aux>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Track {
    /// Identifies the track while the project is open, so the engine keeps its instrument and
    /// plugins through edits and undo. Not saved.
    #[serde(skip)]
    pub id: u64,
    pub name: String,
    /// Software instrument tracks have one; audio tracks don't.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instrument: Option<Instrument>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inserts: Vec<PluginRef>,
    #[serde(default)]
    pub volume_db: f32,
    /// -1 is hard left, 1 hard right.
    #[serde(default)]
    pub pan: f32,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub solo: bool,
    #[serde(default)]
    pub regions: Vec<Region>,
    /// Volume and pan over time. A parameter with points follows them instead of its fader.
    #[serde(default, skip_serializing_if = "Automation::is_empty")]
    pub automation: Automation,
    /// Post-fader sends to aux buses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sends: Vec<AuxSend>,
    /// Live Loops cells: loops to launch, a region each, by scene.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<Cell>,
    /// The bus the track plays into instead of the master: an aux, or a summing stack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<u32>,
}

/// A Live Loops cell: a loop on a track, in a scene's column. Its region's start doesn't matter;
/// it loops over its length from wherever it's launched.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Cell {
    pub scene: usize,
    #[serde(flatten)]
    pub region: Region,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AuxSend {
    pub bus: u32,
    #[serde(default)]
    pub level_db: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Aux {
    /// As `Track::id`.
    #[serde(skip)]
    pub id: u64,
    /// The bus it listens to.
    pub bus: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inserts: Vec<PluginRef>,
    #[serde(default)]
    pub volume_db: f32,
    #[serde(default)]
    pub pan: f32,
    #[serde(default)]
    pub mute: bool,
    /// A summing stack: the channel its member tracks play into, shown with them.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stack: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Automation {
    /// In dB.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volume: Vec<AutoPoint>,
    /// -1 to 1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pan: Vec<AutoPoint>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct AutoPoint {
    pub tick: u64,
    pub value: f32,
}

/// An automatable parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Param {
    #[default]
    Volume,
    Pan,
}

impl Automation {
    pub fn is_empty(&self) -> bool {
        self.volume.is_empty() && self.pan.is_empty()
    }

    pub fn points(&self, param: Param) -> &[AutoPoint] {
        match param {
            Param::Volume => &self.volume,
            Param::Pan => &self.pan,
        }
    }

    pub fn points_mut(&mut self, param: Param) -> &mut Vec<AutoPoint> {
        match param {
            Param::Volume => &mut self.volume,
            Param::Pan => &mut self.pan,
        }
    }

    /// Adds a point in time order and returns its index.
    pub fn add(&mut self, param: Param, point: AutoPoint) -> usize {
        let points = self.points_mut(param);
        let i = points.partition_point(|p| p.tick <= point.tick);
        points.insert(i, point);
        i
    }

    /// Moves a point, but not past its neighbours.
    pub fn set(&mut self, param: Param, i: usize, tick: u64, value: f32) {
        let points = self.points_mut(param);
        let lo = if i > 0 { points[i - 1].tick } else { 0 };
        let hi = points.get(i + 1).map_or(u64::MAX, |p| p.tick);
        points[i] = AutoPoint {
            tick: tick.clamp(lo, hi),
            value,
        };
    }
}

/// A parameter's value at a tick: straight lines between points, level before the first and
/// after the last. None without points.
pub fn value_at(points: &[AutoPoint], tick: f64) -> Option<f32> {
    let i = points.partition_point(|p| (p.tick as f64) <= tick);
    match (i.checked_sub(1).map(|i| points[i]), points.get(i)) {
        (None, None) => None,
        (Some(a), None) => Some(a.value),
        (None, Some(b)) => Some(b.value),
        (Some(a), Some(b)) => {
            let t = ((tick - a.tick as f64) / (b.tick - a.tick) as f64) as f32;
            Some(a.value + (b.value - a.value) * t)
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Instrument {
    /// The built-in sine synth.
    Synth,
    /// The built-in drum kit, on the General MIDI drum notes.
    Drums,
    Vst3(PluginRef),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PluginRef {
    /// The `.vst3` bundle.
    pub path: PathBuf,
    /// The class name, for bundles with more than one plugin.
    pub class: String,
    /// Its settings, saved under `Plugin States/` in the project folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<PathBuf>,
}

impl PluginRef {
    /// Which plugin this is, settings aside.
    pub fn same_plugin(&self, other: &PluginRef) -> bool {
        self.path == other.path && self.class == other.class
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Region {
    /// Position and length on the timeline, in ticks.
    #[serde(default)]
    pub start: u64,
    pub length: u64,
    #[serde(flatten)]
    pub content: Content,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Content {
    Midi {
        notes: Vec<Note>,
    },
    /// `file` is relative to the project folder; `offset` is where in it the region starts, in
    /// samples.
    Audio {
        file: PathBuf,
        #[serde(default)]
        offset: u64,
        /// Flex: how much longer the audio plays than it was recorded, its pitch kept. 2 is half
        /// speed. `offset` is in the file's own samples.
        #[serde(default = "one", skip_serializing_if = "is_one")]
        stretch: f64,
    },
    /// A step pattern, as Logic's step sequencer makes: rows of steps, looped over the region.
    Pattern {
        /// Ticks per step: 240 is a sixteenth.
        #[serde(default = "sixteenth")]
        step: u64,
        rows: Vec<PatternRow>,
    },
    /// A take folder: every take recorded over the region, and the comp of them that plays.
    Takes {
        takes: Vec<Take>,
        /// Which take plays from where, each piece until the next. Empty plays the newest take.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        comp: Vec<CompPiece>,
    },
}

fn one() -> f64 {
    1.0
}

fn is_one(v: &f64) -> bool {
    *v == 1.0
}

fn sixteenth() -> u64 {
    PPQ / 4
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PatternRow {
    pub pitch: u8,
    /// A velocity per step; 0 is a rest.
    pub cells: Vec<u8>,
}

/// The notes a pattern plays over `length` ticks, its steps repeating.
pub fn pattern_notes(step: u64, rows: &[PatternRow], length: u64) -> Vec<Note> {
    let steps = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0) as u64;
    let mut notes = Vec::new();
    if steps == 0 || step == 0 {
        return notes;
    }
    for k in 0..length.div_ceil(step) {
        for row in rows {
            let velocity = row.cells.get((k % steps) as usize).copied().unwrap_or(0);
            if velocity > 0 {
                let (start, length) = ((k * step) as i64, (step * 3 / 4).max(1));
                notes.push(Note {
                    start,
                    length,
                    pitch: row.pitch,
                    velocity: velocity.min(127),
                });
            }
        }
    }
    notes
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Take {
    /// Where the take begins, in ticks from the region's start; before it, it has nothing.
    pub start: i64,
    /// MIDI or audio, never another folder.
    #[serde(flatten)]
    pub content: Content,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CompPiece {
    /// Ticks from the region's start.
    pub from: u64,
    pub take: usize,
}

impl Content {
    /// The notes MIDI content plays over `length` ticks: its notes, or a pattern's steps.
    pub fn notes(&self, length: u64) -> Option<Cow<'_, [Note]>> {
        match self {
            Content::Midi { notes } => Some(Cow::Borrowed(notes)),
            Content::Pattern { step, rows } => Some(Cow::Owned(pattern_notes(*step, rows, length))),
            _ => None,
        }
    }

    pub fn is_midi(&self) -> bool {
        match self {
            Content::Midi { .. } | Content::Pattern { .. } => true,
            Content::Audio { .. } => false,
            Content::Takes { takes, .. } => takes.first().is_some_and(|t| t.content.is_midi()),
        }
    }
}

/// A comp as `(from, to, take)` pieces covering `0..length`, in order. With no comp, the newest
/// take plays throughout.
pub fn comp_segments(comp: &[CompPiece], takes: usize, length: u64) -> Vec<(u64, u64, usize)> {
    if takes == 0 || length == 0 {
        return Vec::new();
    }
    let mut pieces: Vec<CompPiece> = comp
        .iter()
        .copied()
        .filter(|p| p.take < takes && p.from < length)
        .collect();
    pieces.sort_by_key(|p| p.from);
    match pieces.first_mut() {
        Some(first) => first.from = 0,
        None => pieces.push(CompPiece {
            from: 0,
            take: takes - 1,
        }),
    }
    let mut out: Vec<(u64, u64, usize)> = Vec::new();
    for (i, p) in pieces.iter().enumerate() {
        let to = pieces.get(i + 1).map_or(length, |n| n.from);
        match out.last_mut() {
            Some(last) if last.2 == p.take => last.1 = to,
            _ if to > p.from => out.push((p.from, to, p.take)),
            _ => {}
        }
    }
    out
}

fn pieces(segments: impl IntoIterator<Item = (u64, u64, usize)>) -> Vec<CompPiece> {
    let mut out: Vec<CompPiece> = Vec::new();
    for (from, _, take) in segments {
        if out.last().is_none_or(|p| p.take != take) {
            out.push(CompPiece { from, take });
        }
    }
    out
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Note {
    /// Ticks from the start of the region. Notes outside the region (after trimming it) are kept
    /// but don't play, so extending it again brings them back.
    pub start: i64,
    /// Notes play their full length, even past the region's end, as in Logic.
    pub length: u64,
    pub pitch: u8,
    pub velocity: u8,
}

/// Where a region is: its track and its index there.
pub type At = (usize, usize);

fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl Project {
    pub fn new(sample_rate: u32) -> Self {
        Project {
            tempo: 120.0,
            sample_rate,
            master_volume_db: 0.0,
            tracks: Vec::new(),
            auxes: Vec::new(),
        }
    }

    pub fn load(dir: &Path) -> Result<Project, String> {
        let path = dir.join(FILE);
        let json =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut p: Project =
            serde_json::from_str(&json).map_err(|e| format!("{}: {e}", path.display()))?;
        if !p.tempo.is_finite() || p.tempo <= 0.0 || p.sample_rate == 0 {
            return Err(format!(
                "{}: tempo and sample_rate must be positive",
                path.display()
            ));
        }
        p.assign_ids();
        Ok(p)
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(FILE);
        let json = serde_json::to_string_pretty(self).unwrap();
        std::fs::write(&path, json + "\n").map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Gives every track and aux that has none an id.
    pub fn assign_ids(&mut self) {
        for t in self.tracks.iter_mut().filter(|t| t.id == 0) {
            t.id = next_id();
        }
        for a in self.auxes.iter_mut().filter(|a| a.id == 0) {
            a.id = next_id();
        }
    }

    /// Adds an aux on the next free bus, as Logic does when a send picks a new bus; returns the
    /// bus.
    /// Makes a summing stack of these tracks: a stack channel they all play into. Returns its bus.
    pub fn add_stack(&mut self, tracks: &[usize]) -> u32 {
        let bus = self.add_aux();
        let n = self.auxes.iter().filter(|a| a.stack).count() + 1;
        let stack = self.auxes.last_mut().unwrap();
        stack.stack = true;
        stack.name = format!("Stack {n}");
        for &t in tracks {
            self.tracks[t].output = Some(bus);
        }
        bus
    }

    pub fn add_aux(&mut self) -> u32 {
        let bus = self.auxes.iter().map(|a| a.bus).max().unwrap_or(0) + 1;
        self.auxes.push(Aux {
            id: next_id(),
            bus,
            name: format!("Aux {bus}"),
            ..Default::default()
        });
        bus
    }

    /// Ticks to samples at `rate`.
    // ponytail: one tempo for the whole project; a tempo map replaces this.
    pub fn samples(&self, ticks: u64, rate: f64) -> u64 {
        (ticks as f64 / PPQ as f64 * 60.0 / self.tempo * rate).round() as u64
    }

    /// Samples at the project's rate to ticks, rounded down.
    pub fn ticks(&self, samples: u64) -> u64 {
        (samples as f64 / self.sample_rate as f64 * self.tempo / 60.0 * PPQ as f64) as u64
    }

    /// The tick where the last region ends.
    pub fn end(&self) -> u64 {
        self.tracks
            .iter()
            .flat_map(|t| &t.regions)
            .map(|r| r.start + r.length)
            .max()
            .unwrap_or(0)
    }

    pub fn region(&self, at: At) -> &Region {
        &self.tracks[at.0].regions[at.1]
    }

    /// A MIDI region's notes, if it is one.
    pub fn notes_mut(&mut self, at: At) -> Option<&mut Vec<Note>> {
        match &mut self.tracks.get_mut(at.0)?.regions.get_mut(at.1)?.content {
            Content::Midi { notes } => Some(notes),
            Content::Audio { .. } | Content::Takes { .. } | Content::Pattern { .. } => None,
        }
    }

    /// Puts what was just recorded on a track, from `start` to `end`: as a take, if it began
    /// inside a region of its kind there (which becomes a take folder, the new take playing), or
    /// else as a region of its own. Returns where it went.
    pub fn add_take(&mut self, track: usize, start: u64, end: u64, content: Content) -> At {
        let regions = &mut self.tracks[track].regions;
        let over = regions.iter().position(|r| {
            r.content.is_midi() == content.is_midi()
                && (r.start..r.start + r.length).contains(&start)
        });
        let Some(i) = over else {
            regions.push(Region {
                start,
                length: (end - start).max(1),
                content,
            });
            return (track, regions.len() - 1);
        };
        let r = &mut regions[i];
        if !matches!(r.content, Content::Takes { .. }) {
            let old = std::mem::replace(
                &mut r.content,
                Content::Takes {
                    takes: Vec::new(),
                    comp: Vec::new(),
                },
            );
            if let Content::Takes { takes, .. } = &mut r.content {
                takes.push(Take {
                    start: 0,
                    content: old,
                });
            }
        }
        let Content::Takes { takes, comp } = &mut r.content else {
            unreachable!()
        };
        takes.push(Take {
            start: (start - r.start) as i64,
            content,
        });
        comp.clear();
        r.length = r.length.max(end - r.start);
        (track, i)
    }

    /// Flex: stretches a plain audio region to `length` ticks, its audio slower or faster and its
    /// pitch kept, as option-dragging its end does in Logic. From a quarter to four times as long
    /// as it was recorded.
    pub fn flex(&mut self, at: At, length: u64) -> Result<(), String> {
        let r = &mut self.tracks[at.0].regions[at.1];
        let Content::Audio { stretch, .. } = &mut r.content else {
            return Err("only audio regions flex".into());
        };
        // The stretch that fills `length`, within limits, and the length that gives.
        let wanted = *stretch * length.max(1) as f64 / r.length as f64;
        let ratio = wanted.clamp(0.25, 4.0);
        r.length = (length.max(1) as f64 * ratio / wanted).round() as u64;
        *stretch = ratio;
        Ok(())
    }

    /// Comps a take folder: `take` plays from `from` to `to` (ticks from the region's start).
    pub fn comp(&mut self, at: At, from: u64, to: u64, take: usize) -> Result<(), String> {
        let r = &mut self.tracks[at.0].regions[at.1];
        let length = r.length;
        let Content::Takes { takes, comp } = &mut r.content else {
            return Err("that region isn't a take folder".into());
        };
        if take >= takes.len() {
            return Err(format!(
                "the folder has no take {take}: it has {}",
                takes.len()
            ));
        }
        let (from, to) = (from.min(length), to.min(length));
        if from >= to {
            return Err("comp a range: `from` before `to`, inside the region".into());
        }
        let now = comp_segments(comp, takes.len(), length);
        let left = now
            .iter()
            .filter(|s| s.0 < from)
            .map(|&(a, b, t)| (a, b.min(from), t));
        let right = now
            .iter()
            .filter(|s| s.1 > to)
            .map(|&(a, b, t)| (a.max(to), b, t));
        let mut all: Vec<_> = left.chain([(from, to, take)]).chain(right).collect();
        all.sort_by_key(|s| s.0);
        *comp = pieces(all);
        Ok(())
    }

    /// Replaces a take folder with what its comp plays: one MIDI region, or an audio region per
    /// piece. Returns where they went.
    pub fn flatten(&mut self, at: At) -> Vec<At> {
        let r = self.region(at).clone();
        let Content::Takes { takes, comp } = &r.content else {
            return vec![at];
        };
        let rate = self.sample_rate as f64;
        let segments = comp_segments(comp, takes.len(), r.length);
        let mut out = Vec::new();
        if r.content.is_midi() {
            let mut notes = Vec::new();
            for &(a, b, t) in &segments {
                let take = &takes[t];
                if let Some(n) = take.content.notes((b as i64 - take.start).max(0) as u64) {
                    let inside = |n: &&Note| (a as i64..b as i64).contains(&(take.start + n.start));
                    notes.extend(n.iter().filter(inside).map(|n| Note {
                        start: take.start + n.start,
                        ..*n
                    }));
                }
            }
            out.push(Region {
                start: r.start,
                length: r.length,
                content: Content::Midi { notes },
            });
        } else {
            for &(a, b, t) in &segments {
                let take = &takes[t];
                let from = (a as i64).max(take.start);
                if let (
                    Content::Audio {
                        file,
                        offset,
                        stretch,
                    },
                    true,
                ) = (&take.content, from < b as i64)
                {
                    let skip = self.samples((from - take.start) as u64, rate);
                    let content = Content::Audio {
                        file: file.clone(),
                        offset: offset + (skip as f64 / stretch) as u64,
                        stretch: *stretch,
                    };
                    out.push(Region {
                        start: r.start + from as u64,
                        length: b - from as u64,
                        content,
                    });
                }
            }
        }
        let regions = &mut self.tracks[at.0].regions;
        regions.remove(at.1);
        let first = regions.len();
        regions.extend(out);
        (first..regions.len()).map(|i| (at.0, i)).collect()
    }

    /// Adds a track at `index` and returns its id.
    pub fn add_track(&mut self, index: usize, mut track: Track) -> u64 {
        track.id = next_id();
        let id = track.id;
        self.tracks.insert(index.min(self.tracks.len()), track);
        id
    }

    /// Moves a region to `start` on `track`, or keeps it on its own track if `track` can't play
    /// it (MIDI goes on instrument tracks, audio on audio tracks). Returns where it is now.
    pub fn move_region(&mut self, at: At, track: usize, start: u64) -> At {
        let midi = self.region(at).content.is_midi();
        let track = if self.tracks[track].instrument.is_some() == midi {
            track
        } else {
            at.0
        };
        let mut r = self.tracks[at.0].regions.remove(at.1);
        r.start = start;
        self.tracks[track].regions.push(r);
        (track, self.tracks[track].regions.len() - 1)
    }

    /// Moves a region's start to `start`, leaving its content where it is on the timeline. An
    /// audio region can't start before its file does.
    pub fn trim_start(&mut self, at: At, start: u64) {
        let r = self.region(at);
        let end = r.start + r.length;
        let mut start = start.min(end - 1);
        if let Content::Audio {
            offset, stretch, ..
        } = r.content
        {
            start = start.max(
                r.start
                    .saturating_sub(self.ticks((offset as f64 * stretch) as u64)),
            );
        }
        let (old, rate) = (r.start, self.sample_rate as f64);
        let moved = if start >= old {
            self.samples(start - old, rate) as i64
        } else {
            -(self.samples(old - start, rate) as i64)
        };
        let r = &mut self.tracks[at.0].regions[at.1];
        match &mut r.content {
            Content::Midi { notes } => {
                for n in notes {
                    n.start -= start as i64 - old as i64;
                }
            }
            Content::Audio {
                offset, stretch, ..
            } => *offset = (*offset as f64 + moved as f64 / *stretch).max(0.0) as u64,
            // A pattern starts over from the region's start.
            Content::Pattern { .. } => {}
            // Takes keep their place on the timeline; the comp keeps its, from the new start.
            Content::Takes { takes, comp } => {
                let delta = start as i64 - old as i64;
                let segments = comp_segments(comp, takes.len(), end - old);
                for t in takes {
                    t.start -= delta;
                }
                let shifted =
                    segments
                        .into_iter()
                        .filter(|s| s.1 as i64 > delta)
                        .map(|(a, b, t)| {
                            (
                                (a as i64 - delta).max(0) as u64,
                                (b as i64 - delta) as u64,
                                t,
                            )
                        });
                *comp = pieces(shifted);
            }
        }
        r.start = start;
        r.length = end - start;
    }

    /// Moves a region's end to `end`.
    pub fn trim_end(&mut self, at: At, end: u64) {
        let r = &mut self.tracks[at.0].regions[at.1];
        r.length = end.max(r.start + 1) - r.start;
    }

    /// Splits a region at `tick`, if that's inside it, and returns where the second half went.
    /// Notes stay with the half they start in, as Logic's "Keep" does.
    pub fn split_region(&mut self, at: At, tick: u64) -> Option<At> {
        let r = self.region(at);
        if tick <= r.start || tick >= r.start + r.length {
            return None;
        }
        let first = tick - r.start;
        let skip = self.samples(first, self.sample_rate as f64);
        let r = &mut self.tracks[at.0].regions[at.1];
        let mut second = r.clone();
        second.start = tick;
        second.length = r.length - first;
        r.length = first;
        match (&mut r.content, &mut second.content) {
            (Content::Midi { notes: a }, Content::Midi { notes: b }) => {
                a.retain(|n| n.start < first as i64);
                b.retain(|n| n.start >= first as i64);
                for n in b {
                    n.start -= first as i64;
                }
            }
            (
                _,
                Content::Audio {
                    offset, stretch, ..
                },
            ) => *offset += (skip as f64 / *stretch) as u64,
            (Content::Pattern { .. }, _) => {}
            (
                Content::Takes { takes, comp: left },
                Content::Takes {
                    takes: moved,
                    comp: right,
                },
            ) => {
                let segments = comp_segments(left, takes.len(), first + second.length);
                *left = pieces(segments.iter().copied().filter(|s| s.0 < first));
                let after = segments.into_iter().filter(|s| s.1 > first);
                *right = pieces(after.map(|(a, b, t)| (a.max(first) - first, b - first, t)));
                for t in moved {
                    t.start -= first as i64;
                }
            }
            _ => unreachable!(),
        }
        self.tracks[at.0].regions.push(second);
        Some((at.0, self.tracks[at.0].regions.len() - 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn midi(start: u64, length: u64, notes: &[(i64, u8)]) -> Region {
        let notes = notes
            .iter()
            .map(|&(start, pitch)| Note {
                start,
                length: 240,
                pitch,
                velocity: 100,
            })
            .collect();
        Region {
            start,
            length,
            content: Content::Midi { notes },
        }
    }

    fn audio(start: u64, length: u64, offset: u64) -> Region {
        Region {
            start,
            length,
            content: Content::Audio {
                file: "a.wav".into(),
                offset,
                stretch: 1.0,
            },
        }
    }

    fn project(regions: Vec<Region>, instrument: bool) -> Project {
        let track = Track {
            name: "t".into(),
            instrument: instrument.then_some(Instrument::Synth),
            regions,
            ..Default::default()
        };
        Project {
            tracks: vec![track],
            ..Project::new(48_000)
        }
    }

    #[test]
    fn json_round_trips_and_reads_sparse_tracks() {
        let json = r#"{
            "tempo": 120, "sample_rate": 48000,
            "tracks": [
                { "name": "Keys", "instrument": "synth",
                  "regions": [{ "start": 0, "length": 3840,
                                "midi": { "notes": [{ "start": 0, "length": 480, "pitch": 60, "velocity": 100 }] } }] },
                { "name": "Drums", "volume_db": -3,
                  "regions": [{ "start": 960, "length": 1920, "audio": { "file": "Audio Files/drums.wav" } }] }
            ]
        }"#;
        let p: Project = serde_json::from_str(json).unwrap();
        assert_eq!(p.tracks[0].instrument, Some(Instrument::Synth));
        assert_eq!(p.tracks[1].instrument, None);
        assert!(matches!(
            p.tracks[1].regions[0].content,
            Content::Audio { offset: 0, .. }
        ));
        assert_eq!(p.end(), 3840);
        let again: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(p, again);
    }

    #[test]
    fn ticks_and_samples() {
        let p = Project::new(48_000);
        assert_eq!(p.samples(PPQ, 48_000.0), 24_000); // a beat at 120 bpm is half a second
        assert_eq!(p.ticks(24_000), PPQ);
    }

    #[test]
    fn splitting_midi_keeps_notes_with_the_half_they_start_in() {
        let mut p = project(
            vec![midi(960, 1920, &[(0, 60), (900, 62), (960, 64)])],
            true,
        );
        assert_eq!(p.split_region((0, 0), 960), None); // at the region's start: nothing to split
        let at = p.split_region((0, 0), 1920).unwrap();
        let (a, b) = (p.region((0, 0)), p.region(at));
        assert_eq!(
            (a.start, a.length, b.start, b.length),
            (960, 960, 1920, 960)
        );
        let pitches = |r: &Region| match &r.content {
            Content::Midi { notes } => notes.iter().map(|n| (n.start, n.pitch)).collect::<Vec<_>>(),
            _ => unreachable!(),
        };
        assert_eq!(pitches(a), [(0, 60), (900, 62)]);
        assert_eq!(pitches(b), [(0, 64)]);
    }

    #[test]
    fn splitting_audio_moves_the_offset() {
        let mut p = project(vec![audio(0, 1920, 100)], false);
        let at = p.split_region((0, 0), 960).unwrap();
        assert!(matches!(
            p.region(at).content,
            Content::Audio { offset: 24_100, .. }
        ));
    }

    #[test]
    fn trimming_the_start_keeps_content_in_place() {
        let mut p = project(vec![midi(960, 1920, &[(0, 60), (480, 62)])], true);
        p.trim_start((0, 0), 1440);
        let r = p.region((0, 0));
        assert_eq!((r.start, r.length), (1440, 1440));
        let Content::Midi { notes } = &r.content else {
            unreachable!()
        };
        assert_eq!((notes[0].start, notes[1].start), (-480, 0));
        p.trim_start((0, 0), 960); // back again: the hidden note returns
        let Content::Midi { notes } = &p.region((0, 0)).content else {
            unreachable!()
        };
        assert_eq!(notes[0].start, 0);

        // Audio can't start before its file.
        let mut p = project(vec![audio(1920, 960, 12_000)], false);
        p.trim_start((0, 0), 0);
        let r = p.region((0, 0));
        assert_eq!((r.start, r.length), (1440, 1440));
        assert!(matches!(r.content, Content::Audio { offset: 0, .. }));
    }

    #[test]
    fn automation_points_stay_in_order_and_interpolate() {
        let mut a = Automation::default();
        assert_eq!(value_at(a.points(Param::Volume), 0.0), None);
        a.add(
            Param::Volume,
            AutoPoint {
                tick: 960,
                value: -12.0,
            },
        );
        a.add(
            Param::Volume,
            AutoPoint {
                tick: 0,
                value: 0.0,
            },
        );
        assert_eq!(
            a.add(
                Param::Volume,
                AutoPoint {
                    tick: 480,
                    value: -6.0
                }
            ),
            1
        );
        let v = a.points(Param::Volume);
        assert_eq!(value_at(v, 240.0), Some(-3.0));
        assert_eq!(value_at(v, 5000.0), Some(-12.0));
        a.set(Param::Volume, 1, 2000, 0.0); // can't pass the point after it
        assert_eq!(a.points(Param::Volume)[1].tick, 960);
        assert!(a.points(Param::Pan).is_empty());
    }

    fn take(start: i64, pitch: u8) -> Take {
        let notes = vec![Note {
            start: 0,
            length: 240,
            pitch,
            velocity: 100,
        }];
        Take {
            start,
            content: Content::Midi { notes },
        }
    }

    fn folder(p: &Project, at: At) -> (Vec<i64>, Vec<(u64, u64, usize)>) {
        let r = p.region(at);
        let Content::Takes { takes, comp } = &r.content else {
            panic!("not a folder")
        };
        (
            takes.iter().map(|t| t.start).collect(),
            comp_segments(comp, takes.len(), r.length),
        )
    }

    #[test]
    fn recording_over_a_region_makes_a_take_folder() {
        let mut p = project(vec![midi(960, 1920, &[(0, 60)])], true);
        let notes = vec![Note {
            start: 0,
            length: 240,
            pitch: 64,
            velocity: 90,
        }];
        let at = p.add_take(0, 1440, 3360, Content::Midi { notes });
        assert_eq!(at, (0, 0));
        let r = p.region(at);
        assert_eq!((r.start, r.length), (960, 2400)); // grown to the end of the new take
        assert_eq!(folder(&p, at), (vec![0, 480], vec![(0, 2400, 1)])); // the newest plays
        // Recording somewhere else makes a plain region.
        assert_eq!(
            p.add_take(0, 7680, 8640, Content::Midi { notes: vec![] }),
            (0, 1)
        );
    }

    #[test]
    fn comping_overlays_ranges_and_merges_neighbours() {
        let takes = vec![take(0, 60), take(0, 62), take(0, 64)];
        let region = Region {
            start: 0,
            length: 3840,
            content: Content::Takes {
                takes,
                comp: vec![],
            },
        };
        let mut p = project(vec![region], true);
        p.comp((0, 0), 960, 1920, 0).unwrap();
        assert_eq!(
            folder(&p, (0, 0)).1,
            [(0, 960, 2), (960, 1920, 0), (1920, 3840, 2)]
        );
        p.comp((0, 0), 1920, 2880, 0).unwrap();
        assert_eq!(
            folder(&p, (0, 0)).1,
            [(0, 960, 2), (960, 2880, 0), (2880, 3840, 2)]
        );
        p.comp((0, 0), 0, 3840, 1).unwrap();
        assert_eq!(folder(&p, (0, 0)).1, [(0, 3840, 1)]);
        assert!(p.comp((0, 0), 0, 960, 7).is_err());
    }

    #[test]
    fn take_folders_split_trim_and_flatten() {
        // Take 1 starts at 480 with a note 600 in (at 1080), where the comp plays take 1.
        let mut late = take(480, 62);
        let Content::Midi { notes } = &mut late.content else {
            unreachable!()
        };
        notes[0].start = 600;
        let takes = vec![take(0, 60), late];
        let comp = vec![
            CompPiece { from: 0, take: 0 },
            CompPiece { from: 960, take: 1 },
        ];
        let region = Region {
            start: 0,
            length: 1920,
            content: Content::Takes { takes, comp },
        };
        let mut p = project(vec![region], true);
        let at = p.split_region((0, 0), 1440).unwrap();
        assert_eq!(folder(&p, (0, 0)).1, [(0, 960, 0), (960, 1440, 1)]);
        assert_eq!(folder(&p, at), (vec![-1440, -960], vec![(0, 480, 1)]));

        p.trim_start((0, 0), 480); // takes stay put on the timeline
        assert_eq!(
            folder(&p, (0, 0)),
            (vec![-480, 0], vec![(0, 480, 0), (480, 960, 1)])
        );

        // Flattening keeps what the comp plays: take 1's note (1080, now 600 after the trim), not
        // take 0's (now -480, before the region).
        let flat = p.flatten((0, 0));
        let Content::Midi { notes } = &p.region(flat[0]).content else {
            panic!()
        };
        let kept: Vec<(i64, u8)> = notes.iter().map(|n| (n.start, n.pitch)).collect();
        assert_eq!(kept, [(600, 62)]);
    }

    #[test]
    fn regions_only_move_to_tracks_that_can_play_them() {
        let mut p = project(vec![midi(0, 960, &[])], true);
        p.add_track(
            1,
            Track {
                name: "audio".into(),
                ..Default::default()
            },
        );
        p.add_track(
            2,
            Track {
                name: "keys".into(),
                instrument: Some(Instrument::Synth),
                ..Default::default()
            },
        );
        assert_eq!(p.move_region((0, 0), 1, 480), (0, 0));
        assert_eq!(p.move_region((0, 0), 2, 480), (2, 0));
        assert_eq!(p.region((2, 0)).start, 480);
    }
}
