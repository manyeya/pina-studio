//! Notes and controls as MIDI 1.0 for plugins that take only that (Audio Units and VST2): on
//! channel 1, or with MPE on, each note on a channel of its own (2 to 16) with its bend, slide
//! and pressure on that channel, so the plugin hears each note's expression apart.

use super::{ControlKind, ExpressionKind, Note};

/// How far a member channel's bend goes, either way, in semitones: MPE's default.
pub const MPE_BEND: f32 = 48.0;

/// The zone's setup: 15 member channels (an MPE Configuration Message on channel 1), each one
/// bending `MPE_BEND` semitones.
const SETUP: usize = 3 + 15 * 4;

/// MIDI as a plugin hears it: which channel each note sounding is on.
#[derive(Clone, Copy)]
pub(crate) struct Mpe {
    on: bool,
    /// Each pitch's channel (0 to 15) while it sounds with MPE on.
    channel: [u8; 128],
    /// How many notes each member channel holds.
    holding: [u8; 16],
    next: u8,
    told: bool,
}

fn seven(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 127.0).round() as u8
}

impl Mpe {
    pub(crate) fn new(on: bool) -> Self {
        Mpe {
            on,
            channel: [0; 128],
            holding: [0; 16],
            next: 1,
            told: false,
        }
    }

    pub(crate) fn set(&mut self, on: bool) {
        if on != self.on {
            *self = Mpe::new(on);
        }
    }

    /// The zone's setup, the first time it's asked for with MPE on.
    pub(crate) fn setup(&mut self) -> Option<[[u8; 3]; SETUP]> {
        if !self.on || self.told {
            return None;
        }
        self.told = true;
        let mut out = [[0u8; 3]; SETUP];
        // RPN 6 on the manager channel: 15 member channels.
        out[..3].copy_from_slice(&[[0xB0, 101, 0], [0xB0, 100, 6], [0xB0, 6, 15]]);
        for ch in 1..16u8 {
            let at = 3 + (ch as usize - 1) * 4;
            let cc = 0xB0 | ch;
            // RPN 0 on each member channel: its bend range.
            out[at..at + 4].copy_from_slice(&[
                [cc, 101, 0],
                [cc, 100, 0],
                [cc, 6, MPE_BEND as u8],
                [cc, 38, 0],
            ]);
        }
        Some(out)
    }

    /// A note on (velocity 1 to 127) or off, as `send` is given it: on channel 1, or with MPE on,
    /// on the member channel holding fewest notes from the one after the last used (where its
    /// note-on went, for an off), that channel's bend, slide and pressure set back first.
    pub(crate) fn note(&mut self, n: &Note, mut send: impl FnMut([u8; 3])) {
        let pitch = n.pitch & 127;
        let on = n.velocity > 0.0;
        let ch = match (self.on, on) {
            (false, _) => 0,
            (true, true) => {
                let ch = (0..15u8)
                    .map(|k| 1 + (self.next - 1 + k) % 15)
                    .min_by_key(|&c| self.holding[c as usize])
                    .unwrap_or(1);
                self.next = ch % 15 + 1;
                self.holding[ch as usize] = self.holding[ch as usize].saturating_add(1);
                self.channel[pitch as usize] = ch;
                // A fresh note starts unbent, slid to the middle and unpressed.
                send([0xE0 | ch, 0, 64]);
                send([0xB0 | ch, 74, 64]);
                send([0xD0 | ch, 0, 0]);
                ch
            }
            (true, false) => {
                let ch = std::mem::take(&mut self.channel[pitch as usize]);
                self.holding[ch as usize] = self.holding[ch as usize].saturating_sub(1);
                ch
            }
        };
        send(match on {
            true => [0x90 | ch, pitch, seven(n.velocity).max(1)],
            false => [0x80 | ch, pitch, 0],
        });
    }

