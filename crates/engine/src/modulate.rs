//! Modulation, as Bitwig's modulators and Logic's mod matrix: a track's modulators (an LFO, a
//! step sequence of values, an envelope its notes set off, a follower of another track's level,
//! random values, a macro to turn by hand) each moving any number of its settings by a depth:
//! its volume, pan and sends, its EQ, compressor and saturator, its Sampler's voice, its plugins'
//! parameters. The engine works them out a block at a time; the app shows each as it moves and
//! draws its range on the knobs it turns.
//!
//! A modulator's value is -1 to 1 (an LFO, random) or 0 to 1 (steps, an envelope, a follower, a
//! macro); a route moves its target by `depth` times that, in the target's own units: dB for
//! levels, octaves for frequencies, semitones for pitch, -1 to 1 for pan, 0 to 1 for a plugin's
//! parameter.

use serde::{Deserialize, Serialize};

use crate::project::{Comp, Eq, MAX_BANDS, SamplerVoice, Saturator, Shape, Which};

/// The most modulators a track has.
pub const MAX_MODS: usize = 8;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Modulator {
    #[serde(flatten)]
    pub kind: ModKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<ModRoute>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModKind {
    /// A wave, -1 to 1 (0 to 1 if unipolar); `phase` turns it, 0 to 1.
    Lfo {
        shape: LfoShape,
        rate: ModRate,
        #[serde(default)]
        phase: f32,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        unipolar: bool,
    },
    /// Values, 0 to 1, a step each `rate`, round and round; `glide` (0 to 1) slides between them.
    Steps {
        values: Vec<f32>,
        rate: ModRate,
        #[serde(default)]
        glide: f32,
    },
    /// 0 to 1: rises when a note starts on the track, falls when the last one ends.
    Envelope {
        attack_ms: f32,
        decay_ms: f32,
        sustain: f32,
        release_ms: f32,
    },
    /// Another track's level (by id), 0 to 1 from -60 dB to 0: a sidechain, for pumping and ducking.
    Follower {
        source: u64,
        attack_ms: f32,
        release_ms: f32,
    },
    /// A new value, -1 to 1, each `rate`; smooth glides to it.
    Random {
        rate: ModRate,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        smooth: bool,
    },
    /// A value, 0 to 1, set by hand (or automated, or by an agent); 0 if not given.
    Macro {
        #[serde(default)]
        value: f32,
    },
    /// One of the modulators of the stack the track plays into (by its place there): one
    /// modulator every track in the stack shares, each moving its own settings by it.
    Shared { modulator: u8 },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LfoShape {
    Sine,
    Triangle,
    SawUp,
    SawDown,
    Square,
}

/// How fast a modulator goes round: a cycle (or a step) this many beats long, in time with the
/// song; or so many times a second.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ModRate {
    Beats(f32),
    Hz(f32),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ModRoute {
    pub target: ModTarget,
    /// How far, in the target's units, the modulator's full value moves it.
    pub depth: f32,
}

/// What a modulator moves, and its units.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ModTarget {
    /// dB.
    Volume,
    /// -1 to 1 (round the head, for binaural).
    Pan,
    /// A send's level, by index: dB.
    Send(usize),
    /// The EQ's first low cut's frequency: octaves (from off, it sweeps up from 20 Hz).
    LowCut,
    /// An EQ band's gain (dB) and frequency (octaves), by its place in the EQ. (A song made with
    /// the EQ Pina Studio had before opens with its low cut, two bells and shelf as bands 0 to 3, so
    /// `EqGain` 1 to 3 still move what they did.)
    EqGain(u8),
    EqFreq(u8),
    /// The compressor's threshold and makeup: dB.
    CompThreshold,
    CompMakeup,
    /// The saturator's drive (dB), tone (-1 to 1) and mix (0 to 1).
    SatDrive,
    SatTone,
    SatMix,
    Sampler(SamplerParam),
    /// A plugin's parameter: 0 to 1.
    Plugin(Which, u32),
    /// A step pattern's sound (by its row), each step as it falls in the song: its velocity, its
    /// pitch (semitones), its chance (%). Worked out ahead, so only an LFO, steps, random or a
    /// macro can move it.
    Sound(usize, SoundParam),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SoundParam {
    Velocity,
    Pitch,
    Chance,
}

/// The Sampler's voice settings a modulator can move, and their units.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SamplerParam {
    /// Octaves.
    Cutoff,
    /// 0 to 1.
    Resonance,
    /// Octaves.
    FilterEnv,
    /// Semitones.
    Transpose,
    /// dB.
    Volume,
    /// Cents.
    Vibrato,
    /// Octaves.
    LfoFilter,
    /// 0 to 1.
    Tremolo,
}

impl ModKind {
    /// A modulator to start from, of each kind.
    pub fn lfo() -> Self {
        ModKind::Lfo {
            shape: LfoShape::Sine,
            rate: ModRate::Beats(1.0),
            phase: 0.0,
            unipolar: false,
        }
    }

    pub fn steps() -> Self {
        ModKind::Steps {
            values: vec![
                1.0, 0.25, 0.6, 0.25, 0.85, 0.25, 0.6, 0.4, 1.0, 0.25, 0.6, 0.25, 0.85, 0.5, 0.7,
                0.3,
            ],
            rate: ModRate::Beats(0.25),
            glide: 0.0,
        }
    }

    pub fn envelope() -> Self {
        ModKind::Envelope {
            attack_ms: 10.0,
            decay_ms: 300.0,
            sustain: 0.5,
            release_ms: 400.0,
        }
    }

    pub fn follower(source: u64) -> Self {
        ModKind::Follower {
            source,
            attack_ms: 5.0,
            release_ms: 150.0,
        }
    }

    pub fn random() -> Self {
        ModKind::Random {
            rate: ModRate::Beats(0.5),
            smooth: false,
        }
    }

    pub fn mac() -> Self {
        ModKind::Macro { value: 0.0 }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ModKind::Lfo { .. } => "LFO",
            ModKind::Steps { .. } => "Steps",
            ModKind::Envelope { .. } => "Envelope",
            ModKind::Follower { .. } => "Follower",
            ModKind::Random { .. } => "Random",
            ModKind::Macro { .. } => "Macro",
            ModKind::Shared { .. } => "Shared",
        }
    }

    /// Whether its values go below 0.
    pub fn bipolar(&self) -> bool {
        match self {
            ModKind::Lfo { unipolar, .. } => !unipolar,
            ModKind::Random { .. } => true,
            _ => false,
        }
    }
}

