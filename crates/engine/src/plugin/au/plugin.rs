//! The unit's main-thread side: parameters, state, presets and editor.

use super::*;

/// An Audio Unit's main-thread side: its parameters, settings, presets and editor.
pub struct AuPlugin {
    pub(crate) name: String,
    pub(crate) vendor: String,
    pub(crate) unit: Arc<Unit>,
    pub(crate) sidechain: bool,
}

impl AuPlugin {
    /// Apple's General MIDI synth, whose instruments are General MIDI's programs.
    pub(crate) fn general_midi(&self) -> bool {
        self.name == "DLSMusicDevice"
    }

    pub(crate) fn instantiate(
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
    pub(crate) fn write_preset(&self, dir: &Path, name: &str) -> Result<PathBuf, String> {
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
        let path = dir.join(super::super::preset_file_name(name, "aupreset"));
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }
}

/// A new instance of a unit: made here, or for one that must be made asynchronously (an AUv3
/// unit out of process), made there and waited for.
pub(crate) fn new_instance(c: &Component) -> Result<AudioUnit, String> {
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
        let dirs = super::super::preset_dirs(&self.vendor, &self.name);
        out.extend(super::super::preset_files(&dirs, "aupreset"));
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
        let dir = super::super::preset_dirs(&self.vendor, &self.name).remove(0);
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
        let window = Window::new(mtm, &self.name, size);
        let content = window
            .ns()
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
            if w.ns().contentRectForFrameRect(w.ns().frame()).size != size {
                w.set_content_size(size.width, size.height);
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
        window.show();
        Ok(Editor::new(window, move || {
            unsafe { center.removeObserver(observer.as_ref()) };
            view.removeFromSuperview();
        }))
    }
}

/// A global property that's a list (of ids, or pointers), if the unit has it; `zero` fills it.
pub(crate) unsafe fn property_list<T: Copy>(
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
pub(crate) unsafe fn send<F: Copy>() -> F {
    let f = objc2::ffi::objc_msgSend as unsafe extern "C-unwind" fn();
    unsafe { std::mem::transmute_copy(&f) }
}
