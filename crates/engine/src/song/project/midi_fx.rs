//! MIDI effects: what changes the notes an instrument track plays before its instrument hears
//! them, live and from the song alike. An arpeggiator, chords from one note, notes kept to a
//! scale, moved, made harder or softer, repeated, varied, and controllers sent elsewhere.

use serde::{Deserialize, Serialize};

use crate::music::Key;

/// The order an arpeggiator plays what's held in.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ArpOrder {
    #[default]
    Up,
    Down,
    UpDown,
    /// In the order the keys went down.
    Played,
    Random,
    /// Everything held, at once, on each step.
    Chord,
}

impl ArpOrder {
    pub const ALL: [ArpOrder; 6] = [
        ArpOrder::Up,
        ArpOrder::Down,
        ArpOrder::UpDown,
        ArpOrder::Played,
        ArpOrder::Random,
        ArpOrder::Chord,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ArpOrder::Up => "Up",
            ArpOrder::Down => "Down",
            ArpOrder::UpDown => "Up & down",
            ArpOrder::Played => "As played",
            ArpOrder::Random => "Random",
            ArpOrder::Chord => "Chord",
        }
    }
}

/// The chord one note plays.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChordShape {
    #[default]
    Major,
    Minor,
    Seventh,
    MajorSeventh,
    MinorSeventh,
    Sus2,
    Sus4,
    Power,
    Octave,
    /// The scale's triad on the note: in C major, D gives D minor.
    InKey,
    /// The scale's seventh chord on the note.
    InKeySeventh,
}

impl ChordShape {
    pub const ALL: [ChordShape; 11] = [
        ChordShape::Major,
        ChordShape::Minor,
        ChordShape::Seventh,
        ChordShape::MajorSeventh,
        ChordShape::MinorSeventh,
        ChordShape::Sus2,
        ChordShape::Sus4,
        ChordShape::Power,
        ChordShape::Octave,
        ChordShape::InKey,
        ChordShape::InKeySeventh,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ChordShape::Major => "Major",
            ChordShape::Minor => "Minor",
            ChordShape::Seventh => "7th",
            ChordShape::MajorSeventh => "Maj 7th",
            ChordShape::MinorSeventh => "Min 7th",
            ChordShape::Sus2 => "Sus2",
            ChordShape::Sus4 => "Sus4",
            ChordShape::Power => "Power",
            ChordShape::Octave => "Octave",
            ChordShape::InKey => "In key",
            ChordShape::InKeySeventh => "In key 7th",
        }
    }

    /// The pitches `root` plays: stacked on `key`'s scale for the in-key shapes (a major chord
    /// with no key, or the note out of it), else on the shape's own intervals.
    pub fn pitches(self, root: u8, key: Option<Key>) -> Vec<u8> {
        let intervals: &[u8] = match self {
            ChordShape::Major => &[0, 4, 7],
            ChordShape::Minor => &[0, 3, 7],
            ChordShape::Seventh => &[0, 4, 7, 10],
            ChordShape::MajorSeventh => &[0, 4, 7, 11],
            ChordShape::MinorSeventh => &[0, 3, 7, 10],
            ChordShape::Sus2 => &[0, 2, 7],
            ChordShape::Sus4 => &[0, 5, 7],
            ChordShape::Power => &[0, 7, 12],
            ChordShape::Octave => &[0, 12],
            ChordShape::InKey | ChordShape::InKeySeventh => {
                let stack = if self == ChordShape::InKey { 3 } else { 4 };
                if let Some(k) = key.filter(|k| k.contains(root)) {
                    let scale: Vec<u8> = (root..=root.saturating_add(24))
                        .filter(|&p| k.contains(p))
                        .collect();
                    return (0..stack)
                        .filter_map(|d| scale.get(d * 2).copied())
                        .filter(|&p| p <= 127)
                        .collect();
                }
                &[0, 4, 7][..]
            }
        };
        intervals
            .iter()
            .map(|&i| root.saturating_add(i))
            .filter(|&p| p <= 127)
            .collect()
    }
}

