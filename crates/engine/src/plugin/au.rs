//! Hosting Audio Units (macOS): the AUv2 API, which also opens AUv3 units. The unit is one
//! object both halves share: its parameters, settings and editor on the main thread, rendering on
//! the audio thread.

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
    PresetSource, Processor, ProcessorApi, editor_window,
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

/// Where Audio Units are installed, as `.component` bundles.
fn component_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    vec![
        PathBuf::from("/Library/Audio/Plug-Ins/Components"),
        home.join("Library/Audio/Plug-Ins/Components"),
        PathBuf::from("/System/Library/Components"),
    ]
}

/// A four-character code as text, a character a byte ("aumu").
fn fourcc(code: u32) -> String {
    code.to_be_bytes().iter().map(|&b| b as char).collect()
}

/// A unit's type, subtype and manufacturer, as `PluginRef.uid` has them: "aumu samp appl".
fn uid(d: &AudioComponentDescription) -> String {
    format!(
        "{} {} {}",
        fourcc(d.componentType),
        fourcc(d.componentSubType),
        fourcc(d.componentManufacturer)
    )
}

/// The type, subtype and manufacturer a uid names. Codes can end in spaces ("dls "), so it's
/// read by position.
fn parse_uid(uid: &str) -> Option<[u32; 3]> {
    let chars: Vec<char> = uid.chars().collect();
    if chars.len() != 14 || chars[4] != ' ' || chars[9] != ' ' {
        return None;
    }
    let code = |at: usize| {
        chars[at..at + 4].iter().try_fold(0u32, |n, &c| {
            let b = u8::try_from(c as u32).ok()?;
            Some(n << 8 | b as u32)
        })
    };
    Some([code(0)?, code(5)?, code(10)?])
}

/// A unit's name, "Apple: AUSampler", as its vendor and its own name.
fn split_name(name: &str, manufacturer: u32) -> (String, String) {
    match name.split_once(": ") {
        Some((vendor, own)) => (vendor.trim().to_string(), own.trim().to_string()),
        None => (fourcc(manufacturer).trim().to_string(), name.to_string()),
    }
}

/// Takes ownership of a Core Foundation object the API handed over (it's ours to release).
fn owned<T: objc2_core_foundation::Type>(p: *const T) -> Option<CFRetained<T>> {
    NonNull::new(p as *mut T).map(|p| unsafe { CFRetained::from_raw(p) })
}

/// A string the API lends.
fn text(s: *const CFString) -> String {
    unsafe { s.as_ref() }.map_or_else(String::new, |s| s.to_string())
}

/// A property list read from its bytes, XML or binary.
fn plist(bytes: &[u8]) -> Option<CFRetained<CFPropertyList>> {
    let data = CFData::from_bytes(bytes);
    unsafe { CFPropertyListCreateWithData(None, Some(&data), 0, ptr::null_mut(), ptr::null_mut()) }
}

/// A property list as bytes, binary or XML.
fn plist_bytes(list: &CFPropertyList, format: CFPropertyListFormat) -> Option<Vec<u8>> {
    unsafe { CFPropertyListCreateData(None, Some(list), format, 0, ptr::null_mut()) }
        .map(|d| d.to_vec())
}

/// A dictionary's value for `key`, if it's a `T`.
fn value<'a, T: ConcreteType>(dict: &'a CFDictionary, key: &str) -> Option<&'a T> {
    let key = CFString::from_str(key);
    let v = unsafe { dict.value((&*key as *const CFString).cast()) };
    unsafe { v.cast::<CFType>().as_ref() }?.downcast_ref()
}

/// An array's items that are `T`s.
fn items<'a, T: ConcreteType + 'a>(array: &'a CFArray) -> impl Iterator<Item = &'a T> {
    (0..array.count()).filter_map(move |i| {
        let v = unsafe { array.value_at_index(i) };
        unsafe { v.cast::<CFType>().as_ref() }?.downcast_ref()
    })
}

/// An installed unit.
struct Component {
    component: AudioComponent,
    desc: AudioComponentDescription,
    vendor: String,
    name: String,
}

/// The units of a type, subtype and manufacturer (0 matching any).
fn find([t, s, m]: [u32; 3]) -> Vec<Component> {
    let desc = AudioComponentDescription {
        componentType: t,
        componentSubType: s,
        componentManufacturer: m,
        componentFlags: 0,
        componentFlagsMask: 0,
    };
    let mut out = Vec::new();
    let mut component = ptr::null_mut();
    loop {
        component = unsafe { AudioComponentFindNext(component, NonNull::from(&desc)) };
        if component.is_null() {
            return out;
        }
        let mut found = desc;
        let mut name: *const CFString = ptr::null();
        unsafe {
            AudioComponentGetDescription(component, NonNull::from(&mut found));
            AudioComponentCopyName(component, NonNull::from(&mut name));
        }
        let name = owned(name).map(|n| n.to_string()).unwrap_or_default();
        let (vendor, name) = split_name(&name, found.componentManufacturer);
        out.push(Component {
            component,
            desc: found,
            vendor,
            name,
        });
    }
}

/// Every unit Pina Studio can play.
fn all() -> Vec<Component> {
    KINDS
        .into_iter()
        .flat_map(|kind| find([kind, 0, 0]))
        .collect()
}

/// The unit a uid names, if it's installed.
fn by_uid(uid: &str) -> Option<Component> {
    find(parse_uid(uid)?).into_iter().next()
}

/// The `.component` bundles in `dirs`, by the units their `Info.plist`s list.
fn bundles(dirs: &[PathBuf]) -> HashMap<String, PathBuf> {
    let mut out = HashMap::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            if path.extension().is_none_or(|e| e != "component") {
                continue;
            }
            let Some(info) = std::fs::read(path.join("Contents/Info.plist"))
                .ok()
                .and_then(|b| plist(&b))
            else {
                continue;
            };
            let Some(info) = info.downcast_ref::<CFDictionary>() else {
                continue;
            };
            let Some(units) = value::<CFArray>(info, "AudioComponents") else {
                continue;
            };
            for unit in items::<CFDictionary>(units) {
                let code = |key| value::<CFString>(unit, key).map(|s| s.to_string());
                if let (Some(t), Some(s), Some(m)) =
                    (code("type"), code("subtype"), code("manufacturer"))
                {
                    out.entry(format!("{t} {s} {m}"))
                        .or_insert_with(|| path.clone());
                }
            }
        }
    }
    out
}

/// The Audio Units installed, the system's own included.
pub fn list() -> Vec<PluginInfo> {
    let bundles = bundles(&component_dirs());
    all()
        .into_iter()
        .map(|c| {
            let uid = uid(&c.desc);
            PluginInfo {
                format: Format::Au,
                path: bundles.get(&uid).cloned().unwrap_or_default(),
                class: c.name,
                uid: Some(uid),
                vendor: c.vendor,
                instrument: c.desc.componentType == kAudioUnitType_MusicDevice,
                categories: Vec::new(),
            }
        })
        .collect()
}

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

/// Where the transport is, for the unit's host callbacks: set before each block, read as it
/// renders (or by its editor, on the main thread).
struct Transport {
    rate: f64,
    playing: AtomicBool,
    /// Whether `playing` changed since the unit last asked.
    changed: AtomicBool,
    /// In samples.
    position: AtomicU64,
    /// The tempo's bits.
    tempo: AtomicU64,
}

impl Transport {
    fn beat(&self) -> f64 {
        let seconds = self.position.load(Relaxed) as f64 / self.rate;
        seconds * self.tempo() / 60.0
    }

    fn tempo(&self) -> f64 {
        f64::from_bits(self.tempo.load(Relaxed))
    }
}

/// An Audio Unit instance, which both halves of a plugin share; the last to go disposes of it.
struct Unit {
    au: AudioUnit,
    /// What its host callbacks read; boxed, as the unit keeps its address.
    transport: Box<Transport>,
}

// The unit is made to be used from the main thread and the audio thread at once; the transport
// is atomics.
unsafe impl Send for Unit {}
unsafe impl Sync for Unit {}

impl Drop for Unit {
    fn drop(&mut self) {
        one_at_a_time(|| unsafe {
            AudioUnitUninitialize(self.au);
            AudioComponentInstanceDispose(self.au);
        });
    }
}