impl LfoShape {
    pub const ALL: [LfoShape; 5] = [
        LfoShape::Sine,
        LfoShape::Triangle,
        LfoShape::SawUp,
        LfoShape::SawDown,
        LfoShape::Square,
    ];

    pub fn name(self) -> &'static str {
        match self {
            LfoShape::Sine => "Sine",
            LfoShape::Triangle => "Triangle",
            LfoShape::SawUp => "Saw up",
            LfoShape::SawDown => "Saw down",
            LfoShape::Square => "Square",
        }
    }

    /// The wave at a phase (0 to 1), -1 to 1.
    pub fn at(self, phase: f64) -> f32 {
        let p = phase.rem_euclid(1.0) as f32;
        match self {
            LfoShape::Sine => (p * std::f32::consts::TAU).sin(),
            // In step with the sine: 0, up to 1 at a quarter, down to -1 at three quarters.
            LfoShape::Triangle => 1.0 - 4.0 * ((p + 0.25).rem_euclid(1.0) - 0.5).abs(),
            LfoShape::SawUp => 2.0 * p - 1.0,
            LfoShape::SawDown => 1.0 - 2.0 * p,
            LfoShape::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
        }
    }
}

impl ModRate {
    /// Cycles (or steps) a second at a tempo.
    pub fn hz(self, tempo: f64) -> f64 {
        match self {
            ModRate::Beats(b) => tempo / 60.0 / (b.max(1e-3) as f64),
            ModRate::Hz(h) => h.max(0.0) as f64,
        }
    }

    /// As a musician says it: 1/8, 1 bar, 2.0 Hz.
    pub fn label(self) -> String {
        match self {
            ModRate::Hz(h) => format!("{h:.2} Hz"),
            ModRate::Beats(b) => match b {
                b if b >= 4.0 && (b / 4.0).fract() == 0.0 => match (b / 4.0) as u32 {
                    1 => "1 bar".into(),
                    n => format!("{n} bars"),
                },
                b => {
                    // A note's length: a beat is a quarter; dotted and triplet ones too.
                    let quarter = 1.0 / b;
                    for (den, label) in [(1.0, ""), (1.5, "."), (2.0 / 3.0, "T")] {
                        let n = 4.0 * quarter * den;
                        let whole = (n - n.round()).abs() < 1e-3 && n >= 1.0;
                        if whole && (n.round() as u32).is_power_of_two() {
                            return format!("1/{}{label}", n.round() as u32);
                        }
                    }
                    format!("{b:.2} beats")
                }
            },
        }
    }
}