/// One MIDI effect and its settings. Rates are in beats (a quarter note is 1).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MidiFx {
    /// What's held, played a note at a time on the beat: `octaves` up, each `gate` of a step
    /// long; with `latch` it plays on after the keys let go, until new ones go down.
    Arpeggiator {
        rate: f32,
        order: ArpOrder,
        octaves: u8,
        gate: f32,
        latch: bool,
    },
    /// Each note a chord.
    ChordTrigger {
        shape: ChordShape,
    },
    /// Each note kept to a scale: the song's (`root` None), or its own.
    ScaleQuantizer {
        root: Option<u8>,
        minor: bool,
    },
    Transposer {
        semitones: i8,
    },
    /// Velocities as `scale` percent of what was played, plus `add`, kept from `min` to `max`.
    Velocity {
        scale: u8,
        add: i8,
        min: u8,
        max: u8,
    },
    /// Each note played again `count` times, `rate` apart, each `decay` as hard as the last and
    /// `transpose` semitones from it.
    NoteRepeater {
        count: u8,
        rate: f32,
        decay: f32,
        transpose: i8,
    },
    /// Each note moved up to `pitch` semitones and `velocity` either way, and played with
    /// `chance` percent.
    Randomizer {
        pitch: u8,
        velocity: u8,
        chance: u8,
    },
    /// A controller's moves sent as another's (`to` 0: dropped), scaled.
    Modifier {
        from: u8,
        to: u8,
        scale: u8,
    },
}

impl MidiFx {
    pub const KINDS: [&'static str; 8] = [
        "Arpeggiator",
        "Chord Trigger",
        "Scale Quantizer",
        "Transposer",
        "Velocity Processor",
        "Note Repeater",
        "Randomizer",
        "Modifier",
    ];

    /// A fresh one of a kind, by its name in `KINDS`, set as it's most often wanted.
    pub fn fresh(kind: &str) -> Option<MidiFx> {
        Some(match kind {
            "Arpeggiator" => MidiFx::Arpeggiator {
                rate: 0.25,
                order: ArpOrder::Up,
                octaves: 1,
                gate: 0.8,
                latch: false,
            },
            "Chord Trigger" => MidiFx::ChordTrigger {
                shape: ChordShape::InKey,
            },
            "Scale Quantizer" => MidiFx::ScaleQuantizer {
                root: None,
                minor: false,
            },
            "Transposer" => MidiFx::Transposer { semitones: 12 },
            "Velocity Processor" => MidiFx::Velocity {
                scale: 100,
                add: 0,
                min: 1,
                max: 127,
            },
            "Note Repeater" => MidiFx::NoteRepeater {
                count: 3,
                rate: 0.25,
                decay: 0.7,
                transpose: 0,
            },
            "Randomizer" => MidiFx::Randomizer {
                pitch: 0,
                velocity: 20,
                chance: 100,
            },
            "Modifier" => MidiFx::Modifier {
                from: 1,
                to: 74,
                scale: 100,
            },
            _ => return None,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            MidiFx::Arpeggiator { .. } => "Arpeggiator",
            MidiFx::ChordTrigger { .. } => "Chord Trigger",
            MidiFx::ScaleQuantizer { .. } => "Scale Quantizer",
            MidiFx::Transposer { .. } => "Transposer",
            MidiFx::Velocity { .. } => "Velocity Processor",
            MidiFx::NoteRepeater { .. } => "Note Repeater",
            MidiFx::Randomizer { .. } => "Randomizer",
            MidiFx::Modifier { .. } => "Modifier",
        }
    }
}

/// A MIDI effect on a track, and whether it's bypassed.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct MidiFxSlot {
    #[serde(flatten)]
    pub fx: MidiFx,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bypass: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_stack_on_their_intervals_or_the_keys_scale() {
        assert_eq!(ChordShape::Minor.pitches(60, None), [60, 63, 67]);
        let c = Some(Key {
            root: 0,
            minor: false,
        });
        // D in C major: D minor; G: G7 as a seventh.
        assert_eq!(ChordShape::InKey.pitches(62, c), [62, 65, 69]);
        assert_eq!(ChordShape::InKeySeventh.pitches(67, c), [67, 71, 74, 77]);
        // Out of the key, or with none: major.
        assert_eq!(ChordShape::InKey.pitches(61, c), [61, 65, 68]);
        assert_eq!(ChordShape::Octave.pitches(120, None), [120]);
    }

    #[test]
    fn a_slot_saves_as_its_effect_with_its_settings() {
        let slot = MidiFxSlot {
            fx: MidiFx::fresh("Transposer").unwrap(),
            bypass: false,
        };
        let json = serde_json::to_string(&slot).unwrap();
        assert_eq!(json, r#"{"type":"transposer","semitones":12}"#);
        assert_eq!(serde_json::from_str::<MidiFxSlot>(&json).unwrap(), slot);
        assert!(
            MidiFx::KINDS
                .iter()
                .all(|k| MidiFx::fresh(k).is_some_and(|f| f.name() == *k))
        );
    }
}
