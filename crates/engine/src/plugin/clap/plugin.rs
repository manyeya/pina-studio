//! The plugin's main-thread side: parameters, state, presets and editor.

use super::*;

/// A CLAP plugin's main-thread side: its parameters, settings, presets and editor.
pub struct ClapPlugin {
    pub name: String,
    pub(crate) vendor: String,
    pub(crate) id: CString,
    pub(crate) library: Library,
    pub(crate) inst: Arc<Instance>,
    pub(crate) plugin: *const clap_plugin,
    pub(crate) params: Option<&'static clap_plugin_params>,
    pub(crate) state: Option<&'static clap_plugin_state>,
    pub(crate) gui: Option<&'static clap_plugin_gui>,
    pub(crate) preset_load: Option<&'static clap_plugin_preset_load>,
    pub(crate) timer: Option<&'static clap_plugin_timer_support>,
    pub(crate) edits: EditSender,
    /// Edits sent to its processor, and those it hasn't applied yet: a value, and its place
    /// among the edits sent. What's set reads back at once, as a VST3 controller's does.
    pub(crate) sent: Cell<u64>,
    pub(crate) waiting: RefCell<HashMap<u32, (f64, u64)>>,
    pub(crate) ranges: RefCell<Vec<Range>>,
    /// What its preset providers found, asked once.
    // ponytail: never asked again; rediscover when presets are saved from its own editor.
    pub(crate) found: RefCell<Option<Rc<Vec<Found>>>>,
    /// Whether it has a second input, for a sidechain.
    pub(crate) sidechain: bool,
}

