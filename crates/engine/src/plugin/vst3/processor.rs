//! The plugin's audio-thread side, and its events.

use super::*;

/// A VST3 plugin's audio-thread side.
pub struct Vst3Processor {
    pub(crate) processor: ComPtr<IAudioProcessor>,
    pub(crate) component: ComPtr<IComponent>,
    pub(crate) controller: ComPtr<IEditController>,
    /// The component and controller, when they're separate objects talking to each other.
    pub(crate) links: Option<(ComPtr<IConnectionPoint>, ComPtr<IConnectionPoint>)>,
    pub(crate) events: ComWrapper<EventList>,
    pub(crate) params: ComWrapper<ParameterChanges>,
    /// Where the plugin reports its own changes (meters, MIDI it makes), as a host must give it.
    // ponytail: read by nothing; forward them when something shows plugin meters or MIDI out.
    pub(crate) out_events: ComWrapper<EventList>,
    pub(crate) out_params: ComWrapper<ParameterChanges>,
    pub(crate) edits: EditReceiver,
    /// Automation for the next block: a parameter, and its value at the block's start and end.
    pub(crate) automation: [(ParamID, ParamValue, ParamValue); AUTOMATED],
    pub(crate) automated: usize,
    // Channel buffers, their pointers, and the bus structs that point at those.
    pub(crate) _buffers: Vec<Vec<f32>>,
    pub(crate) _channels: Vec<Vec<*mut f32>>,
    pub(crate) inputs: Vec<AudioBusBuffers>,
    pub(crate) outputs: Vec<AudioBusBuffers>,
    pub(crate) context: ProcessContext,
    pub(crate) max_block: usize,
    /// The parameter each MIDI controller moves (0-127, then 128 aftertouch, 129 pitch bend), as
    /// the plugin maps them.
    pub(crate) midi: [Option<ParamID>; 130],
    /// Whether its second input is a sidechain, and switched on.
    pub(crate) sidechain: bool,
}

// The COM pointers are only used from the audio thread once the processor moves there.
unsafe impl Send for Vst3Processor {}

impl Drop for Vst3Processor {
    fn drop(&mut self) {
        unsafe {
            self.processor.setProcessing(0);
            self.component.setActive(0);
            if let Some((a, b)) = &self.links {
                a.disconnect(b.as_ptr());
                b.disconnect(a.as_ptr());
                self.controller.terminate();
            }
            self.component.terminate();
        }
    }
}

impl ProcessorApi for Vst3Processor {
    fn latency(&self) -> u32 {
        unsafe { self.processor.getLatencySamples() }
    }

