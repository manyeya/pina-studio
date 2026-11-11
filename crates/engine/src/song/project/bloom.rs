//! Bloom: sounds grown from a seed. A seed is 48 genes, each 0 to 1, made to sound good when
//! they're changed at random. Twelve branches grow out of it, each its own way through the genes:
//! the further one grows the further its sound strays from the seed, and bent sideways it strays
//! another way at the same distance. A note plays a branch, picked by the keys' mode.
//!
//! Everything that moves smoothly is one number in `params`, its place in `BLOOM_PARAMS`: the
//! plant's own controls, each branch's length and bend, then the seed's genes. So all of it is
//! automatable by being here. `grown` works a branch's genes out, the same for the engine and the
//! window, so what's drawn is what's heard.

use std::{collections::BTreeMap, f32::consts::TAU, sync::LazyLock};

use super::*;

/// What it's called, everywhere it's shown.
pub const BLOOM_NAME: &str = "Bloom";
pub const GENES: usize = 48;
pub const BRANCHES: usize = 12;
/// The most ancestors a plant remembers.
pub const ANCESTORS: usize = 24;

/// The three parts of the strand, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneGroup {
    Motion,
    Tone,
    Space,
}

impl GeneGroup {
    pub const ALL: [GeneGroup; 3] = [GeneGroup::Motion, GeneGroup::Tone, GeneGroup::Space];

    pub fn name(self) -> &'static str {
        match self {
            GeneGroup::Motion => "Envelope & LFO",
            GeneGroup::Tone => "Oscillators",
            GeneGroup::Space => "Filter & Effects",
        }
    }

    /// Its genes' places.
    pub fn genes(self) -> std::ops::Range<usize> {
        match self {
            GeneGroup::Motion => 0..16,
            GeneGroup::Tone => 16..32,
            GeneGroup::Space => 32..48,
        }
    }
}

/// How a gene's 0 to 1 becomes what it sets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GeneShape {
    /// ms, evenly by octaves.
    Time(f32, f32),
    Hz(f32, f32),
    Unit,
    /// -1 to 1.
    Signed,
    /// Semitones either way, a still middle so most seeds have none.
    Semis(f32),
    /// Octaves either way, likewise.
    Octaves(f32),
    Cents(f32),
    /// A frequency ratio, 0.25 to 9.
    Ratio,
    /// One of these, chosen when a note starts.
    Steps(&'static [&'static str]),
    /// Swept through these, smoothly.
    Morph(&'static [&'static str]),
}

/// Either way from the middle, with a still zone there, squared so small moves stay small.
fn bipolar(g: f32) -> f32 {
    let x = 2.0 * g.clamp(0.0, 1.0) - 1.0;
    let m = ((x.abs() - 0.12).max(0.0) / 0.88).min(1.0);
    x.signum() * m * m
}

impl GeneShape {
    pub fn value(self, g: f32) -> f32 {
        let g = g.clamp(0.0, 1.0);
        match self {
            GeneShape::Time(lo, hi) | GeneShape::Hz(lo, hi) => lo * (hi / lo).powf(g),
            GeneShape::Unit | GeneShape::Morph(_) => g,
            GeneShape::Signed => 2.0 * g - 1.0,
            GeneShape::Semis(r) | GeneShape::Octaves(r) => bipolar(g) * r,
            GeneShape::Cents(r) => g * g * r,
            GeneShape::Ratio => 0.25 * 36f32.powf(g),
            GeneShape::Steps(names) => {
                let n = names.len();
                ((g * n as f32) as usize).min(n - 1) as f32
            }
        }
    }

    pub fn say(self, g: f32) -> String {
        let v = self.value(g);
        match self {
            GeneShape::Time(..) if v >= 1000.0 => format!("{:.2} s", v / 1000.0),
            GeneShape::Time(..) => format!("{v:.0} ms"),
            GeneShape::Hz(..) if v >= 1000.0 => format!("{:.2} kHz", v / 1000.0),
            GeneShape::Hz(..) if v < 10.0 => format!("{v:.2} Hz"),
            GeneShape::Hz(..) => format!("{v:.0} Hz"),
            GeneShape::Unit => format!("{:.0} %", v * 100.0),
            GeneShape::Signed => format!("{:+.0} %", v * 100.0),
            GeneShape::Semis(_) => format!("{v:+.1} st"),
            GeneShape::Octaves(_) => format!("{v:+.2} oct"),
            GeneShape::Cents(_) => format!("{v:.0} ct"),
            GeneShape::Ratio => format!("×{v:.2}"),
            GeneShape::Steps(names) => names[v as usize].to_string(),
            GeneShape::Morph(names) => {
                let at = (v * (names.len() - 1) as f32).round() as usize;
                names[at.min(names.len() - 1)].to_string()
            }
        }
    }
}

/// One gene.
pub struct Gene {
    /// What it's saved and asked for by.
    pub id: &'static str,
    pub name: &'static str,
    pub group: GeneGroup,
    /// What it does, in a line, for the strand's help.
    pub about: &'static str,
    pub shape: GeneShape,
    /// How far it strays as a branch grows, against the others.
    pub weight: f32,
    /// Tuning: Atonality decides how far it strays and whether it keeps to musical steps.
    pub pitch: bool,
    /// The whole plant's, not a branch's: the shared echo and space. Branches don't change it.
    pub plant: bool,
}

/// The wavetables an oscillator gene picks among.
pub const BLOOM_TABLES: [&str; 12] = [
    "Basic",
    "Analog",
    "Harmonics",
    "Soft Saw",
    "Formant",
    "Organ",
    "Bell",
    "Glass",
    "Digital",
    "Metal",
    "Air",
    "Pluck",
];
/// The echo's times, beats as a fraction of a bar of 4.
pub const ECHO_TIMES: [&str; 6] = ["1/16", "1/8", "3/16", "1/4", "3/8", "1/2"];
pub const ECHO_BEATS: [f32; 6] = [0.25, 0.5, 0.75, 1.0, 1.5, 2.0];

const fn gene(
    id: &'static str,
    name: &'static str,
    group: GeneGroup,
    shape: GeneShape,
    weight: f32,
    about: &'static str,
) -> Gene {
    Gene {
        id,
        name,
        group,
        about,
        shape,
        weight,
        pitch: false,
        plant: false,
    }
}

const fn tuned(g: Gene) -> Gene {
    Gene { pitch: true, ..g }
}

const fn whole(g: Gene) -> Gene {
    Gene { plant: true, ..g }
}

use GeneGroup::{Motion, Space, Tone};

const LFO_SHAPES: &[&str] = &["Sine", "Triangle", "Square", "Random"];
const FILTER_MODES: &[&str] = &["Low-pass", "Band-pass", "High-pass"];

