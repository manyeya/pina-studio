//! A project: what's on the timeline and how it's mixed. It's a folder:
//! `project.json` plus the audio it uses under `Audio Files/`. The JSON is the source of truth, so
//! people and agents can read, diff and edit it.

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::modulate::{MAX_MODS, ModKind, Modulator};

pub use crate::stretch::Flex;

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

/// What an EQ band does to the sound.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    #[default]
    Bell,
    LowShelf,
    LowCut,
    HighShelf,
    HighCut,
    Notch,
    BandPass,
    /// A shelf that tilts round its frequency: half its gain down below, half up above.
    TiltShelf,
    /// A straight tilt across all that's heard (20 Hz to 20 kHz), level at its frequency: its
    /// gain the difference from end to end.
    FlatTilt,
    /// Shifts the phase round its frequency and leaves the level: to line up two takes.
    AllPass,
}

impl Shape {
    pub const ALL: [Shape; 10] = [
        Shape::Bell,
        Shape::LowShelf,
        Shape::LowCut,
        Shape::HighShelf,
        Shape::HighCut,
        Shape::Notch,
        Shape::BandPass,
        Shape::TiltShelf,
        Shape::FlatTilt,
        Shape::AllPass,
    ];

    /// The slope that's a brickwall: as steep as cuts go.
    pub const BRICKWALL: f32 = 144.0;

    pub fn name(self) -> &'static str {
        match self {
            Shape::Bell => "Bell",
            Shape::LowShelf => "Low Shelf",
            Shape::LowCut => "Low Cut",
            Shape::HighShelf => "High Shelf",
            Shape::HighCut => "High Cut",
            Shape::Notch => "Notch",
            Shape::BandPass => "Band Pass",
            Shape::TiltShelf => "Tilt Shelf",
            Shape::FlatTilt => "Flat Tilt",
            Shape::AllPass => "All Pass",
        }
    }

    /// Whether it boosts or cuts by a gain (the others only take away).
    pub fn has_gain(self) -> bool {
        matches!(
            self,
            Shape::Bell | Shape::LowShelf | Shape::HighShelf | Shape::TiltShelf | Shape::FlatTilt
        )
    }

    /// Whether it can move with the sound: a bell, shelf or tilt.
    pub fn can_be_dynamic(self) -> bool {
        self.has_gain()
    }

    /// The slopes it steps through, in dB an octave (its menu's, the wheel's); those between
    /// work too, as far as `slope_range` goes, but for a notch and an all pass, whose slopes go
    /// in twelves (their filters' order). None for a flat tilt.
    pub fn slopes(self) -> &'static [f32] {
        match self {
            Shape::LowCut | Shape::HighCut => &[
                6.0,
                12.0,
                18.0,
                24.0,
                30.0,
                36.0,
                48.0,
                72.0,
                96.0,
                Shape::BRICKWALL,
            ],
            Shape::LowShelf | Shape::HighShelf | Shape::TiltShelf | Shape::BandPass => {
                &[6.0, 12.0, 18.0, 24.0, 30.0, 36.0, 48.0, 72.0, 96.0]
            }
            Shape::Bell => &[12.0, 18.0, 24.0, 30.0, 36.0, 48.0, 72.0, 96.0],
            Shape::Notch | Shape::AllPass => &[12.0, 24.0, 36.0, 48.0, 72.0, 96.0],
            Shape::FlatTilt => &[],
        }
    }

    /// The slopes it takes, least and most, in dB an octave (a cut's brickwall besides): from 0
    /// for a cut or band pass (none at all), 12 for a bell, notch or all pass (a second-order
    /// filter's), 6 for the rest.
    pub fn slope_range(self) -> Option<(f32, f32)> {
        match self {
            Shape::LowCut | Shape::HighCut | Shape::BandPass => Some((0.0, 96.0)),
            Shape::LowShelf | Shape::HighShelf | Shape::TiltShelf => Some((6.0, 96.0)),
            Shape::Bell | Shape::Notch | Shape::AllPass => Some((12.0, 96.0)),
            Shape::FlatTilt => None,
        }
    }

    /// The slope it takes nearest `s`.
    pub fn fit_slope(self, s: f32) -> f32 {
        let Some((lo, hi)) = self.slope_range() else {
            return 12.0;
        };
        if matches!(self, Shape::LowCut | Shape::HighCut) && s >= Shape::BRICKWALL {
            return Shape::BRICKWALL;
        }
        let s = s.clamp(lo, hi);
        match self {
            Shape::Notch | Shape::AllPass => (s / 12.0).round() * 12.0,
            _ => (s * 100.0).round() / 100.0,
        }
    }

    /// The width it starts with: a cut or shelf with no bump, a bell an octave and a half.
    pub fn default_q(self) -> f32 {
        match self {
            Shape::Bell | Shape::BandPass => 1.0,
            Shape::Notch => 4.0,
            _ => std::f32::consts::FRAC_1_SQRT_2,
        }
    }
}

/// Which part of a stereo sound a band works on.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    #[default]
    Stereo,
    Left,
    Right,
    /// What's the same on both sides: the middle.
    Mid,
    /// What differs: the edges.
    Side,
}

impl Placement {
    pub const ALL: [Placement; 5] = [
        Placement::Stereo,
        Placement::Left,
        Placement::Right,
        Placement::Mid,
        Placement::Side,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Placement::Stereo => "Stereo",
            Placement::Left => "Left",
            Placement::Right => "Right",
            Placement::Mid => "Mid",
            Placement::Side => "Side",
        }
    }

    fn is_stereo(&self) -> bool {
        *self == Placement::Stereo
    }
}

/// A band that moves with the sound: as what it covers gets louder than its threshold, its gain
/// goes by up to `range_db` more (below 0 down, as a compressor would; above, up). Without a
/// threshold it sets its own, following what the band usually hears, so it moves on what
/// stands out; without attack and release, its own, by its frequency. It listens to what the
/// band covers, or between `trigger`'s two frequencies; with `external`, to the EQ's sidechain.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Dynamic {
    pub range_db: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_db: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attack_ms: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_ms: Option<f32>,
    /// What it listens to, low and high Hz, if not what the band covers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<[f32; 2]>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub external: bool,
    /// Kept but still.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bypass: bool,
    /// Spectral: rather than the whole band moving, only what stands out in it, frequency by
    /// frequency (in linear phase, so with the EQ's latency). `density` 0 moves wide stretches
    /// together, 1 each frequency on its own; `tilt` has it hear the highs 3 dB an octave
    /// louder, as a mix's pink spectrum needs.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub spectral: bool,
    #[serde(skip_serializing_if = "is_half")]
    pub density: f32,
    #[serde(skip_serializing_if = "is_true")]
    pub tilt: bool,
}

fn is_half(v: &f32) -> bool {
    *v == 0.5
}

fn is_true(v: &bool) -> bool {
    *v
}

impl Default for Dynamic {
    fn default() -> Self {
        Dynamic {
            range_db: -6.0,
            threshold_db: None,
            attack_ms: None,
            release_ms: None,
            trigger: None,
            external: false,
            bypass: false,
            spectral: false,
            density: 0.5,
            tilt: true,
        }
    }
}

/// How an EQ's filters treat phase: as analog EQs do, with no latency; as an analog EQ's phase
/// too, all through what's heard (a little latency); or not at all, linear phase (more latency,
/// as `resolution` sets it, and its bands' timing kept together).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Processing {
    #[default]
    ZeroLatency,
    Natural,
    Linear,
}

impl Processing {
    pub const ALL: [Processing; 3] = [
        Processing::ZeroLatency,
        Processing::Natural,
        Processing::Linear,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Processing::ZeroLatency => "Zero Latency",
            Processing::Natural => "Natural Phase",
            Processing::Linear => "Linear Phase",
        }
    }

    fn is_zero(&self) -> bool {
        *self == Processing::ZeroLatency
    }
}

/// The resolutions linear phase and spectral bands can work at, low to very high.
pub const RESOLUTIONS: [&str; 4] = ["Low", "Medium", "High", "Very High"];

/// One band of an EQ: its shape at a frequency (Hz), its gain (dB), width (Q) and slope (dB an
/// octave, fractions too); the part of the sound it works on; and whether it moves with the
/// sound.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(from = "BandJson")]
pub struct EqBand {
    pub shape: Shape,
    pub freq: f32,
    #[serde(skip_serializing_if = "zero_f32")]
    pub gain_db: f32,
    pub q: f32,
    #[serde(skip_serializing_if = "is_twelve")]
    pub slope: f32,
    #[serde(skip_serializing_if = "Placement::is_stereo")]
    pub placement: Placement,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic: Option<Dynamic>,
    /// Kept but not heard.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub off: bool,
    /// Its width narrows as its gain grows, as an analog EQ's does.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub gain_q: bool,
    /// Which band it is while the song's open, so the engine keeps its sound through edits (a
    /// band taken out before it fades rather than clicking). Not saved.
    #[serde(skip)]
    pub id: u16,
}

fn zero_f32(v: &f32) -> bool {
    *v == 0.0
}

/// A band as JSON has it: anything left out as a 1 kHz bell has it, but its width, which is
/// its shape's (a cut's or shelf's with no bump).
#[derive(Deserialize)]
#[serde(default)]
struct BandJson {
    shape: Shape,
    freq: f32,
    gain_db: f32,
    q: Option<f32>,
    slope: f32,
    placement: Placement,
    dynamic: Option<Dynamic>,
    off: bool,
    gain_q: bool,
}

impl Default for BandJson {
    fn default() -> Self {
        let b = EqBand::default();
        BandJson {
            shape: b.shape,
            freq: b.freq,
            gain_db: b.gain_db,
            q: None,
            slope: b.slope,
            placement: b.placement,
            dynamic: None,
            off: false,
            gain_q: false,
        }
    }
}

impl From<BandJson> for EqBand {
    fn from(j: BandJson) -> Self {
        EqBand {
            shape: j.shape,
            freq: j.freq,
            gain_db: j.gain_db,
            q: j.q.unwrap_or(j.shape.default_q()),
            slope: j.slope,
            placement: j.placement,
            dynamic: j.dynamic,
            off: j.off,
            gain_q: j.gain_q,
            id: 0,
        }
    }
}

fn is_twelve(v: &f32) -> bool {
    *v == 12.0
}

impl Default for EqBand {
    fn default() -> Self {
        EqBand::new(Shape::Bell, 1_000.0, 0.0)
    }
}

impl EqBand {
    pub fn new(shape: Shape, freq: f32, gain_db: f32) -> Self {
        EqBand {
            shape,
            freq,
            gain_db,
            q: shape.default_q(),
            slope: 12.0,
            placement: Placement::Stereo,
            dynamic: None,
            off: false,
            gain_q: false,
            id: 0,
        }
    }

    /// Its gain as heard, with the EQ's gain scale: none for a shape that has none.
    pub fn gain(&self, scale: f32) -> f32 {
        match self.shape.has_gain() {
            true => self.gain_db * scale,
            false => 0.0,
        }
    }
}

