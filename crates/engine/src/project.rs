//! A project: what's on the timeline and how it's mixed. It's a folder, like Logic's `.logicx`:
//! `project.json` plus the audio it uses under `Audio Files/`. The JSON is the source of truth, so
//! people and agents can read, diff and edit it.

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

/// Ticks per quarter note, as in Logic.
pub const PPQ: u64 = 960;
/// The notes a time signature's beat can be: a whole note to a sixteenth.
pub const BEAT_UNITS: [u64; 5] = [1, 2, 4, 8, 16];

pub const FILE: &str = "project.json";
pub const AUDIO_DIR: &str = "Audio Files";
pub const STATES_DIR: &str = "Plugin States";

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

/// One band of an EQ: its frequency (Hz), gain (dB) and width (Q).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub freq: f32,
    #[serde(default)]
    pub gain_db: f32,
    #[serde(default = "q")]
    pub q: f32,
}

fn q() -> f32 {
    1.0
}

/// A channel's built-in EQ, as Logic's Channel EQ: a low cut, two bells and a high shelf.
/// Anything left out of its JSON is as `Eq::default()` has it: flat.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Eq {
    /// The low cut's corner, in Hz; 20 or less is off.
    #[serde(default)]
    pub low_cut: f32,
    pub low_mid: Band,
    pub high_mid: Band,
    pub high_shelf: Band,
    /// Kept but not heard.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bypass: bool,
}

impl Default for Eq {
    fn default() -> Self {
        let band = |freq, q| Band {
            freq,
            gain_db: 0.0,
            q,
        };
        Eq {
            low_cut: 0.0,
            low_mid: band(400.0, 1.0),
            high_mid: band(2_500.0, 1.0),
            high_shelf: band(10_000.0, std::f32::consts::FRAC_1_SQRT_2),
            bypass: false,
        }
    }
}

/// A channel's built-in compressor, after its EQ.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Comp {
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    #[serde(default)]
    pub makeup_db: f32,
    /// Its level is an RMS average over 15 ms rather than the peak: gentler, more like the ear.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rms: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bypass: bool,
}

impl Default for Comp {
    fn default() -> Self {
        Comp {
            threshold_db: -18.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 120.0,
            makeup_db: 0.0,
            rms: false,
            bypass: false,
        }
    }
}

/// A channel's built-in saturator, after its compressor: it drives the sound into a soft clip, as
/// tape or a tube would, adding harmonics. Quiet sounds keep their level; loud ones round off.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Saturator {
    /// How hard it's driven, in dB: 0 to 24.
    pub drive_db: f32,
    /// What it adds, darker (-1) to brighter (1).
    pub tone: f32,
    /// How much of the saturated sound is heard, 0 to 1; the rest is the dry sound.
    pub mix: f32,
    pub output_db: f32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bypass: bool,
}

impl Default for Saturator {
    fn default() -> Self {
        Saturator {
            drive_db: 6.0,
            tone: 0.0,
            mix: 1.0,
            output_db: 0.0,
            bypass: false,
        }
    }
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
    /// The built-in EQ and compressor, before the inserts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eq: Option<Eq>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comp: Option<Comp>,
    /// The built-in saturator, after the compressor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sat: Option<Saturator>,
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
    /// Volume, pan and plugin parameters over time. A parameter with points follows them instead
    /// of its fader or knob, unless the track's automation is off.
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
    /// What an audio track records: the input device's channels from here. The first two by
    /// default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Input>,
}

/// Input channels to record from.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Input {
    /// The first channel, from 0.
    pub channel: u16,
    /// This channel and the next, or this one alone.
    #[serde(default)]
    pub stereo: bool,
}

impl Input {
    pub fn name(self) -> String {
        match self.stereo {
            true => format!("Input {}–{}", self.channel + 1, self.channel + 2),
            false => format!("Input {}", self.channel + 1),
        }
    }
}

impl Track {
    /// Puts a plugin in insert slot `index` (past the end adds one), or takes it out with None.
    /// Automation follows: a slot's lanes go with its plugin.
    pub fn set_insert(&mut self, index: usize, plugin: Option<PluginRef>) -> Result<(), String> {
        match plugin {
            Some(p) if index < self.inserts.len() => {
                if !self.inserts[index].same_plugin(&p) {
                    self.automation.forget(Which::Insert(index));
                }
                self.inserts[index] = p;
            }
            Some(p) => self.inserts.push(p),
            None if index < self.inserts.len() => {
                self.inserts.remove(index);
                self.automation.insert_removed(index);
            }
            None => return Err(format!("no insert {index} to remove")),
        }
        Ok(())
    }

    /// Changes the instrument; another plugin's parameters aren't this one's, so its lanes go.
    pub fn set_instrument(&mut self, instrument: Instrument) {
        let same = match (&self.instrument, &instrument) {
            (Some(Instrument::Plugin(a)), Instrument::Plugin(b)) => a.same_plugin(b),
            _ => false,
        };
        if !same {
            self.automation.forget(Which::Instrument);
        }
        self.instrument = Some(instrument);
    }

    /// A plugin on the track.
    pub fn plugin(&self, which: Which) -> Option<&PluginRef> {
        match which {
            Which::Instrument => match &self.instrument {
                Some(Instrument::Plugin(r)) => Some(r),
                _ => None,
            },
            Which::Insert(i) => self.inserts.get(i),
        }
    }

