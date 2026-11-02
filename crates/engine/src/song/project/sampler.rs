//! The built-in sampler: its zones and voice.

use super::*;

/// The built-in sampler: a sample on each note, as a drum
/// kit; one sample played across a range of notes, repitched, as an 808 or a bass; a loop sliced
/// across the keys; or layers by velocity and round robins. How every voice plays (how it reads
/// its sample, envelopes, filter, LFOs and what they move, mono and glide) is the instrument's.
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
    /// 12 dB an octave, clean.
    LowPass,
    HighPass,
    BandPass,
    Notch,
    /// 24 dB an octave, the four-pole ladder: fat, singing at high resonance, driven.
    Ladder,
    /// A comb tuned to the cutoff: metallic, pitched resonances.
    Comb,
    /// Vowels: the cutoff sweeps from "a" through "e", "i", "o" to "u".
    Formant,
}

impl FilterMode {
    pub const ALL: [FilterMode; 8] = [
        FilterMode::Off,
        FilterMode::LowPass,
        FilterMode::HighPass,
        FilterMode::BandPass,
        FilterMode::Notch,
        FilterMode::Ladder,
        FilterMode::Comb,
        FilterMode::Formant,
    ];

    pub fn name(self) -> &'static str {
        match self {
            FilterMode::Off => "Off",
            FilterMode::LowPass => "Low pass",
            FilterMode::HighPass => "High pass",
            FilterMode::BandPass => "Band pass",
            FilterMode::Notch => "Notch",
            FilterMode::Ladder => "Ladder",
            FilterMode::Comb => "Comb",
            FilterMode::Formant => "Formant",
        }
    }
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

/// How a sampler reads its samples.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SampleMode {
    /// Faster is higher, as a tape or a classic sampler: pitch and speed together.
    #[default]
    Repitch,
    /// Pitch and speed apart: overlapping grains keep each note's pitch while the sample plays
    /// at `speed` (0 holds it still).
    Stretch,
    /// A cloud of grains around a place in the sample: their size, how many, how scattered,
    /// detuned, reversed and spread.
    Grains,
}

impl SampleMode {
    pub const ALL: [SampleMode; 3] = [SampleMode::Repitch, SampleMode::Stretch, SampleMode::Grains];

    pub fn name(self) -> &'static str {
        match self {
            SampleMode::Repitch => "Repitch",
            SampleMode::Stretch => "Stretch",
            SampleMode::Grains => "Grains",
        }
    }
}

/// What reading a sample does to its sound.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SampleCharacter {
    /// As clean as it can be: interpolated finely, kept from aliasing when it's played high.
    #[default]
    Clean,
    /// Tape: warmth and saturation, the top end softened, a little wow and flutter, as much as
    /// `color` says.
    Tape,
    /// A vintage sampler: read at `crush_khz` and `bits` bits without smoothing (the grit and
    /// aliasing of the old drum machines), and through their output filter if `crush_filter`.
    Vintage,
}

impl SampleCharacter {
    pub const ALL: [SampleCharacter; 3] = [
        SampleCharacter::Clean,
        SampleCharacter::Tape,
        SampleCharacter::Vintage,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SampleCharacter::Clean => "Clean",
            SampleCharacter::Tape => "Tape",
            SampleCharacter::Vintage => "Vintage",
        }
    }
}

/// An LFO's wave.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Wave {
    #[default]
    Sine,
    Triangle,
    SawUp,
    SawDown,
    Square,
    /// A new random value each cycle, held (sample and hold).
    Random,
    /// Random values each cycle, glided between.
    Drift,
}

impl Wave {
    pub const ALL: [Wave; 7] = [
        Wave::Sine,
        Wave::Triangle,
        Wave::SawUp,
        Wave::SawDown,
        Wave::Square,
        Wave::Random,
        Wave::Drift,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Wave::Sine => "Sine",
            Wave::Triangle => "Triangle",
            Wave::SawUp => "Saw up",
            Wave::SawDown => "Saw down",
            Wave::Square => "Square",
            Wave::Random => "Random",
            Wave::Drift => "Drift",
        }
    }
}

/// An LFO: its wave, and how fast it goes (a cycle `beats` long in time with the song, or
/// `hz` times a second when `beats` is 0); started over by each note, or running free.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Lfo {
    pub wave: Wave,
    pub hz: f32,
    pub beats: f32,
    pub retrigger: bool,
}

impl Default for Lfo {
    fn default() -> Self {
        Lfo {
            wave: Wave::Sine,
            hz: 2.0,
            beats: 0.0,
            retrigger: false,
        }
    }
}

/// An envelope: attack, decay and release (ms) and the level held (0 to 1).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Adsr {
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
}

