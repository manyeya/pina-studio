//! Hosting CLAP plugins (`.clap` bundles, macOS): load a bundle's entry, run a plugin's process
//! on the audio thread, and open its editor in a window of its own.
//!
//! A plugin is shared by its two halves (and an open editor): whichever goes last shuts it down.
//! Edits from the main thread reach it through its processor while that runs; when nothing is
//! running it (a render that hasn't started, `tempo params`) they're flushed to it at once.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::{CStr, CString, OsStr, c_char, c_void},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr,
    rc::Rc,
    slice,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicPtr, AtomicU8, AtomicU64, Ordering::SeqCst},
    },
    thread::{self, ThreadId},
    time::{Duration, Instant},
};

use clap_sys::{
    audio_buffer::clap_audio_buffer,
    entry::clap_plugin_entry,
    events::*,
    ext::{
        audio_ports::*, gui::*, latency::*, log::*, note_ports::*, params::*, preset_load::*,
        state::*, thread_check::*, timer_support::*,
    },
    factory::{plugin_factory::*, preset_discovery::*},
    fixedpoint::{CLAP_BEATTIME_FACTOR, CLAP_SECTIME_FACTOR},
    host::clap_host,
    id::clap_id,
    plugin::{clap_plugin, clap_plugin_descriptor},
    plugin_features::CLAP_PLUGIN_FEATURE_INSTRUMENT,
    process::{CLAP_PROCESS_ERROR, clap_process},
    stream::{clap_istream, clap_ostream},
    universal_plugin_id::clap_universal_plugin_id,
    version::{CLAP_VERSION, clap_version_is_compatible},
};
use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::NSWindow;
use objc2_foundation::NSSize;

use super::{
    AUTOMATED, Control, ControlKind, EditReceiver, EditSender, Editor, ExpressionKind, Note,
    Plugin, PluginApi, PluginParam, Preset, PresetSource, Processor, ProcessorApi, edit_queue,
    editor_window,
};
use crate::{
    project::{Format, PluginRef},
    scan::PluginInfo,
};

/// What tempo's own preset files for a CLAP plugin end in: its settings, as `state` gives them.
const EXTENSION: &str = "clap-preset";

/// Room for a block's notes and controls (as many as VST3's event list holds), its automation
/// and the edits waiting for it.
const EVENTS: usize = 512 + 2 * AUTOMATED + 64;

/// How recently a processor must have run to count as running, in ms.
const RUNNING_MS: u64 = 100;

const VERSION: &CStr =
    match CStr::from_bytes_with_nul(concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes()) {
        Ok(v) => v,
        Err(_) => c"0",
    };

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
    fn CFBundleGetDataPointerForName(bundle: *const c_void, name: *const c_void) -> *const c_void;
    fn CFStringCreateWithCString(
        a: *const c_void,
        s: *const c_char,
        encoding: u32,
    ) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

/// A loaded `.clap` bundle: its entry, initialised, and its plugin factory.
///
/// A bundle is opened and its entry initialised once per process, then kept: `deinit` is never
/// called. `Libraries` come and go while the plugins they loaded live on (a render drops its own
/// once its plugins are loaded), so deinit on drop would pull the library out from under them;
/// and CLAP wants one `init` per load, not one per `Libraries`.
#[derive(Clone, Copy)]
pub struct Library {
    entry: *const clap_plugin_entry,
    factory: *const clap_plugin_factory,
}

// The entry and factory are the bundle's, which is never unloaded, and CLAP makes them
// thread-safe.
unsafe impl Send for Library {}
unsafe impl Sync for Library {}

/// Every bundle opened in this process.
static OPENED: Mutex<Vec<(PathBuf, Library)>> = Mutex::new(Vec::new());

/// A plugin a bundle holds, as its descriptor says.
struct Descriptor {
    id: CString,
    name: String,
    vendor: String,
    instrument: bool,
}

impl Library {
    pub fn load(path: &Path) -> Result<Library, String> {
        let mut opened = OPENED.lock().unwrap();
        if let Some((_, library)) = opened.iter().find(|(p, _)| p == path) {
            return Ok(*library);
        }
        let library = Library::open(path)?;
        opened.push((path.to_path_buf(), library));
        Ok(library)
    }

    fn open(path: &Path) -> Result<Library, String> {
        let fail = |why: &str| format!("{}: {why}", path.display());
        const UTF8: u32 = 0x0800_0100;
        let p = CString::new(path.as_os_str().as_encoded_bytes()).map_err(|_| fail("bad path"))?;
        unsafe {
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
            let name = CFStringCreateWithCString(ptr::null(), c"clap_entry".as_ptr(), UTF8);
            let entry = CFBundleGetDataPointerForName(bundle, name).cast::<clap_plugin_entry>();
            CFRelease(name);
            let e = entry.as_ref().ok_or_else(|| fail("no clap_entry"))?;
            if !clap_version_is_compatible(e.clap_version) {
                let v = e.clap_version;
                return Err(fail(&format!(
                    "CLAP {}.{} isn't supported",
                    v.major, v.minor
                )));
            }
            if !e.init.is_some_and(|init| init(p.as_ptr())) {
                return Err(fail("its init failed"));
            }
            let library = Library {
                entry,
                factory: ptr::null(),
            };
            let factory = library.factory::<clap_plugin_factory>(&[CLAP_PLUGIN_FACTORY_ID]);
            if factory.is_null() {
                return Err(fail("no plugin factory"));
            }
            Ok(Library { factory, ..library })
        }
    }

    /// One of the bundle's factories, by the ids it may go by, or null.
    fn factory<T>(&self, ids: &[&CStr]) -> *const T {
        let Some(get) = (unsafe { self.entry.as_ref() }).and_then(|e| e.get_factory) else {
            return ptr::null();
        };
        ids.iter()
            .map(|id| unsafe { get(id.as_ptr()) }.cast::<T>())
            .find(|f| !f.is_null())
            .unwrap_or(ptr::null())
    }

    fn descriptors(&self) -> Vec<Descriptor> {
        let factory = unsafe { &*self.factory };
        let (Some(count), Some(get)) = (factory.get_plugin_count, factory.get_plugin_descriptor)
        else {
            return Vec::new();
        };
        unsafe {
            (0..count(self.factory))
                .filter_map(|i| {
                    let d: &clap_plugin_descriptor = get(self.factory, i).as_ref()?;
                    if d.id.is_null() || !clap_version_is_compatible(d.clap_version) {
                        return None;
                    }
                    Some(Descriptor {
                        id: CStr::from_ptr(d.id).to_owned(),
                        name: text(d.name),
                        vendor: text(d.vendor),
                        instrument: features(d.features).contains(&CLAP_PLUGIN_FEATURE_INSTRUMENT),
                    })
                })
                .collect()
        }
    }

    /// The presets the bundle's preset providers find for plugin `id`, in their order.
    fn discover(&self, id: &CStr) -> Vec<Found> {
        let factory = self.factory::<clap_preset_discovery_factory>(&[
            CLAP_PRESET_DISCOVERY_FACTORY_ID,
            CLAP_PRESET_DISCOVERY_FACTORY_ID_COMPAT,
        ]);
        let Some(f) = (unsafe { factory.as_ref() }) else {
            return Vec::new();
        };
        let (Some(count), Some(describe), Some(create)) = (f.count, f.get_descriptor, f.create)
        else {
            return Vec::new();
        };
        let indexer = Indexer::new();
        let receiver = Receiver::new();
        let mut found = Vec::new();
        unsafe {
            for i in 0..count(factory) {
                let Some(d) = describe(factory, i).as_ref() else {
                    continue;
                };
                let provider = create(factory, &indexer.clap, d.id);
                let Some(p) = provider.as_ref() else {
                    continue;
                };
                // It says where its presets are as it starts.
                let started = p.init.is_some_and(|init| init(provider));
                let extensions = indexer.extensions.take();
                let locations = indexer.locations.take();
                if started && let Some(metadata) = p.get_metadata {
                    // What the provider finds in one place, for this plugin.
                    let mut look = |kind, location: Option<&CString>| {
                        let at = location.map_or(ptr::null(), |l| l.as_ptr());
                        metadata(provider, kind, at, &receiver.clap);
                        for l in receiver.presets.take() {
                            if l.plugins.is_empty() || l.plugins.iter().any(|p| p.as_c_str() == id)
                            {
                                found.push(Found {
                                    name: l.name,
                                    kind,
                                    location: location.cloned(),
                                    key: l.key,
                                });
                            }
                        }
                    };
                    for (kind, location) in locations {
                        match location {
                            None => look(kind, None),
                            Some(place) => {
                                for file in preset_paths(&place, &extensions) {
                                    let Ok(file) = CString::new(file.as_os_str().as_bytes()) else {
                                        continue;
                                    };
                                    look(kind, Some(&file));
                                }
                            }
                        }
                    }
                }
                if let Some(destroy) = p.destroy {
                    destroy(provider);
                }
            }
        }
        found
    }
}

/// The plugins in a `.clap` bundle, opening it in this process.
pub fn inspect(bundle: &Path) -> Result<Vec<PluginInfo>, String> {
    let library = Library::load(bundle)?;
    Ok(library
        .descriptors()
        .into_iter()
        .map(|d| PluginInfo {
            format: Format::Clap,
            path: bundle.to_path_buf(),
            class: d.name,
            uid: Some(d.id.to_string_lossy().into_owned()),
            vendor: d.vendor,
            instrument: d.instrument,
        })
        .collect())
}

/// Loads the plugin `r` names (by its id, else its name), from its bundle (loaded once into
/// `libraries`).
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
    let library = libraries[&r.path];
    let descriptors = library.descriptors();
    let uid = r.uid.as_deref().map(str::as_bytes);
    let d = descriptors
        .iter()
        .find(|d| Some(d.id.to_bytes()) == uid)
        .or_else(|| descriptors.iter().find(|d| d.name == r.class))
        .ok_or_else(|| format!("{}: no plugin named {:?}", r.path.display(), r.class))?;
    let (plugin, processor) = ClapPlugin::instantiate(library, d, rate, max_block, state)?;
    Ok((Box::new(plugin), Box::new(processor)))
}

thread_local! {
    /// Whether this thread is in the plugin's processing side now: the audio thread in
    /// `process`, or another standing in for it when the audio thread isn't running it.
    static AUDIO: Cell<bool> = const { Cell::new(false) };
}

/// Runs `f` as the plugin's audio thread.
fn as_audio<R>(f: impl FnOnce() -> R) -> R {
    let was = AUDIO.replace(true);
    let r = f();
    AUDIO.set(was);
    r
}

/// Milliseconds since this process first asked, never 0.
fn now() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

// Who's in a plugin's processing side.
const IDLE: u8 = 0;
const PROCESSING: u8 = 1;
const FLUSHING: u8 = 2;

