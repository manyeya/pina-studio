//! Pina Studio's synth: three wavetable oscillators (each two warps and up to sixteen voices of
//! unison), a sub and noise, two analog-modelled filters in series or parallel, four envelopes,
//! eight LFOs, eight macros and a matrix between them.
//!
//! Everything that moves smoothly is one number in `params`, its place in `poly_params`. So each
//! is automatable, modulatable and on a knob by being here at all: nothing can be left out of
//! the automation list. What it can't sweep through (a wave's name, a filter's model) is a field
//! of its own. A value is in its own units, Hz or dB, so a song reads as it sounds.

use std::{collections::BTreeMap, sync::LazyLock};

use super::*;

pub const OSCS: usize = 3;
pub const FILTERS: usize = 2;
pub const ENVS: usize = 4;
pub const LFOS: usize = 8;
pub const MACROS: usize = 8;
/// The most routes a synth's matrix holds.
pub const MAX_POLY_ROUTES: usize = 64;

/// What each oscillator has, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OscField {
    Level,
    Pan,
    Octave,
    Semi,
    Fine,
    /// Where it is through its wavetable, 0 to 1.
    Position,
    WarpA,
    WarpB,
    Unison,
    Detune,
    /// How loud the outer unison voices are against the middle.
    Blend,
    /// How far apart the unison voices are, 0 mono.
    Width,
    Phase,
    /// How much the phase is left to chance: 1 free-running, as an analog oscillator's.
    Random,
}
const OSC_FIELDS: usize = 14;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterField {
    Cutoff,
    Resonance,
    Drive,
    /// How much the lows are kept up as the resonance rises.
    Bass,
    /// How far the cutoff follows the keys: 1 an octave an octave.
    KeyTrack,
    /// How far apart the sides' cutoffs are, octaves.
    Spread,
    Mix,
}
const FILTER_FIELDS: usize = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvField {
    Delay,
    Attack,
    Hold,
    Decay,
    Sustain,
    Release,
    /// The segments' bend: 0 straight, 1 as a capacitor charges.
    Curve,
}
const ENV_FIELDS: usize = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LfoField {
    Rate,
    Phase,
    /// How long it takes to fade in after its note, ms.
    Rise,
    /// How much the steps are rounded off.
    Smooth,
}
const LFO_FIELDS: usize = 4;

const OSC_BASE: usize = 0;
const SUB_BASE: usize = OSC_BASE + OSCS * OSC_FIELDS;
pub const SUB_LEVEL: usize = SUB_BASE;
pub const SUB_PAN: usize = SUB_BASE + 1;
const NOISE_BASE: usize = SUB_BASE + 2;
pub const NOISE_LEVEL: usize = NOISE_BASE;
pub const NOISE_PAN: usize = NOISE_BASE + 1;
/// -1 dark to 1 bright.
pub const NOISE_COLOR: usize = NOISE_BASE + 2;
const FILTER_BASE: usize = NOISE_BASE + 3;
const ENV_BASE: usize = FILTER_BASE + FILTERS * FILTER_FIELDS;
const LFO_BASE: usize = ENV_BASE + ENVS * ENV_FIELDS;
const MACRO_BASE: usize = LFO_BASE + LFOS * LFO_FIELDS;
const VOICE_BASE: usize = MACRO_BASE + MACROS;
pub const POLY_GLIDE: usize = VOICE_BASE;
/// A slow wander of each voice's pitch and filter, as old parts have.
pub const POLY_DRIFT: usize = VOICE_BASE + 1;
/// How much velocity takes off a soft note.
pub const POLY_VELOCITY: usize = VOICE_BASE + 2;
pub const POLY_MASTER: usize = VOICE_BASE + 3;
pub const PARAM_COUNT: usize = VOICE_BASE + 4;