impl ClapPlugin {
    /// Creates a plugin at `rate`, restoring `state` (from `PluginApi::state`) if given.
    pub(crate) fn instantiate(
        library: Library,
        d: &Descriptor,
        rate: f64,
        max_block: usize,
        state: Option<&[u8]>,
    ) -> Result<(ClapPlugin, ClapProcessor), String> {
        let fail = |why: &str| format!("{}: {why}", d.name);
        let inst = Instance::new(&d.name);
        unsafe {
            let factory = &*library.factory;
            let create = factory
                .create_plugin
                .ok_or_else(|| fail("can't be created"))?;
            let plugin = create(library.factory, &inst.host, d.id.as_ptr());
            if plugin.is_null() {
                return Err(fail("couldn't create it"));
            }
            // From here, `inst` destroys it when it goes.
            inst.plugin.store(plugin.cast_mut(), SeqCst);
            let p = &*plugin;
            if !p.init.is_some_and(|init| init(plugin)) {
                return Err(fail("init failed"));
            }
            let params = extension::<clap_plugin_params>(plugin, &[CLAP_EXT_PARAMS]);
            let state_ext = extension::<clap_plugin_state>(plugin, &[CLAP_EXT_STATE]);
            if let (Some(bytes), Some(s)) = (state, state_ext) {
                load_state(plugin, s, bytes);
            }

            // Its audio ports, by their channels: the first input is the main one (if it has
            // one), the second a sidechain; the first output is the main one.
            let ports = extension::<clap_plugin_audio_ports>(plugin, &[CLAP_EXT_AUDIO_PORTS]);
            let channels = |input: bool| -> Vec<u32> {
                let Some((count, get)) = ports.and_then(|p| Some((p.count?, p.get?))) else {
                    return Vec::new();
                };
                (0..count(plugin, input))
                    .map(|i| {
                        let mut info: clap_audio_port_info = std::mem::zeroed();
                        match get(plugin, i, input, &mut info) {
                            true => info.channel_count,
                            false => 0,
                        }
                    })
                    .collect()
            };
            let (ins, outs) = (channels(true), channels(false));
            if outs.first().is_none_or(|&c| c == 0) {
                return Err(fail("no audio output"));
            }
            // CLAP's own note events if it takes them, else MIDI.
            let notes = extension::<clap_plugin_note_ports>(plugin, &[CLAP_EXT_NOTE_PORTS]);
            let dialects = notes
                .and_then(|n| {
                    let (count, get) = (n.count?, n.get?);
                    let mut info: clap_note_port_info = std::mem::zeroed();
                    (count(plugin, true) > 0 && get(plugin, 0, true, &mut info))
                        .then_some(info.supported_dialects)
                })
                .unwrap_or(CLAP_NOTE_DIALECT_CLAP);
            let midi_notes =
                dialects & CLAP_NOTE_DIALECT_CLAP == 0 && dialects & CLAP_NOTE_DIALECT_MIDI != 0;

            if !p
                .activate
                .is_some_and(|a| a(plugin, rate, 1, max_block as u32))
            {
                return Err(fail("activate failed"));
            }
            inst.active.store(true, SeqCst);
            // Asked now: it's for the main thread, while active.
            let latency = extension::<clap_plugin_latency>(plugin, &[CLAP_EXT_LATENCY])
                .and_then(|l| l.get)
                .map_or(0, |get| get(plugin));

            // Stepped parameters (switches, types, modes) end a block's edits: see EditReceiver.
            let infos = infos(plugin, params);
            let stepped = infos
                .iter()
                .filter(|i| i.flags & CLAP_PARAM_IS_STEPPED != 0)
                .map(|i| i.id)
                .collect();
            let (edits_tx, edits) = edit_queue(stepped);
            let ranges = ranges(&infos);

            // Every port gets buffers: some plugins write to all of them.
            let mut buffers = Vec::new();
            let mut pointers = Vec::new();
            let mut port = |n: u32| {
                let mut ptrs: Vec<*mut f32> = (0..n)
                    .map(|_| {
                        buffers.push(vec![0f32; max_block]);
                        buffers.last_mut().unwrap().as_mut_ptr()
                    })
                    .collect();
                let b = clap_audio_buffer {
                    data32: ptrs.as_mut_ptr(),
                    data64: ptr::null_mut(),
                    channel_count: n,
                    latency: 0,
                    constant_mask: 0,
                };
                pointers.push(ptrs);
                b
            };
            let inputs: Vec<_> = ins.iter().map(|&n| port(n)).collect();
            let outputs: Vec<_> = outs.iter().map(|&n| port(n)).collect();

            let processor = ClapProcessor {
                inst: inst.clone(),
                plugin,
                edits,
                ranges: ranges.clone(),
                automation: [(0, 0.0, 0.0); AUTOMATED],
                automated: 0,
                events: Vec::with_capacity(EVENTS),
                midi_notes,
                _buffers: buffers,
                _channels: pointers,
                inputs,
                outputs,
                transport: transport(false, 0, 120.0, rate),
                rate,
                playing: false,
                position: 0,
                tempo: 120.0,
                steady: 0,
                max_block,
                latency,
                sidechain: ins.len() > 1,
            };
            let plugin = ClapPlugin {
                name: d.name.clone(),
                vendor: d.vendor.clone(),
                id: d.id.clone(),
                library,
                inst,
                plugin,
                params,
                state: state_ext,
                gui: extension(plugin, &[CLAP_EXT_GUI]),
                preset_load: extension(
                    plugin,
                    &[CLAP_EXT_PRESET_LOAD, CLAP_EXT_PRESET_LOAD_COMPAT],
                ),
                timer: extension(plugin, &[CLAP_EXT_TIMER_SUPPORT]),
                edits: edits_tx,
                sent: Cell::new(0),
                waiting: RefCell::new(HashMap::new()),
                ranges: RefCell::new(ranges),
                found: RefCell::new(None),
                sidechain: ins.len() > 1,
            };
            // Whatever it asked for while loading.
            plugin.idle();
            Ok((plugin, processor))
        }
    }

    /// A parameter's range, as the plugin last described it.
    pub(crate) fn range(&self, id: u32) -> Option<Range> {
        if self.inst.rescan.swap(false, SeqCst) {
            *self.ranges.borrow_mut() = ranges(&infos(self.plugin, self.params));
        }
        range_of(&self.ranges.borrow(), id)
    }

