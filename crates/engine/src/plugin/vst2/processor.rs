//! The plugin's audio-thread side.

use super::*;

/// A VST2 plugin's audio-thread side.
pub struct Vst2Processor {
    pub(crate) instance: Arc<Instance>,
    pub(crate) edits: EditReceiver,
    /// Automation for the next block: a parameter and its value at the block's start.
    pub(crate) automation: [(u32, f32); AUTOMATED],
    pub(crate) automated: usize,
    pub(crate) events: Box<EventList>,
    // Channel buffers, and the pointers to them the plugin is given.
    pub(crate) _buffers: Vec<Vec<f32>>,
    pub(crate) inputs: Vec<*mut f32>,
    pub(crate) outputs: Vec<*mut f32>,
    pub(crate) max_block: usize,
    /// Whether inputs 3 and 4 are a sidechain.
    pub(crate) sidechain: bool,
}

// The buffer pointers are only used from the audio thread once the processor moves there.
unsafe impl Send for Vst2Processor {}

impl ProcessorApi for Vst2Processor {
    fn latency(&self) -> u32 {
        unsafe { (*self.instance.effect).initial_delay.max(0) as u32 }
    }

    fn set_transport(&mut self, playing: bool, position: u64, tempo: f64) {
        let i = &self.instance;
        let started = playing != i.playing.swap(playing, Relaxed);
        let jumped = playing && position != i.position.swap(position, Relaxed);
        if started || jumped {
            i.changed.store(true, Relaxed);
        }
        i.tempo.store(tempo.to_bits(), Relaxed);
    }

    /// ponytail: VST2 has no changes within a block, so a parameter takes its start value for
    /// the whole block; split blocks where automation moves fast if that's heard.
    fn automate(&mut self, id: u32, start: f64, _end: f64) {
        if self.automated < AUTOMATED {
            self.automation[self.automated] = (id, start.clamp(0.0, 1.0) as f32);
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
        let i = &*self.instance;
        PROCESSING.set(true);
        self.edits.drain(|id, value| {
            i.set(id, value as f32);
            i.applied(id, value);
        });
        // Automation after edits: where a parameter has both, automation wins.
        for &(id, value) in &self.automation[..std::mem::take(&mut self.automated)] {
            i.set(id, value);
        }

        // Notes and controls, merged in time.
        self.events.clear();
        let (mut n, mut c) = (notes.iter().peekable(), controls.iter().peekable());
        loop {
            let note_first = match (n.peek(), c.peek()) {
                (Some(a), Some(b)) => a.offset <= b.offset,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            let event = match note_first {
                true => Some(note_midi(n.next().unwrap())),
                false => control_midi(c.next().unwrap()),
            };
            if let Some(e) = event {
                self.events.push(e);
            }
        }
        if self.events.len() > 0 {
            let list = (&mut self.events.list as *mut VstEvents).cast();
            i.call(eff::PROCESS_EVENTS, 0, 0, list, 0.0);
        }

        let (n_in, n_out) = unsafe { ((*i.effect).num_inputs, (*i.effect).num_outputs) };
        let fits = (0..=self.inputs.len() as i32).contains(&n_in)
            && (1..=self.outputs.len() as i32).contains(&n_out);
        if fits {
            for (c, &p) in self.inputs.iter().enumerate() {
                let dst = unsafe { std::slice::from_raw_parts_mut(p, frames) };
                let src = match c {
                    0 | 1 => Some(&buf[c][..frames]),
                    2 | 3 if self.sidechain => sidechain.map(|s| &s[c - 2][..frames]),
                    _ => None,
                };
                match src {
                    Some(src) => dst.copy_from_slice(src),
                    None => dst.fill(0.0),
                }
            }
            let (ins, outs) = (self.inputs.as_mut_ptr(), self.outputs.as_mut_ptr());
            unsafe {
                match (*i.effect).process_replacing {
                    Some(f) if i.flags() & FLAG_CAN_REPLACING != 0 => {
                        f(i.effect, ins, outs, frames as i32)
                    }
                    _ => {
                        // The old `process` adds to what's there.
                        for &p in &self.outputs {
                            std::slice::from_raw_parts_mut(p, frames).fill(0.0);
                        }
                        if let Some(f) = (*i.effect).process {
                            f(i.effect, ins, outs, frames as i32);
                        }
                    }
                }
            }
        }
        PROCESSING.set(false);
        i.position.fetch_add(frames as u64, Relaxed);
        i.changed.store(false, Relaxed);

        // Mono output goes to both sides.
        for (c, dst) in buf.into_iter().enumerate() {
            match fits {
                true => {
                    let p = self.outputs[c.min(n_out as usize - 1)];
                    dst[..frames].copy_from_slice(unsafe { std::slice::from_raw_parts(p, frames) });
                }
                false => dst[..frames].fill(0.0),
            }
        }
    }
}

impl Drop for Vst2Processor {
    fn drop(&mut self) {
        let i = &self.instance;
        i.call(eff::STOP_PROCESS, 0, 0, ptr::null_mut(), 0.0);
        i.call(eff::MAINS_CHANGED, 0, 0, ptr::null_mut(), 0.0);
    }
}