pub const fn osc_param(i: usize, f: OscField) -> usize {
    OSC_BASE + i * OSC_FIELDS + f as usize
}
pub const fn filter_param(i: usize, f: FilterField) -> usize {
    FILTER_BASE + i * FILTER_FIELDS + f as usize
}
pub const fn env_param(i: usize, f: EnvField) -> usize {
    ENV_BASE + i * ENV_FIELDS + f as usize
}
pub const fn lfo_param(i: usize, f: LfoField) -> usize {
    LFO_BASE + i * LFO_FIELDS + f as usize
}
pub const fn macro_param(i: usize) -> usize {
    MACRO_BASE + i
}

/// How a parameter reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reads {
    Percent,
    /// -100% to 100%.
    Signed,
    Db,
    Hz,
    Ms,
    Cents,
    Whole,
    Octaves,
}

/// One of a synth's parameters.
pub struct PolyParam {
    /// What it's saved and asked for by: `osc1_level`, `filter2_cutoff`.
    pub id: String,
    /// What people call it: "A Level".
    pub name: String,
    /// Which part of the synth it's in, for lists.
    pub group: &'static str,
    pub scale: KnobScale,
    pub default: f32,
    pub reads: Reads,
    pub bipolar: bool,
}

impl PolyParam {
    pub fn say(&self, v: f32) -> String {
        match self.reads {
            Reads::Percent => format!("{:.0} %", v * 100.0),
            Reads::Signed => format!("{:+.0} %", v * 100.0),
            Reads::Db => format!("{v:+.1} dB"),
            Reads::Hz if v >= 1000.0 => format!("{:.2} kHz", v / 1000.0),
            Reads::Hz => format!("{v:.1} Hz"),
            Reads::Ms if v >= 1000.0 => format!("{:.2} s", v / 1000.0),
            Reads::Ms => format!("{v:.0} ms"),
            Reads::Cents => format!("{v:+.0} ct"),
            Reads::Whole => format!("{v:.0}"),
            Reads::Octaves => format!("{v:.2} oct"),
        }
    }
}

const UNIT: KnobScale = KnobScale::Lin(0.0, 1.0);
const SWING: KnobScale = KnobScale::Lin(-1.0, 1.0);