    /// Hands parameter changes (0 to 1) straight to the plugin when its processor isn't running
    /// (nothing plays it yet, as before a render), so they take at once and read back. False
    /// when it's running: they go through the processor then.
    pub(crate) fn flush(&self, changes: &[(u32, f64)]) -> bool {
        let Some(flush) = self.params.and_then(|p| p.flush) else {
            return false;
        };
        if self.inst.running() || !self.inst.enter(FLUSHING) {
            return false;
        }
        let events: Vec<Event> = changes
            .iter()
            .filter_map(|&(id, v)| Some(param_event(0, id, self.range(id)?.plain(v))))
            .collect();
        // While active, a flush is for its audio thread, which isn't running it.
        as_audio(|| unsafe { flush(self.plugin, &input_events(&events), &output_events()) });
        self.inst.leave();
        true
    }

    /// Loads one of the presets its providers found, or a file of its own format.
    pub(crate) fn load_from(
        &self,
        kind: clap_preset_discovery_location_kind,
        location: Option<&CStr>,
        key: Option<&CStr>,
    ) -> Result<(), String> {
        let from = self
            .preset_load
            .and_then(|p| p.from_location)
            .ok_or_else(|| format!("{}: it can't load presets", self.name))?;
        let at = |s: Option<&CStr>| s.map_or(ptr::null(), CStr::as_ptr);
        let ok = unsafe { from(self.plugin, kind, at(location), at(key)) };
        self.idle();
        match ok {
            true => Ok(()),
            false => Err(format!("{}: it couldn't load that preset", self.name)),
        }
    }

    pub(crate) fn found(&self) -> Rc<Vec<Found>> {
        let mut found = self.found.borrow_mut();
        found
            .get_or_insert_with(|| Rc::new(self.library.discover(&self.id)))
            .clone()
    }
}

