//! Hosting VST2 plugins (macOS): load a `.vst` bundle, run it on the audio thread, and open its
//! editor in a window of its own. The VST2 interface (2.4) is written out here: the `AEffect` a
//! plugin's entry point returns, the opcodes Pina Studio sends it, and the host callback it calls.
//!
//! VST2 has no controller apart from the processor: what the editor shows, and what Pina Studio reads
//! of a parameter, is what plays.

mod events;
mod ffi;
mod instance;
mod library;
mod plugin;
mod presets;
mod processor;

pub(crate) use events::*;
pub(crate) use ffi::*;
pub(crate) use instance::*;
pub use library::*;
pub use plugin::*;
pub(crate) use presets::*;
pub use processor::*;

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

const FLAG_HAS_EDITOR: i32 = 1;
const FLAG_CAN_REPLACING: i32 = 1 << 4;
const FLAG_PROGRAM_CHUNKS: i32 = 1 << 5;
const FLAG_IS_SYNTH: i32 = 1 << 8;

const CATEGORY_SYNTH: isize = 2;
const CATEGORY_SHELL: isize = 10;

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

const TRANSPORT_CHANGED: i32 = 1;
const TRANSPORT_PLAYING: i32 = 1 << 1;
const PPQ_POS_VALID: i32 = 1 << 9;
const PINA_VALID: i32 = 1 << 10;
const BARS_VALID: i32 = 1 << 11;
const TIME_SIG_VALID: i32 = 1 << 13;

const MIDI_TYPE: i32 = 1;

/// Most events a block sends a plugin; the rest are dropped.
const EVENTS: usize = 512;

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

/// "Nothing waiting" in `Instance::sent`.
const NOTHING: u64 = f64::NAN.to_bits();

const STATE_MAGIC: &[u8; 4] = b"TMV2";

/// Where an `.fxp` file's settings start: after its header and 28-byte program name.
const FXP_HEADER: usize = 56;

#[cfg(test)]
mod tests;
