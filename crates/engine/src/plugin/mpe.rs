//! Notes and controls as MIDI 1.0 for plugins that take only that (Audio Units and VST2): on
//! channel 1, or with MPE on, each note on a channel of its own (2 to 16) with its bend, slide
//! and pressure on that channel, so the plugin hears each note's expression apart.

use super::{ControlKind, ExpressionKind, Note};
use crate::song::project::MpeZone;

/// How far a member channel's bend goes, either way, in semitones: MPE's default.
pub const MPE_BEND: f32 = 48.0;

/// The most a zone's setup says: an MPE Configuration Message on the manager channel, and each
/// member channel's bend range.
const SETUP: usize = 3 + 15 * 4;

/// MIDI as a plugin hears it: which channel each note sounding is on.
#[derive(Clone, Copy)]
pub(crate) struct Mpe {
    zone: Option<MpeZone>,
    /// Each pitch's channel (0 to 15) and 1, while it sounds with MPE on; 0 while it doesn't.
    channel: [u8; 128],
    /// How many notes each channel holds.
    holding: [u8; 16],
    /// The member channel to try first.
    next: u8,
    told: bool,
}

fn seven(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 127.0).round() as u8
}

impl Mpe {
    pub(crate) fn new(zone: Option<MpeZone>) -> Self {
        Mpe {
            zone,
            channel: [0; 128],
            holding: [0; 16],
            next: 0,
            told: false,
        }
    }

    pub(crate) fn set(&mut self, zone: Option<MpeZone>) {
        if zone != self.zone {
            *self = Mpe::new(zone);
        }
    }

    /// The manager channel: 1 (0), or 16 for an upper zone.
    fn manager(&self) -> u8 {
        self.zone.map_or(0, |z| z.channels().0)
    }

    /// The zone's setup, the first time it's asked for with MPE on: what to send, and how much
    /// of it there is.
    pub(crate) fn setup(&mut self) -> Option<([[u8; 3]; SETUP], usize)> {
        let zone = self.zone.filter(|_| !self.told)?;
        self.told = true;
        let (manager, members) = zone.channels();
        let mut out = [[0u8; 3]; SETUP];
        // RPN 6 on the manager channel: how many member channels.
        let cc = 0xB0 | manager;
        let count = members.clone().count() as u8;
        out[..3].copy_from_slice(&[[cc, 101, 0], [cc, 100, 6], [cc, 6, count]]);
        let mut n = 3;
        for ch in members {
            let cc = 0xB0 | ch;
            // RPN 0 on each member channel: its bend range.
            out[n..n + 4].copy_from_slice(&[
                [cc, 101, 0],
                [cc, 100, 0],
                [cc, 6, zone.bend.clamp(1, 96)],
                [cc, 38, 0],
            ]);
            n += 4;
        }
        Some((out, n))
    }

    /// A note on (velocity 1 to 127) or off (with its release velocity), as `send` is given it:
    /// on the manager channel, or with MPE on, on the member channel holding fewest notes from
    /// the one after the last used (where its note-on went, for an off), that channel's bend,
    /// slide and pressure set back first.
    pub(crate) fn note(&mut self, n: &Note, mut send: impl FnMut([u8; 3])) {
        let pitch = n.pitch & 127;
        let on = n.velocity > 0.0;
        let ch = match (self.zone, on) {
            (None, _) => 0,
            (Some(zone), true) => {
                let members = zone.channels().1;
                let (first, count) = (*members.start(), members.count() as u8);
                let ch = (0..count)
                    .map(|k| first + (self.next + k) % count)
                    .min_by_key(|&c| self.holding[c as usize])
                    .unwrap_or(first);
                self.next = (ch - first + 1) % count;
                self.holding[ch as usize] = self.holding[ch as usize].saturating_add(1);
                self.channel[pitch as usize] = ch + 1;
                // A fresh note starts unbent, slid to the middle and unpressed.
                send([0xE0 | ch, 0, 64]);
                send([0xB0 | ch, 74, 64]);
                send([0xD0 | ch, 0, 0]);
                ch
            }
            (Some(_), false) => match std::mem::take(&mut self.channel[pitch as usize]) {
                0 => self.manager(),
                c => {
                    self.holding[c as usize - 1] = self.holding[c as usize - 1].saturating_sub(1);
                    c - 1
                }
            },
        };
        send(match on {
            true => [0x90 | ch, pitch, seven(n.velocity).max(1)],
            false => [0x80 | ch, pitch, seven(n.release)],
        });
    }

