//! MIDI input: every MIDI keyboard connected when Pina Studio starts plays the armed track.

use std::sync::{Arc, Mutex};

use crate::{
    audio::engine::LiveNote,
    plugin::{ControlKind, ExpressionKind, MPE_BEND},
};

/// Where live playing goes: the engine's queues, shared by every input port.
pub struct LiveSink {
    pub notes: rtrb::Producer<LiveNote>,
    pub moves: rtrb::Producer<ControlKind>,
}

pub type LiveInput = Arc<Mutex<LiveSink>>;

/// What a MIDI keyboard plays: a note, or a move of a controller, the bend or pressure, or of
/// one note's own expression.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Live {
    Note(LiveNote),
    Move(ControlKind),
}

/// One input's MIDI, read: the note each channel is playing, so an MPE controller's bend, slide
/// (CC 74) and pressure on a note's own channel are that note's. Channel 1 is everyone's.
// shortcut: any channel but the first playing a note counts as an MPE member channel, so a plain
// keyboard sent on channel 2 bends only its last note; read MPE Configuration Messages if one is.
#[derive(Default)]
pub struct Decoder {
    playing: [Option<u8>; 16],
}

impl Decoder {
    pub fn decode(&mut self, msg: &[u8]) -> Option<Live> {
        let &[status, ..] = msg else {
            return None;
        };
        let ch = (status & 0x0F) as usize;
        let own = (ch > 0).then_some(self.playing[ch]).flatten();
        let seven = |v: u8| (v & 0x7F) as f32 / 127.0;
        let note = |pitch: u8, velocity: u8| {
            Live::Note(LiveNote {
                pitch: pitch & 0x7F,
                velocity: velocity & 0x7F,
            })
        };
        Some(match (status & 0xF0, msg) {
            (0x90, &[_, pitch, velocity, ..]) if velocity > 0 => {
                self.playing[ch] = Some(pitch & 0x7F);
                note(pitch, velocity)
            }
            (0x80 | 0x90, &[_, pitch, ..]) => {
                if self.playing[ch] == Some(pitch & 0x7F) {
                    self.playing[ch] = None;
                }
                note(pitch, 0)
            }
            (0xE0, &[_, lsb, msb, ..]) => {
                let bend =
                    (((msb as u16 & 0x7F) << 7 | (lsb as u16 & 0x7F)) as f32 - 8192.0) / 8192.0;
                match (ch, own) {
                    (0, _) => Live::Move(ControlKind::PitchBend(bend)),
                    (_, Some(pitch)) => Live::Move(ControlKind::Expression {
                        pitch,
                        kind: ExpressionKind::Tuning,
                        value: bend * MPE_BEND,
                    }),
                    // A member channel between notes: nothing of anyone's to bend.
                    _ => return None,
                }
            }
            (0xB0, &[_, 74, value, ..]) if own.is_some() => Live::Move(ControlKind::Expression {
                pitch: own?,
                kind: ExpressionKind::Brightness,
                value: seven(value),
            }),
            (0xB0, &[_, number, value, ..]) => Live::Move(ControlKind::Cc {
                number: number & 0x7F,
                value: seven(value),
            }),
            (0xD0, &[_, value, ..]) => match (ch, own) {
                (0, _) => Live::Move(ControlKind::Aftertouch(seven(value))),
                (_, Some(pitch)) => Live::Move(ControlKind::PolyPressure {
                    pitch,
                    value: seven(value),
                }),
                _ => return None,
            },
            (0xA0, &[_, pitch, value, ..]) => Live::Move(ControlKind::PolyPressure {
                pitch: pitch & 0x7F,
                value: seven(value),
            }),
            _ => return None,
        })
    }
}

/// Connects every MIDI input port; they stay connected while what it returns is kept.
// ponytail: ports present at startup only; watch for devices plugged in later.
pub fn connect(live: &LiveInput) -> Vec<midir::MidiInputConnection<()>> {
    let Ok(probe) = midir::MidiInput::new("Pina Studio") else {
        return Vec::new();
    };
    probe
        .ports()
        .iter()
        .filter_map(|port| {
            let input = midir::MidiInput::new("Pina Studio").ok()?;
            let live = live.clone();
            let mut decoder = Decoder::default();
            let on_message = move |_: u64, msg: &[u8], _: &mut ()| match decoder.decode(msg) {
                Some(Live::Note(n)) => drop(live.lock().unwrap().notes.push(n)),
                Some(Live::Move(m)) => drop(live.lock().unwrap().moves.push(m)),
                None => {}
            };
            input.connect(port, "Pina Studio in", on_message, ()).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests;