    pub fn plugin_mut(&mut self, which: Which) -> Option<&mut PluginRef> {
        match which {
            Which::Instrument => match &mut self.instrument {
                Some(Instrument::Plugin(r)) => Some(r),
                _ => None,
            },
            Which::Insert(i) => self.inserts.get_mut(i),
        }
    }
}

/// A Live Loops cell: a loop on a track, in a scene's column. Its region's start doesn't matter;
/// it loops over its length from wherever it's launched.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Cell {
    pub scene: usize,
    #[serde(flatten)]
    pub region: Region,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct AuxSend {
    pub bus: u32,
    #[serde(default)]
    pub level_db: f32,
    /// Taken before the fader, so the fader doesn't change what's sent.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pre: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Aux {
    /// As `Track::id`.
    #[serde(skip)]
    pub id: u64,
    /// The bus it listens to.
    pub bus: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eq: Option<Eq>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comp: Option<Comp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sat: Option<Saturator>,
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
    /// Plugin parameters over time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<PluginAuto>,
    #[serde(default, skip_serializing_if = "AutoMode::is_read")]
    pub mode: AutoMode,
}

/// A plugin parameter's automation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PluginAuto {
    pub plugin: Which,
    /// The parameter's id in the plugin.
    pub param: u32,
    /// Its name, for people and agents reading the project.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Values from 0 to 1.
    pub points: Vec<AutoPoint>,
}

/// A plugin on a track or aux: its instrument, or an insert by index.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Which {
    Instrument,
    Insert(usize),
}

/// How a track uses its automation, as Logic's modes: Read plays it; Touch and Latch play it and
/// also record what's moved while the song plays (Touch until the control is let go, Latch until
/// playback stops); Off ignores it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutoMode {
    Off,
    #[default]
    Read,
    Touch,
    Latch,
}

impl AutoMode {
    pub const ALL: [AutoMode; 4] = [
        AutoMode::Off,
        AutoMode::Read,
        AutoMode::Touch,
        AutoMode::Latch,
    ];

    fn is_read(&self) -> bool {
        *self == AutoMode::Read
    }

    pub fn name(self) -> &'static str {
        match self {
            AutoMode::Off => "Off",
            AutoMode::Read => "Read",
            AutoMode::Touch => "Touch",
            AutoMode::Latch => "Latch",
        }
    }

    /// Whether moving a control while playing records it.
    pub fn writes(self) -> bool {
        matches!(self, AutoMode::Touch | AutoMode::Latch)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct AutoPoint {
    pub tick: u64,
    pub value: f32,
}

/// An automatable parameter: the fader, the pan knob, or a plugin's parameter by id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Param {
    #[default]
    Volume,
    Pan,
    Plugin(Which, u32),
}

impl Automation {
    pub fn is_empty(&self) -> bool {
        self.volume.is_empty()
            && self.pan.is_empty()
            && self.plugins.is_empty()
            && self.mode == AutoMode::Read
    }

    pub fn points(&self, param: Param) -> &[AutoPoint] {
        match param {
            Param::Volume => &self.volume,
            Param::Pan => &self.pan,
            Param::Plugin(which, id) => self
                .plugins
                .iter()
                .find(|a| a.plugin == which && a.param == id)
                .map_or(&[], |a| &a.points),
        }
    }

    /// A parameter's points, making it a lane if it has none.
    pub fn points_mut(&mut self, param: Param) -> &mut Vec<AutoPoint> {
        match param {
            Param::Volume => &mut self.volume,
            Param::Pan => &mut self.pan,
            Param::Plugin(plugin, param) => {
                let i = match self
                    .plugins
                    .iter()
                    .position(|a| a.plugin == plugin && a.param == param)
                {
                    Some(i) => i,
                    None => {
                        self.plugins.push(PluginAuto {
                            plugin,
                            param,
                            name: String::new(),
                            points: Vec::new(),
                        });
                        self.plugins.len() - 1
                    }
                };
                &mut self.plugins[i].points
            }
        }
    }

    /// Names a plugin parameter's lane, so the project says what it is.
    pub fn name(&mut self, param: Param, name: &str) {
        if let Param::Plugin(which, id) = param
            && let Some(a) = self
                .plugins
                .iter_mut()
                .find(|a| a.plugin == which && a.param == id)
        {
            a.name = name.to_string();
        }
    }

    /// Drops plugin lanes left without points.
    pub fn tidy(&mut self) {
        self.plugins.retain(|a| !a.points.is_empty());
    }

    /// Every parameter with points, faders first.
    pub fn params(&self) -> Vec<Param> {
        let mut out = Vec::new();
        if !self.volume.is_empty() {
            out.push(Param::Volume);
        }
        if !self.pan.is_empty() {
            out.push(Param::Pan);
        }
        out.extend(
            self.plugins
                .iter()
                .filter(|a| !a.points.is_empty())
                .map(|a| Param::Plugin(a.plugin, a.param)),
        );
        out
    }