    /// A control as MIDI, if MIDI has it: a channel's on the manager channel; a note's own on
    /// its channel with MPE on (its bend, slide as CC 74, pressure), else its pressure as
    /// polyphonic aftertouch.
    pub(crate) fn control(&self, kind: ControlKind) -> Option<[u8; 3]> {
        let bend = |ch: u8, v: f32| {
            let b = (8192.0 + v.clamp(-1.0, 1.0) * 8192.0)
                .round()
                .clamp(0.0, 16383.0) as u16;
            [0xE0 | ch, (b & 0x7F) as u8, (b >> 7) as u8]
        };
        // A note's channel, while it sounds.
        let of = |pitch: u8| self.channel[pitch as usize & 127].checked_sub(1);
        let m = self.manager();
        Some(match kind {
            ControlKind::Cc { number, value } => [0xB0 | m, number & 127, seven(value)],
            ControlKind::PitchBend(v) => bend(m, v),
            ControlKind::Aftertouch(v) => [0xD0 | m, seven(v), 0],
            ControlKind::PolyPressure { pitch, value } => match self.zone {
                Some(_) => [0xD0 | of(pitch)?, seven(value), 0],
                None => [0xA0, pitch & 127, seven(value)],
            },
            ControlKind::Expression { pitch, kind, value } if self.zone.is_some() => {
                let ch = of(pitch)?;
                let reach = self.zone.map_or(MPE_BEND, |z| z.bend.clamp(1, 96) as f32);
                match kind {
                    ExpressionKind::Tuning => bend(ch, value / reach),
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
            release: 0.0,
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
        let mut m = Mpe::new(None);
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
        let mut m = Mpe::new(Some(MpeZone::default()));
        let (setup, n) = m.setup().unwrap();
        assert_eq!(n, SETUP);
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

    #[test]
    fn an_upper_zone_of_four_channels_bending_24() {
        let mut m = Mpe::new(Some(MpeZone {
            upper: true,
            channels: 4,
            bend: 24,
        }));
        let (setup, n) = m.setup().unwrap();
        assert_eq!(n, 3 + 4 * 4);
        // Channel 16 manages, 12 to 15 play.
        assert_eq!(setup[..3], [[0xBF, 101, 0], [0xBF, 100, 6], [0xBF, 6, 4]]);
        assert_eq!(setup[5], [0xBB, 6, 24]);
        let chans: Vec<u8> = (0..5)
            .map(|k| sent(&mut m, note(60 + k, 0.8))[3][0] & 15)
            .collect();
        assert_eq!(chans, [11, 12, 13, 14, 11]);
        // Half way up a ±24 bend; the wheel on channel 16.
        let tuned = ControlKind::Expression {
            pitch: 61,
            kind: ExpressionKind::Tuning,
            value: 12.0,
        };
        assert_eq!(m.control(tuned), Some([0xEC, 0, 96]));
        let wheel = ControlKind::Cc {
            number: 1,
            value: 1.0,
        };
        assert_eq!(m.control(wheel), Some([0xBF, 1, 127]));
        // A note off says how fast it was let go.
        let off = Note {
            release: 1.0,
            ..note(61, 0.0)
        };
        assert_eq!(sent(&mut m, off), [[0x8C, 61, 127]]);
    }
}