/// Every gene, in its place on the strand.
pub static GENE_TABLE: [Gene; GENES] = [
    gene(
        "attack",
        "Attack",
        Motion,
        GeneShape::Time(0.5, 4000.0),
        1.0,
        "How long the sound takes to arrive.",
    ),
    gene(
        "decay",
        "Decay",
        Motion,
        GeneShape::Time(5.0, 8000.0),
        1.0,
        "How long it takes to fall to its sustain.",
    ),
    gene(
        "sustain",
        "Sustain",
        Motion,
        GeneShape::Unit,
        0.9,
        "How loud it stays while the key is held.",
    ),
    gene(
        "release",
        "Release",
        Motion,
        GeneShape::Time(5.0, 8000.0),
        0.9,
        "How long it rings once the key is let go.",
    ),
    gene(
        "contour_attack",
        "Contour Attack",
        Motion,
        GeneShape::Time(0.5, 4000.0),
        0.9,
        "The contour's rise: a second envelope that bends pitch, filter and FM.",
    ),
    gene(
        "contour_decay",
        "Contour Decay",
        Motion,
        GeneShape::Time(5.0, 8000.0),
        1.0,
        "How long the contour takes to fall back.",
    ),
    tuned(gene(
        "contour_pitch",
        "Contour → Pitch",
        Motion,
        GeneShape::Semis(48.0),
        0.6,
        "How far the contour bends the pitch: the drop of a kick, the chirp of a zap.",
    )),
    gene(
        "contour_filter",
        "Contour → Filter",
        Motion,
        GeneShape::Octaves(6.0),
        1.0,
        "How far the contour opens or closes the filter.",
    ),
    gene(
        "contour_fm",
        "Contour → FM",
        Motion,
        GeneShape::Unit,
        0.8,
        "How much the contour adds to the FM: a bright, metallic start.",
    ),
    gene(
        "lfo_rate",
        "LFO Rate",
        Motion,
        GeneShape::Hz(0.05, 30.0),
        0.9,
        "How fast the LFO moves.",
    ),
    gene(
        "lfo_shape",
        "LFO Shape",
        Motion,
        GeneShape::Morph(LFO_SHAPES),
        0.7,
        "The LFO's wave, swept from sine through triangle and square to random steps.",
    ),
    tuned(gene(
        "lfo_pitch",
        "LFO → Pitch",
        Motion,
        GeneShape::Unit,
        0.5,
        "Vibrato: how far the LFO moves the pitch.",
    )),
    gene(
        "lfo_filter",
        "LFO → Filter",
        Motion,
        GeneShape::Unit,
        0.8,
        "How far the LFO sweeps the filter.",
    ),
    gene(
        "lfo_amp",
        "LFO → Level",
        Motion,
        GeneShape::Unit,
        0.6,
        "Tremolo: how much the LFO moves the level.",
    ),
    gene(
        "lfo_pan",
        "LFO → Pan",
        Motion,
        GeneShape::Unit,
        0.6,
        "How far the LFO swings the sound side to side.",
    ),
    gene(
        "lfo_rise",
        "LFO Rise",
        Motion,
        GeneShape::Time(1.0, 3000.0),
        0.7,
        "How long the LFO takes to fade in after a note starts.",
    ),
    gene(
        "a_wave",
        "A Wave",
        Tone,
        GeneShape::Unit,
        1.0,
        "Where oscillator A is through its wavetable.",
    ),
    gene(
        "a_table",
        "A Table",
        Tone,
        GeneShape::Steps(&BLOOM_TABLES),
        0.5,
        "The wavetable oscillator A plays.",
    ),
    gene(
        "b_wave",
        "B Wave",
        Tone,
        GeneShape::Unit,
        1.0,
        "Where oscillator B is through its wavetable.",
    ),
    gene(
        "b_table",
        "B Table",
        Tone,
        GeneShape::Steps(&BLOOM_TABLES),
        0.5,
        "The wavetable oscillator B plays.",
    ),
    tuned(gene(
        "b_ratio",
        "B Ratio",
        Tone,
        GeneShape::Ratio,
        0.6,
        "Oscillator B's pitch against A's: whole ratios while it's tonal, anything as it grows atonal.",
    )),
    tuned(gene(
        "b_detune",
        "B Detune",
        Tone,
        GeneShape::Cents(30.0),
        0.7,
        "How far B drifts from its ratio: a slow beating, or a sour one.",
    )),
    gene(
        "balance",
        "Balance",
        Tone,
        GeneShape::Unit,
        1.0,
        "How much is A and how much is B.",
    ),
    gene(
        "fm",
        "FM",
        Tone,
        GeneShape::Unit,
        0.9,
        "How hard B frequency-modulates A: from warm to clanging.",
    ),
    gene(
        "feedback",
        "Feedback",
        Tone,
        GeneShape::Unit,
        0.6,
        "A modulating itself: a sine turned to a saw, then to noise.",
    ),
    gene(
        "noise",
        "Noise",
        Tone,
        GeneShape::Unit,
        0.7,
        "How much noise is mixed in.",
    ),
    gene(
        "noise_color",
        "Noise Color",
        Tone,
        GeneShape::Unit,
        0.8,
        "The noise from dark rumble to bright hiss.",
    ),
    tuned(gene(
        "pitch",
        "Pitch",
        Tone,
        GeneShape::Semis(24.0),
        0.4,
        "The pitch, octaves while it's tonal, anything as it grows atonal.",
    )),
    gene(
        "unison",
        "Unison",
        Tone,
        GeneShape::Unit,
        0.8,
        "Oscillator A three times over, detuned: thicker.",
    ),
    gene(
        "fold",
        "Fold",
        Tone,
        GeneShape::Unit,
        0.7,
        "The waves folded back on themselves: harsher, richer.",
    ),
    gene(
        "ring",
        "Ring",
        Tone,
        GeneShape::Unit,
        0.6,
        "A times B: bells, robots and clangs.",
    ),
    gene(
        "sub",
        "Sub",
        Tone,
        GeneShape::Unit,
        0.7,
        "A sine an octave under, for weight.",
    ),
    gene(
        "cutoff",
        "Cutoff",
        Space,
        GeneShape::Hz(20.0, 20_000.0),
        1.0,
        "Where the filter cuts.",
    ),
    gene(
        "resonance",
        "Resonance",
        Space,
        GeneShape::Unit,
        0.8,
        "How much the filter rings at its cutoff.",
    ),
    gene(
        "filter_mode",
        "Filter Mode",
        Space,
        GeneShape::Morph(FILTER_MODES),
        0.7,
        "The filter swept from low-pass through band-pass to high-pass.",
    ),
    gene(
        "key_track",
        "Key Track",
        Space,
        GeneShape::Unit,
        0.6,
        "How far the cutoff follows the keys.",
    ),
    gene(
        "velocity_filter",
        "Velocity → Filter",
        Space,
        GeneShape::Unit,
        0.6,
        "How much harder notes open the filter.",
    ),
    gene(
        "velocity_level",
        "Velocity → Level",
        Space,
        GeneShape::Unit,
        0.5,
        "How much softer notes are quieter.",
    ),
    gene(
        "drive",
        "Drive",
        Space,
        GeneShape::Unit,
        0.8,
        "How hard the filter is pushed: warm, then gritty.",
    ),
    tuned(gene(
        "resonator",
        "Resonator",
        Space,
        GeneShape::Unit,
        0.6,
        "A string tuned to the note, ringing with the sound: plucked, metallic, alive.",
    )),
    gene(
        "pan",
        "Pan",
        Space,
        GeneShape::Signed,
        0.6,
        "Where it sits, left to right.",
    ),
    gene(
        "width",
        "Width",
        Space,
        GeneShape::Unit,
        0.8,
        "How wide the unison spreads.",
    ),
    gene(
        "tilt",
        "Tilt",
        Space,
        GeneShape::Signed,
        0.8,
        "The tone tilted dark or bright.",
    ),
    gene(
        "crush",
        "Crush",
        Space,
        GeneShape::Unit,
        0.35,
        "Fewer bits and a lower rate: digital grit.",
    ),
    gene(
        "echo",
        "Echo",
        Space,
        GeneShape::Unit,
        0.6,
        "How much goes to the plant's echo.",
    ),
    gene(
        "space",
        "Space",
        Space,
        GeneShape::Unit,
        0.6,
        "How much goes to the plant's reverb.",
    ),
    whole(gene(
        "echo_time",
        "Echo Time",
        Space,
        GeneShape::Steps(&ECHO_TIMES),
        0.0,
        "The echo's time, in time with the song. The whole plant's.",
    )),
    whole(gene(
        "space_size",
        "Space Size",
        Space,
        GeneShape::Unit,
        0.0,
        "How big and long the reverb is. The whole plant's.",
    )),
];

/// A gene's place, by its id or its name (any case).
pub fn gene_index(name: &str) -> Option<usize> {
    GENE_TABLE
        .iter()
        .position(|g| g.id.eq_ignore_ascii_case(name) || g.name.eq_ignore_ascii_case(name))
}

pub fn say_gene(i: usize, g: f32) -> String {
    GENE_TABLE
        .get(i)
        .map_or_else(String::new, |gene| gene.shape.say(g))
}