    /// Records a control's value at `tick` while the song plays, replacing the line since
    /// `after`: the last tick this pass recorded, or None on its first, which keeps the line as it
    /// was up to here.
    pub fn write(&mut self, param: Param, after: Option<u64>, tick: u64, value: f32) {
        let points = self.points_mut(param);
        let from = match after {
            Some(a) => a.min(tick),
            None => {
                let before = tick.saturating_sub(1);
                if let Some(old) = value_at(points, tick as f64)
                    && tick > 0
                    && !points.iter().any(|p| p.tick == before)
                {
                    let i = points.partition_point(|p| p.tick < before);
                    points.insert(
                        i,
                        AutoPoint {
                            tick: before,
                            value: old,
                        },
                    );
                }
                before
            }
        };
        points.retain(|p| p.tick <= from || p.tick > tick);
        let n = points.partition_point(|p| p.tick <= from);
        // Holding still needs no points in between: the last one moves along.
        if after.is_some() && n >= 2 && points[n - 1].value == value && points[n - 2].value == value
        {
            points[n - 1].tick = tick;
        } else {
            points.insert(n, AutoPoint { tick, value });
        }
    }

    /// Plugin lanes after an insert is taken out of slot `i`: its own go, later ones move up.
    fn insert_removed(&mut self, i: usize) {
        self.plugins.retain(|a| a.plugin != Which::Insert(i));
        for a in &mut self.plugins {
            if let Which::Insert(j) = &mut a.plugin
                && *j > i
            {
                *j -= 1;
            }
        }
    }

    /// Drops a plugin's lanes, when another plugin takes its place.
    fn forget(&mut self, which: Which) {
        self.plugins.retain(|a| a.plugin != which);
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
    /// The built-in sampler: samples on notes.
    Sampler(Sampler),
    /// A plugin of any format (once only VST3, which is still read as `"vst3"`).
    #[serde(alias = "vst3")]
    Plugin(PluginRef),
}

impl Instrument {
    /// Whether its notes are sounds rather than pitches: a drum kit, or a sampler with a sample
    /// on each note. A key change leaves them be, and they don't count towards the key.
    pub fn is_kit(&self) -> bool {
        match self {
            Instrument::Drums => true,
            Instrument::Sampler(s) => {
                !s.zones.is_empty() && s.zones.iter().all(|z| z.high.is_none())
            }
            _ => false,
        }
    }
}

/// The built-in sampler, as Logic's Quick Sampler or an MPC's pads: a sample on each note, as a
/// drum kit, or one sample played across a range of notes, repitched, as an 808 or a bass.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Sampler {
    pub zones: Vec<Zone>,
}

/// A sample, and the notes it plays on.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Zone {
    /// Relative to the project folder.
    pub file: PathBuf,
    /// What it's called: "Kick".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The note it plays on; with `high`, the lowest of a range.
    pub note: u8,
    /// The highest note of its range, if it has one: across it, it's repitched from `root`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub high: Option<u8>,
    /// The note the sample sounds at as recorded; `note` by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<u8>,
    /// Fine tuning, in cents: a sample a little sharp of its root comes down by this.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub tune_cents: f32,
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub gain_db: f32,
    /// -1 left to 1 right.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub pan: f32,
    /// Stops when the note does (fading over `release_ms`), rather than playing to its end as a
    /// drum hit does.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gate: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_ms: Option<f32>,
    /// Sounds in one group cut each other off, as a closed hat cuts off an open one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choke: Option<u8>,
}

impl Zone {
    /// Whether it plays a note.
    pub fn covers(&self, note: u8) -> bool {
        (self.note..=self.high.unwrap_or(self.note)).contains(&note)
    }
}

fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
}

/// A plugin format.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Vst3,
    Clap,
    Vst2,
    Au,
}

impl Format {
    fn is_vst3(&self) -> bool {
        *self == Format::Vst3
    }