    /// A control as MIDI, if MIDI has it: a channel's on channel 1; a note's own on its channel
    /// with MPE on (its bend, slide as CC 74, pressure), else its pressure as polyphonic aftertouch.
    pub(crate) fn control(&self, kind: ControlKind) -> Option<[u8; 3]> {
        let bend = |ch: u8, v: f32| {
            let b = (8192.0 + v.clamp(-1.0, 1.0) * 8192.0)
                .round()
                .clamp(0.0, 16383.0) as u16;
            [0xE0 | ch, (b & 0x7F) as u8, (b >> 7) as u8]
        };
        // A note's channel, while it sounds.
        let of = |pitch: u8| Some(self.channel[pitch as usize & 127]).filter(|&c| c > 0);
        Some(match kind {
            ControlKind::Cc { number, value } => [0xB0, number & 127, seven(value)],
            ControlKind::PitchBend(v) => bend(0, v),
            ControlKind::Aftertouch(v) => [0xD0, seven(v), 0],
            ControlKind::PolyPressure { pitch, value } => match self.on {
                true => [0xD0 | of(pitch)?, seven(value), 0],
                false => [0xA0, pitch & 127, seven(value)],
            },
            ControlKind::Expression { pitch, kind, value } if self.on => {
                let ch = of(pitch)?;
                match kind {
                    ExpressionKind::Tuning => bend(ch, value / MPE_BEND),
                    ExpressionKind::Brightness => [0xB0 | ch, 74, seven(value)],
                    ExpressionKind::Expression => [0xD0 | ch, seven(value), 0],
                    _ => return None,
                }
            }
            ControlKind::Expression { .. } => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(pitch: u8, velocity: f32) -> Note {
        Note {
            offset: 0,
            pitch,
            velocity,
        }
    }

    /// What a note sends.
    fn sent(m: &mut Mpe, n: Note) -> Vec<[u8; 3]> {
        let mut out = Vec::new();
        m.note(&n, |x| out.push(x));
        out
    }

    #[test]
    fn off_everything_is_on_channel_one() {
        let mut m = Mpe::new(false);
        assert!(m.setup().is_none());
        assert_eq!(sent(&mut m, note(60, 0.001)), [[0x90, 60, 1]]);
        assert_eq!(sent(&mut m, note(60, 0.0)), [[0x80, 60, 0]]);
        let bend = |v| m.control(ControlKind::PitchBend(v));
        assert_eq!(bend(0.0), Some([0xE0, 0, 64]));
        assert_eq!(bend(1.0), Some([0xE0, 127, 127]));
        let pressed = ControlKind::PolyPressure {
            pitch: 60,
            value: 1.0,
        };
        assert_eq!(m.control(pressed), Some([0xA0, 60, 127]));
        let tuned = ControlKind::Expression {
            pitch: 60,
            kind: ExpressionKind::Tuning,
            value: 1.0,
        };
        assert_eq!(m.control(tuned), None);
    }

    #[test]
    fn on_each_note_has_its_channel_and_its_expression_goes_there() {
        let mut m = Mpe::new(true);
        let setup = m.setup().unwrap();
        assert_eq!(setup[..3], [[0xB0, 101, 0], [0xB0, 100, 6], [0xB0, 6, 15]]);
        assert_eq!(
            setup[3..7],
            [[0xB1, 101, 0], [0xB1, 100, 0], [0xB1, 6, 48], [0xB1, 38, 0]]
        );
        assert!(m.setup().is_none(), "once");
        // Three notes, three channels; a note's expression on its own.
        let first = sent(&mut m, note(60, 0.8));
        // Its channel set back, then the note.
        assert_eq!(
            first,
            [[0xE1, 0, 64], [0xB1, 74, 64], [0xD1, 0, 0], [0x91, 60, 102]]
        );
        let chans: Vec<u8> = [64, 67]
            .map(|p| sent(&mut m, note(p, 0.8))[3][0] & 15)
            .to_vec();
        assert_eq!(chans, [2, 3]);
        let tuned = ControlKind::Expression {
            pitch: 64,
            kind: ExpressionKind::Tuning,
            value: 12.0,
        };
        // A quarter of the way up a ±48 bend.
        assert_eq!(m.control(tuned), Some([0xE2, 0, 80]));
        let slid = ControlKind::Expression {
            pitch: 67,
            kind: ExpressionKind::Brightness,
            value: 1.0,
        };
        assert_eq!(m.control(slid), Some([0xB3, 74, 127]));
        let pressed = ControlKind::PolyPressure {
            pitch: 60,
            value: 0.5,
        };
        assert_eq!(m.control(pressed), Some([0xD1, 64, 0]));
        // The wheel stays on the manager channel.
        let wheel = ControlKind::Cc {
            number: 1,
            value: 1.0,
        };
        assert_eq!(m.control(wheel), Some([0xB0, 1, 127]));
        // Its note-off on its channel; that channel is free again, and the next note takes the
        // emptiest after the last used.
        assert_eq!(sent(&mut m, note(64, 0.0)), [[0x82, 64, 0]]);
        assert_eq!(sent(&mut m, note(72, 0.8))[3][0] & 15, 4);
        let gone = ControlKind::Expression {
            pitch: 64,
            kind: ExpressionKind::Tuning,
            value: 1.0,
        };
        assert_eq!(m.control(gone), None, "a note no longer sounding");
    }
}
