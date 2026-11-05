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
