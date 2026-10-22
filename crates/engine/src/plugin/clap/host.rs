//! What Pina Studio offers a plugin as its host: the extensions it asks for, and their callbacks.

use super::*;

/// A C string from the plugin, or "" for none.
pub(crate) fn text(s: *const c_char) -> String {
    match s.is_null() {
        true => String::new(),
        false => unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned(),
    }
}

/// A C string in a buffer.
pub(crate) fn text_in(chars: &[c_char]) -> String {
    let bytes: Vec<u8> = chars
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// A descriptor's features, up to the null that ends them.
pub(crate) unsafe fn features<'a>(mut list: *const *const c_char) -> Vec<&'a CStr> {
    let mut out = Vec::new();
    while !list.is_null() && unsafe { !(*list).is_null() } {
        out.push(unsafe { CStr::from_ptr(*list) });
        list = unsafe { list.add(1) };
    }
    out
}

pub(crate) unsafe fn instance<'a>(host: *const clap_host) -> &'a Instance {
    unsafe { &*host.cast::<Instance>() }
}

pub(crate) static THREAD_CHECK: clap_host_thread_check = clap_host_thread_check {
    is_main_thread: Some(is_main_thread),
    is_audio_thread: Some(is_audio_thread),
};

pub(crate) static LOG: clap_host_log = clap_host_log { log: Some(log) };

pub(crate) static PARAMS: clap_host_params = clap_host_params {
    rescan: Some(params_rescan),
    clear: Some(params_clear),
    request_flush: Some(request_flush),
};

pub(crate) static STATE: clap_host_state = clap_host_state {
    mark_dirty: Some(mark_dirty),
};

pub(crate) static LATENCY: clap_host_latency = clap_host_latency {
    changed: Some(latency_changed),
};

pub(crate) static GUI: clap_host_gui = clap_host_gui {
    resize_hints_changed: Some(resize_hints_changed),
    request_resize: Some(request_resize),
    request_show: Some(request_show),
    request_hide: Some(request_hide),
    closed: Some(gui_closed),
};

pub(crate) static AUDIO_PORTS: clap_host_audio_ports = clap_host_audio_ports {
    is_rescan_flag_supported: Some(audio_ports_flag),
    rescan: Some(audio_ports_rescan),
};

pub(crate) static NOTE_PORTS: clap_host_note_ports = clap_host_note_ports {
    supported_dialects: Some(note_dialects),
    rescan: Some(note_ports_rescan),
};

pub(crate) static TIMER_SUPPORT: clap_host_timer_support = clap_host_timer_support {
    register_timer: Some(register_timer),
    unregister_timer: Some(unregister_timer),
};

pub(crate) static PRESET_LOAD: clap_host_preset_load = clap_host_preset_load {
    on_error: Some(preset_error),
    loaded: Some(preset_loaded),
};

pub(crate) unsafe extern "C" fn host_extension(
    _host: *const clap_host,
    id: *const c_char,
) -> *const c_void {
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

pub(crate) unsafe extern "C" fn request_restart(_host: *const clap_host) {
    // ponytail: ignored; deactivate and reactivate it when a plugin changes its ports or latency.
}

pub(crate) unsafe extern "C" fn request_process(_host: *const clap_host) {
    // Pina Studio runs every plugin all the time.
}

pub(crate) unsafe extern "C" fn request_callback(host: *const clap_host) {
    unsafe { instance(host) }.callback.store(true, SeqCst);
}

pub(crate) unsafe extern "C" fn is_main_thread(host: *const clap_host) -> bool {
    !AUDIO.get() && thread::current().id() == unsafe { instance(host) }.main
}

pub(crate) unsafe extern "C" fn is_audio_thread(_host: *const clap_host) -> bool {
    AUDIO.get()
}

pub(crate) unsafe extern "C" fn log(
    host: *const clap_host,
    severity: clap_log_severity,
    msg: *const c_char,
) {
    // ponytail: errors to stderr, the rest dropped; show them in the app if they help someone.
    if severity >= CLAP_LOG_ERROR {
        eprintln!("{}: {}", unsafe { instance(host) }.name, text(msg));
    }
}

pub(crate) unsafe extern "C" fn params_rescan(
    host: *const clap_host,
    flags: clap_param_rescan_flags,
) {
    if flags & (CLAP_PARAM_RESCAN_INFO | CLAP_PARAM_RESCAN_ALL) != 0 {
        unsafe { instance(host) }.rescan.store(true, SeqCst);
    }
}

pub(crate) unsafe extern "C" fn params_clear(
    _: *const clap_host,
    _: clap_id,
    _: clap_param_clear_flags,
) {
}

pub(crate) unsafe extern "C" fn request_flush(host: *const clap_host) {
    unsafe { instance(host) }.flush.store(true, SeqCst);
}

pub(crate) unsafe extern "C" fn mark_dirty(_host: *const clap_host) {
    // Saving a project saves every plugin's settings anyway.
}

pub(crate) unsafe extern "C" fn latency_changed(_host: *const clap_host) {
    // ponytail: latency is read once, at load; re-read it when delay compensation can change.
}

pub(crate) unsafe extern "C" fn resize_hints_changed(_host: *const clap_host) {
    // Editor windows are sized by their plugins, not by hand.
}

pub(crate) unsafe extern "C" fn request_resize(
    host: *const clap_host,
    width: u32,
    height: u32,
) -> bool {
    let inst = unsafe { instance(host) };
    inst.resize
        .store(((width as u64) << 32) | height as u64, SeqCst);
    // Off the main thread, `idle` does it.
    inst.resize_window();
    true
}

pub(crate) unsafe extern "C" fn request_show(host: *const clap_host) -> bool {
    let window = unsafe { instance(host) }.window();
    window.map(|w| w.makeKeyAndOrderFront(None)).is_some()
}

pub(crate) unsafe extern "C" fn request_hide(host: *const clap_host) -> bool {
    let window = unsafe { instance(host) }.window();
    window.map(|w| w.orderOut(None)).is_some()
}

pub(crate) unsafe extern "C" fn gui_closed(host: *const clap_host, _was_destroyed: bool) {
    // Its editor is gone from the window; closing the window destroys what's left.
    if let Some(w) = unsafe { instance(host) }.window() {
        w.orderOut(None);
    }
}

pub(crate) unsafe extern "C" fn audio_ports_flag(_: *const clap_host, _: u32) -> bool {
    false
}

pub(crate) unsafe extern "C" fn audio_ports_rescan(_: *const clap_host, _: u32) {
    // ponytail: ports are read once, at load; reload the plugin when they change.
}

pub(crate) unsafe extern "C" fn note_dialects(_: *const clap_host) -> clap_note_dialect {
    CLAP_NOTE_DIALECT_CLAP | CLAP_NOTE_DIALECT_MIDI
}

pub(crate) unsafe extern "C" fn note_ports_rescan(_: *const clap_host, _: u32) {}

// ponytail: timers fire from `idle`, as often as it's called, not on their own.
pub(crate) unsafe extern "C" fn register_timer(
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

pub(crate) unsafe extern "C" fn unregister_timer(host: *const clap_host, id: clap_id) -> bool {
    let mut timers = unsafe { instance(host) }.timers.lock().unwrap();
    let before = timers.len();
    timers.retain(|t| t.id != id);
    timers.len() < before
}