/// Makes, starts or disposes of a unit while no other thread does: Apple's sampler keeps a buffer
/// for it that all its instances share, and aborts when two reach for it at once.
fn one_at_a_time<R>(f: impl FnOnce() -> R) -> R {
    static LOCK: Mutex<()> = Mutex::new(());
    let _held = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    f()
}

/// What a unit says about one of its parameters.
struct Info {
    id: u32,
    name: String,
    units: String,
    range: Range,
    default: f32,
    flags: AudioUnitParameterOptions,
    unit: AudioUnitParameterUnit,
}

impl Info {
    /// Whether the unit puts its values into words, and reads them back.
    fn own_words(&self) -> bool {
        self.flags
            .contains(AudioUnitParameterOptions::Flag_ValuesHaveStrings)
    }
}

impl Unit {
    /// Reads a property into `value`, saying whether the unit had it.
    fn get<T>(
        &self,
        id: AudioUnitPropertyID,
        scope: AudioUnitScope,
        element: u32,
        value: &mut T,
    ) -> bool {
        let mut size = size_of::<T>() as u32;
        let data = NonNull::from(value).cast();
        unsafe {
            AudioUnitGetProperty(self.au, id, scope, element, data, NonNull::from(&mut size)) == 0
        }
    }

    fn set<T>(
        &self,
        id: AudioUnitPropertyID,
        scope: AudioUnitScope,
        element: u32,
        value: &T,
    ) -> i32 {
        let data = (value as *const T).cast();
        unsafe { AudioUnitSetProperty(self.au, id, scope, element, data, size_of::<T>() as u32) }
    }

    fn elements(&self, scope: AudioUnitScope) -> u32 {
        let mut n = 0u32;
        self.get(kAudioUnitProperty_ElementCount, scope, 0, &mut n);
        n
    }

    /// Asks for non-interleaved 32-bit float at `rate` on one of its buses, in stereo or else in
    /// the channels the bus has. Says how many channels it got.
    fn set_format(&self, scope: AudioUnitScope, element: u32, rate: f64) -> Option<u32> {
        let format = |channels: u32| AudioStreamBasicDescription {
            mSampleRate: rate,
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kAudioFormatFlagsNativeFloatPacked | kAudioFormatFlagIsNonInterleaved,
            mBytesPerPacket: 4,
            mFramesPerPacket: 1,
            mBytesPerFrame: 4,
            mChannelsPerFrame: channels,
            mBitsPerChannel: 32,
            mReserved: 0,
        };
        if self.set(kAudioUnitProperty_StreamFormat, scope, element, &format(2)) == 0 {
            return Some(2);
        }
        let mut now: AudioStreamBasicDescription = unsafe { std::mem::zeroed() };
        if !self.get(kAudioUnitProperty_StreamFormat, scope, element, &mut now) {
            return None;
        }
        let channels = now.mChannelsPerFrame.clamp(1, MAX_CHANNELS as u32);
        (self.set(
            kAudioUnitProperty_StreamFormat,
            scope,
            element,
            &format(channels),
        ) == 0)
            .then_some(channels)
    }

    fn ids(&self) -> Vec<u32> {
        unsafe { property_list(self.au, kAudioUnitProperty_ParameterList, 0u32) }
            .unwrap_or_default()
    }

    fn info(&self, id: u32) -> Option<Info> {
        let mut info: AudioUnitParameterInfo = unsafe { std::mem::zeroed() };
        if !self.get(kAudioUnitProperty_ParameterInfo, GLOBAL, id, &mut info) {
            return None;
        }
        let flags = info.flags;
        let release = flags.contains(AudioUnitParameterOptions::Flag_CFNameRelease);
        let name = if flags.contains(AudioUnitParameterOptions::Flag_HasCFNameString)
            && !info.cfNameString.is_null()
        {
            text(info.cfNameString)
        } else {
            unsafe { CStr::from_ptr(info.name.as_ptr()) }
                .to_string_lossy()
                .into_owned()
        };
        let custom = info.unit == AudioUnitParameterUnit::CustomUnit;
        let units = match custom {
            true => text(info.unitName),
            false => units(info.unit).to_string(),
        };
        if release {
            drop(owned(info.cfNameString));
            if custom {
                drop(owned(info.unitName));
            }
        }
        Some(Info {
            id,
            name,
            units,
            range: Range::of(&info),
            default: info.defaultValue,
            flags,
            unit: info.unit,
        })
    }

    fn infos(&self) -> Vec<Info> {
        self.ids()
            .into_iter()
            .filter_map(|id| self.info(id))
            .collect()
    }

    fn param(&self, id: u32) -> f32 {
        let mut v = 0.0;
        unsafe { AudioUnitGetParameter(self.au, id, GLOBAL, 0, NonNull::from(&mut v)) };
        v
    }

    fn set_param(&self, id: u32, value: f32) {
        unsafe { AudioUnitSetParameter(self.au, id, GLOBAL, 0, value, 0) };
    }

    /// Tells the unit's editor (and anything else listening) a parameter changed: `id`, or with
    /// `kAUParameterListener_AnyParameter`, all of them. Main thread only.
    fn notify(&self, id: u32) {
        let param = AudioUnitParameter {
            mAudioUnit: self.au,
            mParameterID: id,
            mScope: GLOBAL,
            mElement: 0,
        };
        unsafe {
            AUParameterListenerNotify(ptr::null_mut(), ptr::null_mut(), NonNull::from(&param))
        };
    }

    /// Its settings: the class info property list.
    fn class_info(&self) -> Option<CFRetained<CFPropertyList>> {
        let mut list: *const CFPropertyList = ptr::null();
        self.get(kAudioUnitProperty_ClassInfo, GLOBAL, 0, &mut list)
            .then(|| owned(list))
            .flatten()
    }

    fn set_class_info(&self, list: &CFPropertyList) -> Result<(), String> {
        let list: *const CFPropertyList = list;
        match self.set(kAudioUnitProperty_ClassInfo, GLOBAL, 0, &list) {
            0 => {
                self.notify(kAUParameterListener_AnyParameter);
                Ok(())
            }
            e => Err(format!("it didn't take the settings (error {e})")),
        }
    }

    /// A parameter's value in the unit's words.
    fn text(&self, id: u32, value: f32) -> Option<String> {
        let mut ask = AudioUnitParameterStringFromValue {
            inParamID: id,
            inValue: NonNull::from(&value),
            outString: ptr::null(),
        };
        let asked = self.get(
            kAudioUnitProperty_ParameterStringFromValue,
            GLOBAL,
            0,
            &mut ask,
        );
        asked
            .then(|| owned(ask.outString))
            .flatten()
            .map(|s| s.to_string())
    }

    /// The unit's reading of a typed value.
    fn read(&self, id: u32, text: &str) -> Option<f32> {
        let text = CFString::from_str(text);
        let mut ask = AudioUnitParameterValueFromString {
            inParamID: id,
            inString: NonNull::from(&*text),
            outValue: 0.0,
        };
        let asked = self.get(
            kAudioUnitProperty_ParameterValueFromString,
            GLOBAL,
            0,
            &mut ask,
        );
        asked.then_some(ask.outValue)
    }

    /// An indexed parameter's names for its values, if it has them.
    fn value_strings(&self, id: u32) -> Vec<String> {
        let mut array: *const CFArray = ptr::null();
        if !self.get(
            kAudioUnitProperty_ParameterValueStrings,
            GLOBAL,
            id,
            &mut array,
        ) {
            return Vec::new();
        }
        owned(array).map_or_else(Vec::new, |a| {
            items::<CFString>(&a).map(|s| s.to_string()).collect()
        })
    }
}

/// How a parameter's 0 to 1 maps onto its values, as its display flags say: steps for an indexed
/// or on/off one, else a curve from its minimum to its maximum.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Range {
    min: f32,
    max: f32,
    steps: u32,
    curve: Curve,
    /// Whether the unit follows ramps scheduled for it.
    ramps: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Curve {
    Linear,
    Log,
    /// The value's share of the range is the position to this power.
    Power(f64),
}

