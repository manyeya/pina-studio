//! Audio Units installed: their component folders and bundles, found and listed.

use super::*;

/// Where Audio Units are installed, as `.component` bundles.
pub(crate) fn component_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    vec![
        PathBuf::from("/Library/Audio/Plug-Ins/Components"),
        home.join("Library/Audio/Plug-Ins/Components"),
        PathBuf::from("/System/Library/Components"),
    ]
}

/// A four-character code as text, a character a byte ("aumu").
pub(crate) fn fourcc(code: u32) -> String {
    code.to_be_bytes().iter().map(|&b| b as char).collect()
}

/// A unit's type, subtype and manufacturer, as `PluginRef.uid` has them: "aumu samp appl".
pub(crate) fn uid(d: &AudioComponentDescription) -> String {
    format!(
        "{} {} {}",
        fourcc(d.componentType),
        fourcc(d.componentSubType),
        fourcc(d.componentManufacturer)
    )
}

/// The type, subtype and manufacturer a uid names. Codes can end in spaces ("dls "), so it's
/// read by position.
pub(crate) fn parse_uid(uid: &str) -> Option<[u32; 3]> {
    let chars: Vec<char> = uid.chars().collect();
    if chars.len() != 14 || chars[4] != ' ' || chars[9] != ' ' {
        return None;
    }
    let code = |at: usize| {
        chars[at..at + 4].iter().try_fold(0u32, |n, &c| {
            let b = u8::try_from(c as u32).ok()?;
            Some(n << 8 | b as u32)
        })
    };
    Some([code(0)?, code(5)?, code(10)?])
}

/// A unit's name, "Apple: AUSampler", as its vendor and its own name.
pub(crate) fn split_name(name: &str, manufacturer: u32) -> (String, String) {
    match name.split_once(": ") {
        Some((vendor, own)) => (vendor.trim().to_string(), own.trim().to_string()),
        None => (fourcc(manufacturer).trim().to_string(), name.to_string()),
    }
}

/// Takes ownership of a Core Foundation object the API handed over (it's ours to release).
pub(crate) fn owned<T: objc2_core_foundation::Type>(p: *const T) -> Option<CFRetained<T>> {
    NonNull::new(p as *mut T).map(|p| unsafe { CFRetained::from_raw(p) })
}

/// A string the API lends.
pub(crate) fn text(s: *const CFString) -> String {
    unsafe { s.as_ref() }.map_or_else(String::new, |s| s.to_string())
}

/// A property list read from its bytes, XML or binary.
pub(crate) fn plist(bytes: &[u8]) -> Option<CFRetained<CFPropertyList>> {
    let data = CFData::from_bytes(bytes);
    unsafe { CFPropertyListCreateWithData(None, Some(&data), 0, ptr::null_mut(), ptr::null_mut()) }
}

/// A property list as bytes, binary or XML.
pub(crate) fn plist_bytes(list: &CFPropertyList, format: CFPropertyListFormat) -> Option<Vec<u8>> {
    unsafe { CFPropertyListCreateData(None, Some(list), format, 0, ptr::null_mut()) }
        .map(|d| d.to_vec())
}

/// A dictionary's value for `key`, if it's a `T`.
pub(crate) fn value<'a, T: ConcreteType>(dict: &'a CFDictionary, key: &str) -> Option<&'a T> {
    let key = CFString::from_str(key);
    let v = unsafe { dict.value((&*key as *const CFString).cast()) };
    unsafe { v.cast::<CFType>().as_ref() }?.downcast_ref()
}

/// An array's items that are `T`s.
pub(crate) fn items<'a, T: ConcreteType + 'a>(array: &'a CFArray) -> impl Iterator<Item = &'a T> {
    (0..array.count()).filter_map(move |i| {
        let v = unsafe { array.value_at_index(i) };
        unsafe { v.cast::<CFType>().as_ref() }?.downcast_ref()
    })
}

/// An installed unit.
pub(crate) struct Component {
    pub(crate) component: AudioComponent,
    pub(crate) desc: AudioComponentDescription,
    pub(crate) vendor: String,
    pub(crate) name: String,
}

/// The units of a type, subtype and manufacturer (0 matching any).
pub(crate) fn find([t, s, m]: [u32; 3]) -> Vec<Component> {
    let desc = AudioComponentDescription {
        componentType: t,
        componentSubType: s,
        componentManufacturer: m,
        componentFlags: 0,
        componentFlagsMask: 0,
    };
    let mut out = Vec::new();
    let mut component = ptr::null_mut();
    loop {
        component = unsafe { AudioComponentFindNext(component, NonNull::from(&desc)) };
        if component.is_null() {
            return out;
        }
        let mut found = desc;
        let mut name: *const CFString = ptr::null();
        unsafe {
            AudioComponentGetDescription(component, NonNull::from(&mut found));
            AudioComponentCopyName(component, NonNull::from(&mut name));
        }
        let name = owned(name).map(|n| n.to_string()).unwrap_or_default();
        let (vendor, name) = split_name(&name, found.componentManufacturer);
        out.push(Component {
            component,
            desc: found,
            vendor,
            name,
        });
    }
}

/// Every unit Pina Studio can play.
pub(crate) fn all() -> Vec<Component> {
    KINDS
        .into_iter()
        .flat_map(|kind| find([kind, 0, 0]))
        .collect()
}

/// The unit a uid names, if it's installed.
pub(crate) fn by_uid(uid: &str) -> Option<Component> {
    find(parse_uid(uid)?).into_iter().next()
}

/// The `.component` bundles in `dirs`, by the units their `Info.plist`s list.
pub(crate) fn bundles(dirs: &[PathBuf]) -> HashMap<String, PathBuf> {
    let mut out = HashMap::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            if path.extension().is_none_or(|e| e != "component") {
                continue;
            }
            let Some(info) = std::fs::read(path.join("Contents/Info.plist"))
                .ok()
                .and_then(|b| plist(&b))
            else {
                continue;
            };
            let Some(info) = info.downcast_ref::<CFDictionary>() else {
                continue;
            };
            let Some(units) = value::<CFArray>(info, "AudioComponents") else {
                continue;
            };
            for unit in items::<CFDictionary>(units) {
                let code = |key| value::<CFString>(unit, key).map(|s| s.to_string());
                if let (Some(t), Some(s), Some(m)) =
                    (code("type"), code("subtype"), code("manufacturer"))
                {
                    out.entry(format!("{t} {s} {m}"))
                        .or_insert_with(|| path.clone());
                }
            }
        }
    }
    out
}

/// The Audio Units installed, the system's own included.
pub fn list() -> Vec<PluginInfo> {
    let bundles = bundles(&component_dirs());
    all()
        .into_iter()
        .map(|c| {
            let uid = uid(&c.desc);
            PluginInfo {
                format: Format::Au,
                path: bundles.get(&uid).cloned().unwrap_or_default(),
                class: c.name,
                uid: Some(uid),
                vendor: c.vendor,
                instrument: c.desc.componentType == kAudioUnitType_MusicDevice,
                categories: Vec::new(),
            }
        })
        .collect()
}