/// Where the plant's controls are among its parameters.
pub const BLOOM_GROWTH: usize = 0;
/// How far the branches stray as they grow.
pub const BLOOM_WILD: usize = 1;
pub const BLOOM_ATONAL: usize = 2;
/// How much of the echo and space is heard.
pub const BLOOM_EFFECT: usize = 3;
pub const BLOOM_TUNE: usize = 4;
pub const BLOOM_VOLUME: usize = 5;
/// The release lengthened or shortened, every branch's.
pub const BLOOM_RELEASE: usize = 6;
pub const BLOOM_GLIDE: usize = 7;
/// How far the mod wheel, and each note's own slide, grow the branches.
pub const BLOOM_WHEEL: usize = 8;
pub const BLOOM_SLIDE: usize = 9;
const LENGTH_BASE: usize = 10;
const BEND_BASE: usize = LENGTH_BASE + BRANCHES;
const GENE_BASE: usize = BEND_BASE + BRANCHES;
pub const BLOOM_PARAM_COUNT: usize = GENE_BASE + GENES;

pub const fn branch_length(b: usize) -> usize {
    LENGTH_BASE + b
}
pub const fn branch_bend(b: usize) -> usize {
    BEND_BASE + b
}
/// A seed gene's place among the parameters.
pub const fn gene_param(i: usize) -> usize {
    GENE_BASE + i
}
/// The gene a parameter is, if it's one.
pub const fn param_gene(i: usize) -> Option<usize> {
    match i >= GENE_BASE && i < BLOOM_PARAM_COUNT {
        true => Some(i - GENE_BASE),
        false => None,
    }
}

/// The notes' names round the plant, C at the top.
pub const BRANCH_NAMES: [&str; BRANCHES] = [
    "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
];

const UNIT: KnobScale = KnobScale::Lin(0.0, 1.0);
const SWING: KnobScale = KnobScale::Lin(-1.0, 1.0);

/// Every parameter, in its place. Genes are on knobs 0 to 1 and said in their own units.
pub static BLOOM_PARAMS: LazyLock<Vec<PolyParam>> = LazyLock::new(|| {
    let mut out = Vec::with_capacity(BLOOM_PARAM_COUNT);
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
    let plant = "Plant";
    add(
        "growth".into(),
        "Growth".into(),
        plant,
        UNIT,
        0.0,
        Reads::Percent,
        false,
    );
    add(
        "wildness".into(),
        "Wildness".into(),
        plant,
        UNIT,
        0.5,
        Reads::Percent,
        false,
    );
    add(
        "atonality".into(),
        "Atonality".into(),
        plant,
        UNIT,
        0.2,
        Reads::Percent,
        false,
    );
    add(
        "effect".into(),
        "Effect".into(),
        plant,
        UNIT,
        0.5,
        Reads::Percent,
        false,
    );
    let tune = KnobScale::Lin(-24.0, 24.0);
    add(
        "tune".into(),
        "Tune".into(),
        plant,
        tune,
        0.0,
        Reads::Semitones,
        true,
    );
    let volume = KnobScale::Lin(-48.0, 12.0);
    add(
        "volume".into(),
        "Volume".into(),
        plant,
        volume,
        -3.0,
        Reads::Db,
        true,
    );
    add(
        "release".into(),
        "Release".into(),
        plant,
        SWING,
        0.0,
        Reads::Signed,
        true,
    );
    let glide = KnobScale::Log(0.1, 5000.0);
    add(
        "glide".into(),
        "Glide".into(),
        plant,
        glide,
        0.1,
        Reads::Ms,
        false,
    );
    add(
        "wheel_growth".into(),
        "Wheel Growth".into(),
        plant,
        UNIT,
        1.0,
        Reads::Percent,
        false,
    );
    add(
        "slide_growth".into(),
        "Slide Growth".into(),
        plant,
        UNIT,
        0.6,
        Reads::Percent,
        false,
    );
    for (b, n) in BRANCH_NAMES.iter().enumerate() {
        add(
            format!("branch{}_length", b + 1),
            format!("{n} Branch Length"),
            "Branches",
            UNIT,
            0.0,
            Reads::Percent,
            false,
        );
    }
    for (b, n) in BRANCH_NAMES.iter().enumerate() {
        add(
            format!("branch{}_bend", b + 1),
            format!("{n} Branch Bend"),
            "Branches",
            SWING,
            0.0,
            Reads::Signed,
            true,
        );
    }
    for (i, g) in GENE_TABLE.iter().enumerate() {
        add(
            format!("gene_{}", g.id),
            g.name.to_string(),
            g.group.name(),
            UNIT,
            NEUTRAL[i],
            Reads::Gene(i as u8),
            false,
        );
    }
    out
});

/// A parameter's place, by its id or its name (any case).
pub fn bloom_param(name: &str) -> Option<usize> {
    BLOOM_PARAMS
        .iter()
        .position(|p| p.id.eq_ignore_ascii_case(name) || p.name.eq_ignore_ascii_case(name))
}

/// A plant's parameters, in their own units. Saved by id, those at their defaults left out.
#[derive(Clone, Debug, PartialEq)]
pub struct BloomValues(pub Vec<f32>);

impl Default for BloomValues {
    fn default() -> Self {
        BloomValues(BLOOM_PARAMS.iter().map(|p| p.default).collect())
    }
}

impl std::ops::Index<usize> for BloomValues {
    type Output = f32;

    fn index(&self, i: usize) -> &f32 {
        &self.0[i]
    }
}

impl std::ops::IndexMut<usize> for BloomValues {
    fn index_mut(&mut self, i: usize) -> &mut f32 {
        &mut self.0[i]
    }
}

impl Serialize for BloomValues {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let changed: BTreeMap<&str, f32> = BLOOM_PARAMS
            .iter()
            .zip(&self.0)
            .filter(|(p, v)| p.default != **v)
            .map(|(p, v)| (p.id.as_str(), *v))
            .collect();
        changed.serialize(s)
    }
}

impl<'de> Deserialize<'de> for BloomValues {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let saved = BTreeMap::<String, f32>::deserialize(d)?;
        let mut out = BloomValues::default();
        for (id, v) in saved {
            // One from a later version is left out, not an error.
            if let Some(i) = bloom_param(&id) {
                out[i] = v;
            }
        }
        Ok(out)
    }
}

/// How notes pick their branch.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BloomKeys {
    /// Each note its own branch, round the octave: every C plays the top one.
    #[default]
    Octave,
    /// The keyboard in twelve zones, low to high.
    Spread,
    /// Softest to hardest, round the plant.
    Velocity,
    /// The branches in the layer, all at once.
    Layer,
    /// Each note the next grown branch: no two hits the same.
    Cycle,
    /// The branch picked, on every key.
    One,
}

impl BloomKeys {
    pub const ALL: [BloomKeys; 6] = [
        BloomKeys::Octave,
        BloomKeys::Spread,
        BloomKeys::Velocity,
        BloomKeys::Layer,
        BloomKeys::Cycle,
        BloomKeys::One,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BloomKeys::Octave => "Octave",
            BloomKeys::Spread => "Spread",
            BloomKeys::Velocity => "Velocity",
            BloomKeys::Layer => "Layer",
            BloomKeys::Cycle => "Cycle",
            BloomKeys::One => "One",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            BloomKeys::Octave => "Each note of the octave plays its own branch",
            BloomKeys::Spread => "Twelve zones up the keyboard, a branch each",
            BloomKeys::Velocity => "Harder notes play branches further round",
            BloomKeys::Layer => "The branches in the layer play together",
            BloomKeys::Cycle => "Every note plays the next grown branch",
            BloomKeys::One => "The picked branch on every key",
        }
    }
}

/// What a seed was planted as: what its genes were drawn around.
#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash, PartialOrd, Ord,
)]
#[serde(rename_all = "snake_case")]
pub enum SeedKind {
    #[default]
    Any,
    Bass,
    Sub,
    Reese,
    Acid,
    Keys,
    Organ,
    Bell,
    Mallet,
    Pluck,
    Harp,
    Strings,
    Stab,
    Lead,
    Brass,
    Flute,
    Choir,
    Pad,
    Drone,
    Atmosphere,
    Swell,
    Kick,
    Snare,
    Hat,
    Perc,
    Texture,
    Riser,
    Zap,
    Glitch,
}

