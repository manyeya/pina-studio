//! Hosting VST2 plugins (macOS): load a `.vst` bundle, run it on the audio thread, and open its
//! editor in a window of its own. The VST2 interface (2.4) is written out here: the `AEffect` a
//! plugin's entry point returns, the opcodes Pina Studio sends it, and the host callback it calls.
//!
//! VST2 has no controller apart from the processor: what the editor shows, and what Pina Studio reads
//! of a parameter, is what plays.

use std::{
    cell::{Cell, OnceCell, UnsafeCell},
    collections::HashMap,
    ffi::{CStr, CString, c_char, c_void},
    path::{Path, PathBuf},
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering::Relaxed},
    },
};

use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::NSWindow;
use objc2_foundation::NSSize;

use super::{
    AUTOMATED, Control, ControlKind, EditReceiver, EditSender, Editor, Note, Plugin, PluginApi,
    PluginParam, Preset, PresetSource, Processor, ProcessorApi, edit_queue, editor_window,
};
use crate::{
    plugin::scan::PluginInfo,
    song::project::{Format, PluginRef},
};

// The VST2 interface, as the 2.4 SDK lays it out.

/// `AEffect::magic`.
const MAGIC: i32 = i32::from_be_bytes(*b"VstP");

type Dispatcher = unsafe extern "C" fn(*mut AEffect, i32, i32, isize, *mut c_void, f32) -> isize;
type ProcessFn = unsafe extern "C" fn(*mut AEffect, *mut *mut f32, *mut *mut f32, i32);
type ProcessDoubleFn = unsafe extern "C" fn(*mut AEffect, *mut *mut f64, *mut *mut f64, i32);
type SetParameter = unsafe extern "C" fn(*mut AEffect, i32, f32);
type GetParameter = unsafe extern "C" fn(*mut AEffect, i32) -> f32;
type HostCallback = unsafe extern "C" fn(*mut AEffect, i32, i32, isize, *mut c_void, f32) -> isize;
type Entry = unsafe extern "C" fn(HostCallback) -> *mut AEffect;

/// A plugin instance, as its entry point makes it.
#[repr(C)]
struct AEffect {
    magic: i32,
    dispatcher: Option<Dispatcher>,
    /// The old accumulating `process`, for plugins without `process_replacing`.
    process: Option<ProcessFn>,
    set_parameter: Option<SetParameter>,
    get_parameter: Option<GetParameter>,
    num_programs: i32,
    num_params: i32,
    num_inputs: i32,
    num_outputs: i32,
    flags: i32,
    /// The host's: Pina Studio keeps the plugin's `Instance` here, for the host callback.
    resvd1: isize,
    resvd2: isize,
    initial_delay: i32,
    real_qualities: i32,
    off_qualities: i32,
    io_ratio: f32,
    object: *mut c_void,
    user: *mut c_void,
    unique_id: i32,
    version: i32,
    process_replacing: Option<ProcessFn>,
    process_double_replacing: Option<ProcessDoubleFn>,
    future: [u8; 56],
}

const FLAG_HAS_EDITOR: i32 = 1;
const FLAG_CAN_REPLACING: i32 = 1 << 4;
const FLAG_PROGRAM_CHUNKS: i32 = 1 << 5;
const FLAG_IS_SYNTH: i32 = 1 << 8;

const CATEGORY_SYNTH: isize = 2;
const CATEGORY_SHELL: isize = 10;

/// What Pina Studio asks of a plugin, through its dispatcher.
mod eff {
    pub const OPEN: i32 = 0;
    pub const CLOSE: i32 = 1;
    pub const SET_PROGRAM: i32 = 2;
    pub const GET_PROGRAM: i32 = 3;
    pub const GET_PARAM_LABEL: i32 = 6;
    pub const GET_PARAM_DISPLAY: i32 = 7;
    pub const GET_PARAM_NAME: i32 = 8;
    pub const SET_SAMPLE_RATE: i32 = 10;
    pub const SET_BLOCK_SIZE: i32 = 11;
    pub const MAINS_CHANGED: i32 = 12;
    pub const EDIT_GET_RECT: i32 = 13;
    pub const EDIT_OPEN: i32 = 14;
    pub const EDIT_CLOSE: i32 = 15;
    pub const EDIT_IDLE: i32 = 19;
    pub const GET_CHUNK: i32 = 23;
    pub const SET_CHUNK: i32 = 24;
    pub const PROCESS_EVENTS: i32 = 25;
    pub const CAN_BE_AUTOMATED: i32 = 26;
    pub const STRING_TO_PARAMETER: i32 = 27;
    pub const GET_PROGRAM_NAME_INDEXED: i32 = 29;
    pub const GET_PLUG_CATEGORY: i32 = 35;
    pub const GET_EFFECT_NAME: i32 = 45;
    pub const GET_VENDOR_STRING: i32 = 47;
    pub const VENDOR_SPECIFIC: i32 = 50;
    pub const GET_PARAMETER_PROPERTIES: i32 = 56;
    pub const BEGIN_SET_PROGRAM: i32 = 67;
    pub const END_SET_PROGRAM: i32 = 68;
    pub const SHELL_GET_NEXT_PLUGIN: i32 = 70;
    pub const START_PROCESS: i32 = 71;
    pub const STOP_PROCESS: i32 = 72;
    pub const SET_PROCESS_PRECISION: i32 = 77;
}

/// What a plugin asks of Pina Studio, through the host callback. The SDK skips 5: the opcodes after
/// `audioMasterPinConnected` (4) start at 6.
mod master {
    pub const AUTOMATE: i32 = 0;
    pub const VERSION: i32 = 1;
    pub const CURRENT_ID: i32 = 2;
    pub const IDLE: i32 = 3;
    pub const GET_TIME: i32 = 7;
    pub const PROCESS_EVENTS: i32 = 8;
    pub const IO_CHANGED: i32 = 13;
    pub const SIZE_WINDOW: i32 = 15;
    pub const GET_SAMPLE_RATE: i32 = 16;
    pub const GET_BLOCK_SIZE: i32 = 17;
    pub const GET_CURRENT_PROCESS_LEVEL: i32 = 23;
    pub const GET_VENDOR_STRING: i32 = 32;
    pub const GET_PRODUCT_STRING: i32 = 33;
    pub const GET_VENDOR_VERSION: i32 = 34;
    pub const CAN_DO: i32 = 37;
    pub const UPDATE_DISPLAY: i32 = 42;
    pub const BEGIN_EDIT: i32 = 43;
    pub const END_EDIT: i32 = 44;
}

/// What a host that can do these says to `audioMasterCanDo`.
const HOST_CAN_DO: [&[u8]; 8] = [
    b"sendVstEvents",
    b"sendVstMidiEvent",
    b"sendVstTimeInfo",
    b"receiveVstEvents",
    b"receiveVstMidiEvent",
    b"sizeWindow",
    b"startStopProcess",
    b"shellCategory",
];

