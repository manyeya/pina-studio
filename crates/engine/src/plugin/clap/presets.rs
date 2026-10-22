//! Presets the plugin's preset providers find, and loading them.

use super::*;

pub(crate) unsafe extern "C" fn preset_error(
    host: *const clap_host,
    _kind: clap_preset_discovery_location_kind,
    location: *const c_char,
    _key: *const c_char,
    _os_error: i32,
    msg: *const c_char,
) {
    let name = &unsafe { instance(host) }.name;
    eprintln!("{name}: {}: {}", text(location), text(msg));
}

pub(crate) unsafe extern "C" fn preset_loaded(
    _: *const clap_host,
    _: clap_preset_discovery_location_kind,
    _: *const c_char,
    _: *const c_char,
) {
}

/// A preset a plugin's providers found: its name, and where and how it's loaded.
pub(crate) struct Found {
    pub(crate) name: String,
    pub(crate) kind: clap_preset_discovery_location_kind,
    pub(crate) location: Option<CString>,
    pub(crate) key: Option<CString>,
}

impl Found {
    /// The file it is, if it's a whole file (not one of several in a file, or built in).
    pub(crate) fn file(&self) -> Option<PathBuf> {
        if self.kind != CLAP_PRESET_DISCOVERY_LOCATION_FILE || self.key.is_some() {
            return None;
        }
        let location = self.location.as_ref()?;
        Some(PathBuf::from(location.to_string_lossy().into_owned()))
    }
}

/// The files at `path` (or it, if it's a file), in subfolders too, with one of `extensions` (any,
/// if there are none).
pub(crate) fn preset_paths(path: &Path, extensions: &[String]) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for path in entries.flatten().map(|e| e.path()) {
            match path.is_dir() {
                true => walk(&path, out),
                false => out.push(path),
            }
        }
    }
    if path.is_file() {
        return vec![path.to_path_buf()];
    }
    let mut out = Vec::new();
    walk(path, &mut out);
    out.sort();
    let matches = |p: &Path| {
        extensions.iter().any(|e| {
            let e = e.trim_start_matches('.');
            e.is_empty() || p.extension().is_some_and(|x| x.eq_ignore_ascii_case(e))
        })
    };
    out.retain(|p| extensions.is_empty() || matches(p));
    out
}

/// Hears where a preset provider's presets are, and what kinds of file they're in.
#[repr(C)]
pub(crate) struct Indexer {
    pub(crate) clap: clap_preset_discovery_indexer,
    pub(crate) extensions: RefCell<Vec<String>>,
    /// Each place's kind, and its path for files.
    pub(crate) locations: RefCell<Vec<(clap_preset_discovery_location_kind, Option<PathBuf>)>>,
}

impl Indexer {
    pub(crate) fn new() -> Box<Indexer> {
        Box::new(Indexer {
            clap: clap_preset_discovery_indexer {
                clap_version: CLAP_VERSION,
                name: c"Pina Studio".as_ptr(),
                vendor: c"Pina Studio".as_ptr(),
                url: c"".as_ptr(),
                version: VERSION.as_ptr(),
                indexer_data: ptr::null_mut(),
                declare_filetype: Some(declare_filetype),
                declare_location: Some(declare_location),
                declare_soundpack: Some(declare_soundpack),
                get_extension: Some(indexer_extension),
            },
            extensions: RefCell::new(Vec::new()),
            locations: RefCell::new(Vec::new()),
        })
    }
}

pub(crate) unsafe fn indexer<'a>(i: *const clap_preset_discovery_indexer) -> &'a Indexer {
    unsafe { &*i.cast::<Indexer>() }
}

pub(crate) unsafe extern "C" fn declare_filetype(
    i: *const clap_preset_discovery_indexer,
    filetype: *const clap_preset_discovery_filetype,
) -> bool {
    let Some(filetype) = (unsafe { filetype.as_ref() }) else {
        return false;
    };
    let extension = text(filetype.file_extension);
    unsafe { indexer(i) }
        .extensions
        .borrow_mut()
        .push(extension);
    true
}