/// An EQ's bands, at most `MAX_BANDS`, kept in place so an EQ copies without allocating (the
/// audio thread copies it to modulate it). In JSON, a list.
#[derive(Clone, Copy, PartialEq)]
pub struct Bands {
    n: u8,
    list: [EqBand; MAX_BANDS],
}

impl Default for Bands {
    fn default() -> Self {
        Bands {
            n: 0,
            list: [EqBand::default(); MAX_BANDS],
        }
    }
}

impl std::ops::Deref for Bands {
    type Target = [EqBand];

    fn deref(&self) -> &[EqBand] {
        &self.list[..self.n as usize]
    }
}

impl std::ops::DerefMut for Bands {
    fn deref_mut(&mut self) -> &mut [EqBand] {
        &mut self.list[..self.n as usize]
    }
}

impl std::fmt::Debug for Bands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl Bands {
    pub fn from_slice(bands: &[EqBand]) -> Self {
        let mut out = Bands::default();
        for b in bands {
            out.push(*b);
        }
        out
    }

    /// Adds a band at the end, giving it an id of its own if it hasn't one: where it went, or
    /// None if the EQ is full.
    pub fn push(&mut self, band: EqBand) -> Option<usize> {
        self.insert(self.n as usize, band)
    }

    pub fn insert(&mut self, at: usize, mut band: EqBand) -> Option<usize> {
        let n = self.n as usize;
        if n == MAX_BANDS {
            return None;
        }
        if band.id == 0 || self.iter().any(|b| b.id == band.id) {
            band.id = self.iter().map(|b| b.id).max().unwrap_or(0) + 1;
        }
        let at = at.min(n);
        self.list.copy_within(at..n, at + 1);
        self.list[at] = band;
        self.n += 1;
        Some(at)
    }

    pub fn remove(&mut self, at: usize) -> Option<EqBand> {
        let n = self.n as usize;
        if at >= n {
            return None;
        }
        let band = self.list[at];
        self.list.copy_within(at + 1..n, at);
        self.n -= 1;
        Some(band)
    }

    /// Where the band with this id is.
    pub fn find(&self, id: u16) -> Option<usize> {
        self.iter().position(|b| b.id == id)
    }
}

impl Serialize for Bands {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.iter())
    }
}

impl<'de> Deserialize<'de> for Bands {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let list = Vec::<EqBand>::deserialize(d)?;
        if list.len() > MAX_BANDS {
            return Err(serde::de::Error::custom(format!(
                "an EQ has at most {MAX_BANDS} bands"
            )));
        }
        // Ids afresh: they're the song's while it's open, not the file's.
        let mut out = Bands::default();
        for b in list {
            out.push(EqBand { id: 0, ..b });
        }
        Ok(out)
    }
}

/// A channel's built-in EQ: up to 24 bands, each a bell, shelf, cut, notch, band pass or tilt
/// over the whole sound or a part of it (left, right, mid, side), still or moving with the sound;
/// then a gain scale for them all and the output level. Anything left out of its JSON is as
/// `Eq::default()` has it: no bands, flat.
///
/// It reads the EQ Pina Studio had before too (a low cut, two bells and a high shelf, by name), and
/// takes those names in `set_eq`: `low_cut` is its first low cut, `low_mid` and `high_mid` its
/// first and second bells, `high_shelf` its first high shelf, each added if missing.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(from = "EqJson")]
pub struct Eq {
    pub bands: Bands,
    /// How much of each band's gain is heard: 1 as set, 0 none, 2 twice.
    #[serde(skip_serializing_if = "one_f32")]
    pub scale: f32,
    #[serde(skip_serializing_if = "zero_f32")]
    pub output_db: f32,
    /// Its output's balance, -1 to 1: left and right, or with `pan_ms` mid (-1) and side (1).
    #[serde(skip_serializing_if = "zero_f32")]
    pub pan: f32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub pan_ms: bool,
    /// Its output turned upside down.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub invert: bool,
    /// Makes up for how much louder or quieter its bands make the sound.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub auto_gain: bool,
    /// Clean, or with an analog EQ's saturation: subtle, or warm.
    #[serde(skip_serializing_if = "Character::is_clean")]
    pub character: Character,
    /// A track (by id) its dynamic bands can listen to instead of what goes through it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sidechain: Option<u64>,
    /// How its filters treat phase, and (for linear phase and spectral bands) at what
    /// resolution: 0 low to 3 very high, the latency doubling each step.
    #[serde(skip_serializing_if = "Processing::is_zero")]
    pub processing: Processing,
    #[serde(skip_serializing_if = "is_one_u8")]
    pub resolution: u8,
    /// Kept but not heard.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bypass: bool,
}

fn is_one_u8(v: &u8) -> bool {
    *v == 1
}

/// An EQ's character: clean (transparent), or the saturation of an analog one, subtle or warm.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Character {
    #[default]
    Clean,
    Subtle,
    Warm,
}

impl Character {
    pub const ALL: [Character; 3] = [Character::Clean, Character::Subtle, Character::Warm];

    fn is_clean(&self) -> bool {
        *self == Character::Clean
    }

    pub fn name(self) -> &'static str {
        match self {
            Character::Clean => "Clean",
            Character::Subtle => "Subtle",
            Character::Warm => "Warm",
        }
    }
}

fn one_f32(v: &f32) -> bool {
    *v == 1.0
}

impl Default for Eq {
    fn default() -> Self {
        Eq {
            bands: Bands::default(),
            scale: 1.0,
            output_db: 0.0,
            pan: 0.0,
            pan_ms: false,
            invert: false,
            auto_gain: false,
            character: Character::Clean,
            sidechain: None,
            processing: Processing::ZeroLatency,
            resolution: 1,
            bypass: false,
        }
    }
}

impl Eq {
    /// The EQ Pina Studio had before, flat: a low cut (off), bells at 400 Hz and 2.5 kHz and a high
    /// shelf at 10 kHz. Songs made with it open with these four, in this order, so what moved
    /// them by number (modulators) still does.
    pub fn classic() -> Self {
        let mut cut = EqBand::new(Shape::LowCut, 20.0, 0.0);
        cut.off = true;
        Eq {
            bands: Bands::from_slice(&[
                cut,
                EqBand::new(Shape::Bell, 400.0, 0.0),
                EqBand::new(Shape::Bell, 2_500.0, 0.0),
                EqBand::new(Shape::HighShelf, 10_000.0, 0.0),
            ]),
            ..Eq::default()
        }
    }

    /// Where the `nth` band of a shape is.
    pub fn nth(&self, shape: Shape, nth: usize) -> Option<usize> {
        self.bands
            .iter()
            .enumerate()
            .filter(|(_, b)| b.shape == shape)
            .nth(nth)
            .map(|(i, _)| i)
    }

    /// The `nth` band of a shape, added (as `new`) if there are fewer.
    fn nth_or_add(&mut self, shape: Shape, nth: usize, new: EqBand) -> Option<usize> {
        match self.nth(shape, nth) {
            Some(i) => Some(i),
            None => self.bands.push(new),
        }
    }

    /// The old EQ's names onto this one's bands.
    fn apply_classic(&mut self, j: &EqJson) {
        if let Some(hz) = j.low_cut {
            let on = hz > 20.0;
            match self.nth(Shape::LowCut, 0) {
                Some(i) => {
                    let b = &mut self.bands[i];
                    (b.freq, b.off) = (hz.max(20.0), !on);
                }
                None if on => {
                    self.bands.insert(0, EqBand::new(Shape::LowCut, hz, 0.0));
                }
                None => {}
            }
        }
        let named = [
            (&j.low_mid, Shape::Bell, 0, 400.0),
            (&j.high_mid, Shape::Bell, 1, 2_500.0),
            (&j.high_shelf, Shape::HighShelf, 0, 10_000.0),
        ];
        for (band, shape, nth, freq) in named {
            let Some(c) = band else { continue };
            let Some(i) = self.nth_or_add(shape, nth, EqBand::new(shape, freq, 0.0)) else {
                continue;
            };
            let b = &mut self.bands[i];
            b.freq = c.freq.unwrap_or(b.freq);
            b.gain_db = c.gain_db.unwrap_or(b.gain_db);
            b.q = c.q.unwrap_or(b.q);
        }
    }
}

/// An EQ as JSON has it: its bands, or the old EQ's names, or both (`set_eq` with the old names
/// on an EQ that has bands).
#[derive(Deserialize)]
struct EqJson {
    bands: Option<Bands>,
    #[serde(default = "unity")]
    scale: f32,
    #[serde(default)]
    output_db: f32,
    #[serde(default)]
    pan: f32,
    #[serde(default)]
    pan_ms: bool,
    #[serde(default)]
    invert: bool,
    #[serde(default)]
    auto_gain: bool,
    #[serde(default)]
    character: Character,
    #[serde(default)]
    sidechain: Option<u64>,
    #[serde(default)]
    processing: Processing,
    #[serde(default = "one_u8")]
    resolution: u8,
    #[serde(default)]
    bypass: bool,
    low_cut: Option<f32>,
    low_mid: Option<ClassicBand>,
    high_mid: Option<ClassicBand>,
    high_shelf: Option<ClassicBand>,
}

fn unity() -> f32 {
    1.0
}

fn one_u8() -> u8 {
    1
}

/// One of the old EQ's bands, as much of it as was given.
#[derive(Deserialize)]
struct ClassicBand {
    freq: Option<f32>,
    gain_db: Option<f32>,
    q: Option<f32>,
}

impl From<EqJson> for Eq {
    fn from(j: EqJson) -> Self {
        let classic = j.low_cut.is_some()
            || j.low_mid.is_some()
            || j.high_mid.is_some()
            || j.high_shelf.is_some();
        let mut eq = match (j.bands, classic) {
            (Some(bands), _) => Eq {
                bands,
                ..Eq::default()
            },
            (None, true) => Eq::classic(),
            (None, false) => Eq::default(),
        };
        eq.apply_classic(&j);
        eq.scale = j.scale.clamp(0.0, 2.0);
        eq.output_db = j.output_db;
        eq.pan = j.pan.clamp(-1.0, 1.0);
        eq.pan_ms = j.pan_ms;
        eq.invert = j.invert;
        eq.auto_gain = j.auto_gain;
        eq.character = j.character;
        eq.sidechain = j.sidechain;
        eq.processing = j.processing;
        eq.resolution = j.resolution.min(3);
        eq.bypass = j.bypass;
        eq
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

/// Where a binaural track's sound is round the listener's head.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    /// Degrees: 0 in front, 90 right, -90 left, ±180 behind.
    pub azimuth: f32,
    /// Degrees: 90 above, -90 below.
    pub elevation: f32,
    /// Metres from the head.
    pub distance: f32,
}

impl Place {
    pub const NEAREST: f32 = 0.25;
    pub const FARTHEST: f32 = 3.0;

    /// A track's place at a pan (its way round).
    pub fn of(track: &Track, pan: f32) -> Place {
        Place {
            azimuth: pan.clamp(-1.0, 1.0) * 180.0,
            elevation: track.elevation.unwrap_or(0.0).clamp(-90.0, 90.0),
            distance: track
                .distance
                .unwrap_or(1.0)
                .clamp(Place::NEAREST, Place::FARTHEST),
        }
    }

