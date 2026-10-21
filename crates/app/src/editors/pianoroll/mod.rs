//! The piano roll: the selected MIDI region's notes against a keyboard, in the editor under the
//! tracks. It shares the tracks area's zoom and horizontal scroll,
//! so its grid lines up with the lanes above; ⌥-scroll zooms its rows.
//!
//! Notes are picked by clicking (shift adds), by a rubber band over the grid, or all with ⌘A;
//! dragged together, by either edge to resize, ⌥-dragged to copy; moved with the arrows (a
//! semitone, ⇧ an octave; left and right a grid step, ⌥ finer), copied, cut, pasted at the
//! playhead and duplicated after themselves (⌘C ⌘X ⌘V ⌘D), quantized with Q. Double-click (or
//! click, with the pencil) to write a note at the last length used, or a chord with a stamp; drag
//! the pencil on to paint more. With "In key" on, notes land on the song's scale. Under the notes,
//! a lane for their velocities (drag a dot, or draw across) or a controller's moves: the mod
//! wheel, pitch bend, expression or the sustain pedal, drawn freehand. Right-click for more.

mod body;
mod controls;
mod mouse;
mod ops;
mod view;

pub(crate) use controls::*;
pub(crate) use mouse::*;

use gpui::{
    App, Bounds, Context, Div, FontWeight, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent,
    Point, ScrollDelta, ScrollWheelEvent, Window, canvas, div, fill, point, prelude::*, px, rgb,
    size,
};
use pina_engine::{
    music::{Key, NoteEdit},
    song::project::{At, Content, ControlPoint, ControlValue, Note, PPQ, Region},
};

use crate::{
    ui::menu::{Menu, MenuAction},
    ui::theme::*,
    workspace::{Drag, Edge, Slider, Tool, Workspace},
};

/// The height of one pitch, as it opens.
pub const KEY_H: f32 = 12.0;
/// The lane under the notes, and how tall a full velocity (or controller) stands in it.
pub const VELOCITY_LANE: f32 = 78.0;
pub const VELOCITY_H: f32 = 60.0;
const ROLL_RULER: f32 = 20.0;
const KEYS_W: f32 = 56.0;

/// The scroll that puts `pitch` near the top, at the opening row height.
pub fn scroll_to(pitch: u8) -> f32 {
    ((127 - pitch as i32) as f32 * KEY_H - KEY_H * 2.0).max(0.0)
}

fn black(pitch: u8) -> bool {
    matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
}

/// Note names: middle C (60) is C3.
pub fn name(pitch: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
    ];
    format!("{}{}", NAMES[pitch as usize % 12], pitch as i32 / 12 - 2)
}

/// A pitch's name as a key spells it (B♭ in F major, A♯ in E major), middle C being C3;
/// with no key, sharps.
pub fn name_in(key: Option<Key>, pitch: u8) -> String {
    let Some(key) = key else {
        return name(pitch);
    };
    let (letter, octave, alter) = key.spell(pitch);
    let accidental = match alter {
        1 => "♯",
        -1 => "♭",
        _ => "",
    };
    let letter = ["C", "D", "E", "F", "G", "A", "B"][letter as usize];
    format!("{letter}{accidental}{}", octave - 1)
}

/// Where a note written at a tick goes: its track, the region it's in (if one takes it), else
/// the span of a new region round it.
type Target = (usize, Option<At>, Option<(u64, u64)>);

/// What a click writes: a note, or a chord stamped on it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Stamp {
    Note,
    Triad,
    Seventh,
    Ninth,
    Sus2,
    Sus4,
    Power,
    Octave,
}

impl Stamp {
    pub const ALL: [Stamp; 8] = [
        Stamp::Note,
        Stamp::Triad,
        Stamp::Seventh,
        Stamp::Ninth,
        Stamp::Sus2,
        Stamp::Sus4,
        Stamp::Power,
        Stamp::Octave,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Stamp::Note => "Note",
            Stamp::Triad => "Triad",
            Stamp::Seventh => "7th",
            Stamp::Ninth => "9th",
            Stamp::Sus2 => "Sus2",
            Stamp::Sus4 => "Sus4",
            Stamp::Power => "Power",
            Stamp::Octave => "Octave",
        }
    }

    /// The pitches it writes on `root`: stacked on the key's scale when the root's in it (so the
    /// chord stays in key: a triad on D in C major is D minor), else on a major chord.
    pub fn pitches(self, root: u8, key: Option<Key>) -> Vec<u8> {
        let degrees: &[usize] = match self {
            Stamp::Note => &[0],
            Stamp::Triad => &[0, 2, 4],
            Stamp::Seventh => &[0, 2, 4, 6],
            Stamp::Ninth => &[0, 2, 4, 6, 8],
            Stamp::Sus2 => &[0, 1, 4],
            Stamp::Sus4 => &[0, 3, 4],
            Stamp::Power => {
                return [root, root + 7, root + 12]
                    .into_iter()
                    .filter(|&p| p <= 127)
                    .collect();
            }
            Stamp::Octave => {
                return [root, root + 12]
                    .into_iter()
                    .filter(|&p| p <= 127)
                    .collect();
            }
        };
        let out: Vec<u8> = match key.filter(|k| k.contains(root)) {
            Some(k) => {
                // The scale upward from the root, as far as the widest stack needs.
                let scale: Vec<u8> = (root..=root.saturating_add(24))
                    .filter(|&p| k.contains(p))
                    .collect();
                degrees
                    .iter()
                    .filter_map(|&d| scale.get(d).copied())
                    .collect()
            }
            None => {
                const MAJOR: [u8; 9] = [0, 2, 4, 5, 7, 9, 11, 12, 14];
                degrees
                    .iter()
                    .map(|&d| root.saturating_add(MAJOR[d]))
                    .collect()
            }
        };
        out.into_iter().filter(|&p| p <= 127).collect()
    }
}