/// The kinds, gathered as they're offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedFamily {
    Low,
    Keys,
    Plucked,
    Voices,
    Spaces,
    Drums,
    Effects,
}

impl SeedFamily {
    pub const ALL: [SeedFamily; 7] = [
        SeedFamily::Low,
        SeedFamily::Keys,
        SeedFamily::Plucked,
        SeedFamily::Voices,
        SeedFamily::Spaces,
        SeedFamily::Drums,
        SeedFamily::Effects,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SeedFamily::Low => "Low end",
            SeedFamily::Keys => "Keys & bells",
            SeedFamily::Plucked => "Plucked & bowed",
            SeedFamily::Voices => "Leads & voices",
            SeedFamily::Spaces => "Pads & spaces",
            SeedFamily::Drums => "Drums",
            SeedFamily::Effects => "Effects",
        }
    }

    /// Its kinds, in order.
    pub fn kinds(self) -> &'static [SeedKind] {
        use SeedKind::*;
        match self {
            SeedFamily::Low => &[Bass, Sub, Reese, Acid],
            SeedFamily::Keys => &[Keys, Organ, Bell, Mallet],
            SeedFamily::Plucked => &[Pluck, Harp, Strings, Stab],
            SeedFamily::Voices => &[Lead, Brass, Flute, Choir],
            SeedFamily::Spaces => &[Pad, Drone, Atmosphere, Swell],
            SeedFamily::Drums => &[Kick, Snare, Hat, Perc],
            SeedFamily::Effects => &[Texture, Riser, Zap, Glitch],
        }
    }

    /// Where its colour sits round the wheel, 0 to 1.
    pub fn hue(self) -> f32 {
        match self {
            SeedFamily::Low => 0.72,
            SeedFamily::Keys => 0.13,
            SeedFamily::Plucked => 0.33,
            SeedFamily::Voices => 0.0,
            SeedFamily::Spaces => 0.53,
            SeedFamily::Drums => 0.06,
            SeedFamily::Effects => 0.86,
        }
    }
}