/// One plugin and the host tempo is to it: what its halves (and its editor) share, the last of
/// them to go shutting it down. `host` comes first, so the `clap_host` the plugin calls back
/// with points at the whole.
#[repr(C)]
struct Instance {
    host: clap_host,
    plugin: AtomicPtr<clap_plugin>,
    name: String,
    /// The thread it was made on, its main thread.
    main: ThreadId,
    active: AtomicBool,
    processing: AtomicBool,
    /// Its editor is open: created, and not yet destroyed.
    gui: AtomicBool,
    /// Who's in its processing side: nobody, the audio thread, or the main thread flushing.
    busy: AtomicU8,
    /// When it last processed (see `now`), 0 for never.
    processed: AtomicU64,
    /// How many edits its processor has applied.
    applied: AtomicU64,
    // What it's asked for: its main thread called, its parameters flushed, their info re-read.
    callback: AtomicBool,
    flush: AtomicBool,
    rescan: AtomicBool,
    /// A size its editor asked for (width << 32 | height), 0 for none.
    resize: AtomicU64,
    window: Mutex<Option<EditorWindow>>,
    timers: Mutex<Vec<Timer>>,
}

/// An open editor's window, kept for the plugin's requests to resize or show it. Used on the
/// main thread only.
struct EditorWindow(Retained<NSWindow>);

unsafe impl Send for EditorWindow {}

struct Timer {
    id: clap_id,
    period: Duration,
    due: Instant,
}

impl Instance {
    fn new(name: &str) -> Arc<Instance> {
        Arc::new(Instance {
            host: clap_host {
                clap_version: CLAP_VERSION,
                host_data: ptr::null_mut(),
                name: c"tempo".as_ptr(),
                vendor: c"tempo".as_ptr(),
                url: c"".as_ptr(),
                version: VERSION.as_ptr(),
                get_extension: Some(host_extension),
                request_restart: Some(request_restart),
                request_process: Some(request_process),
                request_callback: Some(request_callback),
            },
            plugin: AtomicPtr::new(ptr::null_mut()),
            name: name.to_string(),
            main: thread::current().id(),
            active: AtomicBool::new(false),
            processing: AtomicBool::new(false),
            gui: AtomicBool::new(false),
            busy: AtomicU8::new(IDLE),
            processed: AtomicU64::new(0),
            applied: AtomicU64::new(0),
            callback: AtomicBool::new(false),
            flush: AtomicBool::new(false),
            rescan: AtomicBool::new(false),
            resize: AtomicU64::new(0),
            window: Mutex::new(None),
            timers: Mutex::new(Vec::new()),
        })
    }

    /// Whether its processor is being run: it processed a moment ago.
    fn running(&self) -> bool {
        let at = self.processed.load(SeqCst);
        at != 0 && now() - at < RUNNING_MS
    }

    /// Takes its processing side for `who`, if nobody has it.
    fn enter(&self, who: u8) -> bool {
        self.busy
            .compare_exchange(IDLE, who, SeqCst, SeqCst)
            .is_ok()
    }

    fn leave(&self) {
        self.busy.store(IDLE, SeqCst);
    }

    /// Its editor's window, on the main thread.
    fn window(&self) -> Option<Retained<NSWindow>> {
        MainThreadMarker::new()?;
        let window = self.window.lock().unwrap();
        window.as_ref().map(|w| w.0.clone())
    }

    /// Gives its editor's window the size it asked for. Main thread only.
    fn resize_window(&self) {
        let Some(window) = self.window() else {
            return;
        };
        let size = self.resize.swap(0, SeqCst);
        if size != 0 {
            let (w, h) = ((size >> 32) as f64, (size & 0xffff_ffff) as f64);
            window.setContentSize(NSSize::new(w, h));
        }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        let plugin = *self.plugin.get_mut();
        let Some(p) = (unsafe { plugin.as_ref() }) else {
            return;
        };
        unsafe {
            if *self.gui.get_mut()
                && let Some(destroy) =
                    extension::<clap_plugin_gui>(plugin, &[CLAP_EXT_GUI]).and_then(|g| g.destroy)
            {
                destroy(plugin);
            }
            // The audio thread is done with it, so this thread stops its processing instead.
            if *self.processing.get_mut()
                && let Some(stop) = p.stop_processing
            {
                as_audio(|| stop(plugin));
            }
            if *self.active.get_mut()
                && let Some(deactivate) = p.deactivate
            {
                deactivate(plugin);
            }
            if let Some(destroy) = p.destroy {
                destroy(plugin);
            }
        }
    }
}

/// The plugin's extension `id` (by the first of the ids it may go by), if it has it. It lasts as
/// long as the plugin, which outlives whoever holds its `Instance`.
unsafe fn extension<T>(plugin: *const clap_plugin, ids: &[&CStr]) -> Option<&'static T> {
    let get = unsafe { (*plugin).get_extension? };
    ids.iter()
        .find_map(|id| unsafe { get(plugin, id.as_ptr()).cast::<T>().as_ref() })
}

/// A parameter's range, to turn values from 0 to 1 into the plugin's own and back.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Range {
    id: u32,
    min: f64,
    max: f64,
    stepped: bool,
}

impl Range {
    fn of(info: &clap_param_info) -> Range {
        Range {
            id: info.id,
            min: info.min_value,
            max: info.max_value,
            stepped: info.flags & CLAP_PARAM_IS_STEPPED != 0,
        }
    }

    /// The plugin's value for `v`, 0 to 1; a stepped parameter's is a whole number.
    fn plain(self, v: f64) -> f64 {
        let x = self.min + v.clamp(0.0, 1.0) * (self.max - self.min);
        if self.stepped { x.round() } else { x }
    }

