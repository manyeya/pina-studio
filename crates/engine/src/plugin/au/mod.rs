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
    AUTOMATED, Control, ControlKind, Editor, Note, Plugin, PluginApi, PluginParam, Preset,
    PresetSource, Processor, ProcessorApi, Window,
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
const GM: [&str; 128] = [
    "Acoustic Grand Piano",
    "Bright Acoustic Piano",
    "Electric Grand Piano",
    "Honky-tonk Piano",
    "Electric Piano 1",
    "Electric Piano 2",
    "Harpsichord",
    "Clavinet",
    "Celesta",
    "Glockenspiel",
    "Music Box",
    "Vibraphone",
    "Marimba",
    "Xylophone",
    "Tubular Bells",
    "Dulcimer",
    "Drawbar Organ",
    "Percussive Organ",
    "Rock Organ",
    "Church Organ",
    "Reed Organ",
    "Accordion",
    "Harmonica",
    "Tango Accordion",
    "Acoustic Guitar (nylon)",
    "Acoustic Guitar (steel)",
    "Electric Guitar (jazz)",
    "Electric Guitar (clean)",
    "Electric Guitar (muted)",
    "Overdriven Guitar",
    "Distortion Guitar",
    "Guitar Harmonics",
    "Acoustic Bass",
    "Electric Bass (finger)",
    "Electric Bass (pick)",
    "Fretless Bass",
    "Slap Bass 1",
    "Slap Bass 2",
    "Synth Bass 1",
    "Synth Bass 2",
    "Violin",
    "Viola",
    "Cello",
    "Contrabass",
    "Tremolo Strings",
    "Pizzicato Strings",
    "Orchestral Harp",
    "Timpani",
    "String Ensemble 1",
    "String Ensemble 2",
    "Synth Strings 1",
    "Synth Strings 2",
    "Choir Aahs",
    "Voice Oohs",
    "Synth Voice",
    "Orchestra Hit",
    "Trumpet",
    "Trombone",
    "Tuba",
    "Muted Trumpet",
    "French Horn",
    "Brass Section",
    "Synth Brass 1",
    "Synth Brass 2",
    "Soprano Sax",
    "Alto Sax",
    "Tenor Sax",
    "Baritone Sax",
    "Oboe",
    "English Horn",
    "Bassoon",
    "Clarinet",
    "Piccolo",
    "Flute",
    "Recorder",
    "Pan Flute",
    "Blown Bottle",
    "Shakuhachi",
    "Whistle",
    "Ocarina",
    "Lead 1 (square)",
    "Lead 2 (sawtooth)",
    "Lead 3 (calliope)",
    "Lead 4 (chiff)",
    "Lead 5 (charang)",
    "Lead 6 (voice)",
    "Lead 7 (fifths)",
    "Lead 8 (bass + lead)",
    "Pad 1 (new age)",
    "Pad 2 (warm)",
    "Pad 3 (polysynth)",
    "Pad 4 (choir)",
    "Pad 5 (bowed)",
    "Pad 6 (metallic)",
    "Pad 7 (halo)",
    "Pad 8 (sweep)",
    "FX 1 (rain)",
    "FX 2 (soundtrack)",
    "FX 3 (crystal)",
    "FX 4 (atmosphere)",
    "FX 5 (brightness)",
    "FX 6 (goblins)",
    "FX 7 (echoes)",
    "FX 8 (sci-fi)",
    "Sitar",
    "Banjo",
    "Shamisen",
    "Koto",
    "Kalimba",
    "Bagpipe",
    "Fiddle",
    "Shanai",
    "Tinkle Bell",
    "Agogo",
    "Steel Drums",
    "Woodblock",
    "Taiko Drum",
    "Melodic Tom",
    "Synth Drum",
    "Reverse Cymbal",
    "Guitar Fret Noise",
    "Breath Noise",
    "Seashore",
    "Bird Tweet",
    "Telephone Ring",
    "Helicopter",
    "Applause",
    "Gunshot",
];

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