impl SeedKind {
    pub const ALL: [SeedKind; 29] = [
        SeedKind::Any,
        SeedKind::Bass,
        SeedKind::Sub,
        SeedKind::Reese,
        SeedKind::Acid,
        SeedKind::Keys,
        SeedKind::Organ,
        SeedKind::Bell,
        SeedKind::Mallet,
        SeedKind::Pluck,
        SeedKind::Harp,
        SeedKind::Strings,
        SeedKind::Stab,
        SeedKind::Lead,
        SeedKind::Brass,
        SeedKind::Flute,
        SeedKind::Choir,
        SeedKind::Pad,
        SeedKind::Drone,
        SeedKind::Atmosphere,
        SeedKind::Swell,
        SeedKind::Kick,
        SeedKind::Snare,
        SeedKind::Hat,
        SeedKind::Perc,
        SeedKind::Texture,
        SeedKind::Riser,
        SeedKind::Zap,
        SeedKind::Glitch,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SeedKind::Any => "Anything",
            SeedKind::Bass => "Bass",
            SeedKind::Sub => "Sub",
            SeedKind::Reese => "Reese",
            SeedKind::Acid => "Acid",
            SeedKind::Keys => "Keys",
            SeedKind::Organ => "Organ",
            SeedKind::Bell => "Bell",
            SeedKind::Mallet => "Mallet",
            SeedKind::Pluck => "Pluck",
            SeedKind::Harp => "Harp",
            SeedKind::Strings => "Strings",
            SeedKind::Stab => "Stab",
            SeedKind::Lead => "Lead",
            SeedKind::Brass => "Brass",
            SeedKind::Flute => "Flute",
            SeedKind::Choir => "Choir",
            SeedKind::Pad => "Pad",
            SeedKind::Drone => "Drone",
            SeedKind::Atmosphere => "Atmosphere",
            SeedKind::Swell => "Swell",
            SeedKind::Kick => "Kick",
            SeedKind::Snare => "Snare",
            SeedKind::Hat => "Hat",
            SeedKind::Perc => "Percussion",
            SeedKind::Texture => "Texture",
            SeedKind::Riser => "Riser",
            SeedKind::Zap => "Zap",
            SeedKind::Glitch => "Glitch",
        }
    }

    /// What it's like, in a few words, for the picker.
    pub fn about(self) -> &'static str {
        match self {
            SeedKind::Any => "anything at all",
            SeedKind::Bass => "round, played low",
            SeedKind::Sub => "a pure, deep weight",
            SeedKind::Reese => "detuned, moving, wide",
            SeedKind::Acid => "squelching resonance",
            SeedKind::Keys => "struck, warm, electric",
            SeedKind::Organ => "drawbars, held",
            SeedKind::Bell => "clear FM partials",
            SeedKind::Mallet => "wood and metal, struck",
            SeedKind::Pluck => "short and bright",
            SeedKind::Harp => "a ringing string",
            SeedKind::Strings => "bowed, slow, wide",
            SeedKind::Stab => "a chord's punch",
            SeedKind::Lead => "singing, in front",
            SeedKind::Brass => "a swelling blare",
            SeedKind::Flute => "breathy and soft",
            SeedKind::Choir => "voices, vowels",
            SeedKind::Pad => "wide and held",
            SeedKind::Drone => "endless, low",
            SeedKind::Atmosphere => "air and distance",
            SeedKind::Swell => "slowly rising",
            SeedKind::Kick => "a pitched drop",
            SeedKind::Snare => "a crack of noise",
            SeedKind::Hat => "metallic, tight",
            SeedKind::Perc => "toms, clicks, knocks",
            SeedKind::Texture => "noise, moving",
            SeedKind::Riser => "building up",
            SeedKind::Zap => "a laser chirp",
            SeedKind::Glitch => "broken and crushed",
        }
    }

    pub fn family(self) -> Option<SeedFamily> {
        SeedFamily::ALL
            .into_iter()
            .find(|f| f.kinds().contains(&self))
    }

    /// The genes it's drawn around (gene, value), and how far each strays from them.
    fn center(self) -> (&'static [(usize, f32)], f32) {
        match self {
            SeedKind::Any => (&[], 0.42),
            SeedKind::Bass => (
                &[
                    (0, 0.04),
                    (1, 0.42),
                    (2, 0.7),
                    (3, 0.22),
                    (5, 0.35),
                    (7, 0.72),
                    (16, 0.66),
                    (27, 0.13),
                    (31, 0.5),
                    (32, 0.48),
                    (33, 0.3),
                    (41, 0.1),
                    (44, 0.0),
                    (45, 0.03),
                ],
                0.2,
            ),
            SeedKind::Sub => (
                &[
                    (0, 0.1),
                    (1, 0.6),
                    (2, 0.9),
                    (3, 0.3),
                    (16, 0.0),
                    (17, 0.04),
                    (22, 0.0),
                    (23, 0.0),
                    (25, 0.0),
                    (27, 0.13),
                    (28, 0.0),
                    (31, 0.6),
                    (32, 0.5),
                    (33, 0.05),
                    (44, 0.0),
                    (45, 0.02),
                ],
                0.12,
            ),
            SeedKind::Reese => (
                &[
                    (0, 0.1),
                    (2, 0.9),
                    (3, 0.3),
                    (9, 0.25),
                    (12, 0.3),
                    (16, 0.66),
                    (17, 0.125),
                    (18, 0.66),
                    (19, 0.125),
                    (20, 0.387),
                    (21, 0.7),
                    (22, 0.5),
                    (27, 0.13),
                    (28, 0.55),
                    (31, 0.4),
                    (32, 0.55),
                    (33, 0.25),
                    (38, 0.45),
                    (41, 0.3),
                    (44, 0.0),
                    (45, 0.05),
                ],
                0.15,
            ),
            SeedKind::Acid => (
                &[
                    (0, 0.03),
                    (2, 0.7),
                    (3, 0.2),
                    (5, 0.45),
                    (7, 0.9),
                    (16, 0.66),
                    (17, 0.04),
                    (27, 0.13),
                    (32, 0.4),
                    (33, 0.85),
                    (34, 0.0),
                    (35, 0.3),
                    (36, 0.7),
                    (38, 0.55),
                    (44, 0.25),
                    (45, 0.05),
                ],
                0.15,
            ),
            SeedKind::Pluck => (
                &[
                    (0, 0.02),
                    (1, 0.38),
                    (2, 0.0),
                    (3, 0.36),
                    (5, 0.3),
                    (7, 0.8),
                    (32, 0.42),
                    (33, 0.35),
                    (39, 0.25),
                    (44, 0.2),
                    (45, 0.28),
                ],
                0.22,
            ),
            SeedKind::Keys => (
                &[
                    (0, 0.05),
                    (1, 0.62),
                    (2, 0.38),
                    (3, 0.45),
                    (17, 0.45),
                    (20, 0.39),
                    (23, 0.25),
                    (32, 0.75),
                    (36, 0.55),
                    (45, 0.25),
                ],
                0.22,
            ),
            SeedKind::Organ => (
                &[
                    (0, 0.12),
                    (1, 0.5),
                    (2, 1.0),
                    (3, 0.25),
                    (9, 0.7),
                    (11, 0.12),
                    (13, 0.15),
                    (16, 0.6),
                    (17, 0.458),
                    (18, 0.6),
                    (19, 0.458),
                    (20, 0.581),
                    (22, 0.2),
                    (32, 0.82),
                    (38, 0.3),
                    (45, 0.3),
                ],
                0.15,
            ),
            SeedKind::Mallet => (
                &[
                    (0, 0.02),
                    (1, 0.6),
                    (2, 0.0),
                    (3, 0.5),
                    (5, 0.3),
                    (8, 0.5),
                    (16, 0.0),
                    (18, 0.0),
                    (20, 0.774),
                    (22, 0.1),
                    (23, 0.35),
                    (32, 0.75),
                    (39, 0.3),
                    (45, 0.35),
                ],
                0.18,
            ),
            SeedKind::Harp => (
                &[
                    (0, 0.02),
                    (1, 0.62),
                    (2, 0.0),
                    (3, 0.55),
                    (5, 0.35),
                    (7, 0.78),
                    (16, 0.3),
                    (17, 0.958),
                    (32, 0.6),
                    (39, 0.55),
                    (41, 0.4),
                    (45, 0.45),
                ],
                0.18,
            ),
            SeedKind::Strings => (
                &[
                    (0, 0.66),
                    (1, 0.7),
                    (2, 0.9),
                    (3, 0.62),
                    (9, 0.7),
                    (11, 0.15),
                    (15, 0.75),
                    (16, 0.66),
                    (17, 0.125),
                    (21, 0.4),
                    (28, 0.55),
                    (32, 0.66),
                    (41, 0.7),
                    (42, 0.45),
                    (45, 0.5),
                ],
                0.15,
            ),
            SeedKind::Stab => (
                &[
                    (0, 0.03),
                    (1, 0.42),
                    (2, 0.25),
                    (3, 0.35),
                    (5, 0.4),
                    (7, 0.78),
                    (16, 0.66),
                    (28, 0.5),
                    (32, 0.6),
                    (41, 0.7),
                    (44, 0.35),
                    (45, 0.35),
                    (46, 0.42),
                ],
                0.18,
            ),
            SeedKind::Pad => (
                &[
                    (0, 0.68),
                    (1, 0.72),
                    (2, 0.85),
                    (3, 0.74),
                    (9, 0.3),
                    (12, 0.32),
                    (15, 0.55),
                    (28, 0.6),
                    (32, 0.58),
                    (41, 0.85),
                    (45, 0.55),
                    (47, 0.7),
                ],
                0.22,
            ),
            SeedKind::Lead => (
                &[
                    (0, 0.08),
                    (2, 0.8),
                    (3, 0.36),
                    (11, 0.14),
                    (15, 0.62),
                    (28, 0.35),
                    (32, 0.74),
                    (38, 0.4),
                    (44, 0.3),
                    (45, 0.2),
                ],
                0.22,
            ),
            SeedKind::Brass => (
                &[
                    (0, 0.4),
                    (1, 0.55),
                    (2, 0.8),
                    (3, 0.35),
                    (4, 0.4),
                    (5, 0.55),
                    (7, 0.82),
                    (11, 0.1),
                    (15, 0.7),
                    (16, 0.7),
                    (17, 0.125),
                    (28, 0.3),
                    (32, 0.55),
                    (33, 0.2),
                    (38, 0.35),
                    (45, 0.3),
                ],
                0.15,
            ),
            SeedKind::Flute => (
                &[
                    (0, 0.45),
                    (2, 0.85),
                    (3, 0.35),
                    (9, 0.68),
                    (11, 0.15),
                    (13, 0.1),
                    (15, 0.8),
                    (16, 0.05),
                    (17, 0.04),
                    (22, 0.0),
                    (25, 0.35),
                    (26, 0.65),
                    (32, 0.7),
                    (45, 0.4),
                ],
                0.15,
            ),
            SeedKind::Choir => (
                &[
                    (0, 0.62),
                    (2, 0.9),
                    (3, 0.65),
                    (9, 0.3),
                    (12, 0.25),
                    (16, 0.5),
                    (17, 0.375),
                    (18, 0.5),
                    (19, 0.375),
                    (25, 0.2),
                    (26, 0.7),
                    (28, 0.6),
                    (32, 0.62),
                    (33, 0.35),
                    (34, 0.5),
                    (41, 0.7),
                    (45, 0.6),
                ],
                0.15,
            ),
            SeedKind::Bell => (
                &[
                    (0, 0.02),
                    (1, 0.78),
                    (2, 0.0),
                    (3, 0.68),
                    (16, 0.0),
                    (18, 0.0),
                    (20, 0.74),
                    (22, 0.15),
                    (23, 0.48),
                    (8, 0.4),
                    (32, 0.86),
                    (45, 0.42),
                ],
                0.2,
            ),
            SeedKind::Drone => (
                &[
                    (0, 0.75),
                    (2, 1.0),
                    (3, 0.8),
                    (9, 0.15),
                    (12, 0.3),
                    (23, 0.2),
                    (28, 0.5),
                    (31, 0.4),
                    (45, 0.55),
                    (47, 0.75),
                ],
                0.22,
            ),
            SeedKind::Atmosphere => (
                &[
                    (0, 0.8),
                    (1, 0.85),
                    (2, 0.8),
                    (3, 0.85),
                    (9, 0.25),
                    (12, 0.5),
                    (14, 0.5),
                    (16, 0.5),
                    (17, 0.875),
                    (19, 0.625),
                    (25, 0.35),
                    (32, 0.6),
                    (41, 0.9),
                    (44, 0.4),
                    (45, 0.85),
                    (47, 0.85),
                ],
                0.2,
            ),
            SeedKind::Swell => (
                &[
                    (0, 0.9),
                    (2, 1.0),
                    (3, 0.4),
                    (4, 0.9),
                    (5, 0.9),
                    (7, 0.85),
                    (28, 0.5),
                    (32, 0.45),
                    (41, 0.8),
                    (45, 0.55),
                ],
                0.18,
            ),
            SeedKind::Kick => (
                &[
                    (0, 0.0),
                    (1, 0.42),
                    (2, 0.0),
                    (3, 0.3),
                    (4, 0.0),
                    (5, 0.25),
                    (6, 0.94),
                    (16, 0.0),
                    (17, 0.04),
                    (22, 0.0),
                    (25, 0.15),
                    (26, 0.3),
                    (27, 0.2),
                    (31, 0.3),
                    (32, 0.55),
                    (38, 0.4),
                    (44, 0.0),
                    (45, 0.02),
                ],
                0.12,
            ),
            SeedKind::Snare => (
                &[
                    (0, 0.0),
                    (1, 0.38),
                    (2, 0.0),
                    (3, 0.35),
                    (5, 0.2),
                    (6, 0.85),
                    (16, 0.0),
                    (22, 0.2),
                    (25, 0.75),
                    (26, 0.65),
                    (32, 0.8),
                    (33, 0.15),
                    (34, 0.55),
                    (44, 0.0),
                    (45, 0.25),
                ],
                0.15,
            ),
            SeedKind::Hat => (
                &[
                    (0, 0.0),
                    (1, 0.28),
                    (2, 0.0),
                    (3, 0.25),
                    (16, 0.5),
                    (17, 0.792),
                    (20, 0.93),
                    (22, 0.5),
                    (25, 0.85),
                    (26, 1.0),
                    (30, 0.6),
                    (32, 0.92),
                    (33, 0.1),
                    (34, 1.0),
                    (44, 0.0),
                    (45, 0.15),
                ],
                0.12,
            ),
            SeedKind::Perc => (
                &[
                    (0, 0.0),
                    (1, 0.26),
                    (2, 0.0),
                    (3, 0.2),
                    (5, 0.18),
                    (6, 0.95),
                    (25, 0.4),
                    (27, 0.13),
                    (32, 0.6),
                    (44, 0.0),
                    (45, 0.12),
                ],
                0.22,
            ),
            SeedKind::Texture => (
                &[
                    (0, 0.5),
                    (2, 0.7),
                    (3, 0.6),
                    (9, 0.55),
                    (12, 0.45),
                    (25, 0.45),
                    (29, 0.3),
                    (39, 0.4),
                    (43, 0.15),
                    (45, 0.6),
                ],
                0.3,
            ),
            SeedKind::Riser => (
                &[
                    (0, 0.92),
                    (2, 1.0),
                    (3, 0.55),
                    (4, 0.92),
                    (5, 0.95),
                    (6, 0.78),
                    (7, 0.9),
                    (25, 0.5),
                    (26, 0.6),
                    (32, 0.35),
                    (41, 0.8),
                    (44, 0.35),
                    (45, 0.65),
                ],
                0.18,
            ),
            SeedKind::Zap => (
                &[
                    (0, 0.0),
                    (1, 0.3),
                    (2, 0.0),
                    (3, 0.3),
                    (5, 0.2),
                    (6, 0.97),
                    (8, 0.7),
                    (23, 0.4),
                    (32, 0.85),
                    (33, 0.5),
                    (44, 0.4),
                    (46, 0.08),
                ],
                0.2,
            ),
            SeedKind::Glitch => (
                &[
                    (0, 0.02),
                    (1, 0.35),
                    (2, 0.4),
                    (3, 0.2),
                    (9, 0.85),
                    (10, 1.0),
                    (11, 0.3),
                    (12, 0.7),
                    (29, 0.5),
                    (30, 0.5),
                    (43, 0.6),
                    (44, 0.35),
                    (46, 0.08),
                ],
                0.25,
            ),
        }
    }
}

