//! The built-in tools that stack on a channel beside its reverb and delay: a filter that moves,
//! chorus and its kin, a transient shaper, a gate, a de-esser and a stereo tool.

use super::*;

/// A filter that moves: its cutoff swept by an LFO and by how loud the sound is.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct FilterFx {
    pub mode: FilterMode,
    pub cutoff_hz: f32,
    /// 0 to 1: a ringing peak at the cutoff.
    pub resonance: f32,
    /// How hard the sound goes in, 0 (clean) to 1.
    pub drive: f32,
    /// How far the LFO moves the cutoff, octaves (0 to 6), and its shape.
    pub lfo_depth: f32,
    pub lfo_wave: Wave,
    /// Its rate: `lfo_beats` a cycle in time with the song, or (0) `lfo_hz`.
    pub lfo_hz: f32,
    pub lfo_beats: f32,
    /// How far apart the two sides' LFOs are, 0 together to 1 opposite.
    pub lfo_stereo: f32,
    /// How far the sound's own level moves the cutoff, octaves (-6 to 6): a wah.
    pub env_depth: f32,
    pub env_attack_ms: f32,
    pub env_release_ms: f32,
    pub mix: f32,
    pub output_db: f32,
    pub bypass: bool,
}

impl Default for FilterFx {
    fn default() -> Self {
        FilterFx {
            mode: FilterMode::Ladder,
            cutoff_hz: 2000.0,
            resonance: 0.3,
            drive: 0.0,
            lfo_depth: 0.0,
            lfo_wave: Wave::Sine,
            lfo_hz: 1.0,
            lfo_beats: 2.0,
            lfo_stereo: 0.0,
            env_depth: 0.0,
            env_attack_ms: 5.0,
            env_release_ms: 120.0,
            mix: 1.0,
            output_db: 0.0,
            bypass: false,
        }
    }
}

/// What a motion effect does.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MotionStyle {
    /// A few copies, each drifting in pitch: thicker, wider.
    #[default]
    Chorus,
    /// Three copies a third of a cycle apart, shimmering: a string machine's.
    Ensemble,
    /// A very short delay swept, fed back: a jet's whoosh.
    Flanger,
    /// Notches swept through the spectrum.
    Phaser,
    /// All pitch wobble, none of the dry sound.
    Vibrato,
    /// Level wobble; with `spread` at 1, an auto-pan.
    Tremolo,
}

impl MotionStyle {
    pub const ALL: [MotionStyle; 6] = [
        MotionStyle::Chorus,
        MotionStyle::Ensemble,
        MotionStyle::Flanger,
        MotionStyle::Phaser,
        MotionStyle::Vibrato,
        MotionStyle::Tremolo,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MotionStyle::Chorus => "Chorus",
            MotionStyle::Ensemble => "Ensemble",
            MotionStyle::Flanger => "Flanger",
            MotionStyle::Phaser => "Phaser",
            MotionStyle::Vibrato => "Vibrato",
            MotionStyle::Tremolo => "Tremolo",
        }
    }

    /// One of this kind as it usually is: what picking it sets.
    pub fn motion(self) -> Motion {
        let m = Motion {
            style: self,
            ..Motion::default()
        };
        match self {
            MotionStyle::Chorus => m,
            MotionStyle::Ensemble => Motion {
                rate_hz: 0.6,
                depth: 0.55,
                delay_ms: 11.0,
                ..m
            },
            MotionStyle::Flanger => Motion {
                rate_hz: 0.25,
                depth: 0.8,
                delay_ms: 2.5,
                feedback: 0.6,
                ..m
            },
            MotionStyle::Phaser => Motion {
                rate_hz: 0.4,
                depth: 0.8,
                feedback: 0.5,
                stages: 6,
                ..m
            },
            MotionStyle::Vibrato => Motion {
                rate_hz: 5.0,
                depth: 0.35,
                delay_ms: 6.0,
                mix: 1.0,
                ..m
            },
            MotionStyle::Tremolo => Motion {
                rate_hz: 5.0,
                depth: 0.5,
                mix: 1.0,
                ..m
            },
        }
    }
}

