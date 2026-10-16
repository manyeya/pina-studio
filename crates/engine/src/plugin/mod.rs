//! Hosting VST3 plugins (macOS): load a `.vst3` bundle, run its processor on the audio thread,
//! and open its editor in a window of its own.

mod com;

use std::{
    ffi::{CStr, CString, c_char, c_void},
    path::Path,
    ptr,
};

use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::{NSBackingStoreType, NSWindow, NSWindowStyleMask};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
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

/// A plugin parameter: its id, name, and value from 0 to 1.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginParam {
    pub id: u32,
    pub name: String,
    pub value: f64,
}

fn text16(chars: &[TChar]) -> String {
    let end = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
    String::from_utf16_lossy(&chars[..end])
}

/// A plugin's main-thread side: its controller and editor, and its saved state.
pub struct Plugin {
    pub name: String,
    component: ComPtr<IComponent>,
    controller: ComPtr<IEditController>,
    _host: ComWrapper<HostApplication>,
    _handler: ComWrapper<ComponentHandler>,
}

/// A plugin's audio-thread side. Dropping it shuts the plugin down, so it's dropped back on the
/// main thread, after its `Plugin`.
pub struct Processor {
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
    edits: rtrb::Consumer<(ParamID, ParamValue)>,
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
}

/// Most parameters one plugin follows automation for, per block.
pub const AUTOMATED: usize = 32;

// The COM pointers are only used from the audio thread once the processor moves there.
unsafe impl Send for Processor {}

#[derive(Clone, Copy, Default)]
pub struct Note {
    pub offset: u32,
    pub pitch: u8,
    /// 0 is a note off.
    pub velocity: f32,
}

impl Plugin {
    /// Creates a plugin at `rate`, restoring `state` (from `Plugin::state`) if given.
    pub fn instantiate(
        module: &Module,
        class: &ClassInfo,
        rate: f64,
        max_block: usize,
        state: Option<&[u8]>,
    ) -> Result<(Plugin, Processor), String> {
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

            let (edits_tx, edits) = rtrb::RingBuffer::new(1024);
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

            let plugin = Plugin {
                name: class.name.clone(),
                component: component.clone(),
                controller: controller.clone(),
                _host: host,
                _handler: handler,
            };
            let processor = Processor {
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
            };
            Ok((plugin, processor))
        }
    }

    /// The parameters a person can turn: automatable, not hidden, not the bypass.
    pub fn params(&self) -> Vec<PluginParam> {
        use ParameterInfo_::ParameterFlags_::*;
        unsafe {
            (0..self.controller.getParameterCount())
                .filter_map(|i| {
                    let mut info: ParameterInfo = std::mem::zeroed();
                    if self.controller.getParameterInfo(i, &mut info) != kResultOk {
                        return None;
                    }
                    let flags = info.flags;
                    let shown = flags & kCanAutomate != 0 && flags & (kIsHidden | kIsBypass) == 0;
                    shown.then(|| PluginParam {
                        id: info.id,
                        name: text16(&info.title),
                        value: self.controller.getParamNormalized(info.id),
                    })
                })
                .collect()
        }
    }

    /// Sets a parameter (0 to 1), in the controller and, through the edit queue, the processor.
    pub fn set_param(&self, id: u32, value: f64) {
        let value = value.clamp(0.0, 1.0);
        unsafe { self.controller.setParamNormalized(id, value) };
        self._handler.edit(id, value);
    }

    /// Shows a parameter's value (0 to 1) in the controller and editor only.
    pub fn show_param(&self, id: u32, value: f64) {
        unsafe {
            self.controller
                .setParamNormalized(id, value.clamp(0.0, 1.0))
        };
    }

    /// A parameter's value now, from 0 to 1.
    pub fn param_value(&self, id: u32) -> f64 {
        unsafe { self.controller.getParamNormalized(id) }
    }

    /// A parameter's value as the plugin would show it, like "2.40 kHz".
    pub fn param_text(&self, id: u32, value: f64) -> String {
        let mut s: String128 = [0; 128];
        unsafe {
            if self.controller.getParamStringByValue(id, value, &mut s) != kResultOk {
                return format!("{:.0}%", value * 100.0);
            }
        }
        text16(&s)
    }

    /// The plugin's settings, to save with the project and give back to `instantiate`.
    pub fn state(&self) -> Option<Vec<u8>> {
        unsafe {
            let ours = ComWrapper::new(MemoryStream::default());
            if self.component.getState(com_ptr::<_, IBStream>(&ours)) != kResultOk {
                return None;
            }
            let theirs = ComWrapper::new(MemoryStream::default());
            self.controller.getState(com_ptr::<_, IBStream>(&theirs));
            Some(pack(&ours.bytes(), &theirs.bytes()))
        }
    }

    /// Opens the plugin's editor in a window of its own; dropping it closes the window.
    // ponytail: fixed size unless the plugin asks to resize; handle user resizing later.
    pub fn open_editor(&self, mtm: MainThreadMarker) -> Result<Editor, String> {
        let fail = |why: &str| format!("{}: {why}", self.name);
        unsafe {
            let view = ComPtr::from_raw(self.controller.createView(ViewType::kEditor))
                .ok_or_else(|| fail("no editor"))?;
            if view.isPlatformTypeSupported(kPlatformTypeNSView) != kResultTrue {
                return Err(fail("editor isn't an NSView"));
            }
            let mut rect: ViewRect = std::mem::zeroed();
            view.getSize(&mut rect);
            let size = NSSize::new(
                (rect.right - rect.left) as f64,
                (rect.bottom - rect.top) as f64,
            );
            let window = NSWindow::initWithContentRect_styleMask_backing_defer(
                mtm.alloc(),
                NSRect::new(NSPoint::new(200.0, 200.0), size),
                NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::Miniaturizable,
                NSBackingStoreType::Buffered,
                false,
            );
            window.setReleasedWhenClosed(false);
            window.setTitle(&NSString::from_str(&self.name));
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
            Ok(Editor {
                view,
                _frame: frame,
                window,
            })
        }
    }
}

