//! A plugin's instance: its dispatcher called, its settings and editor; and the host's callback.

use super::*;

/// One instance of a plugin, and what Pina Studio tells it through the host callback. The plugin's
/// two halves (and its editor, while open) share it; the last of them to go closes it.
pub(crate) struct Instance {
    pub(crate) effect: *mut AEffect,
    pub(crate) dispatcher: Dispatcher,
    pub(crate) set_parameter: SetParameter,
    pub(crate) get_parameter: GetParameter,
    pub(crate) rate: f64,
    pub(crate) max_block: usize,
    /// The transport, as `set_transport` last put it and `process` moved it on.
    pub(crate) playing: AtomicBool,
    pub(crate) position: AtomicU64,
    pub(crate) tempo: AtomicU64,
    /// Whether it started or stopped, or jumped, since the last block.
    pub(crate) changed: AtomicBool,
    /// What Pina Studio last set each parameter to, until the processor has applied it (`NOTHING`
    /// when nothing's waiting): the plugin has it only then.
    pub(crate) sent: Box<[AtomicU64]>,
    /// The editor's window while one is open, for the plugin to resize.
    pub(crate) window: AtomicPtr<c_void>,
}

// The plugin is called from the main and audio threads, as VST2 plugins expect.
unsafe impl Send for Instance {}

unsafe impl Sync for Instance {}

impl Instance {
    pub(crate) fn call(
        &self,
        op: i32,
        index: i32,
        value: isize,
        ptr: *mut c_void,
        opt: f32,
    ) -> isize {
        unsafe { (self.dispatcher)(self.effect, op, index, value, ptr, opt) }
    }

    /// A string the plugin writes for `op`.
    pub(crate) fn string(&self, op: i32, index: i32) -> String {
        // Far more than the SDK's limits, which plugins don't all keep to.
        let mut buf = [0u8; 256];
        self.call(op, index, 0, buf.as_mut_ptr().cast(), 0.0);
        text(&buf)
    }

    pub(crate) fn num_params(&self) -> u32 {
        unsafe { (*self.effect).num_params.max(0) as u32 }
    }

    pub(crate) fn num_programs(&self) -> u32 {
        unsafe { (*self.effect).num_programs.max(0) as u32 }
    }

    pub(crate) fn flags(&self) -> i32 {
        unsafe { (*self.effect).flags }
    }

    pub(crate) fn unique_id(&self) -> i32 {
        unsafe { (*self.effect).unique_id }
    }

    pub(crate) fn uses_chunks(&self) -> bool {
        self.flags() & FLAG_PROGRAM_CHUNKS != 0
    }

    pub(crate) fn get(&self, id: u32) -> f32 {
        match id < self.num_params() {
            true => unsafe { (self.get_parameter)(self.effect, id as i32) },
            false => 0.0,
        }
    }

    pub(crate) fn set(&self, id: u32, value: f32) {
        if id < self.num_params() {
            unsafe { (self.set_parameter)(self.effect, id as i32, value) };
        }
    }

    /// A parameter's value: what Pina Studio last set it to, if that's still on its way.
    pub(crate) fn value(&self, id: u32) -> f64 {
        let sent = self.sent.get(id as usize).map(|s| s.load(Relaxed));
        match sent.map(f64::from_bits) {
            Some(v) if !v.is_nan() => v,
            _ => self.get(id) as f64,
        }
    }

    /// Marks an edit of Pina Studio's as having reached the plugin, unless a later one is on its way.
    pub(crate) fn applied(&self, id: u32, value: f64) {
        if let Some(s) = self.sent.get(id as usize) {
            let _ = s.compare_exchange(value.to_bits(), NOTHING, Relaxed, Relaxed);
        }
    }

    /// Forgets the edits on their way: the plugin's settings were just replaced.
    pub(crate) fn forget_sent(&self) {
        self.sent.iter().for_each(|s| s.store(NOTHING, Relaxed));
    }

    pub(crate) fn properties(&self, id: u32) -> Option<ParameterProperties> {
        let mut p: ParameterProperties = unsafe { std::mem::zeroed() };
        let ok = self.call(
            eff::GET_PARAMETER_PROPERTIES,
            id as i32,
            0,
            (&mut p as *mut ParameterProperties).cast(),
            0.0,
        );
        (ok != 0).then_some(p)
    }

    pub(crate) fn category(&self) -> isize {
        self.call(eff::GET_PLUG_CATEGORY, 0, 0, ptr::null_mut(), 0.0)
    }

