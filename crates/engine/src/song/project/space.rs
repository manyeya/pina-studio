//! The built-in reverb and delay, as a channel has them: the space a sound is put in, and its
//! echoes. Each stacks on a channel like the other built-ins (see `Unit`), most often on an aux
//! that tracks send to.

use super::*;

/// The kind of space a reverb builds: how big, how its first reflections fall, how dense and
/// bright its tail is.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReverbStyle {
    /// A small room: close walls, the reflections early and many.
    Room,
    /// A chamber: a live, mid-sized room, warm and even.
    Chamber,
    /// A concert hall: wide, the reflections spread out, a long smooth tail.
    #[default]
    Hall,
    /// A plate: no walls, dense from the first instant, bright.
    Plate,
    /// Ambience: the walls and none of the tail, to put a sound in a place.
    Ambience,
    /// A cathedral: huge, slow to build, very long.
    Cathedral,
    /// A hall whose tail climbs an octave as it rings.
    Shimmer,
}

impl ReverbStyle {
    pub const ALL: [ReverbStyle; 7] = [
        ReverbStyle::Room,
        ReverbStyle::Chamber,
        ReverbStyle::Hall,
        ReverbStyle::Plate,
        ReverbStyle::Ambience,
        ReverbStyle::Cathedral,
        ReverbStyle::Shimmer,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ReverbStyle::Room => "Room",
            ReverbStyle::Chamber => "Chamber",
            ReverbStyle::Hall => "Hall",
            ReverbStyle::Plate => "Plate",
            ReverbStyle::Ambience => "Ambience",
            ReverbStyle::Cathedral => "Cathedral",
            ReverbStyle::Shimmer => "Shimmer",
        }
    }

    /// A reverb of this kind as it usually is: its size, decay, first reflections, pre-delay,
    /// brightness and the rest; what picking it sets.
    pub fn reverb(self) -> Reverb {
        let r = Reverb {
            style: self,
            ..Reverb::default()
        };
        match self {
            ReverbStyle::Room => Reverb {
                size: 0.3,
                decay_s: 0.7,
                predelay_ms: 4.0,
                early: 0.7,
                diffusion: 0.75,
                damping_hz: 7000.0,
                ..r
            },
            ReverbStyle::Chamber => Reverb {
                size: 0.5,
                decay_s: 1.4,
                predelay_ms: 12.0,
                early: 0.5,
                diffusion: 0.8,
                damping_hz: 6000.0,
                bass: 1.1,
                ..r
            },
            ReverbStyle::Hall => r,
            ReverbStyle::Plate => Reverb {
                size: 0.55,
                decay_s: 1.8,
                predelay_ms: 10.0,
                early: 0.0,
                diffusion: 1.0,
                damping_hz: 12000.0,
                bass: 0.8,
                modulation: 0.2,
                ..r
            },
            ReverbStyle::Ambience => Reverb {
                size: 0.2,
                decay_s: 0.4,
                predelay_ms: 0.0,
                early: 1.0,
                diffusion: 0.6,
                damping_hz: 8000.0,
                ..r
            },
            ReverbStyle::Cathedral => Reverb {
                size: 1.0,
                decay_s: 6.0,
                predelay_ms: 40.0,
                early: 0.35,
                diffusion: 0.85,
                damping_hz: 4500.0,
                bass: 1.25,
                modulation: 0.45,
                ..r
            },
            ReverbStyle::Shimmer => Reverb {
                size: 0.85,
                decay_s: 5.0,
                predelay_ms: 30.0,
                early: 0.2,
                damping_hz: 9000.0,
                modulation: 0.5,
                shimmer: 0.45,
                ..r
            },
        }
    }
}

/// A channel's built-in reverb.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Reverb {
    pub style: ReverbStyle,
    /// How long before its first reflection, ms (0 to 500); with `predelay_beats`, in beats
    /// instead, so it moves with the tempo.
    pub predelay_ms: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predelay_beats: Option<f32>,
    /// How big the space is, 0 (a booth) to 1 (the biggest of its kind).
    pub size: f32,
    /// How long its tail takes to fall 60 dB, seconds (0.1 to 30).
    pub decay_s: f32,
    /// How loud its first reflections are, 0 to 1.
    pub early: f32,
    /// How quickly its echoes blur into a smooth tail, 0 (grainy) to 1 (smooth at once).
    pub diffusion: f32,
    /// Above this, Hz, the tail dies away faster: lower is darker.
    pub damping_hz: f32,
    /// How much longer (over 1) or shorter its lows ring than the rest.
    pub bass: f32,
    /// How much its tail moves (a chorus in it), 0 to 1: lusher, never metallic.
    pub modulation: f32,
    /// The tail's low and high cuts, Hz.
    pub low_cut_hz: f32,
    pub high_cut_hz: f32,
    /// Its stereo width: 0 mono, 1 as it is, 2 wider.
    pub width: f32,
    /// How far the tail ducks while the sound going in plays, 0 to 1: clear while it plays,
    /// blooming in the gaps.
    pub ducking: f32,
    /// How much of the tail climbs an octave as it rings, 0 to 1.
    #[serde(skip_serializing_if = "is_off")]
    pub shimmer: f32,
    /// Holds the tail as it is, forever, nothing new going in.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub freeze: bool,
    /// How much of the reverb is heard, 0 (dry) to 1 (only the reverb, as on an aux).
    pub mix: f32,
    pub output_db: f32,
    pub bypass: bool,
}

fn is_off(v: &f32) -> bool {
    *v == 0.0
}

