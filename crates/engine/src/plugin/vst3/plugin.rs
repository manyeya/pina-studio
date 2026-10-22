//! The plugin's main-thread side: parameters, state, presets and editor.

use super::*;

/// A VST3 plugin's main-thread side: its controller and editor, and its saved state.
pub struct Vst3Plugin {
    pub name: String,
    pub(crate) vendor: String,
    pub(crate) sidechain: bool,
    pub(crate) cid: TUID,
    pub(crate) component: ComPtr<IComponent>,
    pub(crate) controller: ComPtr<IEditController>,
    pub(crate) _host: ComWrapper<HostApplication>,
    pub(crate) _handler: ComWrapper<ComponentHandler>,
}

impl Vst3Plugin {
    /// Creates a plugin at `rate`, restoring `state` (from `Plugin::state`) if given.
    pub fn instantiate(
        module: &Module,
        class: &ClassInfo,
        rate: f64,
        max_block: usize,
        state: Option<&[u8]>,
    ) -> Result<(Vst3Plugin, Vst3Processor), String> {
        let fail = |why: &str| format!("{}: {why}", class.name);
        unsafe {
            let host = ComWrapper::new(HostApplication);
            let host_ptr = com_ptr::<_, IHostApplication>(&host).cast::<FUnknown>();
            let component: ComPtr<IComponent> = module
                .create(&class.cid)
                .ok_or_else(|| fail("couldn't create it"))?;
            if component.initialize(host_ptr) != kResultOk {
                return Err(fail("initialize failed"));
            }
            let processor: ComPtr<IAudioProcessor> = component
                .cast()
                .ok_or_else(|| fail("not an audio processor"))?;

            let mut links = None;
            let controller: ComPtr<IEditController> = match component.cast() {
                Some(c) => c,
                None => {
                    let mut cid: TUID = [0; 16];
                    if component.getControllerClassId(&mut cid) != kResultOk {
                        return Err(fail("no edit controller"));
                    }
                    let c: ComPtr<IEditController> = module
                        .create(&cid)
                        .ok_or_else(|| fail("couldn't create its controller"))?;
                    c.initialize(host_ptr);
                    if let (Some(a), Some(b)) = (
                        component.cast::<IConnectionPoint>(),
                        c.cast::<IConnectionPoint>(),
                    ) {
                        a.connect(b.as_ptr());
                        b.connect(a.as_ptr());
                        links = Some((a, b));
                    }
                    let state = ComWrapper::new(MemoryStream::default());
                    if component.getState(com_ptr::<_, IBStream>(&state)) == kResultOk {
                        state.rewind();
                        c.setComponentState(com_ptr::<_, IBStream>(&state));
                    }
                    c
                }
            };

            if let Some((ours, theirs)) = state.and_then(unpack) {
                let stream = |bytes: &[u8]| ComWrapper::new(MemoryStream::from(bytes.to_vec()));
                let s = stream(ours);
                component.setState(com_ptr::<_, IBStream>(&s));
                let s = stream(ours);
                controller.setComponentState(com_ptr::<_, IBStream>(&s));
                if !theirs.is_empty() {
                    let s = stream(theirs);
                    controller.setState(com_ptr::<_, IBStream>(&s));
                }
            }

            // Stepped parameters (switches, types, modes) end a block's edits: see EditReceiver.
            let stepped = (0..controller.getParameterCount())
                .filter_map(|i| {
                    let mut info: ParameterInfo = std::mem::zeroed();
                    (controller.getParameterInfo(i, &mut info) == kResultOk && info.stepCount > 0)
                        .then_some(info.id)
                })
                .collect();
            let (edits_tx, edits) = edit_queue(stepped);
            let handler = ComWrapper::new(ComponentHandler::new(edits_tx));
            controller.setComponentHandler(com_ptr::<_, IComponentHandler>(&handler));

            // Ask for stereo everywhere, then take whatever the plugin settled on.
            let n_in = component.getBusCount(kAudio as _, kInput as _);
            let n_out = component.getBusCount(kAudio as _, kOutput as _);
            if n_out == 0 {
                return Err(fail("no audio output"));
            }
            let mut ins = vec![SpeakerArr::kStereo; n_in as usize];
            let mut outs = vec![SpeakerArr::kStereo; n_out as usize];
            processor.setBusArrangements(ins.as_mut_ptr(), n_in, outs.as_mut_ptr(), n_out);
            component.activateBus(kAudio as _, kOutput as _, 0, 1);
            if n_in > 0 {
                component.activateBus(kAudio as _, kInput as _, 0, 1);
            }
            // A second input that's an aux bus is a sidechain.
            let mut second: BusInfo = std::mem::zeroed();
            let sidechain = n_in > 1
                && component.getBusInfo(kAudio as _, kInput as _, 1, &mut second) == kResultOk
                && second.busType == BusTypes_::kAux as BusType;
            if sidechain {
                component.activateBus(kAudio as _, kInput as _, 1, 1);
            }
            // The parameters its MIDI controllers move, asked now: the controller isn't for the
            // audio thread.
            let mut midi = [None; 130];
            if let Some(m) = controller.cast::<IMidiMapping>() {
                for (cc, slot) in midi.iter_mut().enumerate() {
                    let mut id: ParamID = 0;
                    if m.getMidiControllerAssignment(0, 0, cc as _, &mut id) == kResultOk {
                        *slot = Some(id);
                    }
                }
            }
            if component.getBusCount(kEvent as _, kInput as _) > 0 {
                component.activateBus(kEvent as _, kInput as _, 0, 1);
            }

            // Every bus gets buffers, active or not: some plugins write to all of them.
            let mut buffers = Vec::new();
            let mut channels = Vec::new();
            let mut bus = |dir, i| {
                let mut arr: SpeakerArrangement = 0;
                processor.getBusArrangement(dir as _, i, &mut arr);
                let mut ptrs: Vec<*mut f32> = (0..arr.count_ones())
                    .map(|_| {
                        buffers.push(vec![0f32; max_block]);
                        buffers.last_mut().unwrap().as_mut_ptr()
                    })
                    .collect();
                let b = AudioBusBuffers {
                    numChannels: ptrs.len() as int32,
                    silenceFlags: 0,
                    __field0: AudioBusBuffers__type0 {
                        channelBuffers32: ptrs.as_mut_ptr(),
                    },
                };
                channels.push(ptrs);
                b
            };
            let inputs = (0..n_in).map(|i| bus(kInput, i)).collect();
            let outputs: Vec<AudioBusBuffers> = (0..n_out).map(|i| bus(kOutput, i)).collect();
            if outputs[0].numChannels == 0 {
                return Err(fail("main output has no channels"));
            }

            let mut setup = ProcessSetup {
                processMode: kRealtime as _,
                symbolicSampleSize: kSample32 as _,
                maxSamplesPerBlock: max_block as _,
                sampleRate: rate,
            };
            if processor.setupProcessing(&mut setup) != kResultOk {
                return Err(fail("setupProcessing failed"));
            }
            component.setActive(1);
            processor.setProcessing(1);

            let mut context: ProcessContext = std::mem::zeroed();
            context.sampleRate = rate;
            context.tempo = 120.0;
            context.timeSigNumerator = 4;
            context.timeSigDenominator = 4;
            context.state = (kPlaying | kTempoValid | kTimeSigValid) as _;

            let plugin = Vst3Plugin {
                name: class.name.clone(),
                vendor: class.vendor.clone(),
                sidechain,
                cid: class.cid,
                component: component.clone(),
                controller: controller.clone(),
                _host: host,
                _handler: handler,
            };
            let processor = Vst3Processor {
                processor,
                component,
                controller,
                links,
                events: ComWrapper::new(EventList::with_capacity(512)),
                params: ComWrapper::new(ParameterChanges::with_capacity(64)),
                out_events: ComWrapper::new(EventList::with_capacity(512)),
                out_params: ComWrapper::new(ParameterChanges::with_capacity(64)),
                edits,
                automation: [(0, 0.0, 0.0); AUTOMATED],
                automated: 0,
                _buffers: buffers,
                _channels: channels,
                inputs,
                outputs,
                context,
                max_block,
                midi,
                sidechain,
            };
            Ok((plugin, processor))
        }
    }
}

