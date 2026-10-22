//! Hosting VST3 plugins (macOS): load a `.vst3` bundle, run its processor on the audio thread,
//! and open its editor in a window of its own.

mod editor;
mod module;
mod plugin;
mod presets;
mod processor;

pub use editor::*;
pub use module::*;
pub use plugin::*;
pub(crate) use presets::*;
pub use processor::*;

mod com;

use std::{
    collections::HashMap,
    ffi::{CStr, CString, c_char, c_void},
    path::{Path, PathBuf},
    ptr,
};

use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::NSWindow;
use objc2_foundation::NSSize;
use vst3::{
    Class, ComPtr, ComRef, ComWrapper, Interface,
    Steinberg::{
        Vst::{
            BusDirections_::*, Event_::EventTypes_::*, MediaTypes_::*,
            ProcessContext_::StatesAndFlags_::*, ProcessModes_::*, SymbolicSampleSizes_::*, *,
        },
        *,
    },
};

use com::*;

use super::{
    AUTOMATED, Control, ControlKind, EditReceiver, Editor, ExpressionKind, Note, Plugin, PluginApi,
    PluginParam, Preset, PresetSource, Processor, ProcessorApi, edit_queue, editor_window,
};
use crate::{plugin::scan::PluginInfo, song::project::PluginRef};

/// The category of a plugin's processor class.
pub const AUDIO_MODULE: &str = "Audio Module Class";

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

/// Loads the plugin `r` names, from its bundle (loaded once into `modules`).
pub fn instantiate(
    modules: &mut HashMap<PathBuf, Module>,
    r: &PluginRef,
    rate: f64,
    max_block: usize,
    state: Option<&[u8]>,
) -> Result<(Plugin, Processor), String> {
    if !modules.contains_key(&r.path) {
        modules.insert(r.path.clone(), Module::load(&r.path)?);
    }
    let module = &modules[&r.path];
    let classes = module.classes();
    let class = classes
        .iter()
        .find(|c| c.category == AUDIO_MODULE && c.name == r.class)
        .ok_or_else(|| format!("{}: no plugin named {:?}", r.path.display(), r.class))?;
    let (plugin, processor) = Vst3Plugin::instantiate(module, class, rate, max_block, state)?;
    Ok((Box::new(plugin), Box::new(processor)))
}

const STATE_MAGIC: &[u8; 4] = b"TMPS";

#[cfg(test)]
mod tests;
