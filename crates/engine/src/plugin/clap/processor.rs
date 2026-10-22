//! The plugin's audio-thread side.

use super::*;

/// A CLAP plugin's audio-thread side.
pub struct ClapProcessor {
    pub(crate) inst: Arc<Instance>,
    pub(crate) plugin: *const clap_plugin,
    pub(crate) edits: EditReceiver,
    /// As the plugin first described its parameters.
    // ponytail: not updated when it rescans them; send new ranges if a plugin changes its own.
    pub(crate) ranges: Vec<Range>,
    /// Automation for the next block: a parameter, and its value at the block's start and end.
    pub(crate) automation: [(u32, f64, f64); AUTOMATED],
    pub(crate) automated: usize,
    pub(crate) events: Vec<Event>,
    /// Its notes are MIDI, not CLAP's own note events (and it has no note expression).
    pub(crate) midi_notes: bool,
    // Channel buffers, their pointers, and the port structs that point at those.
    pub(crate) _buffers: Vec<Vec<f32>>,
    pub(crate) _channels: Vec<Vec<*mut f32>>,
    pub(crate) inputs: Vec<clap_audio_buffer>,
    pub(crate) outputs: Vec<clap_audio_buffer>,
    pub(crate) transport: clap_event_transport,
    pub(crate) rate: f64,
    pub(crate) playing: bool,
    pub(crate) position: u64,
    pub(crate) tempo: f64,
    pub(crate) steady: i64,
    pub(crate) max_block: usize,
    pub(crate) latency: u32,
    /// Whether it has a second input, for a sidechain.
    pub(crate) sidechain: bool,
}

// The plugin's pointers are only used from the audio thread once the processor moves there.
unsafe impl Send for ClapProcessor {}

impl ProcessorApi for ClapProcessor {
    fn latency(&self) -> u32 {
        self.latency
    }

    fn set_transport(&mut self, playing: bool, position: u64, tempo: f64) {
        (self.playing, self.position, self.tempo) = (playing, position, tempo);
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
        self.edits.pending() || self.inst.flush.load(SeqCst)
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
        // Events in time order, as CLAP has them: edits, then automation from the block's start
        // (where a parameter has both, automation wins), then notes and controls, then where
        // automation ends.
        let last = frames as u32 - 1;
        self.events.clear();
        // A process is the flush it may have asked for.
        self.inst.flush.store(false, SeqCst);
        let (events, ranges) = (&mut self.events, &self.ranges);
        let mut push = |e: Option<Event>| {
            if let Some(e) = e
                && events.len() < events.capacity()
            {
                events.push(e);
            }
        };
        let param = |time, id, v| Some(param_event(time, id, range_of(ranges, id)?.plain(v)));
        let mut edits = 0;
        self.edits.drain(|id, v| {
            push(param(0, id, v));
            edits += 1;
        });
        let automation = &self.automation[..std::mem::take(&mut self.automated)];
        for &(id, start, _) in automation {
            push(param(0, id, start));
        }
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
                let note = n.next().unwrap();
                push(Some(note_event(
                    note,
                    note.offset.min(last),
                    self.midi_notes,
                )));
            } else {
                let control = c.next().unwrap();
                push(control_event(
                    control,
                    control.offset.min(last),
                    self.midi_notes,
                ));
            }
        }
        for &(id, _, end) in automation {
            push(param(last, id, end));
        }

        for (i, port) in self.inputs.iter().enumerate() {
            for c in 0..port.channel_count as usize {
                let dst = unsafe { slice::from_raw_parts_mut(*port.data32.add(c), frames) };
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
        for port in &mut self.outputs {
            port.constant_mask = 0;
        }
        self.transport = transport(self.playing, self.position, self.tempo, self.rate);
        let (input, output) = (input_events(&self.events), output_events());
        let data = clap_process {
            steady_time: self.steady,
            frames_count: frames as u32,
            transport: &self.transport,
            audio_inputs: match self.inputs.is_empty() {
                true => ptr::null(),
                false => self.inputs.as_ptr(),
            },
            audio_outputs: self.outputs.as_mut_ptr(),
            audio_inputs_count: self.inputs.len() as u32,
            audio_outputs_count: self.outputs.len() as u32,
            in_events: &input,
            out_events: &output,
        };
        // The main thread may be flushing to it, for a moment, if it had stopped running.
        while !self.inst.enter(PROCESSING) {
            std::hint::spin_loop();
        }
        let (plugin, inst) = (self.plugin, &self.inst);
        let ok = as_audio(|| unsafe {
            let p = &*plugin;
            // CLAP starts processing on the audio thread, so it starts with the first block.
            if !inst.processing.load(SeqCst) {
                if !p.start_processing.is_none_or(|start| start(plugin)) {
                    return false;
                }
                inst.processing.store(true, SeqCst);
            }
            p.process
                .is_some_and(|process| process(plugin, &data) != CLAP_PROCESS_ERROR)
        });
        self.inst.leave();
        self.inst.processed.store(now(), SeqCst);
        self.inst.applied.fetch_add(edits, SeqCst);
        self.steady += frames as i64;
        if self.playing {
            self.position += frames as u64;
        }
        // Mono output goes to both sides.
        let out = &self.outputs[0];
        for (c, dst) in buf.into_iter().enumerate() {
            let dst = &mut dst[..frames];
            if !ok {
                dst.fill(0.0);
                continue;
            }
            let c = c.min(out.channel_count as usize - 1);
            let src = unsafe { slice::from_raw_parts(*out.data32.add(c), frames) };
            match out.constant_mask & (1 << c) != 0 {
                true => dst.fill(src[0]),
                false => dst.copy_from_slice(src),
            }
        }
    }
}
