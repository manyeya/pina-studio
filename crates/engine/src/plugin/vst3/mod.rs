//! Hosting VST3 plugins: load a `.vst3` bundle, run its processor on the audio thread, and open
//! its editor in a window of its own.

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
    ffi::{c_char, c_void},
    path::{Path, PathBuf},
    ptr,
};

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
    AUTOMATED, Control, ControlKind, EditReceiver, Editor, ExpressionKind, MainThreadMarker, Note,
    Plugin, PluginApi, PluginParam, Preset, PresetSource, Processor, ProcessorApi, Window,
    edit_queue, platform::Binary,
};
use crate::{plugin::scan::PluginInfo, song::project::PluginRef};

/// The category of a plugin's processor class.
pub const AUDIO_MODULE: &str = "Audio Module Class";

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
