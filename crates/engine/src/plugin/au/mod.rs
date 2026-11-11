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
    kAudioUnitType_MIDIProcessor, kAudioUnitType_MusicDevice, kAudioUnitType_MusicEffect,
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

/// The kinds of unit Pina Studio plays: instruments, effects that take MIDI, effects, and MIDI
/// processors (MIDI FX: Scaler's, arpeggiators, chord makers).
const KINDS: [u32; 4] = [
    kAudioUnitType_MusicDevice,
    kAudioUnitType_MusicEffect,
    kAudioUnitType_Effect,
    kAudioUnitType_MIDIProcessor,
];

/// `kAudioUnitProperty_MIDIOutputCallback`: where a unit sends the MIDI it makes.
const MIDI_OUTPUT_CALLBACK: AudioUnitPropertyID = 48;

/// What `kAudioUnitProperty_MIDIOutputCallback` takes (Apple's `AUMIDIOutputCallbackStruct`).
#[repr(C)]
struct MidiOutputCallback {
    callback: Option<
        unsafe extern "C-unwind" fn(*mut c_void, *const AudioTimeStamp, u32, *const u8) -> i32,
    >,
    user: *mut c_void,
}

thread_local! {
    /// On the audio thread, while a unit renders: only then is the MIDI it sends taken, so a
    /// unit sending from its window's thread never races the block reading it.
    pub(crate) static RENDERING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The notes a unit played last block, and how long the block is.
pub(crate) struct NotesOut {
    pub(crate) list: Vec<Note>,
    pub(crate) frames: u32,
}

/// A unit's MIDI out: its packets (a `MIDIPacketList`) read for their notes. Each packet's time
/// is a sample into the block.
unsafe extern "C-unwind" fn midi_out(
    user: *mut c_void,
    _time: *const AudioTimeStamp,
    _port: u32,
    packets: *const u8,
) -> i32 {
    if !RENDERING.with(|r| r.get()) {
        return 0;
    }
    let (Some(out), false) = (
        unsafe { user.cast::<NotesOut>().as_mut() },
        packets.is_null(),
    ) else {
        return 0;
    };
    let count = unsafe { packets.cast::<u32>().read_unaligned() };
    // The packets: a time (8 bytes), a length (2), the bytes; each after the last, on four
    // bytes on Apple silicon.
    let mut at = unsafe { packets.add(4) };
    for _ in 0..count.min(256) {
        let time = unsafe { at.cast::<u64>().read_unaligned() };
        let len = unsafe { at.add(8).cast::<u16>().read_unaligned() } as usize;
        let data = unsafe { std::slice::from_raw_parts(at.add(10), len.min(256)) };
        let offset = match time < out.frames as u64 {
            true => time as u32,
            false => 0,
        };
        // Channel messages: three bytes, but program change and pressure (two).
        let mut k = 0;
        while k < data.len() {
            let status = data[k];
            let size = match status & 0xf0 {
                0xc0 | 0xd0 => 2,
                s if (0x80..0xf0).contains(&s) => 3,
                _ => 1,
            };
            if k + size > data.len() {
                break;
            }
            if size == 3 && out.list.len() < out.list.capacity() {
                let (a, b) = (data[k + 1] & 0x7f, data[k + 2]);
                match status & 0xf0 {
                    0x90 if b > 0 => out.list.push(Note {
                        offset,
                        pitch: a,
                        velocity: b as f32 / 127.0,
                        release: 0.0,
                    }),
                    0x80 | 0x90 => out.list.push(Note {
                        offset,
                        pitch: a,
                        velocity: 0.0,
                        release: 0.0,
                    }),
                    _ => {}
                }
            }
            k += size;
        }
        let next = unsafe { at.add(10 + len) } as usize;
        let next = match cfg!(target_arch = "aarch64") {
            true => (next + 3) & !3,
            false => next,
        };
        at = next as *const u8;
    }
    0
}

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