/// Every parameter, in its place.
pub static POLY_PARAMS: LazyLock<Vec<PolyParam>> = LazyLock::new(|| {
    let mut out = Vec::with_capacity(PARAM_COUNT);
    let mut add = |id: String, name: String, group, scale, default, reads, bipolar| {
        out.push(PolyParam {
            id,
            name,
            group,
            scale,
            default,
            reads,
            bipolar,
        });
    };
    for i in 0..OSCS {
        let (o, n) = (i + 1, ["A", "B", "C"][i]);
        let group = ["Oscillator A", "Oscillator B", "Oscillator C"][i];
        let level = if i == 0 { 0.75 } else { 0.0 };
        let mut osc = |id: &str, name: &str, scale, default, reads, bipolar| {
            add(
                format!("osc{o}_{id}"),
                format!("{n} {name}"),
                group,
                scale,
                default,
                reads,
                bipolar,
            );
        };
        osc("level", "Level", UNIT, level, Reads::Percent, false);
        osc("pan", "Pan", SWING, 0.0, Reads::Signed, true);
        osc(
            "octave",
            "Octave",
            KnobScale::Lin(-4.0, 4.0),
            0.0,
            Reads::Whole,
            true,
        );
        osc(
            "semi",
            "Semitones",
            KnobScale::Lin(-12.0, 12.0),
            0.0,
            Reads::Whole,
            true,
        );
        osc(
            "fine",
            "Fine",
            KnobScale::Lin(-100.0, 100.0),
            0.0,
            Reads::Cents,
            true,
        );
        osc("position", "Position", UNIT, 0.0, Reads::Percent, false);
        osc("warp_a", "Warp 1", UNIT, 0.0, Reads::Percent, false);
        osc("warp_b", "Warp 2", UNIT, 0.0, Reads::Percent, false);
        osc(
            "unison",
            "Unison",
            KnobScale::Lin(1.0, 16.0),
            1.0,
            Reads::Whole,
            false,
        );
        osc(
            "detune",
            "Detune",
            KnobScale::Lin(0.0, 100.0),
            20.0,
            Reads::Cents,
            false,
        );
        osc("blend", "Blend", UNIT, 0.75, Reads::Percent, false);
        osc("width", "Width", UNIT, 1.0, Reads::Percent, false);
        osc("phase", "Phase", UNIT, 0.0, Reads::Percent, false);
        osc("random", "Random Phase", UNIT, 1.0, Reads::Percent, false);
    }
    add(
        "sub_level".into(),
        "Sub Level".into(),
        "Sub",
        UNIT,
        0.0,
        Reads::Percent,
        false,
    );
    add(
        "sub_pan".into(),
        "Sub Pan".into(),
        "Sub",
        SWING,
        0.0,
        Reads::Signed,
        true,
    );
    add(
        "noise_level".into(),
        "Noise Level".into(),
        "Noise",
        UNIT,
        0.0,
        Reads::Percent,
        false,
    );
    add(
        "noise_pan".into(),
        "Noise Pan".into(),
        "Noise",
        SWING,
        0.0,
        Reads::Signed,
        true,
    );
    add(
        "noise_color".into(),
        "Noise Color".into(),
        "Noise",
        SWING,
        0.0,
        Reads::Signed,
        true,
    );
    for i in 0..FILTERS {
        let (o, n) = (i + 1, i + 1);
        let group = ["Filter 1", "Filter 2"][i];
        let mut filter = |id: &str, name: &str, scale, default, reads, bipolar| {
            add(
                format!("filter{o}_{id}"),
                format!("Filter {n} {name}"),
                group,
                scale,
                default,
                reads,
                bipolar,
            );
        };
        filter(
            "cutoff",
            "Cutoff",
            KnobScale::Log(20.0, 20_000.0),
            20_000.0,
            Reads::Hz,
            false,
        );
        filter("resonance", "Resonance", UNIT, 0.0, Reads::Percent, false);
        filter("drive", "Drive", UNIT, 0.0, Reads::Percent, false);
        filter("bass", "Bass", UNIT, 0.0, Reads::Percent, false);
        filter(
            "key_track",
            "Key Track",
            KnobScale::Lin(-1.0, 2.0),
            0.0,
            Reads::Percent,
            true,
        );
        filter(
            "spread",
            "Spread",
            KnobScale::Lin(0.0, 2.0),
            0.0,
            Reads::Octaves,
            false,
        );
        filter("mix", "Mix", UNIT, 1.0, Reads::Percent, false);
    }
    const TIME: KnobScale = KnobScale::Log(0.1, 30_000.0);
    for i in 0..ENVS {
        let (o, group) = (
            i + 1,
            ["Envelope 1", "Envelope 2", "Envelope 3", "Envelope 4"][i],
        );
        let mut env = |id: &str, name: &str, scale, default, reads| {
            add(
                format!("env{o}_{id}"),
                format!("Envelope {o} {name}"),
                group,
                scale,
                default,
                reads,
                false,
            );
        };
        env(
            "delay",
            "Delay",
            KnobScale::Log(0.1, 10_000.0),
            0.1,
            Reads::Ms,
        );
        env("attack", "Attack", TIME, 1.0, Reads::Ms);
        env(
            "hold",
            "Hold",
            KnobScale::Log(0.1, 10_000.0),
            0.1,
            Reads::Ms,
        );
        env("decay", "Decay", TIME, 300.0, Reads::Ms);
        env("sustain", "Sustain", UNIT, 1.0, Reads::Percent);
        env("release", "Release", TIME, 150.0, Reads::Ms);
        env("curve", "Curve", UNIT, 0.7, Reads::Percent);
    }
    for i in 0..LFOS {
        let o = i + 1;
        let group = [
            "LFO 1", "LFO 2", "LFO 3", "LFO 4", "LFO 5", "LFO 6", "LFO 7", "LFO 8",
        ][i];
        let mut lfo = |id: &str, name: &str, scale, default, reads| {
            add(
                format!("lfo{o}_{id}"),
                format!("LFO {o} {name}"),
                group,
                scale,
                default,
                reads,
                false,
            );
        };
        lfo("rate", "Rate", KnobScale::Log(0.01, 200.0), 2.0, Reads::Hz);
        lfo("phase", "Phase", UNIT, 0.0, Reads::Percent);
        lfo(
            "rise",
            "Rise",
            KnobScale::Log(0.1, 10_000.0),
            0.1,
            Reads::Ms,
        );
        lfo("smooth", "Smooth", UNIT, 0.0, Reads::Percent);
    }
    for i in 0..MACROS {
        add(
            format!("macro{}", i + 1),
            format!("Macro {}", i + 1),
            "Macros",
            UNIT,
            0.0,
            Reads::Percent,
            false,
        );
    }
    add(
        "glide".into(),
        "Glide".into(),
        "Voice",
        KnobScale::Log(0.1, 5000.0),
        0.1,
        Reads::Ms,
        false,
    );
    add(
        "drift".into(),
        "Drift".into(),
        "Voice",
        UNIT,
        0.1,
        Reads::Percent,
        false,
    );
    add(
        "velocity".into(),
        "Velocity".into(),
        "Voice",
        UNIT,
        0.6,
        Reads::Percent,
        false,
    );
    add(
        "master".into(),
        "Master".into(),
        "Voice",
        KnobScale::Lin(-48.0, 12.0),
        -6.0,
        Reads::Db,
        true,
    );
    out
});