    /// Right, up and forward of the head, in metres.
    pub fn xyz(&self) -> [f32; 3] {
        let (a, e) = (self.azimuth.to_radians(), self.elevation.to_radians());
        let d = self.distance;
        [d * a.sin() * e.cos(), d * e.sin(), d * a.cos() * e.cos()]
    }

    /// From right, up and forward of the head.
    pub fn from_xyz([x, y, z]: [f32; 3]) -> Place {
        let flat = x.hypot(z);
        Place {
            azimuth: x.atan2(z).to_degrees(),
            elevation: y.atan2(flat).to_degrees(),
            distance: (flat.hypot(y)).clamp(Place::NEAREST, Place::FARTHEST),
        }
    }
}

/// How a track's pan places it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PanMode {
    /// One side turned down as the other stays: the stereo image kept.
    #[default]
    Balance,
    /// Each side placed on its own: the pan where they centre, `spread` how far apart.
    Stereo,
    /// Placed around the listener's head, as headphones hear it: the far ear later, quieter and
    /// darker.
    Binaural,
}

impl PanMode {
    pub fn is_balance(&self) -> bool {
        *self == PanMode::Balance
    }

    pub fn name(self) -> &'static str {
        match self {
            PanMode::Balance => "Balance",
            PanMode::Stereo => "Stereo Pan",
            PanMode::Binaural => "Binaural",
        }
    }
}

/// How loud a sound panned to the middle is, against one panned hard to a side.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PanLaw {
    /// Balance: the middle as loud as each side alone.
    #[default]
    Flat,
    /// Equal power: the middle 3 dB down.
    Minus3,
    /// Equal power, made up: the middle as it plays, the sides 3 dB up.
    Minus3Compensated,
    /// Equal voltage: the middle 6 dB down.
    Minus6,
}

impl PanLaw {
    pub fn is_flat(&self) -> bool {
        *self == PanLaw::Flat
    }

    pub fn name(self) -> &'static str {
        match self {
            PanLaw::Flat => "0 dB",
            PanLaw::Minus3 => "−3 dB",
            PanLaw::Minus3Compensated => "−3 dB compensated",
            PanLaw::Minus6 => "−6 dB",
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
    /// Its colour, "#d97757", if one was chosen; else its family's, by what it plays (`look`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Its icon (an id from `look::ICONS`), if one was chosen; else what it plays says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
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
    /// How the pan places it: balance, stereo pan (each side placed, `spread`
    /// apart), or binaural (around the head).
    #[serde(default, skip_serializing_if = "PanMode::is_balance")]
    pub pan_mode: PanMode,
    /// Stereo pan's width: 1 (or none) each side where it was, 0 both in the middle (mono), -1
    /// swapped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spread: Option<f32>,
    /// Binaural: how high round the head (degrees, -90 below to 90 above; none level) and how
    /// far from it (metres, 0.25 to 3; none 1). Its pan is the way round: 0 in front, 0.5 right,
    /// -0.5 left, ±1 behind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elevation: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<f32>,
    /// Its modulators, and what each moves (`modulate.rs`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modulators: Vec<crate::modulate::Modulator>,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub solo: bool,
    /// How tall its lane shows, as dragged (pixels; the usual height without).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
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
    /// A stack's shared modulators: each runs once, and a track in the stack moves its own
    /// settings by one through a `Shared` modulator of its own, its routes its own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modulators: Vec<crate::modulate::Modulator>,
    /// A stack's tracks folded away under it in the arrangement.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub folded: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Automation {
    /// In dB.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volume: Vec<AutoPoint>,
    /// -1 to 1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pan: Vec<AutoPoint>,
    /// 1 muted, 0 not: steps, never slopes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mute: Vec<AutoPoint>,
    /// Send levels (dB), by the bus sent to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sends: Vec<SendAuto>,
    /// Plugin parameters over time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<PluginAuto>,
    /// The EQ's bands' frequencies, gains and widths over time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub eq: Vec<EqAuto>,
    /// Its modulators' settings over time: a macro turned, an LFO's phase.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<ModAuto>,
    #[serde(default, skip_serializing_if = "AutoMode::is_read")]
    pub mode: AutoMode,
}

/// An EQ band's setting over time: the band by its place in the EQ; Hz, dB or Q.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EqAuto {
    pub band: u8,
    pub field: EqField,
    pub points: Vec<AutoPoint>,
}

/// A modulator's setting over time: the modulator by its place on the track.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ModAuto {
    pub modulator: u8,
    pub field: crate::modulate::ModParam,
    pub points: Vec<AutoPoint>,
}

/// What of an EQ band automation moves.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EqField {
    Freq,
    Gain,
    Q,
}

/// A send's level over time.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SendAuto {
    pub bus: u32,
    pub points: Vec<AutoPoint>,
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

/// How a track uses its automation: Read plays it; Touch and Latch play it and
/// also record what's moved while the song plays (Touch until the control is let go, Latch until
/// playback stops); Trim records as Touch does, but moves the line that's there up or down rather
/// than drawing a new one; Write records every automated control from where playback starts, over
/// what was there; Off ignores it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutoMode {
    Off,
    #[default]
    Read,
    Touch,
    Latch,
    Trim,
    Write,
}

impl AutoMode {
    pub const ALL: [AutoMode; 6] = [
        AutoMode::Off,
        AutoMode::Read,
        AutoMode::Touch,
        AutoMode::Latch,
        AutoMode::Trim,
        AutoMode::Write,
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
            AutoMode::Trim => "Trim",
            AutoMode::Write => "Write",
        }
    }

    /// Whether moving a control while playing records it.
    pub fn writes(self) -> bool {
        !matches!(self, AutoMode::Off | AutoMode::Read)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct AutoPoint {
    pub tick: u64,
    pub value: f32,
    /// How the line bends on to the next point: 0 straight; towards 1 it starts slowly and ends
    /// fast, towards -1 the other way round.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub curve: f32,
    /// Holds its value up to the next point, which it jumps to: a step.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub step: bool,
    /// Eases out of its value and into the next: an S, slow at both ends (`curve` aside).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ease: bool,
}

impl AutoPoint {
    pub fn new(tick: u64, value: f32) -> Self {
        AutoPoint {
            tick,
            value,
            ..Default::default()
        }
    }

    /// How far along its change the line on to the next point is, `t` of the way there.
    pub fn along(&self, t: f32) -> f32 {
        match self.ease {
            true => t * t * (3.0 - 2.0 * t),
            false => bend(t, self.curve),
        }
    }

    /// Whether the line on from it is anything but straight.
    pub fn shaped(&self) -> bool {
        self.step || self.ease || self.curve != 0.0
    }
}

/// An automatable parameter: the fader, the pan knob, the mute button, a send's level (by its
/// bus), a plugin's parameter by id, or an EQ band's frequency, gain or width (by its place).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Param {
    #[default]
    Volume,
    Pan,
    Mute,
    Send(u32),
    Plugin(Which, u32),
    Eq(u8, EqField),
    /// A modulator's setting, by the modulator's place on the track.
    Mod(u8, crate::modulate::ModParam),
}

impl Param {
    /// The values it takes, lowest and highest: dB for the fader and sends.
    pub fn range(self) -> (f32, f32) {
        match self {
            Param::Volume | Param::Send(_) => (SILENT_DB, 6.0),
            Param::Pan => (-1.0, 1.0),
            Param::Mute | Param::Plugin(..) => (0.0, 1.0),
            Param::Eq(_, EqField::Freq) => (10.0, 30_000.0),
            Param::Eq(_, EqField::Gain) => (-30.0, 30.0),
            Param::Eq(_, EqField::Q) => (0.025, 40.0),
            Param::Mod(_, p) => p.range(),
        }
    }

    /// Whether its scale is octaves (frequencies, widths, times) rather than steps.
    fn logarithmic(self) -> bool {
        use crate::modulate::ModParam;
        matches!(
            self,
            Param::Eq(_, EqField::Freq | EqField::Q)
                | Param::Mod(_, ModParam::Attack | ModParam::Decay | ModParam::Release)
        )
    }

    /// Whether it only ever jumps from value to value.
    pub fn steps(self) -> bool {
        self == Param::Mute
    }

    /// A value as a share of its range, levels as their faders show them (so silence isn't
    /// a hundred dB away from quiet).
    pub fn to_unit(self, v: f32) -> f32 {
        let (lo, hi) = self.range();
        match self {
            Param::Volume | Param::Send(_) => 10f32.powf((v - 6.0) / 40.0).clamp(0.0, 1.0),
            p if p.logarithmic() => ((v.max(lo) / lo).ln() / (hi / lo).ln()).clamp(0.0, 1.0),
            _ => ((v - lo) / (hi - lo)).clamp(0.0, 1.0),
        }
    }

    pub fn from_unit(self, u: f32) -> f32 {
        let (lo, hi) = self.range();
        match self {
            Param::Volume | Param::Send(_) if u <= 1e-4 => SILENT_DB,
            Param::Volume | Param::Send(_) => (40.0 * u.log10() + 6.0).max(SILENT_DB),
            p if p.logarithmic() => lo * (hi / lo).powf(u.clamp(0.0, 1.0)),
            _ => lo + u.clamp(0.0, 1.0) * (hi - lo),
        }
    }
}

/// The quietest a fader goes: silence.
pub const SILENT_DB: f32 = -144.0;

/// A run of automation lifted out of a track, its ticks from where it was taken, to put down again
/// (copied points, or a region's automation going with it).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct AutoClip {
    pub length: u64,
    pub lanes: Vec<(Param, Vec<AutoPoint>)>,
}

/// A shape drawn into automation, a cycle every so often.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoShape {
    Sine,
    Triangle,
    SawUp,
    SawDown,
    Square,
    Random,
    /// Up from low to high across the whole span, as a fade in; `RampDown` the other way.
    RampUp,
    RampDown,
}

impl AutoShape {
    pub const ALL: [AutoShape; 8] = [
        AutoShape::Sine,
        AutoShape::Triangle,
        AutoShape::SawUp,
        AutoShape::SawDown,
        AutoShape::Square,
        AutoShape::Random,
        AutoShape::RampUp,
        AutoShape::RampDown,
    ];

