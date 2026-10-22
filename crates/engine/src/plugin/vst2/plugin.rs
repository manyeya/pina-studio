//! The plugin's main-thread side: parameters, state, presets and editor.

use super::*;

/// A VST2 plugin's main-thread side: its parameters, settings, presets and editor.
pub struct Vst2Plugin {
    pub(crate) name: String,
    pub(crate) vendor: String,
    /// Whether inputs 3 and 4 are a sidechain, as VST2 plugins have one.
    pub(crate) sidechain: bool,
    pub(crate) library: Library,
    pub(crate) instance: Arc<Instance>,
    pub(crate) edits: EditSender,
    /// Each parameter's value when it was loaded.
    pub(crate) defaults: Vec<f64>,
    /// A second instance of the plugin, never heard, that shows values the playing one doesn't
    /// have: made the first time it's needed (`None` if it couldn't be).
    pub(crate) twin: OnceCell<Option<Arc<Instance>>>,
}

impl Vst2Plugin {
    /// Sets an opened plugin up to run at `rate`, restoring `state` (from `Plugin::state`) if
    /// given.
    pub(crate) fn start(
        library: Library,
        i: Arc<Instance>,
        bundle: &Path,
        rate: f64,
        max_block: usize,
        state: Option<&[u8]>,
    ) -> Result<(Vst2Plugin, Vst2Processor), String> {
        let (n_in, n_out) = unsafe { ((*i.effect).num_inputs, (*i.effect).num_outputs) };
        if n_out <= 0 {
            return Err("no audio output".into());
        }
        if i.flags() & FLAG_CAN_REPLACING == 0 && unsafe { (*i.effect).process.is_none() } {
            return Err("no way to process audio".into());
        }
        i.call(eff::SET_SAMPLE_RATE, 0, 0, ptr::null_mut(), rate as f32);
        i.call(
            eff::SET_BLOCK_SIZE,
            0,
            max_block as isize,
            ptr::null_mut(),
            0.0,
        );
        i.call(eff::SET_PROCESS_PRECISION, 0, 0, ptr::null_mut(), 0.0);
        let defaults = (0..i.num_params()).map(|p| i.get(p) as f64).collect();
        if let Some((program, settings)) = state.and_then(unpack) {
            if program != i.program() && (0..i.num_programs() as i32).contains(&program) {
                i.set_program(program);
            }
            i.restore(&settings, false);
        }
        // Stepped parameters (switches, types, modes) end a block's edits: see EditReceiver.
        let stepped = (0..i.num_params())
            .filter(|&p| i.properties(p).is_some_and(|props| steps(&props) > 0))
            .collect();
        let (edits_tx, edits) = edit_queue(stepped);
        i.call(eff::MAINS_CHANGED, 0, 1, ptr::null_mut(), 0.0);
        i.call(eff::START_PROCESS, 0, 0, ptr::null_mut(), 0.0);

        let mut buffers: Vec<Vec<f32>> = (0..n_in.max(0) + n_out)
            .map(|_| vec![0f32; max_block])
            .collect();
        let mut pointers: Vec<*mut f32> = buffers.iter_mut().map(|b| b.as_mut_ptr()).collect();
        let outputs = pointers.split_off(n_in.max(0) as usize);
        let plugin = Vst2Plugin {
            name: i.name(bundle),
            vendor: i.string(eff::GET_VENDOR_STRING, 0),
            sidechain: n_in >= 4,
            library,
            instance: i.clone(),
            edits: edits_tx,
            defaults,
            twin: OnceCell::new(),
        };
        let processor = Vst2Processor {
            instance: i,
            edits,
            automation: [(0, 0.0); AUTOMATED],
            automated: 0,
            events: EventList::new(),
            _buffers: buffers,
            inputs: pointers,
            outputs,
            max_block,
            sidechain: n_in >= 4,
        };
        Ok((plugin, processor))
    }

    /// Its settings as a blob for `state`.
    pub(crate) fn snapshot(&self) -> Option<Vec<u8>> {
        let settings = self.instance.settings(false)?;
        Some(pack(self.instance.program(), &settings))
    }