/// A parameter's place, by its id or its name (any case).
pub fn poly_param(name: &str) -> Option<usize> {
    POLY_PARAMS
        .iter()
        .position(|p| p.id.eq_ignore_ascii_case(name) || p.name.eq_ignore_ascii_case(name))
}

/// A synth's parameters, in their own units. Saved by id, those left as they come left out, so a
/// song stays small and a parameter added later comes in at its default.
#[derive(Clone, Debug, PartialEq)]
pub struct PolyValues(pub Vec<f32>);

impl Default for PolyValues {
    fn default() -> Self {
        PolyValues(POLY_PARAMS.iter().map(|p| p.default).collect())
    }
}

impl std::ops::Index<usize> for PolyValues {
    type Output = f32;

    fn index(&self, i: usize) -> &f32 {
        &self.0[i]
    }
}

impl std::ops::IndexMut<usize> for PolyValues {
    fn index_mut(&mut self, i: usize) -> &mut f32 {
        &mut self.0[i]
    }
}

impl Serialize for PolyValues {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let changed: BTreeMap<&str, f32> = POLY_PARAMS
            .iter()
            .zip(&self.0)
            .filter(|(p, v)| p.default != **v)
            .map(|(p, v)| (p.id.as_str(), *v))
            .collect();
        changed.serialize(s)
    }
}

impl<'de> Deserialize<'de> for PolyValues {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let saved = BTreeMap::<String, f32>::deserialize(d)?;
        let mut out = PolyValues::default();
        for (id, v) in saved {
            // One from a later version is left out, not an error.
            if let Some(i) = poly_param(&id) {
                out[i] = v;
            }
        }
        Ok(out)
    }
}

/// What a warp does to an oscillator.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Warp {
    #[default]
    None,
    /// Restarts the wave, faster: the sound of hard sync.
    Sync,
    /// Bends the phase, pushing the wave's energy up, or down, or to both ends.
    BendUp,
    BendDown,
    BendBoth,
    /// Squeezes the wave into part of its cycle: pulse-width on anything.
    Pwm,
    /// Leans the wave one way.
    Asym,
    /// Reflects the wave back on itself past a point.
    Mirror,
    /// Inverts part of the cycle.
    Flip,
    /// Steps the phase: a coarse, crunched wave.
    Quantize,
    /// Folds it back at its peaks.
    Fold,
    /// Turns the phase with itself.
    PhaseDist,
    /// Frequency-modulated by an earlier oscillator.
    FmFromA,
    FmFromB,
    /// Ring-modulated by an earlier oscillator.
    RingFromA,
    RingFromB,
    /// Its level moved by an earlier oscillator.
    AmFromA,
    AmFromB,
    /// Frequency-modulated by the noise, or the sub.
    FmFromNoise,
    FmFromSub,
}

