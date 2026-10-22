//! A project: what's on the timeline and how it's mixed. It's a folder:
//! `project.json` plus the audio it uses under `Audio Files/`. The JSON is the source of truth, so
//! people and agents can read, diff and edit it.

mod automation;
mod editing;
mod eq;
mod params;
mod patterns;
mod plugins;
mod regions;
mod sampler;
mod stacks;
mod strip;
mod track;

pub use automation::*;
pub use eq::*;
pub use params::*;
pub use patterns::*;
pub use plugins::*;
pub use regions::*;
pub use sampler::*;
pub use strip::*;
pub use track::*;

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::dsp::modulate::{MAX_MODS, ModKind, Modulator};

pub use crate::audio::stretch::Flex;

/// Ticks per quarter note.
pub const PPQ: u64 = 960;
/// The notes a time signature's beat can be: a whole note to a sixteenth.
pub const BEAT_UNITS: [u64; 5] = [1, 2, 4, 8, 16];

pub const FILE: &str = "project.json";
pub const AUDIO_DIR: &str = "Audio Files";
pub const STATES_DIR: &str = "Plugin States";
/// Where unsaved edits are kept, as a project of their own, until they're saved or discarded.
pub const RECOVERY_DIR: &str = ".recovery";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Project {
    /// Quarter notes per minute.
    pub tempo: f64,
    /// Beats to a bar, and the note that is a beat: [4, 4], [3, 4], [6, 8].
    #[serde(default = "common_time", skip_serializing_if = "is_common_time")]
    pub time_signature: [u64; 2],
    pub sample_rate: u32,
    #[serde(default)]
    pub master_volume_db: f32,
    /// The master's built-in EQ and compressor, after everything is summed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub master_eq: Option<Eq>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub master_comp: Option<Comp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub master_sat: Option<Saturator>,
    /// Plugins on the master, after its built-in effects.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub master_inserts: Vec<PluginRef>,
    /// How panning keeps level (the pan law).
    #[serde(default, skip_serializing_if = "PanLaw::is_flat")]
    pub pan_law: PanLaw,
    pub tracks: Vec<Track>,
    /// Aux channels: tracks send to them by bus number, and they play into the master.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auxes: Vec<Aux>,
    /// Named places in the song, in time order: Verse, Chorus.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<Marker>,
    /// The song's key, as "A minor" or "Eb major"; without one it's worked out from the notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The stretch playback loops over, while it's on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle: Option<Cycle>,
    /// The song's chords, in time order: what its parts are written over.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chords: Vec<ChordSpan>,
    /// What the song is, as a playbook's id ("lofi", "trap"): what its parts and mix aim for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    /// A finished song it's measured against: its mix targets, over the genre's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<Reference>,
    /// Sounds the person picked for an agent to build the song with.
    #[serde(default, skip_serializing_if = "Palette::is_empty")]
    pub palette: Palette,
}

/// What the person picked for an agent to make the song with: sounds (audio files) and devices
/// (instruments and effects), each with the part it's for if they said; `only` asks for these
/// and no others of each kind picked.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Palette {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sounds: Vec<PaletteSound>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<PaletteDevice>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub only: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PaletteSound {
    pub path: PathBuf,
    /// What it's for, as the person said ("kick", "bass", "pad"); without, the agent decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// An instrument or effect picked for the agent: Pina Studio's own (`own`: "synth", "drums",
/// "sampler", "eq", "comp" or "sat") or a plugin, as `set_instrument` and `set_insert` take it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PaletteDevice {
    pub name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub instrument: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<PluginRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

impl PaletteDevice {
    /// The same device: Pina Studio's own by name, a plugin by its class in its bundle.
    pub fn same(&self, other: &PaletteDevice) -> bool {
        match (&self.plugin, &other.plugin) {
            (Some(a), Some(b)) => a.class == b.class && a.path == b.path,
            (None, None) => self.own == other.own,
            _ => false,
        }
    }
}

impl Palette {
    pub fn is_empty(&self) -> bool {
        self.sounds.is_empty() && self.devices.is_empty() && !self.only
    }