/// A modulator's value at a point in the song (beats), for those that depend only on where the
/// song is: an LFO, steps, random, a macro. An envelope or a follower depends on what plays.
pub fn value_at(kind: &ModKind, beats: f64, tempo: f64) -> Option<f32> {
    let cycles = |rate: ModRate| match rate {
        ModRate::Beats(b) => beats / b.max(1e-3) as f64,
        ModRate::Hz(h) => beats * 60.0 / tempo.max(1.0) * h as f64,
    };
    match kind {
        ModKind::Lfo {
            shape,
            rate,
            phase,
            unipolar,
        } => {
            let v = shape.at(cycles(*rate) + *phase as f64);
            Some(if *unipolar { 0.5 + 0.5 * v } else { v })
        }
        ModKind::Steps { values, rate, .. } => match values.len() {
            0 => Some(0.0),
            n => Some(values[(cycles(*rate).floor() as i64).rem_euclid(n as i64) as usize]),
        },
        ModKind::Random { rate, smooth } => {
            let c = cycles(*rate);
            let n = c.floor() as i64;
            Some(match smooth {
                false => noise(n),
                true => {
                    let t = c.fract() as f32;
                    let t = t * t * (3.0 - 2.0 * t);
                    noise(n) + (noise(n + 1) - noise(n)) * t
                }
            })
        }
        ModKind::Macro { value } => Some(value.clamp(0.0, 1.0)),
        // Its stack's modulator's, found by whoever knows the stack.
        ModKind::Envelope { .. } | ModKind::Follower { .. } | ModKind::Shared { .. } => None,
    }
}

/// A step's chance roll on a row, 0 to 100, the same each time the song plays.
pub fn roll(step: i64, row: usize) -> f32 {
    (noise(step.wrapping_mul(7919) ^ (row as i64).wrapping_mul(104_729)) + 1.0) * 50.0
}

/// What a modulator remembers from block to block.
#[derive(Clone, Copy, Default)]
pub struct ModState {
    /// Where it is round its cycle (or through its steps), counting whole cycles.
    phase: f64,
    /// The envelope's or follower's level, and the envelope's stage.
    level: f32,
    stage: Stage,
    /// The value it glides from, and last gave.
    pub value: f32,
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Stage {
    #[default]
    Idle,
    Attack,
    Decay,
    Release,
}

/// What a modulator hears each block.
#[derive(Clone, Copy, Default)]
pub struct ModInput {
    /// The song's position in beats, while it plays.
    pub beats: Option<f64>,
    /// How long the block is, in seconds; and the tempo.
    pub seconds: f64,
    pub tempo: f64,
    /// A note started on the track this block; some note is held.
    pub trigger: bool,
    pub gate: bool,
    /// The followed track's peak, last block.
    pub follow: f32,
}

/// A whole number's noise, -1 to 1, the same every time.
fn noise(n: i64) -> f32 {
    let mut x = n as u64 ^ 0x9e37_79b9_7f4a_7c15;
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    (x >> 40) as f32 / (1u64 << 23) as f32 - 1.0
}

/// A modulator's value this block. While the song plays, those in time with it follow its
/// position (a bounce sounds as playing did); otherwise they run on their own.
pub fn step(kind: &ModKind, state: &mut ModState, input: &ModInput) -> f32 {
    step_over(kind, state, input, &ModOver::default())
}

/// A modulator's setting automation can move: a macro's value, an LFO's phase, steps' glide, an
/// envelope's (or follower's) times and sustain.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ModParam {
    Value,
    Phase,
    Glide,
    Attack,
    Decay,
    Sustain,
    Release,
}

impl ModParam {
    pub const ALL: [ModParam; 7] = [
        ModParam::Value,
        ModParam::Phase,
        ModParam::Glide,
        ModParam::Attack,
        ModParam::Decay,
        ModParam::Sustain,
        ModParam::Release,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ModParam::Value => "Value",
            ModParam::Phase => "Phase",
            ModParam::Glide => "Glide",
            ModParam::Attack => "Attack",
            ModParam::Decay => "Decay",
            ModParam::Sustain => "Sustain",
            ModParam::Release => "Release",
        }
    }

    /// By its name, as a command gives it ("value", "Attack").
    pub fn parse(s: &str) -> Option<ModParam> {
        ModParam::ALL
            .into_iter()
            .find(|p| p.name().eq_ignore_ascii_case(s.trim()))
    }

    /// Its lowest and highest: milliseconds for times, else 0 to 1.
    pub fn range(self) -> (f32, f32) {
        match self {
            ModParam::Attack | ModParam::Decay | ModParam::Release => (1.0, 10_000.0),
            _ => (0.0, 1.0),
        }
    }

    /// Its value on a modulator, if the modulator has it.
    pub fn get(self, kind: &ModKind) -> Option<f32> {
        Some(match (self, kind) {
            (ModParam::Value, ModKind::Macro { value }) => *value,
            (ModParam::Phase, ModKind::Lfo { phase, .. }) => *phase,
            (ModParam::Glide, ModKind::Steps { glide, .. }) => *glide,
            (ModParam::Attack, ModKind::Envelope { attack_ms, .. })
            | (ModParam::Attack, ModKind::Follower { attack_ms, .. }) => *attack_ms,
            (ModParam::Decay, ModKind::Envelope { decay_ms, .. }) => *decay_ms,
            (ModParam::Sustain, ModKind::Envelope { sustain, .. }) => *sustain,
            (ModParam::Release, ModKind::Envelope { release_ms, .. })
            | (ModParam::Release, ModKind::Follower { release_ms, .. }) => *release_ms,
            _ => return None,
        })
    }

    /// Sets it on a modulator that has it, kept in its range.
    pub fn set(self, kind: &mut ModKind, v: f32) {
        let (lo, hi) = self.range();
        let v = v.clamp(lo, hi);
        match (self, kind) {
            (ModParam::Value, ModKind::Macro { value }) => *value = v,
            (ModParam::Phase, ModKind::Lfo { phase, .. }) => *phase = v,
            (ModParam::Glide, ModKind::Steps { glide, .. }) => *glide = v,
            (ModParam::Attack, ModKind::Envelope { attack_ms, .. })
            | (ModParam::Attack, ModKind::Follower { attack_ms, .. }) => *attack_ms = v,
            (ModParam::Decay, ModKind::Envelope { decay_ms, .. }) => *decay_ms = v,
            (ModParam::Sustain, ModKind::Envelope { sustain, .. }) => *sustain = v,
            (ModParam::Release, ModKind::Envelope { release_ms, .. })
            | (ModParam::Release, ModKind::Follower { release_ms, .. }) => *release_ms = v,
            _ => {}
        }
    }

    /// The settings a modulator has that automation can move.
    pub fn of(kind: &ModKind) -> Vec<ModParam> {
        ModParam::ALL
            .into_iter()
            .filter(|p| p.get(kind).is_some())
            .collect()
    }
}