impl Default for Adsr {
    fn default() -> Self {
        Adsr {
            attack_ms: 0.0,
            decay_ms: 500.0,
            sustain: 0.0,
            release_ms: 300.0,
        }
    }
}

/// What moves a sampler setting, voice by voice.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModSource {
    #[default]
    None,
    Lfo1,
    Lfo2,
    /// The modulation envelope.
    ModEnv,
    AmpEnv,
    FilterEnv,
    /// How hard the note was played.
    Velocity,
    /// Where the note is: -1 five octaves under middle C, 1 five over.
    Key,
    /// The mod wheel.
    Wheel,
    /// Aftertouch, the channel's or the note's own.
    Pressure,
    /// A value of its own for each note, -1 to 1.
    Random,
    /// The pitch bend.
    Bend,
}

impl ModSource {
    pub const ALL: [ModSource; 11] = [
        ModSource::Lfo1,
        ModSource::Lfo2,
        ModSource::ModEnv,
        ModSource::AmpEnv,
        ModSource::FilterEnv,
        ModSource::Velocity,
        ModSource::Key,
        ModSource::Wheel,
        ModSource::Pressure,
        ModSource::Random,
        ModSource::Bend,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ModSource::None => "None",
            ModSource::Lfo1 => "LFO 1",
            ModSource::Lfo2 => "LFO 2",
            ModSource::ModEnv => "Mod env",
            ModSource::AmpEnv => "Amp env",
            ModSource::FilterEnv => "Filter env",
            ModSource::Velocity => "Velocity",
            ModSource::Key => "Key",
            ModSource::Wheel => "Mod wheel",
            ModSource::Pressure => "Pressure",
            ModSource::Random => "Random",
            ModSource::Bend => "Pitch bend",
        }
    }
}

/// A sampler setting a route moves, and how far its full amount takes it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModDest {
    /// 24 semitones.
    #[default]
    Pitch,
    /// 6 octaves.
    Cutoff,
    Resonance,
    /// Its level: from silent to twice as loud.
    Level,
    Pan,
    /// Where a note starts in its sample, as far as the whole of it.
    Start,
    /// Where the grains read in the sample.
    Position,
    /// The grains' size: 8 times bigger or smaller.
    Size,
    /// How many grains: 8 times more or fewer.
    Density,
    Spray,
    /// How fast it plays through the sample: 2 either way.
    Speed,
    Drive,
    Fm,
    Sub,
    Noise,
}

impl ModDest {
    pub const ALL: [ModDest; 15] = [
        ModDest::Pitch,
        ModDest::Cutoff,
        ModDest::Resonance,
        ModDest::Level,
        ModDest::Pan,
        ModDest::Start,
        ModDest::Position,
        ModDest::Size,
        ModDest::Density,
        ModDest::Spray,
        ModDest::Speed,
        ModDest::Drive,
        ModDest::Fm,
        ModDest::Sub,
        ModDest::Noise,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ModDest::Pitch => "Pitch",
            ModDest::Cutoff => "Cutoff",
            ModDest::Resonance => "Resonance",
            ModDest::Level => "Level",
            ModDest::Pan => "Pan",
            ModDest::Start => "Start",
            ModDest::Position => "Position",
            ModDest::Size => "Grain size",
            ModDest::Density => "Density",
            ModDest::Spray => "Spray",
            ModDest::Speed => "Speed",
            ModDest::Drive => "Drive",
            ModDest::Fm => "FM",
            ModDest::Sub => "Sub",
            ModDest::Noise => "Noise",
        }
    }
}

/// A route of the modulation matrix: a source moving a setting, -1 to 1 of its full amount.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct Route {
    pub source: ModSource,
    pub dest: ModDest,
    pub amount: f32,
}

/// The most routes a sampler's matrix has.
pub const MAX_ROUTES: usize = 8;

/// The modulation matrix: up to `MAX_ROUTES` routes, saved as a list of those set.
#[derive(Clone, Copy, PartialEq, Default)]
pub struct Routes([Route; MAX_ROUTES]);

impl Routes {
    pub fn from_slice(routes: &[Route]) -> Self {
        let mut r = Routes::default();
        for (slot, route) in r.0.iter_mut().zip(routes) {
            *slot = *route;
        }
        r
    }

    /// The routes set (a source chosen).
    pub fn iter(&self) -> impl Iterator<Item = &Route> {
        self.0.iter().filter(|r| r.source != ModSource::None)
    }

    pub fn slots(&self) -> &[Route; MAX_ROUTES] {
        &self.0
    }

