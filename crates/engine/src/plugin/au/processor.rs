//! The unit's audio-thread side: its input, the transport it's told, and events.

use super::*;

/// An Audio Unit's audio-thread side.
pub struct AuProcessor {
    pub(crate) unit: Arc<Unit>,
    /// What the input render callback hands the unit; boxed, as the unit keeps its address.
    pub(crate) input: Box<Input>,
    /// The buffers the unit renders into, and the list that points at them.
    pub(crate) out: Vec<Vec<f32>>,
    pub(crate) list: Box<BufferList>,
    /// Channels on its main input and sidechain, mono ones hearing both sides mixed.
    pub(crate) in_channels: [u32; 2],
    pub(crate) time: AudioTimeStamp,
    /// Whether it hears MIDI: an instrument, or an effect that takes it.
    pub(crate) midi: bool,
    pub(crate) latency: u32,
    pub(crate) sidechain: bool,
    /// Its parameters' ranges, by id, for automation.
    pub(crate) ranges: Vec<(u32, Range)>,
    /// Automation for the next block: a parameter, and its value at the block's start and end.
    pub(crate) automation: [(u32, f64, f64); AUTOMATED],
    pub(crate) automated: usize,
    pub(crate) max_block: usize,
}

// The unit's handles are only used from the audio thread once the processor moves there.
unsafe impl Send for AuProcessor {}

/// The block's input as the unit pulls it, through `render_input`: the main input, the
/// sidechain, and silence for any other input.
pub(crate) struct Input {
    pub(crate) buses: [[Vec<f32>; 2]; 3],
    pub(crate) frames: usize,
}

/// An `AudioBufferList` with room for `MAX_CHANNELS` buffers.
#[repr(C)]
pub(crate) struct BufferList {
    pub(crate) count: u32,
    pub(crate) buffers: [AudioBuffer; MAX_CHANNELS],
}

/// Hands the unit the block's input when it pulls an input bus: the main input on the first,
/// the sidechain on the second, silence on others.
pub(crate) unsafe extern "C-unwind" fn render_input(
    refcon: NonNull<c_void>,
    _flags: NonNull<AudioUnitRenderActionFlags>,
    _time: NonNull<AudioTimeStamp>,
    element: u32,
    frames: u32,
    data: *mut AudioBufferList,
) -> i32 {
    let input = unsafe { &mut *refcon.as_ptr().cast::<Input>() };
    let Some(list) = (unsafe { data.as_mut() }) else {
        return 0;
    };
    let frames = frames as usize;
    let buffers = unsafe {
        std::slice::from_raw_parts_mut(list.mBuffers.as_mut_ptr(), list.mNumberBuffers as usize)
    };
    let have = input.frames;
    let bus = (element as usize).min(2);
    for (c, b) in buffers.iter_mut().enumerate() {
        let src = match c {
            0 | 1 => &mut input.buses[bus][c],
            _ => &mut input.buses[2][0],
        };
        if b.mData.is_null() {
            // It wants Pina Studio's buffers.
            if frames > src.len() {
                return kAudioUnitErr_TooManyFramesToProcess;
            }
            b.mData = src.as_mut_ptr().cast();
            b.mDataByteSize = (frames * size_of::<f32>()) as u32;
        } else {
            let room = b.mDataByteSize as usize / size_of::<f32>();
            let dst =
                unsafe { std::slice::from_raw_parts_mut(b.mData.cast::<f32>(), frames.min(room)) };
            let n = dst.len().min(have);
            dst[..n].copy_from_slice(&src[..n]);
            dst[n..].fill(0.0);
        }
    }
    0
}

pub(crate) unsafe extern "C-unwind" fn beat_and_tempo(
    user: *mut c_void,
    beat: *mut f64,
    tempo: *mut f64,
) -> i32 {
    let t = unsafe { &*user.cast::<Transport>() };
    unsafe {
        if let Some(beat) = beat.as_mut() {
            *beat = t.beat();
        }
        if let Some(tempo) = tempo.as_mut() {
            *tempo = t.tempo();
        }
    }
    0
}

// ponytail: 4/4, as for VST3; pass the time signature when the engine gives it to plugins.
pub(crate) unsafe extern "C-unwind" fn musical_time(
    user: *mut c_void,
    to_next_beat: *mut u32,
    numerator: *mut f32,
    denominator: *mut u32,
    downbeat: *mut f64,
) -> i32 {
    let t = unsafe { &*user.cast::<Transport>() };
    let beat = t.beat();
    let samples_per_beat = t.rate * 60.0 / t.tempo().max(1.0);
    unsafe {
        if let Some(n) = to_next_beat.as_mut() {
            *n = ((beat.ceil() - beat) * samples_per_beat).round() as u32;
        }
        if let Some(n) = numerator.as_mut() {
            *n = 4.0;
        }
        if let Some(d) = denominator.as_mut() {
            *d = 4;
        }
        if let Some(d) = downbeat.as_mut() {
            *d = (beat / 4.0).floor() * 4.0;
        }
    }
    0
}