impl Warp {
    pub const ALL: [Warp; 20] = [
        Warp::None,
        Warp::Sync,
        Warp::BendUp,
        Warp::BendDown,
        Warp::BendBoth,
        Warp::Pwm,
        Warp::Asym,
        Warp::Mirror,
        Warp::Flip,
        Warp::Quantize,
        Warp::Fold,
        Warp::PhaseDist,
        Warp::FmFromA,
        Warp::FmFromB,
        Warp::RingFromA,
        Warp::RingFromB,
        Warp::AmFromA,
        Warp::AmFromB,
        Warp::FmFromNoise,
        Warp::FmFromSub,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Warp::None => "Off",
            Warp::Sync => "Sync",
            Warp::BendUp => "Bend +",
            Warp::BendDown => "Bend -",
            Warp::BendBoth => "Bend +/-",
            Warp::Pwm => "PWM",
            Warp::Asym => "Asym",
            Warp::Mirror => "Mirror",
            Warp::Flip => "Flip",
            Warp::Quantize => "Quantize",
            Warp::Fold => "Fold",
            Warp::PhaseDist => "Phase Dist",
            Warp::FmFromA => "FM from A",
            Warp::FmFromB => "FM from B",
            Warp::RingFromA => "Ring from A",
            Warp::RingFromB => "Ring from B",
            Warp::AmFromA => "AM from A",
            Warp::AmFromB => "AM from B",
            Warp::FmFromNoise => "FM from Noise",
            Warp::FmFromSub => "FM from Sub",
        }
    }
}

/// A filter's model. The first ones are circuits, modelled to sing and distort as they do.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PolyFilter {
    /// Off: passes the sound.
    #[default]
    None,
    /// Four transistor stages, fat and singing; closes the bass as it resonates.
    Ladder,
    /// The ladder's two-stage tap: rounder, less steep.
    Ladder2,
    /// Four cleaner stages, as polysynths use: glassy resonance.
    Cascade,
    /// A diode ladder: rubbery, squelchy, the acid box's.
    Diode,
    /// Two poles and a clipping feedback path: snarling, as small Japanese monos do.
    Snarl,
    /// A clean state-variable filter.
    Low12,
    Low24,
    High12,
    High24,
    Band12,
    Band24,
    Notch,
    Peak,
    /// Tuned delay: metallic, plucked.
    CombUp,
    CombDown,
    /// A voice, sweeping through its vowels.
    Formant,
    /// Sweeping notches.
    Phaser,
    /// Ring-modulated at the cutoff.
    Ring,
    /// Held and let go at the cutoff: digital crunch.
    Crush,
}

impl PolyFilter {
    pub const ALL: [PolyFilter; 20] = [
        PolyFilter::None,
        PolyFilter::Ladder,
        PolyFilter::Ladder2,
        PolyFilter::Cascade,
        PolyFilter::Diode,
        PolyFilter::Snarl,
        PolyFilter::Low12,
        PolyFilter::Low24,
        PolyFilter::High12,
        PolyFilter::High24,
        PolyFilter::Band12,
        PolyFilter::Band24,
        PolyFilter::Notch,
        PolyFilter::Peak,
        PolyFilter::CombUp,
        PolyFilter::CombDown,
        PolyFilter::Formant,
        PolyFilter::Phaser,
        PolyFilter::Ring,
        PolyFilter::Crush,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PolyFilter::None => "Off",
            PolyFilter::Ladder => "Ladder 24",
            PolyFilter::Ladder2 => "Ladder 12",
            PolyFilter::Cascade => "Cascade",
            PolyFilter::Diode => "Diode",
            PolyFilter::Snarl => "Snarl",
            PolyFilter::Low12 => "Low 12",
            PolyFilter::Low24 => "Low 24",
            PolyFilter::High12 => "High 12",
            PolyFilter::High24 => "High 24",
            PolyFilter::Band12 => "Band 12",
            PolyFilter::Band24 => "Band 24",
            PolyFilter::Notch => "Notch",
            PolyFilter::Peak => "Peak",
            PolyFilter::CombUp => "Comb +",
            PolyFilter::CombDown => "Comb -",
            PolyFilter::Formant => "Formant",
            PolyFilter::Phaser => "Phaser",
            PolyFilter::Ring => "Ring",
            PolyFilter::Crush => "Crush",
        }
    }
}

