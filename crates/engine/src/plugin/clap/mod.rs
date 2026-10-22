//! Hosting CLAP plugins (`.clap` bundles on macOS, DLLs on Windows): load one's entry, run a
//! plugin's process on the audio thread, and open its editor in a window of its own.
//!
//! A plugin is shared by its two halves (and an open editor): whichever goes last shuts it down.
//! Edits from the main thread reach it through its processor while that runs; when nothing is
//! running it (a render that hasn't started, `pina params`) they're flushed to it at once.

mod events;
mod host;
mod instance;
mod library;
mod params;
mod plugin;
mod presets;
mod processor;

pub(crate) use events::*;
pub(crate) use host::*;
pub(crate) use instance::*;
pub use library::*;
pub(crate) use params::*;
pub use plugin::*;
pub(crate) use presets::*;
pub use processor::*;

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::{CStr, CString, OsStr, c_char, c_void},
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

use super::{
    AUTOMATED, Control, ControlKind, EditReceiver, EditSender, Editor, ExpressionKind,
    MainThreadMarker, Note, Plugin, PluginApi, PluginParam, Preset, PresetSource, Processor,
    ProcessorApi, Window, edit_queue, platform::Binary,
};
use crate::{
    plugin::scan::PluginInfo,
    song::project::{Format, PluginRef},
};

/// What Pina Studio's own preset files for a CLAP plugin end in: its settings, as `state` gives them.
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

// Who's in a plugin's processing side.
const IDLE: u8 = 0;
const PROCESSING: u8 = 1;
const FLUSHING: u8 = 2;

#[cfg(test)]
mod tests;