/// What automation has a modulator's settings at this block, over its own: copied, so working
/// it out allocates nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct ModOver([Option<f32>; 7]);

impl ModOver {
    pub fn set(&mut self, p: ModParam, v: f32) {
        let (lo, hi) = p.range();
        self.0[p as usize] = Some(v.clamp(lo, hi));
    }

    fn or(&self, p: ModParam, own: f32) -> f32 {
        self.0[p as usize].unwrap_or(own)
    }
}

/// `step`, with any settings automation has moved (`over`) in place of the modulator's own.
pub fn step_over(kind: &ModKind, state: &mut ModState, input: &ModInput, over: &ModOver) -> f32 {
    let ms = |t: f32| (input.seconds * 1000.0 / t.max(0.1) as f64) as f32;
    let advance = |state: &mut ModState, rate: ModRate| match (rate, input.beats) {
        (ModRate::Beats(b), Some(beats)) => state.phase = beats / b.max(1e-3) as f64,
        _ => state.phase += rate.hz(input.tempo) * input.seconds,
    };
    let value = match kind {
        ModKind::Lfo {
            shape,
            rate,
            phase,
            unipolar,
        } => {
            advance(state, *rate);
            let v = shape.at(state.phase + over.or(ModParam::Phase, *phase) as f64);
            if *unipolar { 0.5 + 0.5 * v } else { v }
        }
        ModKind::Steps {
            values,
            rate,
            glide,
        } => {
            advance(state, *rate);
            let to = match values.len() {
                0 => 0.0,
                n => values[(state.phase.floor() as i64).rem_euclid(n as i64) as usize],
            };
            // Glide: a share of the way each block, slower the more it glides.
            let keep = (over.or(ModParam::Glide, *glide).clamp(0.0, 1.0) * 0.95).powf(0.5);
            state.value + (to - state.value) * (1.0 - keep)
        }
        ModKind::Envelope {
            attack_ms,
            decay_ms,
            sustain,
            release_ms,
        } => {
            if input.trigger {
                state.stage = Stage::Attack;
            } else if !input.gate && state.stage != Stage::Idle {
                state.stage = Stage::Release;
            }
            let sustain = over.or(ModParam::Sustain, *sustain).clamp(0.0, 1.0);
            let (attack_ms, decay_ms, release_ms) = (
                over.or(ModParam::Attack, *attack_ms),
                over.or(ModParam::Decay, *decay_ms),
                over.or(ModParam::Release, *release_ms),
            );
            match state.stage {
                Stage::Attack => {
                    state.level += ms(attack_ms);
                    if state.level >= 1.0 {
                        (state.level, state.stage) = (1.0, Stage::Decay);
                    }
                }
                Stage::Decay => {
                    state.level = (state.level - ms(decay_ms) * (1.0 - sustain)).max(sustain);
                }
                Stage::Release => {
                    state.level -= ms(release_ms);
                    if state.level <= 0.0 {
                        (state.level, state.stage) = (0.0, Stage::Idle);
                    }
                }
                Stage::Idle => state.level = 0.0,
            }
            state.level
        }
        ModKind::Follower {
            attack_ms,
            release_ms,
            ..
        } => {
            let db = 20.0 * input.follow.max(1e-6).log10();
            let to = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
            let t = if to > state.level {
                over.or(ModParam::Attack, *attack_ms)
            } else {
                over.or(ModParam::Release, *release_ms)
            };
            let k = 1.0 - (-(input.seconds * 1000.0) as f32 / t.max(0.1)).exp();
            state.level += (to - state.level) * k;
            state.level
        }
        ModKind::Random { rate, smooth } => {
            advance(state, *rate);
            let n = state.phase.floor() as i64;
            match smooth {
                false => noise(n),
                true => {
                    let t = state.phase.fract() as f32;
                    let t = t * t * (3.0 - 2.0 * t);
                    noise(n) + (noise(n + 1) - noise(n)) * t
                }
            }
        }
        ModKind::Macro { value } => over.or(ModParam::Value, *value).clamp(0.0, 1.0),
        // The engine plays the stack's.
        ModKind::Shared { .. } => state.value,
    };
    state.value = value;
    value
}