    pub fn name(self) -> &'static str {
        match self {
            Format::Vst3 => "VST3",
            Format::Clap => "CLAP",
            Format::Vst2 => "VST2",
            Format::Au => "Audio Unit",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct PluginRef {
    /// Its format: VST3 unless it says.
    #[serde(default, skip_serializing_if = "Format::is_vst3")]
    pub format: Format,
    /// The bundle: a `.vst3`, `.clap`, `.vst` or `.component` (an Audio Unit may have none).
    #[serde(default)]
    pub path: PathBuf,
    /// Its name, which picks it out of a bundle with more than one.
    pub class: String,
    /// What its format knows it by, where a name isn't enough: an Audio Unit's type, subtype
    /// and manufacturer ("aumu samp appl"), a CLAP plugin's id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    /// Its settings, saved under `Plugin States/` in the project folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<PathBuf>,
    /// The preset tempo last loaded into it: loaded over its settings, and undone like any edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<Preset>,
    /// Parameters set from tempo (its Smart Controls, or an agent), by id, 0 to 1, in the order
    /// they were set: applied over its preset and settings, and undone like any edit.
    #[serde(default, skip_serializing_if = "Params::is_empty")]
    pub params: Params,
    /// For a plugin with a sidechain input: the track (by id) it hears there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidechain: Option<u64>,
}

impl PluginRef {
    /// Which plugin this is, settings aside.
    pub fn same_plugin(&self, other: &PluginRef) -> bool {
        self.format == other.format
            && self.path == other.path
            && self.class == other.class
            && self.uid == other.uid
    }
}

/// Something a plugin can load: one of its own programs, or a preset file.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Preset {
    pub name: String,
    #[serde(flatten)]
    pub source: PresetSource,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum PresetSource {
    /// A program the plugin has built in: its place in the plugin's list `list` (VST3 has
    /// several lists, the others one, 0).
    Program {
        #[serde(default)]
        list: i32,
        program: u32,
    },
    /// A preset file: `.vstpreset`, `.clap-preset` or whatever the plugin keeps, `.fxp`,
    /// `.aupreset`.
    File { file: PathBuf },
}

/// Parameter values by id, in the order they were set: some parameters (an effect's type, a
/// mode) reset others, so order matters. Written as a JSON object, in that order.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Params(Vec<(u32, f64)>);

impl Params {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn get(&self, id: &u32) -> Option<&f64> {
        self.0.iter().find(|(k, _)| k == id).map(|(_, v)| v)
    }

    pub fn contains_key(&self, id: &u32) -> bool {
        self.get(id).is_some()
    }

    /// Sets a parameter, as the latest set. Returns what it was.
    pub fn insert(&mut self, id: u32, value: f64) -> Option<f64> {
        let was = self.remove(&id);
        self.0.push((id, value));
        was
    }

    pub fn remove(&mut self, id: &u32) -> Option<f64> {
        let i = self.0.iter().position(|(k, _)| k == id)?;
        Some(self.0.remove(i).1)
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = (&u32, &f64)> {
        self.0.iter().map(|(k, v)| (k, v))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&u32, &mut f64)> {
        self.0.iter_mut().map(|(k, v)| (&*k, v))
    }
}

impl<'a> IntoIterator for &'a Params {
    type Item = (&'a u32, &'a f64);
    type IntoIter =
        std::iter::Map<std::slice::Iter<'a, (u32, f64)>, fn(&'a (u32, f64)) -> (&'a u32, &'a f64)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(|(k, v)| (k, v))
    }
}

impl FromIterator<(u32, f64)> for Params {
    fn from_iter<I: IntoIterator<Item = (u32, f64)>>(iter: I) -> Self {
        let mut p = Params::default();
        for (k, v) in iter {
            p.insert(k, v);
        }
        p
    }
}

impl Serialize for Params {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            map.serialize_entry(&k.to_string(), v)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Params {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visit;
        impl<'de> serde::de::Visitor<'de> for Visit {
            type Value = Params;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("parameter values by id")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut m: A) -> Result<Params, A::Error> {
                let mut p = Params::default();
                while let Some((k, v)) = m.next_entry::<String, f64>()? {
                    let id = k.parse().map_err(serde::de::Error::custom)?;
                    p.insert(id, v);
                }
                Ok(p)
            }
        }
        d.deserialize_map(Visit)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Region {
    /// What it's called, like "Hook"; without a name it goes by its track's.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Position and length on the timeline, in ticks.
    #[serde(default)]
    pub start: u64,
    pub length: u64,
    /// Aliases share a link: they play the same notes or steps, and editing one edits them all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<u64>,
    /// Plays over and over, until the next region on its track or the end of the song.
    #[serde(default, rename = "loop", skip_serializing_if = "std::ops::Not::not")]
    pub looped: bool,
    /// Playback settings, as Logic's region inspector has: MIDI plays moved by `transpose`
    /// semitones, `velocity` louder, and started on a `quantize` grid (ticks), its notes kept as
    /// they are. Audio is moved by `transpose` too, its length kept.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub transpose: i8,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub velocity: i8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantize: Option<u64>,
    #[serde(flatten)]
    pub content: Content,
}

fn is_zero(v: &i8) -> bool {
    *v == 0
}

impl Region {
    /// A note as this region plays it, after its playback settings.
    pub fn played(&self, n: &Note) -> Note {
        let start = match self.quantize.filter(|&q| q > 0) {
            Some(q) => {
                let q = q as i64;
                (n.start + q / 2).div_euclid(q) * q
            }
            None => n.start,
        };
        Note {
            start,
            pitch: (n.pitch as i16 + self.transpose as i16).clamp(0, 127) as u8,
            velocity: (n.velocity as i16 + self.velocity as i16).clamp(1, 127) as u8,
            ..*n
        }
    }