/// Chorus, ensemble, flanger, phaser, vibrato or tremolo.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Motion {
    pub style: MotionStyle,
    /// Its rate: `beats` a cycle in time with the song, or (0) `rate_hz`.
    pub rate_hz: f32,
    pub beats: f32,
    /// How far it moves, 0 to 1.
    pub depth: f32,
    /// How much comes round again, -0.95 to 0.95 (flanger and phaser).
    pub feedback: f32,
    /// The delay it sweeps around, ms (chorus, ensemble, flanger, vibrato).
    pub delay_ms: f32,
    /// A phaser's stages, 2 to 12.
    pub stages: u8,
    /// How far apart the two sides move, 0 together to 1 opposite.
    pub spread: f32,
    pub mix: f32,
    pub output_db: f32,
    pub bypass: bool,
}

impl Default for Motion {
    fn default() -> Self {
        Motion {
            style: MotionStyle::Chorus,
            rate_hz: 0.8,
            beats: 0.0,
            depth: 0.5,
            feedback: 0.0,
            delay_ms: 16.0,
            stages: 4,
            spread: 0.5,
            mix: 0.5,
            output_db: 0.0,
            bypass: false,
        }
    }
}

/// A transient shaper: a sound's fronts and tails made louder or softer, whatever its level.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Shaper {
    /// Its fronts, -1 softer to 1 harder (up to 18 dB).
    pub attack: f32,
    /// What rings after them, -1 tighter to 1 longer (up to 18 dB).
    pub sustain: f32,
    /// How long a front lasts, ms.
    pub speed_ms: f32,
    /// Rounds off what goes over full scale.
    pub clip: bool,
    pub mix: f32,
    pub output_db: f32,
    pub bypass: bool,
}

impl Default for Shaper {
    fn default() -> Self {
        Shaper {
            attack: 0.0,
            sustain: 0.0,
            speed_ms: 60.0,
            clip: true,
            mix: 1.0,
            output_db: 0.0,
            bypass: false,
        }
    }
}

/// A gate or expander: what falls under its threshold comes down.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Gate {
    pub threshold_db: f32,
    /// How far down it goes, dB.
    pub range_db: f32,
    /// How steeply it falls under the threshold: 1.5 an expander's, 20 a gate's.
    pub ratio: f32,
    pub attack_ms: f32,
    pub hold_ms: f32,
    pub release_ms: f32,
    /// What its ear hears: a kick's lows, say, so the hats don't open it.
    pub low_cut_hz: f32,
    pub high_cut_hz: f32,
    pub output_db: f32,
    pub bypass: bool,
}

impl Default for Gate {
    fn default() -> Self {
        Gate {
            threshold_db: -36.0,
            range_db: 40.0,
            ratio: 8.0,
            attack_ms: 1.0,
            hold_ms: 40.0,
            release_ms: 120.0,
            low_cut_hz: 20.0,
            high_cut_hz: 20_000.0,
            output_db: 0.0,
            bypass: false,
        }
    }
}

/// A de-esser: a voice's esses turned down as they come, the rest left alone.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct DeEsser {
    /// Where the esses are, Hz, and how wide a band it hears (octaves).
    pub freq_hz: f32,
    pub width: f32,
    pub threshold_db: f32,
    /// The most it turns down, dB.
    pub range_db: f32,
    /// Turns the whole sound down, not just the band.
    pub wide: bool,
    /// Hear only what it's hearing, to find them.
    pub listen: bool,
    pub bypass: bool,
}

impl Default for DeEsser {
    fn default() -> Self {
        DeEsser {
            freq_hz: 6500.0,
            width: 1.2,
            threshold_db: -30.0,
            range_db: 8.0,
            wide: false,
            listen: false,
            bypass: false,
        }
    }
}