    pub fn slots_mut(&mut self) -> &mut [Route; MAX_ROUTES] {
        &mut self.0
    }
}

impl std::fmt::Debug for Routes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl Serialize for Routes {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.iter())
    }
}

impl<'de> Deserialize<'de> for Routes {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let list = Vec::<Route>::deserialize(d)?;
        Ok(Routes::from_slice(&list))
    }
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
    /// How hard the sound goes into the filter, 0 (clean) to 1.
    pub drive: f32,
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
    /// LFO 1: its rate (Hz), and how far it moves pitch (cents), the cutoff (octaves) and
    /// level (0 to 1); its wave, its length in beats when in time with the song (0: `lfo_hz`),
    /// and whether each note starts it over.
    pub lfo_hz: f32,
    pub vibrato_cents: f32,
    pub lfo_filter: f32,
    pub tremolo: f32,
    pub lfo_wave: Wave,
    pub lfo_beats: f32,
    pub lfo_retrigger: bool,
    /// LFO 2, which moves what the matrix routes it to.
    pub lfo2: Lfo,
    /// The modulation envelope, for the matrix.
    pub mod_env: Adsr,
    pub matrix: Routes,
    /// Semitones, and dB, over every zone's own.
    pub transpose: f32,
    pub gain_db: f32,
    /// How it reads its samples, and what that does to their sound.
    pub mode: SampleMode,
    pub character: SampleCharacter,
    /// Tape's warmth, 0 to 1.
    pub color: f32,
    /// A vintage sampler's rate (kHz), bits and output filter.
    pub crush_khz: f32,
    pub bits: u8,
    pub crush_filter: bool,
    /// How fast it plays through its sample: 1 as recorded, 0 still, under 0 backwards (in
    /// Repitch, faster is higher too).
    pub speed: f32,
    /// Where in the sample Stretch and Grains begin, as a share of the zone's part.
    pub position: f32,
    /// Grains stay where they are (Stretch and Grains): `position` and what moves it say
    /// where.
    pub freeze: bool,
    /// The grains: their size (ms), how many a second (Grains; Stretch overlaps four), how far
    /// they're scattered about (0 to 1, up to a second), each one's pitch set off at random
    /// (cents), the chance one plays backwards (0 to 1), how far apart they spread (0 to 1), and
    /// their shape (0 smooth, 1 hard-edged).
    pub grain_ms: f32,
    pub density: f32,
    pub spray: f32,
    pub grain_pitch: f32,
    pub grain_reverse: f32,
    pub grain_spread: f32,
    pub grain_shape: f32,
    /// Unison: how many voices each note plays (1 to 8), detuned (cents) and spread across the
    /// stereo field (0 to 1).
    pub unison: u8,
    pub detune_cents: f32,
    pub spread: f32,
    /// A drop in pitch at each note's start (semitones, under 0 a rise) falling away over
    /// `pitch_decay_ms`: an 808's knock, a tom's bend.
    pub pitch_env: f32,
    pub pitch_decay_ms: f32,
    /// What's played with the sample: a sine an octave under the note (level 0 to 1), noise
    /// (level, and -1 dark to 1 bright), the sample's pitch moved by a sine at `osc_ratio`
    /// times the note (FM, 0 to 1), the sound multiplied by one (ring, 0 to 1), and its start
    /// pushed louder (punch, 0 to 1).
    pub sub: f32,
    pub noise: f32,
    pub noise_color: f32,
    pub fm: f32,
    pub ring: f32,
    pub osc_ratio: f32,
    pub punch: f32,
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
            drive: 0.0,
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
            lfo_wave: Wave::Sine,
            lfo_beats: 0.0,
            lfo_retrigger: false,
            lfo2: Lfo::default(),
            mod_env: Adsr::default(),
            matrix: Routes::default(),
            transpose: 0.0,
            gain_db: 0.0,
            mode: SampleMode::Repitch,
            character: SampleCharacter::Clean,
            color: 0.5,
            crush_khz: 26.04,
            bits: 12,
            crush_filter: false,
            speed: 1.0,
            position: 0.0,
            freeze: false,
            grain_ms: 80.0,
            density: 20.0,
            spray: 0.1,
            grain_pitch: 0.0,
            grain_reverse: 0.0,
            grain_spread: 0.3,
            grain_shape: 0.0,
            unison: 1,
            detune_cents: 12.0,
            spread: 0.5,
            pitch_env: 0.0,
            pitch_decay_ms: 60.0,
            sub: 0.0,
            noise: 0.0,
            noise_color: 0.0,
            fm: 0.0,
            ring: 0.0,
            osc_ratio: 1.0,
            punch: 0.0,
        }
    }
}