    /// Whether its playback settings change anything.
    pub fn plain(&self) -> bool {
        self.transpose == 0 && self.velocity == 0 && self.quantize.is_none()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Content {
    Midi {
        notes: Vec<Note>,
        /// Controllers, pitch bend, pressure and per-note expression, in time order.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        controls: Vec<ControlPoint>,
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
        /// How late every second step comes, as MPC swing: 50 is straight, 66 a triplet feel,
        /// up to 75.
        #[serde(default = "straight", skip_serializing_if = "is_straight")]
        swing: f32,
    },
    /// A take folder: every take recorded over the region, and the comp of them that plays.
    Takes {
        takes: Vec<Take>,
        /// Which take plays from where, each piece until the next. Empty plays the newest take.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        comp: Vec<CompPiece>,
    },
}

impl Default for Content {
    fn default() -> Self {
        Content::Midi {
            notes: Vec::new(),
            controls: Vec::new(),
        }
    }
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

fn straight() -> f32 {
    50.0
}

fn is_straight(v: &f32) -> bool {
    *v == 50.0
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PatternRow {
    pub pitch: u8,
    /// A velocity per step; 0 is a rest.
    pub cells: Vec<u8>,
}

/// The notes a pattern plays over `length` ticks, its steps repeating, every second step late by
/// its swing.
pub fn pattern_notes(step: u64, rows: &[PatternRow], length: u64, swing: f32) -> Vec<Note> {
    // Swing is where in a pair of steps the second lands: at 50% exactly between.
    let late = ((swing.clamp(50.0, 75.0) - 50.0) / 50.0 * step as f32) as i64;
    let steps = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0) as u64;
    let mut notes = Vec::new();
    if steps == 0 || step == 0 {
        return notes;
    }
    for k in 0..length.div_ceil(step) {
        for row in rows {
            let velocity = row.cells.get((k % steps) as usize).copied().unwrap_or(0);
            if velocity > 0 {
                let swung = if k % 2 == 1 { late } else { 0 };
                let (start, length) = ((k * step) as i64 + swung, (step * 3 / 4).max(1));
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
            Content::Midi { notes, .. } => Some(Cow::Borrowed(notes)),
            Content::Pattern { step, rows, swing } => {
                Some(Cow::Owned(pattern_notes(*step, rows, length, *swing)))
            }
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

/// A move of a MIDI controller, pitch bend or pressure, or of one note's expression, at `start`
/// ticks from the region's start. It holds until the next move of the same thing.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ControlPoint {
    pub start: i64,
    #[serde(flatten)]
    pub kind: ControlValue,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlValue {
    /// A MIDI controller, 0-127 (1 is the mod wheel, 7 volume, 10 pan, 11 expression, 64 the
    /// sustain pedal), set to 0-127.
    Cc { number: u8, value: u8 },
    /// -8192 to 8191, 0 being none; how many semitones that is is the instrument's bend range.
    PitchBend { value: i16 },
    /// Channel pressure, 0-127.
    Aftertouch { value: u8 },
    /// One note's pressure, 0-127.
    PolyPressure { pitch: u8, value: u8 },
    /// One sounding note's expression (instruments that have it: VST3 and CLAP): `tuning` in
    /// semitones (-120 to 120), `volume` as gain (0 to 4, 1 as played), `pan` -1 to 1,
    /// `brightness`, `vibrato` and `expression` 0 to 1.
    Expression {
        pitch: u8,
        kind: ExpressionKind,
        value: f32,
    },
}

/// The moves from `from` to `to`, led by each controller's value as it stands at `from` (its
/// last move before, moved to `from`).
pub fn chase(
    points: impl Iterator<Item = ControlPoint>,
    from: i64,
    to: i64,
) -> impl Iterator<Item = ControlPoint> {
    let mut before: Vec<ControlPoint> = Vec::new();
    let mut inside = Vec::new();
    for p in points {
        if p.start < from {
            match before.iter_mut().find(|b| b.kind.same_thing(&p.kind)) {
                Some(b) if b.start <= p.start => *b = p,
                Some(_) => {}
                None => before.push(p),
            }
        } else if p.start < to {
            inside.push(p);
        }
    }
    for b in &mut before {
        b.start = from;
    }
    inside.sort_by_key(|p| p.start);
    before.into_iter().chain(inside)
}

impl ControlValue {
    /// Whether two moves are of the same thing: the same controller, or the same note's
    /// pressure or expression.
    pub fn same_thing(&self, other: &ControlValue) -> bool {
        use ControlValue::*;
        match (self, other) {
            (Cc { number: a, .. }, Cc { number: b, .. }) => a == b,
            (PitchBend { .. }, PitchBend { .. }) | (Aftertouch { .. }, Aftertouch { .. }) => true,
            (PolyPressure { pitch: a, .. }, PolyPressure { pitch: b, .. }) => a == b,
            (
                Expression {
                    pitch: a, kind: x, ..
                },
                Expression {
                    pitch: b, kind: y, ..
                },
            ) => a == b && x == y,
            _ => false,
        }
    }

    /// What it means to an instrument.
    pub fn to_control(self) -> crate::plugin::ControlKind {
        use crate::plugin::ControlKind as K;
        match self {
            ControlValue::Cc { number, value } => K::Cc {
                number: number.min(127),
                value: value.min(127) as f32 / 127.0,
            },
            ControlValue::PitchBend { value } => {
                K::PitchBend((value as f32 / 8192.0).clamp(-1.0, 1.0))
            }
            ControlValue::Aftertouch { value } => K::Aftertouch(value.min(127) as f32 / 127.0),
            ControlValue::PolyPressure { pitch, value } => K::PolyPressure {
                pitch,
                value: value.min(127) as f32 / 127.0,
            },
            ControlValue::Expression { pitch, kind, value } => K::Expression { pitch, kind, value },
        }
    }

    /// The note it's about, if it's about one.
    pub fn pitch_mut(&mut self) -> Option<&mut u8> {
        match self {
            ControlValue::PolyPressure { pitch, .. } | ControlValue::Expression { pitch, .. } => {
                Some(pitch)
            }
            _ => None,
        }
    }
}

/// Per-note expression, as VST3 and CLAP have it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExpressionKind {
    /// Semitones up or down, -120 to 120.
    Tuning,
    /// Gain, 0 to 4 (1 is as played).
    Volume,
    /// -1 left to 1 right.
    Pan,
    /// 0 to 1.
    Brightness,
    /// 0 to 1.
    Vibrato,
    /// 0 to 1.
    Expression,
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
            master_eq: None,
            master_comp: None,
            master_sat: None,
            master_inserts: Vec::new(),
            time_signature: common_time(),
            tracks: Vec::new(),
            auxes: Vec::new(),
            markers: Vec::new(),
            key: None,
            cycle: None,
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

    /// Where a looped region stops: at the next region on its track, or else the end of the song.
    pub fn loop_end(&self, at: At) -> u64 {
        let r = self.region(at);
        let next = self.tracks[at.0]
            .regions
            .iter()
            .map(|o| o.start)
            .filter(|&s| s > r.start)
            .min();
        next.unwrap_or_else(|| self.end()).max(r.start + r.length)
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
            Content::Midi { notes, .. } => Some(notes),
            Content::Audio { .. } | Content::Takes { .. } | Content::Pattern { .. } => None,
        }
    }

    /// A MIDI region's controller moves, if it is one.
    pub fn controls_mut(&mut self, at: At) -> Option<&mut Vec<ControlPoint>> {
        match &mut self.tracks.get_mut(at.0)?.regions.get_mut(at.1)?.content {
            Content::Midi { controls, .. } => Some(controls),
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
                ..Default::default()
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
            let (mut notes, mut controls) = (Vec::new(), Vec::new());
            for &(a, b, t) in &segments {
                let take = &takes[t];
                if let Some(n) = take.content.notes((b as i64 - take.start).max(0) as u64) {
                    let inside = |n: &&Note| (a as i64..b as i64).contains(&(take.start + n.start));
                    notes.extend(n.iter().filter(inside).map(|n| Note {
                        start: take.start + n.start,
                        ..*n
                    }));
                }
                // Each piece's controllers, as they stand where it begins.
                if let Content::Midi { controls: c, .. } = &take.content {
                    let moved = c.iter().map(|c| ControlPoint {
                        start: take.start + c.start,
                        ..*c
                    });
                    controls.extend(chase(moved, a as i64, b as i64));
                }
            }
            out.push(Region {
                start: r.start,
                length: r.length,
                content: Content::Midi { notes, controls },
                ..Default::default()
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
                        ..Default::default()
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
            // Its notes move, so an alias trimmed at the start becomes a region of its own.
            Content::Midi { notes, controls } => {
                r.link = None;
                for n in notes {
                    n.start -= start as i64 - old as i64;
                }
                for c in controls {
                    c.start -= start as i64 - old as i64;
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

    /// Makes an alias of a MIDI or pattern region at `start` on `track`: a region linked to it,
    /// so editing either's notes or steps edits both. Returns where it went.
    pub fn alias(&mut self, at: At, track: usize, start: u64) -> Result<At, String> {
        let r = self.region(at);
        if !matches!(r.content, Content::Midi { .. } | Content::Pattern { .. }) {
            return Err("only MIDI and pattern regions have aliases".into());
        }
        if self.tracks[track].instrument.is_none() {
            return Err("an alias goes on an instrument track".into());
        }
        let link = match r.link {
            Some(l) => l,
            None => {
                let next = self
                    .tracks
                    .iter()
                    .flat_map(|t| &t.regions)
                    .filter_map(|r| r.link)
                    .max()
                    .map_or(1, |l| l + 1);
                self.tracks[at.0].regions[at.1].link = Some(next);
                next
            }
        };
        let alias = Region {
            start,
            link: Some(link),
            ..self.region(at).clone()
        };
        self.tracks[track].regions.push(alias);
        Ok((track, self.tracks[track].regions.len() - 1))
    }

    /// Makes an alias a region of its own, its notes copied.
    pub fn unlink(&mut self, at: At) {
        self.tracks[at.0].regions[at.1].link = None;
    }

    /// What each group of aliases plays now, for `sync_links`.
    pub fn link_contents(&self) -> HashMap<u64, Content> {
        let mut out = HashMap::new();
        for r in self.tracks.iter().flat_map(|t| &t.regions) {
            if let Some(l) = r.link {
                out.entry(l).or_insert_with(|| r.content.clone());
            }
        }
        out
    }

    /// Keeps aliases alike after an edit: in each group, the region whose content changed since
    /// `last` (the groups' contents after the previous call) is copied to the others. A region
    /// left on its own is no longer an alias.
    pub fn sync_links(&mut self, last: &mut HashMap<u64, Content>) {
        let mut groups: BTreeMap<u64, Vec<At>> = BTreeMap::new();
        for (t, track) in self.tracks.iter().enumerate() {
            for (r, region) in track.regions.iter().enumerate() {
                if let Some(l) = region.link {
                    groups.entry(l).or_default().push((t, r));
                }
            }
        }
        last.retain(|l, _| groups.contains_key(l));
        for (link, members) in groups {
            if members.len() == 1 {
                let (t, r) = members[0];
                self.tracks[t].regions[r].link = None;
                last.remove(&link);
                continue;
            }
            let changed = last
                .get(&link)
                .and_then(|c| {
                    members
                        .iter()
                        .copied()
                        .find(|&at| self.region(at).content != *c)
                })
                .unwrap_or(members[0]);
            let content = self.region(changed).content.clone();
            for &(t, r) in members.iter().filter(|&&at| at != changed) {
                let other = &mut self.tracks[t].regions[r].content;
                if *other != content {
                    *other = content.clone();
                }
            }
            last.insert(link, content);
        }
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
            // Each half keeps only its notes, so neither is an alias any more.
            (
                Content::Midi {
                    notes: a,
                    controls: ca,
                },
                Content::Midi {
                    notes: b,
                    controls: cb,
                },
            ) => {
                r.link = None;
                second.link = None;
                a.retain(|n| n.start < first as i64);
                b.retain(|n| n.start >= first as i64);
                for n in b {
                    n.start -= first as i64;
                }
                // The second half starts with each controller where the first left it.
                ca.retain(|c| c.start < first as i64);
                *cb = chase(cb.iter().copied(), first as i64, i64::MAX)
                    .map(|c| ControlPoint {
                        start: c.start - first as i64,
                        ..c
                    })
                    .collect();
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
            content: Content::Midi {
                notes,
                controls: Vec::new(),
            },
            ..Default::default()
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
            ..Default::default()
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
            Content::Midi { notes, .. } => {
                notes.iter().map(|n| (n.start, n.pitch)).collect::<Vec<_>>()
            }
            _ => unreachable!(),
        };
        assert_eq!(pitches(a), [(0, 60), (900, 62)]);
        assert_eq!(pitches(b), [(0, 64)]);
    }

    #[test]
    fn splitting_midi_keeps_each_controller_where_it_stood() {
        let cc = |start, value| ControlPoint {
            start,
            kind: ControlValue::Cc { number: 1, value },
        };
        let bend = |start, value| ControlPoint {
            start,
            kind: ControlValue::PitchBend { value },
        };
        let mut p = Project::new(48_000);
        p.tracks.push(Track {
            regions: vec![Region {
                start: 0,
                length: 4 * PPQ,
                content: Content::Midi {
                    notes: vec![],
                    controls: vec![
                        cc(0, 10),
                        bend(100, 500),
                        cc(PPQ as i64, 90),
                        cc(3 * PPQ as i64, 127),
                    ],
                },
                ..Default::default()
            }],
            ..Default::default()
        });
        let second = p.split_region((0, 0), 2 * PPQ).unwrap();
        let controls = |at| match &p.region(at).content {
            Content::Midi { controls, .. } => controls.clone(),
            _ => unreachable!(),
        };
        assert_eq!(
            controls((0, 0)),
            [cc(0, 10), bend(100, 500), cc(PPQ as i64, 90)]
        );
        // The second half starts with the wheel at 90 and the bend where it was.
        let mut start = controls(second);
        start.sort_by_key(|c| (c.start, matches!(c.kind, ControlValue::PitchBend { .. })));
        assert_eq!(start, [cc(0, 90), bend(0, 500), cc(PPQ as i64, 127)]);
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
        let Content::Midi { notes, .. } = &r.content else {
            unreachable!()
        };
        assert_eq!((notes[0].start, notes[1].start), (-480, 0));
        p.trim_start((0, 0), 960); // back again: the hidden note returns
        let Content::Midi { notes, .. } = &p.region((0, 0)).content else {
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
            content: Content::Midi {
                notes,
                controls: Vec::new(),
            },
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
        let at = p.add_take(
            0,
            1440,
            3360,
            Content::Midi {
                notes,
                controls: Vec::new(),
            },
        );
        assert_eq!(at, (0, 0));
        let r = p.region(at);
        assert_eq!((r.start, r.length), (960, 2400)); // grown to the end of the new take
        assert_eq!(folder(&p, at), (vec![0, 480], vec![(0, 2400, 1)])); // the newest plays
        // Recording somewhere else makes a plain region.
        assert_eq!(
            p.add_take(
                0,
                7680,
                8640,
                Content::Midi {
                    notes: vec![],
                    controls: Vec::new()
                }
            ),
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
            ..Default::default()
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
        let Content::Midi { notes, .. } = &mut late.content else {
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
            ..Default::default()
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
        let Content::Midi { notes, .. } = &p.region(flat[0]).content else {
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

    #[test]
    fn aliases_share_their_notes_until_unlinked() {
        let mut p = project(vec![midi(0, 960, &[(0, 60)])], true);
        let alias = p.alias((0, 0), 0, 3840).unwrap();
        assert!(p.region(alias).link.is_some());
        assert_eq!(p.region(alias).link, p.region((0, 0)).link);
        assert_eq!(p.region(alias).start, 3840);
        let mut links = p.link_contents();
        // Editing the alias edits the original, and the other way round.
        p.notes_mut(alias).unwrap()[0].pitch = 64;
        p.sync_links(&mut links);
        assert_eq!(p.notes_mut((0, 0)).unwrap()[0].pitch, 64);
        p.notes_mut((0, 0)).unwrap()[0].pitch = 67;
        p.sync_links(&mut links);
        assert_eq!(p.notes_mut(alias).unwrap()[0].pitch, 67);
        // Its place and playback settings are its own.
        p.tracks[0].regions[1].transpose = 12;
        p.sync_links(&mut links);
        assert_eq!(p.region((0, 0)).transpose, 0);
        // Splitting makes regions of their own; one left alone isn't an alias.
        p.split_region(alias, 3840 + 480).unwrap();
        p.sync_links(&mut links);
        assert!(p.tracks[0].regions.iter().all(|r| r.link.is_none()));
        // Audio has no aliases.
        let mut p = project(vec![audio(0, 960, 0)], false);
        assert!(p.alias((0, 0), 0, 960).is_err());
    }

    #[test]
    fn recording_automation_replaces_the_line_it_passes() {
        let mut a = Automation::default();
        for tick in [0, 3840] {
            a.add(Param::Volume, AutoPoint { tick, value: -6.0 });
        }
        a.write(Param::Volume, None, 960, 0.0);
        a.write(Param::Volume, Some(960), 1440, -3.0);
        a.write(Param::Volume, Some(1440), 1920, -3.0);
        a.write(Param::Volume, Some(1920), 2400, -3.0); // held still: the last point moves on
        let points: Vec<(u64, f32)> = a.volume.iter().map(|p| (p.tick, p.value)).collect();
        assert_eq!(
            points,
            [
                (0, -6.0),
                (959, -6.0), // the line as it was, up to where recording began
                (960, 0.0),
                (1440, -3.0),
                (2400, -3.0),
                (3840, -6.0)
            ]
        );
        // Plugin parameters record the same way, in lanes of their own.
        let cutoff = Param::Plugin(Which::Instrument, 7);
        a.write(cutoff, None, 0, 0.25);
        a.name(cutoff, "Cutoff");
        assert_eq!(a.points(cutoff).len(), 1);
        assert_eq!(a.plugins[0].name, "Cutoff");
        assert_eq!(a.params(), [Param::Volume, cutoff]);
    }

    #[test]
    fn a_slots_automation_goes_with_its_plugin() {
        let plugin = |class: &str| PluginRef {
            path: "/p.vst3".into(),
            class: class.into(),
            ..Default::default()
        };
        let mut t = Track {
            inserts: vec![plugin("a"), plugin("b")],
            ..Default::default()
        };
        for i in 0..2 {
            t.automation.add(
                Param::Plugin(Which::Insert(i), 1),
                AutoPoint {
                    tick: 0,
                    value: i as f32,
                },
            );
        }
        t.set_insert(0, None).unwrap();
        assert_eq!(t.automation.plugins.len(), 1);
        assert_eq!(t.automation.plugins[0].plugin, Which::Insert(0)); // b's, moved up
        assert_eq!(t.automation.plugins[0].points[0].value, 1.0);
        // The same plugin with other settings keeps it; another plugin doesn't.
        t.set_insert(0, Some(plugin("b"))).unwrap();
        assert_eq!(t.automation.plugins.len(), 1);
        t.set_insert(0, Some(plugin("c"))).unwrap();
        assert!(t.automation.plugins.is_empty());
    }

    #[test]
    fn time_signatures_set_the_bar() {
        let mut p = Project::new(48_000);
        assert_eq!((p.beat(), p.bar()), (PPQ, 4 * PPQ));
        p.time_signature = [6, 8];
        assert_eq!(
            (p.beat(), p.bar(), p.beats_per_bar()),
            (PPQ / 2, 3 * PPQ, 6)
        );
        p.time_signature = [3, 4];
        assert_eq!(p.bar(), 3 * PPQ);
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"time_signature\":[3,4]"));
        assert!(
            !serde_json::to_string(&Project::new(48_000))
                .unwrap()
                .contains("time_signature")
        );
    }

    #[test]
    fn changing_the_key_can_move_the_music_into_it() {
        let mut p = project(vec![midi(0, 960, &[(0, 60), (240, 64), (480, 67)])], true);
        p.add_track(
            1,
            Track {
                instrument: Some(Instrument::Drums),
                regions: vec![midi(0, 960, &[(0, 36)])],
                ..Default::default()
            },
        );
        let key = |s: &str| crate::music::Key::parse(s);
        let pitches = |p: &Project, t: usize| match &p.tracks[t].regions[0].content {
            Content::Midi { notes, .. } => notes.iter().map(|n| n.pitch).collect::<Vec<_>>(),
            _ => unreachable!(),
        };
        assert_eq!(p.song_key(), key("C major")); // a C major chord
        // Just naming it: nothing moves.
        let mut named = p.clone();
        named.set_key(key("D major"), false);
        assert_eq!(pitches(&named, 0), [60, 64, 67]);
        // Moving into D major: a D major chord; into C minor: C minor's. Drums stay.
        let mut d = p.clone();
        d.set_key(key("D major"), true);
        assert_eq!(
            (pitches(&d, 0), d.key.as_deref()),
            (vec![62, 66, 69], Some("D major"))
        );
        assert_eq!(pitches(&d, 1), [36]);
        p.set_key(key("C minor"), true);
        assert_eq!(pitches(&p, 0), [60, 63, 67]);
        // Back to working it out: nothing moves.
        p.set_key(None, true);
        assert_eq!((pitches(&p, 0), p.key.clone()), (vec![60, 63, 67], None));
    }

    #[test]
    fn swing_makes_every_second_step_late() {
        let rows = [PatternRow {
            pitch: 42,
            cells: vec![100; 4],
        }];
        let starts = |swing| {
            pattern_notes(PPQ / 4, &rows, PPQ, swing)
                .iter()
                .map(|n| n.start)
                .collect::<Vec<_>>()
        };
        assert_eq!(starts(50.0), [0, 240, 480, 720]);
        // 66%: the second of each pair two thirds of the way through it, a triplet feel.
        assert_eq!(starts(66.0), [0, 316, 480, 796]);
    }
}
