//! A `.clap` file: opened once, its plugins listed, one made.

use super::*;

/// A loaded `.clap` bundle: its entry, initialised, and its plugin factory.
///
/// A bundle is opened and its entry initialised once per process, then kept: `deinit` is never
/// called. `Libraries` come and go while the plugins they loaded live on (a render drops its own
/// once its plugins are loaded), so deinit on drop would pull the library out from under them;
/// and CLAP wants one `init` per load, not one per `Libraries`.
#[derive(Clone, Copy)]
pub struct Library {
    pub(crate) entry: *const clap_plugin_entry,
    pub(crate) factory: *const clap_plugin_factory,
}

// The entry and factory are the bundle's, which is never unloaded, and CLAP makes them
// thread-safe.
unsafe impl Send for Library {}

unsafe impl Sync for Library {}

/// Every bundle opened in this process.
pub(crate) static OPENED: Mutex<Vec<(PathBuf, Library)>> = Mutex::new(Vec::new());

/// A plugin a bundle holds, as its descriptor says.
pub(crate) struct Descriptor {
    pub(crate) id: CString,
    pub(crate) name: String,
    pub(crate) vendor: String,
    pub(crate) instrument: bool,
    /// Its features: "instrument", "synthesizer"; "audio-effect", "reverb".
    pub(crate) features: Vec<String>,
}

impl Library {
    pub fn load(path: &Path) -> Result<Library, String> {
        let mut opened = OPENED.lock().unwrap();
        if let Some((_, library)) = opened.iter().find(|(p, _)| p == path) {
            return Ok(*library);
        }
        let library = Library::open(path)?;
        opened.push((path.to_path_buf(), library));
        Ok(library)
    }

    pub(crate) fn open(path: &Path) -> Result<Library, String> {
        let fail = |why: &str| format!("{}: {why}", path.display());
        let p = CString::new(path.as_os_str().as_encoded_bytes()).map_err(|_| fail("bad path"))?;
        let binary = Binary::load(path)?;
        unsafe {
            let entry = binary.data(c"clap_entry").cast::<clap_plugin_entry>();
            let e = entry.as_ref().ok_or_else(|| fail("no clap_entry"))?;
            if !clap_version_is_compatible(e.clap_version) {
                let v = e.clap_version;
                return Err(fail(&format!(
                    "CLAP {}.{} isn't supported",
                    v.major, v.minor
                )));
            }
            if !e.init.is_some_and(|init| init(p.as_ptr())) {
                return Err(fail("its init failed"));
            }
            let library = Library {
                entry,
                factory: ptr::null(),
            };
            let factory = library.factory::<clap_plugin_factory>(&[CLAP_PLUGIN_FACTORY_ID]);
            if factory.is_null() {
                return Err(fail("no plugin factory"));
            }
            Ok(Library { factory, ..library })
        }
    }

    /// One of the bundle's factories, by the ids it may go by, or null.
    pub(crate) fn factory<T>(&self, ids: &[&CStr]) -> *const T {
        let Some(get) = (unsafe { self.entry.as_ref() }).and_then(|e| e.get_factory) else {
            return ptr::null();
        };
        ids.iter()
            .map(|id| unsafe { get(id.as_ptr()) }.cast::<T>())
            .find(|f| !f.is_null())
            .unwrap_or(ptr::null())
    }

    pub(crate) fn descriptors(&self) -> Vec<Descriptor> {
        let factory = unsafe { &*self.factory };
        let (Some(count), Some(get)) = (factory.get_plugin_count, factory.get_plugin_descriptor)
        else {
            return Vec::new();
        };
        unsafe {
            (0..count(self.factory))
                .filter_map(|i| {
                    let d: &clap_plugin_descriptor = get(self.factory, i).as_ref()?;
                    if d.id.is_null() || !clap_version_is_compatible(d.clap_version) {
                        return None;
                    }
                    Some(Descriptor {
                        id: CStr::from_ptr(d.id).to_owned(),
                        name: text(d.name),
                        vendor: text(d.vendor),
                        instrument: features(d.features).contains(&CLAP_PLUGIN_FEATURE_INSTRUMENT),
                        features: features(d.features)
                            .iter()
                            .map(|f| f.to_string_lossy().into_owned())
                            .collect(),
                    })
                })
                .collect()
        }
    }

