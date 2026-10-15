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
        unsafe {
            (0..self.factory.countClasses())
                .filter_map(|i| {
                    let mut info: PClassInfo = std::mem::zeroed();
                    (self.factory.getClassInfo(i, &mut info) == kResultOk).then(|| ClassInfo {
                        name: text(&info.name),
                        category: text(&info.category),
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

/// A plugin's main-thread side: its controller and editor.
// ponytail: no teardown; plugins live until the process exits. Add with track deletion.
pub struct Plugin {
    pub name: String,
    controller: ComPtr<IEditController>,
    _host: ComWrapper<HostApplication>,
    _handler: ComWrapper<ComponentHandler>,
}

/// A plugin's audio-thread side.
pub struct Processor {
    processor: ComPtr<IAudioProcessor>,
    events: ComWrapper<EventList>,
    params: ComWrapper<ParameterChanges>,
    edits: rtrb::Consumer<(ParamID, ParamValue)>,
    // Channel buffers, their pointers, and the bus structs that point at those.
    _buffers: Vec<Vec<f32>>,
    _channels: Vec<Vec<*mut f32>>,
    inputs: Vec<AudioBusBuffers>,
    outputs: Vec<AudioBusBuffers>,
    context: ProcessContext,
    max_block: usize,
}

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
    pub fn instantiate(
        module: &Module,
        class: &ClassInfo,
        rate: f64,
        max_block: usize,
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
                    }
                    let state = ComWrapper::new(MemoryStream::default());
                    if component.getState(com_ptr::<_, IBStream>(&state)) == kResultOk {
                        state.rewind();
                        c.setComponentState(com_ptr::<_, IBStream>(&state));
                    }
                    c
                }
            };

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
                controller,
                _host: host,
                _handler: handler,
            };
            let processor = Processor {
                processor,
                events: ComWrapper::new(EventList::with_capacity(512)),
                params: ComWrapper::new(ParameterChanges::with_capacity(64)),
                edits,
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

    /// Opens the plugin's editor in a window of its own.
    // ponytail: fixed size unless the plugin asks to resize, and closing only hides it; handle
    // user resizing and `removed()` when the mixer opens and closes editors.
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
                _view: view,
                _frame: frame,
                window,
            })
        }
    }
}

pub struct Editor {
    _view: ComPtr<IPlugView>,
    _frame: ComWrapper<PlugFrame>,
    pub window: Retained<NSWindow>,
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
    /// Tells the plugin where the transport is, before a `process`.
    pub fn set_transport(&mut self, playing: bool, position: u64, tempo: f64) {
        let c = &mut self.context;
        c.tempo = tempo;
        c.projectTimeSamples = position as i64;
        c.projectTimeMusic = position as f64 / c.sampleRate * tempo / 60.0;
        let playing = if playing { kPlaying } else { 0 };
        c.state = (kTempoValid | kTimeSigValid | kProjectTimeMusicValid | playing) as _;
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
            outputParameterChanges: ptr::null_mut(),
            inputEvents: com_ptr::<_, IEventList>(&self.events),
            outputEvents: ptr::null_mut(),
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