/// How the two filters sit.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FilterRouting {
    /// One into the other.
    #[default]
    Series,
    /// Side by side, added.
    Parallel,
    /// Each on its own side.
    Stereo,
}

/// What an LFO's wave is.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LfoWave {
    #[default]
    Sine,
    Triangle,
    SawUp,
    SawDown,
    Square,
    /// A new value each cycle.
    Hold,
    /// New values, glided between.
    Smooth,
    /// Two strange attractors: never quite repeating, always related.
    Lorenz,
    Rossler,
    /// Drawn: (where, value) points, 0 to 1 each way, in order.
    Drawn(Vec<(f32, f32)>),
}

/// One of a synth's LFOs.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct PolyLfo {
    pub wave: LfoWave,
    /// A cycle in beats, with the song; 0 free, at its rate.
    pub beats: f32,
    /// Starts again on each note.
    pub retrigger: bool,
    /// Once through and stops: an envelope you draw.
    pub one_shot: bool,
}

/// What moves a parameter from within a voice.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolySource {
    Env(u8),
    Lfo(u8),
    Macro(u8),
    Velocity,
    /// The note played: 0 at C-2 to 1 at G8.
    Note,
    Wheel,
    Pressure,
    Bend,
    /// A new value each note.
    Random,
}

/// What a route moves: one of the synth's parameters, or a knob of an effect on the track, so a
/// synth's LFO can ride the reverb.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolyDest {
    Param(u16),
    Effect(u8, u8),
}

/// A line of the matrix: `source`, scaled by `by` if it's given, moves `dest` by `depth` of its
/// travel.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct PolyRoute {
    pub source: PolySource,
    pub dest: PolyDest,
    pub depth: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<PolySource>,
}

/// How notes are played.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Voicing {
    pub polyphony: u8,
    pub mono: bool,
    /// Mono: a note under a held one slides, its envelopes running on.
    pub legato: bool,
    /// Glide only between held notes.
    pub glide_held: bool,
    pub bend_semitones: f32,
}

impl Default for Voicing {
    fn default() -> Self {
        Voicing {
            polyphony: 16,
            mono: false,
            legato: false,
            glide_held: false,
            bend_semitones: 2.0,
        }
    }
}

/// How hard it works: how many times over it runs its voices, for less aliasing and filters that
/// behave at their extremes.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PolyQuality {
    Draft,
    #[default]
    Good,
    Best,
}

impl PolyQuality {
    pub fn times(self) -> usize {
        match self {
            PolyQuality::Draft => 1,
            PolyQuality::Good => 2,
            PolyQuality::Best => 4,
        }
    }
}

/// The arpeggiator.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ArpMode {
    #[default]
    Off,
    Up,
    Down,
    UpDown,
    Played,
    Random,
    Chord,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Arp {
    pub mode: ArpMode,
    /// A step, in beats.
    pub beats: f32,
    pub octaves: u8,
    /// How much of a step each note lasts.
    pub gate: f32,
    /// Every other step late, 0 to 0.75.
    pub swing: f32,
}

impl Default for Arp {
    fn default() -> Self {
        Arp {
            mode: ArpMode::Off,
            beats: 0.25,
            octaves: 1,
            gate: 0.8,
            swing: 0.0,
        }
    }
}