impl PluginApi for Vst3Plugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn sidechain(&self) -> bool {
        self.sidechain
    }

    fn params(&self) -> Vec<PluginParam> {
        use ParameterInfo_::ParameterFlags_::*;
        self.infos()
            .into_iter()
            .filter(|info| {
                let flags = info.flags;
                flags & kCanAutomate != 0 && flags & (kIsHidden | kIsBypass) == 0
            })
            .map(|info| PluginParam {
                id: info.id,
                name: text16(&info.title),
                value: unsafe { self.controller.getParamNormalized(info.id) },
                default: info.defaultNormalizedValue,
                steps: info.stepCount.max(0) as u32,
                units: text16(&info.units),
            })
            .collect()
    }

    fn set_param(&self, id: u32, value: f64) {
        let value = value.clamp(0.0, 1.0);
        unsafe { self.controller.setParamNormalized(id, value) };
        self._handler.edit(id, value);
    }

    fn show_param(&self, id: u32, value: f64) {
        unsafe {
            self.controller
                .setParamNormalized(id, value.clamp(0.0, 1.0))
        };
    }

    fn param_value(&self, id: u32) -> f64 {
        unsafe { self.controller.getParamNormalized(id) }
    }

    fn param_text(&self, id: u32, value: f64) -> String {
        let mut s: String128 = [0; 128];
        unsafe {
            if self.controller.getParamStringByValue(id, value, &mut s) != kResultOk {
                return format!("{:.0}%", value * 100.0);
            }
        }
        text16(&s)
    }

    fn param_from_text(&self, id: u32, text: &str) -> Option<f64> {
        let mut s: Vec<TChar> = text.encode_utf16().map(|c| c as TChar).collect();
        s.push(0);
        let mut v = 0.0;
        let ok = unsafe {
            self.controller
                .getParamValueByString(id, s.as_mut_ptr(), &mut v)
        };
        (ok == kResultOk).then_some(v)
    }

    fn state(&self) -> Option<Vec<u8>> {
        let (ours, theirs) = self.states()?;
        Some(pack(&ours, &theirs))
    }

    fn set_state(&self, state: &[u8]) -> Result<(), String> {
        let (ours, theirs) = unpack(state).ok_or("not a VST3 plugin's settings")?;
        self.restore(ours, theirs)
    }

    fn presets(&self) -> Vec<Preset> {
        let mut out = Vec::new();
        if let Some(units) = self.controller.cast::<IUnitInfo>() {
            unsafe {
                for i in 0..units.getProgramListCount() {
                    let mut list: ProgramListInfo = std::mem::zeroed();
                    if units.getProgramListInfo(i, &mut list) != kResultOk {
                        continue;
                    }
                    let named = |program: i32| {
                        let mut name: String128 = [0; 128];
                        units.getProgramName(list.id, program, &mut name);
                        text16(&name)
                    };
                    // One unnamed program is no list at all (many plugins have just that).
                    if list.programCount <= 1 && named(0).trim().is_empty() {
                        continue;
                    }
                    for program in 0..list.programCount.max(0) {
                        let name = named(program);
                        out.push(Preset {
                            name: match name.trim().is_empty() {
                                true => format!("Program {}", program + 1),
                                false => name,
                            },
                            source: PresetSource::Program {
                                list: list.id,
                                program: program as u32,
                            },
                        });
                    }
                }
            }
        }
        let cid = hex(&self.cid);
        let files = super::super::preset_files(
            &super::super::preset_dirs(&self.vendor, &self.name),
            "vstpreset",
        );
        out.extend(
            files
                .into_iter()
                .filter(|p| matches!(&p.source, PresetSource::File { file } if preset_class(file).as_deref() == Some(&cid[..]))),
        );
        out
    }

    fn load_preset(&self, preset: &Preset) -> Result<(), String> {
        match &preset.source {
            PresetSource::Program { list, program } => {
                use ParameterInfo_::ParameterFlags_::*;
                let units = self.controller.cast::<IUnitInfo>();
                // The program-change parameter of the unit this list belongs to, or any.
                let unit_of_list = |unit: i32| {
                    let units = units.as_ref()?;
                    unsafe {
                        (0..units.getUnitCount()).find_map(|i| {
                            let mut info: UnitInfo = std::mem::zeroed();
                            (units.getUnitInfo(i, &mut info) == kResultOk
                                && info.id == unit
                                && info.programListId == *list)
                                .then_some(())
                        })
                    }
                };
                let changers: Vec<ParameterInfo> = self
                    .infos()
                    .into_iter()
                    .filter(|i| i.flags & kIsProgramChange != 0)
                    .collect();
                let param = changers
                    .iter()
                    .find(|i| unit_of_list(i.unitId).is_some())
                    .or(changers.first())
                    .ok_or_else(|| format!("{}: its programs can't be chosen", self.name))?;
                let steps = param.stepCount.max(1) as f64;
                self.set_param(param.id, *program as f64 / steps);
                Ok(())
            }
            PresetSource::File { file } => {
                let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
                let chunk = |id: &[u8; 4]| preset_chunk(&bytes, id);
                let ours = chunk(b"Comp")
                    .ok_or_else(|| format!("{}: not a VST3 preset", file.display()))?;
                self.restore(ours, chunk(b"Cont").unwrap_or(&[]))
            }
        }
    }

    fn save_preset(&self, name: &str) -> Result<PathBuf, String> {
        let (ours, theirs) = self
            .states()
            .ok_or("the plugin wouldn't give its settings")?;
        let dir = super::super::preset_dirs(&self.vendor, &self.name).remove(0);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(super::super::preset_file_name(name, "vstpreset"));
        let bytes = write_preset(&hex(&self.cid), &ours, &theirs);
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }

    fn open_editor(&self, mtm: MainThreadMarker) -> Result<Editor, String> {
        let fail = |why: &str| format!("{}: {why}", self.name);
        unsafe {
            let view = ComPtr::from_raw(self.controller.createView(ViewType::kEditor))
                .ok_or_else(|| fail("no editor"))?;
            // The kind of view this platform's windows hold.
            #[cfg(target_os = "macos")]
            let kind = kPlatformTypeNSView;
            #[cfg(windows)]
            let kind = kPlatformTypeHWND;
            if view.isPlatformTypeSupported(kind) != kResultTrue {
                return Err(fail("its editor can't open in a window here"));
            }
            let mut rect: ViewRect = std::mem::zeroed();
            view.getSize(&mut rect);
            let size = (
                (rect.right - rect.left) as f64,
                (rect.bottom - rect.top) as f64,
            );
            let window = Window::new(mtm, &self.name, size);
            let frame = ComWrapper::new(PlugFrame {
                window: window.clone(),
            });
            view.setFrame(com_ptr::<_, IPlugFrame>(&frame));
            if view.attached(window.parent(), kind) != kResultOk {
                return Err(fail("editor wouldn't attach"));
            }
            window.show();
            Ok(Editor::new(window, move || {
                view.removed();
                drop(frame);
            }))
        }
    }
}

