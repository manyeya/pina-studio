//! Hosting VST3 plugins (macOS): load a `.vst3` bundle, run its processor on the audio thread,
//! and open its editor in a window of its own.

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
use crate::{project::PluginRef, scan::PluginInfo};

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

/// A loaded `.vst3` bundle.
// ponytail: never unloaded (no bundleExit); add when the plugin browser rescans.
pub struct Module {
    factory: ComPtr<IPluginFactory>,
}

pub struct ClassInfo {
    pub name: String,
    pub category: String,
    pub vendor: String,
    /// An instrument rather than an effect, by its subcategories.
    pub instrument: bool,
    cid: TUID,
}

impl Module {
    pub fn load(path: &Path) -> Result<Module, String> {
        let fail = |why: &str| format!("{}: {why}", path.display());
        unsafe {
            let p =
                CString::new(path.as_os_str().as_encoded_bytes()).map_err(|_| fail("bad path"))?;
            let url = CFURLCreateFromFileSystemRepresentation(
                ptr::null(),
                p.as_ptr().cast(),
                p.as_bytes().len() as isize,
                1,
            );
            if url.is_null() {
                return Err(fail("bad path"));
            }
            let bundle = CFBundleCreate(ptr::null(), url);
            CFRelease(url);
            if bundle.is_null() {
                return Err(fail("not a bundle"));
            }
            if CFBundleLoadExecutable(bundle) == 0 {
                return Err(fail("couldn't load its binary (wrong architecture?)"));
            }
            let entry: unsafe extern "C" fn(*const c_void) -> bool =
                symbol(bundle, c"bundleEntry").ok_or_else(|| fail("no bundleEntry"))?;
            let get_factory: unsafe extern "C" fn() -> *mut IPluginFactory =
                symbol(bundle, c"GetPluginFactory").ok_or_else(|| fail("no GetPluginFactory"))?;
            if !entry(bundle) {
                return Err(fail("bundleEntry failed"));
            }
            let factory =
                ComPtr::from_raw(get_factory()).ok_or_else(|| fail("no plugin factory"))?;
            Ok(Module { factory })
        }
    }

    pub fn classes(&self) -> Vec<ClassInfo> {
        let factory2 = self.factory.cast::<IPluginFactory2>();
        unsafe {
            (0..self.factory.countClasses())
                .filter_map(|i| {
                    if let Some(f) = &factory2 {
                        let mut info: PClassInfo2 = std::mem::zeroed();
                        if f.getClassInfo2(i, &mut info) == kResultOk {
                            return Some(ClassInfo {
                                name: text(&info.name),
                                category: text(&info.category),
                                vendor: text(&info.vendor),
                                instrument: text(&info.subCategories).contains("Instrument"),
                                cid: info.cid,
                            });
                        }
                    }
                    let mut info: PClassInfo = std::mem::zeroed();
                    (self.factory.getClassInfo(i, &mut info) == kResultOk).then(|| ClassInfo {
                        name: text(&info.name),
                        category: text(&info.category),
                        vendor: String::new(),
                        instrument: false,
                        cid: info.cid,
                    })
                })
                .collect()
        }
    }

    unsafe fn create<I: Interface>(&self, cid: &TUID) -> Option<ComPtr<I>> {
        let mut obj = ptr::null_mut();
        let iid = I::IID;
        unsafe {
            (self
                .factory
                .createInstance(cid.as_ptr(), iid.as_ptr().cast(), &mut obj)
                == kResultOk)
                .then(|| ComPtr::from_raw(obj.cast()))
                .flatten()
        }
    }
}

unsafe fn symbol<F: Copy>(bundle: *const c_void, name: &CStr) -> Option<F> {
    const UTF8: u32 = 0x0800_0100;
    unsafe {
        let s = CFStringCreateWithCString(ptr::null(), name.as_ptr(), UTF8);
        let f = CFBundleGetFunctionPointerForName(bundle, s);
        CFRelease(s);
        (!f.is_null()).then(|| std::mem::transmute_copy(&f))
    }
}

