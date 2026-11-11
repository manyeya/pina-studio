//! MIDI input: every MIDI keyboard connected when Pina Studio starts plays the armed track.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

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

/// How an input is read: as MPE when it plays like it (`mpe`), and how far its notes' own bends
/// reach, in semitones, until it says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputSetup {
    pub mpe: bool,
    pub bend: u8,
}

impl Default for InputSetup {
    fn default() -> Self {
        InputSetup {
            mpe: true,
            bend: MPE_BEND as u8,
        }
    }
}

/// One input's MIDI, read. With MPE: the note each channel is playing, so an MPE controller's
/// bend, slide (CC 74) and pressure on a note's own channel are that note's, the zone's manager
/// channel (1, or 16 for an upper zone) being everyone's. It's MPE once a note comes on a channel
/// but the manager's or the controller says so (an MPE Configuration Message), and a member
/// channel's bend range is what the controller sets (RPN 0) or the setup's. Without, every
/// channel's moves are everyone's.
#[derive(Clone, Copy, Debug)]
pub struct Decoder {
    pub setup: InputSetup,
    playing: [Option<u8>; 16],
    /// The zone's manager channel, 0 or 15.
    manager: usize,
    /// The bend range the controller set, in semitones.
    told_bend: Option<f32>,
    /// Each channel's registered parameter being set (CC 101 and 100).
    rpn: [(u8, u8); 16],
    /// It's been heard playing as MPE.
    pub found: bool,
}

impl Default for Decoder {
    fn default() -> Self {
        Decoder::new(InputSetup::default())
    }
}

impl Decoder {
    pub fn new(setup: InputSetup) -> Self {
        Decoder {
            setup,
            playing: [None; 16],
            manager: 0,
            told_bend: None,
            rpn: [(127, 127); 16],
            found: false,
        }
    }

    pub fn decode(&mut self, msg: &[u8]) -> Option<Live> {
        let &[status, ..] = msg else {
            return None;
        };
        let ch = (status & 0x0F) as usize;
        let member = self.setup.mpe && ch != self.manager;
        let own = member.then_some(self.playing[ch]).flatten();
        let reach = self.told_bend.unwrap_or(self.setup.bend as f32);
        let seven = |v: u8| (v & 0x7F) as f32 / 127.0;
        let note = |pitch: u8, velocity: u8, release: u8| {
            Live::Note(LiveNote {
                pitch: pitch & 0x7F,
                velocity: velocity & 0x7F,
                release: release & 0x7F,
            })
        };
        Some(match (status & 0xF0, msg) {
            (0x90, &[_, pitch, velocity, ..]) if velocity > 0 => {
                if member {
                    self.playing[ch] = Some(pitch & 0x7F);
                    self.found = true;
                }
                note(pitch, velocity, 0)
            }
            (0x80 | 0x90, &[_, pitch, ..]) => {
                if self.playing[ch] == Some(pitch & 0x7F) {
                    self.playing[ch] = None;
                }
                // A note off's velocity: how fast the key came up.
                let release = match status & 0xF0 {
                    0x80 => msg.get(2).copied().unwrap_or(0),
                    _ => 0,
                };
                note(pitch, 0, release)
            }
            (0xE0, &[_, lsb, msb, ..]) => {
                let bend =
                    (((msb as u16 & 0x7F) << 7 | (lsb as u16 & 0x7F)) as f32 - 8192.0) / 8192.0;
                match (member, own) {
                    (false, _) => Live::Move(ControlKind::PitchBend(bend)),
                    (true, Some(pitch)) => Live::Move(ControlKind::Expression {
                        pitch,
                        kind: ExpressionKind::Tuning,
                        value: bend * reach,
                    }),
                    // A member channel between notes: nothing of anyone's to bend.
                    _ => return None,
                }
            }
            // Choosing a registered parameter, and setting it: the zone (RPN 6, on a manager
            // channel) and a member channel's bend range (RPN 0).
            (0xB0, &[_, 101, v, ..]) => {
                self.rpn[ch].0 = v & 0x7F;
                return None;
            }
            (0xB0, &[_, 100, v, ..]) => {
                self.rpn[ch].1 = v & 0x7F;
                return None;
            }
            (0xB0, &[_, 6 | 38, value, ..]) if self.rpn[ch] != (127, 127) => {
                match (self.rpn[ch], msg[1]) {
                    ((0, 6), 6) if ch == 0 || ch == 15 => {
                        if value > 0 {
                            self.manager = ch;
                            self.found = true;
                        }
                    }
                    ((0, 0), 6) if ch != self.manager && value > 0 => {
                        self.told_bend = Some(value as f32);
                    }
                    _ => {}
                }
                return None;
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
            (0xD0, &[_, value, ..]) => match (member, own) {
                (false, _) => Live::Move(ControlKind::Aftertouch(seven(value))),
                (true, Some(pitch)) => Live::Move(ControlKind::PolyPressure {
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

/// A MIDI input connected: its name, how it's read, and whether it's been heard playing as MPE.
#[derive(Clone, Debug, PartialEq)]
pub struct InputState {
    pub name: String,
    pub setup: InputSetup,
    pub found: bool,
}

/// The inputs connected, shared with each one's reader, so a change to how one's read applies
/// as it's made.
pub type Inputs = Arc<Mutex<Vec<InputState>>>;

/// Connects every MIDI input port, each read as `inputs` says (listing each there); they stay
/// connected while what it returns is kept.
// ponytail: ports present at startup only; watch for devices plugged in later.
pub fn connect(live: &LiveInput, inputs: &Inputs) -> Vec<midir::MidiInputConnection<()>> {
    let Ok(probe) = midir::MidiInput::new("Pina Studio") else {
        return Vec::new();
    };
    probe
        .ports()
        .iter()
        .filter_map(|port| {
            let input = midir::MidiInput::new("Pina Studio").ok()?;
            let name = probe
                .port_name(port)
                .unwrap_or_else(|_| "MIDI input".into());
            let k = {
                let mut all = inputs.lock().ok()?;
                let setup = all
                    .iter()
                    .find(|i| i.name == name)
                    .map_or(InputSetup::default(), |i| i.setup);
                all.push(InputState {
                    name,
                    setup,
                    found: false,
                });
                all.len() - 1
            };
            let (live, inputs) = (live.clone(), inputs.clone());
            let mut decoder = Decoder::default();
            let on_message = move |_: u64, msg: &[u8], _: &mut ()| {
                let was = decoder.found;
                if let Some(state) = inputs.lock().ok().as_deref().and_then(|a| a.get(k)) {
                    decoder.setup = state.setup;
                }
                let got = decoder.decode(msg);
                if decoder.found
                    && !was
                    && let Ok(mut all) = inputs.lock()
                    && let Some(state) = all.get_mut(k)
                {
                    state.found = true;
                }
                match got {
                    Some(Live::Note(n)) => drop(live.lock().unwrap().notes.push(n)),
                    Some(Live::Move(m)) => drop(live.lock().unwrap().moves.push(m)),
                    None => {}
                }
            };
            input.connect(port, "Pina Studio in", on_message, ()).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests;