const PROCESS_LEVEL_USER: isize = 1;
const PROCESS_LEVEL_REALTIME: isize = 2;

/// The answer to the host extension that shows any value of a parameter, not just its own.
const COCKOS_YES: isize = 0xbeef;

#[repr(C)]
#[derive(Clone, Copy)]
struct VstTimeInfo {
    sample_pos: f64,
    sample_rate: f64,
    nano_seconds: f64,
    ppq_pos: f64,
    tempo: f64,
    bar_start_pos: f64,
    cycle_start_pos: f64,
    cycle_end_pos: f64,
    time_sig_numerator: i32,
    time_sig_denominator: i32,
    smpte_offset: i32,
    smpte_frame_rate: i32,
    samples_to_next_clock: i32,
    flags: i32,
}

const TRANSPORT_CHANGED: i32 = 1;
const TRANSPORT_PLAYING: i32 = 1 << 1;
const PPQ_POS_VALID: i32 = 1 << 9;
const PINA_VALID: i32 = 1 << 10;
const BARS_VALID: i32 = 1 << 11;
const TIME_SIG_VALID: i32 = 1 << 13;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct VstMidiEvent {
    kind: i32,
    byte_size: i32,
    delta_frames: i32,
    flags: i32,
    note_length: i32,
    note_offset: i32,
    midi_data: [u8; 4],
    detune: i8,
    note_off_velocity: u8,
    reserved1: u8,
    reserved2: u8,
}

const MIDI_TYPE: i32 = 1;

/// Most events a block sends a plugin; the rest are dropped.
const EVENTS: usize = 512;

/// `VstEvents`, with room for `EVENTS`.
#[repr(C)]
struct VstEvents {
    num_events: i32,
    reserved: isize,
    events: [*mut VstMidiEvent; EVENTS],
}

#[repr(C)]
#[derive(Default)]
struct ERect {
    top: i16,
    left: i16,
    bottom: i16,
    right: i16,
}

#[repr(C)]
struct ParameterProperties {
    step_float: f32,
    small_step_float: f32,
    large_step_float: f32,
    label: [u8; 64],
    flags: i32,
    min_integer: i32,
    max_integer: i32,
    step_integer: i32,
    large_step_integer: i32,
    short_label: [u8; 8],
    display_index: i16,
    category: i16,
    num_parameters_in_category: i16,
    reserved: i16,
    category_label: [u8; 24],
    future: [u8; 16],
}

const PARAM_IS_SWITCH: i32 = 1;
const PARAM_USES_INTEGER_MIN_MAX: i32 = 1 << 1;
const PARAM_USES_INT_STEP: i32 = 1 << 3;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFURLCreateFromFileSystemRepresentation(
        a: *const c_void,
        buf: *const u8,
        len: isize,
        dir: u8,
    ) -> *const c_void;
    fn CFBundleCreate(a: *const c_void, url: *const c_void) -> *const c_void;
    fn CFBundleLoadExecutable(bundle: *const c_void) -> u8;
    fn CFBundleGetFunctionPointerForName(
        bundle: *const c_void,
        name: *const c_void,
    ) -> *const c_void;
    fn CFStringCreateWithCString(
        a: *const c_void,
        s: *const c_char,
        encoding: u32,
    ) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

/// A loaded `.vst` bundle: its entry point, which makes a new instance each time it's called.
// ponytail: never unloaded; add when the plugin browser rescans.
#[derive(Clone, Copy)]
pub struct Library {
    entry: Entry,
}

impl Library {
    pub fn load(path: &Path) -> Result<Library, String> {
        let fail = |why: &str| format!("{}: {why}", path.display());
        unsafe {
            let p =
                CString::new(path.as_os_str().as_encoded_bytes()).map_err(|_| fail("bad path"))?;
            let url = CFURLCreateFromFileSystemRepresentation(
                ptr::null(),
                p.as_ptr().cast(),
                p.as_bytes().len() as isize,
                1,
            );
            if url.is_null() {
                return Err(fail("bad path"));
            }
            let bundle = CFBundleCreate(ptr::null(), url);
            CFRelease(url);
            if bundle.is_null() {
                return Err(fail("not a bundle"));
            }
            if CFBundleLoadExecutable(bundle) == 0 {
                return Err(fail("couldn't load its binary (wrong architecture?)"));
            }
            let entry = [c"VSTPluginMain", c"main_macho", c"main"]
                .into_iter()
                .find_map(|name| symbol(bundle, name))
                .ok_or_else(|| fail("no VSTPluginMain"))?;
            Ok(Library { entry })
        }
    }

    /// A new instance of its plugin, opened: for a shell, of the plugin `uid` in it (0, any).
    fn open(&self, uid: i32, rate: f64, max_block: usize) -> Result<Arc<Instance>, String> {
        LOADING.set(Some((uid, rate, max_block)));
        let effect = unsafe { (self.entry)(host_callback) };
        LOADING.set(None);
        if effect.is_null() {
            return Err("its entry point made no plugin".into());
        }
        let e = unsafe { &*effect };
        let (true, Some(dispatcher), Some(set_parameter), Some(get_parameter)) = (
            e.magic == MAGIC,
            e.dispatcher,
            e.set_parameter,
            e.get_parameter,
        ) else {
            return Err("not a VST2 plugin".into());
        };
        let sent = (0..e.num_params.max(0))
            .map(|_| AtomicU64::new(NOTHING))
            .collect();
        let instance = Arc::new(Instance {
            effect,
            dispatcher,
            set_parameter,
            get_parameter,
            rate,
            max_block,
            playing: AtomicBool::new(false),
            position: AtomicU64::new(0),
            tempo: AtomicU64::new(120f64.to_bits()),
            changed: AtomicBool::new(false),
            sent,
            window: AtomicPtr::new(ptr::null_mut()),
        });
        // From here on the host callback knows which instance is calling.
        unsafe { (*effect).resvd1 = Arc::as_ptr(&instance) as isize };
        instance.call(eff::OPEN, 0, 0, ptr::null_mut(), 0.0);
        Ok(instance)
    }
}

unsafe fn symbol<F: Copy>(bundle: *const c_void, name: &CStr) -> Option<F> {
    const UTF8: u32 = 0x0800_0100;
    unsafe {
        let s = CFStringCreateWithCString(ptr::null(), name.as_ptr(), UTF8);
        let f = CFBundleGetFunctionPointerForName(bundle, s);
        CFRelease(s);
        (!f.is_null()).then(|| std::mem::transmute_copy(&f))
    }
}

/// A plugin's text from a buffer it filled: UTF-8 if it is, else Latin-1, as older plugins
/// write it.
fn text(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let bytes = buf[..end].to_vec();
    let s = String::from_utf8(bytes)
        .unwrap_or_else(|e| e.into_bytes().iter().map(|&b| b as char).collect());
    s.trim().to_string()
}