    pub fn has(&self, path: &Path) -> bool {
        self.sounds.iter().any(|s| s.path == path)
    }

    pub fn has_device(&self, d: &PaletteDevice) -> bool {
        self.devices.iter().any(|x| x.same(d))
    }

    /// Nothing picked (`only` aside).
    pub fn none(&self) -> bool {
        self.sounds.is_empty() && self.devices.is_empty()
    }
}

/// What a reference song measures, for the mix to aim at.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Reference {
    /// Its file, as it was measured.
    pub file: PathBuf,
    pub lufs: f32,
    pub crest_db: f32,
    /// Sub, bass, low mid, mid, presence and air, dB of the total (as `analysis::Spectrum`).
    pub spectrum: [f32; 6],
    pub width_db: f32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub key: String,
}

/// A chord on the song's chord track, by its symbol ("Am9", "C/E"), from `start` for `length`
/// ticks.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ChordSpan {
    pub start: u64,
    pub length: u64,
    pub chord: String,
}

fn common_time() -> [u64; 2] {
    [4, 4]
}

fn is_common_time(t: &[u64; 2]) -> bool {
    *t == [4, 4]
}

/// A named place on the timeline.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Marker {
    pub tick: u64,
    pub name: String,
}

/// Where playback loops, in ticks: from `start` back to it at `end`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Cycle {
    pub start: u64,
    pub end: u64,
    /// Kept but not looping.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub off: bool,
}

/// The most bands an EQ has.
pub const MAX_BANDS: usize = 24;

/// The resolutions linear phase and spectral bands can work at, low to very high.
pub const RESOLUTIONS: [&str; 4] = ["Low", "Medium", "High", "Very High"];

/// The quietest a fader goes: silence.
pub const SILENT_DB: f32 = -144.0;

