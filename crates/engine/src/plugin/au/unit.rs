//! An Audio Unit made: its properties and parameters read and set, one call at a time.

use super::*;

/// Where the transport is, for the unit's host callbacks: set before each block, read as it
/// renders (or by its editor, on the main thread).
pub(crate) struct Transport {
    pub(crate) rate: f64,
    pub(crate) playing: AtomicBool,
    /// Whether `playing` changed since the unit last asked.
    pub(crate) changed: AtomicBool,
    /// In samples.
    pub(crate) position: AtomicU64,
    /// The tempo's bits.
    pub(crate) tempo: AtomicU64,
}

impl Transport {
    pub(crate) fn beat(&self) -> f64 {
        let seconds = self.position.load(Relaxed) as f64 / self.rate;
        seconds * self.tempo() / 60.0
    }

    pub(crate) fn tempo(&self) -> f64 {
        f64::from_bits(self.tempo.load(Relaxed))
    }
}

/// An Audio Unit instance, which both halves of a plugin share; the last to go disposes of it.
pub(crate) struct Unit {
    pub(crate) au: AudioUnit,
    /// What its host callbacks read; boxed, as the unit keeps its address.
    pub(crate) transport: Box<Transport>,
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
pub(crate) fn one_at_a_time<R>(f: impl FnOnce() -> R) -> R {
    static LOCK: Mutex<()> = Mutex::new(());
    let _held = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    f()
}

/// What a unit says about one of its parameters.
pub(crate) struct Info {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) units: String,
    pub(crate) range: Range,
    pub(crate) default: f32,
    pub(crate) flags: AudioUnitParameterOptions,
    pub(crate) unit: AudioUnitParameterUnit,
}

impl Info {
    /// Whether the unit puts its values into words, and reads them back.
    pub(crate) fn own_words(&self) -> bool {
        self.flags
            .contains(AudioUnitParameterOptions::Flag_ValuesHaveStrings)
    }
}

impl Unit {
    /// Reads a property into `value`, saying whether the unit had it.
    pub(crate) fn get<T>(
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

    pub(crate) fn set<T>(
        &self,
        id: AudioUnitPropertyID,
        scope: AudioUnitScope,
        element: u32,
        value: &T,
    ) -> i32 {
        let data = (value as *const T).cast();
        unsafe { AudioUnitSetProperty(self.au, id, scope, element, data, size_of::<T>() as u32) }
    }

    pub(crate) fn elements(&self, scope: AudioUnitScope) -> u32 {
        let mut n = 0u32;
        self.get(kAudioUnitProperty_ElementCount, scope, 0, &mut n);
        n
    }

    /// Asks for non-interleaved 32-bit float at `rate` on one of its buses, in stereo or else in
    /// the channels the bus has. Says how many channels it got.
    pub(crate) fn set_format(&self, scope: AudioUnitScope, element: u32, rate: f64) -> Option<u32> {
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

    pub(crate) fn ids(&self) -> Vec<u32> {
        unsafe { property_list(self.au, kAudioUnitProperty_ParameterList, 0u32) }
            .unwrap_or_default()
    }

    pub(crate) fn info(&self, id: u32) -> Option<Info> {
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

    pub(crate) fn infos(&self) -> Vec<Info> {
        self.ids()
            .into_iter()
            .filter_map(|id| self.info(id))
            .collect()
    }

    pub(crate) fn param(&self, id: u32) -> f32 {
        let mut v = 0.0;
        unsafe { AudioUnitGetParameter(self.au, id, GLOBAL, 0, NonNull::from(&mut v)) };
        v
    }

    pub(crate) fn set_param(&self, id: u32, value: f32) {
        unsafe { AudioUnitSetParameter(self.au, id, GLOBAL, 0, value, 0) };
    }

    /// Tells the unit's editor (and anything else listening) a parameter changed: `id`, or with
    /// `kAUParameterListener_AnyParameter`, all of them. Main thread only.
    pub(crate) fn notify(&self, id: u32) {
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
    pub(crate) fn class_info(&self) -> Option<CFRetained<CFPropertyList>> {
        let mut list: *const CFPropertyList = ptr::null();
        self.get(kAudioUnitProperty_ClassInfo, GLOBAL, 0, &mut list)
            .then(|| owned(list))
            .flatten()
    }

    pub(crate) fn set_class_info(&self, list: &CFPropertyList) -> Result<(), String> {
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
    pub(crate) fn text(&self, id: u32, value: f32) -> Option<String> {
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
    pub(crate) fn read(&self, id: u32, text: &str) -> Option<f32> {
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
    pub(crate) fn value_strings(&self, id: u32) -> Vec<String> {
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