impl Range {
    fn of(info: &AudioUnitParameterInfo) -> Range {
        use AudioUnitParameterOptions as F;
        let (min, max) = (info.minValue, info.maxValue.max(info.minValue));
        let steps = match info.unit {
            AudioUnitParameterUnit::Indexed | AudioUnitParameterUnit::Boolean => {
                (max - min).round().max(0.0) as u32
            }
            _ => 0,
        };
        let flags = info.flags;
        let curve = if flags.contains(F::Flag_DisplayLogarithmic) && min > 0.0 {
            Curve::Log
        } else {
            match F(flags.0 & F::Flag_DisplayMask.0) {
                F::Flag_DisplaySquareRoot => Curve::Power(2.0),
                F::Flag_DisplaySquared => Curve::Power(0.5),
                F::Flag_DisplayCubed => Curve::Power(1.0 / 3.0),
                F::Flag_DisplayCubeRoot => Curve::Power(3.0),
                _ => Curve::Linear,
            }
        };
        Range {
            min,
            max,
            steps,
            curve,
            ramps: flags.contains(F::Flag_CanRamp),
        }
    }

    /// The value at `x`, 0 to 1.
    fn value(&self, x: f64) -> f32 {
        let (min, max, x) = (self.min as f64, self.max as f64, x.clamp(0.0, 1.0));
        let v = if self.steps > 0 {
            min + (x * self.steps as f64).round()
        } else {
            match self.curve {
                Curve::Log => min * (max / min).powf(x),
                Curve::Power(p) => min + (max - min) * x.powf(p),
                Curve::Linear => min + (max - min) * x,
            }
        };
        v as f32
    }

    /// Where `v` sits, 0 to 1.
    fn position(&self, v: f32) -> f64 {
        let (min, max, v) = (self.min as f64, self.max as f64, v as f64);
        if max <= min {
            return 0.0;
        }
        let x = if self.steps > 0 {
            (v - min).round() / self.steps as f64
        } else {
            let share = (v - min) / (max - min);
            match self.curve {
                Curve::Log => (v.max(min) / min).ln() / (max / min).ln(),
                Curve::Power(p) => share.max(0.0).powf(1.0 / p),
                Curve::Linear => share,
            }
        };
        x.clamp(0.0, 1.0)
    }
}

/// A parameter unit's name, as Pina Studio shows it after a value.
fn units(unit: AudioUnitParameterUnit) -> &'static str {
    use AudioUnitParameterUnit as U;
    match unit {
        U::Percent | U::EqualPowerCrossfade => "%",
        U::Seconds => "s",
        U::SampleFrames => "samples",
        U::Phase | U::Degrees => "°",
        U::Rate => "x",
        U::Hertz => "Hz",
        U::Cents | U::AbsoluteCents => "cents",
        U::RelativeSemiTones => "semitones",
        U::Decibels => "dB",
        U::Meters => "m",
        U::Octaves => "octaves",
        U::BPM => "BPM",
        U::Beats => "beats",
        U::Milliseconds => "ms",
        U::Ratio => ":1",
        _ => "",
    }
}

/// A value and its unit, as Pina Studio shows a parameter the unit doesn't put into words: "2400 Hz",
/// "-6.0 dB", "50%".
fn with_units(v: f32, units: &str) -> String {
    let v = v as f64;
    let digits = match v.abs() {
        a if a >= 100.0 => 0,
        a if a >= 10.0 || a == 0.0 => 1,
        a if a >= 1.0 => 2,
        _ => 3,
    };
    let n = format!("{v:.digits$}");
    match units {
        "" => n,
        "%" | "°" | ":1" | "x" => format!("{n}{units}"),
        _ => format!("{n} {units}"),
    }
}