#[allow(clippy::too_many_arguments)]
pub(crate) unsafe extern "C-unwind" fn transport_state2(
    user: *mut c_void,
    playing: *mut u8,
    recording: *mut u8,
    changed: *mut u8,
    sample: *mut f64,
    cycling: *mut u8,
    cycle_start: *mut f64,
    cycle_end: *mut f64,
) -> i32 {
    let t = unsafe { &*user.cast::<Transport>() };
    unsafe {
        if let Some(p) = playing.as_mut() {
            *p = t.playing.load(Relaxed) as u8;
        }
        if let Some(r) = recording.as_mut() {
            *r = 0;
        }
        if let Some(c) = changed.as_mut() {
            *c = t.changed.swap(false, Relaxed) as u8;
        }
        if let Some(s) = sample.as_mut() {
            *s = t.position.load(Relaxed) as f64;
        }
        // ponytail: the cycle isn't passed to plugins yet.
        if let Some(c) = cycling.as_mut() {
            *c = 0;
        }
        for b in [cycle_start, cycle_end] {
            if let Some(b) = b.as_mut() {
                *b = 0.0;
            }
        }
    }
    0
}

pub(crate) unsafe extern "C-unwind" fn transport_state(
    user: *mut c_void,
    playing: *mut u8,
    changed: *mut u8,
    sample: *mut f64,
    cycling: *mut u8,
    cycle_start: *mut f64,
    cycle_end: *mut f64,
) -> i32 {
    unsafe {
        transport_state2(
            user,
            playing,
            ptr::null_mut(),
            changed,
            sample,
            cycling,
            cycle_start,
            cycle_end,
        )
    }
}

/// An `AudioUnitParameterEvent` holding a ramp (the larger of its union's two cases).
#[repr(C)]
pub(crate) struct ParameterEvent {
    pub(crate) scope: u32,
    pub(crate) element: u32,
    pub(crate) parameter: u32,
    pub(crate) kind: u32,
    pub(crate) start_offset: i32,
    pub(crate) duration: u32,
    pub(crate) start: f32,
    pub(crate) end: f32,
}

impl ProcessorApi for AuProcessor {
    fn latency(&self) -> u32 {
        self.latency
    }

    fn set_transport(&mut self, playing: bool, position: u64, tempo: f64) {
        let t = &self.unit.transport;
        if t.playing.swap(playing, Relaxed) != playing {
            t.changed.store(true, Relaxed);
        }
        t.position.store(position, Relaxed);
        t.tempo.store(tempo.to_bits(), Relaxed);
    }

    fn automate(&mut self, id: u32, start: f64, end: f64) {
        if self.automated < AUTOMATED {
            self.automation[self.automated] = (id, start.clamp(0.0, 1.0), end.clamp(0.0, 1.0));
            self.automated += 1;
        }
    }

    fn sidechain(&self) -> bool {
        self.sidechain
    }

    // Parameters set on the main thread go straight into the unit.
    fn pending(&self) -> bool {
        false
    }