/// A seed's genes when nothing's said: a plain, playable saw through a gentle filter.
pub const NEUTRAL: [f32; GENES] = [
    0.15, 0.45, 0.6, 0.4, 0.1, 0.4, 0.5, 0.6, 0.1, 0.45, 0.0, 0.05, 0.1, 0.0, 0.0,
    0.3, // motion
    0.66, 0.0, 0.33, 0.0, 0.387, 0.2, 0.3, 0.0, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0, 0.0,
    0.1, // tone
    0.7, 0.2, 0.0, 0.5, 0.3, 0.5, 0.1, 0.0, 0.5, 0.3, 0.5, 0.0, 0.05, 0.15, 0.5, 0.5, // space
];

/// A gene changed by hand on one branch: what it grows to at the branch's tip.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct BranchEdit {
    pub branch: u8,
    pub gene: u8,
    pub tip: f32,
}

/// A seed the plant grew from, kept to go back to.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Ancestor {
    pub genes: Vec<f32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}

/// A Bloom plant.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Bloom {
    /// The sound it came from, for the window's title.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sound: String,
    pub params: BloomValues,
    /// Each branch's way through the genes.
    pub paths: [u32; BRANCHES],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub edits: Vec<BranchEdit>,
    /// Genes no branch changes, one bit each.
    #[serde(skip_serializing_if = "is_zero")]
    pub locks: u64,
    pub keys: BloomKeys,
    /// The branches Layer plays, one bit each.
    pub layer: u16,
    /// The branch picked: what One plays, what's replanted and saved.
    pub chosen: u8,
    pub voicing: Voicing,
    pub quality: PolyQuality,
    /// The LFO in time with the song, its rate a note length.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub sync: bool,
    pub kind: SeedKind,
    /// How many times it's been replanted, and the seeds it came from, newest last.
    pub generation: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ancestors: Vec<Ancestor>,
}

fn is_zero(x: &u64) -> bool {
    *x == 0
}

impl Default for Bloom {
    fn default() -> Self {
        let mut b = Bloom {
            sound: String::new(),
            params: BloomValues::default(),
            paths: std::array::from_fn(|i| mix32(0x5eed_0000 + i as u32)),
            edits: Vec::new(),
            locks: 0,
            keys: BloomKeys::Octave,
            layer: 0b0000_1001_0001,
            chosen: 0,
            voicing: Voicing::default(),
            quality: PolyQuality::Good,
            sync: false,
            kind: SeedKind::Any,
            generation: 0,
            ancestors: Vec::new(),
        };
        for k in 0..BRANCHES {
            b.params[branch_length(k)] = 0.25;
        }
        b
    }
}

/// A well-mixed 32 bits from any: lowbias32.
pub fn mix32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

/// 0 to 1, from a hash.
fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Folded back into 0 to 1 at its ends, as a ball off walls: a gene pushed past an end comes
/// back rather than sticking there.
pub fn reflect(x: f32) -> f32 {
    let m = x.rem_euclid(2.0);
    if m > 1.0 { 2.0 - m } else { m }
}

/// What a branch's growth is worked out from, for every gene.
#[derive(Clone, Copy, Debug)]
pub struct Growing {
    /// How far it's grown, 0 (the seed) to 1.
    pub reach: f32,
    /// Bent sideways, -1 to 1.
    pub bend: f32,
    pub wild: f32,
    pub atonal: f32,
}

/// Branch `path`'s genes, grown from `seed` as `g` says, `locks` kept, `edits` (gene, tip) of its
/// own steering where they grow to. Every gene moves smoothly with the reach and bend, wandering
/// on its way rather than in a line, so somewhere between is its own sound too.
pub fn grown(
    seed: &[f32],
    path: u32,
    g: Growing,
    locks: u64,
    edits: impl Iterator<Item = (usize, f32)> + Clone,
    out: &mut [f32; GENES],
) {
    let r = g.reach.clamp(0.0, 1.0);
    let wild = 0.25 + 1.25 * g.wild.clamp(0.0, 1.0);
    let theta = g.bend.clamp(-1.0, 1.0) * std::f32::consts::FRAC_PI_2;
    let (c, s) = (theta.cos(), theta.sin());
    for (i, o) in out.iter_mut().enumerate() {
        let base = seed.get(i).copied().unwrap_or(NEUTRAL[i]);
        let gene = &GENE_TABLE[i];
        if let Some((_, tip)) = edits.clone().find(|e| e.0 == i) {
            *o = base + (tip - base) * r;
            continue;
        }
        if gene.plant || locks >> i & 1 == 1 || r == 0.0 {
            *o = base;
            continue;
        }
        let h = mix32(path ^ (i as u32).wrapping_mul(0x9e37_79b9));
        let a = 2.0 * unit(h) - 1.0;
        let b = 2.0 * unit(mix32(h ^ 0x68e3_1da4)) - 1.0;
        let f = 0.6 + 1.6 * unit(mix32(h ^ 0xb529_7a4d));
        let phase = TAU * unit(mix32(h ^ 0x1b56_c4e9));
        let tuning = match gene.pitch {
            true => 0.08 + 0.92 * g.atonal.clamp(0.0, 1.0),
            false => 1.0,
        };
        let way = c * a + s * b + 0.45 * (TAU * f * r + phase).sin();
        *o = reflect(base + wild * gene.weight * tuning * r * way * 0.55);
    }
}

impl Bloom {
    /// The seed's genes, 0 to 1 each.
    pub fn seed(&self) -> [f32; GENES] {
        std::array::from_fn(|i| self.params[gene_param(i)])
    }