impl SamplerVoice {
    pub fn is_default(&self) -> bool {
        *self == SamplerVoice::default()
    }
}

/// When a zone plays: as its note starts, or as it's let go (a key's release, a string's
/// squeak).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    #[default]
    Attack,
    Release,
}

/// A sample, and the notes it plays on.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Zone {
    /// Relative to the project folder.
    pub file: PathBuf,
    /// What an old song's synth had in place of a sample: read only to bring it up to date.
    #[serde(default, skip_serializing)]
    pub wave: Option<OscWave>,
    /// The library's file it was brought in from, so what sounds like it can be found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<PathBuf>,
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
    /// Looping while held, between these shares of the sample (its play range by default),
    /// crossfading over `loop_fade` of the loop's length (0 to 0.5) so its seam doesn't click.
    #[serde(default, skip_serializing_if = "is_loop_off")]
    pub loop_mode: LoopMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_start: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_end: Option<f32>,
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub loop_fade: f32,
    /// Played backwards.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reverse: bool,
    /// The velocities it plays for (1 to 127): layers, a soft hit and a hard one. Zones that
    /// share notes and velocities take turns, as round robins; layers overlapping play
    /// together. `velocity_fade` velocities either side of its range it fades in and out over,
    /// so layers blend rather than switch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub velocities: Option<(u8, u8)>,
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub velocity_fade: u8,
    /// When it plays: as its note starts, or as it's let go.
    #[serde(default, skip_serializing_if = "is_attack")]
    pub trigger: Trigger,
    /// Zones on different layers play together, stacked (a kick, and a sub under it); on one
    /// layer, zones alike in notes and velocities take turns.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub layer: u8,
}

pub(crate) fn is_loop_off(m: &LoopMode) -> bool {
    *m == LoopMode::Off
}

fn is_attack(t: &Trigger) -> bool {
    *t == Trigger::Attack
}

fn is_zero_u8(v: &u8) -> bool {
    *v == 0
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

/// How a pad's sounds (zones on one note) play: all at once, stacked; in turn, a round robin,
/// so a hit repeated isn't the same each time; or by how hard it's hit, the first softest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layering {
    Stack,
    Cycle,
    Velocity,
}

impl Layering {
    pub const ALL: [Layering; 3] = [Layering::Stack, Layering::Cycle, Layering::Velocity];

    pub fn name(self) -> &'static str {
        match self {
            Layering::Stack => "Stack",
            Layering::Cycle => "Cycle",
            Layering::Velocity => "Velocity",
        }
    }
}

impl Sampler {
    /// How its zones play together, if as one of the layerings: one zone is a stack of one.
    pub fn layering(&self) -> Option<Layering> {
        let z = &self.zones;
        let layers: Vec<u8> = z.iter().map(|z| z.layer).collect();
        let velocities: Vec<_> = z.iter().map(|z| z.velocities).collect();
        let plain = velocities.iter().all(Option::is_none);
        if plain && distinct(&layers) {
            Some(Layering::Stack)
        } else if plain && layers.iter().all(|&l| l == layers[0]) {
            Some(Layering::Cycle)
        } else if velocities.iter().all(Option::is_some) && distinct(&velocities) {
            Some(Layering::Velocity)
        } else {
            None
        }
    }

    /// Its zones played as `l` says: by velocity, the range shared out in their order (blending
    /// as they did, if they did).
    pub fn set_layering(&mut self, l: Layering) {
        let n = self.zones.len().max(1);
        let blend = self.blends();
        for (i, z) in self.zones.iter_mut().enumerate() {
            (z.layer, z.velocities) = match l {
                Layering::Stack => (i.min(255) as u8, None),
                Layering::Cycle => (0, None),
                Layering::Velocity => {
                    let (lo, hi) = (1 + 127 * i / n, 127 * (i + 1) / n);
                    (0, Some((lo as u8, hi as u8)))
                }
            };
        }
        self.set_blend(blend);
    }

    /// Whether its velocity layers blend into each other rather than switching.
    pub fn blends(&self) -> bool {
        self.zones.iter().any(|z| z.velocity_fade > 0)
    }

    /// Its velocity layers blended, each fading in and out over a third of its range either
    /// side, or switched between.
    pub fn set_blend(&mut self, on: bool) {
        for z in &mut self.zones {
            z.velocity_fade = match (on, z.velocities) {
                (true, Some((lo, hi))) => (hi.saturating_sub(lo) / 3).max(1),
                _ => 0,
            };
        }
    }
}

fn distinct<T: PartialEq>(k: &[T]) -> bool {
    (1..k.len()).all(|i| !k[..i].contains(&k[i]))
}

pub(crate) fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
}