/// A typed value as `with_units` shows it, a number in the parameter's units: "850 Hz", "2.4
/// kHz", "-6 dB".
fn typed(text: &str) -> Option<f32> {
    let text = text.trim();
    let end = text
        .find(|c: char| !(c.is_ascii_digit() || "+-.".contains(c)))
        .unwrap_or(text.len());
    let n: f64 = text[..end].parse().ok()?;
    let kilo = text[end..].trim_start().starts_with(['k', 'K']);
    Some((if kilo { n * 1e3 } else { n }) as f32)
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

/// An Audio Unit's main-thread side: its parameters, settings, presets and editor.
pub struct AuPlugin {
    name: String,
    vendor: String,
    unit: Arc<Unit>,
    sidechain: bool,
}

/// An Audio Unit's audio-thread side.
pub struct AuProcessor {
    unit: Arc<Unit>,
    /// What the input render callback hands the unit; boxed, as the unit keeps its address.
    input: Box<Input>,
    /// The buffers the unit renders into, and the list that points at them.
    out: Vec<Vec<f32>>,
    list: Box<BufferList>,
    /// Channels on its main input and sidechain, mono ones hearing both sides mixed.
    in_channels: [u32; 2],
    time: AudioTimeStamp,
    /// Whether it hears MIDI: an instrument, or an effect that takes it.
    midi: bool,
    latency: u32,
    sidechain: bool,
    /// Its parameters' ranges, by id, for automation.
    ranges: Vec<(u32, Range)>,
    /// Automation for the next block: a parameter, and its value at the block's start and end.
    automation: [(u32, f64, f64); AUTOMATED],
    automated: usize,
    max_block: usize,
}

// The unit's handles are only used from the audio thread once the processor moves there.
unsafe impl Send for AuProcessor {}

/// The block's input as the unit pulls it, through `render_input`: the main input, the
/// sidechain, and silence for any other input.
struct Input {
    buses: [[Vec<f32>; 2]; 3],
    frames: usize,
}

/// An `AudioBufferList` with room for `MAX_CHANNELS` buffers.
#[repr(C)]
struct BufferList {
    count: u32,
    buffers: [AudioBuffer; MAX_CHANNELS],
}

impl AuPlugin {
    /// Apple's General MIDI synth, whose instruments are General MIDI's programs.
    fn general_midi(&self) -> bool {
        self.name == "DLSMusicDevice"
    }

    fn instantiate(
        c: &Component,
        rate: f64,
        max_block: usize,
        state: Option<&[u8]>,
    ) -> Result<(AuPlugin, AuProcessor), String> {
        let fail = |why: String| format!("{}: {why}", c.name);
        let au = one_at_a_time(|| new_instance(c)).map_err(fail)?;
        let unit = Arc::new(Unit {
            au,
            transport: Box::new(Transport {
                rate,
                playing: AtomicBool::new(true),
                changed: AtomicBool::new(false),
                position: AtomicU64::new(0),
                tempo: AtomicU64::new(120f64.to_bits()),
            }),
        });

        // Formats on every bus, the first input and output's being the ones that must take.
        let (inputs, outputs) = (
            unit.elements(kAudioUnitScope_Input),
            unit.elements(kAudioUnitScope_Output),
        );
        if outputs == 0 {
            return Err(fail("no audio output".into()));
        }
        let channels = unit
            .set_format(kAudioUnitScope_Output, 0, rate)
            .ok_or_else(|| fail("its output won't take 32-bit float".into()))?;
        for i in 1..outputs {
            unit.set_format(kAudioUnitScope_Output, i, rate);
        }
        let mut in_channels = [0; 2];
        for i in 0..inputs {
            let got = unit.set_format(kAudioUnitScope_Input, i, rate);
            if i == 0 && got.is_none() {
                return Err(fail("its input won't take 32-bit float".into()));
            }
            if let Some(slot) = in_channels.get_mut(i as usize) {
                *slot = got.unwrap_or(0);
            }
        }
        unit.set(
            kAudioUnitProperty_MaximumFramesPerSlice,
            GLOBAL,
            0,
            &(max_block as u32),
        );

        // It pulls its inputs from Pina Studio, and asks Pina Studio where the transport is.
        let silence = || [vec![0f32; max_block], vec![0f32; max_block]];
        let mut input = Box::new(Input {
            buses: [silence(), silence(), silence()],
            frames: 0,
        });
        let callback = AURenderCallbackStruct {
            inputProc: Some(render_input),
            inputProcRefCon: (&mut *input as *mut Input).cast(),
        };
        for i in 0..inputs {
            unit.set(
                kAudioUnitProperty_SetRenderCallback,
                kAudioUnitScope_Input,
                i,
                &callback,
            );
        }
        let host = HostCallbackInfo {
            hostUserData: (&*unit.transport as *const Transport).cast_mut().cast(),
            beatAndTempoProc: Some(beat_and_tempo),
            musicalTimeLocationProc: Some(musical_time),
            transportStateProc: Some(transport_state),
            transportStateProc2: Some(transport_state2),
        };
        unit.set(kAudioUnitProperty_HostCallbacks, GLOBAL, 0, &host);

        let status = one_at_a_time(|| unsafe { AudioUnitInitialize(au) });
        if status != 0 {
            return Err(fail(format!("it wouldn't start (error {status})")));
        }
        // Settings that don't load leave it as it starts, as for VST3.
        if let Some(list) = state.and_then(plist) {
            let _ = unit.set_class_info(&list);
        }
        let mut seconds = 0f64;
        unit.get(kAudioUnitProperty_Latency, GLOBAL, 0, &mut seconds);
        // ponytail: ranges are read once; reread them when a unit changes its parameter list
        // (a listener on kAudioUnitProperty_ParameterList), as some synths do with a new patch.
        let mut ranges: Vec<(u32, Range)> = unit.infos().iter().map(|i| (i.id, i.range)).collect();
        ranges.sort_unstable_by_key(|r| r.0);

        let mut time: AudioTimeStamp = unsafe { std::mem::zeroed() };
        time.mFlags = AudioTimeStampFlags::SampleTimeValid;
        let out: Vec<Vec<f32>> = (0..channels).map(|_| vec![0f32; max_block]).collect();
        let list = Box::new(BufferList {
            count: channels,
            buffers: [AudioBuffer {
                mNumberChannels: 1,
                mDataByteSize: 0,
                mData: ptr::null_mut(),
            }; MAX_CHANNELS],
        });
        let kind = c.desc.componentType;
        // A second input is a sidechain.
        let sidechain = inputs > 1 && in_channels[1] > 0;
        let plugin = AuPlugin {
            name: c.name.clone(),
            vendor: c.vendor.clone(),
            unit: unit.clone(),
            sidechain,
        };
        let processor = AuProcessor {
            unit,
            input,
            out,
            list,
            in_channels,
            time,
            midi: kind == kAudioUnitType_MusicDevice || kind == kAudioUnitType_MusicEffect,
            latency: (seconds.max(0.0) * rate).round() as u32,
            sidechain,
            ranges,
            automation: [(0, 0.0, 0.0); AUTOMATED],
            automated: 0,
            max_block,
        };
        Ok((plugin, processor))
    }

    /// Writes its settings as `<name>.aupreset` into `dir`, named `name` inside too.
    fn write_preset(&self, dir: &Path, name: &str) -> Result<PathBuf, String> {
        let info = self
            .unit
            .class_info()
            .ok_or("the plugin wouldn't give its settings")?;
        let dict = info
            .downcast_ref::<CFDictionary>()
            .ok_or("its settings aren't a dictionary")?;
        let named = unsafe { CFMutableDictionary::new_copy(None, 0, Some(dict)) }
            .ok_or("couldn't copy its settings")?;
        let (key, value) = (CFString::from_str("name"), CFString::from_str(name));
        unsafe {
            CFMutableDictionary::set_value(
                Some(&named),
                (&*key as *const CFString).cast(),
                (&*value as *const CFString).cast(),
            )
        };
        let bytes = plist_bytes(&named, CFPropertyListFormat::XMLFormat_v1_0)
            .ok_or("couldn't write its settings")?;
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(super::preset_file_name(name, "aupreset"));
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }
}

/// A new instance of a unit: made here, or for one that must be made asynchronously (an AUv3
/// unit out of process), made there and waited for.
fn new_instance(c: &Component) -> Result<AudioUnit, String> {
    let flags = AudioComponentFlags(c.desc.componentFlags);
    if !flags.contains(AudioComponentFlags::RequiresAsyncInstantiation) {
        let mut au = ptr::null_mut();
        let status = unsafe { AudioComponentInstanceNew(c.component, NonNull::from(&mut au)) };
        return match (status, au.is_null()) {
            (0, false) => Ok(au),
            _ => Err(format!("couldn't create it (error {status})")),
        };
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let done = RcBlock::new(move |au: AudioUnit, status: i32| {
        let _ = tx.send((au as usize, status));
    });
    unsafe {
        AudioComponentInstantiate(
            c.component,
            AudioComponentInstantiationOptions::empty(),
            &done,
        )
    };
    // ponytail: waits on this thread; if a unit answers on the main queue while this is the main
    // thread, it times out. Make loading asynchronous if such units turn up.
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok((au, 0)) if au != 0 => Ok(au as AudioUnit),
        Ok((_, status)) => Err(format!("couldn't create it (error {status})")),
        Err(_) => Err("it didn't load in time".into()),
    }
}

impl PluginApi for AuPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn sidechain(&self) -> bool {
        self.sidechain
    }

    fn params(&self) -> Vec<PluginParam> {
        let writable = AudioUnitParameterOptions::Flag_IsWritable;
        self.unit
            .infos()
            .into_iter()
            .filter(|i| i.flags.contains(writable))
            .map(|i| PluginParam {
                id: i.id,
                value: i.range.position(self.unit.param(i.id)),
                default: i.range.position(i.default),
                steps: i.range.steps,
                name: i.name,
                units: i.units,
            })
            .collect()
    }

    fn set_param(&self, id: u32, value: f64) {
        if let Some(info) = self.unit.info(id) {
            self.unit.set_param(id, info.range.value(value));
            self.unit.notify(id);
        }
    }

    // An Audio Unit has one value for its editor and its sound, so showing a value sets it: the
    // automation the engine plays sets the same one.
    fn show_param(&self, id: u32, value: f64) {
        self.set_param(id, value);
    }

    fn param_value(&self, id: u32) -> f64 {
        self.unit
            .info(id)
            .map_or(0.0, |info| info.range.position(self.unit.param(id)))
    }

    fn param_text(&self, id: u32, value: f64) -> String {
        let Some(info) = self.unit.info(id) else {
            return format!("{:.0}%", value * 100.0);
        };
        let v = info.range.value(value);
        if let Some(s) = info.own_words().then(|| self.unit.text(id, v)).flatten() {
            // Bare numbers get their unit.
            return match s.trim().parse::<f64>() {
                Ok(_) if !info.units.is_empty() => format!("{} {}", s.trim(), info.units),
                _ => s,
            };
        }
        if info.range.steps > 0 {
            let names = self.unit.value_strings(id);
            let i = (v - info.range.min).round().max(0.0) as usize;
            if let Some(name) = names.get(i) {
                return name.clone();
            }
            if info.unit == AudioUnitParameterUnit::Boolean {
                return (if v >= 0.5 { "On" } else { "Off" }).into();
            }
            return with_units(v.round(), &info.units);
        }
        with_units(v, &info.units)
    }

    fn param_from_text(&self, id: u32, text: &str) -> Option<f64> {
        let info = self.unit.info(id)?;
        if let Some(v) = info.own_words().then(|| self.unit.read(id, text)).flatten() {
            return Some(info.range.position(v));
        }
        let names = self.unit.value_strings(id);
        if !names.is_empty() {
            let i = names
                .iter()
                .position(|n| n.trim().eq_ignore_ascii_case(text.trim()))?;
            return Some(info.range.position(info.range.min + i as f32));
        }
        // Otherwise it reads as Pina Studio shows it: a number in its units.
        typed(text).map(|v| info.range.position(v))
    }

    fn state(&self) -> Option<Vec<u8>> {
        let info = self.unit.class_info()?;
        plist_bytes(&info, CFPropertyListFormat::BinaryFormat_v1_0)
    }

    fn set_state(&self, state: &[u8]) -> Result<(), String> {
        let list = plist(state).ok_or("not an Audio Unit's settings")?;
        self.unit
            .set_class_info(&list)
            .map_err(|e| format!("{}: {e}", self.name))
    }

    fn presets(&self) -> Vec<Preset> {
        let mut out = Vec::new();
        let mut array: *const CFArray = ptr::null();
        let got = self
            .unit
            .get(kAudioUnitProperty_FactoryPresets, GLOBAL, 0, &mut array);
        let factory = got.then(|| owned(array)).flatten();
        let factory: Vec<(String, i32)> = factory.map_or_else(Vec::new, |array| {
            (0..array.count())
                .filter_map(|i| unsafe { array.value_at_index(i).cast::<AUPreset>().as_ref() })
                .map(|p| (text(p.presetName).trim().to_string(), p.presetNumber))
                .collect()
        });
        // One unnamed program is no list at all (many units have just that).
        if !matches!(&factory[..], [(name, _)] if name.is_empty()) {
            out.extend(factory.into_iter().map(|(name, number)| Preset {
                name: match name.is_empty() {
                    true => format!("Program {}", number + 1),
                    false => name,
                },
                source: PresetSource::Program {
                    list: 0,
                    program: number.max(0) as u32,
                },
            }));
        }
        if out.is_empty() && self.general_midi() {
            out.extend(GM.iter().enumerate().map(|(program, name)| Preset {
                name: name.to_string(),
                source: PresetSource::Program {
                    list: 0,
                    program: program as u32,
                },
            }));
        }
        let dirs = super::preset_dirs(&self.vendor, &self.name);
        out.extend(super::preset_files(&dirs, "aupreset"));
        out
    }

    fn load_preset(&self, preset: &Preset) -> Result<(), String> {
        match &preset.source {
            // General MIDI: a program change on channel 1, where the notes go, before they do.
            PresetSource::Program { program, .. } if self.general_midi() && *program < 128 => {
                unsafe { MusicDeviceMIDIEvent(self.unit.au, 0xC0, *program, 0, 0) };
                Ok(())
            }
            PresetSource::Program { program, .. } => {
                let name = CFString::from_str(&preset.name);
                let p = AUPreset {
                    presetNumber: *program as i32,
                    presetName: &*name,
                };
                match self
                    .unit
                    .set(kAudioUnitProperty_PresentPreset, GLOBAL, 0, &p)
                {
                    0 => {
                        self.unit.notify(kAUParameterListener_AnyParameter);
                        Ok(())
                    }
                    e => Err(format!(
                        "{}: it wouldn't load {:?} (error {e})",
                        self.name, preset.name
                    )),
                }
            }
            PresetSource::File { file } => {
                let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
                let list = plist(&bytes)
                    .ok_or_else(|| format!("{}: not an Audio Unit preset", file.display()))?;
                self.unit
                    .set_class_info(&list)
                    .map_err(|e| format!("{}: {e}", file.display()))
            }
        }
    }

    fn save_preset(&self, name: &str) -> Result<PathBuf, String> {
        let dir = super::preset_dirs(&self.vendor, &self.name).remove(0);
        self.write_preset(&dir, name)
    }

    fn open_editor(&self, mtm: MainThreadMarker) -> Result<Editor, String> {
        let au = self.unit.au;
        // ponytail: an AUv3 unit's own view (kAudioUnitProperty_RequestViewController) isn't
        // asked for; it gets the generic one.
        let view = autoreleasepool(|_| unsafe { cocoa_view(au).or_else(|| generic_view(au)) })
            .ok_or_else(|| format!("{}: no editor", self.name))?;
        let size = view.frame().size;
        let size = match size.width > 0.0 && size.height > 0.0 {
            true => (size.width, size.height),
            false => (480.0, 320.0),
        };
        let window = editor_window(mtm, &self.name, size);
        let content = window
            .contentView()
            .ok_or_else(|| format!("{}: window has no content view", self.name))?;
        // The view sizes itself and the window follows; the window never sizes the view, which
        // some (Apple's generic one) would answer by resizing again.
        content.setAutoresizesSubviews(false);
        view.setFrameOrigin(NSPoint::new(0.0, 0.0));
        content.addSubview(&view);
        view.setPostsFrameChangedNotifications(true);
        let (w, v) = (window.clone(), view.clone());
        let resized = RcBlock::new(move |_: NonNull<NSNotification>| {
            let size = v.frame().size;
            if w.contentRectForFrameRect(w.frame()).size != size {
                w.setContentSize(size);
            }
        });
        let center = NSNotificationCenter::defaultCenter();
        let observer = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(NSViewFrameDidChangeNotification),
                Some(&view),
                None,
                &resized,
            )
        };
        window.makeKeyAndOrderFront(None);
        Ok(Editor::new(window, move || {
            unsafe { center.removeObserver(observer.as_ref()) };
            view.removeFromSuperview();
        }))
    }
}