    pub fn set_seed(&mut self, genes: &[f32]) {
        for (i, g) in genes.iter().take(GENES).enumerate() {
            self.params[gene_param(i)] = g.clamp(0.0, 1.0);
        }
    }

    /// Branch `b`'s own steering: (gene, tip).
    pub fn edits_of(&self, b: usize) -> impl Iterator<Item = (usize, f32)> + Clone + '_ {
        self.edits
            .iter()
            .filter(move |e| e.branch as usize == b)
            .map(|e| (e.gene as usize, e.tip))
    }

    /// Branch `b`'s genes, grown `reach` along it as it's bent.
    pub fn branch_genes(&self, b: usize, reach: f32) -> [f32; GENES] {
        let mut out = [0.0; GENES];
        let g = Growing {
            reach,
            bend: self.params[branch_bend(b)],
            wild: self.params[BLOOM_WILD],
            atonal: self.params[BLOOM_ATONAL],
        };
        let b = b % BRANCHES;
        grown(
            &self.seed(),
            self.paths[b],
            g,
            self.locks,
            self.edits_of(b),
            &mut out,
        );
        out
    }

    /// How far each branch reaches with the plant's growth on top of its own length.
    pub fn reach(&self, b: usize, extra: f32) -> f32 {
        let l = self.params[branch_length(b % BRANCHES)].clamp(0.0, 1.0);
        let grow = (self.params[BLOOM_GROWTH] + extra).clamp(0.0, 1.0);
        l + grow * (1.0 - l)
    }

    /// A new seed, of `kind`, from `luck`: its branches sprouting new ways, a little grown. The
    /// one it replaces is kept among its ancestors.
    pub fn plant(&mut self, kind: SeedKind, luck: u32) {
        let genes = seed_of(kind, luck);
        self.remember();
        self.set_seed(&genes);
        self.kind = kind;
        self.sprout(luck, 0.18, 0.42);
        self.sound.clear();
    }

    /// The picked branch, as far as it's grown, made the seed: new branches grow from it.
    pub fn replant(&mut self, luck: u32) {
        let b = self.chosen as usize % BRANCHES;
        let genes = self.branch_genes(b, self.reach(b, 0.0));
        self.remember();
        self.set_seed(&genes);
        self.sprout(luck, 0.08, 0.3);
    }

    /// Every branch made the picked one: its way, bend, length and steering.
    pub fn clone_chosen(&mut self) {
        let b = self.chosen as usize % BRANCHES;
        let (path, len, bend) = (
            self.paths[b],
            self.params[branch_length(b)],
            self.params[branch_bend(b)],
        );
        let own: Vec<BranchEdit> = self
            .edits
            .iter()
            .filter(|e| e.branch as usize == b)
            .copied()
            .collect();
        self.edits.clear();
        for k in 0..BRANCHES {
            self.paths[k] = path;
            self.params[branch_length(k)] = len;
            self.params[branch_bend(k)] = bend;
            self.edits.extend(own.iter().map(|e| BranchEdit {
                branch: k as u8,
                ..*e
            }));
        }
    }

    /// A child of the picked branch and `other` (genes 0 to 1 each), crossed part by part, a
    /// little changed: made the seed.
    pub fn cross(&mut self, other: &[f32], luck: u32) {
        let b = self.chosen as usize % BRANCHES;
        let mine = self.branch_genes(b, self.reach(b, 0.0));
        let child = crossed(&mine, other, luck);
        self.remember();
        self.set_seed(&child);
        self.sprout(luck ^ 0xc0ff_ee00, 0.1, 0.32);
    }

    /// Back to an ancestor: it's the seed again, what was the seed kept in its place.
    pub fn go_back(&mut self, k: usize) {
        let Some(a) = self.ancestors.get(k).cloned() else {
            return;
        };
        let now = Ancestor {
            genes: self.seed().to_vec(),
            name: self.sound.clone(),
        };
        self.ancestors[k] = now;
        self.set_seed(&a.genes);
        self.sound = a.name;
    }

    /// The seed kept among its ancestors, before it changes.
    pub fn remember(&mut self) {
        self.ancestors.push(Ancestor {
            genes: self.seed().to_vec(),
            name: self.sound.clone(),
        });
        if self.ancestors.len() > ANCESTORS {
            self.ancestors.remove(0);
        }
        self.generation += 1;
    }

    /// New ways for every branch, lengths drawn between `short` and `long`, bends and steering
    /// gone.
    fn sprout(&mut self, luck: u32, short: f32, long: f32) {
        self.edits.clear();
        for k in 0..BRANCHES {
            let h = mix32(luck ^ mix32(k as u32 + 0x51ed));
            self.paths[k] = h;
            self.params[branch_length(k)] = short + (long - short) * unit(mix32(h ^ 0x77));
            self.params[branch_bend(k)] = 0.0;
        }
    }

    /// Whether branch `b` is in the layer.
    pub fn layered(&self, b: usize) -> bool {
        self.layer >> (b % BRANCHES) & 1 == 1
    }

    pub fn locked(&self, gene: usize) -> bool {
        self.locks >> gene & 1 == 1
    }
}

/// A sound's genes said in a few words: how bright, how it plays out, what it's made of, how it
/// moves and where it sits.
pub fn describe(g: &[f32; GENES]) -> String {
    let value = |i: usize| GENE_TABLE[i].shape.value(g[i]);
    let past = |i: usize, at: f32| g[i] > at;
    let mut words: Vec<&str> = Vec::new();
    words.push(match (g[34], g[32]) {
        (m, _) if m > 0.7 => "thin",
        (_, c) if c < 0.38 => "dark",
        (_, c) if c < 0.55 => "mellow",
        (_, c) if c < 0.7 => "warm",
        (_, c) if c < 0.84 => "clear",
        _ => "bright",
    });
    let (attack, decay, release) = (value(0), value(1), value(3));
    words.push(match g[2] {
        s if s < 0.15 && decay < 120.0 => "clipped",
        s if s < 0.15 && decay < 400.0 => "plucky",
        s if s < 0.15 => "ringing",
        _ if attack > 800.0 => "swelling",
        _ if attack > 150.0 => "soft-edged",
        _ if release > 1500.0 => "lingering",
        _ => "held",
    });
    // What it's made of: the strongest of these.
    let character = [
        (g[23].max(g[30]), "metallic"),
        (g[25] * 1.1, "breathy"),
        (g[29].max(g[43] * 1.2), "gritty"),
        (g[33] * 0.75, "resonant"),
        (g[39] * 0.9, "stringy"),
        (g[38] * 0.6, "driven"),
    ]
    .into_iter()
    .filter(|(v, _)| *v > 0.42)
    .max_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, w)) = character {
        words.push(w);
    }
    // The oscillator's own colour, when nothing stronger's been said.
    if words.len() < 3 {
        let table = ((g[17] * 12.0) as usize).min(11);
        words.push(match table {
            0 if g[16] < 0.2 => "round",
            0 if g[16] < 0.45 => "soft",
            0 if g[16] < 0.8 => "buzzy",
            0 => "hollow",
            1 => "analog",
            2 => "organ-like",
            3 => "smooth",
            4 => "vocal",
            5 => "reedy",
            6 => "bell-like",
            7 => "glassy",
            8 => "digital",
            9 => "clanging",
            10 => "airy",
            _ => "plucked",
        });
    }
    if g[27] < 0.3 || g[31] > 0.55 {
        words.push("deep");
    }
    if past(12, 0.4) || past(13, 0.4) || past(11, 0.35) || past(14, 0.5) {
        words.push("moving");
    }
    if past(28, 0.45) {
        words.push(if past(41, 0.5) { "wide" } else { "detuned" });
    }
    if g[45] > 0.65 {
        words.push("far away");
    } else if g[44] > 0.5 {
        words.push("echoing");
    } else if g[45] < 0.08 && g[44] < 0.08 {
        words.push("dry");
    }
    words.truncate(4);
    words.join(", ")
}