/// How far a block's modulators move each of a track's settings, in their units.
#[derive(Clone, Copy)]
pub struct Offsets {
    pub volume: f32,
    pub pan: f32,
    pub sends: [f32; 8],
    pub low_cut: f32,
    pub eq_gain: [f32; MAX_BANDS],
    pub eq_freq: [f32; MAX_BANDS],
    pub comp: [f32; 2],
    pub sat: [f32; 3],
    pub sampler: [f32; 8],
    pub plugins: [(Which, u32, f32); 16],
    pub plugin_count: usize,
    /// Whether anything moves the strip, the Sampler.
    pub strip: bool,
    pub voice: bool,
}

impl Default for Offsets {
    fn default() -> Self {
        Offsets {
            volume: 0.0,
            pan: 0.0,
            sends: [0.0; 8],
            low_cut: 0.0,
            eq_gain: [0.0; MAX_BANDS],
            eq_freq: [0.0; MAX_BANDS],
            comp: [0.0; 2],
            sat: [0.0; 3],
            sampler: [0.0; 8],
            plugins: [(Which::Instrument, 0, 0.0); 16],
            plugin_count: 0,
            strip: false,
            voice: false,
        }
    }
}

impl SamplerParam {
    pub const ALL: [SamplerParam; 8] = [
        SamplerParam::Cutoff,
        SamplerParam::Resonance,
        SamplerParam::FilterEnv,
        SamplerParam::Transpose,
        SamplerParam::Volume,
        SamplerParam::Vibrato,
        SamplerParam::LfoFilter,
        SamplerParam::Tremolo,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|&p| p == self).unwrap_or(0)
    }
}

impl Offsets {
    /// A route's share: `amount` more of its target.
    pub fn add(&mut self, target: ModTarget, amount: f32) {
        match target {
            ModTarget::Volume => self.volume += amount,
            ModTarget::Pan => self.pan += amount,
            ModTarget::Send(i) => {
                if let Some(s) = self.sends.get_mut(i) {
                    *s += amount
                }
            }
            ModTarget::LowCut => (self.low_cut, self.strip) = (self.low_cut + amount, true),
            ModTarget::EqGain(b) => {
                if let Some(g) = self.eq_gain.get_mut(b as usize) {
                    (*g, self.strip) = (*g + amount, true);
                }
            }
            ModTarget::EqFreq(b) => {
                if let Some(f) = self.eq_freq.get_mut(b as usize) {
                    (*f, self.strip) = (*f + amount, true);
                }
            }
            ModTarget::CompThreshold => (self.comp[0], self.strip) = (self.comp[0] + amount, true),
            ModTarget::CompMakeup => (self.comp[1], self.strip) = (self.comp[1] + amount, true),
            ModTarget::SatDrive => (self.sat[0], self.strip) = (self.sat[0] + amount, true),
            ModTarget::SatTone => (self.sat[1], self.strip) = (self.sat[1] + amount, true),
            ModTarget::SatMix => (self.sat[2], self.strip) = (self.sat[2] + amount, true),
            ModTarget::Sampler(p) => {
                self.sampler[p.index()] += amount;
                self.voice = true;
            }
            // Worked out with the pattern's notes.
            ModTarget::Sound(..) => {}
            ModTarget::Plugin(which, id) => {
                let n = self.plugin_count;
                let at = self.plugins[..n]
                    .iter()
                    .position(|p| p.0 == which && p.1 == id);
                match at {
                    Some(i) => self.plugins[i].2 += amount,
                    None if n < self.plugins.len() => {
                        self.plugins[n] = (which, id, amount);
                        self.plugin_count += 1;
                    }
                    None => {}
                }
            }
        }
    }

