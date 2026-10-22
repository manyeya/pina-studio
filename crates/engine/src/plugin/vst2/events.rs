//! MIDI events for a block.

use super::*;

/// How many steps a parameter has from 0 to 1, by its properties (0, continuous).
pub(crate) fn steps(p: &ParameterProperties) -> u32 {
    if p.flags & PARAM_IS_SWITCH != 0 {
        return 1;
    }
    if p.flags & PARAM_USES_INTEGER_MIN_MAX != 0 && p.max_integer > p.min_integer {
        let step = match p.flags & PARAM_USES_INT_STEP != 0 {
            true => p.step_integer.max(1),
            false => 1,
        };
        return ((p.max_integer - p.min_integer) / step) as u32;
    }
    0
}

/// A MIDI message for the plugin, `offset` samples into the block.
pub(crate) fn midi(offset: u32, data: [u8; 3]) -> VstMidiEvent {
    VstMidiEvent {
        kind: MIDI_TYPE,
        byte_size: size_of::<VstMidiEvent>() as i32,
        delta_frames: offset as i32,
        flags: 0,
        note_length: 0,
        note_offset: 0,
        midi_data: [data[0], data[1], data[2], 0],
        detune: 0,
        note_off_velocity: 0,
        reserved1: 0,
        reserved2: 0,
    }
}

/// 0 to 1 as MIDI's 0 to 127.
pub(crate) fn seven_bit(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 127.0).round() as u8
}

/// A note on or off, on channel 1.
pub(crate) fn note_midi(n: &Note) -> VstMidiEvent {
    match n.velocity > 0.0 {
        true => midi(
            n.offset,
            [0x90, n.pitch & 0x7f, seven_bit(n.velocity).max(1)],
        ),
        false => midi(n.offset, [0x80, n.pitch & 0x7f, 0]),
    }
}

/// A control as MIDI, if MIDI has it.
pub(crate) fn control_midi(c: &Control) -> Option<VstMidiEvent> {
    let data = match c.kind {
        ControlKind::Cc { number, value } => [0xb0, number & 0x7f, seven_bit(value)],
        ControlKind::PitchBend(v) => {
            let bend = ((v.clamp(-1.0, 1.0) + 1.0) / 2.0 * 16383.0).round() as u16;
            [0xe0, (bend & 0x7f) as u8, (bend >> 7) as u8]
        }
        ControlKind::Aftertouch(v) => [0xd0, seven_bit(v), 0],
        ControlKind::PolyPressure { pitch, value } => [0xa0, pitch & 0x7f, seven_bit(value)],
        // ponytail: VST2 has no per-note expression; MPE (a channel per note) could carry it.
        ControlKind::Expression { .. } => return None,
    };
    Some(midi(c.offset, data))
}

/// A block's MIDI for the plugin: room for `EVENTS`, made once, so filling it never allocates.
pub(crate) struct EventList {
    pub(crate) list: VstEvents,
    pub(crate) midi: [VstMidiEvent; EVENTS],
}

impl EventList {
    pub(crate) fn new() -> Box<EventList> {
        Box::new(EventList {
            list: VstEvents {
                num_events: 0,
                reserved: 0,
                events: [ptr::null_mut(); EVENTS],
            },
            midi: [midi(0, [0; 3]); EVENTS],
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.list.num_events as usize
    }

    pub(crate) fn clear(&mut self) {
        self.list.num_events = 0;
    }

    /// Adds an event, unless the list is full.
    pub(crate) fn push(&mut self, e: VstMidiEvent) {
        let n = self.len();
        if n < EVENTS {
            self.midi[n] = e;
            self.list.events[n] = &mut self.midi[n];
            self.list.num_events += 1;
        }
    }
}