/// A plugin's unique id as its four characters ("DfRr"), or as a number when they aren't
/// printable (or would read as one).
fn uid_text(id: i32) -> String {
    let bytes = id.to_be_bytes();
    let code = String::from_utf8_lossy(&bytes);
    match bytes.iter().all(|b| b.is_ascii_graphic()) && code.parse::<i32>().is_err() {
        true => code.into_owned(),
        false => id.to_string(),
    }
}

fn parse_uid(s: &str) -> Option<i32> {
    s.parse().ok().or_else(|| {
        <[u8; 4]>::try_from(s.as_bytes())
            .ok()
            .map(i32::from_be_bytes)
    })
}

/// "Nothing waiting" in `Instance::sent`.
const NOTHING: u64 = f64::NAN.to_bits();

/// One instance of a plugin, and what Pina Studio tells it through the host callback. The plugin's
/// two halves (and its editor, while open) share it; the last of them to go closes it.
struct Instance {
    effect: *mut AEffect,
    dispatcher: Dispatcher,
    set_parameter: SetParameter,
    get_parameter: GetParameter,
    rate: f64,
    max_block: usize,
    /// The transport, as `set_transport` last put it and `process` moved it on.
    playing: AtomicBool,
    position: AtomicU64,
    tempo: AtomicU64,
    /// Whether it started or stopped, or jumped, since the last block.
    changed: AtomicBool,
    /// What Pina Studio last set each parameter to, until the processor has applied it (`NOTHING`
    /// when nothing's waiting): the plugin has it only then.
    sent: Box<[AtomicU64]>,
    /// The editor's window while one is open, for the plugin to resize.
    window: AtomicPtr<NSWindow>,
}

// The plugin is called from the main and audio threads, as VST2 plugins expect.
unsafe impl Send for Instance {}
unsafe impl Sync for Instance {}

impl Instance {
    fn call(&self, op: i32, index: i32, value: isize, ptr: *mut c_void, opt: f32) -> isize {
        unsafe { (self.dispatcher)(self.effect, op, index, value, ptr, opt) }
    }

    /// A string the plugin writes for `op`.
    fn string(&self, op: i32, index: i32) -> String {
        // Far more than the SDK's limits, which plugins don't all keep to.
        let mut buf = [0u8; 256];
        self.call(op, index, 0, buf.as_mut_ptr().cast(), 0.0);
        text(&buf)
    }

    fn num_params(&self) -> u32 {
        unsafe { (*self.effect).num_params.max(0) as u32 }
    }

    fn num_programs(&self) -> u32 {
        unsafe { (*self.effect).num_programs.max(0) as u32 }
    }

    fn flags(&self) -> i32 {
        unsafe { (*self.effect).flags }
    }

    fn unique_id(&self) -> i32 {
        unsafe { (*self.effect).unique_id }
    }

    fn uses_chunks(&self) -> bool {
        self.flags() & FLAG_PROGRAM_CHUNKS != 0
    }

    fn get(&self, id: u32) -> f32 {
        match id < self.num_params() {
            true => unsafe { (self.get_parameter)(self.effect, id as i32) },
            false => 0.0,
        }
    }

    fn set(&self, id: u32, value: f32) {
        if id < self.num_params() {
            unsafe { (self.set_parameter)(self.effect, id as i32, value) };
        }
    }

    /// A parameter's value: what Pina Studio last set it to, if that's still on its way.
    fn value(&self, id: u32) -> f64 {
        let sent = self.sent.get(id as usize).map(|s| s.load(Relaxed));
        match sent.map(f64::from_bits) {
            Some(v) if !v.is_nan() => v,
            _ => self.get(id) as f64,
        }
    }

    /// Marks an edit of Pina Studio's as having reached the plugin, unless a later one is on its way.
    fn applied(&self, id: u32, value: f64) {
        if let Some(s) = self.sent.get(id as usize) {
            let _ = s.compare_exchange(value.to_bits(), NOTHING, Relaxed, Relaxed);
        }
    }

    /// Forgets the edits on their way: the plugin's settings were just replaced.
    fn forget_sent(&self) {
        self.sent.iter().for_each(|s| s.store(NOTHING, Relaxed));
    }