    pub fn name(self) -> &'static str {
        match self {
            AutoShape::Sine => "Sine",
            AutoShape::Triangle => "Triangle",
            AutoShape::SawUp => "Saw up",
            AutoShape::SawDown => "Saw down",
            AutoShape::Square => "Square",
            AutoShape::Random => "Random",
            AutoShape::RampUp => "Ramp up",
            AutoShape::RampDown => "Ramp down",
        }
    }

    /// Its points from `from` to `to`, a cycle every `every` ticks, between `low` and `high`.
    pub fn points(self, from: u64, to: u64, every: u64, low: f32, high: f32) -> Vec<AutoPoint> {
        let mid = (low + high) / 2.0;
        let every = every.max(1);
        let at = |tick: u64, value: f32, curve: f32, step: bool| AutoPoint {
            tick,
            value,
            curve,
            step,
            ease: false,
        };
        let mut out = Vec::new();
        match self {
            AutoShape::RampUp | AutoShape::RampDown => {
                let (a, b) = match self {
                    AutoShape::RampUp => (low, high),
                    _ => (high, low),
                };
                out.push(at(from, a, 0.0, false));
                out.push(at(to, b, 0.0, false));
                return out;
            }
            _ => {}
        }
        let mut start = from;
        let mut n = 0u64;
        while start < to {
            let q = |k: u64| (start + every * k / 4).min(to);
            match self {
                // A quarter cycle at a time, each bent as a sine's is.
                AutoShape::Sine => {
                    for (k, value, curve) in [
                        (0, mid, -0.3),
                        (1, high, 0.3),
                        (2, mid, -0.3),
                        (3, low, 0.3),
                    ] {
                        if q(k) < to || k == 0 {
                            out.push(at(q(k), value, curve, false));
                        }
                    }
                }
                AutoShape::Triangle => {
                    out.push(at(start, low, 0.0, false));
                    if q(2) < to {
                        out.push(at(q(2), high, 0.0, false));
                    }
                }
                AutoShape::SawUp | AutoShape::SawDown => {
                    let (a, b) = match self {
                        AutoShape::SawUp => (low, high),
                        _ => (high, low),
                    };
                    let end = (start + every).min(to);
                    out.push(at(start, a, 0.0, false));
                    out.push(at(end, b, 0.0, false));
                }
                AutoShape::Square => {
                    out.push(at(start, high, 0.0, true));
                    if q(2) < to {
                        out.push(at(q(2), low, 0.0, true));
                    }
                }
                AutoShape::Random => {
                    let r = crate::modulate::roll(n as i64, 7);
                    out.push(at(start, low + (high - low) * r, 0.0, true));
                }
                AutoShape::RampUp | AutoShape::RampDown => unreachable!(),
            }
            start += every;
            n += 1;
        }
        // It ends where the next cycle would start.
        let last = match self {
            AutoShape::Sine => mid,
            AutoShape::Triangle => low,
            AutoShape::SawUp | AutoShape::SawDown => return out,
            _ => out.last().map_or(mid, |p| p.value),
        };
        if out.last().is_some_and(|p| p.tick < to) {
            out.push(at(to, last, 0.0, false));
        }
        out
    }
}

impl Automation {
    pub fn is_empty(&self) -> bool {
        self.volume.is_empty()
            && self.pan.is_empty()
            && self.mute.is_empty()
            && self.sends.is_empty()
            && self.plugins.is_empty()
            && self.eq.is_empty()
            && self.mods.is_empty()
            && self.mode == AutoMode::Read
    }

    pub fn points(&self, param: Param) -> &[AutoPoint] {
        match param {
            Param::Volume => &self.volume,
            Param::Pan => &self.pan,
            Param::Mute => &self.mute,
            Param::Send(bus) => self
                .sends
                .iter()
                .find(|a| a.bus == bus)
                .map_or(&[], |a| &a.points),
            Param::Plugin(which, id) => self
                .plugins
                .iter()
                .find(|a| a.plugin == which && a.param == id)
                .map_or(&[], |a| &a.points),
            Param::Eq(band, field) => self
                .eq
                .iter()
                .find(|a| a.band == band && a.field == field)
                .map_or(&[], |a| &a.points),
            Param::Mod(k, field) => self
                .mods
                .iter()
                .find(|a| a.modulator == k && a.field == field)
                .map_or(&[], |a| &a.points),
        }
    }

    /// A parameter's points, making it a lane if it has none.
    pub fn points_mut(&mut self, param: Param) -> &mut Vec<AutoPoint> {
        match param {
            Param::Volume => &mut self.volume,
            Param::Pan => &mut self.pan,
            Param::Mute => &mut self.mute,
            Param::Send(bus) => {
                let i = match self.sends.iter().position(|a| a.bus == bus) {
                    Some(i) => i,
                    None => {
                        self.sends.push(SendAuto {
                            bus,
                            points: Vec::new(),
                        });
                        self.sends.len() - 1
                    }
                };
                &mut self.sends[i].points
            }
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
            Param::Eq(band, field) => {
                let i = match self
                    .eq
                    .iter()
                    .position(|a| a.band == band && a.field == field)
                {
                    Some(i) => i,
                    None => {
                        self.eq.push(EqAuto {
                            band,
                            field,
                            points: Vec::new(),
                        });
                        self.eq.len() - 1
                    }
                };
                &mut self.eq[i].points
            }
            Param::Mod(modulator, field) => {
                let i = match self
                    .mods
                    .iter()
                    .position(|a| a.modulator == modulator && a.field == field)
                {
                    Some(i) => i,
                    None => {
                        self.mods.push(ModAuto {
                            modulator,
                            field,
                            points: Vec::new(),
                        });
                        self.mods.len() - 1
                    }
                };
                &mut self.mods[i].points
            }
        }
    }