/// A stereo tool: how wide each part of the spectrum is, the lows made mono, the sides' balance.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Stereo {
    /// How wide the lows, middle and highs are, 0 mono, 1 as they are, 2 wider.
    pub width: [f32; 3],
    /// Where they split, Hz.
    pub crossovers: [f32; 2],
    /// Everything under this is mono, Hz (0 none).
    pub mono_hz: f32,
    pub mid_db: f32,
    pub side_db: f32,
    /// -1 left to 1 right.
    pub balance: f32,
    pub swap: bool,
    pub invert_left: bool,
    pub invert_right: bool,
    pub output_db: f32,
    pub bypass: bool,
}

impl Default for Stereo {
    fn default() -> Self {
        Stereo {
            width: [1.0; 3],
            crossovers: [250.0, 4000.0],
            mono_hz: 0.0,
            mid_db: 0.0,
            side_db: 0.0,
            balance: 0.0,
            swap: false,
            invert_left: false,
            invert_right: false,
            output_db: 0.0,
            bypass: false,
        }
    }
}

/// A multiband compressor: the sound split in three at `crossovers`, phase intact, each band
/// compressed alone, and lifted where it's quiet (`lift`, the huge, close sound).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Multiband {
    pub crossovers: [f32; 2],
    pub threshold_db: [f32; 3],
    pub ratio: [f32; 3],
    /// How far each band's quiet parts come up, 0 to 1.
    pub lift: [f32; 3],
    pub gain_db: [f32; 3],
    pub attack_ms: f32,
    pub release_ms: f32,
    pub mix: f32,
    pub output_db: f32,
    pub bypass: bool,
}

impl Default for Multiband {
    fn default() -> Self {
        Multiband {
            crossovers: [160.0, 2500.0],
            threshold_db: [-18.0; 3],
            ratio: [3.0; 3],
            lift: [0.0; 3],
            gain_db: [0.0; 3],
            attack_ms: 10.0,
            release_ms: 120.0,
            mix: 1.0,
            output_db: 0.0,
            bypass: false,
        }
    }
}

/// A pitch shifter: the sound moved, its length kept.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct PitchShift {
    pub semitones: f32,
    pub cents: f32,
    /// What it plays back into itself, each round moved again: rising or falling stacks.
    pub feedback: f32,
    pub mix: f32,
    pub output_db: f32,
    pub bypass: bool,
}

impl Default for PitchShift {
    fn default() -> Self {
        PitchShift {
            semitones: 0.0,
            cents: 0.0,
            feedback: 0.0,
            mix: 1.0,
            output_db: 0.0,
            bypass: false,
        }
    }
}

/// A tape stop: `stop` up, the sound slows to nothing as a tape does; down, it spins back up.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct TapeStop {
    /// Over a half, it's stopping: for automation to throw.
    pub stop: f32,
    /// How long it takes to stop and to start, beats.
    pub stop_beats: f32,
    pub start_beats: f32,
    /// 0 slows evenly; 1 falls away fast, then drags.
    pub curve: f32,
    pub mix: f32,
    pub output_db: f32,
    pub bypass: bool,
}

impl Default for TapeStop {
    fn default() -> Self {
        TapeStop {
            stop: 0.0,
            stop_beats: 1.0,
            start_beats: 0.5,
            curve: 0.4,
            mix: 1.0,
            output_db: 0.0,
            bypass: false,
        }
    }
}

/// Pitch correction: a voice brought to the nearest of `notes`, its character kept.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Tune {
    /// Which of the twelve notes, from C, it may land on.
    pub notes: [bool; 12],
    /// How quickly it gets there: 0 snaps, the robot; 40 ms and more leaves a voice human.
    pub speed_ms: f32,
    /// How much of the way, 0 to 1.
    pub amount: f32,
    /// The range to listen in.
    pub low_hz: f32,
    pub high_hz: f32,
    pub output_db: f32,
    pub bypass: bool,
}

impl Default for Tune {
    fn default() -> Self {
        Tune {
            notes: [true; 12],
            speed_ms: 40.0,
            amount: 1.0,
            low_hz: 80.0,
            high_hz: 1000.0,
            output_db: 0.0,
            bypass: false,
        }
    }
}
