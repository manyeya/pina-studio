//! Hosting Audio Units (macOS): the AUv2 API, which also opens AUv3 units. The unit is one
//! object both halves share: its parameters, settings and editor on the main thread, rendering on
//! the audio thread.

mod components;
mod editor;
mod params;
mod plugin;
mod processor;
mod unit;

pub use components::*;
pub(crate) use editor::*;
pub(crate) use params::*;
pub use plugin::*;
pub use processor::*;
pub(crate) use unit::*;

use std::{
    collections::HashMap,
    ffi::{CStr, c_void},
    mem::size_of,
    path::{Path, PathBuf},
    ptr::{self, NonNull},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
    },
    time::Duration,
};

use block2::RcBlock;
use objc2::{
    MainThreadMarker,
    rc::{Retained, autoreleasepool},
    runtime::{AnyClass, AnyObject, Sel},
    sel,
};
use objc2_app_kit::{NSView, NSViewFrameDidChangeNotification};
use objc2_audio_toolbox::{
    AUParameterListenerNotify, AUPreset, AURenderCallbackStruct, AudioComponent,
    AudioComponentCopyName, AudioComponentDescription, AudioComponentFindNext, AudioComponentFlags,
    AudioComponentGetDescription, AudioComponentInstanceDispose, AudioComponentInstanceNew,
    AudioComponentInstantiate, AudioComponentInstantiationOptions, AudioUnit,
    AudioUnitGetParameter, AudioUnitGetProperty, AudioUnitGetPropertyInfo, AudioUnitInitialize,
    AudioUnitParameter, AudioUnitParameterInfo, AudioUnitParameterOptions,
    AudioUnitParameterStringFromValue, AudioUnitParameterUnit, AudioUnitParameterValueFromString,
    AudioUnitPropertyID, AudioUnitRender, AudioUnitRenderActionFlags, AudioUnitScope,
    AudioUnitSetParameter, AudioUnitSetProperty, AudioUnitUninitialize, HostCallbackInfo,
    MusicDeviceMIDIEvent, kAUParameterListener_AnyParameter, kAudioUnitErr_TooManyFramesToProcess,
    kAudioUnitProperty_ClassInfo, kAudioUnitProperty_CocoaUI, kAudioUnitProperty_ElementCount,
    kAudioUnitProperty_FactoryPresets, kAudioUnitProperty_HostCallbacks,
    kAudioUnitProperty_Latency, kAudioUnitProperty_MaximumFramesPerSlice,
    kAudioUnitProperty_ParameterInfo, kAudioUnitProperty_ParameterList,
    kAudioUnitProperty_ParameterStringFromValue, kAudioUnitProperty_ParameterValueFromString,
    kAudioUnitProperty_ParameterValueStrings, kAudioUnitProperty_PresentPreset,
    kAudioUnitProperty_SetRenderCallback, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Global,
    kAudioUnitScope_Input, kAudioUnitScope_Output, kAudioUnitType_Effect,
    kAudioUnitType_MusicDevice, kAudioUnitType_MusicEffect,
};
use objc2_core_audio_types::{
    AudioBuffer, AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp, AudioTimeStampFlags,
    kAudioFormatFlagIsNonInterleaved, kAudioFormatFlagsNativeFloatPacked, kAudioFormatLinearPCM,
};
use objc2_core_foundation::{
    CFArray, CFData, CFDictionary, CFMutableDictionary, CFPropertyList, CFPropertyListCreateData,
    CFPropertyListCreateWithData, CFPropertyListFormat, CFRetained, CFString, CFType, CFURL,
    ConcreteType,
};
use objc2_foundation::{NSBundle, NSNotification, NSNotificationCenter, NSPoint, NSSize, NSString};

use super::{
    AUTOMATED, Control, Editor, Note, Plugin, PluginApi, PluginParam, Preset, PresetSource,
    Processor, ProcessorApi, Window,
};
use crate::{
    plugin::scan::PluginInfo,
    song::project::{Format, PluginRef},
};

/// The kinds of unit Pina Studio plays: instruments, effects that take MIDI, and effects.
const KINDS: [u32; 3] = [
    kAudioUnitType_MusicDevice,
    kAudioUnitType_MusicEffect,
    kAudioUnitType_Effect,
];

/// Most channels a unit's output can have; Pina Studio hears the first two.
const MAX_CHANNELS: usize = 8;

/// The scope of what's the unit's as a whole: its settings, and the parameters Pina Studio shows.
const GLOBAL: AudioUnitScope = kAudioUnitScope_Global;

/// Loads the Audio Unit `r` names: by its uid, or failing that its name.
pub fn instantiate(
    r: &PluginRef,
    rate: f64,
    max_block: usize,
    state: Option<&[u8]>,
) -> Result<(Plugin, Processor), String> {
    let component = r
        .uid
        .as_deref()
        .and_then(by_uid)
        .or_else(|| all().into_iter().find(|c| c.name == r.class))
        .ok_or_else(|| match &r.uid {
            Some(uid) => format!("{}: no Audio Unit {uid:?} installed", r.class),
            None => format!("{}: no Audio Unit by that name installed", r.class),
        })?;
    let (plugin, processor) = AuPlugin::instantiate(&component, rate, max_block, state)?;
    Ok((Box::new(plugin), Box::new(processor)))
}

/// General MIDI's instruments, by program: what Apple's DLS synth plays for each (it has no
/// presets of its own; a program change picks one).
use crate::song::midi_file::GM;

#[link(name = "CoreAudioKit", kind = "framework")]
unsafe extern "C" {}

#[link(name = "AudioToolbox", kind = "framework")]
unsafe extern "C" {
    fn AudioUnitScheduleParameters(unit: AudioUnit, events: *const ParameterEvent, n: u32) -> i32;
}

const RAMPED: u32 = 2;

// Apple's own units are on every Mac, so these run without anything installed, and render
// offline: no audio device needed.
#[cfg(test)]
mod tests;