/// A global property that's a list (of ids, or pointers), if the unit has it; `zero` fills it.
unsafe fn property_list<T: Copy>(
    au: AudioUnit,
    id: AudioUnitPropertyID,
    zero: T,
) -> Option<Vec<T>> {
    let mut size = 0u32;
    let ok = unsafe { AudioUnitGetPropertyInfo(au, id, GLOBAL, 0, &mut size, ptr::null_mut()) };
    if ok != 0 || (size as usize) < size_of::<T>() {
        return None;
    }
    let mut items = vec![zero; size as usize / size_of::<T>()];
    let mut size = (items.len() * size_of::<T>()) as u32;
    let ok = unsafe {
        let data = NonNull::new_unchecked(items.as_mut_ptr()).cast();
        AudioUnitGetProperty(au, id, GLOBAL, 0, data, NonNull::from(&mut size))
    };
    items.truncate(size as usize / size_of::<T>());
    (ok == 0).then_some(items)
}

/// Sends a message `objc2` can't check: these methods take an `AudioUnit`, which units' classes
/// encode under another name.
unsafe fn send<F: Copy>() -> F {
    let f = objc2::ffi::objc_msgSend as unsafe extern "C-unwind" fn();
    unsafe { std::mem::transmute_copy(&f) }
}

/// The unit's own Cocoa view, made by the factory class its view bundle names.
unsafe fn cocoa_view(au: AudioUnit) -> Option<Retained<NSView>> {
    // An `AudioUnitCocoaViewInfo`: the view bundle's URL, then class names, ours to release.
    let words = unsafe { property_list(au, kAudioUnitProperty_CocoaUI, ptr::null::<c_void>()) }?;
    let (url, classes) = words.split_first()?;
    let url = owned(url.cast::<CFURL>());
    let classes: Vec<CFRetained<CFString>> = classes
        .iter()
        .filter_map(|&p| owned(p.cast::<CFString>()))
        .collect();
    let path = url?.to_file_path()?;
    let bundle = NSBundle::bundleWithPath(&NSString::from_str(path.to_str()?))?;
    let class: &AnyClass = bundle.classNamed(&NSString::from_str(&classes.first()?.to_string()))?;
    let pick = sel!(uiViewForAudioUnit:withSize:);
    if !class.responds_to(pick) {
        return None;
    }
    let factory: Retained<AnyObject> = unsafe { objc2::msg_send![class, new] };
    let view_for: unsafe extern "C-unwind" fn(
        *const AnyObject,
        Sel,
        AudioUnit,
        NSSize,
    ) -> *mut NSView = unsafe { send() };
    let view = unsafe { view_for(&*factory, pick, au, NSSize::new(800.0, 600.0)) };
    unsafe { Retained::retain(view) }
}

#[link(name = "CoreAudioKit", kind = "framework")]
unsafe extern "C" {}

/// Apple's generic editor, a slider for each parameter: what units without one of their own get.
unsafe fn generic_view(au: AudioUnit) -> Option<Retained<NSView>> {
    let class = AnyClass::get(c"AUGenericView")?;
    let alloc: unsafe extern "C-unwind" fn(*const AnyClass, Sel) -> *mut AnyObject =
        unsafe { send() };
    let init: unsafe extern "C-unwind" fn(*mut AnyObject, Sel, AudioUnit) -> *mut NSView =
        unsafe { send() };
    let view = unsafe { init(alloc(class, sel!(alloc)), sel!(initWithAudioUnit:), au) };
    unsafe { Retained::from_raw(view) }
}

/// Hands the unit the block's input when it pulls an input bus: the main input on the first,
/// the sidechain on the second, silence on others.
unsafe extern "C-unwind" fn render_input(
    refcon: NonNull<c_void>,
    _flags: NonNull<AudioUnitRenderActionFlags>,
    _time: NonNull<AudioTimeStamp>,
    element: u32,
    frames: u32,
    data: *mut AudioBufferList,
) -> i32 {
    let input = unsafe { &mut *refcon.as_ptr().cast::<Input>() };
    let Some(list) = (unsafe { data.as_mut() }) else {
        return 0;
    };
    let frames = frames as usize;
    let buffers = unsafe {
        std::slice::from_raw_parts_mut(list.mBuffers.as_mut_ptr(), list.mNumberBuffers as usize)
    };
    let have = input.frames;
    let bus = (element as usize).min(2);
    for (c, b) in buffers.iter_mut().enumerate() {
        let src = match c {
            0 | 1 => &mut input.buses[bus][c],
            _ => &mut input.buses[2][0],
        };
        if b.mData.is_null() {
            // It wants Pina Studio's buffers.
            if frames > src.len() {
                return kAudioUnitErr_TooManyFramesToProcess;
            }
            b.mData = src.as_mut_ptr().cast();
            b.mDataByteSize = (frames * size_of::<f32>()) as u32;
        } else {
            let room = b.mDataByteSize as usize / size_of::<f32>();
            let dst =
                unsafe { std::slice::from_raw_parts_mut(b.mData.cast::<f32>(), frames.min(room)) };
            let n = dst.len().min(have);
            dst[..n].copy_from_slice(&src[..n]);
            dst[n..].fill(0.0);
        }
    }
    0
}