    fn process(
        &mut self,
        buf: [&mut [f32]; 2],
        frames: usize,
        notes: &[Note],
        controls: &[Control],
        sidechain: Option<[&[f32]; 2]>,
    ) {
        assert!(frames <= self.max_block);
        if frames == 0 {
            return;
        }
        let au = self.unit.au;
        // The input it'll pull: a mono bus hears both sides.
        let input = &mut *self.input;
        input.frames = frames;
        let [main, side, silence] = &mut input.buses;
        let fill = |dst: &mut [Vec<f32>; 2], src: Option<[&[f32]; 2]>, channels: u32| match src {
            Some([l, r]) if channels == 1 => {
                for (d, (l, r)) in dst[0][..frames].iter_mut().zip(l.iter().zip(r)) {
                    *d = (l + r) / 2.0;
                }
            }
            Some(src) => {
                for (d, s) in dst.iter_mut().zip(src) {
                    d[..frames].copy_from_slice(&s[..frames]);
                }
            }
            None => dst.iter_mut().for_each(|d| d[..frames].fill(0.0)),
        };
        fill(main, Some([&buf[0][..], &buf[1][..]]), self.in_channels[0]);
        fill(side, sidechain, self.in_channels[1]);
        silence.iter_mut().for_each(|d| d[..frames].fill(0.0));

        // Notes and controls, merged in time.
        if self.midi {
            let (mut n, mut c) = (notes.iter().peekable(), controls.iter().peekable());
            loop {
                let note_first = match (n.peek(), c.peek()) {
                    (Some(a), Some(b)) => a.offset <= b.offset,
                    (Some(_), None) => true,
                    (None, Some(_)) => false,
                    (None, None) => break,
                };
                let (offset, [status, data1, data2]) = match note_first {
                    true => {
                        let note = n.next().unwrap();
                        (note.offset, note_message(note))
                    }
                    false => {
                        let control = c.next().unwrap();
                        let Some(m) = control_message(control.kind) else {
                            continue;
                        };
                        (control.offset, m)
                    }
                };
                let offset = offset.min(frames as u32 - 1);
                unsafe { MusicDeviceMIDIEvent(au, status, data1, data2, offset) };
            }
        }

        // Automation: the start value now, and a ramp to the end if the unit follows them.
        for &(id, start, end) in &self.automation[..std::mem::take(&mut self.automated)] {
            let Ok(i) = self.ranges.binary_search_by_key(&id, |r| r.0) else {
                continue;
            };
            let range = self.ranges[i].1;
            let (from, to) = (range.value(start), range.value(end));
            unsafe { AudioUnitSetParameter(au, id, GLOBAL, 0, from, 0) };
            if range.ramps && from != to {
                let ramp = ParameterEvent {
                    scope: GLOBAL,
                    element: 0,
                    parameter: id,
                    kind: RAMPED,
                    start_offset: 0,
                    duration: frames as u32,
                    start: from,
                    end: to,
                };
                unsafe { AudioUnitScheduleParameters(au, &ramp, 1) };
            }
        }

        // Render into Pina Studio's buffers; the unit may point the list at its own instead.
        let list = &mut *self.list;
        for (b, out) in list.buffers.iter_mut().zip(&mut self.out) {
            *b = AudioBuffer {
                mNumberChannels: 1,
                mDataByteSize: (frames * size_of::<f32>()) as u32,
                mData: out.as_mut_ptr().cast(),
            };
        }
        list.count = self.out.len() as u32;
        let mut flags = AudioUnitRenderActionFlags::empty();
        let status = unsafe {
            AudioUnitRender(
                au,
                &mut flags,
                NonNull::from(&mut self.time),
                0,
                frames as u32,
                NonNull::from(&mut *self.list).cast(),
            )
        };
        self.time.mSampleTime += frames as f64;
        // Mono output goes to both sides; a failed render is silence.
        let list = &*self.list;
        for (c, dst) in buf.into_iter().enumerate() {
            let b = &list.buffers[c.min(list.count as usize - 1)];
            if status != 0 || b.mData.is_null() {
                dst[..frames].fill(0.0);
                continue;
            }
            let src = unsafe { std::slice::from_raw_parts(b.mData.cast::<f32>(), frames) };
            dst[..frames].copy_from_slice(src);
        }
    }
}

/// A note on (velocity 1 to 127) or off, on channel 1.
pub(crate) fn note_message(n: &Note) -> [u32; 3] {
    if n.velocity > 0.0 {
        let velocity = (n.velocity * 127.0).round().clamp(1.0, 127.0) as u32;
        [0x90, n.pitch as u32, velocity]
    } else {
        [0x80, n.pitch as u32, 0]
    }
}

/// A control as a MIDI message on channel 1, if MIDI has one for it.
pub(crate) fn control_message(kind: ControlKind) -> Option<[u32; 3]> {
    let seven = |v: f32| (v * 127.0).round().clamp(0.0, 127.0) as u32;
    Some(match kind {
        ControlKind::Cc { number, value } => [0xB0, number.min(127) as u32, seven(value)],
        ControlKind::PitchBend(v) => {
            let bend = (8192.0 + v as f64 * 8192.0).round().clamp(0.0, 16383.0) as u32;
            [0xE0, bend & 0x7F, bend >> 7]
        }
        ControlKind::Aftertouch(v) => [0xD0, seven(v), 0],
        ControlKind::PolyPressure { pitch, value } => [0xA0, pitch as u32, seven(value)],
        // ponytail: per-note expression has no AUv2 message; send it as MIDI 2.0 (or MPE) when a
        // unit that takes those turns up.
        ControlKind::Expression { .. } => return None,
    })
}