    pub fn plugin(&self, which: Which, id: u32) -> Option<f32> {
        self.plugins[..self.plugin_count]
            .iter()
            .find(|p| p.0 == which && p.1 == id)
            .map(|p| p.2)
    }

    pub fn eq(&self, eq: &mut Eq) {
        apply_eq(eq, ModTarget::LowCut, self.low_cut);
        for b in 0..eq.bands.len() {
            apply_eq(eq, ModTarget::EqGain(b as u8), self.eq_gain[b]);
            apply_eq(eq, ModTarget::EqFreq(b as u8), self.eq_freq[b]);
        }
    }

    pub fn comp(&self, c: &mut Comp) {
        apply_comp(c, ModTarget::CompThreshold, self.comp[0]);
        apply_comp(c, ModTarget::CompMakeup, self.comp[1]);
    }

    pub fn sat(&self, s: &mut Saturator) {
        apply_sat(s, ModTarget::SatDrive, self.sat[0]);
        apply_sat(s, ModTarget::SatTone, self.sat[1]);
        apply_sat(s, ModTarget::SatMix, self.sat[2]);
    }

    pub fn voice(&self, v: &mut SamplerVoice) {
        for p in SamplerParam::ALL {
            apply_voice(v, p, self.sampler[p.index()]);
        }
    }
}

/// A setting moved by `by` in its units, kept in its range: as the engine plays it and the app
/// draws it.
pub fn apply_volume(db: f32, by: f32) -> f32 {
    if by == 0.0 {
        db
    } else {
        (db + by).clamp(-144.0, 6.0)
    }
}

pub fn apply_pan(pan: f32, by: f32) -> f32 {
    (pan + by).clamp(-1.0, 1.0)
}

pub fn apply_eq(eq: &mut Eq, target: ModTarget, by: f32) {
    if by == 0.0 {
        return;
    }
    match target {
        // From off, a modulated low cut sweeps up from 20 Hz.
        ModTarget::LowCut => {
            if let Some(i) = eq.nth(Shape::LowCut, 0) {
                let b = &mut eq.bands[i];
                let from = if b.off { 20.0 } else { b.freq.max(20.0) };
                b.freq = (from * 2f32.powf(by)).clamp(20.0, 2000.0);
                b.off = false;
            }
        }
        ModTarget::EqGain(i) => {
            if let Some(b) = eq.bands.get_mut(i as usize) {
                b.gain_db = (b.gain_db + by).clamp(-30.0, 30.0);
            }
        }
        ModTarget::EqFreq(i) => {
            if let Some(b) = eq.bands.get_mut(i as usize) {
                b.freq = (b.freq * 2f32.powf(by)).clamp(10.0, 30_000.0);
            }
        }
        _ => {}
    }
}

pub fn apply_comp(c: &mut Comp, target: ModTarget, by: f32) {
    match target {
        ModTarget::CompThreshold => c.threshold_db = (c.threshold_db + by).clamp(-60.0, 0.0),
        ModTarget::CompMakeup => c.makeup_db = (c.makeup_db + by).clamp(0.0, 24.0),
        _ => {}
    }
}

pub fn apply_sat(s: &mut Saturator, target: ModTarget, by: f32) {
    match target {
        ModTarget::SatDrive => s.drive_db = (s.drive_db + by).clamp(0.0, 24.0),
        ModTarget::SatTone => s.tone = (s.tone + by).clamp(-1.0, 1.0),
        ModTarget::SatMix => s.mix = (s.mix + by).clamp(0.0, 1.0),
        _ => {}
    }
}

pub fn apply_voice(v: &mut SamplerVoice, p: SamplerParam, by: f32) {
    if by == 0.0 {
        return;
    }
    match p {
        SamplerParam::Cutoff => v.cutoff_hz = (v.cutoff_hz * 2f32.powf(by)).clamp(20.0, 20_000.0),
        SamplerParam::Resonance => v.resonance = (v.resonance + by).clamp(0.0, 1.0),
        SamplerParam::FilterEnv => v.filter_env = (v.filter_env + by).clamp(-6.0, 6.0),
        SamplerParam::Transpose => v.transpose = (v.transpose + by).clamp(-48.0, 48.0),
        SamplerParam::Volume => v.gain_db = (v.gain_db + by).clamp(-60.0, 12.0),
        SamplerParam::Vibrato => v.vibrato_cents = (v.vibrato_cents + by).clamp(0.0, 1200.0),
        SamplerParam::LfoFilter => v.lfo_filter = (v.lfo_filter + by).clamp(0.0, 6.0),
        SamplerParam::Tremolo => v.tremolo = (v.tremolo + by).clamp(0.0, 1.0),
    }
}