impl Vst3Plugin {
    pub(crate) fn infos(&self) -> Vec<ParameterInfo> {
        unsafe {
            (0..self.controller.getParameterCount())
                .filter_map(|i| {
                    let mut info: ParameterInfo = std::mem::zeroed();
                    (self.controller.getParameterInfo(i, &mut info) == kResultOk).then_some(info)
                })
                .collect()
        }
    }

    /// The component's settings and the controller's.
    pub(crate) fn states(&self) -> Option<(Vec<u8>, Vec<u8>)> {
        unsafe {
            let ours = ComWrapper::new(MemoryStream::default());
            if self.component.getState(com_ptr::<_, IBStream>(&ours)) != kResultOk {
                return None;
            }
            let theirs = ComWrapper::new(MemoryStream::default());
            self.controller.getState(com_ptr::<_, IBStream>(&theirs));
            Some((ours.bytes(), theirs.bytes()))
        }
    }

    /// Gives the component and controller their settings back, as a preset or saved state does.
    pub(crate) fn restore(&self, ours: &[u8], theirs: &[u8]) -> Result<(), String> {
        let stream = |bytes: &[u8]| ComWrapper::new(MemoryStream::from(bytes.to_vec()));
        unsafe {
            let s = stream(ours);
            if self.component.setState(com_ptr::<_, IBStream>(&s)) != kResultOk {
                return Err(format!("{}: it didn't take the settings", self.name));
            }
            let s = stream(ours);
            self.controller
                .setComponentState(com_ptr::<_, IBStream>(&s));
            if !theirs.is_empty() {
                let s = stream(theirs);
                self.controller.setState(com_ptr::<_, IBStream>(&s));
            }
        }
        Ok(())
    }
}

/// A plugin's state as one blob: the component's, then the controller's.
pub(crate) fn pack(component: &[u8], controller: &[u8]) -> Vec<u8> {
    let mut out = STATE_MAGIC.to_vec();
    out.extend((component.len() as u32).to_le_bytes());
    out.extend(component);
    out.extend(controller);
    out
}

pub(crate) fn unpack(blob: &[u8]) -> Option<(&[u8], &[u8])> {
    let rest = blob.strip_prefix(STATE_MAGIC)?;
    let (len, rest) = rest.split_first_chunk::<4>()?;
    let len = u32::from_le_bytes(*len) as usize;
    (len <= rest.len()).then(|| rest.split_at(len))
}