unsafe extern "C-unwind" fn beat_and_tempo(
    user: *mut c_void,
    beat: *mut f64,
    tempo: *mut f64,
) -> i32 {
    let t = unsafe { &*user.cast::<Transport>() };
    unsafe {
        if let Some(beat) = beat.as_mut() {
            *beat = t.beat();
        }
        if let Some(tempo) = tempo.as_mut() {
            *tempo = t.tempo();
        }
    }
    0
}

// ponytail: 4/4, as for VST3; pass the time signature when the engine gives it to plugins.
unsafe extern "C-unwind" fn musical_time(
    user: *mut c_void,
    to_next_beat: *mut u32,
    numerator: *mut f32,
    denominator: *mut u32,
    downbeat: *mut f64,
) -> i32 {
    let t = unsafe { &*user.cast::<Transport>() };
    let beat = t.beat();
    let samples_per_beat = t.rate * 60.0 / t.tempo().max(1.0);
    unsafe {
        if let Some(n) = to_next_beat.as_mut() {
            *n = ((beat.ceil() - beat) * samples_per_beat).round() as u32;
        }
        if let Some(n) = numerator.as_mut() {
            *n = 4.0;
        }
        if let Some(d) = denominator.as_mut() {
            *d = 4;
        }
        if let Some(d) = downbeat.as_mut() {
            *d = (beat / 4.0).floor() * 4.0;
        }
    }
    0
}

#[allow(clippy::too_many_arguments)]
unsafe extern "C-unwind" fn transport_state2(
    user: *mut c_void,
    playing: *mut u8,
    recording: *mut u8,
    changed: *mut u8,
    sample: *mut f64,
    cycling: *mut u8,
    cycle_start: *mut f64,
    cycle_end: *mut f64,
) -> i32 {
    let t = unsafe { &*user.cast::<Transport>() };
    unsafe {
        if let Some(p) = playing.as_mut() {
            *p = t.playing.load(Relaxed) as u8;
        }
        if let Some(r) = recording.as_mut() {
            *r = 0;
        }
        if let Some(c) = changed.as_mut() {
            *c = t.changed.swap(false, Relaxed) as u8;
        }
        if let Some(s) = sample.as_mut() {
            *s = t.position.load(Relaxed) as f64;
        }
        // ponytail: the cycle isn't passed to plugins yet.
        if let Some(c) = cycling.as_mut() {
            *c = 0;
        }
        for b in [cycle_start, cycle_end] {
            if let Some(b) = b.as_mut() {
                *b = 0.0;
            }
        }
    }
    0
}

unsafe extern "C-unwind" fn transport_state(
    user: *mut c_void,
    playing: *mut u8,
    changed: *mut u8,
    sample: *mut f64,
    cycling: *mut u8,
    cycle_start: *mut f64,
    cycle_end: *mut f64,
) -> i32 {
    unsafe {
        transport_state2(
            user,
            playing,
            ptr::null_mut(),
            changed,
            sample,
            cycling,
            cycle_start,
            cycle_end,
        )
    }
}

#[link(name = "AudioToolbox", kind = "framework")]
unsafe extern "C" {
    fn AudioUnitScheduleParameters(unit: AudioUnit, events: *const ParameterEvent, n: u32) -> i32;
}

/// An `AudioUnitParameterEvent` holding a ramp (the larger of its union's two cases).
#[repr(C)]
struct ParameterEvent {
    scope: u32,
    element: u32,
    parameter: u32,
    kind: u32,
    start_offset: i32,
    duration: u32,
    start: f32,
    end: f32,
}

const RAMPED: u32 = 2;

impl ProcessorApi for AuProcessor {
    fn latency(&self) -> u32 {
        self.latency
    }

    fn set_transport(&mut self, playing: bool, position: u64, tempo: f64) {
        let t = &self.unit.transport;
        if t.playing.swap(playing, Relaxed) != playing {
            t.changed.store(true, Relaxed);
        }
        t.position.store(position, Relaxed);
        t.tempo.store(tempo.to_bits(), Relaxed);
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

    // Parameters set on the main thread go straight into the unit.
    fn pending(&self) -> bool {
        false
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
        let au = self.unit.au;
        // The input it'll pull: a mono bus hears both sides.
        let input = &mut *self.input;
        input.frames = frames;
        let [main, side, silence] = &mut input.buses;
        let fill = |dst: &mut [Vec<f32>; 2], src: Option<[&[f32]; 2]>, channels: u32| match src {
            Some([l, r]) if channels == 1 => {
                for (d, (l, r)) in dst[0][..frames].iter_mut().zip(l.iter().zip(r)) {
                    *d = (l + r) / 2.0;
                }
            }
            Some(src) => {
                for (d, s) in dst.iter_mut().zip(src) {
                    d[..frames].copy_from_slice(&s[..frames]);
                }
            }
            None => dst.iter_mut().for_each(|d| d[..frames].fill(0.0)),
        };
        fill(main, Some([&buf[0][..], &buf[1][..]]), self.in_channels[0]);
        fill(side, sidechain, self.in_channels[1]);
        silence.iter_mut().for_each(|d| d[..frames].fill(0.0));

        // Notes and controls, merged in time.
        if self.midi {
            let (mut n, mut c) = (notes.iter().peekable(), controls.iter().peekable());
            loop {
                let note_first = match (n.peek(), c.peek()) {
                    (Some(a), Some(b)) => a.offset <= b.offset,
                    (Some(_), None) => true,
                    (None, Some(_)) => false,
                    (None, None) => break,
                };
                let (offset, [status, data1, data2]) = match note_first {
                    true => {
                        let note = n.next().unwrap();
                        (note.offset, note_message(note))
                    }
                    false => {
                        let control = c.next().unwrap();
                        let Some(m) = control_message(control.kind) else {
                            continue;
                        };
                        (control.offset, m)
                    }
                };
                let offset = offset.min(frames as u32 - 1);
                unsafe { MusicDeviceMIDIEvent(au, status, data1, data2, offset) };
            }
        }

        // Automation: the start value now, and a ramp to the end if the unit follows them.
        for &(id, start, end) in &self.automation[..std::mem::take(&mut self.automated)] {
            let Ok(i) = self.ranges.binary_search_by_key(&id, |r| r.0) else {
                continue;
            };
            let range = self.ranges[i].1;
            let (from, to) = (range.value(start), range.value(end));
            unsafe { AudioUnitSetParameter(au, id, GLOBAL, 0, from, 0) };
            if range.ramps && from != to {
                let ramp = ParameterEvent {
                    scope: GLOBAL,
                    element: 0,
                    parameter: id,
                    kind: RAMPED,
                    start_offset: 0,
                    duration: frames as u32,
                    start: from,
                    end: to,
                };
                unsafe { AudioUnitScheduleParameters(au, &ramp, 1) };
            }
        }

        // Render into Pina Studio's buffers; the unit may point the list at its own instead.
        let list = &mut *self.list;
        for (b, out) in list.buffers.iter_mut().zip(&mut self.out) {
            *b = AudioBuffer {
                mNumberChannels: 1,
                mDataByteSize: (frames * size_of::<f32>()) as u32,
                mData: out.as_mut_ptr().cast(),
            };
        }
        list.count = self.out.len() as u32;
        let mut flags = AudioUnitRenderActionFlags::empty();
        let status = unsafe {
            AudioUnitRender(
                au,
                &mut flags,
                NonNull::from(&mut self.time),
                0,
                frames as u32,
                NonNull::from(&mut *self.list).cast(),
            )
        };
        self.time.mSampleTime += frames as f64;
        // Mono output goes to both sides; a failed render is silence.
        let list = &*self.list;
        for (c, dst) in buf.into_iter().enumerate() {
            let b = &list.buffers[c.min(list.count as usize - 1)];
            if status != 0 || b.mData.is_null() {
                dst[..frames].fill(0.0);
                continue;
            }
            let src = unsafe { std::slice::from_raw_parts(b.mData.cast::<f32>(), frames) };
            dst[..frames].copy_from_slice(src);
        }
    }
}

/// A note on (velocity 1 to 127) or off, on channel 1.
fn note_message(n: &Note) -> [u32; 3] {
    if n.velocity > 0.0 {
        let velocity = (n.velocity * 127.0).round().clamp(1.0, 127.0) as u32;
        [0x90, n.pitch as u32, velocity]
    } else {
        [0x80, n.pitch as u32, 0]
    }
}

/// A control as a MIDI message on channel 1, if MIDI has one for it.
fn control_message(kind: ControlKind) -> Option<[u32; 3]> {
    let seven = |v: f32| (v * 127.0).round().clamp(0.0, 127.0) as u32;
    Some(match kind {
        ControlKind::Cc { number, value } => [0xB0, number.min(127) as u32, seven(value)],
        ControlKind::PitchBend(v) => {
            let bend = (8192.0 + v as f64 * 8192.0).round().clamp(0.0, 16383.0) as u32;
            [0xE0, bend & 0x7F, bend >> 7]
        }
        ControlKind::Aftertouch(v) => [0xD0, seven(v), 0],
        ControlKind::PolyPressure { pitch, value } => [0xA0, pitch as u32, seven(value)],
        // ponytail: per-note expression has no AUv2 message; send it as MIDI 2.0 (or MPE) when a
        // unit that takes those turns up.
        ControlKind::Expression { .. } => return None,
    })
}

// Apple's own units are on every Mac, so these run without anything installed, and render
// offline: no audio device needed.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::value_from_text;