    pub(crate) fn is_synth(&self) -> bool {
        self.flags() & FLAG_IS_SYNTH != 0 || self.category() == CATEGORY_SYNTH
    }

    /// Its name, or failing that its bundle's.
    pub(crate) fn name(&self, bundle: &Path) -> String {
        let name = self.string(eff::GET_EFFECT_NAME, 0);
        match name.is_empty() {
            true => bundle
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            false => name,
        }
    }

    /// A shell's plugins: their ids and names.
    pub(crate) fn shell_plugins(&self) -> Vec<(i32, String)> {
        let mut out = Vec::new();
        loop {
            let mut buf = [0u8; 256];
            let id = self.call(
                eff::SHELL_GET_NEXT_PLUGIN,
                0,
                0,
                buf.as_mut_ptr().cast(),
                0.0,
            ) as i32;
            if id == 0 || out.len() > 10_000 {
                return out;
            }
            out.push((id, text(&buf)));
        }
    }

    pub(crate) fn program(&self) -> i32 {
        self.call(eff::GET_PROGRAM, 0, 0, ptr::null_mut(), 0.0) as i32
    }

    pub(crate) fn set_program(&self, program: i32) {
        self.call(eff::BEGIN_SET_PROGRAM, 0, 0, ptr::null_mut(), 0.0);
        self.call(eff::SET_PROGRAM, 0, program as isize, ptr::null_mut(), 0.0);
        self.call(eff::END_SET_PROGRAM, 0, 0, ptr::null_mut(), 0.0);
    }

    /// Its chunk: all of its settings, or with `preset` its current program's.
    pub(crate) fn chunk(&self, preset: bool) -> Option<Vec<u8>> {
        let mut data: *mut c_void = ptr::null_mut();
        let size = self.call(
            eff::GET_CHUNK,
            preset as i32,
            0,
            (&mut data as *mut *mut c_void).cast(),
            0.0,
        );
        (size > 0 && !data.is_null()).then(|| {
            unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size as usize) }.to_vec()
        })
    }

    pub(crate) fn set_chunk(&self, preset: bool, chunk: &[u8]) {
        self.call(
            eff::SET_CHUNK,
            preset as i32,
            chunk.len() as isize,
            chunk.as_ptr() as *mut c_void,
            0.0,
        );
    }

    /// Its settings now: its chunk (of everything, or with `preset` of its program) if it keeps
    /// one, else its parameters' values.
    pub(crate) fn settings(&self, preset: bool) -> Option<Settings<'static>> {
        match self.uses_chunks() {
            true => self.chunk(preset).map(|c| Settings::Chunk(c.into())),
            false => Some(Settings::Params(
                (0..self.num_params())
                    .map(|p| self.value(p) as f32)
                    .collect(),
            )),
        }
    }

    /// Gives it settings (from `settings`) back, on the main thread, as VST2 hosts do.
    pub(crate) fn restore(&self, settings: &Settings, preset: bool) {
        match settings {
            Settings::Chunk(c) => self.set_chunk(preset, c),
            Settings::Params(values) => {
                for (p, &v) in (0..self.num_params()).zip(values) {
                    self.set(p, v);
                }
            }
        }
        self.forget_sent();
    }

    /// Its editor's size, if it says.
    pub(crate) fn editor_size(&self) -> Option<(f64, f64)> {
        let mut rect: *mut ERect = ptr::null_mut();
        self.call(
            eff::EDIT_GET_RECT,
            0,
            0,
            (&mut rect as *mut *mut ERect).cast(),
            0.0,
        );
        let r = unsafe { rect.as_ref() }?;
        let size = ((r.right - r.left) as f64, (r.bottom - r.top) as f64);
        (size.0 > 0.0 && size.1 > 0.0).then_some(size)
    }

    /// Where the transport is, as the plugin asks for it.
    pub(crate) fn time(&self) -> VstTimeInfo {
        let tempo = f64::from_bits(self.tempo.load(Relaxed));
        let position = self.position.load(Relaxed) as f64;
        let ppq = position / self.rate * tempo / 60.0;
        let mut flags = PINA_VALID | PPQ_POS_VALID | BARS_VALID | TIME_SIG_VALID;
        if self.playing.load(Relaxed) {
            flags |= TRANSPORT_PLAYING;
        }
        if self.changed.load(Relaxed) {
            flags |= TRANSPORT_CHANGED;
        }
        let mut t: VstTimeInfo = unsafe { std::mem::zeroed() };
        t.sample_pos = position;
        t.sample_rate = self.rate;
        t.ppq_pos = ppq;
        t.tempo = tempo;
        // ponytail: 4/4, as the VST3 host's context; pass the project's signature when it varies.
        t.bar_start_pos = (ppq / 4.0).floor() * 4.0;
        t.time_sig_numerator = 4;
        t.time_sig_denominator = 4;
        t.flags = flags;
        t
    }

    /// Resizes the editor's window, as the plugin asks: only on the main thread, with one open.
    pub(crate) fn resize(&self, width: i32, height: i32) -> bool {
        let window = self.window.load(Relaxed);
        if window.is_null() || MainThreadMarker::new().is_none() {
            return false;
        }
        unsafe { Window::resize_raw(window, width as f64, height as f64) };
        true
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe { (*self.effect).resvd1 = 0 };
        self.call(eff::CLOSE, 0, 0, ptr::null_mut(), 0.0);
    }
}

