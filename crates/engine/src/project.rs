//! A project: what's on the timeline and how it's mixed. It's a folder, like Logic's `.logicx`:
//! `project.json` plus the audio it uses under `Audio Files/`. The JSON is the source of truth, so
//! people and agents can read, diff and edit it.

use std::{
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
    },
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
            Content::Audio { .. } => None,
        }
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
        let midi = matches!(self.region(at).content, Content::Midi { .. });
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
        if let Content::Audio { offset, .. } = r.content {
            start = start.max(r.start.saturating_sub(self.ticks(offset)));
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
            Content::Audio { offset, .. } => *offset = (*offset as i64 + moved).max(0) as u64,
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
            (_, Content::Audio { offset, .. }) => *offset += skip,
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