fn text(chars: &[c_char]) -> String {
    let bytes: Vec<u8> = chars
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn com_ptr<C: Class, I: Interface>(obj: &ComWrapper<C>) -> *mut I {
    obj.as_com_ref::<I>().unwrap().as_ptr()
}

fn text16(chars: &[TChar]) -> String {
    let end = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
    String::from_utf16_lossy(&chars[..end])
}

/// The plugins in a `.vst3` bundle, opening it in this process.
pub fn inspect(bundle: &Path) -> Result<Vec<PluginInfo>, String> {
    let module = Module::load(bundle)?;
    Ok(module
        .classes()
        .into_iter()
        .filter(|c| c.category == AUDIO_MODULE)
        .map(|c| PluginInfo {
            format: crate::project::Format::Vst3,
            path: bundle.to_path_buf(),
            class: c.name,
            uid: None,
            vendor: c.vendor,
            instrument: c.instrument,
        })
        .collect())
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

/// A VST3 plugin's main-thread side: its controller and editor, and its saved state.
pub struct Vst3Plugin {
    pub name: String,
    vendor: String,
    sidechain: bool,
    cid: TUID,
    component: ComPtr<IComponent>,
    controller: ComPtr<IEditController>,
    _host: ComWrapper<HostApplication>,
    _handler: ComWrapper<ComponentHandler>,
}

/// A VST3 plugin's audio-thread side.
pub struct Vst3Processor {
    processor: ComPtr<IAudioProcessor>,
    component: ComPtr<IComponent>,
    controller: ComPtr<IEditController>,
    /// The component and controller, when they're separate objects talking to each other.
    links: Option<(ComPtr<IConnectionPoint>, ComPtr<IConnectionPoint>)>,
    events: ComWrapper<EventList>,
    params: ComWrapper<ParameterChanges>,
    /// Where the plugin reports its own changes (meters, MIDI it makes), as a host must give it.
    // ponytail: read by nothing; forward them when something shows plugin meters or MIDI out.
    out_events: ComWrapper<EventList>,
    out_params: ComWrapper<ParameterChanges>,
    edits: EditReceiver,
    /// Automation for the next block: a parameter, and its value at the block's start and end.
    automation: [(ParamID, ParamValue, ParamValue); AUTOMATED],
    automated: usize,
    // Channel buffers, their pointers, and the bus structs that point at those.
    _buffers: Vec<Vec<f32>>,
    _channels: Vec<Vec<*mut f32>>,
    inputs: Vec<AudioBusBuffers>,
    outputs: Vec<AudioBusBuffers>,
    context: ProcessContext,
    max_block: usize,
    /// The parameter each MIDI controller moves (0-127, then 128 aftertouch, 129 pitch bend), as
    /// the plugin maps them.
    midi: [Option<ParamID>; 130],
    /// Whether its second input is a sidechain, and switched on.
    sidechain: bool,
}

// The COM pointers are only used from the audio thread once the processor moves there.
unsafe impl Send for Vst3Processor {}

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
                    // One unnamed program is no list at all (JUCE plugins all have it).
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
        let files = super::preset_files(&super::preset_dirs(&self.vendor, &self.name), "vstpreset");
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
        let dir = super::preset_dirs(&self.vendor, &self.name).remove(0);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(super::preset_file_name(name, "vstpreset"));
        let bytes = write_preset(&hex(&self.cid), &ours, &theirs);
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }

    fn open_editor(&self, mtm: MainThreadMarker) -> Result<Editor, String> {
        let fail = |why: &str| format!("{}: {why}", self.name);
        unsafe {
            let view = ComPtr::from_raw(self.controller.createView(ViewType::kEditor))
                .ok_or_else(|| fail("no editor"))?;
            if view.isPlatformTypeSupported(kPlatformTypeNSView) != kResultTrue {
                return Err(fail("editor isn't an NSView"));
            }
            let mut rect: ViewRect = std::mem::zeroed();
            view.getSize(&mut rect);
            let size = (
                (rect.right - rect.left) as f64,
                (rect.bottom - rect.top) as f64,
            );
            let window = editor_window(mtm, &self.name, size);
            let frame = ComWrapper::new(PlugFrame {
                window: window.clone(),
            });
            view.setFrame(com_ptr::<_, IPlugFrame>(&frame));
            let content = window
                .contentView()
                .ok_or_else(|| fail("window has no content view"))?;
            if view.attached(
                Retained::as_ptr(&content) as *mut c_void,
                kPlatformTypeNSView,
            ) != kResultOk
            {
                return Err(fail("editor wouldn't attach"));
            }
            window.makeKeyAndOrderFront(None);
            Ok(Editor::new(window, move || {
                view.removed();
                drop(frame);
            }))
        }
    }
}