    fn properties(&self, id: u32) -> Option<ParameterProperties> {
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

    fn category(&self) -> isize {
        self.call(eff::GET_PLUG_CATEGORY, 0, 0, ptr::null_mut(), 0.0)
    }

    fn is_synth(&self) -> bool {
        self.flags() & FLAG_IS_SYNTH != 0 || self.category() == CATEGORY_SYNTH
    }

    /// Its name, or failing that its bundle's.
    fn name(&self, bundle: &Path) -> String {
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
    fn shell_plugins(&self) -> Vec<(i32, String)> {
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

    fn program(&self) -> i32 {
        self.call(eff::GET_PROGRAM, 0, 0, ptr::null_mut(), 0.0) as i32
    }

    fn set_program(&self, program: i32) {
        self.call(eff::BEGIN_SET_PROGRAM, 0, 0, ptr::null_mut(), 0.0);
        self.call(eff::SET_PROGRAM, 0, program as isize, ptr::null_mut(), 0.0);
        self.call(eff::END_SET_PROGRAM, 0, 0, ptr::null_mut(), 0.0);
    }

    /// Its chunk: all of its settings, or with `preset` its current program's.
    fn chunk(&self, preset: bool) -> Option<Vec<u8>> {
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

    fn set_chunk(&self, preset: bool, chunk: &[u8]) {
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
    fn settings(&self, preset: bool) -> Option<Settings<'static>> {
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
    fn restore(&self, settings: &Settings, preset: bool) {
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
    fn editor_size(&self) -> Option<(f64, f64)> {
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
    fn time(&self) -> VstTimeInfo {
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
    fn resize(&self, width: i32, height: i32) -> bool {
        let window = self.window.load(Relaxed);
        if window.is_null() || MainThreadMarker::new().is_none() {
            return false;
        }
        let size = NSSize::new(width as f64, height as f64);
        unsafe { (*window).setContentSize(size) };
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
    static PROCESSING: Cell<bool> = const { Cell::new(false) };
    /// While a plugin's entry point runs on this thread, before it has an `Instance`: the
    /// shell plugin wanted (0, any), the sample rate and block size.
    static LOADING: Cell<Option<(i32, f64, usize)>> = const { Cell::new(None) };
    /// Where `audioMasterGetTime` answers: one per thread, so threads never share it.
    static TIME: UnsafeCell<VstTimeInfo> = const { UnsafeCell::new(unsafe { std::mem::zeroed() }) };
}

/// What a plugin asks of Pina Studio.
unsafe extern "C" fn host_callback(
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

/// What a plugin's settings are kept as: its own chunk, or its parameters' values.
#[derive(Clone, Debug, PartialEq)]
enum Settings<'a> {
    Chunk(std::borrow::Cow<'a, [u8]>),
    Params(Vec<f32>),
}

const STATE_MAGIC: &[u8; 4] = b"TMV2";

/// A plugin's state as one blob: its program, then its chunk or its parameters' values.
fn pack(program: i32, settings: &Settings) -> Vec<u8> {
    let mut out = STATE_MAGIC.to_vec();
    out.extend(program.to_le_bytes());
    match settings {
        Settings::Chunk(c) => {
            out.push(b'C');
            out.extend(c.iter());
        }
        Settings::Params(values) => {
            out.push(b'P');
            out.extend(values.iter().flat_map(|v| v.to_le_bytes()));
        }
    }
    out
}

fn unpack(blob: &[u8]) -> Option<(i32, Settings<'_>)> {
    let rest = blob.strip_prefix(STATE_MAGIC)?;
    let (program, rest) = rest.split_first_chunk::<4>()?;
    let settings = match rest.split_first()? {
        (b'C', chunk) => Settings::Chunk(chunk.into()),
        (b'P', values) if values.len() % 4 == 0 => Settings::Params(
            values
                .as_chunks::<4>()
                .0
                .iter()
                .map(|&b| f32::from_le_bytes(b))
                .collect(),
        ),
        _ => return None,
    };
    Some((i32::from_le_bytes(*program), settings))
}

/// An `.fxp` preset: the plugin it's for, its name and settings.
#[derive(Debug, PartialEq)]
struct Fxp<'a> {
    id: i32,
    name: String,
    settings: Settings<'a>,
}

/// Where an `.fxp` file's settings start: after its header and 28-byte program name.
const FXP_HEADER: usize = 56;

fn read_fxp(bytes: &[u8]) -> Option<Fxp<'_>> {
    let be = |at: usize| Some(i32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    if bytes.get(..4)? != b"CcnK" {
        return None;
    }
    let count = be(24)?.max(0) as usize;
    let settings = match bytes.get(8..12)? {
        b"FxCk" if count <= (bytes.len().saturating_sub(FXP_HEADER)) / 4 => Settings::Params(
            (0..count)
                .map(|i| be(FXP_HEADER + 4 * i).map(|b| f32::from_bits(b as u32)))
                .collect::<Option<_>>()?,
        ),
        b"FPCh" => {
            let size = be(FXP_HEADER)?.max(0) as usize;
            let at = FXP_HEADER + 4;
            Settings::Chunk(bytes.get(at..at.checked_add(size)?)?.into())
        }
        _ => return None,
    };
    Some(Fxp {
        id: be(16)?,
        name: text(bytes.get(28..FXP_HEADER)?),
        settings,
    })
}

/// The plugin an `.fxp` file is for, from its header.
fn fxp_id(file: &Path) -> Option<i32> {
    use std::io::Read;
    let mut head = [0u8; 20];
    std::fs::File::open(file).ok()?.read_exact(&mut head).ok()?;
    (&head[..4] == b"CcnK").then(|| i32::from_be_bytes(head[16..20].try_into().unwrap()))
}

/// An `.fxp` file: `settings` for plugin `id` (at `version`, with `params` parameters), called
/// `name`.
fn write_fxp(id: i32, version: i32, params: u32, name: &str, settings: &Settings) -> Vec<u8> {
    let mut body = Vec::new();
    let magic = match settings {
        Settings::Params(_) => b"FxCk",
        Settings::Chunk(_) => b"FPCh",
    };
    body.extend(magic);
    body.extend(1i32.to_be_bytes());
    body.extend(id.to_be_bytes());
    body.extend(version.to_be_bytes());
    let count = match settings {
        Settings::Params(values) => values.len() as i32,
        Settings::Chunk(_) => params as i32,
    };
    body.extend(count.to_be_bytes());
    let mut prg_name = [0u8; 28];
    for (dst, src) in prg_name[..27].iter_mut().zip(name.bytes()) {
        *dst = src;
    }
    body.extend(prg_name);
    match settings {
        Settings::Params(values) => body.extend(values.iter().flat_map(|v| v.to_be_bytes())),
        Settings::Chunk(c) => {
            body.extend((c.len() as i32).to_be_bytes());
            body.extend(c.iter());
        }
    }
    let mut out = b"CcnK".to_vec();
    out.extend((body.len() as i32).to_be_bytes());
    out.extend(body);
    out
}

/// How many steps a parameter has from 0 to 1, by its properties (0, continuous).
fn steps(p: &ParameterProperties) -> u32 {
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
fn midi(offset: u32, data: [u8; 3]) -> VstMidiEvent {
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
fn seven_bit(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 127.0).round() as u8
}

/// A note on or off, on channel 1.
fn note_midi(n: &Note) -> VstMidiEvent {
    match n.velocity > 0.0 {
        true => midi(
            n.offset,
            [0x90, n.pitch & 0x7f, seven_bit(n.velocity).max(1)],
        ),
        false => midi(n.offset, [0x80, n.pitch & 0x7f, 0]),
    }
}

/// A control as MIDI, if MIDI has it.
fn control_midi(c: &Control) -> Option<VstMidiEvent> {
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
struct EventList {
    list: VstEvents,
    midi: [VstMidiEvent; EVENTS],
}

impl EventList {
    fn new() -> Box<EventList> {
        Box::new(EventList {
            list: VstEvents {
                num_events: 0,
                reserved: 0,
                events: [ptr::null_mut(); EVENTS],
            },
            midi: [midi(0, [0; 3]); EVENTS],
        })
    }

    fn len(&self) -> usize {
        self.list.num_events as usize
    }

    fn clear(&mut self) {
        self.list.num_events = 0;
    }

    /// Adds an event, unless the list is full.
    fn push(&mut self, e: VstMidiEvent) {
        let n = self.len();
        if n < EVENTS {
            self.midi[n] = e;
            self.list.events[n] = &mut self.midi[n];
            self.list.num_events += 1;
        }
    }
}

/// The plugins in a `.vst` bundle, opening it in this process.
pub fn inspect(bundle: &Path) -> Result<Vec<PluginInfo>, String> {
    let library = Library::load(bundle)?;
    let i = library
        .open(0, 44_100.0, 512)
        .map_err(|e| format!("{}: {e}", bundle.display()))?;
    let vendor = i.string(eff::GET_VENDOR_STRING, 0);
    // VST2's categories, as words.
    let category = match i.category() {
        1 => "Fx",
        2 => "Instrument",
        3 => "Analyzer",
        4 => "Mastering",
        5 => "Spatial",
        6 => "Reverb",
        7 => "Surround",
        8 => "Restoration",
        11 => "Generator",
        _ => "",
    };
    let info = |class: String, uid: i32, instrument: bool| PluginInfo {
        format: Format::Vst2,
        path: bundle.to_path_buf(),
        class,
        uid: Some(uid_text(uid)),
        vendor: vendor.clone(),
        instrument,
        categories: [category]
            .iter()
            .filter(|c| !c.is_empty())
            .map(|c| c.to_string())
            .collect(),
    };
    if i.category() == CATEGORY_SHELL {
        // ponytail: a shell's plugins are listed as effects, without opening each (slow for
        // big shells); open each if a shell of instruments turns up.
        let plugins = i.shell_plugins();
        return Ok(plugins
            .into_iter()
            .map(|(uid, name)| info(name, uid, false))
            .collect());
    }
    Ok(vec![info(i.name(bundle), i.unique_id(), i.is_synth())])
}

/// Loads the plugin `r` names, from its bundle (loaded once into `libraries`).
pub fn instantiate(
    libraries: &mut HashMap<PathBuf, Library>,
    r: &PluginRef,
    rate: f64,
    max_block: usize,
    state: Option<&[u8]>,
) -> Result<(Plugin, Processor), String> {
    if !libraries.contains_key(&r.path) {
        libraries.insert(r.path.clone(), Library::load(&r.path)?);
    }
    let library = &libraries[&r.path];
    let fail = |why: &str| format!("{}: {why}", r.class);
    let uid = r.uid.as_deref().and_then(parse_uid).unwrap_or(0);
    let mut instance = library.open(uid, rate, max_block).map_err(|e| fail(&e))?;
    // A shell that didn't make the plugin asked for: find it by name.
    if instance.category() == CATEGORY_SHELL {
        let (uid, _) = instance
            .shell_plugins()
            .into_iter()
            .find(|(_, name)| *name == r.class)
            .ok_or_else(|| format!("{}: no plugin named {:?}", r.path.display(), r.class))?;
        drop(instance);
        instance = library.open(uid, rate, max_block).map_err(|e| fail(&e))?;
    }
    let (plugin, processor) =
        Vst2Plugin::start(*library, instance, &r.path, rate, max_block, state)
            .map_err(|e| fail(&e))?;
    Ok((Box::new(plugin), Box::new(processor)))
}

/// A VST2 plugin's main-thread side: its parameters, settings, presets and editor.
pub struct Vst2Plugin {
    name: String,
    vendor: String,
    /// Whether inputs 3 and 4 are a sidechain, as VST2 plugins have one.
    sidechain: bool,
    library: Library,
    instance: Arc<Instance>,
    edits: EditSender,
    /// Each parameter's value when it was loaded.
    defaults: Vec<f64>,
    /// A second instance of the plugin, never heard, that shows values the playing one doesn't
    /// have: made the first time it's needed (`None` if it couldn't be).
    twin: OnceCell<Option<Arc<Instance>>>,
}

/// A VST2 plugin's audio-thread side.
pub struct Vst2Processor {
    instance: Arc<Instance>,
    edits: EditReceiver,
    /// Automation for the next block: a parameter and its value at the block's start.
    automation: [(u32, f32); AUTOMATED],
    automated: usize,
    events: Box<EventList>,
    // Channel buffers, and the pointers to them the plugin is given.
    _buffers: Vec<Vec<f32>>,
    inputs: Vec<*mut f32>,
    outputs: Vec<*mut f32>,
    max_block: usize,
    /// Whether inputs 3 and 4 are a sidechain.
    sidechain: bool,
}

// The buffer pointers are only used from the audio thread once the processor moves there.
unsafe impl Send for Vst2Processor {}

impl Vst2Plugin {
    /// Sets an opened plugin up to run at `rate`, restoring `state` (from `Plugin::state`) if
    /// given.
    fn start(
        library: Library,
        i: Arc<Instance>,
        bundle: &Path,
        rate: f64,
        max_block: usize,
        state: Option<&[u8]>,
    ) -> Result<(Vst2Plugin, Vst2Processor), String> {
        let (n_in, n_out) = unsafe { ((*i.effect).num_inputs, (*i.effect).num_outputs) };
        if n_out <= 0 {
            return Err("no audio output".into());
        }
        if i.flags() & FLAG_CAN_REPLACING == 0 && unsafe { (*i.effect).process.is_none() } {
            return Err("no way to process audio".into());
        }
        i.call(eff::SET_SAMPLE_RATE, 0, 0, ptr::null_mut(), rate as f32);
        i.call(
            eff::SET_BLOCK_SIZE,
            0,
            max_block as isize,
            ptr::null_mut(),
            0.0,
        );
        i.call(eff::SET_PROCESS_PRECISION, 0, 0, ptr::null_mut(), 0.0);
        let defaults = (0..i.num_params()).map(|p| i.get(p) as f64).collect();
        if let Some((program, settings)) = state.and_then(unpack) {
            if program != i.program() && (0..i.num_programs() as i32).contains(&program) {
                i.set_program(program);
            }
            i.restore(&settings, false);
        }
        // Stepped parameters (switches, types, modes) end a block's edits: see EditReceiver.
        let stepped = (0..i.num_params())
            .filter(|&p| i.properties(p).is_some_and(|props| steps(&props) > 0))
            .collect();
        let (edits_tx, edits) = edit_queue(stepped);
        i.call(eff::MAINS_CHANGED, 0, 1, ptr::null_mut(), 0.0);
        i.call(eff::START_PROCESS, 0, 0, ptr::null_mut(), 0.0);

        let mut buffers: Vec<Vec<f32>> = (0..n_in.max(0) + n_out)
            .map(|_| vec![0f32; max_block])
            .collect();
        let mut pointers: Vec<*mut f32> = buffers.iter_mut().map(|b| b.as_mut_ptr()).collect();
        let outputs = pointers.split_off(n_in.max(0) as usize);
        let plugin = Vst2Plugin {
            name: i.name(bundle),
            vendor: i.string(eff::GET_VENDOR_STRING, 0),
            sidechain: n_in >= 4,
            library,
            instance: i.clone(),
            edits: edits_tx,
            defaults,
            twin: OnceCell::new(),
        };
        let processor = Vst2Processor {
            instance: i,
            edits,
            automation: [(0, 0.0); AUTOMATED],
            automated: 0,
            events: EventList::new(),
            _buffers: buffers,
            inputs: pointers,
            outputs,
            max_block,
            sidechain: n_in >= 4,
        };
        Ok((plugin, processor))
    }

    /// Its settings as a blob for `state`.
    fn snapshot(&self) -> Option<Vec<u8>> {
        let settings = self.instance.settings(false)?;
        Some(pack(self.instance.program(), &settings))
    }

    /// The twin, its parameters set as the playing instance's are.
    // ponytail: settings a plugin keeps outside its parameters (in its chunk) aren't copied;
    // copy the chunk too if a display depends on them.
    fn twin(&self) -> Option<&Instance> {
        let i = &self.instance;
        let twin = self.twin.get_or_init(|| {
            let twin = self.library.open(i.unique_id(), i.rate, i.max_block).ok()?;
            twin.call(eff::SET_SAMPLE_RATE, 0, 0, ptr::null_mut(), i.rate as f32);
            twin.call(
                eff::SET_BLOCK_SIZE,
                0,
                i.max_block as isize,
                ptr::null_mut(),
                0.0,
            );
            Some(twin)
        });
        let twin = twin.as_deref()?;
        for p in 0..i.num_params().min(twin.num_params()) {
            let v = i.value(p) as f32;
            if twin.get(p) != v {
                twin.set(p, v);
            }
        }
        Some(twin)
    }
}

impl PluginApi for Vst2Plugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn sidechain(&self) -> bool {
        self.sidechain
    }

    fn params(&self) -> Vec<PluginParam> {
        let i = &self.instance;
        let n = i.num_params();
        // A plugin that says none can be automated doesn't answer: then all of them can.
        let automatable: Vec<bool> = (0..n)
            .map(|p| i.call(eff::CAN_BE_AUTOMATED, p as i32, 0, ptr::null_mut(), 0.0) != 0)
            .collect();
        let any = automatable.contains(&true);
        (0..n)
            .filter(|&p| !any || automatable[p as usize])
            .map(|p| {
                let props = i.properties(p);
                // Its properties' label is its full name; the name itself may be cut short.
                let name = props
                    .as_ref()
                    .map(|props| text(&props.label))
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| i.string(eff::GET_PARAM_NAME, p as i32));
                PluginParam {
                    id: p,
                    name,
                    value: i.value(p),
                    default: self.defaults.get(p as usize).copied().unwrap_or(0.0),
                    steps: props.as_ref().map_or(0, steps),
                    units: i.string(eff::GET_PARAM_LABEL, p as i32),
                }
            })
            .collect()
    }

    fn set_param(&self, id: u32, value: f64) {
        let value = value.clamp(0.0, 1.0);
        if let Some(s) = self.instance.sent.get(id as usize) {
            s.store(value.to_bits(), Relaxed);
        }
        self.edits.push(id, value);
    }

    fn show_param(&self, _id: u32, _value: f64) {
        // Its editor shows what plays, and automation sets that as it plays.
    }

    fn param_value(&self, id: u32) -> f64 {
        self.instance.value(id)
    }

    /// VST2 shows only a parameter's current value. Another is shown by the host extension
    /// (where the plugin has it), else by the twin set to it: never by touching what
    /// plays.
    fn param_text(&self, id: u32, value: f64) -> String {
        let i = &self.instance;
        if id >= i.num_params() {
            return String::new();
        }
        let units = i.string(eff::GET_PARAM_LABEL, id as i32);
        let with_units = |s: String| match units.is_empty() || s.is_empty() {
            true => s,
            false => format!("{s} {units}"),
        };
        if (value as f32 - i.get(id)).abs() < 1e-6 {
            return with_units(i.string(eff::GET_PARAM_DISPLAY, id as i32));
        }
        let mut buf = [0u8; 256];
        let cockos = i.call(
            eff::VENDOR_SPECIFIC,
            eff::GET_PARAM_DISPLAY,
            id as isize,
            buf.as_mut_ptr().cast(),
            value as f32,
        );
        if cockos == COCKOS_YES {
            return with_units(text(&buf));
        }
        match self.twin() {
            Some(twin) => {
                twin.set(id, value as f32);
                with_units(twin.string(eff::GET_PARAM_DISPLAY, id as i32))
            }
            None => format!("{:.0}%", value * 100.0),
        }
    }

    /// VST2 reads a typed value only by setting the parameter to it, so the twin reads it.
    fn param_from_text(&self, id: u32, text: &str) -> Option<f64> {
        if id >= self.instance.num_params() {
            return None;
        }
        let text = CString::new(text).ok()?;
        let twin = self.twin()?;
        let ok = twin.call(
            eff::STRING_TO_PARAMETER,
            id as i32,
            0,
            text.as_ptr() as *mut c_void,
            0.0,
        );
        (ok != 0).then(|| twin.get(id) as f64)
    }

    fn state(&self) -> Option<Vec<u8>> {
        self.snapshot()
    }

    fn set_state(&self, state: &[u8]) -> Result<(), String> {
        let (program, settings) = unpack(state).ok_or("not a VST2 plugin's settings")?;
        let i = &self.instance;
        if program != i.program() && (0..i.num_programs() as i32).contains(&program) {
            i.set_program(program);
        }
        i.restore(&settings, false);
        Ok(())
    }

    fn presets(&self) -> Vec<Preset> {
        let i = &self.instance;
        let named = |program: u32| {
            let mut buf = [0u8; 256];
            let ok = i.call(
                eff::GET_PROGRAM_NAME_INDEXED,
                program as i32,
                -1,
                buf.as_mut_ptr().cast(),
                0.0,
            );
            match ok {
                0 => String::new(),
                _ => text(&buf),
            }
        };
        let mut out: Vec<Preset> = Vec::new();
        // One unnamed program is no list at all (many plugins have just that).
        if i.num_programs() > 1 || !named(0).is_empty() {
            out.extend((0..i.num_programs()).map(|program| Preset {
                name: match named(program) {
                    name if name.is_empty() => format!("Program {}", program + 1),
                    name => name,
                },
                source: PresetSource::Program { list: 0, program },
            }));
        }
        let id = i.unique_id();
        let files = super::preset_files(&super::preset_dirs(&self.vendor, &self.name), "fxp");
        out.extend(files.into_iter().filter(
            |p| matches!(&p.source, PresetSource::File { file } if fxp_id(file) == Some(id)),
        ));
        out
    }

    fn load_preset(&self, preset: &Preset) -> Result<(), String> {
        let i = &self.instance;
        match &preset.source {
            PresetSource::Program { program, .. } => {
                if *program >= i.num_programs() {
                    return Err(format!("{}: it has no program {program}", self.name));
                }
                i.set_program(*program as i32);
                i.forget_sent();
                Ok(())
            }
            PresetSource::File { file } => {
                let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
                let fxp = read_fxp(&bytes)
                    .ok_or_else(|| format!("{}: not an .fxp preset", file.display()))?;
                if fxp.id != i.unique_id() {
                    return Err(format!("{}: a preset for another plugin", file.display()));
                }
                i.restore(&fxp.settings, true);
                Ok(())
            }
        }
    }

    fn save_preset(&self, name: &str) -> Result<PathBuf, String> {
        let i = &self.instance;
        let settings = i
            .settings(true)
            .ok_or("the plugin wouldn't give its settings")?;
        let dir = super::preset_dirs(&self.vendor, &self.name).remove(0);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(super::preset_file_name(name, "fxp"));
        let version = unsafe { (*i.effect).version };
        let bytes = write_fxp(i.unique_id(), version, i.num_params(), name, &settings);
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }

    fn open_editor(&self, mtm: MainThreadMarker) -> Result<Editor, String> {
        let fail = |why: &str| format!("{}: {why}", self.name);
        let i = &self.instance;
        if i.flags() & FLAG_HAS_EDITOR == 0 {
            return Err(fail("no editor"));
        }
        // Some editors know their size only once open.
        let size = i.editor_size();
        let window = editor_window(mtm, &self.name, size.unwrap_or((400.0, 300.0)));
        let content = window
            .contentView()
            .ok_or_else(|| fail("window has no content view"))?;
        i.window
            .store(Retained::as_ptr(&window) as *mut NSWindow, Relaxed);
        i.call(
            eff::EDIT_OPEN,
            0,
            0,
            Retained::as_ptr(&content) as *mut c_void,
            0.0,
        );
        if let Some(opened) = i.editor_size().filter(|&s| Some(s) != size) {
            window.setContentSize(NSSize::new(opened.0, opened.1));
        }
        window.makeKeyAndOrderFront(None);
        let instance = i.clone();
        Ok(Editor::new(window, move || {
            instance.window.store(ptr::null_mut(), Relaxed);
            instance.call(eff::EDIT_CLOSE, 0, 0, ptr::null_mut(), 0.0);
        }))
    }

    fn idle(&self) {
        if !self.instance.window.load(Relaxed).is_null() {
            self.instance
                .call(eff::EDIT_IDLE, 0, 0, ptr::null_mut(), 0.0);
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_abi_is_laid_out_as_the_sdk_has_it() {
        assert_eq!(size_of::<AEffect>(), 192);
        assert_eq!(std::mem::offset_of!(AEffect, resvd1), 64);
        assert_eq!(std::mem::offset_of!(AEffect, unique_id), 112);
        assert_eq!(std::mem::offset_of!(AEffect, process_replacing), 120);
        assert_eq!(size_of::<VstTimeInfo>(), 88);
        assert_eq!(size_of::<VstMidiEvent>(), 32);
        assert_eq!(std::mem::offset_of!(VstEvents, events), 16);
        assert_eq!(size_of::<ParameterProperties>(), 152);
        assert_eq!(size_of::<ERect>(), 8);
    }

    #[test]
    fn state_blobs_round_trip() {
        let chunk = Settings::Chunk(b"chunk"[..].into());
        assert_eq!(unpack(&pack(3, &chunk)), Some((3, chunk)));
        let params = Settings::Params(vec![0.0, 0.25, 1.0]);
        let blob = pack(-1, &params);
        assert_eq!(unpack(&blob), Some((-1, params)));
        assert_eq!(unpack(b"junk"), None);
        assert_eq!(unpack(&blob[..blob.len() - 1]), None); // cut short
        assert_eq!(unpack(&blob[..8]), None);
    }

    #[test]
    fn fxp_presets_round_trip() {
        let params = Settings::Params(vec![0.5, 0.125]);
        let bytes = write_fxp(0x4466_5272, 7, 2, "Small Room", &params);
        assert_eq!(&bytes[..4], b"CcnK");
        assert_eq!(&bytes[8..12], b"FxCk");
        assert_eq!(bytes.len(), FXP_HEADER + 8);
        let fxp = read_fxp(&bytes).unwrap();
        assert_eq!(
            fxp,
            Fxp {
                id: 0x4466_5272,
                name: "Small Room".into(),
                settings: params
            }
        );

        // A chunk, and a name longer than the file has room for.
        let chunk = Settings::Chunk(b"opaque settings"[..].into());
        let long = "A name far longer than twenty-seven bytes";
        let bytes = write_fxp(42, 1, 12, long, &chunk);
        assert_eq!(&bytes[8..12], b"FPCh");
        let fxp = read_fxp(&bytes).unwrap();
        assert_eq!(fxp.settings, chunk);
        assert_eq!(fxp.name, long[..27]);
        let size = i32::from_be_bytes(bytes[4..8].try_into().unwrap());
        assert_eq!(size as usize, bytes.len() - 8);

        assert_eq!(read_fxp(&bytes[..bytes.len() - 1]), None); // cut short
        assert_eq!(read_fxp(b"CcnK"), None);
        let mut lying = write_fxp(42, 1, 2, "x", &Settings::Params(vec![0.5]));
        lying[24..28].copy_from_slice(&1000i32.to_be_bytes()); // more values than it holds
        assert_eq!(read_fxp(&lying), None);
    }

    #[test]
    fn ids_read_as_their_characters_or_numbers() {
        assert_eq!(uid_text(i32::from_be_bytes(*b"DfRr")), "DfRr");
        assert_eq!(uid_text(1234), "1234");
        // Four digits are a number's characters, so they're written as the number they are.
        let digits = i32::from_be_bytes(*b"1234");
        assert_eq!(uid_text(digits), digits.to_string());
        for id in [i32::from_be_bytes(*b"DfRr"), 1234, digits, -5, 0] {
            assert_eq!(parse_uid(&uid_text(id)), Some(id));
        }
        assert_eq!(parse_uid("toolong"), None);
    }

    #[test]
    fn notes_and_controls_pack_as_midi() {
        let on = note_midi(&Note {
            offset: 7,
            pitch: 60,
            velocity: 1.0,
        });
        assert_eq!((on.delta_frames, on.midi_data), (7, [0x90, 60, 127, 0]));
        assert_eq!((on.kind, on.byte_size), (MIDI_TYPE, 32));
        // The quietest note is still a note on.
        let soft = note_midi(&Note {
            offset: 0,
            pitch: 60,
            velocity: 0.001,
        });
        assert_eq!(soft.midi_data, [0x90, 60, 1, 0]);
        let off = note_midi(&Note {
            offset: 9,
            pitch: 60,
            velocity: 0.0,
        });
        assert_eq!(off.midi_data, [0x80, 60, 0, 0]);

        let control = |kind| control_midi(&Control { offset: 3, kind }).map(|e| e.midi_data);
        let pedal = ControlKind::Cc {
            number: 64,
            value: 1.0,
        };
        assert_eq!(control(pedal), Some([0xb0, 64, 127, 0]));
        assert_eq!(control(ControlKind::PitchBend(0.0)), Some([0xe0, 0, 64, 0]));
        assert_eq!(control(ControlKind::PitchBend(-1.0)), Some([0xe0, 0, 0, 0]));
        assert_eq!(
            control(ControlKind::PitchBend(1.0)),
            Some([0xe0, 127, 127, 0])
        );
        assert_eq!(
            control(ControlKind::Aftertouch(0.5)),
            Some([0xd0, 64, 0, 0])
        );
        let pressure = ControlKind::PolyPressure {
            pitch: 62,
            value: 0.0,
        };
        assert_eq!(control(pressure), Some([0xa0, 62, 0, 0]));
        let expression = ControlKind::Expression {
            pitch: 60,
            kind: crate::plugin::ExpressionKind::Tuning,
            value: 1.0,
        };
        assert_eq!(control(expression), None);
    }

    #[test]
    fn the_event_list_points_at_its_events_and_never_grows() {
        let mut list = EventList::new();
        for i in 0..EVENTS + 10 {
            list.push(midi(i as u32, [0x90, 60, 100]));
        }
        assert_eq!(list.len(), EVENTS);
        let last = unsafe { *list.list.events[EVENTS - 1] };
        assert_eq!(last.delta_frames, EVENTS as i32 - 1);
        list.clear();
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn switches_and_integer_ranges_have_steps() {
        let mut p: ParameterProperties = unsafe { std::mem::zeroed() };
        assert_eq!(steps(&p), 0);
        p.flags = PARAM_IS_SWITCH;
        assert_eq!(steps(&p), 1);
        p.flags = PARAM_USES_INTEGER_MIN_MAX;
        (p.min_integer, p.max_integer) = (0, 3);
        assert_eq!(steps(&p), 3);
        p.flags |= PARAM_USES_INT_STEP;
        (p.max_integer, p.step_integer) = (10, 2);
        assert_eq!(steps(&p), 5);
    }

    /// With a real plugin, which CI doesn't have; an effect hears an impulse, an instrument
    /// plays a note:
    /// `PINA_TEST_VST2=/path/DragonflyRoomReverb.vst cargo test -p pina-engine -- --ignored`
    #[test]
    #[ignore]
    fn a_real_plugin_plays_saves_restores_and_shuts_down() {
        let path = std::env::var("PINA_TEST_VST2").expect("set PINA_TEST_VST2");
        let found = inspect(Path::new(&path)).unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        let info = &found[0];
        println!("{info:?}");
        assert_eq!(info.format, Format::Vst2);
        let mut libraries = HashMap::new();
        let r = info.plugin_ref();
        let (plugin, mut processor) = instantiate(&mut libraries, &r, 48_000.0, 512, None).unwrap();
        assert_eq!(plugin.name(), info.class);

        // Sound, well after a burst of sine (a reverb's tail) or a note's start.
        let (mut l, mut rt) = (vec![0f32; 512], vec![0f32; 512]);
        let note = Note {
            offset: 0,
            pitch: 60,
            velocity: 1.0,
        };
        match info.instrument {
            true => processor.process([&mut l, &mut rt], 512, &[note], &[], None),
            false => {
                for (n, (a, b)) in l.iter_mut().zip(&mut rt).enumerate() {
                    *a = (n as f32 * 0.06).sin() * 0.5;
                    *b = *a;
                }
                processor.process([&mut l, &mut rt], 512, &[], &[], None);
            }
        }
        let mut peaks = Vec::new();
        for _ in 0..20 {
            l.fill(0.0);
            rt.fill(0.0);
            processor.process([&mut l, &mut rt], 512, &[], &[], None);
            assert!(l.iter().chain(&rt).all(|x| x.is_finite()));
            peaks.push(l.iter().chain(&rt).fold(0f32, |m, x| m.max(x.abs())));
        }
        println!("peaks: {peaks:?}");
        let later = peaks[4..12].iter().fold(0f32, |m, &x| m.max(x));
        assert!(later > 1e-3, "{later}");

        // Its parameters can be read, turned, shown and typed.
        let params = plugin.params();
        for p in &params {
            println!(
                "{} {:?} = {:.3} ({}) default {:.3} steps {} units {:?}",
                p.id,
                p.name,
                p.value,
                plugin.param_text(p.id, p.value),
                p.default,
                p.steps,
                p.units
            );
        }
        // A continuous one, in units other than percent if it has one.
        let continuous = || params.iter().filter(|p| p.steps == 0);
        let p = continuous()
            .find(|p| !p.units.is_empty() && p.units != "%")
            .or_else(|| continuous().next())
            .expect("a continuous parameter");
        let to = if p.value > 0.5 { 0.25 } else { 0.75 };
        plugin.set_param(p.id, to);
        assert!((plugin.param_value(p.id) - to).abs() < 1e-6);
        assert!(processor.pending());
        crate::plugin::settle(&mut processor, 512);
        assert!(!processor.pending());
        // Now the plugin has it, and shows it.
        let now = plugin.param_value(p.id);
        assert!((now - to).abs() < 0.02, "{now}");
        let shown = plugin.param_text(p.id, to);
        let other = plugin.param_text(p.id, 0.1);
        println!("{} at {to}: {shown:?}; at 0.1: {other:?}", p.name);
        assert!(!shown.is_empty());
        // Another value is shown as the plugin would show it, not as a percentage: unless the
        // plugin shows percentages, the two differ.
        if p.units != "%" {
            assert_ne!(other, "10%");
        }
        match plugin.param_from_text(p.id, &shown) {
            Some(v) => {
                println!("{shown:?} reads as {v}");
                assert!((v - to).abs() < 0.02, "{v}");
            }
            None => println!("it can't read typed values"),
        }
        // Typed values are read, by the plugin or by its display, without touching what plays.
        let typed = crate::plugin::value_from_text(&*plugin, p, &other).unwrap();
        println!("{other:?} reads as {typed}");
        assert!((typed - 0.1).abs() < 0.01, "{typed}");
        assert!((plugin.param_value(p.id) - to).abs() < 0.02);

        // Its settings come back in a new instance.
        let state = plugin.state().unwrap();
        let mut presets = plugin.presets();
        println!("presets: {presets:?}");
        drop(plugin);
        drop(processor);
        let (again, _processor) =
            instantiate(&mut libraries, &r, 48_000.0, 512, Some(&state)).unwrap();
        assert!((again.param_value(p.id) - to).abs() < 0.02);
        assert_eq!(again.state().unwrap(), state);

        // Programs load.
        presets.retain(|p| matches!(p.source, PresetSource::Program { .. }));
        if let Some(last) = presets.last() {
            again.load_preset(last).unwrap();
        }

        // A preset saved, the parameter moved, and the preset loaded puts it back.
        again.set_state(&state).unwrap();
        let file = again.save_preset("pina test preset").unwrap();
        again.set_param(p.id, 1.0 - to);
        let preset = Preset {
            name: "pina test preset".into(),
            source: PresetSource::File { file: file.clone() },
        };
        again.load_preset(&preset).unwrap();
        let back = again.param_value(p.id);
        let listed = again.presets().contains(&preset);
        std::fs::remove_file(&file).unwrap();
        // Its folders too, if the test made them.
        let _ = std::fs::remove_dir(file.parent().unwrap());
        let _ = std::fs::remove_dir(file.parent().unwrap().parent().unwrap());
        assert!((back - to).abs() < 0.02, "{back}");
        assert!(listed);
    }
}
