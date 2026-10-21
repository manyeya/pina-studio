//! MIDI input: every MIDI keyboard connected when Pina Studio starts plays the armed track.

use std::sync::{Arc, Mutex};

use crate::audio::engine::LiveNote;

/// Where live notes go: the engine's queue, shared by every input port.
pub type LiveInput = Arc<Mutex<rtrb::Producer<LiveNote>>>;

/// A note on or off from a MIDI message, on any channel.
pub fn parse(msg: &[u8]) -> Option<LiveNote> {
    match *msg {
        [status, pitch, velocity, ..] if status & 0xF0 == 0x90 => Some(LiveNote {
            pitch: pitch & 0x7F,
            velocity: velocity & 0x7F,
        }),
        [status, pitch, ..] if status & 0xF0 == 0x80 => Some(LiveNote {
            pitch: pitch & 0x7F,
            velocity: 0,
        }),
        _ => None,
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
            let on_message = move |_: u64, msg: &[u8], _: &mut ()| {
                if let Some(n) = parse(msg) {
                    let _ = live.lock().unwrap().push(n);
                }
            };
            input.connect(port, "Pina Studio in", on_message, ()).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_notes_on_any_channel() {
        let on = |pitch, velocity| Some(LiveNote { pitch, velocity });
        assert_eq!(parse(&[0x90, 60, 100]), on(60, 100));
        assert_eq!(parse(&[0x95, 61, 0]), on(61, 0)); // note on at velocity 0 is an off
        assert_eq!(parse(&[0x83, 62, 64]), on(62, 0));
        assert_eq!(parse(&[0xB0, 7, 100]), None); // a controller
        assert_eq!(parse(&[0x90]), None);
    }
}