/// A route's depth to start from, by what it moves: a sweep you'd hear, not a whisper.
pub fn default_depth(target: ModTarget) -> f32 {
    match target {
        ModTarget::Volume | ModTarget::Send(_) => -12.0,
        ModTarget::Pan | ModTarget::SatTone => 0.8,
        ModTarget::LowCut => 3.0,
        ModTarget::EqGain(_) => 6.0,
        ModTarget::EqFreq(_) => 1.0,
        ModTarget::CompThreshold => -12.0,
        ModTarget::CompMakeup | ModTarget::SatDrive => 9.0,
        ModTarget::SatMix => 0.5,
        ModTarget::Sampler(SamplerParam::Cutoff) => 2.0,
        ModTarget::Sampler(SamplerParam::Transpose) => 12.0,
        ModTarget::Sampler(SamplerParam::Volume) => -12.0,
        ModTarget::Sampler(SamplerParam::Vibrato) => 50.0,
        ModTarget::Sampler(SamplerParam::FilterEnv | SamplerParam::LfoFilter) => 2.0,
        ModTarget::Sampler(_) => 0.5,
        ModTarget::Plugin(..) => 0.3,
        ModTarget::Sound(_, SoundParam::Velocity) => -40.0,
        ModTarget::Sound(_, SoundParam::Pitch) => 12.0,
        ModTarget::Sound(_, SoundParam::Chance) => -50.0,
    }
}

/// A depth's range: how far a route can move its target either way.
pub fn depth_range(target: ModTarget) -> f32 {
    match target {
        ModTarget::Volume | ModTarget::Send(_) | ModTarget::CompThreshold => 48.0,
        ModTarget::Pan | ModTarget::SatTone => 2.0,
        ModTarget::LowCut | ModTarget::EqFreq(_) => 6.0,
        ModTarget::EqGain(_) | ModTarget::CompMakeup | ModTarget::SatDrive => 24.0,
        ModTarget::SatMix | ModTarget::Plugin(..) => 1.0,
        ModTarget::Sampler(SamplerParam::Cutoff | SamplerParam::FilterEnv) => 8.0,
        ModTarget::Sampler(SamplerParam::LfoFilter) => 6.0,
        ModTarget::Sampler(SamplerParam::Transpose) => 24.0,
        ModTarget::Sampler(SamplerParam::Volume) => 48.0,
        ModTarget::Sampler(SamplerParam::Vibrato) => 1200.0,
        ModTarget::Sampler(_) => 1.0,
        ModTarget::Sound(_, SoundParam::Velocity) => 127.0,
        ModTarget::Sound(_, SoundParam::Pitch) => 24.0,
        ModTarget::Sound(_, SoundParam::Chance) => 100.0,
    }
}