    const RATE: f64 = 48_000.0;
    const BLOCK: usize = 512;

    fn load(uid: &str) -> (AuPlugin, AuProcessor) {
        let c = by_uid(uid).unwrap_or_else(|| panic!("{uid} isn't installed"));
        AuPlugin::instantiate(&c, RATE, BLOCK, None).unwrap()
    }

    fn param(p: &AuPlugin, name: &str) -> PluginParam {
        let params = p.params();
        let found = params.iter().find(|p| p.name == name).cloned();
        found.unwrap_or_else(|| panic!("no {name:?} in {params:?}"))
    }

    /// Runs `blocks` blocks of `input` through it, `notes` and `controls` in the first, and
    /// returns its left side.
    fn run(
        p: &mut AuProcessor,
        blocks: usize,
        input: impl Fn(usize) -> f32,
        notes: &[Note],
        controls: &[Control],
    ) -> Vec<f32> {
        let mut out = Vec::new();
        for b in 0..blocks {
            let mut l: Vec<f32> = (0..BLOCK).map(|i| input(b * BLOCK + i)).collect();
            let mut r = l.clone();
            let (n, c) = if b == 0 {
                (notes, controls)
            } else {
                (&[][..], &[][..])
            };
            p.process([&mut l, &mut r], BLOCK, n, c, None);
            out.extend(l);
        }
        out
    }

    fn silence(_: usize) -> f32 {
        0.0
    }

    fn sine(hz: f64) -> impl Fn(usize) -> f32 {
        move |i| (std::f64::consts::TAU * hz * i as f64 / RATE).sin() as f32 * 0.5
    }