impl PluginApi for ClapPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn params(&self) -> Vec<PluginParam> {
        let infos = infos(self.plugin, self.params);
        self.inst.rescan.store(false, SeqCst);
        *self.ranges.borrow_mut() = ranges(&infos);
        infos
            .iter()
            .filter(|i| shown(i.flags))
            .map(|i| {
                let range = Range::of(i);
                PluginParam {
                    id: i.id,
                    name: text_in(&i.name),
                    value: self.param_value(i.id),
                    default: range.normal(i.default_value),
                    steps: range.steps(),
                    units: String::new(),
                }
            })
            .collect()
    }

    fn set_param(&self, id: u32, value: f64) {
        let value = value.clamp(0.0, 1.0);
        if !self.flush(&[(id, value)]) {
            self.edits.push(id, value);
            self.sent.set(self.sent.get() + 1);
            self.waiting
                .borrow_mut()
                .insert(id, (value, self.sent.get()));
        }
    }

    fn show_param(&self, _id: u32, _value: f64) {
        // A CLAP plugin's controls and editor are its processor's: automation shows itself.
    }

    fn param_value(&self, id: u32) -> f64 {
        let mut waiting = self.waiting.borrow_mut();
        if let Some(&(value, nth)) = waiting.get(&id) {
            if self.inst.applied.load(SeqCst) < nth {
                return value;
            }
            waiting.remove(&id);
        }
        drop(waiting);
        let (Some(range), Some(get)) = (self.range(id), self.params.and_then(|p| p.get_value))
        else {
            return 0.0;
        };
        let mut v = 0.0;
        match unsafe { get(self.plugin, id, &mut v) } {
            true => range.normal(v),
            false => 0.0,
        }
    }

    fn param_text(&self, id: u32, value: f64) -> String {
        let shown = (self.range(id), self.params.and_then(|p| p.value_to_text));
        if let (Some(range), Some(to_text)) = shown {
            let mut s = [0 as c_char; 256];
            let plain = range.plain(value);
            if unsafe { to_text(self.plugin, id, plain, s.as_mut_ptr(), s.len() as u32) } {
                return text_in(&s);
            }
        }
        format!("{:.0}%", value * 100.0)
    }

    fn param_from_text(&self, id: u32, text: &str) -> Option<f64> {
        let range = self.range(id)?;
        let from_text = self.params?.text_to_value?;
        let text = CString::new(text).ok()?;
        let mut v = 0.0;
        unsafe { from_text(self.plugin, id, text.as_ptr(), &mut v) }.then(|| range.normal(v))
    }

    fn state(&self) -> Option<Vec<u8>> {
        save_state(self.plugin, self.state?)
    }

    fn set_state(&self, state: &[u8]) -> Result<(), String> {
        let ext = self
            .state
            .ok_or_else(|| format!("{}: it has no settings to set", self.name))?;
        let ok = load_state(self.plugin, ext, state);
        self.idle();
        match ok {
            true => Ok(()),
            false => Err(format!("{}: it didn't take the settings", self.name)),
        }
    }

    fn presets(&self) -> Vec<Preset> {
        // Those its providers found: files of its own as files, the rest (built in, or one of
        // several in a file) as programs, numbered in order.
        let mut program = 0;
        let mut out: Vec<Preset> = self
            .found()
            .iter()
            .map(|f| {
                let source = f.file().map_or_else(
                    || {
                        program += 1;
                        PresetSource::Program {
                            list: 0,
                            program: program - 1,
                        }
                    },
                    |file| PresetSource::File { file },
                );
                Preset {
                    name: f.name.clone(),
                    source,
                }
            })
            .collect();
        let dirs = super::super::preset_dirs(&self.vendor, &self.name);
        out.extend(super::super::preset_files(&dirs, EXTENSION));
        out
    }

    fn load_preset(&self, preset: &Preset) -> Result<(), String> {
        match &preset.source {
            PresetSource::File { file } if file.extension() == Some(OsStr::new(EXTENSION)) => {
                let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
                self.set_state(&bytes)
            }
            PresetSource::File { file } => {
                let path = CString::new(file.as_os_str().as_bytes())
                    .map_err(|_| format!("{}: bad path", file.display()))?;
                self.load_from(CLAP_PRESET_DISCOVERY_LOCATION_FILE, Some(&path), None)
            }
            PresetSource::Program { program, .. } => {
                let found = self.found();
                let f = found
                    .iter()
                    .filter(|f| f.file().is_none())
                    .nth(*program as usize)
                    .ok_or_else(|| format!("{}: no preset {program}", self.name))?;
                self.load_from(f.kind, f.location.as_deref(), f.key.as_deref())
            }
        }
    }

    fn save_preset(&self, name: &str) -> Result<PathBuf, String> {
        let bytes = self
            .state()
            .ok_or("the plugin wouldn't give its settings")?;
        let dir = super::super::preset_dirs(&self.vendor, &self.name).remove(0);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(super::super::preset_file_name(name, EXTENSION));
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }

    fn open_editor(&self, mtm: MainThreadMarker) -> Result<Editor, String> {
        let fail = |why: &str| format!("{}: {why}", self.name);
        let gui = self.gui.ok_or_else(|| fail("no editor"))?;
        let api = CLAP_WINDOW_API_COCOA.as_ptr();
        let plugin = self.plugin;
        unsafe {
            if !gui.is_api_supported.is_some_and(|f| f(plugin, api, false)) {
                return Err(fail("editor isn't an NSView"));
            }
            if self.inst.gui.load(SeqCst) {
                return Err(fail("its editor is already open"));
            }
            let (Some(create), Some(destroy), Some(get_size), Some(set_parent)) =
                (gui.create, gui.destroy, gui.get_size, gui.set_parent)
            else {
                return Err(fail("no editor"));
            };
            if !create(plugin, api, false) {
                return Err(fail("its editor wouldn't open"));
            }
            self.inst.gui.store(true, SeqCst);
            let closing = |why: &str| {
                destroy(plugin);
                self.inst.gui.store(false, SeqCst);
                fail(why)
            };
            // Cocoa sizes are in points, so there's no scale to set.
            let (mut w, mut h) = (0u32, 0u32);
            if !get_size(plugin, &mut w, &mut h) {
                return Err(closing("editor has no size"));
            }
            let window = editor_window(mtm, &self.name, (w as f64, h as f64));
            let Some(content) = window.contentView() else {
                return Err(closing("window has no content view"));
            };
            let parent = clap_window {
                api,
                specific: clap_window_handle {
                    cocoa: Retained::as_ptr(&content) as *mut c_void,
                },
            };
            if !set_parent(plugin, &parent) {
                return Err(closing("editor wouldn't attach"));
            }
            if let Some(show) = gui.show {
                show(plugin);
            }
            *self.inst.window.lock().unwrap() = Some(EditorWindow(window.clone()));
            window.makeKeyAndOrderFront(None);
            let inst = self.inst.clone();
            Ok(Editor::new(window, move || {
                inst.window.lock().unwrap().take();
                if inst.gui.swap(false, SeqCst) {
                    if let Some(hide) = gui.hide {
                        hide(plugin);
                    }
                    destroy(plugin);
                }
            }))
        }
    }

    fn sidechain(&self) -> bool {
        self.sidechain
    }

    fn idle(&self) {
        let inst = &self.inst;
        if inst.callback.swap(false, SeqCst)
            && let Some(f) = unsafe { (*self.plugin).on_main_thread }
        {
            unsafe { f(self.plugin) };
        }
        // A flush it asked for: its processor does it when running, else it's done here.
        if inst.flush.load(SeqCst) && self.flush(&[]) {
            inst.flush.store(false, SeqCst);
        }
        inst.resize_window();
        if let Some(on_timer) = self.timer.and_then(|t| t.on_timer) {
            let now = Instant::now();
            let due: Vec<clap_id> = inst
                .timers
                .lock()
                .unwrap()
                .iter_mut()
                .filter(|t| t.due <= now)
                .map(|t| {
                    t.due = now + t.period;
                    t.id
                })
                .collect();
            for id in due {
                unsafe { on_timer(self.plugin, id) };
            }
        }
    }
}