/// A step's lanes, as usual when it has none.
pub const CHANCE: u8 = 100;
pub const GATE: u8 = 75;

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
            master_eq: None,
            master_comp: None,
            master_sat: None,
            master_inserts: Vec::new(),
            pan_law: PanLaw::Flat,
            time_signature: common_time(),
            tracks: Vec::new(),
            auxes: Vec::new(),
            markers: Vec::new(),
            key: None,
            cycle: None,
            chords: Vec::new(),
            genre: None,
            reference: None,
            palette: Palette::default(),
        }
    }

    /// Whether a sound is in the song yet: as an audio region's file or a sampler zone's, by
    /// its file name (a sound put in is copied into the song's folder).
    pub fn uses(&self, path: &Path) -> bool {
        let Some(name) = path.file_name() else {
            return false;
        };
        self.tracks.iter().any(|t| {
            let audio = t.regions.iter().any(|r| {
                matches!(&r.content, Content::Audio { file, .. } if file.file_name() == Some(name))
            });
            let zone = matches!(&t.instrument, Some(Instrument::Sampler(s))
                if s.zones.iter().any(|z| z.file.file_name() == Some(name)));
            audio || zone
        })
    }

    /// Whether a device picked for the agent is on a track (or an aux, or the master) yet.
    pub fn uses_device(&self, d: &PaletteDevice) -> bool {
        if let Some(r) = &d.plugin {
            let same = |x: &PluginRef| x.class == r.class && x.path == r.path;
            let plays = |t: &Track| matches!(&t.instrument, Some(Instrument::Plugin(x)) if same(x));
            return self
                .tracks
                .iter()
                .any(|t| plays(t) || t.inserts.iter().any(same))
                || self.auxes.iter().flat_map(|a| &a.inserts).any(same)
                || self.master_inserts.iter().any(same);
        }
        self.tracks.iter().any(|t| match d.own.as_deref() {
            Some("synth") => matches!(t.instrument, Some(Instrument::Synth)),
            Some("drums") => matches!(t.instrument, Some(Instrument::Drums)),
            Some("sampler") => matches!(t.instrument, Some(Instrument::Sampler(_))),
            Some("eq") => t.eq.is_some(),
            Some("comp") => t.comp.is_some(),
            Some("sat") => t.sat.is_some(),
            _ => false,
        })
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
        p.resolve_sources();
        Ok(p)
    }

    /// Every sidechain (an EQ's, an insert's or instrument's) and what it's set to.
    fn sidechains_mut(&mut self) -> Vec<&mut Option<u64>> {
        fn eqs(eq: &mut Option<Eq>) -> Option<&mut Option<u64>> {
            eq.as_mut().map(|e| &mut e.sidechain)
        }
        let mut out: Vec<&mut Option<u64>> = Vec::new();
        for t in &mut self.tracks {
            out.extend(eqs(&mut t.eq));
            if let Some(Instrument::Plugin(r)) = &mut t.instrument {
                out.push(&mut r.sidechain);
            }
            out.extend(t.inserts.iter_mut().map(|r| &mut r.sidechain));
        }
        for a in &mut self.auxes {
            out.extend(eqs(&mut a.eq));
            out.extend(a.inserts.iter_mut().map(|r| &mut r.sidechain));
        }
        out.extend(eqs(&mut self.master_eq));
        out.extend(self.master_inserts.iter_mut().map(|r| &mut r.sidechain));
        out
    }

    /// The project as it's saved or sent to another process: a sidechain names its source by
    /// the track's number (from 1), since ids are only this session's.
    pub fn portable(&self) -> Project {
        let mut p = self.clone();
        let ids: Vec<u64> = p.tracks.iter().map(|t| t.id).collect();
        for sc in p.sidechains_mut() {
            *sc = sc.and_then(|id| ids.iter().position(|&x| x == id).map(|i| i as u64 + 1));
        }
        // A follower too (0 when what it followed has gone).
        for s in p.followed_mut() {
            *s = ids
                .iter()
                .position(|&x| x == *s)
                .map_or(0, |i| i as u64 + 1);
        }
        p
    }

    /// Every follower's source, on tracks and stacks.
    fn followed_mut(&mut self) -> Vec<&mut u64> {
        let tracks = self.tracks.iter_mut().flat_map(|t| &mut t.modulators);
        let stacks = self.auxes.iter_mut().flat_map(|a| &mut a.modulators);
        tracks
            .chain(stacks)
            .filter_map(|m| match &mut m.kind {
                ModKind::Follower { source, .. } => Some(source),
                _ => None,
            })
            .collect()
    }

    /// After reading a portable project and giving its tracks ids: each sidechain's and
    /// follower's number back to its track's id (one past the last track, or 0, is none).
    pub fn resolve_sources(&mut self) {
        let ids: Vec<u64> = self.tracks.iter().map(|t| t.id).collect();
        for sc in self.sidechains_mut() {
            *sc = sc.and_then(|n| ids.get((n as usize).checked_sub(1)?).copied());
        }
        for s in self.followed_mut() {
            *s = (*s as usize)
                .checked_sub(1)
                .and_then(|i| ids.get(i).copied())
                .unwrap_or(0);
        }
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(FILE);
        let json = serde_json::to_string_pretty(&self.portable()).unwrap();
        // Written beside it and moved over it, so stopping halfway leaves the old one whole.
        let part = dir.join(format!("{FILE}.part"));
        std::fs::write(&part, json + "\n")
            .and_then(|()| std::fs::rename(&part, &path))
            .map_err(|e| format!("{}: {e}", path.display()))
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

    /// Sets the tempo. Audio that follows it stretches, so its beats stay on the song's.
    pub fn set_tempo(&mut self, bpm: f64) {
        let by = self.tempo / bpm;
        fn follow(content: &mut Content, by: f64) {
            match content {
                Content::Audio {
                    stretch,
                    follow: true,
                    warps,
                    ..
                } => {
                    // Rounded, so going back to a tempo comes back to the same stretch.
                    let was = *stretch;
                    *stretch = (*stretch * by * 1e9).round() / 1e9;
                    for w in warps {
                        w[1] = (w[1] as f64 * *stretch / was).round() as u64;
                    }
                }
                Content::Takes { takes, .. } => {
                    for take in takes {
                        follow(&mut take.content, by);
                    }
                }
                _ => {}
            }
        }
        for t in &mut self.tracks {
            for r in &mut t.regions {
                follow(&mut r.content, by);
            }
            for c in &mut t.cells {
                follow(&mut c.region.content, by);
            }
        }
        self.tempo = bpm;
    }

    /// Sets the song's key, or with None has it worked out from the notes. With `move_notes`, the
    /// music moves into the new key (`Key::move_pitch`): every note and pattern row, in regions,
    /// take folders and Live Loops cells, on every pitched instrument. Drums and audio stay put.
    pub fn set_key(&mut self, key: Option<crate::music::Key>, move_notes: bool) {
        if let (true, Some(from), Some(to)) = (move_notes, self.song_key(), key)
            && from != to
        {
            fn shift(content: &mut Content, from: &crate::music::Key, to: &crate::music::Key) {
                match content {
                    Content::Midi { notes, controls } => {
                        for n in notes {
                            n.pitch = from.move_pitch(n.pitch, to);
                        }
                        for p in controls.iter_mut().filter_map(|c| c.kind.pitch_mut()) {
                            *p = from.move_pitch(*p, to);
                        }
                    }
                    Content::Pattern { rows, .. } => {
                        for row in rows {
                            row.pitch = from.move_pitch(row.pitch, to);
                        }
                    }
                    Content::Takes { takes, .. } => {
                        for take in takes {
                            shift(&mut take.content, from, to);
                        }
                    }
                    Content::Audio { .. } => {}
                }
            }
            let pitched = |t: &&mut Track| t.instrument.as_ref().is_some_and(|i| !i.is_kit());
            for t in self.tracks.iter_mut().filter(pitched) {
                for r in &mut t.regions {
                    shift(&mut r.content, &from, &to);
                }
                for c in &mut t.cells {
                    shift(&mut c.region.content, &from, &to);
                }
            }
        }
        self.key = key.map(|k| k.name());
    }

    /// The song's key: the one set, or else the one its notes suggest (drums aside).
    pub fn song_key(&self) -> Option<crate::music::Key> {
        if let Some(k) = self.key.as_deref().and_then(crate::music::Key::parse) {
            return Some(k);
        }
        let mut notes = Vec::new();
        for t in self
            .tracks
            .iter()
            .filter(|t| !t.instrument.as_ref().is_some_and(Instrument::is_kit))
        {
            for r in &t.regions {
                if let Some(list) = r.content.notes(r.length) {
                    notes.extend(list.iter().map(|n| (n.pitch, n.length)));
                }
            }
        }
        crate::music::detect(notes)
    }

    /// Ticks in a beat: a quarter note in 4/4, an eighth in 6/8.
    pub fn beat(&self) -> u64 {
        PPQ * 4 / self.time_signature[1].clamp(1, 16)
    }

    pub fn beats_per_bar(&self) -> u64 {
        self.time_signature[0].clamp(1, 32)
    }

    /// Ticks in a bar.
    pub fn bar(&self) -> u64 {
        self.beat() * self.beats_per_bar()
    }

    /// A tick as bars, beats, sixteenths and ticks, each from 0.
    pub fn bbt(&self, tick: u64) -> [u64; 4] {
        let (bar, beat) = (self.bar(), self.beat());
        [
            tick / bar,
            tick % bar / beat,
            tick % beat / (PPQ / 4),
            tick % (PPQ / 4),
        ]
    }

    /// Where a looped region stops: where its loops were dragged to, or the next region on its
    /// track, or else the end of the song; whichever comes first.
    pub fn loop_end(&self, at: At) -> u64 {
        let r = self.region(at);
        let next = self.tracks[at.0]
            .regions
            .iter()
            .map(|o| o.start)
            .filter(|&s| s > r.start)
            .min();
        let stop = match r.loop_length {
            Some(l) => (r.start + l).min(next.unwrap_or(u64::MAX)),
            None => next.unwrap_or_else(|| self.end()),
        };
        stop.max(r.start + r.length)
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
    /// Where the last region stops, or the last loops dragged out past it.
    pub fn end(&self) -> u64 {
        self.tracks
            .iter()
            .flat_map(|t| &t.regions)
            .map(|r| match (r.looped, r.loop_length) {
                (true, Some(l)) => r.start + l.max(r.length),
                _ => r.start + r.length,
            })
            .max()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests;