/// A synth.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Poly {
    /// The patch it came from, for the window's title.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub patch: String,
    pub params: PolyValues,
    /// Each oscillator's wavetable, by name.
    pub tables: [String; OSCS],
    pub warps: [[Warp; 2]; OSCS],
    /// Which filters each oscillator goes through: 1, 2 or both (the sub and noise after them).
    pub to_filter: [[bool; FILTERS]; OSCS + 2],
    pub filters: [PolyFilter; FILTERS],
    pub routing: FilterRouting,
    /// The sub's wave: 0 sine, 1 triangle, 2 saw, 3 square; how many octaves down.
    pub sub_wave: u8,
    pub sub_octave: u8,
    pub lfos: [PolyLfo; LFOS],
    pub routes: Vec<PolyRoute>,
    pub voicing: Voicing,
    pub arp: Arp,
    pub quality: PolyQuality,
}

impl Default for Poly {
    fn default() -> Self {
        Poly {
            patch: String::new(),
            params: PolyValues::default(),
            tables: ["Basic".into(), "Basic".into(), "Basic".into()],
            warps: [[Warp::None; 2]; OSCS],
            to_filter: [[true, false]; OSCS + 2],
            filters: [PolyFilter::None; FILTERS],
            routing: FilterRouting::Series,
            sub_wave: 0,
            sub_octave: 1,
            lfos: Default::default(),
            routes: Vec::new(),
            voicing: Voicing::default(),
            arp: Arp::default(),
            quality: PolyQuality::default(),
        }
    }
}

impl Poly {
    /// Each parameter's place on its knob, 0 to 1.
    pub fn positions(&self) -> Vec<f32> {
        POLY_PARAMS
            .iter()
            .zip(&self.params.0)
            .map(|(p, v)| p.scale.pos(*v))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_parameter_has_a_name_a_place_and_a_way_back() {
        assert_eq!(POLY_PARAMS.len(), PARAM_COUNT);
        let mut ids: Vec<&str> = POLY_PARAMS.iter().map(|p| p.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), PARAM_COUNT, "an id is used twice");
        for (i, p) in POLY_PARAMS.iter().enumerate() {
            assert_eq!(poly_param(&p.id), Some(i), "{}", p.id);
            let back = p.scale.value(p.scale.pos(p.default));
            assert!(
                (back - p.default).abs() < 1e-3 * p.default.abs().max(1.0),
                "{}",
                p.id
            );
            assert!(!p.say(p.default).is_empty());
        }
        // The layout names what it says it does.
        assert_eq!(
            POLY_PARAMS[osc_param(1, OscField::Position)].id,
            "osc2_position"
        );
        assert_eq!(
            POLY_PARAMS[filter_param(1, FilterField::Drive)].id,
            "filter2_drive"
        );
        assert_eq!(POLY_PARAMS[env_param(3, EnvField::Curve)].id, "env4_curve");
        assert_eq!(
            POLY_PARAMS[lfo_param(7, LfoField::Smooth)].id,
            "lfo8_smooth"
        );
        assert_eq!(POLY_PARAMS[macro_param(7)].id, "macro8");
        assert_eq!(POLY_PARAMS[POLY_MASTER].id, "master");
    }

    #[test]
    fn a_synth_saves_only_what_it_changed_and_comes_back_whole() {
        let mut p = Poly::default();
        p.params[filter_param(0, FilterField::Cutoff)] = 800.0;
        p.filters[0] = PolyFilter::Ladder;
        p.warps[1][0] = Warp::Sync;
        p.routes.push(PolyRoute {
            source: PolySource::Env(1),
            dest: PolyDest::Param(filter_param(0, FilterField::Cutoff) as u16),
            depth: 0.5,
            by: None,
        });
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["params"].as_object().unwrap().len(), 1, "{json}");
        let back: Poly = serde_json::from_value(json).unwrap();
        assert_eq!(back, p);
        // Something from a later version is passed over.
        let v = serde_json::json!({ "params": { "osc1_wobble": 3.0, "master": -3.0 } });
        let later: Poly = serde_json::from_value(v).unwrap();
        assert_eq!(later.params[POLY_MASTER], -3.0);
    }
}
