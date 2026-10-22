//! A plugin made from its factory: its host, its threads, its editor's window.

use super::*;

thread_local! {
    /// Whether this thread is in the plugin's processing side now: the audio thread in
    /// `process`, or another standing in for it when the audio thread isn't running it.
    pub(crate) static AUDIO: Cell<bool> = const { Cell::new(false) };
}

/// Runs `f` as the plugin's audio thread.
pub(crate) fn as_audio<R>(f: impl FnOnce() -> R) -> R {
    let was = AUDIO.replace(true);
    let r = f();
    AUDIO.set(was);
    r
}

/// Milliseconds since this process first asked, never 0.
pub(crate) fn now() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

/// One plugin and the host Pina Studio is to it: what its halves (and its editor) share, the last of
/// them to go shutting it down. `host` comes first, so the `clap_host` the plugin calls back
/// with points at the whole.
#[repr(C)]
pub(crate) struct Instance {
    pub(crate) host: clap_host,
    pub(crate) plugin: AtomicPtr<clap_plugin>,
    pub(crate) name: String,
    /// The thread it was made on, its main thread.
    pub(crate) main: ThreadId,
    pub(crate) active: AtomicBool,
    pub(crate) processing: AtomicBool,
    /// Its editor is open: created, and not yet destroyed.
    pub(crate) gui: AtomicBool,
    /// Who's in its processing side: nobody, the audio thread, or the main thread flushing.
    pub(crate) busy: AtomicU8,
    /// When it last processed (see `now`), 0 for never.
    pub(crate) processed: AtomicU64,
    /// How many edits its processor has applied.
    pub(crate) applied: AtomicU64,
    // What it's asked for: its main thread called, its parameters flushed, their info re-read.
    pub(crate) callback: AtomicBool,
    pub(crate) flush: AtomicBool,
    pub(crate) rescan: AtomicBool,
    /// A size its editor asked for (width << 32 | height), 0 for none.
    pub(crate) resize: AtomicU64,
    pub(crate) window: Mutex<Option<EditorWindow>>,
    pub(crate) timers: Mutex<Vec<Timer>>,
}

/// An open editor's window, kept for the plugin's requests to resize or show it. Used on the
/// main thread only.
pub(crate) struct EditorWindow(pub(crate) Retained<NSWindow>);

unsafe impl Send for EditorWindow {}

pub(crate) struct Timer {
    pub(crate) id: clap_id,
    pub(crate) period: Duration,
    pub(crate) due: Instant,
}

impl Instance {
    pub(crate) fn new(name: &str) -> Arc<Instance> {
        Arc::new(Instance {
            host: clap_host {
                clap_version: CLAP_VERSION,
                host_data: ptr::null_mut(),
                name: c"Pina Studio".as_ptr(),
                vendor: c"Pina Studio".as_ptr(),
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
    pub(crate) fn running(&self) -> bool {
        let at = self.processed.load(SeqCst);
        at != 0 && now() - at < RUNNING_MS
    }

    /// Takes its processing side for `who`, if nobody has it.
    pub(crate) fn enter(&self, who: u8) -> bool {
        self.busy
            .compare_exchange(IDLE, who, SeqCst, SeqCst)
            .is_ok()
    }

    pub(crate) fn leave(&self) {
        self.busy.store(IDLE, SeqCst);
    }

    /// Its editor's window, on the main thread.
    pub(crate) fn window(&self) -> Option<Retained<NSWindow>> {
        MainThreadMarker::new()?;
        let window = self.window.lock().unwrap();
        window.as_ref().map(|w| w.0.clone())
    }

    /// Gives its editor's window the size it asked for. Main thread only.
    pub(crate) fn resize_window(&self) {
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
pub(crate) unsafe fn extension<T>(plugin: *const clap_plugin, ids: &[&CStr]) -> Option<&'static T> {
    let get = unsafe { (*plugin).get_extension? };
    ids.iter()
        .find_map(|id| unsafe { get(plugin, id.as_ptr()).cast::<T>().as_ref() })
}