pub struct Editor {
    view: ComPtr<IPlugView>,
    _frame: ComWrapper<PlugFrame>,
    pub window: Retained<NSWindow>,
}

impl Editor {
    /// Brings the window back, if it was closed.
    pub fn show(&self) {
        self.window.makeKeyAndOrderFront(None);
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        unsafe { self.view.removed() };
        self.window.close();
    }
}

impl Drop for Processor {
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

impl Processor {
    /// How late the plugin's output is, in samples.
    pub fn latency(&self) -> u32 {
        unsafe { self.processor.getLatencySamples() }
    }

    /// Tells the plugin where the transport is, before a `process`.
    pub fn set_transport(&mut self, playing: bool, position: u64, tempo: f64) {
        let c = &mut self.context;
        c.tempo = tempo;
        c.projectTimeSamples = position as i64;
        c.projectTimeMusic = position as f64 / c.sampleRate * tempo / 60.0;
        let playing = if playing { kPlaying } else { 0 };
        c.state = (kTempoValid | kTimeSigValid | kProjectTimeMusicValid | playing) as _;
    }

    /// Moves a parameter (0 to 1) across the next block, from `start` to `end`, as automation
    /// does. Past `AUTOMATED` parameters a block, the rest wait.
    pub fn automate(&mut self, id: u32, start: f64, end: f64) {
        if self.automated < AUTOMATED {
            self.automation[self.automated] = (id, start.clamp(0.0, 1.0), end.clamp(0.0, 1.0));
            self.automated += 1;
        }
    }

    /// Runs one block in place: `buf` goes to the main input (if the plugin has one) and comes
    /// back holding the main output. Applies these notes and any edits from the editor.
    /// Allocation-free: safe on the audio thread.
    pub fn process(&mut self, buf: [&mut [f32]; 2], frames: usize, notes: &[Note]) {
        assert!(frames <= self.max_block);
        self.events.clear();
        for n in notes {
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
                    noteId: -1,
                };
            } else {
                e.r#type = kNoteOffEvent as _;
                e.__field0.noteOff = NoteOffEvent {
                    channel: 0,
                    pitch: n.pitch as _,
                    velocity: 0.0,
                    noteId: -1,
                    tuning: 0.0,
                };
            }
            self.events.push(e);
        }
        self.params.clear();
        while let Ok((id, value)) = self.edits.pop() {
            self.params.add(id, 0, value);
        }
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
                match buf.get(c).filter(|_| i == 0) {
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
            Plugin::instantiate(&module, class, 48_000.0, 512, None).unwrap();
        let (mut l, mut r) = (vec![0f32; 512], vec![0f32; 512]);
        let note = Note {
            offset: 0,
            pitch: 60,
            velocity: 1.0,
        };
        processor.process([&mut l, &mut r], 512, &[note]);
        let mut loudest = 0f32;
        for _ in 0..20 {
            processor.process([&mut l, &mut r], 512, &[]);
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
            Plugin::instantiate(&module, class, 48_000.0, 512, Some(&state)).unwrap();
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
