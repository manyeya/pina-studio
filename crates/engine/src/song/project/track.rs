//! Tracks and auxes: what they play, where, panned how; their sends and Live Loops cells.

use super::*;

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
    pub modulators: Vec<crate::dsp::modulate::Modulator>,
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
    /// The pattern a new pattern region on it starts as (its rows and settings, no steps on),
    /// stored from the step sequencer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern_template: Option<super::Pattern>,
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
    pub modulators: Vec<crate::dsp::modulate::Modulator>,
    /// A stack's tracks folded away under it in the arrangement.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub folded: bool,
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