thread_local! {
    /// Set while this thread runs a plugin's audio, so it hears it's on the audio thread.
    pub(crate) static PROCESSING: Cell<bool> = const { Cell::new(false) };
    /// While a plugin's entry point runs on this thread, before it has an `Instance`: the
    /// shell plugin wanted (0, any), the sample rate and block size.
    pub(crate) static LOADING: Cell<Option<(i32, f64, usize)>> = const { Cell::new(None) };
    /// Where `audioMasterGetTime` answers: one per thread, so threads never share it.
    static TIME: UnsafeCell<VstTimeInfo> = const { UnsafeCell::new(unsafe { std::mem::zeroed() }) };
}

/// What a plugin asks of Pina Studio.
pub(crate) unsafe extern "C" fn host_callback(
    effect: *mut AEffect,
    op: i32,
    index: i32,
    value: isize,
    ptr: *mut c_void,
    _opt: f32,
) -> isize {
    // While its entry point runs, `resvd1` isn't Pina Studio's yet.
    let loading = LOADING.get();
    let instance = match effect.is_null() || loading.is_some() {
        true => None,
        false => unsafe { ((*effect).resvd1 as *const Instance).as_ref() },
    };
    let (uid, rate, block) = loading.unwrap_or((0, 44_100.0, 512));
    match op {
        // A parameter turned in the plugin's editor: the plugin has it already, so it's what
        // Pina Studio reads from now on.
        master::AUTOMATE => {
            if let Some(s) = instance.and_then(|i| i.sent.get(index.max(0) as usize)) {
                s.store(NOTHING, Relaxed);
            }
            0
        }
        master::VERSION => 2400,
        master::CURRENT_ID => match (uid, effect.is_null()) {
            (0, false) => unsafe { (*effect).unique_id as isize },
            _ => uid as isize,
        },
        master::GET_TIME => match instance {
            Some(i) => TIME.with(|t| {
                unsafe { *t.get() = i.time() };
                t.get() as isize
            }),
            None => 0,
        },
        master::GET_SAMPLE_RATE => instance.map_or(rate, |i| i.rate) as isize,
        master::GET_BLOCK_SIZE => instance.map_or(block, |i| i.max_block) as isize,
        master::GET_CURRENT_PROCESS_LEVEL => match PROCESSING.get() {
            true => PROCESS_LEVEL_REALTIME,
            false => PROCESS_LEVEL_USER,
        },
        master::SIZE_WINDOW => instance.is_some_and(|i| i.resize(index, value as i32)) as isize,
        master::GET_VENDOR_STRING | master::GET_PRODUCT_STRING if !ptr.is_null() => {
            unsafe { ptr::copy_nonoverlapping(c"Pina Studio".as_ptr(), ptr.cast(), 12) };
            1
        }
        master::GET_VENDOR_VERSION => 1,
        master::CAN_DO if !ptr.is_null() => {
            let asked = unsafe { CStr::from_ptr(ptr.cast()) }.to_bytes();
            HOST_CAN_DO.contains(&asked) as isize
        }
        // ponytail: MIDI the plugin sends is dropped; forward it when a track can take it.
        master::PROCESS_EVENTS => 1,
        // ponytail: a change of latency or channels takes effect when the plugin is next
        // loaded; `process` goes silent rather than pass it fewer channels than it now has.
        master::IO_CHANGED => 1,
        // Editors redraw from `idle`; parameters are read from the plugin each time.
        master::IDLE | master::UPDATE_DISPLAY | master::BEGIN_EDIT | master::END_EDIT => 1,
        _ => 0,
    }
}