/// A seed of `kind`, drawn from `luck`.
pub fn seed_of(kind: SeedKind, luck: u32) -> [f32; GENES] {
    let (center, spread) = kind.center();
    let mut genes = NEUTRAL;
    for &(i, v) in center {
        genes[i] = v;
    }
    for (i, g) in genes.iter_mut().enumerate() {
        let gene = &GENE_TABLE[i];
        let h = mix32(luck ^ mix32(i as u32 ^ 0xa5a5_0000));
        // Wavetables and the plant's own genes are drawn anew; the rest stray from the middle.
        let wide = match (gene.shape, gene.plant) {
            (GeneShape::Steps(_), _) | (_, true) => 1.0,
            _ => spread * (0.4 + gene.weight),
        };
        *g = reflect(*g + wide * (2.0 * unit(h) - 1.0));
    }
    // Something must sound: a seed can't be only noise-free silence or a filter shut tight.
    genes[32] = genes[32].max(0.32);
    genes
}

/// Two parents' genes crossed: each part from one or the other, sometimes between, a little
/// changed.
pub fn crossed(a: &[f32], b: &[f32], luck: u32) -> [f32; GENES] {
    std::array::from_fn(|i| {
        let h = mix32(luck ^ mix32(i as u32 ^ 0x0c05_5000));
        let (x, y) = (
            a.get(i).copied().unwrap_or(NEUTRAL[i]),
            b.get(i).copied().unwrap_or(NEUTRAL[i]),
        );
        let pick = unit(h);
        let mixed = match pick {
            p if p < 0.4 => x,
            p if p < 0.8 => y,
            _ => x + (y - x) * unit(mix32(h ^ 0x99)),
        };
        reflect(mixed + 0.06 * (2.0 * unit(mix32(h ^ 0x1234)) - 1.0))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_parameter_and_gene_has_a_name_a_place_and_a_way_back() {
        assert_eq!(BLOOM_PARAMS.len(), BLOOM_PARAM_COUNT);
        let mut ids: Vec<&str> = BLOOM_PARAMS.iter().map(|p| p.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), BLOOM_PARAM_COUNT, "an id is used twice");
        for (i, p) in BLOOM_PARAMS.iter().enumerate() {
            assert_eq!(bloom_param(&p.id), Some(i), "{}", p.id);
            assert!(!p.say(p.default).is_empty(), "{}", p.id);
        }
        for (i, g) in GENE_TABLE.iter().enumerate() {
            assert_eq!(gene_index(g.id), Some(i));
            assert!(g.group.genes().contains(&i), "{} is out of its group", g.id);
            for v in [0.0, 0.5, 1.0] {
                assert!(g.shape.value(v).is_finite());
                assert!(!g.shape.say(v).is_empty());
            }
        }
        assert_eq!(param_gene(gene_param(32)), Some(32));
        assert_eq!(param_gene(BLOOM_GROWTH), None);
        assert_eq!(GENE_TABLE[32].id, "cutoff");
        assert_eq!(GENE_TABLE[47].id, "space_size");
    }

    #[test]
    fn a_branch_starts_at_its_seed_and_strays_further_as_it_grows() {
        let b = Bloom::default();
        let seed = b.seed();
        assert_eq!(b.branch_genes(3, 0.0), seed);
        let away = |r: f32| -> f32 {
            let g = b.branch_genes(3, r);
            g.iter().zip(&seed).map(|(a, s)| (a - s).abs()).sum()
        };
        assert!(
            away(0.1) < away(0.5) && away(0.5) < away(1.0),
            "{} {} {}",
            away(0.1),
            away(0.5),
            away(1.0)
        );
        // Smoothly: a hair further is a hair different.
        let (a, c) = (b.branch_genes(3, 0.5), b.branch_genes(3, 0.501));
        assert!(a.iter().zip(&c).all(|(x, y)| (x - y).abs() < 0.01));
        // Each branch its own way.
        assert_ne!(b.branch_genes(2, 1.0), b.branch_genes(3, 1.0));
        assert!(
            b.branch_genes(5, 1.0)
                .iter()
                .all(|g| (0.0..=1.0).contains(g))
        );
    }

    #[test]
    fn locked_and_whole_plant_genes_stay_and_steering_is_followed() {
        let mut b = Bloom {
            locks: 1 << 32,
            ..Default::default()
        };
        b.edits.push(BranchEdit {
            branch: 4,
            gene: 0,
            tip: 0.9,
        });
        let seed = b.seed();
        let g = b.branch_genes(4, 1.0);
        assert_eq!(g[32], seed[32], "locked");
        assert_eq!(g[46], seed[46], "the plant's");
        assert!((g[0] - 0.9).abs() < 1e-6, "steered");
        let half = b.branch_genes(4, 0.5);
        assert!((half[0] - (seed[0] + (0.9 - seed[0]) * 0.5)).abs() < 1e-6);
    }

    #[test]
    fn atonality_keeps_the_tuning_genes_close() {
        let mut b = Bloom::default();
        let seed = b.seed();
        let stray = |b: &Bloom| -> f32 {
            (0..BRANCHES)
                .map(|k| (b.branch_genes(k, 1.0)[27] - seed[27]).abs())
                .sum()
        };
        b.params[BLOOM_ATONAL] = 0.0;
        let tonal = stray(&b);
        b.params[BLOOM_ATONAL] = 1.0;
        assert!(stray(&b) > tonal * 3.0, "{} {}", stray(&b), tonal);
    }

    /// How varied the lines are across many seeds: run with --ignored to read them.
    #[test]
    #[ignore]
    fn what_seeds_are_like() {
        let mut lines = std::collections::BTreeMap::new();
        for k in SeedKind::ALL {
            for luck in 0..8u32 {
                *lines
                    .entry(describe(&seed_of(k, mix32(0xf10e_0000 ^ luck))))
                    .or_insert(0) += 1;
            }
        }
        eprintln!(
            "{} different lines of {}",
            lines.len(),
            SeedKind::ALL.len() * 8
        );
        let mut by: Vec<_> = lines.into_iter().collect();
        by.sort_by_key(|x| std::cmp::Reverse(x.1));
        for (l, n) in by.iter().take(12) {
            eprintln!("{n:3} {l}");
        }
    }

    #[test]
    fn planting_replanting_crossing_and_going_back() {
        let mut b = Bloom::default();
        let first = b.seed();
        b.plant(SeedKind::Pad, 7);
        assert_ne!(b.seed(), first);
        assert_eq!(b.ancestors.len(), 1);
        assert_eq!(b.kind, SeedKind::Pad);
        b.chosen = 5;
        let tip = b.branch_genes(5, b.reach(5, 0.0));
        b.replant(9);
        assert_eq!(b.seed(), tip);
        assert_eq!(b.generation, 2);
        b.go_back(0);
        assert_eq!(b.seed(), first);
        let other = seed_of(SeedKind::Bass, 3);
        b.cross(&other, 11);
        assert!(b.seed().iter().all(|g| (0.0..=1.0).contains(g)));
        // Every kind draws a seed that can be heard, and says what it's like.
        for k in SeedKind::ALL {
            for luck in 0..20 {
                let g = seed_of(k, luck);
                assert!(g[32] >= 0.32 && g.iter().all(|x| (0.0..=1.0).contains(x)));
                assert!(describe(&g).split(", ").count() >= 2);
            }
        }
        // Each kind but Anything is in one family, once.
        let mut all: Vec<SeedKind> = SeedFamily::ALL
            .iter()
            .flat_map(|f| f.kinds().to_vec())
            .collect();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), SeedKind::ALL.len() - 1);
        assert_eq!(SeedKind::Any.family(), None);
        assert_eq!(SeedKind::Reese.family(), Some(SeedFamily::Low));
    }

    #[test]
    fn a_plant_saves_only_what_changed_and_comes_back_whole() {
        let mut b = Bloom::default();
        b.params[gene_param(32)] = 0.4;
        b.keys = BloomKeys::Layer;
        b.edits.push(BranchEdit {
            branch: 1,
            gene: 3,
            tip: 0.2,
        });
        let json = serde_json::to_value(&b).unwrap();
        let back: Bloom = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(back, b);
        assert!(json["params"].as_object().unwrap().len() <= 14, "{json}");
    }
}
