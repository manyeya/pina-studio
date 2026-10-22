//! Regions and what they hold: notes, controllers, audio, takes and comps.

use super::*;

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

pub(crate) fn is_none_u64(v: &u64) -> bool {
    *v == 0
}

pub(crate) fn is_zero(v: &i8) -> bool {
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
        /// How it's stretched: Polyphonic, Slicing or Speed (see `crate::audio::stretch`).
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

pub(crate) fn one() -> f64 {
    1.0
}

pub(crate) fn is_one(v: &f64) -> bool {
    *v == 1.0
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

pub(crate) fn pieces(segments: impl IntoIterator<Item = (u64, u64, usize)>) -> Vec<CompPiece> {
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