/// A depth as people read it, in its target's units.
pub fn depth_text(target: ModTarget, depth: f32) -> String {
    let sign = if depth > 0.0 {
        "+"
    } else if depth < 0.0 {
        "−"
    } else {
        ""
    };
    let d = depth.abs();
    match target {
        ModTarget::Volume
        | ModTarget::Send(_)
        | ModTarget::EqGain(_)
        | ModTarget::CompThreshold
        | ModTarget::CompMakeup
        | ModTarget::SatDrive
        | ModTarget::Sampler(SamplerParam::Volume) => format!("{sign}{d:.1} dB"),
        ModTarget::LowCut
        | ModTarget::EqFreq(_)
        | ModTarget::Sampler(
            SamplerParam::Cutoff | SamplerParam::FilterEnv | SamplerParam::LfoFilter,
        ) => {
            format!("{sign}{d:.1} oct")
        }
        ModTarget::Sampler(SamplerParam::Transpose) => format!("{sign}{d:.1} st"),
        ModTarget::Sampler(SamplerParam::Vibrato) => format!("{sign}{d:.0} ct"),
        ModTarget::Sound(_, SoundParam::Velocity) => format!("{sign}{d:.0} vel"),
        ModTarget::Sound(_, SoundParam::Pitch) => format!("{sign}{d:.1} st"),
        ModTarget::Sound(_, SoundParam::Chance) => format!("{sign}{d:.0}%"),
        _ => format!("{sign}{:.0}%", d * 100.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(beats: Option<f64>) -> ModInput {
        ModInput {
            beats,
            seconds: 0.01,
            tempo: 120.0,
            ..Default::default()
        }
    }

    #[test]
    fn waves_rates_and_steps_follow_the_song() {
        // A beat-long sine: 0 on the beat, 1 a quarter of the way, -1 three quarters.
        let lfo = ModKind::lfo();
        let mut s = ModState::default();
        assert!(step(&lfo, &mut s, &input(Some(4.0))).abs() < 1e-4);
        assert!((step(&lfo, &mut s, &input(Some(4.25))) - 1.0).abs() < 1e-4);
        assert!((step(&lfo, &mut s, &input(Some(4.75))) + 1.0).abs() < 1e-4);
        for shape in LfoShape::ALL {
            for k in 0..20 {
                let v = shape.at(k as f64 / 20.0);
                assert!((-1.0..=1.0).contains(&v), "{shape:?} {v}");
            }
        }
        assert_eq!(LfoShape::Triangle.at(0.25), 1.0);
        assert_eq!(LfoShape::Triangle.at(0.75), -1.0);
        // Sixteenths of values: the third sixteenth's.
        let steps = ModKind::steps();
        let mut s = ModState::default();
        assert_eq!(step(&steps, &mut s, &input(Some(0.5))), 0.6);
        // Rates as musicians say them.
        let labels: Vec<String> = [16.0, 4.0, 1.0, 0.5, 0.375, 1.0 / 3.0, 0.25]
            .map(|b| ModRate::Beats(b).label())
            .into();
        assert_eq!(
            labels,
            ["4 bars", "1 bar", "1/4", "1/8", "1/16.", "1/8T", "1/16"]
        );
    }

    #[test]
    fn an_envelope_rises_with_a_note_and_a_follower_with_a_level() {
        let env = ModKind::Envelope {
            attack_ms: 10.0,
            decay_ms: 100.0,
            sustain: 0.5,
            release_ms: 100.0,
        };
        let mut s = ModState::default();
        let on = ModInput {
            trigger: true,
            gate: true,
            ..input(None)
        };
        let held = ModInput {
            gate: true,
            ..input(None)
        };
        assert_eq!(step(&env, &mut s, &on), 1.0); // a 10 ms attack in a 10 ms block
        for _ in 0..30 {
            step(&env, &mut s, &held);
        }
        assert!((s.value - 0.5).abs() < 1e-4, "{}", s.value); // held at the sustain
        for _ in 0..12 {
            step(&env, &mut s, &input(None));
        }
        assert_eq!(s.value, 0.0); // released
        // A follower: up to a loud track fast, down slowly.
        let follow = ModKind::follower(1);
        let mut s = ModState::default();
        let loud = ModInput {
            follow: 1.0,
            ..input(None)
        };
        for _ in 0..5 {
            step(&follow, &mut s, &loud);
        }
        assert!(s.value > 0.99);
        step(&follow, &mut s, &input(None));
        assert!(s.value > 0.9);
    }

    #[test]
    fn offsets_move_settings_in_their_units_and_stay_in_range() {
        let mut o = Offsets::default();
        o.add(ModTarget::Sampler(SamplerParam::Cutoff), 1.0);
        o.add(ModTarget::Sampler(SamplerParam::Cutoff), 1.0);
        o.add(ModTarget::Plugin(Which::Insert(0), 7), 0.25);
        o.add(ModTarget::EqGain(2), 30.0);
        let mut v = SamplerVoice {
            cutoff_hz: 1000.0,
            ..Default::default()
        };
        o.voice(&mut v);
        assert_eq!(v.cutoff_hz, 4000.0); // two octaves up
        assert_eq!(o.plugin(Which::Insert(0), 7), Some(0.25));
        o.add(ModTarget::LowCut, 2.0);
        let mut eq = Eq::classic();
        o.eq(&mut eq);
        // The old EQ's high mid, band 2; and its low cut, off, swept up from 20 Hz.
        assert_eq!(eq.bands[2].gain_db, 30.0);
        assert_eq!((eq.bands[0].freq, eq.bands[0].off), (80.0, false));
        assert_eq!(apply_pan(0.8, 0.5), 1.0);
        assert_eq!(depth_text(ModTarget::Volume, -12.0), "−12.0 dB");
    }

    #[test]
    fn automation_moves_a_modulators_settings_over_its_own() {
        // A macro set at 0, automated to 0.75: it gives 0.75.
        let mac = ModKind::Macro { value: 0.0 };
        let mut over = ModOver::default();
        over.set(ModParam::Value, 0.75);
        let mut s = ModState::default();
        assert_eq!(step_over(&mac, &mut s, &input(None), &over), 0.75);
        assert_eq!(step(&mac, &mut s, &input(None)), 0.0);
        // An LFO's phase turned a quarter: a sine at its start reads 1, not 0.
        let lfo = ModKind::lfo();
        let mut over = ModOver::default();
        over.set(ModParam::Phase, 0.25);
        let at_start = step_over(&lfo, &mut ModState::default(), &input(Some(0.0)), &over);
        assert!((at_start - 1.0).abs() < 1e-3, "{at_start}");
        // Out of range, kept in it; a setting it hasn't, none to get or set.
        over.set(ModParam::Value, 3.0);
        assert_eq!(step_over(&mac, &mut s, &input(None), &over), 1.0);
        assert_eq!(ModParam::Attack.get(&mac), None);
        assert_eq!(ModParam::of(&lfo), [ModParam::Phase]);
        assert_eq!(ModParam::parse("Release"), Some(ModParam::Release));
    }
}