impl Vst3Plugin {
    fn infos(&self) -> Vec<ParameterInfo> {
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
    fn states(&self) -> Option<(Vec<u8>, Vec<u8>)> {
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
    fn restore(&self, ours: &[u8], theirs: &[u8]) -> Result<(), String> {
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

/// A class id as `.vstpreset` files write it: its 16 bytes in hex.
fn hex(cid: &TUID) -> String {
    cid.iter().map(|&b| format!("{:02X}", b as u8)).collect()
}

/// The class id a `.vstpreset` file is for, from its header.
fn preset_class(file: &Path) -> Option<String> {
    use std::io::Read;
    let mut head = [0u8; 40];
    std::fs::File::open(file).ok()?.read_exact(&mut head).ok()?;
    (&head[..4] == b"VST3").then(|| String::from_utf8_lossy(&head[8..40]).to_uppercase())
}

/// A chunk of a `.vstpreset` file: "Comp" (the component's settings) or "Cont" (the
/// controller's), found through the chunk list its header points at.
fn preset_chunk<'a>(bytes: &'a [u8], id: &[u8; 4]) -> Option<&'a [u8]> {
    let le32 = |at: usize| Some(i32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let le64 = |at: usize| Some(i64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?));
    if bytes.get(..4)? != b"VST3" {
        return None;
    }
    let list = le64(40)? as usize;
    if bytes.get(list..list + 4)? != b"List" {
        return None;
    }
    (0..le32(list + 4)?.max(0) as usize).find_map(|i| {
        let at = list + 8 + i * 20;
        (bytes.get(at..at + 4)? == id).then_some(())?;
        let (offset, size) = (le64(at + 4)? as usize, le64(at + 12)? as usize);
        bytes.get(offset..offset + size)
    })
}

/// A `.vstpreset` file holding a component's and controller's settings.
fn write_preset(class: &str, ours: &[u8], theirs: &[u8]) -> Vec<u8> {
    let mut out = b"VST3".to_vec();
    out.extend(1i32.to_le_bytes());
    out.extend(class.as_bytes());
    let list_at = 48 + ours.len() + theirs.len();
    out.extend((list_at as i64).to_le_bytes());
    out.extend(ours);
    out.extend(theirs);
    out.extend(b"List");
    out.extend(2i32.to_le_bytes());
    for (id, offset, size) in [
        (b"Comp", 48, ours.len()),
        (b"Cont", 48 + ours.len(), theirs.len()),
    ] {
        out.extend(id);
        out.extend((offset as i64).to_le_bytes());
        out.extend((size as i64).to_le_bytes());
    }
    out
}

impl Drop for Vst3Processor {
    fn drop(&mut self) {
        unsafe {
            self.processor.setProcessing(0);
            self.component.setActive(0);
            if let Some((a, b)) = &self.links {
                a.disconnect(b.as_ptr());
                b.disconnect(a.as_ptr());
                self.controller.terminate();
            }
            self.component.terminate();
        }
    }
}

const STATE_MAGIC: &[u8; 4] = b"TMPS";

/// A plugin's state as one blob: the component's, then the controller's.
fn pack(component: &[u8], controller: &[u8]) -> Vec<u8> {
    let mut out = STATE_MAGIC.to_vec();
    out.extend((component.len() as u32).to_le_bytes());
    out.extend(component);
    out.extend(controller);
    out
}

fn unpack(blob: &[u8]) -> Option<(&[u8], &[u8])> {
    let rest = blob.strip_prefix(STATE_MAGIC)?;
    let (len, rest) = rest.split_first_chunk::<4>()?;
    let len = u32::from_le_bytes(*len) as usize;
    (len <= rest.len()).then(|| rest.split_at(len))
}

/// Lets an editor resize its window.
pub struct PlugFrame {
    window: Retained<NSWindow>,
}

impl Class for PlugFrame {
    type Interfaces = (IPlugFrame,);
}

impl IPlugFrameTrait for PlugFrame {
    unsafe fn resizeView(&self, view: *mut IPlugView, size: *mut ViewRect) -> tresult {
        let (Some(view), false) = (unsafe { ComRef::from_raw(view) }, size.is_null()) else {
            return kInvalidArgument;
        };
        let r = unsafe { *size };
        self.window.setContentSize(NSSize::new(
            (r.right - r.left) as f64,
            (r.bottom - r.top) as f64,
        ));
        unsafe { view.onSize(size) };
        kResultOk
    }
}

impl ProcessorApi for Vst3Processor {
    fn latency(&self) -> u32 {
        unsafe { self.processor.getLatencySamples() }
    }

    fn set_transport(&mut self, playing: bool, position: u64, tempo: f64) {
        let c = &mut self.context;
        c.tempo = tempo;
        c.projectTimeSamples = position as i64;
        c.projectTimeMusic = position as f64 / c.sampleRate * tempo / 60.0;
        let playing = if playing { kPlaying } else { 0 };
        c.state = (kTempoValid | kTimeSigValid | kProjectTimeMusicValid | playing) as _;
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

    fn pending(&self) -> bool {
        self.edits.pending()
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
        self.events.clear();
        self.params.clear();
        // Notes and controls, merged in time; a note is known by its pitch, so expression finds it.
        let (mut n, mut c) = (notes.iter().peekable(), controls.iter().peekable());
        loop {
            let note_first = match (n.peek(), c.peek()) {
                (Some(a), Some(b)) => a.offset <= b.offset,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            if note_first {
                self.events.push(note_event(n.next().unwrap()));
            } else {
                let c = c.next().unwrap();
                match c.kind {
                    ControlKind::Cc { number, value } => {
                        self.midi_param(number as usize, c.offset, value as f64)
                    }
                    ControlKind::Aftertouch(v) => self.midi_param(128, c.offset, v as f64),
                    ControlKind::PitchBend(v) => {
                        self.midi_param(129, c.offset, (v as f64 + 1.0) / 2.0)
                    }
                    ControlKind::PolyPressure { pitch, value } => {
                        let mut e: Event = unsafe { std::mem::zeroed() };
                        e.sampleOffset = c.offset as int32;
                        e.r#type = kPolyPressureEvent as _;
                        e.__field0.polyPressure = PolyPressureEvent {
                            channel: 0,
                            pitch: pitch as _,
                            pressure: value.clamp(0.0, 1.0),
                            noteId: pitch as _,
                        };
                        self.events.push(e);
                    }
                    ControlKind::Expression { pitch, kind, value } => {
                        let (type_id, value) = expression(kind, value as f64);
                        let mut e: Event = unsafe { std::mem::zeroed() };
                        e.sampleOffset = c.offset as int32;
                        e.r#type = kNoteExpressionValueEvent as _;
                        e.__field0.noteExpressionValue = NoteExpressionValueEvent {
                            typeId: type_id,
                            noteId: pitch as _,
                            value,
                        };
                        self.events.push(e);
                    }
                }
            }
        }
        let params = &self.params;
        self.edits.drain(|id, value| params.add(id, 0, value));
        // Automation after edits: where a parameter has both, automation wins.
        for &(id, start, end) in &self.automation[..std::mem::take(&mut self.automated)] {
            self.params.add(id, 0, start);
            self.params.add(id, frames.saturating_sub(1) as int32, end);
        }
        self.out_events.clear();
        self.out_params.clear();
        for b in &mut self.outputs {
            b.silenceFlags = 0;
        }
        for (i, b) in self.inputs.iter().enumerate() {
            for c in 0..b.numChannels as usize {
                let dst = unsafe {
                    std::slice::from_raw_parts_mut(*b.__field0.channelBuffers32.add(c), frames)
                };
                let src = match i {
                    0 => buf.get(c).map(|s| &s[..]),
                    1 => sidechain.map(|s| s[c.min(1)]),
                    _ => None,
                };
                match src {
                    Some(src) => dst.copy_from_slice(&src[..frames]),
                    None => dst.fill(0.0),
                }
            }
        }
        let mut data = ProcessData {
            processMode: kRealtime as _,
            symbolicSampleSize: kSample32 as _,
            numSamples: frames as int32,
            numInputs: self.inputs.len() as int32,
            numOutputs: self.outputs.len() as int32,
            inputs: if self.inputs.is_empty() {
                ptr::null_mut()
            } else {
                self.inputs.as_mut_ptr()
            },
            outputs: self.outputs.as_mut_ptr(),
            inputParameterChanges: com_ptr::<_, IParameterChanges>(&self.params),
            outputParameterChanges: com_ptr::<_, IParameterChanges>(&self.out_params),
            inputEvents: com_ptr::<_, IEventList>(&self.events),
            outputEvents: com_ptr::<_, IEventList>(&self.out_events),
            processContext: &mut self.context,
        };
        unsafe { self.processor.process(&mut data) };
        self.context.projectTimeSamples += frames as i64;
        self.context.continousTimeSamples += frames as i64;
        // Mono output goes to both sides.
        let out = &self.outputs[0];
        for (c, dst) in buf.into_iter().enumerate() {
            let c = c.min(out.numChannels as usize - 1);
            let src = unsafe {
                std::slice::from_raw_parts(*out.__field0.channelBuffers32.add(c), frames)
            };
            dst[..frames].copy_from_slice(src);
        }
    }
}

impl Vst3Processor {
    /// A MIDI controller's move, as a change to the parameter the plugin maps it to (if any).
    fn midi_param(&self, controller: usize, offset: u32, value: f64) {
        if let Some(Some(id)) = self.midi.get(controller) {
            self.params.add(*id, offset as int32, value.clamp(0.0, 1.0));
        }
    }
}

/// A note on or off, its id its pitch.
fn note_event(n: &Note) -> Event {
    let mut e: Event = unsafe { std::mem::zeroed() };
    e.sampleOffset = n.offset as int32;
    if n.velocity > 0.0 {
        e.r#type = kNoteOnEvent as _;
        e.__field0.noteOn = NoteOnEvent {
            channel: 0,
            pitch: n.pitch as _,
            tuning: 0.0,
            velocity: n.velocity,
            length: 0,
            noteId: n.pitch as _,
        };
    } else {
        e.r#type = kNoteOffEvent as _;
        e.__field0.noteOff = NoteOffEvent {
            channel: 0,
            pitch: n.pitch as _,
            velocity: 0.0,
            noteId: n.pitch as _,
            tuning: 0.0,
        };
    }
    e
}

/// A note expression's VST3 type, and its value as VST3 has it, 0 to 1.
fn expression(kind: ExpressionKind, value: f64) -> (NoteExpressionTypeID, f64) {
    use NoteExpressionTypeIDs_::*;
    let (id, v) = match kind {
        ExpressionKind::Tuning => (kTuningTypeID, 0.5 + value / 240.0),
        // 0.25 is as played; 1 is 12 dB up.
        ExpressionKind::Volume => (kVolumeTypeID, value / 4.0),
        ExpressionKind::Pan => (kPanTypeID, (value + 1.0) / 2.0),
        ExpressionKind::Brightness => (kBrightnessTypeID, value),
        ExpressionKind::Vibrato => (kVibratoTypeID, value),
        ExpressionKind::Expression => (kExpressionTypeID, value),
    };
    (id as _, v.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With a real instrument, which CI doesn't have:
    /// `TEMPO_TEST_VST3="/path/Some Synth.vst3" cargo test -p tempo-engine -- --ignored`
    #[test]
    #[ignore]
    fn a_real_instrument_plays_saves_restores_and_shuts_down() {
        let path = std::env::var("TEMPO_TEST_VST3").expect("set TEMPO_TEST_VST3");
        let module = Module::load(Path::new(&path)).unwrap();
        let classes = module.classes();
        let class = classes.iter().find(|c| c.category == AUDIO_MODULE).unwrap();
        assert!(class.instrument, "{} isn't an instrument", class.name);
        let (plugin, mut processor) =
            Vst3Plugin::instantiate(&module, class, 48_000.0, 512, None).unwrap();
        let (mut l, mut r) = (vec![0f32; 512], vec![0f32; 512]);
        let note = Note {
            offset: 0,
            pitch: 60,
            velocity: 1.0,
        };
        processor.process([&mut l, &mut r], 512, &[note], &[], None);
        let mut loudest = 0f32;
        for _ in 0..20 {
            processor.process([&mut l, &mut r], 512, &[], &[], None);
            loudest = l.iter().fold(loudest, |m, x| m.max(x.abs()));
        }
        assert!(loudest > 1e-3, "{loudest}");

        // Its parameters can be read and turned, as Smart Controls do.
        let params = plugin.params();
        assert!(!params.is_empty());
        let p = &params[0];
        let to = if p.value > 0.5 { 0.25 } else { 0.75 };
        plugin.set_param(p.id, to);
        assert!(
            (plugin.param_value(p.id) - to).abs() < 0.02,
            "{}",
            plugin.param_value(p.id)
        );
        assert!(!plugin.param_text(p.id, to).is_empty());
        plugin.set_param(p.id, p.value);

        let state = plugin.state().unwrap();
        drop(plugin);
        drop(processor);
        let (again, _processor) =
            Vst3Plugin::instantiate(&module, class, 48_000.0, 512, Some(&state)).unwrap();
        assert_eq!(again.state().unwrap(), state);
    }

    #[test]
    fn state_blobs_round_trip() {
        let blob = pack(b"component", b"controller");
        assert_eq!(unpack(&blob), Some((&b"component"[..], &b"controller"[..])));
        assert_eq!(unpack(b"junk"), None);
        assert_eq!(unpack(&blob[..10]), None); // cut short
    }
}