    /// The presets the bundle's preset providers find for plugin `id`, in their order.
    pub(crate) fn discover(&self, id: &CStr) -> Vec<Found> {
        let factory = self.factory::<clap_preset_discovery_factory>(&[
            CLAP_PRESET_DISCOVERY_FACTORY_ID,
            CLAP_PRESET_DISCOVERY_FACTORY_ID_COMPAT,
        ]);
        let Some(f) = (unsafe { factory.as_ref() }) else {
            return Vec::new();
        };
        let (Some(count), Some(describe), Some(create)) = (f.count, f.get_descriptor, f.create)
        else {
            return Vec::new();
        };
        let indexer = Indexer::new();
        let receiver = Receiver::new();
        let mut found = Vec::new();
        unsafe {
            for i in 0..count(factory) {
                let Some(d) = describe(factory, i).as_ref() else {
                    continue;
                };
                let provider = create(factory, &indexer.clap, d.id);
                let Some(p) = provider.as_ref() else {
                    continue;
                };
                // It says where its presets are as it starts.
                let started = p.init.is_some_and(|init| init(provider));
                let extensions = indexer.extensions.take();
                let locations = indexer.locations.take();
                if started && let Some(metadata) = p.get_metadata {
                    // What the provider finds in one place, for this plugin.
                    let mut look = |kind, location: Option<&CString>| {
                        let at = location.map_or(ptr::null(), |l| l.as_ptr());
                        metadata(provider, kind, at, &receiver.clap);
                        for l in receiver.presets.take() {
                            if l.plugins.is_empty() || l.plugins.iter().any(|p| p.as_c_str() == id)
                            {
                                found.push(Found {
                                    name: l.name,
                                    kind,
                                    location: location.cloned(),
                                    key: l.key,
                                });
                            }
                        }
                    };
                    for (kind, location) in locations {
                        match location {
                            None => look(kind, None),
                            Some(place) => {
                                for file in preset_paths(&place, &extensions) {
                                    let Ok(file) =
                                        CString::new(file.as_os_str().as_encoded_bytes())
                                    else {
                                        continue;
                                    };
                                    look(kind, Some(&file));
                                }
                            }
                        }
                    }
                }
                if let Some(destroy) = p.destroy {
                    destroy(provider);
                }
            }
        }
        found
    }
}

/// The plugins in a `.clap` bundle, opening it in this process.
pub fn inspect(bundle: &Path) -> Result<Vec<PluginInfo>, String> {
    let library = Library::load(bundle)?;
    Ok(library
        .descriptors()
        .into_iter()
        .map(|d| PluginInfo {
            format: Format::Clap,
            path: bundle.to_path_buf(),
            class: d.name,
            uid: Some(d.id.to_string_lossy().into_owned()),
            vendor: d.vendor,
            instrument: d.instrument,
            categories: d.features,
        })
        .collect())
}

/// Loads the plugin `r` names (by its id, else its name), from its bundle (loaded once into
/// `libraries`).
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
    let library = libraries[&r.path];
    let descriptors = library.descriptors();
    let uid = r.uid.as_deref().map(str::as_bytes);
    let d = descriptors
        .iter()
        .find(|d| Some(d.id.to_bytes()) == uid)
        .or_else(|| descriptors.iter().find(|d| d.name == r.class))
        .ok_or_else(|| format!("{}: no plugin named {:?}", r.path.display(), r.class))?;
    let (plugin, processor) = ClapPlugin::instantiate(library, d, rate, max_block, state)?;
    Ok((Box::new(plugin), Box::new(processor)))
}