pub(crate) unsafe extern "C" fn declare_location(
    i: *const clap_preset_discovery_indexer,
    location: *const clap_preset_discovery_location,
) -> bool {
    let Some(l) = (unsafe { location.as_ref() }) else {
        return false;
    };
    let path =
        (l.kind == CLAP_PRESET_DISCOVERY_LOCATION_FILE && !l.location.is_null()).then(|| {
            // CLAP's locations are UTF-8.
            PathBuf::from(
                unsafe { CStr::from_ptr(l.location) }
                    .to_string_lossy()
                    .into_owned(),
            )
        });
    let locations = &unsafe { indexer(i) }.locations;
    locations.borrow_mut().push((l.kind, path));
    true
}

pub(crate) unsafe extern "C" fn declare_soundpack(
    _: *const clap_preset_discovery_indexer,
    _: *const clap_preset_discovery_soundpack,
) -> bool {
    true
}

pub(crate) unsafe extern "C" fn indexer_extension(
    _: *const clap_preset_discovery_indexer,
    _: *const c_char,
) -> *const c_void {
    ptr::null()
}

/// Hears the presets a provider finds in one place.
#[repr(C)]
pub(crate) struct Receiver {
    pub(crate) clap: clap_preset_discovery_metadata_receiver,
    pub(crate) presets: RefCell<Vec<Listed>>,
}

/// A preset as a provider lists it: its name, load key, and the CLAP plugins it's for.
pub(crate) struct Listed {
    pub(crate) name: String,
    pub(crate) key: Option<CString>,
    pub(crate) plugins: Vec<CString>,
}

pub(crate) type Metadata = *const clap_preset_discovery_metadata_receiver;

impl Receiver {
    pub(crate) fn new() -> Box<Receiver> {
        Box::new(Receiver {
            clap: clap_preset_discovery_metadata_receiver {
                receiver_data: ptr::null_mut(),
                on_error: Some(metadata_error),
                begin_preset: Some(begin_preset),
                add_plugin_id: Some(add_plugin_id),
                set_soundpack_id: Some(ignore_text),
                set_flags: Some(set_flags),
                add_creator: Some(ignore_text),
                set_description: Some(ignore_text),
                set_timestamps: Some(set_timestamps),
                add_feature: Some(ignore_text),
                add_extra_info: Some(add_extra_info),
            },
            presets: RefCell::new(Vec::new()),
        })
    }
}

pub(crate) unsafe fn receiver<'a>(r: Metadata) -> &'a Receiver {
    unsafe { &*r.cast::<Receiver>() }
}

pub(crate) unsafe extern "C" fn begin_preset(
    r: Metadata,
    name: *const c_char,
    key: *const c_char,
) -> bool {
    // No key, or an empty one (as Surge gives), is a whole file.
    let key = Some(text(key))
        .filter(|k| !k.is_empty())
        .and_then(|k| CString::new(k).ok());
    let presets = &unsafe { receiver(r) }.presets;
    presets.borrow_mut().push(Listed {
        name: text(name),
        key,
        plugins: Vec::new(),
    });
    true
}

pub(crate) unsafe extern "C" fn add_plugin_id(r: Metadata, id: *const clap_universal_plugin_id) {
    let Some(id) = (unsafe { id.as_ref() }) else {
        return;
    };
    if text(id.abi) != "clap" || id.id.is_null() {
        return;
    }
    let mut presets = unsafe { receiver(r) }.presets.borrow_mut();
    if let Some(listed) = presets.last_mut() {
        listed
            .plugins
            .push(unsafe { CStr::from_ptr(id.id) }.to_owned());
    }
}

pub(crate) unsafe extern "C" fn metadata_error(_: Metadata, _: i32, _: *const c_char) {}

pub(crate) unsafe extern "C" fn ignore_text(_: Metadata, _: *const c_char) {}

pub(crate) unsafe extern "C" fn set_flags(_: Metadata, _: u32) {}

pub(crate) unsafe extern "C" fn set_timestamps(_: Metadata, _: u64, _: u64) {}

pub(crate) unsafe extern "C" fn add_extra_info(_: Metadata, _: *const c_char, _: *const c_char) {}