/// Its settings, from its state extension.
pub(crate) fn save_state(plugin: *const clap_plugin, state: &clap_plugin_state) -> Option<Vec<u8>> {
    let save = state.save?;
    let mut bytes: Vec<u8> = Vec::new();
    let stream = clap_ostream {
        ctx: ptr::from_mut(&mut bytes).cast(),
        write: Some(write_stream),
    };
    unsafe { save(plugin, &stream) }.then_some(bytes)
}

pub(crate) fn load_state(
    plugin: *const clap_plugin,
    state: &clap_plugin_state,
    bytes: &[u8],
) -> bool {
    let Some(load) = state.load else {
        return false;
    };
    let mut rest = bytes;
    let stream = clap_istream {
        ctx: ptr::from_mut(&mut rest).cast(),
        read: Some(read_stream),
    };
    unsafe { load(plugin, &stream) }
}

pub(crate) unsafe extern "C" fn write_stream(
    stream: *const clap_ostream,
    buffer: *const c_void,
    size: u64,
) -> i64 {
    let bytes = unsafe { &mut *(*stream).ctx.cast::<Vec<u8>>() };
    if size > 0 {
        bytes.extend_from_slice(unsafe { slice::from_raw_parts(buffer.cast(), size as usize) });
    }
    size as i64
}

pub(crate) unsafe extern "C" fn read_stream(
    stream: *const clap_istream,
    buffer: *mut c_void,
    size: u64,
) -> i64 {
    let rest = unsafe { &mut *(*stream).ctx.cast::<&[u8]>() };
    let n = (size as usize).min(rest.len());
    unsafe { ptr::copy_nonoverlapping(rest.as_ptr(), buffer.cast::<u8>(), n) };
    *rest = &rest[n..];
    n as i64
}