    fn rms(s: &[f32]) -> f32 {
        (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
    }

    /// Its pitch, by how often it rises through zero.
    fn pitch(s: &[f32]) -> f64 {
        let rises = s.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        rises as f64 * RATE / s.len() as f64
    }

    fn note(pitch: u8, velocity: f32) -> Note {
        Note {
            offset: 0,
            pitch,
            velocity,
        }
    }

    #[test]
    fn apples_units_are_listed_with_their_bundles() {
        let units = list();
        let sampler = units
            .iter()
            .find(|u| u.uid.as_deref() == Some("aumu samp appl"))
            .unwrap();
        assert_eq!(sampler.format, Format::Au);
        assert_eq!(
            (&sampler.class[..], &sampler.vendor[..]),
            ("AUSampler", "Apple")
        );
        assert!(sampler.instrument);
        assert!(sampler.path.ends_with("CoreAudio.component"), "{sampler:?}");
        let delay = units.iter().find(|u| u.class == "AUDelay").unwrap();
        assert_eq!(delay.uid.as_deref(), Some("aufx dely appl"));
        assert!(!delay.instrument);
    }

    #[test]
    fn uids_are_read_by_position() {
        let dls = parse_uid("aumu dls  appl").unwrap();
        assert_eq!(
            dls,
            [
                kAudioUnitType_MusicDevice,
                u32::from_be_bytes(*b"dls "),
                u32::from_be_bytes(*b"appl")
            ]
        );
        assert_eq!(
            by_uid("aumu dls  appl").map(|c| uid(&c.desc)).as_deref(),
            Some("aumu dls  appl")
        );
        assert_eq!(parse_uid("aumu dls appl"), None);
        assert_eq!(parse_uid("aumu samp appl x"), None);
        // Loading falls back on the name.
        let r = PluginRef {
            format: Format::Au,
            class: "AULowpass".into(),
            ..Default::default()
        };
        assert_eq!(
            instantiate(&r, RATE, BLOCK, None).unwrap().0.name(),
            "AULowpass"
        );
    }

    #[test]
    fn an_instrument_plays_notes_and_hears_pitch_bend() {
        let (_, mut sampler) = load("aumu samp appl");
        // Its default sound is a sine: A is 440 Hz.
        let played = run(&mut sampler, 40, silence, &[note(69, 1.0)], &[]);
        let loudest = played.iter().fold(0f32, |m, x| m.max(x.abs()));
        assert!(loudest > 0.1, "{loudest}");
        let hz = pitch(&played[10 * BLOCK..]);
        assert!((hz - 440.0).abs() < 5.0, "{hz}");

        // Bent all the way up: two semitones.
        let (_, mut bent) = load("aumu samp appl");
        let up = Control {
            offset: 0,
            kind: ControlKind::PitchBend(1.0),
        };
        let played = run(&mut bent, 40, silence, &[note(69, 1.0)], &[up]);
        let hz = pitch(&played[10 * BLOCK..]);
        assert!((hz - 493.9).abs() < 5.0, "{hz}");

        // A note off lets it fade.
        let tail = run(&mut bent, 60, silence, &[note(69, 0.0)], &[]);
        assert!(
            rms(&tail[50 * BLOCK..]) < 1e-3,
            "{}",
            rms(&tail[50 * BLOCK..])
        );
    }

    #[test]
    fn a_general_midi_synth_hears_controllers() {
        let (_, mut dls) = load("aumu dls  appl");
        let loud = rms(&run(&mut dls, 20, silence, &[note(60, 0.8)], &[]));
        let (_, mut dls) = load("aumu dls  appl");
        let volume = Control {
            offset: 0,
            kind: ControlKind::Cc {
                number: 7,
                value: 0.0,
            },
        };
        let quiet = rms(&run(&mut dls, 20, silence, &[note(60, 0.8)], &[volume]));
        assert!(loud > 1e-3 && quiet < loud / 100.0, "{loud} {quiet}");
    }

    #[test]
    fn parameters_are_named_turned_and_read_as_text() {
        let (lowpass, _p) = load("aufx lpas appl");
        let cutoff = param(&lowpass, "Cutoff Frequency");
        assert_eq!(cutoff.units, "Hz");
        assert_eq!(cutoff.steps, 0);
        lowpass.set_param(cutoff.id, 0.25);
        assert!((lowpass.param_value(cutoff.id) - 0.25).abs() < 1e-4);
        let v = value_from_text(&lowpass, &cutoff, "1000 Hz").unwrap();
        assert_eq!(lowpass.param_text(cutoff.id, v), "1000 Hz");

        // An indexed one, with names for its values.
        let (eq, _p) = load("aufx nbeq appl");
        let kind = eq.params().into_iter().find(|p| p.name == "Type").unwrap();
        assert_eq!(kind.steps, 10);
        assert_eq!(eq.param_text(kind.id, 0.0), "Parametric");
        assert_eq!(eq.param_from_text(kind.id, "resonant low pass"), Some(0.3));
        let switch = eq
            .params()
            .into_iter()
            .find(|p| p.name == "Bypass")
            .unwrap();
        assert_eq!(switch.steps, 1);
        assert_eq!(
            (eq.param_text(switch.id, 0.0), eq.param_text(switch.id, 1.0)),
            ("Off".into(), "On".into())
        );
    }

    #[test]
    fn settings_round_trip() {
        let (delay, _p) = load("aufx dely appl");
        let time = param(&delay, "Delay Time");
        delay.set_param(time.id, 0.2);
        let state = delay.state().unwrap();
        let c = by_uid("aufx dely appl").unwrap();
        let (again, _p) = AuPlugin::instantiate(&c, RATE, BLOCK, Some(&state)).unwrap();
        assert!((again.param_value(time.id) - 0.2).abs() < 1e-4);
        let (third, _p) = load("aufx dely appl");
        third.set_state(&state).unwrap();
        assert!((third.param_value(time.id) - 0.2).abs() < 1e-4);
        assert!(third.set_state(b"junk").is_err());
        // Another unit's settings don't load.
        let (lowpass, _p) = load("aufx lpas appl");
        assert!(lowpass.set_state(&state).is_err());
    }

    #[test]
    fn factory_presets_load() {
        let (reverb, _p) = load("aufx rvb2 appl");
        let presets = reverb.presets();
        let hall = presets.iter().find(|p| p.name == "Large Hall").unwrap();
        assert!(matches!(hall.source, PresetSource::Program { list: 0, .. }));
        let before: Vec<f64> = reverb.params().iter().map(|p| p.value).collect();
        reverb.load_preset(hall).unwrap();
        let after: Vec<f64> = reverb.params().iter().map(|p| p.value).collect();
        assert_ne!(before, after);
    }

    #[test]
    fn presets_save_to_files_and_load_back() {
        let dir = std::env::temp_dir().join(format!("pina-test-aupreset-{}", std::process::id()));
        let (lowpass, _p) = load("aufx lpas appl");
        let cutoff = param(&lowpass, "Cutoff Frequency");
        lowpass.set_param(cutoff.id, 0.3);
        let file = lowpass.write_preset(&dir, "Dark: Very").unwrap();
        assert_eq!(file, dir.join("Dark- Very.aupreset"));
        let xml = std::fs::read_to_string(&file).unwrap();
        assert!(xml.contains("<string>Dark: Very</string>"), "{xml}");
        lowpass.set_param(cutoff.id, 0.9);
        let preset = Preset {
            name: "Dark".into(),
            source: PresetSource::File { file },
        };
        lowpass.load_preset(&preset).unwrap();
        assert!((lowpass.param_value(cutoff.id) - 0.3).abs() < 1e-4);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_effect_processes_its_input() {
        let (lowpass, mut p) = load("aufx lpas appl");
        let cutoff = param(&lowpass, "Cutoff Frequency");
        let v = value_from_text(&lowpass, &cutoff, "300 Hz").unwrap();
        lowpass.set_param(cutoff.id, v);
        let low = rms(&run(&mut p, 20, sine(100.0), &[], &[])[10 * BLOCK..]);
        let high = rms(&run(&mut p, 20, sine(8000.0), &[], &[])[10 * BLOCK..]);
        assert!(low > 0.3 && high < 0.01, "{low} {high}");
    }

    #[test]
    fn automation_moves_parameters() {
        let (lowpass, mut p) = load("aufx lpas appl");
        let cutoff = param(&lowpass, "Cutoff Frequency");
        p.automate(cutoff.id, 0.1, 0.2);
        run(&mut p, 1, silence, &[], &[]);
        assert!((lowpass.param_value(cutoff.id) - 0.1).abs() < 1e-4);
    }

    #[test]
    fn the_transport_reaches_the_host_callbacks() {
        let (_, mut p) = load("aumu samp appl");
        p.set_transport(true, 48_000, 90.0);
        let user = (&*p.unit.transport as *const Transport).cast_mut().cast();
        let (mut beat, mut tempo) = (0.0, 0.0);
        unsafe { beat_and_tempo(user, &mut beat, &mut tempo) };
        assert_eq!((beat, tempo), (1.5, 90.0));
        let (mut next, mut num, mut den, mut downbeat) = (0, 0.0, 0, 0.0);
        unsafe { musical_time(user, &mut next, &mut num, &mut den, &mut downbeat) };
        assert_eq!((next, num, den, downbeat), (16_000, 4.0, 4, 0.0));
        p.set_transport(false, 0, 90.0);
        let (mut playing, mut changed) = (1u8, 0u8);
        let none = ptr::null_mut();
        unsafe {
            transport_state2(
                user,
                &mut playing,
                none,
                &mut changed,
                ptr::null_mut(),
                none,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        assert_eq!((playing, changed), (0, 1));
        unsafe {
            transport_state2(
                user,
                &mut playing,
                none,
                &mut changed,
                ptr::null_mut(),
                none,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        assert_eq!(changed, 0);
    }

    /// With an instrument from elsewhere (its own editor, presets and settings):
    /// `PINA_TEST_AU="aumu SgXT VmbA" cargo test -p pina-engine -- --ignored`
    #[test]
    #[ignore]
    fn a_real_instrument_plays_saves_restores_and_shuts_down() {
        let uid = std::env::var("PINA_TEST_AU").expect("set PINA_TEST_AU");
        let (plugin, mut p) = load(&uid);
        let played = run(&mut p, 40, silence, &[note(60, 1.0)], &[]);
        let loudest = played.iter().fold(0f32, |m, x| m.max(x.abs()));
        assert!(loudest > 1e-3, "{loudest}");

        let presets = plugin.presets();
        if let Some(preset) = presets.get(1) {
            plugin.load_preset(preset).unwrap();
        }

        // Its parameters turn, though some snap to steps they don't declare.
        let params = plugin.params();
        let turned = |p: &&PluginParam| {
            let to = if p.value > 0.5 { 0.25 } else { 0.75 };
            plugin.set_param(p.id, to);
            (plugin.param_value(p.id) - to).abs() < 0.02
        };
        let param = params.iter().find(turned).expect("no parameter turns");
        let to = plugin.param_value(param.id);
        assert!(!plugin.param_text(param.id, to).is_empty());
        println!("{} parameters, {} presets", params.len(), presets.len());

        let state = plugin.state().unwrap();
        drop(plugin);
        drop(p);
        let c = by_uid(&uid).unwrap();
        let (again, _p) = AuPlugin::instantiate(&c, RATE, BLOCK, Some(&state)).unwrap();
        assert!((again.param_value(param.id) - to).abs() < 0.02);
    }

    #[test]
    fn samplers_start_and_stop_on_several_threads_at_once() {
        let threads: Vec<_> = (0..3)
            .map(|_| {
                std::thread::spawn(|| {
                    for _ in 0..30 {
                        drop(load("aumu samp appl"));
                    }
                })
            })
            .collect();
        threads.into_iter().for_each(|t| t.join().unwrap());
    }

    #[test]
    fn ranges_map_both_ways() {
        let range = |min, max, steps, curve| Range {
            min,
            max,
            steps,
            curve,
            ramps: false,
        };
        let log = range(20.0, 20_000.0, 0, Curve::Log);
        assert!((log.value(0.5) - 632.46).abs() < 0.01);
        assert!((log.position(632.456) - 0.5).abs() < 1e-5);
        let steps = range(1.0, 5.0, 4, Curve::Linear);
        assert_eq!((steps.value(0.6), steps.position(3.0)), (3.0, 0.5));
        let squared = range(0.0, 100.0, 0, Curve::Power(2.0));
        assert_eq!((squared.value(0.5), squared.position(25.0)), (25.0, 0.5));
        assert_eq!(range(1.0, 1.0, 0, Curve::Linear).position(1.0), 0.0);
    }

    #[test]
    fn controls_become_midi() {
        let bend = |v| control_message(ControlKind::PitchBend(v));
        assert_eq!(bend(0.0), Some([0xE0, 0, 64]));
        assert_eq!(bend(1.0), Some([0xE0, 127, 127]));
        assert_eq!(bend(-1.0), Some([0xE0, 0, 0]));
        let cc = ControlKind::Cc {
            number: 64,
            value: 1.0,
        };
        assert_eq!(control_message(cc), Some([0xB0, 64, 127]));
        assert_eq!(note_message(&note(60, 0.001)), [0x90, 60, 1]);
        assert_eq!(note_message(&note(60, 0.0)), [0x80, 60, 0]);
    }
}
