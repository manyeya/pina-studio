//! A project: what's on the timeline and how it's mixed. It's a folder, like Logic's `.logicx`:
//! `project.json` plus the audio it uses under `Audio Files/`. The JSON is the source of truth, so
//! people and agents can read, diff and edit it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Ticks per quarter note, as in Logic.
pub const PPQ: u64 = 960;

pub const FILE: &str = "project.json";
pub const AUDIO_DIR: &str = "Audio Files";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Project {
    /// Beats per minute.
    pub tempo: f64,
    pub sample_rate: u32,
    #[serde(default)]
    pub master_volume_db: f32,
    pub tracks: Vec<Track>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Track {
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
    /// Ticks from the start of the region.
    pub start: u64,
    pub length: u64,
    pub pitch: u8,
    pub velocity: u8,
}

impl Project {
    pub fn new(sample_rate: u32) -> Self {
        Project {
            tempo: 120.0,
            sample_rate,
            master_volume_db: 0.0,
            tracks: Vec::new(),
        }
    }

    pub fn load(dir: &Path) -> Result<Project, String> {
        let path = dir.join(FILE);
        let json =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let p: Project =
            serde_json::from_str(&json).map_err(|e| format!("{}: {e}", path.display()))?;
        if !p.tempo.is_finite() || p.tempo <= 0.0 || p.sample_rate == 0 {
            return Err(format!(
                "{}: tempo and sample_rate must be positive",
                path.display()
            ));
        }
        Ok(p)
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(FILE);
        let json = serde_json::to_string_pretty(self).unwrap();
        std::fs::write(&path, json + "\n").map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Ticks to samples at `rate`.
    // ponytail: one tempo for the whole project; a tempo map replaces this.
    pub fn samples(&self, ticks: u64, rate: f64) -> u64 {
        (ticks as f64 / PPQ as f64 * 60.0 / self.tempo * rate).round() as u64
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn ticks_to_samples() {
        let p = Project::new(48_000);
        assert_eq!(p.samples(PPQ, 48_000.0), 24_000); // a beat at 120 bpm is half a second
    }
}
