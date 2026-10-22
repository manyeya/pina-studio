//! A VST2 library: opened, its entry point found, its plugins listed, one made.

use super::*;

/// A loaded `.vst` bundle: its entry point, which makes a new instance each time it's called.
// ponytail: never unloaded; add when the plugin browser rescans.
#[derive(Clone, Copy)]
pub struct Library {
    pub(crate) entry: Entry,
}

impl Library {
    pub fn load(path: &Path) -> Result<Library, String> {
        let fail = |why: &str| format!("{}: {why}", path.display());
        let binary = Binary::load(path)?;
        let entry = [c"VSTPluginMain", c"main_macho", c"main"]
            .into_iter()
            .find_map(|name| unsafe { binary.function(name) })
            .ok_or_else(|| fail("no VSTPluginMain"))?;
        Ok(Library { entry })
    }

    /// A new instance of its plugin, opened: for a shell, of the plugin `uid` in it (0, any).
    pub(crate) fn open(
        &self,
        uid: i32,
        rate: f64,
        max_block: usize,
    ) -> Result<Arc<Instance>, String> {
        LOADING.set(Some((uid, rate, max_block)));
        let effect = unsafe { (self.entry)(host_callback) };
        LOADING.set(None);
        if effect.is_null() {
            return Err("its entry point made no plugin".into());
        }
        let e = unsafe { &*effect };
        let (true, Some(dispatcher), Some(set_parameter), Some(get_parameter)) = (
            e.magic == MAGIC,
            e.dispatcher,
            e.set_parameter,
            e.get_parameter,
        ) else {
            return Err("not a VST2 plugin".into());
        };
        let sent = (0..e.num_params.max(0))
            .map(|_| AtomicU64::new(NOTHING))
            .collect();
        let instance = Arc::new(Instance {
            effect,
            dispatcher,
            set_parameter,
            get_parameter,
            rate,
            max_block,
            playing: AtomicBool::new(false),
            position: AtomicU64::new(0),
            tempo: AtomicU64::new(120f64.to_bits()),
            changed: AtomicBool::new(false),
            sent,
            window: AtomicPtr::new(ptr::null_mut()),
        });
        // From here on the host callback knows which instance is calling.
        unsafe { (*effect).resvd1 = Arc::as_ptr(&instance) as isize };
        instance.call(eff::OPEN, 0, 0, ptr::null_mut(), 0.0);
        Ok(instance)
    }
}

/// A plugin's text from a buffer it filled: UTF-8 if it is, else Latin-1, as older plugins
/// write it.
pub(crate) fn text(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let bytes = buf[..end].to_vec();
    let s = String::from_utf8(bytes)
        .unwrap_or_else(|e| e.into_bytes().iter().map(|&b| b as char).collect());
    s.trim().to_string()
}

/// A plugin's unique id as its four characters ("DfRr"), or as a number when they aren't
/// printable (or would read as one).
pub(crate) fn uid_text(id: i32) -> String {
    let bytes = id.to_be_bytes();
    let code = String::from_utf8_lossy(&bytes);
    match bytes.iter().all(|b| b.is_ascii_graphic()) && code.parse::<i32>().is_err() {
        true => code.into_owned(),
        false => id.to_string(),
    }
}

pub(crate) fn parse_uid(s: &str) -> Option<i32> {
    s.parse().ok().or_else(|| {
        <[u8; 4]>::try_from(s.as_bytes())
            .ok()
            .map(i32::from_be_bytes)
    })
}

/// The plugins in a `.vst` bundle, opening it in this process.
pub fn inspect(bundle: &Path) -> Result<Vec<PluginInfo>, String> {
    let library = Library::load(bundle)?;
    let i = library
        .open(0, 44_100.0, 512)
        .map_err(|e| format!("{}: {e}", bundle.display()))?;
    let vendor = i.string(eff::GET_VENDOR_STRING, 0);
    // VST2's categories, as words.
    let category = match i.category() {
        1 => "Fx",
        2 => "Instrument",
        3 => "Analyzer",
        4 => "Mastering",
        5 => "Spatial",
        6 => "Reverb",
        7 => "Surround",
        8 => "Restoration",
        11 => "Generator",
        _ => "",
    };
    let info = |class: String, uid: i32, instrument: bool| PluginInfo {
        format: Format::Vst2,
        path: bundle.to_path_buf(),
        class,
        uid: Some(uid_text(uid)),
        vendor: vendor.clone(),
        instrument,
        categories: [category]
            .iter()
            .filter(|c| !c.is_empty())
            .map(|c| c.to_string())
            .collect(),
    };
    if i.category() == CATEGORY_SHELL {
        // ponytail: a shell's plugins are listed as effects, without opening each (slow for
        // big shells); open each if a shell of instruments turns up.
        let plugins = i.shell_plugins();
        return Ok(plugins
            .into_iter()
            .map(|(uid, name)| info(name, uid, false))
            .collect());
    }
    Ok(vec![info(i.name(bundle), i.unique_id(), i.is_synth())])
}

/// Loads the plugin `r` names, from its bundle (loaded once into `libraries`).
pub fn instantiate(
    libraries: &mut HashMap<PathBuf, Library>,
    r: &PluginRef,
    rate: f64,
    max_block: usize,
    state: Option<&[u8]>,
) -> Result<(Plugin, Processor), String> {
    if !libraries.contains_key(&r.path) {
        libraries.insert(r.path.clone(), Library::load(&r.path)?);
    }
    let library = &libraries[&r.path];
    let fail = |why: &str| format!("{}: {why}", r.class);
    let uid = r.uid.as_deref().and_then(parse_uid).unwrap_or(0);
    let mut instance = library.open(uid, rate, max_block).map_err(|e| fail(&e))?;
    // A shell that didn't make the plugin asked for: find it by name.
    if instance.category() == CATEGORY_SHELL {
        let (uid, _) = instance
            .shell_plugins()
            .into_iter()
            .find(|(_, name)| *name == r.class)
            .ok_or_else(|| format!("{}: no plugin named {:?}", r.path.display(), r.class))?;
        drop(instance);
        instance = library.open(uid, rate, max_block).map_err(|e| fail(&e))?;
    }
    let (plugin, processor) =
        Vst2Plugin::start(*library, instance, &r.path, rate, max_block, state)
            .map_err(|e| fail(&e))?;
    Ok((Box::new(plugin), Box::new(processor)))
}