impl Default for Reverb {
    fn default() -> Self {
        Reverb {
            style: ReverbStyle::Hall,
            predelay_ms: 20.0,
            predelay_beats: None,
            size: 0.7,
            decay_s: 2.4,
            early: 0.4,
            diffusion: 0.85,
            damping_hz: 6500.0,
            bass: 1.15,
            modulation: 0.35,
            low_cut_hz: 120.0,
            high_cut_hz: 12000.0,
            width: 1.0,
            ducking: 0.0,
            shimmer: 0.0,
            freeze: false,
            mix: 0.3,
            output_db: 0.0,
            bypass: false,
        }
    }
}

impl Reverb {
    /// Its pre-delay in seconds at `bpm`.
    pub fn predelay_s(&self, bpm: f64) -> f32 {
        match self.predelay_beats {
            Some(b) => (b as f64 * 60.0 / bpm.max(1.0)) as f32,
            None => self.predelay_ms / 1000.0,
        }
        .clamp(0.0, 0.5)
    }
}

/// How a delay's echoes sound.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DelayStyle {
    /// Clean: each echo as it went in.
    #[default]
    Digital,
    /// Tape: echoes that wobble, warm and saturate as they repeat.
    Tape,
    /// A bucket brigade: darker each time round, softly driven.
    Analog,
    /// Bouncing left and right.
    PingPong,
    /// Dub: long, dark, thinned repeats that pile up.
    Dub,
    /// One quick echo, as off a near wall.
    Slapback,
}

impl DelayStyle {
    pub const ALL: [DelayStyle; 6] = [
        DelayStyle::Digital,
        DelayStyle::Tape,
        DelayStyle::Analog,
        DelayStyle::PingPong,
        DelayStyle::Dub,
        DelayStyle::Slapback,
    ];

    pub fn name(self) -> &'static str {
        match self {
            DelayStyle::Digital => "Digital",
            DelayStyle::Tape => "Tape",
            DelayStyle::Analog => "Analog",
            DelayStyle::PingPong => "Ping-Pong",
            DelayStyle::Dub => "Dub",
            DelayStyle::Slapback => "Slapback",
        }
    }

    /// A delay of this kind as it usually is: what picking it sets.
    pub fn delay(self) -> Delay {
        let d = Delay {
            style: self,
            ..Delay::default()
        };
        match self {
            DelayStyle::Digital => d,
            DelayStyle::Tape => Delay {
                feedback: 0.45,
                high_cut_hz: 6000.0,
                drive: 0.35,
                wow: 0.35,
                flutter: 0.3,
                ..d
            },
            DelayStyle::Analog => Delay {
                feedback: 0.45,
                high_cut_hz: 3500.0,
                low_cut_hz: 150.0,
                drive: 0.3,
                wow: 0.1,
                ..d
            },
            DelayStyle::PingPong => Delay {
                beats: [0.75, 0.75],
                cross: 1.0,
                feedback: 0.5,
                ..d
            },
            DelayStyle::Dub => Delay {
                beats: [0.75, 0.75],
                feedback: 0.72,
                low_cut_hz: 350.0,
                high_cut_hz: 2500.0,
                drive: 0.45,
                wow: 0.2,
                diffusion: 0.25,
                ..d
            },
            DelayStyle::Slapback => Delay {
                sync: false,
                ms: [95.0, 105.0],
                feedback: 0.08,
                high_cut_hz: 7000.0,
                drive: 0.2,
                ..d
            },
        }
    }
}

/// A channel's built-in delay.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Delay {
    pub style: DelayStyle,
    /// In time with the song: each side's time in beats (0.75 a dotted eighth); else in ms.
    pub sync: bool,
    pub beats: [f32; 2],
    pub ms: [f32; 2],
    /// How much of each echo comes round again, 0 to 1.1 (past 1 it grows, held by its drive).
    pub feedback: f32,
    /// How much each echo crosses to the other side, 0 to 1 (1 ping-pongs).
    pub cross: f32,
    /// What each echo keeps: its low and high cuts, Hz, each time round.
    pub low_cut_hz: f32,
    pub high_cut_hz: f32,
    /// How hard each echo is driven, 0 to 1: warmer, and held in.
    pub drive: f32,
    /// Slow and fast pitch wobble, as tape: 0 to 1.
    pub wow: f32,
    pub flutter: f32,
    /// How much each echo blurs, 0 (sharp) to 1 (a smear).
    pub diffusion: f32,
    /// How far the echoes duck while the sound going in plays, 0 to 1.
    pub ducking: f32,
    pub width: f32,
    /// Holds the echoes going round, nothing new going in.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub freeze: bool,
    /// How much of the echoes is heard, 0 to 1 (1 only the echoes, as on an aux).
    pub mix: f32,
    pub output_db: f32,
    pub bypass: bool,
}

impl Default for Delay {
    fn default() -> Self {
        Delay {
            style: DelayStyle::Digital,
            sync: true,
            beats: [0.5, 0.5],
            ms: [250.0, 250.0],
            feedback: 0.35,
            cross: 0.0,
            low_cut_hz: 80.0,
            high_cut_hz: 12000.0,
            drive: 0.0,
            wow: 0.0,
            flutter: 0.0,
            diffusion: 0.0,
            ducking: 0.0,
            width: 1.0,
            freeze: false,
            mix: 0.25,
            output_db: 0.0,
            bypass: false,
        }
    }
}

impl Delay {
    /// Each side's time in seconds at `bpm`, at most `MAX_DELAY_S`.
    pub fn times_s(&self, bpm: f64) -> [f32; 2] {
        let beat = 60.0 / bpm.max(1.0) as f32;
        std::array::from_fn(|k| {
            match self.sync {
                true => self.beats[k] * beat,
                false => self.ms[k] / 1000.0,
            }
            .clamp(0.001, MAX_DELAY_S)
        })
    }
}

/// The longest a delay waits, seconds.
pub const MAX_DELAY_S: f32 = 5.0;