    fn set_transport(&mut self, playing: bool, position: u64, tempo: f64) {
        let c = &mut self.context;
        c.tempo = tempo;
        c.projectTimeSamples = position as i64;
        c.projectTimeMusic = position as f64 / c.sampleRate * tempo / 60.0;
        let playing = if playing { kPlaying } else { 0 };
        c.state = (kTempoValid | kTimeSigValid | kProjectTimeMusicValid | playing) as _;
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

    fn pending(&self) -> bool {
        self.edits.pending()
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
        self.events.clear();
        self.params.clear();
        // Notes and controls, merged in time; a note is known by its pitch, so expression finds it.
        let (mut n, mut c) = (notes.iter().peekable(), controls.iter().peekable());
        loop {
            let note_first = match (n.peek(), c.peek()) {
                (Some(a), Some(b)) => a.offset <= b.offset,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            if note_first {
                self.events.push(note_event(n.next().unwrap()));
            } else {
                let c = c.next().unwrap();
                match c.kind {
                    ControlKind::Cc { number, value } => {
                        self.midi_param(number as usize, c.offset, value as f64)
                    }
                    ControlKind::Aftertouch(v) => self.midi_param(128, c.offset, v as f64),
                    ControlKind::PitchBend(v) => {
                        self.midi_param(129, c.offset, (v as f64 + 1.0) / 2.0)
                    }
                    ControlKind::PolyPressure { pitch, value } => {
                        let mut e: Event = unsafe { std::mem::zeroed() };
                        e.sampleOffset = c.offset as int32;
                        e.r#type = kPolyPressureEvent as _;
                        e.__field0.polyPressure = PolyPressureEvent {
                            channel: 0,
                            pitch: pitch as _,
                            pressure: value.clamp(0.0, 1.0),
                            noteId: pitch as _,
                        };
                        self.events.push(e);
                    }
                    ControlKind::Expression { pitch, kind, value } => {
                        let (type_id, value) = expression(kind, value as f64);
                        let mut e: Event = unsafe { std::mem::zeroed() };
                        e.sampleOffset = c.offset as int32;
                        e.r#type = kNoteExpressionValueEvent as _;
                        e.__field0.noteExpressionValue = NoteExpressionValueEvent {
                            typeId: type_id,
                            noteId: pitch as _,
                            value,
                        };
                        self.events.push(e);
                    }
                }
            }
        }
        let params = &self.params;
        self.edits.drain(|id, value| params.add(id, 0, value));
        // Automation after edits: where a parameter has both, automation wins.
        for &(id, start, end) in &self.automation[..std::mem::take(&mut self.automated)] {
            self.params.add(id, 0, start);
            self.params.add(id, frames.saturating_sub(1) as int32, end);
        }
        self.out_events.clear();
        self.out_params.clear();
        for b in &mut self.outputs {
            b.silenceFlags = 0;
        }
        for (i, b) in self.inputs.iter().enumerate() {
            for c in 0..b.numChannels as usize {
                let dst = unsafe {
                    std::slice::from_raw_parts_mut(*b.__field0.channelBuffers32.add(c), frames)
                };
                let src = match i {
                    0 => buf.get(c).map(|s| &s[..]),
                    1 => sidechain.map(|s| s[c.min(1)]),
                    _ => None,
                };
                match src {
                    Some(src) => dst.copy_from_slice(&src[..frames]),
                    None => dst.fill(0.0),
                }
            }
        }
        let mut data = ProcessData {
            processMode: kRealtime as _,
            symbolicSampleSize: kSample32 as _,
            numSamples: frames as int32,
            numInputs: self.inputs.len() as int32,
            numOutputs: self.outputs.len() as int32,
            inputs: if self.inputs.is_empty() {
                ptr::null_mut()
            } else {
                self.inputs.as_mut_ptr()
            },
            outputs: self.outputs.as_mut_ptr(),
            inputParameterChanges: com_ptr::<_, IParameterChanges>(&self.params),
            outputParameterChanges: com_ptr::<_, IParameterChanges>(&self.out_params),
            inputEvents: com_ptr::<_, IEventList>(&self.events),
            outputEvents: com_ptr::<_, IEventList>(&self.out_events),
            processContext: &mut self.context,
        };
        unsafe { self.processor.process(&mut data) };
        self.context.projectTimeSamples += frames as i64;
        self.context.continousTimeSamples += frames as i64;
        // Mono output goes to both sides.
        let out = &self.outputs[0];
        for (c, dst) in buf.into_iter().enumerate() {
            let c = c.min(out.numChannels as usize - 1);
            let src = unsafe {
                std::slice::from_raw_parts(*out.__field0.channelBuffers32.add(c), frames)
            };
            dst[..frames].copy_from_slice(src);
        }
    }
}

impl Vst3Processor {
    /// A MIDI controller's move, as a change to the parameter the plugin maps it to (if any).
    pub(crate) fn midi_param(&self, controller: usize, offset: u32, value: f64) {
        if let Some(Some(id)) = self.midi.get(controller) {
            self.params.add(*id, offset as int32, value.clamp(0.0, 1.0));
        }
    }
}

/// A note on or off, its id its pitch.
pub(crate) fn note_event(n: &Note) -> Event {
    let mut e: Event = unsafe { std::mem::zeroed() };
    e.sampleOffset = n.offset as int32;
    if n.velocity > 0.0 {
        e.r#type = kNoteOnEvent as _;
        e.__field0.noteOn = NoteOnEvent {
            channel: 0,
            pitch: n.pitch as _,
            tuning: 0.0,
            velocity: n.velocity,
            length: 0,
            noteId: n.pitch as _,
        };
    } else {
        e.r#type = kNoteOffEvent as _;
        e.__field0.noteOff = NoteOffEvent {
            channel: 0,
            pitch: n.pitch as _,
            velocity: 0.0,
            noteId: n.pitch as _,
            tuning: 0.0,
        };
    }
    e
}

/// A note expression's VST3 type, and its value as VST3 has it, 0 to 1.
pub(crate) fn expression(kind: ExpressionKind, value: f64) -> (NoteExpressionTypeID, f64) {
    use NoteExpressionTypeIDs_::*;
    let (id, v) = match kind {
        ExpressionKind::Tuning => (kTuningTypeID, 0.5 + value / 240.0),
        // 0.25 is as played; 1 is 12 dB up.
        ExpressionKind::Volume => (kVolumeTypeID, value / 4.0),
        ExpressionKind::Pan => (kPanTypeID, (value + 1.0) / 2.0),
        ExpressionKind::Brightness => (kBrightnessTypeID, value),
        ExpressionKind::Vibrato => (kVibratoTypeID, value),
        ExpressionKind::Expression => (kExpressionTypeID, value),
    };
    (id as _, v.clamp(0.0, 1.0))
}