    /// A modulator taken off the track: its lanes go, and those of the ones after it follow
    /// it down a place.
    pub fn modulator_removed(&mut self, k: usize) {
        self.mods.retain(|a| a.modulator as usize != k);
        for a in &mut self.mods {
            if a.modulator as usize > k {
                a.modulator -= 1;
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

    /// Drops plugin, send, EQ and modulator lanes left without points.
    pub fn tidy(&mut self) {
        self.plugins.retain(|a| !a.points.is_empty());
        self.sends.retain(|a| !a.points.is_empty());
        self.eq.retain(|a| !a.points.is_empty());
        self.mods.retain(|a| !a.points.is_empty());
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
        if !self.mute.is_empty() {
            out.push(Param::Mute);
        }
        out.extend(
            self.sends
                .iter()
                .filter(|a| !a.points.is_empty())
                .map(|a| Param::Send(a.bus)),
        );
        out.extend(
            self.plugins
                .iter()
                .filter(|a| !a.points.is_empty())
                .map(|a| Param::Plugin(a.plugin, a.param)),
        );
        out.extend(
            self.eq
                .iter()
                .filter(|a| !a.points.is_empty())
                .map(|a| Param::Eq(a.band, a.field)),
        );
        out.extend(
            self.mods
                .iter()
                .filter(|a| !a.points.is_empty())
                .map(|a| Param::Mod(a.modulator, a.field)),
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
                            step: param.steps(),
                            ..AutoPoint::new(before, old)
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
            points.insert(
                n,
                AutoPoint {
                    step: param.steps(),
                    ..AutoPoint::new(tick, value)
                },
            );
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
            ..points[i]
        };
    }

    /// Puts `new` in place of the line from `from` to `to`, keeping it as it was either side
    /// (where it had points): it jumps into the new and back out.
    pub fn replace(&mut self, param: Param, from: u64, to: u64, new: Vec<AutoPoint>) {
        let points = self.points_mut(param);
        let had = !points.is_empty();
        let before = value_at(points, from as f64 - 0.5);
        let after = value_at(points, to as f64);
        let stepped = |i: usize, points: &Vec<AutoPoint>| i > 0 && points[i - 1].step;
        let i = points.partition_point(|p| p.tick < from);
        let j = points.partition_point(|p| p.tick <= to);
        // A step that ran into the span still holds up to it.
        let held = stepped(i, points);
        let mut put: Vec<AutoPoint> = Vec::new();
        if had && from > 0 {
            put.push(AutoPoint {
                step: held || param.steps(),
                ..AutoPoint::new(from, before.unwrap_or(0.0))
            });
        }
        put.extend(
            new.into_iter()
                .filter(|p| (from..=to).contains(&p.tick))
                .map(|p| AutoPoint {
                    step: p.step || param.steps(),
                    ..p
                }),
        );
        if had && let Some(after) = after {
            put.push(AutoPoint {
                step: param.steps() || points.get(j.saturating_sub(1)).is_some_and(|p| p.step),
                ..AutoPoint::new(to, after)
            });
        }
        points.splice(i..j, put);
        self.tidy();
    }

    /// Draws a shape from `from` to `to`, a cycle every `every` ticks, over what was there.
    #[allow(clippy::too_many_arguments)]
    pub fn shape(
        &mut self,
        param: Param,
        shape: AutoShape,
        from: u64,
        to: u64,
        every: u64,
        low: f32,
        high: f32,
    ) {
        let (lo, hi) = param.range();
        let (low, high) = (low.clamp(lo, hi), high.clamp(lo, hi));
        self.replace(param, from, to, shape.points(from, to, every, low, high));
    }

    /// Moves the whole line up or down by `by` (dB for the fader and sends), each point within
    /// the parameter's range.
    pub fn offset(&mut self, param: Param, by: f32) {
        let (lo, hi) = param.range();
        for p in self.points_mut(param) {
            p.value = (p.value + by).clamp(lo, hi);
        }
    }

    /// Takes out points that change nothing: each within `tolerance` (a share of the
    /// parameter's range) of the straight line its neighbours would draw without it. Steps and
    /// curves stay.
    pub fn thin(&mut self, param: Param, tolerance: f32) {
        let (lo, hi) = param.range();
        // The fader's range is mostly silence: judge it by the top 60 dB.
        let span = match param {
            Param::Volume | Param::Send(_) => 66.0,
            // On an octave scale a straight line in Hz isn't one: keep every point.
            p if p.logarithmic() => 0.0,
            _ => hi - lo,
        };
        let tol = tolerance * span;
        let points = self.points_mut(param);
        let n = points.len();
        if n < 3 {
            return;
        }
        let mut keep = vec![true; n];
        // Douglas-Peucker over each run of straight segments.
        let fixed = |i: usize| {
            i == 0
                || i == n - 1
                || points[i].shaped()
                || points[i - 1].shaped()
                || points[i].tick == points[i - 1].tick
                || points[i + 1].tick == points[i].tick
        };
        let mut a = 0;
        for b in 1..n {
            if fixed(b) {
                simplify(points, a, b, tol, &mut keep);
                a = b;
            }
        }
        let mut k = 0;
        points.retain(|_| {
            k += 1;
            keep[k - 1]
        });
    }

    /// Lifts out every lane's points from `from` to `to` (as they sound: with the line's values
    /// at either end), their ticks from `from`.
    pub fn take(&self, from: u64, to: u64) -> AutoClip {
        let mut lanes = Vec::new();
        for param in self.params() {
            let points = self.points(param);
            let mut out: Vec<AutoPoint> = Vec::new();
            if let Some(v) = value_at(points, from as f64) {
                out.push(AutoPoint {
                    step: param.steps()
                        || points
                            .iter()
                            .rev()
                            .find(|p| p.tick <= from)
                            .is_some_and(|p| p.step),
                    ..AutoPoint::new(0, v)
                });
            }
            out.extend(
                points
                    .iter()
                    .filter(|p| p.tick > from && p.tick < to)
                    .map(|p| AutoPoint {
                        tick: p.tick - from,
                        ..*p
                    }),
            );
            if let Some(v) = value_at(points, to as f64 - 0.5) {
                out.push(AutoPoint::new(to - from, v));
            }
            lanes.push((param, out));
        }
        AutoClip {
            length: to.saturating_sub(from),
            lanes,
        }
    }

    /// Puts a lifted run down at `at`, over what was there for its length, lane by lane.
    pub fn put(&mut self, at: u64, clip: &AutoClip) {
        for (param, points) in &clip.lanes {
            let shifted = points
                .iter()
                .map(|p| AutoPoint {
                    tick: p.tick + at,
                    ..*p
                })
                .collect();
            self.replace(*param, at, at + clip.length, shifted);
        }
    }

    /// Smooths the line from `from` to `to`: blurred over about `width` ticks either side (on
    /// the fader's scale for levels), its ends left where they were so it joins the line either
    /// side, then as few points as keep its shape. Steps and bends inside go smooth too; mute,
    /// all steps, stays as it is.
    pub fn smooth(&mut self, param: Param, from: u64, to: u64, width: u64) {
        let points = self.points(param);
        if param.steps() || to <= from || points.len() < 2 {
            return;
        }
        let width = width.max(1) as f64;
        let n = (((to - from) as f64 / (width / 4.0).clamp(10.0, PPQ as f64 / 16.0)).ceil()
            as usize)
            .max(2);
        let step = (to - from) as f64 / n as f64;
        let reach = (3.0 * width / step).ceil() as i64;
        // The line as it is, sampled out past both ends, as a share of its range.
        let line: Vec<f32> = (-reach..=n as i64 + reach)
            .map(|k| {
                let t = (from as f64 + k as f64 * step).max(0.0);
                param.to_unit(value_at(points, t).unwrap_or(0.0))
            })
            .collect();
        let kernel: Vec<f32> = (-reach..=reach)
            .map(|j| (-(j as f64 * step / width).powi(2) / 2.0).exp() as f32)
            .collect();
        let total: f32 = kernel.iter().sum();
        let mut blurred: Vec<f32> = (0..=n)
            .map(|k| {
                let around = &line[k..k + kernel.len()];
                around.iter().zip(&kernel).map(|(v, g)| v * g).sum::<f32>() / total
            })
            .collect();
        // Its ends pinned where they were.
        let r = reach as usize;
        let (d0, d1) = (line[r] - blurred[0], line[r + n] - blurred[n]);
        for (k, v) in blurred.iter_mut().enumerate() {
            let u = k as f32 / n as f32;
            *v = (*v + d0 * (1.0 - u) + d1 * u).clamp(0.0, 1.0);
        }
        let sampled: Vec<AutoPoint> = blurred
            .iter()
            .enumerate()
            .map(|(k, &u)| AutoPoint::new(from + (k as f64 * step).round() as u64, u))
            .collect();
        let mut keep = vec![true; sampled.len()];
        simplify(&sampled, 0, sampled.len() - 1, 0.004, &mut keep);
        let ends = (value_at(points, from as f64), value_at(points, to as f64));
        let mut smooth: Vec<AutoPoint> = sampled
            .iter()
            .zip(&keep)
            .filter(|(_, k)| **k)
            .map(|(p, _)| AutoPoint::new(p.tick, param.from_unit(p.value)))
            .collect();
        // Exactly where they were, not as near as a round trip through the fader's scale.
        if let (Some(a), Some(b)) = ends {
            let last = smooth.len() - 1;
            (smooth[0].value, smooth[last].value) = (a, b);
        }
        let points = self.points_mut(param);
        let i = points.partition_point(|p| p.tick < from);
        let j = points.partition_point(|p| p.tick <= to);
        points.splice(i..j, smooth);
    }

    /// Takes out every lane's points from `from` to `to`, the line either side kept as it was.
    pub fn erase(&mut self, from: u64, to: u64) {
        for param in self.params() {
            let points = self.points(param);
            let (a, b) = (
                value_at(points, from as f64 - 0.5),
                value_at(points, to as f64),
            );
            let mut put = Vec::new();
            if let Some(a) = a.filter(|_| from > 0) {
                put.push(AutoPoint::new(from, a));
            }
            if let Some(b) = b {
                put.push(AutoPoint::new(to, b));
            }
            let points = self.points_mut(param);
            points.retain(|p| p.tick < from || p.tick > to);
            let i = points.partition_point(|p| p.tick < from);
            points.splice(i..i, put);
        }
        self.tidy();
    }
}

/// Keeps the points between `a` and `b` that bend the line more than `tol` (Douglas-Peucker).
fn simplify(points: &[AutoPoint], a: usize, b: usize, tol: f32, keep: &mut [bool]) {
    if b <= a + 1 {
        return;
    }
    let (pa, pb) = (points[a], points[b]);
    let line = |t: u64| {
        let f = (t - pa.tick) as f32 / (pb.tick - pa.tick).max(1) as f32;
        pa.value + (pb.value - pa.value) * f
    };
    let (far, off) = (a + 1..b)
        .map(|i| (i, (points[i].value - line(points[i].tick)).abs()))
        .fold((a, 0.0), |m, x| if x.1 > m.1 { x } else { m });
    if off > tol {
        simplify(points, a, far, tol, keep);
        simplify(points, far, b, tol, keep);
    } else {
        keep[a + 1..b].iter_mut().for_each(|k| *k = false);
    }
}

/// How far along a segment's change a bent line is, `t` of the way along it.
pub fn bend(t: f32, curve: f32) -> f32 {
    if curve.abs() < 1e-3 {
        return t;
    }
    let k = curve.clamp(-1.0, 1.0) * 6.0;
    ((k * t).exp() - 1.0) / (k.exp() - 1.0)
}

/// A parameter's value at a tick: lines (straight, bent or eased by the point they start from)
/// between points, steps after a stepped one, level before the first and after the last. None
/// without points.
pub fn value_at(points: &[AutoPoint], tick: f64) -> Option<f32> {
    let i = points.partition_point(|p| (p.tick as f64) <= tick);
    match (i.checked_sub(1).map(|i| points[i]), points.get(i)) {
        (None, None) => None,
        (Some(a), None) => Some(a.value),
        (None, Some(b)) => Some(b.value),
        (Some(a), Some(_)) if a.step => Some(a.value),
        (Some(a), Some(b)) => {
            let t = ((tick - a.tick as f64) / (b.tick - a.tick) as f64) as f32;
            Some(a.value + (b.value - a.value) * a.along(t))
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

/// The built-in sampler: a sample on each note, as a drum
/// kit; one sample played across a range of notes, repitched, as an 808 or a bass; a loop sliced
/// across the keys; or layers by velocity and round robins. How every voice plays (envelopes,
/// filter, LFO, mono and glide) is the instrument's.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Sampler {
    pub zones: Vec<Zone>,
    #[serde(default, skip_serializing_if = "SamplerVoice::is_default")]
    pub voice: SamplerVoice,
}

/// What a sampler's filter does.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FilterMode {
    #[default]
    Off,
    LowPass,
    HighPass,
    BandPass,
}

/// How a zone's sample loops while its note is held.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LoopMode {
    #[default]
    Off,
    Forward,
    /// Forward, then back.
    PingPong,
}

/// How a sampler's voices play.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct SamplerVoice {
    /// The amp envelope: attack and decay (ms), the level held (0 to 1), and release (ms; a
    /// zone's own release, if it has one, instead).
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
    pub filter: FilterMode,
    pub cutoff_hz: f32,
    /// 0 to 1: a ringing peak at the cutoff as it rises.
    pub resonance: f32,
    /// How far the filter envelope moves the cutoff, in octaves (-6 to 6), and its shape.
    pub filter_env: f32,
    pub filter_attack_ms: f32,
    pub filter_decay_ms: f32,
    pub filter_sustain: f32,
    pub filter_release_ms: f32,
    /// How much the cutoff follows the note played: 1 an octave an octave.
    pub key_track: f32,
    /// How much velocity changes the level: 0 not at all, 1 as a drum does.
    pub velocity: f32,
    /// One note at a time; legato, a note played over another glides to it without starting
    /// over; glide, how long that takes (ms), mono or not.
    pub mono: bool,
    pub legato: bool,
    pub glide_ms: f32,
    /// The LFO: its rate (Hz), and how far it moves pitch (cents), the cutoff (octaves) and
    /// level (0 to 1).
    pub lfo_hz: f32,
    pub vibrato_cents: f32,
    pub lfo_filter: f32,
    pub tremolo: f32,
    /// Semitones, and dB, over every zone's own.
    pub transpose: f32,
    pub gain_db: f32,
}

impl Default for SamplerVoice {
    fn default() -> Self {
        SamplerVoice {
            attack_ms: 0.0,
            decay_ms: 0.0,
            sustain: 1.0,
            release_ms: 60.0,
            filter: FilterMode::Off,
            cutoff_hz: 18_000.0,
            resonance: 0.1,
            filter_env: 0.0,
            filter_attack_ms: 0.0,
            filter_decay_ms: 400.0,
            filter_sustain: 0.0,
            filter_release_ms: 200.0,
            key_track: 0.0,
            velocity: 1.0,
            mono: false,
            legato: false,
            glide_ms: 0.0,
            lfo_hz: 5.0,
            vibrato_cents: 0.0,
            lfo_filter: 0.0,
            tremolo: 0.0,
            transpose: 0.0,
            gain_db: 0.0,
        }
    }
}

impl SamplerVoice {
    pub fn is_default(&self) -> bool {
        *self == SamplerVoice::default()
    }
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
    /// Where in the sample it plays from and to, as shares of its length (0 to 1): a slice of a
    /// loop, or a sound trimmed. All of it by default.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub start: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f32>,
    /// Looping while held, between these shares of the sample (its play range by default).
    #[serde(default, skip_serializing_if = "is_loop_off")]
    pub loop_mode: LoopMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_start: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_end: Option<f32>,
    /// Played backwards.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reverse: bool,
    /// The velocities it plays for (1 to 127): layers, a soft hit and a hard one. Zones that
    /// share notes and velocities take turns, as round robins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub velocities: Option<(u8, u8)>,
}

fn is_loop_off(m: &LoopMode) -> bool {
    *m == LoopMode::Off
}

impl Zone {
    /// Whether it plays a note.
    pub fn covers(&self, note: u8) -> bool {
        (self.note..=self.high.unwrap_or(self.note)).contains(&note)
    }

    /// Whether it plays a note at a velocity (1 to 127).
    pub fn takes(&self, note: u8, velocity: u8) -> bool {
        let (lo, hi) = self.velocities.unwrap_or((1, 127));
        self.covers(note) && (lo..=hi).contains(&velocity.max(1))
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
    /// The preset Pina Studio last loaded into it: loaded over its settings, and undone like any edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<Preset>,
    /// Parameters set from Pina Studio (its Smart Controls, or an agent), by id, 0 to 1, in the order
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
    /// Where its loops stop, in ticks from its start, as the loop tool drags them out (the
    /// next region still stops them); none runs them on to the next region or the song's end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_length: Option<u64>,
    /// Silent, kept where it is.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mute: bool,
    /// An audio region's fades, in and out, in ticks from its ends (an equal-power curve).
    #[serde(default, skip_serializing_if = "is_none_u64")]
    pub fade_in: u64,
    #[serde(default, skip_serializing_if = "is_none_u64")]
    pub fade_out: u64,
    /// Playback settings, as the region inspector shows them: MIDI plays moved by `transpose`
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

fn is_none_u64(v: &u64) -> bool {
    *v == 0
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
        /// How it's stretched: Polyphonic, Slicing or Speed (see `crate::stretch`).
        #[serde(default, skip_serializing_if = "Flex::is_default")]
        flex: Flex,
        /// It follows the song's tempo, as a loop from the browser does: a tempo change
        /// stretches it, so its beats stay on the song's.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        follow: bool,
        /// Moved in pitch, its formants kept, as a voice wants.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        formants: bool,
        /// Warp markers: [a frame of the file, where it plays in the
        /// stretched audio], both at the project's rate. The audio between is fitted to them.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        warps: Vec<[u64; 2]>,
    },
    /// A step pattern, as the step sequencer makes: rows of steps, looped over the region.
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct PatternRow {
    pub pitch: u8,
    /// A velocity per step; 0 is a rest. A row shorter than the others loops on its own length
    /// (a polymeter: three steps against sixteen).
    pub cells: Vec<u8>,
    /// Silent, its steps kept.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mute: bool,
    /// Each step's own settings (parameter locks): the
    /// chance it plays (0 to 100), semitones up or down, how many times it hits in its step (a
    /// ratchet, 1 to 4), how long it holds (% of a step) and how far off the grid it is (% of a
    /// step, -50 to 50). Missing steps (or an empty lane) are as usual: 100, 0, 1, 75, 0.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chance: Vec<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transpose: Vec<i8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repeat: Vec<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gate: Vec<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nudge: Vec<i8>,
}

/// A step's lanes, as usual when it has none.
pub const CHANCE: u8 = 100;
pub const GATE: u8 = 75;

impl PatternRow {
    pub fn chance_at(&self, i: usize) -> u8 {
        self.chance.get(i).copied().unwrap_or(CHANCE).min(100)
    }
    pub fn transpose_at(&self, i: usize) -> i8 {
        self.transpose.get(i).copied().unwrap_or(0)
    }
    pub fn repeat_at(&self, i: usize) -> u8 {
        self.repeat.get(i).copied().unwrap_or(1).clamp(1, 4)
    }
    pub fn gate_at(&self, i: usize) -> u8 {
        self.gate.get(i).copied().unwrap_or(GATE).clamp(5, 200)
    }
    pub fn nudge_at(&self, i: usize) -> i8 {
        self.nudge.get(i).copied().unwrap_or(0).clamp(-50, 50)
    }

    /// Every lane as long as its steps (filled as usual), to edit together.
    pub fn fill_lanes(&mut self) {
        let n = self.cells.len();
        let fill = |v: &mut Vec<u8>, d: u8| v.resize(n, d);
        fill(&mut self.chance, CHANCE);
        fill(&mut self.repeat, 1);
        fill(&mut self.gate, GATE);
        self.transpose.resize(n, 0);
        self.nudge.resize(n, 0);
    }

    /// Lanes as usual throughout taken away, so the song keeps only what's set.
    pub fn tidy_lanes(&mut self) {
        let n = self.cells.len();
        let all = |v: &[u8], d: u8| v.iter().take(n).all(|&x| x == d);
        if all(&self.chance, CHANCE) {
            self.chance.clear();
        }
        if all(&self.repeat, 1) {
            self.repeat.clear();
        }
        if all(&self.gate, GATE) {
            self.gate.clear();
        }
        if self.transpose.iter().all(|&x| x == 0) {
            self.transpose.clear();
        }
        if self.nudge.iter().all(|&x| x == 0) {
            self.nudge.clear();
        }
    }
}

/// A MIDI region's notes as a pattern's rows: a row a pitch (lowest last, as drums read: kick at
/// the bottom), each note on the step nearest its start, `steps` long, at its velocity.
pub fn to_pattern(notes: &[Note], step: u64, steps: usize) -> Vec<PatternRow> {
    let mut pitches: Vec<u8> = notes.iter().map(|n| n.pitch).collect();
    pitches.sort_unstable_by(|a, b| b.cmp(a));
    pitches.dedup();
    pitches
        .into_iter()
        .map(|pitch| {
            let mut cells = vec![0u8; steps];
            for n in notes.iter().filter(|n| n.pitch == pitch && n.start >= 0) {
                let k = ((n.start as f64 / step.max(1) as f64).round() as usize) % steps.max(1);
                cells[k] = cells[k].max(n.velocity.max(1));
            }
            PatternRow {
                pitch,
                cells,
                mute: false,
                ..Default::default()
            }
        })
        .collect()
}

/// The notes a pattern plays over `length` ticks, each row's steps repeating, every second step
/// late by its swing.
pub fn pattern_notes(step: u64, rows: &[PatternRow], length: u64, swing: f32) -> Vec<Note> {
    pattern_played(step, rows, length, swing, 0, &[], 120.0)
}

/// A pattern's notes as they play from `start` (ticks into the song): each step's lanes (its
/// chance, transpose, ratchet, gate and nudge), and what the track's modulators do to each sound
/// (its velocity, pitch, chance), worked out where each step falls in the song. A step's chance
/// is rolled afresh each time round, the same each time the song plays.
pub fn pattern_played(
    step: u64,
    rows: &[PatternRow],
    length: u64,
    swing: f32,
    start: u64,
    mods: &[crate::modulate::Modulator],
    tempo: f64,
) -> Vec<Note> {
    use crate::modulate::{ModTarget, SoundParam};
    // Swing is where in a pair of steps the second lands: at 50% exactly between.
    let late = ((swing.clamp(50.0, 75.0) - 50.0) / 50.0 * step as f32) as i64;
    let mut notes = Vec::new();
    if step == 0 {
        return notes;
    }
    let first = (start / step) as i64;
    for k in 0..length.div_ceil(step) {
        for (r, row) in rows.iter().enumerate() {
            if row.mute || row.cells.is_empty() {
                continue;
            }
            let i = (k % row.cells.len() as u64) as usize;
            let velocity = row.cells[i];
            if velocity == 0 {
                continue;
            }
            // What the modulators on this sound make of this step.
            let (mut dv, mut dp, mut dc) = (0f32, 0f32, 0f32);
            let beats = (start + k * step) as f64 / PPQ as f64;
            for m in mods {
                for route in &m.routes {
                    if let ModTarget::Sound(on, param) = route.target
                        && on == r
                        && let Some(v) = crate::modulate::value_at(&m.kind, beats, tempo)
                    {
                        let by = route.depth * v;
                        match param {
                            SoundParam::Velocity => dv += by,
                            SoundParam::Pitch => dp += by,
                            SoundParam::Chance => dc += by,
                        }
                    }
                }
            }
            let chance = (row.chance_at(i) as f32 + dc).clamp(0.0, 100.0);
            if chance < 100.0 && crate::modulate::roll(first + k as i64, r) >= chance {
                continue;
            }
            let velocity = (velocity as f32 + dv).round().clamp(1.0, 127.0) as u8;
            let pitch = (row.pitch as i32 + row.transpose_at(i) as i32 + dp.round() as i32)
                .clamp(0, 127) as u8;
            let repeat = row.repeat_at(i) as u64;
            let sub = (step / repeat).max(1);
            let held = ((sub as f32 * row.gate_at(i) as f32 / 100.0).round() as u64).max(1);
            let swung = if k % 2 == 1 { late } else { 0 };
            let nudge = (step as f32 * row.nudge_at(i) as f32 / 100.0).round() as i64;
            for j in 0..repeat {
                notes.push(Note {
                    start: (k * step + j * sub) as i64 + swung + nudge,
                    length: held,
                    pitch,
                    velocity,
                });
            }
        }
    }
    notes.sort_by_key(|n| n.start);
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

    /// Its notes (and controllers) or steps, played `ratio` times as long: a MIDI or pattern
    /// region time-stretched, as ⌥-dragging a region's end does. Audio flexes instead.
    pub fn stretched(&self, ratio: f64) -> Option<Content> {
        let at = |t: i64| (t as f64 * ratio).round() as i64;
        match self {
            Content::Midi { notes, controls } => Some(Content::Midi {
                notes: notes
                    .iter()
                    .map(|n| Note {
                        start: at(n.start),
                        length: ((n.length as f64 * ratio).round() as u64).max(1),
                        ..*n
                    })
                    .collect(),
                controls: controls
                    .iter()
                    .map(|c| ControlPoint {
                        start: at(c.start),
                        ..*c
                    })
                    .collect(),
            }),
            Content::Pattern { step, rows, swing } => Some(Content::Pattern {
                step: ((*step as f64 * ratio).round() as u64).max(1),
                rows: rows.clone(),
                swing: *swing,
            }),
            Content::Audio { .. } | Content::Takes { .. } => None,
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
    /// Notes play their full length, even past the region's end.
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

    /// Adds an aux on the next free bus, as a send picking a new bus makes; returns the
    /// bus.
    /// Makes a summing stack of these tracks: a stack channel they all play into, and the tracks
    /// moved together under the first of them. Returns its bus, and where each
    /// track is now, by where it was.
    pub fn add_stack(&mut self, tracks: &[usize]) -> (u32, Vec<usize>) {
        let bus = self.add_aux();
        let n = self.auxes.iter().filter(|a| a.stack).count() + 1;
        let stack = self.auxes.last_mut().unwrap();
        stack.stack = true;
        stack.name = format!("Stack {n}");
        for &t in tracks {
            self.tracks[t].output = Some(bus);
        }
        (bus, self.gather(bus))
    }

    /// Moves the tracks playing into `bus` together, under the first of them, in their order;
    /// returns where each track is now, by where it was.
    pub fn gather(&mut self, bus: u32) -> Vec<usize> {
        let n = self.tracks.len();
        let members: Vec<usize> = (0..n)
            .filter(|&i| self.tracks[i].output == Some(bus))
            .collect();
        let order: Vec<usize> = (0..n)
            .flat_map(|i| match members.first() {
                Some(&first) if i == first => members.clone(),
                _ if members.contains(&i) => Vec::new(),
                _ => vec![i],
            })
            .collect();
        let mut old: Vec<Option<Track>> = std::mem::take(&mut self.tracks)
            .into_iter()
            .map(Some)
            .collect();
        self.tracks = order.iter().map(|&i| old[i].take().unwrap()).collect();
        let mut moved = vec![0; n];
        for (to, &from) in order.iter().enumerate() {
            moved[from] = to;
        }
        moved
    }

    /// Undoes a stack (Flatten Stack): its tracks play into the master again, and the
    /// stack goes, with the modulators it shared.
    pub fn flatten_stack(&mut self, bus: u32) {
        while self
            .auxes
            .iter()
            .any(|a| a.bus == bus && !a.modulators.is_empty())
        {
            self.remove_shared(bus, 0);
        }
        for t in &mut self.tracks {
            if t.output == Some(bus) {
                t.output = None;
            }
            t.sends.retain(|s| s.bus != bus);
        }
        self.auxes.retain(|a| a.bus != bus);
    }

    /// The stack a track plays into, if it plays into one.
    pub fn stack_of(&self, t: usize) -> Option<&Aux> {
        let bus = self.tracks.get(t)?.output?;
        self.auxes.iter().find(|a| a.bus == bus && a.stack)
    }

    /// A track's modulator's kind; for one its stack shares, the stack's (none if the track has
    /// left the stack, or the stack the modulator).
    pub fn mod_kind(&self, t: usize, k: usize) -> Option<&ModKind> {
        match &self.tracks.get(t)?.modulators.get(k)?.kind {
            ModKind::Shared { modulator } => {
                Some(&self.stack_of(t)?.modulators.get(*modulator as usize)?.kind)
            }
            kind => Some(kind),
        }
    }

    pub fn mod_kind_mut(&mut self, t: usize, k: usize) -> Option<&mut ModKind> {
        let shared = match self.tracks.get(t)?.modulators.get(k)?.kind {
            ModKind::Shared { modulator } => Some(modulator as usize),
            _ => None,
        };
        match shared {
            Some(s) => {
                let bus = self.stack_of(t)?.bus;
                let aux = self.auxes.iter_mut().find(|a| a.bus == bus)?;
                Some(&mut aux.modulators.get_mut(s)?.kind)
            }
            None => Some(&mut self.tracks[t].modulators[k].kind),
        }
    }

    /// A track's modulators as they move what's worked out ahead (a pattern's sounds): any its
    /// stack shares as the stack's, with the track's own routes.
    pub fn mods_of(&self, t: usize) -> Vec<Modulator> {
        let Some(track) = self.tracks.get(t) else {
            return Vec::new();
        };
        (0..track.modulators.len())
            .map(|k| Modulator {
                kind: self
                    .mod_kind(t, k)
                    .unwrap_or(&track.modulators[k].kind)
                    .clone(),
                routes: track.modulators[k].routes.clone(),
            })
            .collect()
    }

    /// A modulator shared by the stack on `bus`, which every track in it gets (to route as it
    /// likes); returns its place on the stack.
    pub fn add_shared(&mut self, bus: u32, kind: ModKind) -> Option<usize> {
        let aux = self.auxes.iter_mut().find(|a| a.bus == bus && a.stack)?;
        if aux.modulators.len() >= MAX_MODS {
            return None;
        }
        aux.modulators.push(Modulator {
            kind,
            routes: Vec::new(),
        });
        let s = aux.modulators.len() - 1;
        for t in self.tracks.iter_mut().filter(|t| t.output == Some(bus)) {
            if t.modulators.len() < MAX_MODS {
                t.modulators.push(Modulator {
                    kind: ModKind::Shared { modulator: s as u8 },
                    routes: Vec::new(),
                });
            }
        }
        Some(s)
    }

    /// Takes a shared modulator off its stack, and off every track sharing it (their lanes for it
    /// too); those after it move down a place.
    pub fn remove_shared(&mut self, bus: u32, s: usize) {
        let Some(aux) = self.auxes.iter_mut().find(|a| a.bus == bus) else {
            return;
        };
        if s >= aux.modulators.len() {
            return;
        }
        aux.modulators.remove(s);
        for t in self.tracks.iter_mut().filter(|t| t.output == Some(bus)) {
            let mut k = 0;
            while k < t.modulators.len() {
                match &mut t.modulators[k].kind {
                    ModKind::Shared { modulator } if *modulator as usize == s => {
                        t.modulators.remove(k);
                        t.automation.modulator_removed(k);
                        continue;
                    }
                    ModKind::Shared { modulator } if *modulator as usize > s => *modulator -= 1,
                    _ => {}
                }
                k += 1;
            }
        }
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
    /// pitch kept, as option-dragging its end does. From a quarter to four times as long
    /// as it was recorded.
    pub fn flex(&mut self, at: At, length: u64) -> Result<(), String> {
        let r = &mut self.tracks[at.0].regions[at.1];
        let old = r.length;
        // MIDI and patterns: their notes or steps, longer or shorter; an alias its own then.
        if let Some(c) = r
            .content
            .stretched(length.max(1) as f64 / old.max(1) as f64)
        {
            r.content = c;
            r.length = length.max(1);
            r.link = None;
            return Ok(());
        }
        let Content::Audio { stretch, warps, .. } = &mut r.content else {
            return Err("a take folder doesn't stretch: flatten it first".into());
        };
        // The stretch that fills `length`, within limits, and the length that gives; warp
        // markers stretch with it.
        let wanted = *stretch * length.max(1) as f64 / old as f64;
        let ratio = wanted.clamp(0.25, 4.0);
        for w in warps.iter_mut() {
            w[1] = (w[1] as f64 * ratio / *stretch).round() as u64;
        }
        *stretch = ratio;
        r.length = (length.max(1) as f64 * ratio / wanted).round() as u64;
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
                        offset, stretch, ..
                    },
                    true,
                ) = (&take.content, from < b as i64)
                {
                    let skip = self.samples((from - take.start) as u64, rate);
                    let mut content = take.content.clone();
                    if let Content::Audio { offset: o, .. } = &mut content {
                        *o = offset + (skip as f64 / stretch) as u64;
                    }
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
        // Its loops stop where they did.
        if let Some(l) = &mut r.loop_length {
            *l = (old + *l).saturating_sub(start);
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
    /// Notes stay with the half they start in.
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

    #[test]
    fn a_patterns_lanes_and_the_modulators_on_its_sounds() {
        use crate::modulate::{
            LfoShape, ModKind, ModRate, ModRoute, ModTarget, Modulator, SoundParam,
        };
        let step = PPQ / 4;
        let row = |cells: Vec<u8>| PatternRow {
            pitch: 36,
            cells,
            ..Default::default()
        };
        // A ratchet of three, a fifth up, held half its time, a quarter of a step late.
        let mut r = row(vec![100, 0, 0, 0]);
        (r.repeat, r.transpose, r.gate, r.nudge) = (vec![3], vec![7], vec![50], vec![25]);
        let notes = pattern_notes(step, &[r], step * 4, 50.0);
        let starts: Vec<i64> = notes.iter().map(|n| n.start).collect();
        let sub = (step / 3) as i64;
        let late = (step / 4) as i64;
        assert_eq!(starts, [late, sub + late, 2 * sub + late]);
        assert!(
            notes
                .iter()
                .all(|n| n.pitch == 43 && n.length == (sub as u64) / 2)
        );
        // Half a chance: some steps of a long run play, some don't, the same each time.
        let mut r = row(vec![100; 64]);
        r.chance = vec![50; 64];
        let played = pattern_notes(step, std::slice::from_ref(&r), step * 64, 50.0);
        assert!((20..44).contains(&played.len()), "{}", played.len());
        assert_eq!(played, pattern_notes(step, &[r], step * 64, 50.0));
        // A square LFO a beat long on the sound's velocity: loud for the first half of each beat.
        let lfo = Modulator {
            kind: ModKind::Lfo {
                shape: LfoShape::Square,
                rate: ModRate::Beats(1.0),
                phase: 0.0,
                unipolar: false,
            },
            routes: vec![ModRoute {
                target: ModTarget::Sound(0, SoundParam::Velocity),
                depth: -40.0,
            }],
        };
        let notes = pattern_played(step, &[row(vec![100; 4])], step * 4, 50.0, 0, &[lfo], 120.0);
        let v: Vec<u8> = notes.iter().map(|n| n.velocity).collect();
        assert_eq!(v, [60, 60, 127, 127]);
    }

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

    #[test]
    fn audio_that_follows_the_tempo_keeps_its_beats() {
        let mut p = Project::new(48_000);
        p.tempo = 100.0;
        // A loop at 90 stretched to 100 BPM, following; a recording that doesn't.
        let mut follows = audio(0, PPQ * 4, 0);
        if let Content::Audio {
            stretch, follow, ..
        } = &mut follows.content
        {
            (*stretch, *follow) = (0.9, true);
        }
        p.tracks.push(Track {
            regions: vec![follows, audio(0, PPQ * 4, 0)],
            ..Default::default()
        });
        let stretch = |p: &Project, r: usize| match p.tracks[0].regions[r].content {
            Content::Audio { stretch, .. } => stretch,
            _ => unreachable!(),
        };
        if let Content::Audio { warps, .. } = &mut p.tracks[0].regions[0].content {
            warps.push([1000, 900]);
        }
        p.set_tempo(120.0);
        assert_eq!((stretch(&p, 0), stretch(&p, 1)), (0.75, 1.0));
        // Its warp markers move with it: a beat still lands on its beat.
        let warp = match &p.tracks[0].regions[0].content {
            Content::Audio { warps, .. } => warps[0],
            _ => unreachable!(),
        };
        assert_eq!(warp, [1000, 750]);
        assert_eq!(p.tracks[0].regions[0].length, PPQ * 4);
        // Back where it was, exactly.
        p.set_tempo(100.0);
        assert_eq!(stretch(&p, 0), 0.9);
    }

    fn audio(start: u64, length: u64, offset: u64) -> Region {
        Region {
            start,
            length,
            content: Content::Audio {
                file: "a.wav".into(),
                offset,
                stretch: 1.0,
                flex: Flex::default(),
                follow: false,
                formants: false,
                warps: Vec::new(),
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
    fn a_follower_follows_its_track_after_the_song_is_saved_and_read() {
        let mut p: Project = serde_json::from_str(
            r#"{"tempo": 120, "sample_rate": 48000,
                "tracks": [{"name": "Kick"}, {"name": "Bass"}]}"#,
        )
        .unwrap();
        for _ in 0..5 {
            next_id();
        }
        p.assign_ids();
        let kick = p.tracks[0].id;
        p.tracks[1].modulators.push(Modulator {
            kind: ModKind::follower(kick),
            routes: vec![],
        });
        let saved = serde_json::to_string(&p.portable()).unwrap();
        let mut read: Project = serde_json::from_str(&saved).unwrap();
        read.assign_ids();
        read.resolve_sources();
        assert_eq!(
            read.tracks[1].modulators[0].kind,
            ModKind::follower(read.tracks[0].id)
        );
    }

    #[test]
    fn a_stack_gathers_its_tracks_and_flattening_it_lets_them_go() {
        let mut p: Project = serde_json::from_str(
            r#"{"tempo": 120, "sample_rate": 48000, "tracks": [
                {"name": "Kick"}, {"name": "Bass"}, {"name": "Snare"}, {"name": "Pad"}, {"name": "Hats"}]}"#,
        )
        .unwrap();
        p.assign_ids();
        let (bus, moved) = p.add_stack(&[0, 2, 4]);
        let names: Vec<&str> = p.tracks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Kick", "Snare", "Hats", "Bass", "Pad"]);
        assert_eq!(moved, [0, 3, 1, 4, 2]);
        p.add_shared(bus, ModKind::mac());
        p.flatten_stack(bus);
        assert!(p.auxes.is_empty());
        assert!(
            p.tracks
                .iter()
                .all(|t| t.output.is_none() && t.modulators.is_empty())
        );
    }

    #[test]
    fn a_sidechain_names_its_track_wherever_the_song_goes() {
        // Ids are a session's own: the drums' here aren't the drums' after it's saved and read
        // elsewhere, so a sidechain is saved as the track's number.
        let mut p: Project = serde_json::from_str(
            r#"{"tempo": 120, "sample_rate": 48000,
                "tracks": [{"name": "Drums"}, {"name": "Bass"}, {"name": "Pad"}]}"#,
        )
        .unwrap();
        for _ in 0..5 {
            next_id(); // as an app that's opened other songs has used ids up
        }
        p.assign_ids();
        let drums = p.tracks[0].id;
        p.tracks[2].eq = Some(Eq {
            sidechain: Some(drums),
            ..Eq::default()
        });
        let saved = serde_json::to_string(&p.portable()).unwrap();
        let mut read: Project = serde_json::from_str(&saved).unwrap();
        read.assign_ids();
        read.resolve_sources();
        assert_ne!(read.tracks[0].id, drums);
        assert_eq!(
            read.tracks[2].eq.as_ref().unwrap().sidechain,
            Some(read.tracks[0].id)
        );
        // A command names it by its number, as commands do every track.
        let dir = std::env::temp_dir();
        let set: crate::commands::Command = serde_json::from_value(serde_json::json!({
            "op": "set_eq", "channel": { "track": 1 }, "eq": { "sidechain": 0, "bands": [] }
        }))
        .unwrap();
        crate::commands::apply(&mut read, &dir, set).unwrap();
        assert_eq!(
            read.tracks[1].eq.as_ref().unwrap().sidechain,
            Some(read.tracks[0].id)
        );
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
    fn automation_bends_steps_and_takes_shapes() {
        let p = |tick, value| AutoPoint::new(tick, value);
        // Straight, bent slow-then-fast, and a step.
        let line = [p(0, 0.0), p(100, 1.0)];
        assert_eq!(value_at(&line, 50.0), Some(0.5));
        let slow = [
            AutoPoint {
                curve: 0.8,
                ..p(0, 0.0)
            },
            p(100, 1.0),
        ];
        let fast = [
            AutoPoint {
                curve: -0.8,
                ..p(0, 0.0)
            },
            p(100, 1.0),
        ];
        assert!(value_at(&slow, 50.0).unwrap() < 0.2);
        assert!(value_at(&fast, 50.0).unwrap() > 0.8);
        assert_eq!(value_at(&slow, 100.0), Some(1.0));
        let step = [
            AutoPoint {
                step: true,
                ..p(0, 0.0)
            },
            p(100, 1.0),
        ];
        assert_eq!(value_at(&step, 99.0), Some(0.0));
        assert_eq!(value_at(&step, 100.0), Some(1.0));

        // A sine over a bar, a cycle a beat, drawn into a line that was flat at -6 dB: it stays
        // -6 either side, and swings between -12 and 0 inside.
        let mut a = Automation {
            volume: vec![p(0, -6.0), p(PPQ * 8, -6.0)],
            ..Default::default()
        };
        a.shape(
            Param::Volume,
            AutoShape::Sine,
            PPQ * 2,
            PPQ * 6,
            PPQ,
            -12.0,
            0.0,
        );
        let v = |t: u64| value_at(&a.volume, t as f64).unwrap();
        assert_eq!(v(PPQ), -6.0);
        assert_eq!(v(PPQ * 7), -6.0);
        assert!((v(PPQ * 2 + PPQ / 4) - 0.0).abs() < 1e-4);
        assert!((v(PPQ * 2 + PPQ * 3 / 4) + 12.0).abs() < 1e-4);
        assert!(
            (v(PPQ * 2 + PPQ / 8) + 1.8).abs() < 0.8,
            "{}",
            v(PPQ * 2 + PPQ / 8)
        );
        // A square steps; mute is always steps.
        a.shape(Param::Mute, AutoShape::Square, 0, PPQ * 2, PPQ, 0.0, 1.0);
        assert_eq!(value_at(&a.mute, (PPQ / 2 - 1) as f64), Some(1.0));
        assert_eq!(value_at(&a.mute, (PPQ / 2) as f64), Some(0.0));
        assert!(a.mute.iter().all(|p| p.step));

        // Thinning takes out what changes nothing, and keeps the shape.
        let mut a = Automation {
            pan: (0..=100)
                .map(|i| p(i * 10, (i as f32 / 100.0).min(0.5)))
                .collect(),
            ..Default::default()
        };
        a.thin(Param::Pan, 0.002);
        assert_eq!(a.pan.len(), 3, "{:?}", a.pan);
        assert_eq!(value_at(&a.pan, 250.0), Some(0.25));

        // Lifted out and put down elsewhere: the same line there.
        let mut a = Automation {
            volume: vec![p(0, -20.0), p(PPQ * 4, 0.0)],
            pan: vec![p(PPQ, -1.0), p(PPQ * 3, 1.0)],
            ..Default::default()
        };
        let clip = a.take(PPQ, PPQ * 3);
        assert_eq!(clip.length, PPQ * 2);
        a.put(PPQ * 8, &clip);
        for t in [0, PPQ / 2, PPQ, PPQ * 3 / 2, PPQ * 2 - 1] {
            let (was, now) = (PPQ + t, PPQ * 8 + t);
            assert!(
                (value_at(&a.volume, was as f64).unwrap()
                    - value_at(&a.volume, now as f64).unwrap())
                .abs()
                    < 0.01
            );
            assert!(
                (value_at(&a.pan, was as f64).unwrap() - value_at(&a.pan, now as f64).unwrap())
                    .abs()
                    < 0.01
            );
        }
        // Trimmed: all of it up, never past the top.
        a.offset(Param::Volume, 10.0);
        assert_eq!(a.volume[0].value, -10.0);
        assert!(a.volume.iter().all(|p| p.value <= 6.0));
        // Erased: the line either side holds.
        a.erase(PPQ * 8, PPQ * 10);
        assert!(
            a.pan
                .iter()
                .all(|p| !(PPQ * 8 + 1..PPQ * 10).contains(&p.tick))
        );
    }

    #[test]
    fn smoothing_rounds_corners_and_wobbles_off_and_leaves_its_ends() {
        let p = |tick, value| AutoPoint::new(tick, value);
        // An S eases out and in: slow at both ends, halfway at the middle.
        let s = [
            AutoPoint {
                ease: true,
                ..p(0, 0.0)
            },
            p(100, 1.0),
        ];
        assert!(value_at(&s, 10.0).unwrap() < 0.05);
        assert_eq!(value_at(&s, 50.0), Some(0.5));
        assert!(value_at(&s, 90.0).unwrap() > 0.95);

        // A pan recorded with a wobble every sixteenth over two bars, flat either side.
        let wobble: Vec<AutoPoint> = (0..=32)
            .map(|i| p(PPQ * 2 + i * PPQ / 4, if i % 2 == 0 { -0.2 } else { 0.2 }))
            .collect();
        let mut a = Automation {
            pan: [vec![p(0, -0.2)], wobble, vec![p(PPQ * 16, -0.2)]].concat(),
            ..Default::default()
        };
        let before = a.pan.clone();
        a.smooth(Param::Pan, PPQ * 2, PPQ * 10, PPQ / 2);
        let v = |a: &Automation, t: u64| value_at(&a.pan, t as f64).unwrap();
        // The wobble's gone (it swung 0.4), the ends are where they were, outside untouched.
        let swing = (PPQ * 3..PPQ * 9)
            .step_by(30)
            .map(|t| v(&a, t))
            .fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
        assert!(swing.1 - swing.0 < 0.05, "{swing:?}");
        assert_eq!(v(&a, PPQ * 2), -0.2);
        assert!((v(&a, PPQ * 10) - value_at(&before, (PPQ * 10) as f64).unwrap()).abs() < 1e-3);
        assert_eq!(
            v(&a, PPQ * 12),
            v(
                &Automation {
                    pan: before.clone(),
                    ..Default::default()
                },
                PPQ * 12
            )
        );
        assert!(a.pan.len() < before.len(), "{}", a.pan.len());

        // A sharp peak in the volume, rounded off: lower at the top, still rising to it; the
        // silence at its ends stays silence.
        let mut a = Automation {
            volume: vec![p(0, -144.0), p(PPQ * 4, 0.0), p(PPQ * 8, -144.0)],
            ..Default::default()
        };
        a.smooth(Param::Volume, 0, PPQ * 8, PPQ);
        let peak = value_at(&a.volume, (PPQ * 4) as f64).unwrap();
        assert!(peak < -0.5 && peak > -30.0, "{peak}");
        let rise = |t: u64| value_at(&a.volume, t as f64).unwrap();
        assert!(rise(PPQ * 2) < rise(PPQ * 3) && rise(PPQ * 3) < rise(PPQ * 4));
        assert_eq!(value_at(&a.volume, 0.0), Some(-144.0));
        assert_eq!(value_at(&a.volume, (PPQ * 8) as f64), Some(-144.0));
        // Mute's steps stay steps.
        let mut a = Automation {
            mute: vec![
                AutoPoint {
                    step: true,
                    ..p(0, 0.0)
                },
                AutoPoint {
                    step: true,
                    ..p(PPQ, 1.0)
                },
            ],
            ..Default::default()
        };
        let was = a.mute.clone();
        a.smooth(Param::Mute, 0, PPQ, PPQ);
        assert_eq!(a.mute, was);
    }

    #[test]
    fn automation_points_stay_in_order_and_interpolate() {
        let mut a = Automation::default();
        assert_eq!(value_at(a.points(Param::Volume), 0.0), None);
        a.add(Param::Volume, AutoPoint::new(960, -12.0));
        a.add(Param::Volume, AutoPoint::new(0, 0.0));
        assert_eq!(a.add(Param::Volume, AutoPoint::new(480, -6.0)), 1);
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
            a.add(Param::Volume, AutoPoint::new(tick, -6.0));
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
                AutoPoint::new(0, i as f32),
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
            mute: false,
            ..Default::default()
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

    #[test]
    fn rows_loop_on_their_own_lengths_muted_rows_rest_and_midi_becomes_a_pattern() {
        let step = PPQ / 4;
        let row = |pitch: u8, cells: Vec<u8>, mute: bool| PatternRow {
            pitch,
            cells,
            mute,
            ..Default::default()
        };
        let rows = vec![
            row(36, vec![100, 0, 0, 0], false),
            row(42, vec![80, 0, 0], false),
            row(38, vec![100; 4], true),
        ];
        let notes = pattern_notes(step, &rows, step * 8, 50.0);
        let at = |p: u8| {
            notes
                .iter()
                .filter(|n| n.pitch == p)
                .map(|n| n.start)
                .collect::<Vec<_>>()
        };
        let s = step as i64;
        assert_eq!(at(36), [0, 4 * s]);
        assert_eq!(at(42), [0, 3 * s, 6 * s]);
        assert!(at(38).is_empty());
        // A beat of MIDI: kick on 1 and 3 (a little late), a hat between; highest row first.
        let n = |start: i64, pitch: u8, velocity: u8| Note {
            start,
            length: 100,
            pitch,
            velocity,
        };
        let rows = to_pattern(
            &[n(0, 36, 110), n(2 * s + 20, 36, 90), n(s, 42, 70)],
            step,
            4,
        );
        assert_eq!(
            rows.iter()
                .map(|r| (r.pitch, r.cells.clone()))
                .collect::<Vec<_>>(),
            [(42, vec![0, 70, 0, 0]), (36, vec![110, 0, 90, 0])]
        );
    }
}
