//! Events into and out of a block: notes, MIDI, expression, parameters and the transport.

use super::*;

/// One event for the plugin, of whichever kind; each starts with its header.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) union Event {
    pub(crate) header: clap_event_header,
    pub(crate) note: clap_event_note,
    pub(crate) expression: clap_event_note_expression,
    pub(crate) param: clap_event_param_value,
    pub(crate) midi: clap_event_midi,
}

pub(crate) fn header<T>(time: u32, kind: clap_event_type) -> clap_event_header {
    clap_event_header {
        size: size_of::<T>() as u32,
        time,
        space_id: CLAP_CORE_EVENT_SPACE_ID,
        type_: kind,
        flags: 0,
    }
}

/// A parameter set to `value`, the plugin's own, for every note.
pub(crate) fn param_event(time: u32, id: u32, value: f64) -> Event {
    Event {
        param: clap_event_param_value {
            header: header::<clap_event_param_value>(time, CLAP_EVENT_PARAM_VALUE),
            param_id: id,
            cookie: ptr::null_mut(),
            note_id: -1,
            port_index: -1,
            channel: -1,
            key: -1,
            value,
        },
    }
}

pub(crate) fn midi_event(time: u32, data: [u8; 3]) -> Event {
    Event {
        midi: clap_event_midi {
            header: header::<clap_event_midi>(time, CLAP_EVENT_MIDI),
            port_index: 0,
            data,
        },
    }
}

/// A note on or off, its id its pitch; as MIDI for a plugin that only takes that.
pub(crate) fn note_event(n: &Note, time: u32, midi: bool) -> Event {
    let on = n.velocity > 0.0;
    if midi {
        return match on {
            true => midi_event(time, [0x90, n.pitch & 0x7f, seven(n.velocity).max(1)]),
            false => midi_event(time, [0x80, n.pitch & 0x7f, seven(n.release)]),
        };
    }
    let kind = if on {
        CLAP_EVENT_NOTE_ON
    } else {
        CLAP_EVENT_NOTE_OFF
    };
    Event {
        note: clap_event_note {
            header: header::<clap_event_note>(time, kind),
            note_id: n.pitch as i32,
            port_index: 0,
            channel: 0,
            key: n.pitch as i16,
            velocity: match on {
                true => n.velocity,
                false => n.release,
            }
            .clamp(0.0, 1.0) as f64,
        },
    }
}

pub(crate) fn expression_event(
    time: u32,
    pitch: u8,
    id: clap_note_expression,
    value: f64,
) -> Event {
    Event {
        expression: clap_event_note_expression {
            header: header::<clap_event_note_expression>(time, CLAP_EVENT_NOTE_EXPRESSION),
            expression_id: id,
            note_id: pitch as i32,
            port_index: 0,
            channel: 0,
            key: pitch as i16,
            value,
        },
    }
}

/// A control as the plugin hears it: MIDI for controllers, bend and channel pressure, note
/// expression for one held note's (or poly pressure as MIDI, and no expression, for a plugin
/// that only takes MIDI).
pub(crate) fn control_event(c: &Control, time: u32, midi: bool) -> Option<Event> {
    Some(match c.kind {
        ControlKind::Cc { number, value } => midi_event(time, [0xb0, number & 0x7f, seven(value)]),
        ControlKind::PitchBend(v) => {
            let b = bend(v);
            midi_event(time, [0xe0, (b & 0x7f) as u8, (b >> 7) as u8])
        }
        ControlKind::Aftertouch(v) => midi_event(time, [0xd0, seven(v), 0]),
        ControlKind::PolyPressure { pitch, value } if midi => {
            midi_event(time, [0xa0, pitch & 0x7f, seven(value)])
        }
        ControlKind::PolyPressure { pitch, value } => expression_event(
            time,
            pitch,
            CLAP_NOTE_EXPRESSION_PRESSURE,
            value.clamp(0.0, 1.0) as f64,
        ),
        ControlKind::Expression { .. } if midi => return None,
        ControlKind::Expression { pitch, kind, value } => {
            let (id, value) = expression(kind, value as f64);
            expression_event(time, pitch, id, value)
        }
    })
}

/// 0 to 1 as a MIDI value, 0 to 127.
pub(crate) fn seven(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 127.0).round() as u8
}

/// A bend, -1 to 1, as MIDI's 14 bits (8192 is none).
pub(crate) fn bend(v: f32) -> u16 {
    ((v.clamp(-1.0, 1.0) + 1.0) / 2.0 * 16383.0).round() as u16
}