/// What the lane under the notes shows and edits.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Lane {
    Velocity,
    Mod,
    Bend,
    Expression,
    Sustain,
}

impl Lane {
    pub const ALL: [Lane; 5] = [
        Lane::Velocity,
        Lane::Mod,
        Lane::Bend,
        Lane::Expression,
        Lane::Sustain,
    ];

    pub fn short(self) -> &'static str {
        match self {
            Lane::Velocity => "Vel",
            Lane::Mod => "Mod",
            Lane::Bend => "Bend",
            Lane::Expression => "Expr",
            Lane::Sustain => "Sus",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Lane::Velocity => "Velocity",
            Lane::Mod => "Mod wheel",
            Lane::Bend => "Pitch bend",
            Lane::Expression => "Expression",
            Lane::Sustain => "Sustain",
        }
    }

    fn cc(self) -> Option<u8> {
        match self {
            Lane::Mod => Some(1),
            Lane::Expression => Some(11),
            Lane::Sustain => Some(64),
            _ => None,
        }
    }

    /// Whether a controller move is this lane's.
    fn holds(self, v: &ControlValue) -> bool {
        match (self, v) {
            (Lane::Bend, ControlValue::PitchBend { .. }) => true,
            (lane, ControlValue::Cc { number, .. }) => lane.cc() == Some(*number),
            _ => false,
        }
    }

    /// A move's height in the lane, 0 to 1 (pitch bend's middle, none, at a half).
    fn level(self, v: &ControlValue) -> f32 {
        match v {
            ControlValue::Cc { value, .. } => *value as f32 / 127.0,
            ControlValue::PitchBend { value } => (*value as f32 + 8192.0) / 16383.0,
            _ => 0.0,
        }
    }

    /// The move a height makes (the sustain pedal's down or up).
    fn make(self, level: f32) -> ControlValue {
        let level = level.clamp(0.0, 1.0);
        match self {
            Lane::Bend => {
                // Near the middle is no bend at all.
                let v = level * 16383.0 - 8192.0;
                ControlValue::PitchBend {
                    value: if v.abs() < 300.0 {
                        0
                    } else {
                        v.round().clamp(-8192.0, 8191.0) as i16
                    },
                }
            }
            Lane::Sustain => ControlValue::Cc {
                number: 64,
                value: if level >= 0.5 { 127 } else { 0 },
            },
            lane => ControlValue::Cc {
                number: lane.cc().unwrap_or(1),
                value: (level * 127.0).round() as u8,
            },
        }
    }

    /// Its value as words.
    fn say(self, v: &ControlValue) -> String {
        match v {
            ControlValue::PitchBend { value } => format!("{:+.2} st", *value as f32 / 8192.0 * 2.0),
            ControlValue::Cc { number: 64, value } => {
                if *value >= 64 {
                    "down".into()
                } else {
                    "up".into()
                }
            }
            ControlValue::Cc { value, .. } => value.to_string(),
            _ => String::new(),
        }
    }
}

/// Something done to the piano roll's notes: from a key, the context menu or the tools.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RollOp {
    SelectAll,
    /// Every note of a pitch.
    SelectPitch(u8),
    Deselect,
    Copy,
    Cut,
    Paste,
    Duplicate,
    Delete,
    /// Semitones; on the scale's notes with "In key" on, a semitone being a scale step.
    Transpose(i32),
    /// Quarters of the grid.
    Nudge(i32),
    Quantize,
    Edit(NoteEdit),
}

/// A velocity's colour: soft notes cool, hard ones hot.
pub fn velocity_color(v: u8) -> u32 {
    const STOPS: [(f32, u32); 6] = [
        (0.0, 0x6366f1),
        (0.3, 0x38bdf8),
        (0.5, 0x3ddc97),
        (0.7, 0xffc53d),
        (0.86, 0xff7a59),
        (1.0, 0xff4d6d),
    ];
    let t = v as f32 / 127.0;
    let k = STOPS
        .iter()
        .position(|s| s.0 >= t)
        .unwrap_or(STOPS.len() - 1)
        .max(1);
    let (a, b) = (STOPS[k - 1], STOPS[k]);
    let f = ((t - a.0) / (b.0 - a.0).max(1e-6)).clamp(0.0, 1.0);
    let mix = |shift: u32| {
        let (x, y) = (
            ((a.1 >> shift) & 0xff) as f32,
            ((b.1 >> shift) & 0xff) as f32,
        );
        ((x + (y - x) * f).round() as u32) << shift
    };
    mix(16) | mix(8) | mix(0)
}

#[cfg(test)]
mod spelling;