    /// The plugin's value `x`, 0 to 1.
    fn normal(self, x: f64) -> f64 {
        if self.max > self.min {
            ((x - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn steps(self) -> u32 {
        match self.stepped {
            true => (self.max - self.min).round().max(0.0) as u32,
            false => 0,
        }
    }
}

/// Every parameter the plugin has, as it describes them.
fn infos(plugin: *const clap_plugin, params: Option<&clap_plugin_params>) -> Vec<clap_param_info> {
    let Some((count, get)) = params.and_then(|p| Some((p.count?, p.get_info?))) else {
        return Vec::new();
    };
    unsafe {
        (0..count(plugin))
            .filter_map(|i| {
                let mut info: clap_param_info = std::mem::zeroed();
                get(plugin, i, &mut info).then_some(info)
            })
            .collect()
    }
}

/// Their ranges, by id.
fn ranges(infos: &[clap_param_info]) -> Vec<Range> {
    let mut ranges: Vec<Range> = infos.iter().map(Range::of).collect();
    ranges.sort_by_key(|r| r.id);
    ranges
}

fn range_of(ranges: &[Range], id: u32) -> Option<Range> {
    let i = ranges.binary_search_by_key(&id, |r| r.id).ok()?;
    Some(ranges[i])
}

/// The parameters a person can turn.
fn shown(flags: clap_param_info_flags) -> bool {
    flags & CLAP_PARAM_IS_AUTOMATABLE != 0
        && flags & (CLAP_PARAM_IS_HIDDEN | CLAP_PARAM_IS_READONLY | CLAP_PARAM_IS_BYPASS) == 0
}

/// A CLAP plugin's main-thread side: its parameters, settings, presets and editor.
pub struct ClapPlugin {
    pub name: String,
    vendor: String,
    id: CString,
    library: Library,
    inst: Arc<Instance>,
    plugin: *const clap_plugin,
    params: Option<&'static clap_plugin_params>,
    state: Option<&'static clap_plugin_state>,
    gui: Option<&'static clap_plugin_gui>,
    preset_load: Option<&'static clap_plugin_preset_load>,
    timer: Option<&'static clap_plugin_timer_support>,
    edits: EditSender,
    /// Edits sent to its processor, and those it hasn't applied yet: a value, and its place
    /// among the edits sent. What's set reads back at once, as a VST3 controller's does.
    sent: Cell<u64>,
    waiting: RefCell<HashMap<u32, (f64, u64)>>,
    ranges: RefCell<Vec<Range>>,
    /// What its preset providers found, asked once.
    // ponytail: never asked again; rediscover when presets are saved from its own editor.
    found: RefCell<Option<Rc<Vec<Found>>>>,
    /// Whether it has a second input, for a sidechain.
    sidechain: bool,
}

/// A CLAP plugin's audio-thread side.
pub struct ClapProcessor {
    inst: Arc<Instance>,
    plugin: *const clap_plugin,
    edits: EditReceiver,
    /// As the plugin first described its parameters.
    // ponytail: not updated when it rescans them; send new ranges if a plugin changes its own.
    ranges: Vec<Range>,
    /// Automation for the next block: a parameter, and its value at the block's start and end.
    automation: [(u32, f64, f64); AUTOMATED],
    automated: usize,
    events: Vec<Event>,
    /// Its notes are MIDI, not CLAP's own note events (and it has no note expression).
    midi_notes: bool,
    // Channel buffers, their pointers, and the port structs that point at those.
    _buffers: Vec<Vec<f32>>,
    _channels: Vec<Vec<*mut f32>>,
    inputs: Vec<clap_audio_buffer>,
    outputs: Vec<clap_audio_buffer>,
    transport: clap_event_transport,
    rate: f64,
    playing: bool,
    position: u64,
    tempo: f64,
    steady: i64,
    max_block: usize,
    latency: u32,
    /// Whether it has a second input, for a sidechain.
    sidechain: bool,
}

// The plugin's pointers are only used from the audio thread once the processor moves there.
unsafe impl Send for ClapProcessor {}

impl ClapPlugin {
    /// Creates a plugin at `rate`, restoring `state` (from `PluginApi::state`) if given.
    fn instantiate(
        library: Library,
        d: &Descriptor,
        rate: f64,
        max_block: usize,
        state: Option<&[u8]>,
    ) -> Result<(ClapPlugin, ClapProcessor), String> {
        let fail = |why: &str| format!("{}: {why}", d.name);
        let inst = Instance::new(&d.name);
        unsafe {
            let factory = &*library.factory;
            let create = factory
                .create_plugin
                .ok_or_else(|| fail("can't be created"))?;
            let plugin = create(library.factory, &inst.host, d.id.as_ptr());
            if plugin.is_null() {
                return Err(fail("couldn't create it"));
            }
            // From here, `inst` destroys it when it goes.
            inst.plugin.store(plugin.cast_mut(), SeqCst);
            let p = &*plugin;
            if !p.init.is_some_and(|init| init(plugin)) {
                return Err(fail("init failed"));
            }
            let params = extension::<clap_plugin_params>(plugin, &[CLAP_EXT_PARAMS]);
            let state_ext = extension::<clap_plugin_state>(plugin, &[CLAP_EXT_STATE]);
            if let (Some(bytes), Some(s)) = (state, state_ext) {
                load_state(plugin, s, bytes);
            }

            // Its audio ports, by their channels: the first input is the main one (if it has
            // one), the second a sidechain; the first output is the main one.
            let ports = extension::<clap_plugin_audio_ports>(plugin, &[CLAP_EXT_AUDIO_PORTS]);
            let channels = |input: bool| -> Vec<u32> {
                let Some((count, get)) = ports.and_then(|p| Some((p.count?, p.get?))) else {
                    return Vec::new();
                };
                (0..count(plugin, input))
                    .map(|i| {
                        let mut info: clap_audio_port_info = std::mem::zeroed();
                        match get(plugin, i, input, &mut info) {
                            true => info.channel_count,
                            false => 0,
                        }
                    })
                    .collect()
            };
            let (ins, outs) = (channels(true), channels(false));
            if outs.first().is_none_or(|&c| c == 0) {
                return Err(fail("no audio output"));
            }
            // CLAP's own note events if it takes them, else MIDI.
            let notes = extension::<clap_plugin_note_ports>(plugin, &[CLAP_EXT_NOTE_PORTS]);
            let dialects = notes
                .and_then(|n| {
                    let (count, get) = (n.count?, n.get?);
                    let mut info: clap_note_port_info = std::mem::zeroed();
                    (count(plugin, true) > 0 && get(plugin, 0, true, &mut info))
                        .then_some(info.supported_dialects)
                })
                .unwrap_or(CLAP_NOTE_DIALECT_CLAP);
            let midi_notes =
                dialects & CLAP_NOTE_DIALECT_CLAP == 0 && dialects & CLAP_NOTE_DIALECT_MIDI != 0;

            if !p
                .activate
                .is_some_and(|a| a(plugin, rate, 1, max_block as u32))
            {
                return Err(fail("activate failed"));
            }
            inst.active.store(true, SeqCst);
            // Asked now: it's for the main thread, while active.
            let latency = extension::<clap_plugin_latency>(plugin, &[CLAP_EXT_LATENCY])
                .and_then(|l| l.get)
                .map_or(0, |get| get(plugin));

            // Stepped parameters (switches, types, modes) end a block's edits: see EditReceiver.
            let infos = infos(plugin, params);
            let stepped = infos
                .iter()
                .filter(|i| i.flags & CLAP_PARAM_IS_STEPPED != 0)
                .map(|i| i.id)
                .collect();
            let (edits_tx, edits) = edit_queue(stepped);
            let ranges = ranges(&infos);

            // Every port gets buffers: some plugins write to all of them.
            let mut buffers = Vec::new();
            let mut pointers = Vec::new();
            let mut port = |n: u32| {
                let mut ptrs: Vec<*mut f32> = (0..n)
                    .map(|_| {
                        buffers.push(vec![0f32; max_block]);
                        buffers.last_mut().unwrap().as_mut_ptr()
                    })
                    .collect();
                let b = clap_audio_buffer {
                    data32: ptrs.as_mut_ptr(),
                    data64: ptr::null_mut(),
                    channel_count: n,
                    latency: 0,
                    constant_mask: 0,
                };
                pointers.push(ptrs);
                b
            };
            let inputs: Vec<_> = ins.iter().map(|&n| port(n)).collect();
            let outputs: Vec<_> = outs.iter().map(|&n| port(n)).collect();

            let processor = ClapProcessor {
                inst: inst.clone(),
                plugin,
                edits,
                ranges: ranges.clone(),
                automation: [(0, 0.0, 0.0); AUTOMATED],
                automated: 0,
                events: Vec::with_capacity(EVENTS),
                midi_notes,
                _buffers: buffers,
                _channels: pointers,
                inputs,
                outputs,
                transport: transport(false, 0, 120.0, rate),
                rate,
                playing: false,
                position: 0,
                tempo: 120.0,
                steady: 0,
                max_block,
                latency,
                sidechain: ins.len() > 1,
            };
            let plugin = ClapPlugin {
                name: d.name.clone(),
                vendor: d.vendor.clone(),
                id: d.id.clone(),
                library,
                inst,
                plugin,
                params,
                state: state_ext,
                gui: extension(plugin, &[CLAP_EXT_GUI]),
                preset_load: extension(
                    plugin,
                    &[CLAP_EXT_PRESET_LOAD, CLAP_EXT_PRESET_LOAD_COMPAT],
                ),
                timer: extension(plugin, &[CLAP_EXT_TIMER_SUPPORT]),
                edits: edits_tx,
                sent: Cell::new(0),
                waiting: RefCell::new(HashMap::new()),
                ranges: RefCell::new(ranges),
                found: RefCell::new(None),
                sidechain: ins.len() > 1,
            };
            // Whatever it asked for while loading.
            plugin.idle();
            Ok((plugin, processor))
        }
    }

    /// A parameter's range, as the plugin last described it.
    fn range(&self, id: u32) -> Option<Range> {
        if self.inst.rescan.swap(false, SeqCst) {
            *self.ranges.borrow_mut() = ranges(&infos(self.plugin, self.params));
        }
        range_of(&self.ranges.borrow(), id)
    }

    /// Hands parameter changes (0 to 1) straight to the plugin when its processor isn't running
    /// (nothing plays it yet, as before a render), so they take at once and read back. False
    /// when it's running: they go through the processor then.
    fn flush(&self, changes: &[(u32, f64)]) -> bool {
        let Some(flush) = self.params.and_then(|p| p.flush) else {
            return false;
        };
        if self.inst.running() || !self.inst.enter(FLUSHING) {
            return false;
        }
        let events: Vec<Event> = changes
            .iter()
            .filter_map(|&(id, v)| Some(param_event(0, id, self.range(id)?.plain(v))))
            .collect();
        // While active, a flush is for its audio thread, which isn't running it.
        as_audio(|| unsafe { flush(self.plugin, &input_events(&events), &output_events()) });
        self.inst.leave();
        true
    }

    /// Loads one of the presets its providers found, or a file of its own format.
    fn load_from(
        &self,
        kind: clap_preset_discovery_location_kind,
        location: Option<&CStr>,
        key: Option<&CStr>,
    ) -> Result<(), String> {
        let from = self
            .preset_load
            .and_then(|p| p.from_location)
            .ok_or_else(|| format!("{}: it can't load presets", self.name))?;
        let at = |s: Option<&CStr>| s.map_or(ptr::null(), CStr::as_ptr);
        let ok = unsafe { from(self.plugin, kind, at(location), at(key)) };
        self.idle();
        match ok {
            true => Ok(()),
            false => Err(format!("{}: it couldn't load that preset", self.name)),
        }
    }

    fn found(&self) -> Rc<Vec<Found>> {
        let mut found = self.found.borrow_mut();
        found
            .get_or_insert_with(|| Rc::new(self.library.discover(&self.id)))
            .clone()
    }
}

impl PluginApi for ClapPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn params(&self) -> Vec<PluginParam> {
        let infos = infos(self.plugin, self.params);
        self.inst.rescan.store(false, SeqCst);
        *self.ranges.borrow_mut() = ranges(&infos);
        infos
            .iter()
            .filter(|i| shown(i.flags))
            .map(|i| {
                let range = Range::of(i);
                PluginParam {
                    id: i.id,
                    name: text_in(&i.name),
                    value: self.param_value(i.id),
                    default: range.normal(i.default_value),
                    steps: range.steps(),
                    units: String::new(),
                }
            })
            .collect()
    }

    fn set_param(&self, id: u32, value: f64) {
        let value = value.clamp(0.0, 1.0);
        if !self.flush(&[(id, value)]) {
            self.edits.push(id, value);
            self.sent.set(self.sent.get() + 1);
            self.waiting
                .borrow_mut()
                .insert(id, (value, self.sent.get()));
        }
    }

    fn show_param(&self, _id: u32, _value: f64) {
        // A CLAP plugin's controls and editor are its processor's: automation shows itself.
    }

    fn param_value(&self, id: u32) -> f64 {
        let mut waiting = self.waiting.borrow_mut();
        if let Some(&(value, nth)) = waiting.get(&id) {
            if self.inst.applied.load(SeqCst) < nth {
                return value;
            }
            waiting.remove(&id);
        }
        drop(waiting);
        let (Some(range), Some(get)) = (self.range(id), self.params.and_then(|p| p.get_value))
        else {
            return 0.0;
        };
        let mut v = 0.0;
        match unsafe { get(self.plugin, id, &mut v) } {
            true => range.normal(v),
            false => 0.0,
        }
    }

    fn param_text(&self, id: u32, value: f64) -> String {
        let shown = (self.range(id), self.params.and_then(|p| p.value_to_text));
        if let (Some(range), Some(to_text)) = shown {
            let mut s = [0 as c_char; 256];
            let plain = range.plain(value);
            if unsafe { to_text(self.plugin, id, plain, s.as_mut_ptr(), s.len() as u32) } {
                return text_in(&s);
            }
        }
        format!("{:.0}%", value * 100.0)
    }

    fn param_from_text(&self, id: u32, text: &str) -> Option<f64> {
        let range = self.range(id)?;
        let from_text = self.params?.text_to_value?;
        let text = CString::new(text).ok()?;
        let mut v = 0.0;
        unsafe { from_text(self.plugin, id, text.as_ptr(), &mut v) }.then(|| range.normal(v))
    }

    fn state(&self) -> Option<Vec<u8>> {
        save_state(self.plugin, self.state?)
    }

    fn set_state(&self, state: &[u8]) -> Result<(), String> {
        let ext = self
            .state
            .ok_or_else(|| format!("{}: it has no settings to set", self.name))?;
        let ok = load_state(self.plugin, ext, state);
        self.idle();
        match ok {
            true => Ok(()),
            false => Err(format!("{}: it didn't take the settings", self.name)),
        }
    }

    fn presets(&self) -> Vec<Preset> {
        // Those its providers found: files of its own as files, the rest (built in, or one of
        // several in a file) as programs, numbered in order.
        let mut program = 0;
        let mut out: Vec<Preset> = self
            .found()
            .iter()
            .map(|f| {
                let source = f.file().map_or_else(
                    || {
                        program += 1;
                        PresetSource::Program {
                            list: 0,
                            program: program - 1,
                        }
                    },
                    |file| PresetSource::File { file },
                );
                Preset {
                    name: f.name.clone(),
                    source,
                }
            })
            .collect();
        let dirs = super::preset_dirs(&self.vendor, &self.name);
        out.extend(super::preset_files(&dirs, EXTENSION));
        out
    }

    fn load_preset(&self, preset: &Preset) -> Result<(), String> {
        match &preset.source {
            PresetSource::File { file } if file.extension() == Some(OsStr::new(EXTENSION)) => {
                let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
                self.set_state(&bytes)
            }
            PresetSource::File { file } => {
                let path = CString::new(file.as_os_str().as_bytes())
                    .map_err(|_| format!("{}: bad path", file.display()))?;
                self.load_from(CLAP_PRESET_DISCOVERY_LOCATION_FILE, Some(&path), None)
            }
            PresetSource::Program { program, .. } => {
                let found = self.found();
                let f = found
                    .iter()
                    .filter(|f| f.file().is_none())
                    .nth(*program as usize)
                    .ok_or_else(|| format!("{}: no preset {program}", self.name))?;
                self.load_from(f.kind, f.location.as_deref(), f.key.as_deref())
            }
        }
    }

    fn save_preset(&self, name: &str) -> Result<PathBuf, String> {
        let bytes = self
            .state()
            .ok_or("the plugin wouldn't give its settings")?;
        let dir = super::preset_dirs(&self.vendor, &self.name).remove(0);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(super::preset_file_name(name, EXTENSION));
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }

    fn open_editor(&self, mtm: MainThreadMarker) -> Result<Editor, String> {
        let fail = |why: &str| format!("{}: {why}", self.name);
        let gui = self.gui.ok_or_else(|| fail("no editor"))?;
        let api = CLAP_WINDOW_API_COCOA.as_ptr();
        let plugin = self.plugin;
        unsafe {
            if !gui.is_api_supported.is_some_and(|f| f(plugin, api, false)) {
                return Err(fail("editor isn't an NSView"));
            }
            if self.inst.gui.load(SeqCst) {
                return Err(fail("its editor is already open"));
            }
            let (Some(create), Some(destroy), Some(get_size), Some(set_parent)) =
                (gui.create, gui.destroy, gui.get_size, gui.set_parent)
            else {
                return Err(fail("no editor"));
            };
            if !create(plugin, api, false) {
                return Err(fail("its editor wouldn't open"));
            }
            self.inst.gui.store(true, SeqCst);
            let closing = |why: &str| {
                destroy(plugin);
                self.inst.gui.store(false, SeqCst);
                fail(why)
            };
            // Cocoa sizes are in points, so there's no scale to set.
            let (mut w, mut h) = (0u32, 0u32);
            if !get_size(plugin, &mut w, &mut h) {
                return Err(closing("editor has no size"));
            }
            let window = editor_window(mtm, &self.name, (w as f64, h as f64));
            let Some(content) = window.contentView() else {
                return Err(closing("window has no content view"));
            };
            let parent = clap_window {
                api,
                specific: clap_window_handle {
                    cocoa: Retained::as_ptr(&content) as *mut c_void,
                },
            };
            if !set_parent(plugin, &parent) {
                return Err(closing("editor wouldn't attach"));
            }
            if let Some(show) = gui.show {
                show(plugin);
            }
            *self.inst.window.lock().unwrap() = Some(EditorWindow(window.clone()));
            window.makeKeyAndOrderFront(None);
            let inst = self.inst.clone();
            Ok(Editor::new(window, move || {
                inst.window.lock().unwrap().take();
                if inst.gui.swap(false, SeqCst) {
                    if let Some(hide) = gui.hide {
                        hide(plugin);
                    }
                    destroy(plugin);
                }
            }))
        }
    }

    fn sidechain(&self) -> bool {
        self.sidechain
    }

    fn idle(&self) {
        let inst = &self.inst;
        if inst.callback.swap(false, SeqCst)
            && let Some(f) = unsafe { (*self.plugin).on_main_thread }
        {
            unsafe { f(self.plugin) };
        }
        // A flush it asked for: its processor does it when running, else it's done here.
        if inst.flush.load(SeqCst) && self.flush(&[]) {
            inst.flush.store(false, SeqCst);
        }
        inst.resize_window();
        if let Some(on_timer) = self.timer.and_then(|t| t.on_timer) {
            let now = Instant::now();
            let due: Vec<clap_id> = inst
                .timers
                .lock()
                .unwrap()
                .iter_mut()
                .filter(|t| t.due <= now)
                .map(|t| {
                    t.due = now + t.period;
                    t.id
                })
                .collect();
            for id in due {
                unsafe { on_timer(self.plugin, id) };
            }
        }
    }
}

/// Its settings, from its state extension.
fn save_state(plugin: *const clap_plugin, state: &clap_plugin_state) -> Option<Vec<u8>> {
    let save = state.save?;
    let mut bytes: Vec<u8> = Vec::new();
    let stream = clap_ostream {
        ctx: ptr::from_mut(&mut bytes).cast(),
        write: Some(write_stream),
    };
    unsafe { save(plugin, &stream) }.then_some(bytes)
}

fn load_state(plugin: *const clap_plugin, state: &clap_plugin_state, bytes: &[u8]) -> bool {
    let Some(load) = state.load else {
        return false;
    };
    let mut rest = bytes;
    let stream = clap_istream {
        ctx: ptr::from_mut(&mut rest).cast(),
        read: Some(read_stream),
    };
    unsafe { load(plugin, &stream) }
}

unsafe extern "C" fn write_stream(
    stream: *const clap_ostream,
    buffer: *const c_void,
    size: u64,
) -> i64 {
    let bytes = unsafe { &mut *(*stream).ctx.cast::<Vec<u8>>() };
    if size > 0 {
        bytes.extend_from_slice(unsafe { slice::from_raw_parts(buffer.cast(), size as usize) });
    }
    size as i64
}

unsafe extern "C" fn read_stream(
    stream: *const clap_istream,
    buffer: *mut c_void,
    size: u64,
) -> i64 {
    let rest = unsafe { &mut *(*stream).ctx.cast::<&[u8]>() };
    let n = (size as usize).min(rest.len());
    unsafe { ptr::copy_nonoverlapping(rest.as_ptr(), buffer.cast::<u8>(), n) };
    *rest = &rest[n..];
    n as i64
}

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

/// One event for the plugin, of whichever kind; each starts with its header.
#[repr(C)]
#[derive(Clone, Copy)]
union Event {
    header: clap_event_header,
    note: clap_event_note,
    expression: clap_event_note_expression,
    param: clap_event_param_value,
    midi: clap_event_midi,
}

fn header<T>(time: u32, kind: clap_event_type) -> clap_event_header {
    clap_event_header {
        size: size_of::<T>() as u32,
        time,
        space_id: CLAP_CORE_EVENT_SPACE_ID,
        type_: kind,
        flags: 0,
    }
}

/// A parameter set to `value`, the plugin's own, for every note.
fn param_event(time: u32, id: u32, value: f64) -> Event {
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

fn midi_event(time: u32, data: [u8; 3]) -> Event {
    Event {
        midi: clap_event_midi {
            header: header::<clap_event_midi>(time, CLAP_EVENT_MIDI),
            port_index: 0,
            data,
        },
    }
}

/// A note on or off, its id its pitch; as MIDI for a plugin that only takes that.
fn note_event(n: &Note, time: u32, midi: bool) -> Event {
    let on = n.velocity > 0.0;
    if midi {
        return match on {
            true => midi_event(time, [0x90, n.pitch & 0x7f, seven(n.velocity).max(1)]),
            false => midi_event(time, [0x80, n.pitch & 0x7f, 0]),
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
            velocity: n.velocity.clamp(0.0, 1.0) as f64,
        },
    }
}

fn expression_event(time: u32, pitch: u8, id: clap_note_expression, value: f64) -> Event {
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
fn control_event(c: &Control, time: u32, midi: bool) -> Option<Event> {
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
fn seven(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 127.0).round() as u8
}

/// A bend, -1 to 1, as MIDI's 14 bits (8192 is none).
fn bend(v: f32) -> u16 {
    ((v.clamp(-1.0, 1.0) + 1.0) / 2.0 * 16383.0).round() as u16
}

/// A note expression's CLAP id, and its value as CLAP has it.
fn expression(kind: ExpressionKind, value: f64) -> (clap_note_expression, f64) {
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
fn transport(playing: bool, position: u64, tempo: f64, rate: f64) -> clap_event_transport {
    let seconds = position as f64 / rate;
    let beats = seconds * tempo / 60.0;
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
fn input_events(events: &Vec<Event>) -> clap_input_events {
    clap_input_events {
        ctx: ptr::from_ref(events).cast_mut().cast(),
        size: Some(events_size),
        get: Some(events_get),
    }
}

unsafe extern "C" fn events_size(list: *const clap_input_events) -> u32 {
    let events = unsafe { &*(*list).ctx.cast::<Vec<Event>>() };
    events.len() as u32
}

unsafe extern "C" fn events_get(
    list: *const clap_input_events,
    i: u32,
) -> *const clap_event_header {
    let events = unsafe { &*(*list).ctx.cast::<Vec<Event>>() };
    events
        .get(i as usize)
        .map_or(ptr::null(), |e| ptr::from_ref(e).cast())
}

/// Where the plugin reports its own changes (its editor's edits, MIDI it makes), as a host
/// must give it.
// ponytail: dropped; read them when something records plugin edits or MIDI out.
fn output_events() -> clap_output_events {
    clap_output_events {
        ctx: ptr::null_mut(),
        try_push: Some(drop_event),
    }
}

unsafe extern "C" fn drop_event(_: *const clap_output_events, _: *const clap_event_header) -> bool {
    true
}

/// A C string from the plugin, or "" for none.
fn text(s: *const c_char) -> String {
    match s.is_null() {
        true => String::new(),
        false => unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned(),
    }
}

/// A C string in a buffer.
fn text_in(chars: &[c_char]) -> String {
    let bytes: Vec<u8> = chars
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// A descriptor's features, up to the null that ends them.
unsafe fn features<'a>(mut list: *const *const c_char) -> Vec<&'a CStr> {
    let mut out = Vec::new();
    while !list.is_null() && unsafe { !(*list).is_null() } {
        out.push(unsafe { CStr::from_ptr(*list) });
        list = unsafe { list.add(1) };
    }
    out
}

// The host's side: what a plugin can ask of tempo.

unsafe fn instance<'a>(host: *const clap_host) -> &'a Instance {
    unsafe { &*host.cast::<Instance>() }
}

static THREAD_CHECK: clap_host_thread_check = clap_host_thread_check {
    is_main_thread: Some(is_main_thread),
    is_audio_thread: Some(is_audio_thread),
};
static LOG: clap_host_log = clap_host_log { log: Some(log) };
static PARAMS: clap_host_params = clap_host_params {
    rescan: Some(params_rescan),
    clear: Some(params_clear),
    request_flush: Some(request_flush),
};
static STATE: clap_host_state = clap_host_state {
    mark_dirty: Some(mark_dirty),
};
static LATENCY: clap_host_latency = clap_host_latency {
    changed: Some(latency_changed),
};
static GUI: clap_host_gui = clap_host_gui {
    resize_hints_changed: Some(resize_hints_changed),
    request_resize: Some(request_resize),
    request_show: Some(request_show),
    request_hide: Some(request_hide),
    closed: Some(gui_closed),
};
static AUDIO_PORTS: clap_host_audio_ports = clap_host_audio_ports {
    is_rescan_flag_supported: Some(audio_ports_flag),
    rescan: Some(audio_ports_rescan),
};
static NOTE_PORTS: clap_host_note_ports = clap_host_note_ports {
    supported_dialects: Some(note_dialects),
    rescan: Some(note_ports_rescan),
};
static TIMER_SUPPORT: clap_host_timer_support = clap_host_timer_support {
    register_timer: Some(register_timer),
    unregister_timer: Some(unregister_timer),
};
static PRESET_LOAD: clap_host_preset_load = clap_host_preset_load {
    on_error: Some(preset_error),
    loaded: Some(preset_loaded),
};

unsafe extern "C" fn host_extension(_host: *const clap_host, id: *const c_char) -> *const c_void {
    if id.is_null() {
        return ptr::null();
    }
    let id = unsafe { CStr::from_ptr(id) };
    let ext: *const c_void = match id {
        id if id == CLAP_EXT_THREAD_CHECK => ptr::from_ref(&THREAD_CHECK).cast(),
        id if id == CLAP_EXT_LOG => ptr::from_ref(&LOG).cast(),
        id if id == CLAP_EXT_PARAMS => ptr::from_ref(&PARAMS).cast(),
        id if id == CLAP_EXT_STATE => ptr::from_ref(&STATE).cast(),
        id if id == CLAP_EXT_LATENCY => ptr::from_ref(&LATENCY).cast(),
        id if id == CLAP_EXT_GUI => ptr::from_ref(&GUI).cast(),
        id if id == CLAP_EXT_AUDIO_PORTS => ptr::from_ref(&AUDIO_PORTS).cast(),
        id if id == CLAP_EXT_NOTE_PORTS => ptr::from_ref(&NOTE_PORTS).cast(),
        id if id == CLAP_EXT_TIMER_SUPPORT => ptr::from_ref(&TIMER_SUPPORT).cast(),
        id if id == CLAP_EXT_PRESET_LOAD || id == CLAP_EXT_PRESET_LOAD_COMPAT => {
            ptr::from_ref(&PRESET_LOAD).cast()
        }
        _ => ptr::null(),
    };
    ext
}

unsafe extern "C" fn request_restart(_host: *const clap_host) {
    // ponytail: ignored; deactivate and reactivate it when a plugin changes its ports or latency.
}

unsafe extern "C" fn request_process(_host: *const clap_host) {
    // tempo runs every plugin all the time.
}

unsafe extern "C" fn request_callback(host: *const clap_host) {
    unsafe { instance(host) }.callback.store(true, SeqCst);
}

unsafe extern "C" fn is_main_thread(host: *const clap_host) -> bool {
    !AUDIO.get() && thread::current().id() == unsafe { instance(host) }.main
}

unsafe extern "C" fn is_audio_thread(_host: *const clap_host) -> bool {
    AUDIO.get()
}

unsafe extern "C" fn log(host: *const clap_host, severity: clap_log_severity, msg: *const c_char) {
    // ponytail: errors to stderr, the rest dropped; show them in the app if they help someone.
    if severity >= CLAP_LOG_ERROR {
        eprintln!("{}: {}", unsafe { instance(host) }.name, text(msg));
    }
}

unsafe extern "C" fn params_rescan(host: *const clap_host, flags: clap_param_rescan_flags) {
    if flags & (CLAP_PARAM_RESCAN_INFO | CLAP_PARAM_RESCAN_ALL) != 0 {
        unsafe { instance(host) }.rescan.store(true, SeqCst);
    }
}

unsafe extern "C" fn params_clear(_: *const clap_host, _: clap_id, _: clap_param_clear_flags) {}

unsafe extern "C" fn request_flush(host: *const clap_host) {
    unsafe { instance(host) }.flush.store(true, SeqCst);
}

unsafe extern "C" fn mark_dirty(_host: *const clap_host) {
    // Saving a project saves every plugin's settings anyway.
}

unsafe extern "C" fn latency_changed(_host: *const clap_host) {
    // ponytail: latency is read once, at load; re-read it when delay compensation can change.
}

unsafe extern "C" fn resize_hints_changed(_host: *const clap_host) {
    // Editor windows are sized by their plugins, not by hand.
}

unsafe extern "C" fn request_resize(host: *const clap_host, width: u32, height: u32) -> bool {
    let inst = unsafe { instance(host) };
    inst.resize
        .store(((width as u64) << 32) | height as u64, SeqCst);
    // Off the main thread, `idle` does it.
    inst.resize_window();
    true
}

unsafe extern "C" fn request_show(host: *const clap_host) -> bool {
    let window = unsafe { instance(host) }.window();
    window.map(|w| w.makeKeyAndOrderFront(None)).is_some()
}

unsafe extern "C" fn request_hide(host: *const clap_host) -> bool {
    let window = unsafe { instance(host) }.window();
    window.map(|w| w.orderOut(None)).is_some()
}

unsafe extern "C" fn gui_closed(host: *const clap_host, _was_destroyed: bool) {
    // Its editor is gone from the window; closing the window destroys what's left.
    if let Some(w) = unsafe { instance(host) }.window() {
        w.orderOut(None);
    }
}

unsafe extern "C" fn audio_ports_flag(_: *const clap_host, _: u32) -> bool {
    false
}

unsafe extern "C" fn audio_ports_rescan(_: *const clap_host, _: u32) {
    // ponytail: ports are read once, at load; reload the plugin when they change.
}

unsafe extern "C" fn note_dialects(_: *const clap_host) -> clap_note_dialect {
    CLAP_NOTE_DIALECT_CLAP | CLAP_NOTE_DIALECT_MIDI
}

unsafe extern "C" fn note_ports_rescan(_: *const clap_host, _: u32) {}

// ponytail: timers fire from `idle`, as often as it's called, not on their own.
unsafe extern "C" fn register_timer(
    host: *const clap_host,
    period_ms: u32,
    id: *mut clap_id,
) -> bool {
    if id.is_null() {
        return false;
    }
    let mut timers = unsafe { instance(host) }.timers.lock().unwrap();
    let new = timers.iter().map(|t| t.id + 1).max().unwrap_or(0);
    let period = Duration::from_millis(period_ms.max(1) as u64);
    timers.push(Timer {
        id: new,
        period,
        due: Instant::now() + period,
    });
    unsafe { *id = new };
    true
}

unsafe extern "C" fn unregister_timer(host: *const clap_host, id: clap_id) -> bool {
    let mut timers = unsafe { instance(host) }.timers.lock().unwrap();
    let before = timers.len();
    timers.retain(|t| t.id != id);
    timers.len() < before
}

unsafe extern "C" fn preset_error(
    host: *const clap_host,
    _kind: clap_preset_discovery_location_kind,
    location: *const c_char,
    _key: *const c_char,
    _os_error: i32,
    msg: *const c_char,
) {
    let name = &unsafe { instance(host) }.name;
    eprintln!("{name}: {}: {}", text(location), text(msg));
}

unsafe extern "C" fn preset_loaded(
    _: *const clap_host,
    _: clap_preset_discovery_location_kind,
    _: *const c_char,
    _: *const c_char,
) {
}

// Preset discovery: what tempo hands a bundle's preset providers.

/// A preset a plugin's providers found: its name, and where and how it's loaded.
struct Found {
    name: String,
    kind: clap_preset_discovery_location_kind,
    location: Option<CString>,
    key: Option<CString>,
}

impl Found {
    /// The file it is, if it's a whole file (not one of several in a file, or built in).
    fn file(&self) -> Option<PathBuf> {
        if self.kind != CLAP_PRESET_DISCOVERY_LOCATION_FILE || self.key.is_some() {
            return None;
        }
        let location = self.location.as_ref()?;
        Some(PathBuf::from(OsStr::from_bytes(location.to_bytes())))
    }
}

/// The files at `path` (or it, if it's a file), in subfolders too, with one of `extensions` (any,
/// if there are none).
fn preset_paths(path: &Path, extensions: &[String]) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for path in entries.flatten().map(|e| e.path()) {
            match path.is_dir() {
                true => walk(&path, out),
                false => out.push(path),
            }
        }
    }
    if path.is_file() {
        return vec![path.to_path_buf()];
    }
    let mut out = Vec::new();
    walk(path, &mut out);
    out.sort();
    let matches = |p: &Path| {
        extensions.iter().any(|e| {
            let e = e.trim_start_matches('.');
            e.is_empty() || p.extension().is_some_and(|x| x.eq_ignore_ascii_case(e))
        })
    };
    out.retain(|p| extensions.is_empty() || matches(p));
    out
}

/// Hears where a preset provider's presets are, and what kinds of file they're in.
#[repr(C)]
struct Indexer {
    clap: clap_preset_discovery_indexer,
    extensions: RefCell<Vec<String>>,
    /// Each place's kind, and its path for files.
    locations: RefCell<Vec<(clap_preset_discovery_location_kind, Option<PathBuf>)>>,
}

impl Indexer {
    fn new() -> Box<Indexer> {
        Box::new(Indexer {
            clap: clap_preset_discovery_indexer {
                clap_version: CLAP_VERSION,
                name: c"tempo".as_ptr(),
                vendor: c"tempo".as_ptr(),
                url: c"".as_ptr(),
                version: VERSION.as_ptr(),
                indexer_data: ptr::null_mut(),
                declare_filetype: Some(declare_filetype),
                declare_location: Some(declare_location),
                declare_soundpack: Some(declare_soundpack),
                get_extension: Some(indexer_extension),
            },
            extensions: RefCell::new(Vec::new()),
            locations: RefCell::new(Vec::new()),
        })
    }
}

unsafe fn indexer<'a>(i: *const clap_preset_discovery_indexer) -> &'a Indexer {
    unsafe { &*i.cast::<Indexer>() }
}

unsafe extern "C" fn declare_filetype(
    i: *const clap_preset_discovery_indexer,
    filetype: *const clap_preset_discovery_filetype,
) -> bool {
    let Some(filetype) = (unsafe { filetype.as_ref() }) else {
        return false;
    };
    let extension = text(filetype.file_extension);
    unsafe { indexer(i) }
        .extensions
        .borrow_mut()
        .push(extension);
    true
}

unsafe extern "C" fn declare_location(
    i: *const clap_preset_discovery_indexer,
    location: *const clap_preset_discovery_location,
) -> bool {
    let Some(l) = (unsafe { location.as_ref() }) else {
        return false;
    };
    let path =
        (l.kind == CLAP_PRESET_DISCOVERY_LOCATION_FILE && !l.location.is_null()).then(|| {
            PathBuf::from(OsStr::from_bytes(
                unsafe { CStr::from_ptr(l.location) }.to_bytes(),
            ))
        });
    let locations = &unsafe { indexer(i) }.locations;
    locations.borrow_mut().push((l.kind, path));
    true
}

unsafe extern "C" fn declare_soundpack(
    _: *const clap_preset_discovery_indexer,
    _: *const clap_preset_discovery_soundpack,
) -> bool {
    true
}

unsafe extern "C" fn indexer_extension(
    _: *const clap_preset_discovery_indexer,
    _: *const c_char,
) -> *const c_void {
    ptr::null()
}

/// Hears the presets a provider finds in one place.
#[repr(C)]
struct Receiver {
    clap: clap_preset_discovery_metadata_receiver,
    presets: RefCell<Vec<Listed>>,
}

/// A preset as a provider lists it: its name, load key, and the CLAP plugins it's for.
struct Listed {
    name: String,
    key: Option<CString>,
    plugins: Vec<CString>,
}

type Metadata = *const clap_preset_discovery_metadata_receiver;

impl Receiver {
    fn new() -> Box<Receiver> {
        Box::new(Receiver {
            clap: clap_preset_discovery_metadata_receiver {
                receiver_data: ptr::null_mut(),
                on_error: Some(metadata_error),
                begin_preset: Some(begin_preset),
                add_plugin_id: Some(add_plugin_id),
                set_soundpack_id: Some(ignore_text),
                set_flags: Some(set_flags),
                add_creator: Some(ignore_text),
                set_description: Some(ignore_text),
                set_timestamps: Some(set_timestamps),
                add_feature: Some(ignore_text),
                add_extra_info: Some(add_extra_info),
            },
            presets: RefCell::new(Vec::new()),
        })
    }
}

unsafe fn receiver<'a>(r: Metadata) -> &'a Receiver {
    unsafe { &*r.cast::<Receiver>() }
}

unsafe extern "C" fn begin_preset(r: Metadata, name: *const c_char, key: *const c_char) -> bool {
    // No key, or an empty one (as Surge gives), is a whole file.
    let key = Some(text(key))
        .filter(|k| !k.is_empty())
        .and_then(|k| CString::new(k).ok());
    let presets = &unsafe { receiver(r) }.presets;
    presets.borrow_mut().push(Listed {
        name: text(name),
        key,
        plugins: Vec::new(),
    });
    true
}

unsafe extern "C" fn add_plugin_id(r: Metadata, id: *const clap_universal_plugin_id) {
    let Some(id) = (unsafe { id.as_ref() }) else {
        return;
    };
    if text(id.abi) != "clap" || id.id.is_null() {
        return;
    }
    let mut presets = unsafe { receiver(r) }.presets.borrow_mut();
    if let Some(listed) = presets.last_mut() {
        listed
            .plugins
            .push(unsafe { CStr::from_ptr(id.id) }.to_owned());
    }
}

unsafe extern "C" fn metadata_error(_: Metadata, _: i32, _: *const c_char) {}
unsafe extern "C" fn ignore_text(_: Metadata, _: *const c_char) {}
unsafe extern "C" fn set_flags(_: Metadata, _: u32) {}
unsafe extern "C" fn set_timestamps(_: Metadata, _: u64, _: u64) {}
unsafe extern "C" fn add_extra_info(_: Metadata, _: *const c_char, _: *const c_char) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::value_from_text;
    use clap_sys::process::{CLAP_PROCESS_CONTINUE, clap_process_status};

    /// What a fake plugin heard, each at its time.
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Heard {
        On(u32, i16, f64),
        Off(u32, i16),
        Param(u32, u32, f64),
        Midi(u32, [u8; 3]),
        Expression(u32, i32, i16, f64),
    }

    /// A plugin of tempo's own for the tests: an instrument playing its Level (parameter 1, 0
    /// to 2), or an effect playing its input at its Level plus its sidechain. It keeps what it
    /// hears, and checks it's started and shut down in order.
    #[repr(C)]
    struct Fake {
        clap: clap_plugin,
        effect: bool,
        level: Cell<f64>,
        /// Parameter 2, 0 to 3 in steps.
        mode: Cell<f64>,
        heard: RefCell<Vec<Heard>>,
        /// The last transport: its flags, position in beats and tempo.
        transport: Cell<Option<(u32, i64, f64)>>,
        active: Cell<bool>,
        processing: Cell<bool>,
    }

    struct Shared<T>(T);
    unsafe impl<T> Sync for Shared<T> {}

    static INSTRUMENT: Shared<[*const c_char; 2]> = Shared([c"instrument".as_ptr(), ptr::null()]);
    static EFFECT: Shared<[*const c_char; 2]> = Shared([c"audio-effect".as_ptr(), ptr::null()]);
    static DESCRIPTORS: [clap_plugin_descriptor; 2] = [
        descriptor(c"tempo.test-synth", c"Test Synth", &INSTRUMENT.0),
        descriptor(c"tempo.test-fx", c"Test Effect", &EFFECT.0),
    ];
    static FACTORY: clap_plugin_factory = clap_plugin_factory {
        get_plugin_count: Some(fake_count),
        get_plugin_descriptor: Some(fake_descriptor),
        create_plugin: Some(fake_create),
    };

    const fn descriptor(
        id: &'static CStr,
        name: &'static CStr,
        features: &'static [*const c_char; 2],
    ) -> clap_plugin_descriptor {
        clap_plugin_descriptor {
            clap_version: CLAP_VERSION,
            id: id.as_ptr(),
            name: name.as_ptr(),
            vendor: c"tempo".as_ptr(),
            url: ptr::null(),
            manual_url: ptr::null(),
            support_url: ptr::null(),
            version: ptr::null(),
            description: ptr::null(),
            features: features.as_ptr(),
        }
    }

    unsafe extern "C" fn fake_count(_: *const clap_plugin_factory) -> u32 {
        2
    }

    unsafe extern "C" fn fake_descriptor(
        _: *const clap_plugin_factory,
        i: u32,
    ) -> *const clap_plugin_descriptor {
        &DESCRIPTORS[i as usize]
    }

    unsafe extern "C" fn fake_create(
        _: *const clap_plugin_factory,
        _: *const clap_host,
        id: *const c_char,
    ) -> *const clap_plugin {
        let effect = unsafe { CStr::from_ptr(id) } == c"tempo.test-fx";
        let fake = Box::new(Fake {
            clap: clap_plugin {
                desc: &DESCRIPTORS[effect as usize],
                plugin_data: ptr::null_mut(),
                init: Some(fake_init),
                destroy: Some(fake_destroy),
                activate: Some(fake_activate),
                deactivate: Some(fake_deactivate),
                start_processing: Some(fake_start),
                stop_processing: Some(fake_stop),
                reset: Some(fake_nothing),
                process: Some(fake_process),
                get_extension: Some(fake_extension),
                on_main_thread: Some(fake_nothing),
            },
            effect,
            level: Cell::new(1.0),
            mode: Cell::new(0.0),
            heard: RefCell::new(Vec::new()),
            transport: Cell::new(None),
            active: Cell::new(false),
            processing: Cell::new(false),
        });
        Box::into_raw(fake).cast()
    }

    unsafe fn fake<'a>(p: *const clap_plugin) -> &'a Fake {
        unsafe { &*p.cast::<Fake>() }
    }

    unsafe extern "C" fn fake_init(_: *const clap_plugin) -> bool {
        true
    }

    unsafe extern "C" fn fake_destroy(p: *const clap_plugin) {
        let f = unsafe { Box::from_raw(p.cast_mut().cast::<Fake>()) };
        assert!(
            !f.processing.get() && !f.active.get(),
            "destroyed while running"
        );
    }

    unsafe extern "C" fn fake_activate(p: *const clap_plugin, _: f64, _: u32, _: u32) -> bool {
        unsafe { fake(p) }.active.set(true);
        true
    }

    unsafe extern "C" fn fake_deactivate(p: *const clap_plugin) {
        let f = unsafe { fake(p) };
        assert!(!f.processing.get(), "deactivated while processing");
        f.active.set(false);
    }

    unsafe extern "C" fn fake_start(p: *const clap_plugin) -> bool {
        assert!(AUDIO.get(), "started off the audio thread");
        unsafe { fake(p) }.processing.set(true);
        true
    }

    unsafe extern "C" fn fake_stop(p: *const clap_plugin) {
        assert!(AUDIO.get(), "stopped off the audio thread");
        unsafe { fake(p) }.processing.set(false);
    }

    unsafe extern "C" fn fake_nothing(_: *const clap_plugin) {}

    impl Fake {
        fn hear(&self, events: *const clap_input_events) {
            let list = unsafe { &*events };
            for i in 0..unsafe { list.size.unwrap()(events) } {
                let e = unsafe { list.get.unwrap()(events, i) };
                let (time, kind) = unsafe { ((*e).time, (*e).type_) };
                let heard = match kind {
                    CLAP_EVENT_NOTE_ON | CLAP_EVENT_NOTE_OFF => {
                        let n = unsafe { &*e.cast::<clap_event_note>() };
                        assert_eq!(n.note_id, n.key as i32);
                        match kind == CLAP_EVENT_NOTE_ON {
                            true => Heard::On(time, n.key, n.velocity),
                            false => Heard::Off(time, n.key),
                        }
                    }
                    CLAP_EVENT_PARAM_VALUE => {
                        let p = unsafe { &*e.cast::<clap_event_param_value>() };
                        match p.param_id {
                            1 => self.level.set(p.value),
                            _ => self.mode.set(p.value),
                        }
                        Heard::Param(time, p.param_id, p.value)
                    }
                    CLAP_EVENT_MIDI => {
                        Heard::Midi(time, unsafe { (*e.cast::<clap_event_midi>()).data })
                    }
                    CLAP_EVENT_NOTE_EXPRESSION => {
                        let x = unsafe { &*e.cast::<clap_event_note_expression>() };
                        Heard::Expression(time, x.expression_id, x.key, x.value)
                    }
                    other => panic!("unexpected event {other}"),
                };
                self.heard.borrow_mut().push(heard);
            }
        }
    }

    unsafe extern "C" fn fake_process(
        p: *const clap_plugin,
        data: *const clap_process,
    ) -> clap_process_status {
        let (f, d) = unsafe { (fake(p), &*data) };
        assert!(f.processing.get() && AUDIO.get());
        f.hear(d.in_events);
        let t = unsafe { &*d.transport };
        f.transport.set(Some((t.flags, t.song_pos_beats, t.tempo)));
        let n = d.frames_count as usize;
        let out = unsafe { &*d.audio_outputs };
        for c in 0..2 {
            let dst = unsafe { slice::from_raw_parts_mut(*out.data32.add(c), n) };
            for (i, s) in dst.iter_mut().enumerate() {
                let input = |port: usize| unsafe {
                    let b = &*d.audio_inputs.add(port);
                    *(*b.data32.add(c)).add(i)
                };
                *s = match f.effect {
                    true => input(0) * f.level.get() as f32 + input(1),
                    false => f.level.get() as f32,
                };
            }
        }
        CLAP_PROCESS_CONTINUE
    }

    static FAKE_PARAMS: clap_plugin_params = clap_plugin_params {
        count: Some(param_count),
        get_info: Some(param_info),
        get_value: Some(param_get),
        value_to_text: Some(param_to_text),
        text_to_value: Some(param_from_text),
        flush: Some(param_flush),
    };
    static FAKE_STATE: clap_plugin_state = clap_plugin_state {
        save: Some(state_save),
        load: Some(state_load),
    };
    static FAKE_PORTS: clap_plugin_audio_ports = clap_plugin_audio_ports {
        count: Some(port_count),
        get: Some(port_get),
    };
    static FAKE_NOTES: clap_plugin_note_ports = clap_plugin_note_ports {
        count: Some(note_count),
        get: Some(note_get),
    };

    unsafe extern "C" fn fake_extension(_: *const clap_plugin, id: *const c_char) -> *const c_void {
        match unsafe { CStr::from_ptr(id) } {
            id if id == CLAP_EXT_PARAMS => ptr::from_ref(&FAKE_PARAMS).cast(),
            id if id == CLAP_EXT_STATE => ptr::from_ref(&FAKE_STATE).cast(),
            id if id == CLAP_EXT_AUDIO_PORTS => ptr::from_ref(&FAKE_PORTS).cast(),
            id if id == CLAP_EXT_NOTE_PORTS => ptr::from_ref(&FAKE_NOTES).cast(),
            _ => ptr::null(),
        }
    }

    unsafe extern "C" fn param_count(_: *const clap_plugin) -> u32 {
        3
    }

    unsafe extern "C" fn param_info(
        _: *const clap_plugin,
        i: u32,
        info: *mut clap_param_info,
    ) -> bool {
        let (id, name, flags, max, default) = match i {
            0 => (1, "Level", 0, 2.0, 1.0),
            1 => (2, "Mode", CLAP_PARAM_IS_STEPPED, 3.0, 0.0),
            _ => (3, "Hidden", CLAP_PARAM_IS_HIDDEN, 1.0, 0.0),
        };
        let info = unsafe { &mut *info };
        info.id = id;
        info.flags = flags | CLAP_PARAM_IS_AUTOMATABLE;
        for (dst, &src) in info.name.iter_mut().zip(name.as_bytes()) {
            *dst = src as c_char;
        }
        (info.min_value, info.max_value, info.default_value) = (0.0, max, default);
        true
    }

    unsafe extern "C" fn param_get(p: *const clap_plugin, id: clap_id, v: *mut f64) -> bool {
        let f = unsafe { fake(p) };
        unsafe { *v = if id == 1 { f.level.get() } else { f.mode.get() } };
        true
    }

    const MODES: [&str; 4] = ["Sine", "Saw", "Square", "Noise"];

    unsafe extern "C" fn param_to_text(
        _: *const clap_plugin,
        id: clap_id,
        v: f64,
        out: *mut c_char,
        size: u32,
    ) -> bool {
        let text = match id {
            1 => format!("{v:.2}"),
            _ => MODES[v as usize].to_string(),
        };
        let out = unsafe { slice::from_raw_parts_mut(out, size as usize) };
        for (dst, src) in out.iter_mut().zip(text.bytes().chain([0])) {
            *dst = src as c_char;
        }
        true
    }

    unsafe extern "C" fn param_from_text(
        _: *const clap_plugin,
        id: clap_id,
        text: *const c_char,
        v: *mut f64,
    ) -> bool {
        let text = unsafe { CStr::from_ptr(text) }.to_str().unwrap();
        let value = match id {
            1 => text.parse().ok(),
            _ => MODES.iter().position(|&m| m == text).map(|i| i as f64),
        };
        value.map(|x| unsafe { *v = x }).is_some()
    }

    unsafe extern "C" fn param_flush(
        p: *const clap_plugin,
        events: *const clap_input_events,
        _: *const clap_output_events,
    ) {
        assert!(AUDIO.get(), "flushed off the audio thread while active");
        unsafe { fake(p) }.hear(events);
    }

    unsafe extern "C" fn state_save(p: *const clap_plugin, s: *const clap_ostream) -> bool {
        let f = unsafe { fake(p) };
        let mut bytes = f.level.get().to_le_bytes().to_vec();
        bytes.extend(f.mode.get().to_le_bytes());
        let write = unsafe { (*s).write.unwrap() };
        unsafe { write(s, bytes.as_ptr().cast(), bytes.len() as u64) == 16 }
    }

    unsafe extern "C" fn state_load(p: *const clap_plugin, s: *const clap_istream) -> bool {
        let mut bytes = [0u8; 17];
        let read = unsafe { (*s).read.unwrap() };
        if unsafe { read(s, bytes.as_mut_ptr().cast(), 17) } != 16 {
            return false;
        }
        let f = unsafe { fake(p) };
        f.level
            .set(f64::from_le_bytes(bytes[..8].try_into().unwrap()));
        f.mode
            .set(f64::from_le_bytes(bytes[8..16].try_into().unwrap()));
        true
    }

    unsafe extern "C" fn port_count(p: *const clap_plugin, input: bool) -> u32 {
        match (input, unsafe { fake(p) }.effect) {
            (false, _) => 1,
            (true, true) => 2,
            (true, false) => 0,
        }
    }

    unsafe extern "C" fn port_get(
        _: *const clap_plugin,
        i: u32,
        _: bool,
        info: *mut clap_audio_port_info,
    ) -> bool {
        let info = unsafe { &mut *info };
        info.id = i;
        info.channel_count = 2;
        info.flags = if i == 0 { CLAP_AUDIO_PORT_IS_MAIN } else { 0 };
        true
    }

    unsafe extern "C" fn note_count(p: *const clap_plugin, input: bool) -> u32 {
        (input && !unsafe { fake(p) }.effect) as u32
    }

    unsafe extern "C" fn note_get(
        _: *const clap_plugin,
        _: u32,
        _: bool,
        info: *mut clap_note_port_info,
    ) -> bool {
        unsafe { (*info).supported_dialects = CLAP_NOTE_DIALECT_CLAP | CLAP_NOTE_DIALECT_MIDI };
        true
    }

    fn load(id: &CStr, state: Option<&[u8]>) -> (ClapPlugin, ClapProcessor) {
        let library = Library {
            entry: ptr::null(),
            factory: &FACTORY,
        };
        let d = library
            .descriptors()
            .into_iter()
            .find(|d| d.id.as_c_str() == id)
            .unwrap();
        ClapPlugin::instantiate(library, &d, 48_000.0, 512, state).unwrap()
    }

    fn heard(p: &ClapProcessor) -> Vec<Heard> {
        unsafe { fake(p.plugin) }.heard.take()
    }

    #[test]
    fn notes_controls_and_automation_arrive_in_time_order() {
        use ControlKind::*;
        let (_plugin, mut processor) = load(c"tempo.test-synth", None);
        // A second in, at 120 BPM: two beats.
        processor.set_transport(true, 48_000, 120.0);
        processor.automate(2, 0.0, 1.0);
        let note = |offset, velocity| Note {
            offset,
            pitch: 60,
            velocity,
        };
        let at = |offset, kind| Control { offset, kind };
        let controls = [
            at(
                10,
                Cc {
                    number: 1,
                    value: 1.0,
                },
            ),
            at(20, PitchBend(1.0)),
            at(30, Aftertouch(0.5)),
            at(
                40,
                PolyPressure {
                    pitch: 60,
                    value: 0.25,
                },
            ),
            at(
                50,
                Expression {
                    pitch: 60,
                    kind: ExpressionKind::Tuning,
                    value: 2.0,
                },
            ),
        ];
        let (mut l, mut r) = (vec![0f32; 128], vec![0f32; 128]);
        let notes = [note(0, 1.0), note(100, 0.0)];
        processor.process([&mut l, &mut r], 128, &notes, &controls, None);
        use Heard as H;
        assert_eq!(
            heard(&processor),
            [
                H::Param(0, 2, 0.0),
                H::On(0, 60, 1.0),
                H::Midi(10, [0xb0, 1, 127]),
                H::Midi(20, [0xe0, 0x7f, 0x7f]),
                H::Midi(30, [0xd0, 64, 0]),
                H::Expression(40, CLAP_NOTE_EXPRESSION_PRESSURE, 60, 0.25),
                H::Expression(50, CLAP_NOTE_EXPRESSION_TUNING, 60, 2.0),
                H::Off(100, 60),
                H::Param(127, 2, 3.0),
            ]
        );
        let (flags, beats, tempo) = unsafe { fake(processor.plugin) }.transport.get().unwrap();
        assert!(flags & CLAP_TRANSPORT_IS_PLAYING != 0);
        assert_eq!((beats, tempo), (2 * CLAP_BEATTIME_FACTOR, 120.0));
        // Its level, 1, on both sides.
        assert!(l.iter().chain(&r).all(|&s| s == 1.0));
        // Automation is for one block; the transport moves on with it.
        processor.process([&mut l, &mut r], 128, &[], &[], None);
        assert_eq!(heard(&processor), []);
        let beats = unsafe { fake(processor.plugin) }.transport.get().unwrap().1;
        let next = transport(true, 48_128, 120.0, 48_000.0);
        assert_eq!(beats, next.song_pos_beats);
    }

    #[test]
    fn values_turn_into_clap_s() {
        assert_eq!((bend(-1.0), bend(0.0), bend(1.0)), (0, 8192, 16383));
        assert_eq!(seven(0.5), 64);
        let pan = expression(ExpressionKind::Pan, -1.0);
        assert_eq!(pan, (CLAP_NOTE_EXPRESSION_PAN, 0.0));
        let volume = expression(ExpressionKind::Volume, 1.0);
        assert_eq!(volume, (CLAP_NOTE_EXPRESSION_VOLUME, 1.0));
        let mode = Range {
            id: 2,
            min: 0.0,
            max: 3.0,
            stepped: true,
        };
        let (plain, normal) = (mode.plain(0.4), mode.normal(2.0));
        assert_eq!((plain, normal, mode.steps()), (1.0, 2.0 / 3.0, 3));
        // Five and a half beats in: the second bar, which starts on beat 4.
        let t = transport(false, 132_000, 120.0, 48_000.0);
        assert_eq!((t.bar_number, t.bar_start), (1, 4 * CLAP_BEATTIME_FACTOR));
        assert_eq!(t.flags & CLAP_TRANSPORT_IS_PLAYING, 0);
    }

    #[test]
    fn a_plugin_that_only_takes_midi_hears_notes_as_midi() {
        let note = Note {
            offset: 3,
            pitch: 64,
            velocity: 0.5,
        };
        let on = note_event(&note, 3, true);
        assert_eq!(unsafe { on.midi.data }, [0x90, 64, 64]);
        let off = note_event(
            &Note {
                velocity: 0.0,
                ..note
            },
            3,
            true,
        );
        assert_eq!(unsafe { off.midi.data }, [0x80, 64, 0]);
        let vibrato = Control {
            offset: 0,
            kind: ControlKind::Expression {
                pitch: 64,
                kind: ExpressionKind::Vibrato,
                value: 1.0,
            },
        };
        assert!(control_event(&vibrato, 0, true).is_none());
    }

    #[test]
    fn parameters_are_read_turned_and_typed_as_the_plugin_says() {
        let (plugin, processor) = load(c"tempo.test-synth", None);
        let params = plugin.params();
        let shown: Vec<_> = params
            .iter()
            .map(|p| (p.id, &p.name[..], p.steps))
            .collect();
        assert_eq!(shown, [(1, "Level", 0), (2, "Mode", 3)]);
        assert_eq!((params[0].value, params[0].default), (0.5, 0.5));
        // Nothing's running it, so a change takes at once.
        plugin.set_param(1, 0.75);
        assert_eq!(plugin.param_value(1), 0.75);
        assert!(!processor.pending());
        assert_eq!(plugin.param_text(2, 1.0), "Noise");
        assert_eq!(plugin.param_text(1, 0.5), "1.00");
        assert_eq!(plugin.param_from_text(1, "0.5"), Some(0.25));
        assert_eq!(value_from_text(&plugin, &params[1], "Saw"), Ok(1.0 / 3.0));
    }

    #[test]
    fn edits_go_through_a_running_processor() {
        let (plugin, mut processor) = load(c"tempo.test-synth", None);
        let (mut l, mut r) = (vec![0f32; 64], vec![0f32; 64]);
        processor.process([&mut l, &mut r], 64, &[], &[], None);
        // Just now, however slow the machine running the test.
        processor.inst.processed.store(now(), SeqCst);
        plugin.set_param(1, 0.25);
        assert!(processor.pending());
        // It reads back as set, though the plugin hasn't had it yet.
        let fake = unsafe { fake(processor.plugin) };
        let level = || fake.level.get();
        assert_eq!((plugin.param_value(1), level()), (0.25, 1.0));
        processor.process([&mut l, &mut r], 64, &[], &[], None);
        assert!(!processor.pending());
        assert_eq!((plugin.param_value(1), level(), l[0]), (0.25, 0.5, 0.5));
        // Once it has, the plugin's own value counts again (moved in its editor, say).
        fake.level.set(2.0);
        assert_eq!(plugin.param_value(1), 1.0);
    }

    #[test]
    fn settings_round_trip() {
        let synth = c"tempo.test-synth";
        let (plugin, _processor) = load(synth, None);
        plugin.set_param(1, 0.25);
        plugin.set_param(2, 1.0);
        let state = plugin.state().unwrap();
        let (again, _p) = load(synth, Some(&state));
        assert_eq!((again.param_value(1), again.param_value(2)), (0.25, 1.0));
        plugin.set_param(1, 1.0);
        plugin.set_state(&state).unwrap();
        assert_eq!(plugin.param_value(1), 0.25);
        assert!(plugin.set_state(b"junk").is_err());
    }

    #[test]
    fn an_effect_hears_its_input_and_sidechain() {
        let (plugin, mut fx) = load(c"tempo.test-fx", None);
        assert!(plugin.sidechain() && fx.sidechain());
        assert!(!load(c"tempo.test-synth", None).0.sidechain());
        let (mut l, mut r) = (vec![1f32; 64], vec![0.5f32; 64]);
        let side = vec![0.25f32; 64];
        fx.process([&mut l, &mut r], 64, &[], &[], Some([&side, &side]));
        assert!(l.iter().all(|&s| s == 1.25) && r.iter().all(|&s| s == 0.75));
        // Either half can go last: the plugin is shut down then, in order.
        drop(fx);
        drop(plugin);
    }

    /// Runs `blocks` blocks of 512, the notes and controls in the first, and returns what it
    /// played on the left.
    fn run(p: &mut Processor, blocks: usize, notes: &[Note], controls: &[Control]) -> Vec<f32> {
        let mut played = Vec::new();
        for b in 0..blocks {
            let (mut l, mut r) = (vec![0f32; 512], vec![0f32; 512]);
            let (notes, controls) = if b == 0 {
                (notes, controls)
            } else {
                (&[][..], &[][..])
            };
            p.process([&mut l, &mut r], 512, notes, controls, None);
            played.extend(l);
        }
        played
    }

    fn loudest(s: &[f32]) -> f32 {
        s.iter().fold(0f32, |m, x| m.max(x.abs()))
    }

    /// How often it goes up through zero: its pitch, roughly.
    fn rises(s: &[f32]) -> usize {
        s.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count()
    }

    /// With a real instrument, which CI doesn't have:
    /// `TEMPO_TEST_CLAP="/path/Surge XT.clap" cargo test -p tempo-engine clap -- --ignored`
    /// (skipped without it, so the other formats' real-plugin tests run alone).
    #[test]
    #[ignore]
    fn a_real_instrument_plays_saves_restores_and_shuts_down() {
        let Ok(path) = std::env::var("TEMPO_TEST_CLAP") else {
            return eprintln!("skipped: set TEMPO_TEST_CLAP to a .clap instrument");
        };
        let found = inspect(Path::new(&path)).unwrap();
        let info = found.iter().find(|p| p.instrument).expect("no instrument");
        let r = info.plugin_ref();
        let mut libraries = HashMap::new();
        let (plugin, mut processor) =
            super::instantiate(&mut libraries, &r, 48_000.0, 512, None).unwrap();
        let note = |velocity| Note {
            offset: 0,
            pitch: 60,
            velocity,
        };
        let played = run(&mut processor, 20, &[note(1.0)], &[]);
        assert!(loudest(&played) > 1e-3, "{}", loudest(&played));

        // Pitch bend reaches it: the held note goes up.
        let bent = |v| Control {
            offset: 0,
            kind: ControlKind::PitchBend(v),
        };
        let before = rises(&run(&mut processor, 40, &[], &[]));
        run(&mut processor, 4, &[], &[bent(1.0)]);
        let after = rises(&run(&mut processor, 40, &[], &[]));
        eprintln!("bending up: {before} rises, then {after}");
        assert!(after as f64 > before as f64 * 1.05);
        run(&mut processor, 1, &[note(0.0)], &[bent(0.0)]);

        // Its parameters can be read and turned, while it runs and while it doesn't.
        let params = plugin.params();
        assert!(!params.is_empty());
        let p = params.iter().find(|p| p.steps == 0).unwrap();
        let to = if p.value > 0.5 { 0.25 } else { 0.75 };
        plugin.set_param(p.id, to);
        run(&mut processor, 1, &[], &[]);
        let read = plugin.param_value(p.id);
        assert!((read - to).abs() < 0.02, "{} is {read}", p.name);
        std::thread::sleep(Duration::from_millis(2 * RUNNING_MS));
        plugin.set_param(p.id, p.value);
        let read = plugin.param_value(p.id);
        assert!((read - p.value).abs() < 0.02, "{} is {read}", p.name);
        eprintln!("{}: {} parameters", plugin.name(), params.len());

        // And typed: a display reads back as the value shown. Only where the display follows
        // the value asked about: Surge's macros and Dexed's parameters show what's set.
        let varies =
            |p: &&PluginParam| plugin.param_text(p.id, 0.25) != plugin.param_text(p.id, 0.75);
        match params.iter().filter(|p| p.steps == 0).find(varies) {
            Some(p) => {
                let shown = plugin.param_text(p.id, to);
                let typed = value_from_text(&*plugin, p, &shown).unwrap();
                eprintln!("{} at {to} shows {shown:?}, which reads as {typed}", p.name);
                assert!((typed - to).abs() < 0.02);
            }
            None => eprintln!("its displays show what's set, whatever's asked"),
        }

        // Its settings survive a reload.
        let state = plugin.state().unwrap();
        drop(plugin);
        drop(processor);
        let (again, mut processor) =
            super::instantiate(&mut libraries, &r, 48_000.0, 512, Some(&state)).unwrap();
        let read = again.param_value(p.id);
        assert!((read - p.value).abs() < 0.02, "{} is {read}", p.name);
        assert!(loudest(&run(&mut processor, 20, &[note(1.0)], &[])) > 1e-3);
        run(&mut processor, 1, &[note(0.0)], &[]);

        // Its presets: those it finds, and tempo's own files.
        let presets = again.presets();
        let some: Vec<_> = presets.iter().take(3).collect();
        eprintln!("{} presets, like {some:?}", presets.len());
        if let Some(preset) = presets.get(presets.len() / 2) {
            let before = again.state().unwrap();
            again.load_preset(preset).unwrap();
            // A plugin may load it in the background while it plays (Surge does): give it time.
            let changed = (0..200).any(|_| {
                run(&mut processor, 1, &[], &[]);
                std::thread::sleep(Duration::from_millis(10));
                again.state().unwrap() != before
            });
            assert!(changed, "{preset:?} changed nothing");
        }
        let saved = again.save_preset("tempo clap test").unwrap();
        let file = PresetSource::File {
            file: saved.clone(),
        };
        let ours = again.presets().into_iter().find(|p| p.source == file);
        let loaded = ours.map(|p| again.load_preset(&p));
        std::fs::remove_file(&saved).unwrap();
        loaded.expect("its saved preset isn't listed").unwrap();
    }

    /// `TEMPO_TEST_CLAP_FX=/path/DragonflyRoomReverb.clap cargo test -p tempo-engine clap -- --ignored`
    #[test]
    #[ignore]
    fn a_real_effect_processes_audio() {
        let Ok(path) = std::env::var("TEMPO_TEST_CLAP_FX") else {
            return eprintln!("skipped: set TEMPO_TEST_CLAP_FX to a .clap effect");
        };
        let found = inspect(Path::new(&path)).unwrap();
        let info = found.iter().find(|p| !p.instrument).expect("no effect");
        let mut libraries = HashMap::new();
        let (plugin, mut fx) =
            super::instantiate(&mut libraries, &info.plugin_ref(), 48_000.0, 512, None).unwrap();
        let (mut heard, mut changed, mut tail) = (0f32, 0f32, 0f32);
        for b in 0..60 {
            let sine = |i: usize| {
                let t = (b * 512 + i) as f32 / 48_000.0;
                (t * 440.0 * std::f32::consts::TAU).sin() * 0.5
            };
            let input: Vec<f32> = (0..512)
                .map(|i| if b < 10 { sine(i) } else { 0.0 })
                .collect();
            let (mut l, mut r) = (input.clone(), input.clone());
            fx.process([&mut l, &mut r], 512, &[], &[], None);
            if b < 10 {
                heard = heard.max(loudest(&l));
                let diff = l.iter().zip(&input).map(|(a, b)| (a - b).abs());
                changed = diff.fold(changed, f32::max);
            } else {
                tail = tail.max(loudest(&l));
            }
        }
        eprintln!(
            "{}: loudest {heard}, most changed {changed}, tail {tail}, sidechain {}, {} parameters",
            plugin.name(),
            fx.sidechain(),
            plugin.params().len()
        );
        assert!(heard > 1e-3 && changed > 1e-3);
        let state = plugin.state().unwrap();
        plugin.set_state(&state).unwrap();
    }
}