/// A note expression's CLAP id, and its value as CLAP has it.
pub(crate) fn expression(kind: ExpressionKind, value: f64) -> (clap_note_expression, f64) {
    match kind {
        ExpressionKind::Tuning => (CLAP_NOTE_EXPRESSION_TUNING, value.clamp(-120.0, 120.0)),
        // 1 is as played; 4 is 12 dB up.
        ExpressionKind::Volume => (CLAP_NOTE_EXPRESSION_VOLUME, value.clamp(0.0, 4.0)),
        ExpressionKind::Pan => (
            CLAP_NOTE_EXPRESSION_PAN,
            ((value + 1.0) / 2.0).clamp(0.0, 1.0),
        ),
        ExpressionKind::Brightness => (CLAP_NOTE_EXPRESSION_BRIGHTNESS, value.clamp(0.0, 1.0)),
        ExpressionKind::Vibrato => (CLAP_NOTE_EXPRESSION_VIBRATO, value.clamp(0.0, 1.0)),
        ExpressionKind::Expression => (CLAP_NOTE_EXPRESSION_EXPRESSION, value.clamp(0.0, 1.0)),
    }
}

/// Where the song is, as CLAP has it: in beats and seconds, at `tempo`, in 4/4.
pub(crate) fn transport(
    playing: bool,
    position: u64,
    beats: f64,
    tempo: f64,
    rate: f64,
) -> clap_event_transport {
    let seconds = position as f64 / rate;
    let bar = (beats / 4.0).floor();
    let fixed = |x: f64, factor: i64| (x * factor as f64).round() as i64;
    let playing = if playing {
        CLAP_TRANSPORT_IS_PLAYING
    } else {
        0
    };
    clap_event_transport {
        header: header::<clap_event_transport>(0, CLAP_EVENT_TRANSPORT),
        flags: CLAP_TRANSPORT_HAS_TEMPO
            | CLAP_TRANSPORT_HAS_BEATS_TIMELINE
            | CLAP_TRANSPORT_HAS_SECONDS_TIMELINE
            | CLAP_TRANSPORT_HAS_TIME_SIGNATURE
            | playing,
        song_pos_beats: fixed(beats, CLAP_BEATTIME_FACTOR),
        song_pos_seconds: fixed(seconds, CLAP_SECTIME_FACTOR),
        tempo,
        tempo_inc: 0.0,
        loop_start_beats: 0,
        loop_end_beats: 0,
        loop_start_seconds: 0,
        loop_end_seconds: 0,
        bar_start: fixed(bar * 4.0, CLAP_BEATTIME_FACTOR),
        bar_number: bar as i32,
        tsig_num: 4,
        tsig_denom: 4,
    }
}

/// `events` as the plugin reads them.
pub(crate) fn input_events(events: &Vec<Event>) -> clap_input_events {
    clap_input_events {
        ctx: ptr::from_ref(events).cast_mut().cast(),
        size: Some(events_size),
        get: Some(events_get),
    }
}

pub(crate) unsafe extern "C" fn events_size(list: *const clap_input_events) -> u32 {
    let events = unsafe { &*(*list).ctx.cast::<Vec<Event>>() };
    events.len() as u32
}

pub(crate) unsafe extern "C" fn events_get(
    list: *const clap_input_events,
    i: u32,
) -> *const clap_event_header {
    let events = unsafe { &*(*list).ctx.cast::<Vec<Event>>() };
    events
        .get(i as usize)
        .map_or(ptr::null(), |e| ptr::from_ref(e).cast())
}

/// Where the plugin reports its own changes (its editor's edits, MIDI it makes), as a host
/// must give it: the notes it plays go into `into` (a `Vec<Note>` with room made for them, or
/// none), the rest are let go.
pub(crate) fn output_events(into: *mut Vec<crate::plugin::Note>) -> clap_output_events {
    clap_output_events {
        ctx: into.cast(),
        try_push: Some(take_event),
    }
}

/// A note it played (CLAP's own, or MIDI), kept; anything else let go.
pub(crate) unsafe extern "C" fn take_event(
    list: *const clap_output_events,
    e: *const clap_event_header,
) -> bool {
    let (Some(list), Some(h)) = (unsafe { list.as_ref() }, unsafe { e.as_ref() }) else {
        return true;
    };
    let Some(into) = (unsafe { list.ctx.cast::<Vec<crate::plugin::Note>>().as_mut() }) else {
        return true;
    };
    if h.space_id != CLAP_CORE_EVENT_SPACE_ID || into.len() == into.capacity() {
        return true;
    }
    let note = |pitch: i32, velocity: f32| crate::plugin::Note {
        offset: h.time,
        pitch: pitch.clamp(0, 127) as u8,
        velocity,
        release: 0.0,
    };
    match h.type_ {
        CLAP_EVENT_NOTE_ON | CLAP_EVENT_NOTE_OFF => {
            let n = unsafe { &*e.cast::<clap_event_note>() };
            let on = h.type_ == CLAP_EVENT_NOTE_ON && n.velocity > 0.0;
            into.push(note(n.key as i32, if on { n.velocity as f32 } else { 0.0 }));
        }
        CLAP_EVENT_MIDI => {
            let m = unsafe { &*e.cast::<clap_event_midi>() };
            let [status, data1, data2] = m.data;
            match status & 0xf0 {
                0x90 if data2 > 0 => into.push(note(data1 as i32, data2 as f32 / 127.0)),
                0x80 | 0x90 => into.push(note(data1 as i32, 0.0)),
                _ => {}
            }
        }
        _ => {}
    }
    true
}