    /// The twin, its parameters set as the playing instance's are.
    // ponytail: settings a plugin keeps outside its parameters (in its chunk) aren't copied;
    // copy the chunk too if a display depends on them.
    pub(crate) fn twin(&self) -> Option<&Instance> {
        let i = &self.instance;
        let twin = self.twin.get_or_init(|| {
            let twin = self.library.open(i.unique_id(), i.rate, i.max_block).ok()?;
            twin.call(eff::SET_SAMPLE_RATE, 0, 0, ptr::null_mut(), i.rate as f32);
            twin.call(
                eff::SET_BLOCK_SIZE,
                0,
                i.max_block as isize,
                ptr::null_mut(),
                0.0,
            );
            Some(twin)
        });
        let twin = twin.as_deref()?;
        for p in 0..i.num_params().min(twin.num_params()) {
            let v = i.value(p) as f32;
            if twin.get(p) != v {
                twin.set(p, v);
            }
        }
        Some(twin)
    }
}

impl PluginApi for Vst2Plugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn sidechain(&self) -> bool {
        self.sidechain
    }

    fn params(&self) -> Vec<PluginParam> {
        let i = &self.instance;
        let n = i.num_params();
        // A plugin that says none can be automated doesn't answer: then all of them can.
        let automatable: Vec<bool> = (0..n)
            .map(|p| i.call(eff::CAN_BE_AUTOMATED, p as i32, 0, ptr::null_mut(), 0.0) != 0)
            .collect();
        let any = automatable.contains(&true);
        (0..n)
            .filter(|&p| !any || automatable[p as usize])
            .map(|p| {
                let props = i.properties(p);
                // Its properties' label is its full name; the name itself may be cut short.
                let name = props
                    .as_ref()
                    .map(|props| text(&props.label))
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| i.string(eff::GET_PARAM_NAME, p as i32));
                PluginParam {
                    id: p,
                    name,
                    value: i.value(p),
                    default: self.defaults.get(p as usize).copied().unwrap_or(0.0),
                    steps: props.as_ref().map_or(0, steps),
                    units: i.string(eff::GET_PARAM_LABEL, p as i32),
                }
            })
            .collect()
    }

    fn set_param(&self, id: u32, value: f64) {
        let value = value.clamp(0.0, 1.0);
        if let Some(s) = self.instance.sent.get(id as usize) {
            s.store(value.to_bits(), Relaxed);
        }
        self.edits.push(id, value);
    }

    fn show_param(&self, _id: u32, _value: f64) {
        // Its editor shows what plays, and automation sets that as it plays.
    }

    fn param_value(&self, id: u32) -> f64 {
        self.instance.value(id)
    }

    /// VST2 shows only a parameter's current value. Another is shown by the host extension
    /// (where the plugin has it), else by the twin set to it: never by touching what
    /// plays.
    fn param_text(&self, id: u32, value: f64) -> String {
        let i = &self.instance;
        if id >= i.num_params() {
            return String::new();
        }
        let units = i.string(eff::GET_PARAM_LABEL, id as i32);
        let with_units = |s: String| match units.is_empty() || s.is_empty() {
            true => s,
            false => format!("{s} {units}"),
        };
        if (value as f32 - i.get(id)).abs() < 1e-6 {
            return with_units(i.string(eff::GET_PARAM_DISPLAY, id as i32));
        }
        let mut buf = [0u8; 256];
        let cockos = i.call(
            eff::VENDOR_SPECIFIC,
            eff::GET_PARAM_DISPLAY,
            id as isize,
            buf.as_mut_ptr().cast(),
            value as f32,
        );
        if cockos == COCKOS_YES {
            return with_units(text(&buf));
        }
        match self.twin() {
            Some(twin) => {
                twin.set(id, value as f32);
                with_units(twin.string(eff::GET_PARAM_DISPLAY, id as i32))
            }
            None => format!("{:.0}%", value * 100.0),
        }
    }

    /// VST2 reads a typed value only by setting the parameter to it, so the twin reads it.
    fn param_from_text(&self, id: u32, text: &str) -> Option<f64> {
        if id >= self.instance.num_params() {
            return None;
        }
        let text = CString::new(text).ok()?;
        let twin = self.twin()?;
        let ok = twin.call(
            eff::STRING_TO_PARAMETER,
            id as i32,
            0,
            text.as_ptr() as *mut c_void,
            0.0,
        );
        (ok != 0).then(|| twin.get(id) as f64)
    }

    fn state(&self) -> Option<Vec<u8>> {
        self.snapshot()
    }

    fn set_state(&self, state: &[u8]) -> Result<(), String> {
        let (program, settings) = unpack(state).ok_or("not a VST2 plugin's settings")?;
        let i = &self.instance;
        if program != i.program() && (0..i.num_programs() as i32).contains(&program) {
            i.set_program(program);
        }
        i.restore(&settings, false);
        Ok(())
    }

    fn presets(&self) -> Vec<Preset> {
        let i = &self.instance;
        let named = |program: u32| {
            let mut buf = [0u8; 256];
            let ok = i.call(
                eff::GET_PROGRAM_NAME_INDEXED,
                program as i32,
                -1,
                buf.as_mut_ptr().cast(),
                0.0,
            );
            match ok {
                0 => String::new(),
                _ => text(&buf),
            }
        };
        let mut out: Vec<Preset> = Vec::new();
        // One unnamed program is no list at all (many plugins have just that).
        if i.num_programs() > 1 || !named(0).is_empty() {
            out.extend((0..i.num_programs()).map(|program| Preset {
                name: match named(program) {
                    name if name.is_empty() => format!("Program {}", program + 1),
                    name => name,
                },
                source: PresetSource::Program { list: 0, program },
            }));
        }
        let id = i.unique_id();
        let files =
            super::super::preset_files(&super::super::preset_dirs(&self.vendor, &self.name), "fxp");
        out.extend(files.into_iter().filter(
            |p| matches!(&p.source, PresetSource::File { file } if fxp_id(file) == Some(id)),
        ));
        out
    }

    fn load_preset(&self, preset: &Preset) -> Result<(), String> {
        let i = &self.instance;
        match &preset.source {
            PresetSource::Program { program, .. } => {
                if *program >= i.num_programs() {
                    return Err(format!("{}: it has no program {program}", self.name));
                }
                i.set_program(*program as i32);
                i.forget_sent();
                Ok(())
            }
            PresetSource::File { file } => {
                let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
                let fxp = read_fxp(&bytes)
                    .ok_or_else(|| format!("{}: not an .fxp preset", file.display()))?;
                if fxp.id != i.unique_id() {
                    return Err(format!("{}: a preset for another plugin", file.display()));
                }
                i.restore(&fxp.settings, true);
                Ok(())
            }
        }
    }

    fn save_preset(&self, name: &str) -> Result<PathBuf, String> {
        let i = &self.instance;
        let settings = i
            .settings(true)
            .ok_or("the plugin wouldn't give its settings")?;
        let dir = super::super::preset_dirs(&self.vendor, &self.name).remove(0);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(super::super::preset_file_name(name, "fxp"));
        let version = unsafe { (*i.effect).version };
        let bytes = write_fxp(i.unique_id(), version, i.num_params(), name, &settings);
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }

    fn open_editor(&self, mtm: MainThreadMarker) -> Result<Editor, String> {
        let fail = |why: &str| format!("{}: {why}", self.name);
        let i = &self.instance;
        if i.flags() & FLAG_HAS_EDITOR == 0 {
            return Err(fail("no editor"));
        }
        // Some editors know their size only once open.
        let size = i.editor_size();
        let window = editor_window(mtm, &self.name, size.unwrap_or((400.0, 300.0)));
        let content = window
            .contentView()
            .ok_or_else(|| fail("window has no content view"))?;
        i.window
            .store(Retained::as_ptr(&window) as *mut NSWindow, Relaxed);
        i.call(
            eff::EDIT_OPEN,
            0,
            0,
            Retained::as_ptr(&content) as *mut c_void,
            0.0,
        );
        if let Some(opened) = i.editor_size().filter(|&s| Some(s) != size) {
            window.setContentSize(NSSize::new(opened.0, opened.1));
        }
        window.makeKeyAndOrderFront(None);
        let instance = i.clone();
        Ok(Editor::new(window, move || {
            instance.window.store(ptr::null_mut(), Relaxed);
            instance.call(eff::EDIT_CLOSE, 0, 0, ptr::null_mut(), 0.0);
        }))
    }

    fn idle(&self) {
        if !self.instance.window.load(Relaxed).is_null() {
            self.instance
                .call(eff::EDIT_IDLE, 0, 0, ptr::null_mut(), 0.0);
        }
    }
}
