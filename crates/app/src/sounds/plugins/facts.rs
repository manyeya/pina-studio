//! What's known of each plugin: its picture and how hard it works the computer (kept between
//! runs), and of each plugin file how big it is, whether this Mac runs it natively, its version
//! and when it was installed; and which plugins the songs use and miss.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use pina_engine::plugin::scan::PluginInfo;

use super::*;

/// A device's picture (a file in `pictures_dir`, its size in pixels), its load (0 to 1 of real
/// time) and why it wouldn't load, if it didn't.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Fact {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
    #[serde(default)]
    pub w: u32,
    #[serde(default)]
    pub h: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
    /// Being photographed: if Pina Studio stops while it is, the plugin is what stopped it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub taking: bool,
    /// It has no window to photograph.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub windowless: bool,
}

impl Fact {
    /// Whether its picture's still to be taken: none yet, and nothing in the way.
    pub fn wants_picture(&self) -> bool {
        self.picture.is_none() && self.failed.is_none() && !self.windowless
    }
}

/// A plugin file: its size, whether it has no code for Apple silicon (Rosetta would have to run
/// it), its version and when it was put there (seconds since 1970).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileFacts {
    pub size: u64,
    pub intel_only: bool,
    pub version: Option<String>,
    pub installed: Option<u64>,
    /// Part of macOS (Apple's own Audio Units): not the user's to remove.
    pub system: bool,
}

/// Which devices the songs use (by key, with how many songs each), and the plugins they name
/// that aren't installed (by name, with the songs).
#[derive(Clone, Debug, Default)]
pub struct SongsUse {
    pub songs: usize,
    pub used: HashMap<String, usize>,
    pub missing: HashMap<String, Vec<String>>,
}

pub fn pictures_dir() -> PathBuf {
    pina_engine::paths::caches().join("Plugin Pictures")
}

fn facts_file() -> PathBuf {
    pina_engine::paths::caches().join("plugin-index.json")
}

pub(crate) fn load_facts() -> HashMap<String, Fact> {
    if cfg!(test) {
        return HashMap::new();
    }
    let mut facts: HashMap<String, Fact> = std::fs::read(facts_file())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    // One still being photographed when Pina Studio stopped: it's what stopped it.
    for f in facts.values_mut().filter(|f| f.taking) {
        f.taking = false;
        f.failed = Some("Pina Studio stopped while opening it".into());
    }
    facts
}

pub(crate) fn save_facts(facts: &HashMap<String, Fact>) {
    if cfg!(test) {
        return;
    }
    let file = facts_file();
    let _ = std::fs::create_dir_all(file.parent().unwrap());
    let _ = std::fs::write(file, serde_json::to_vec(facts).unwrap_or_default());
}

/// A file name for a device's picture.
pub fn picture_name(key: &str) -> String {
    let safe: String = key
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    format!("{safe}.jpg")
}

/// How big a file or folder is, everything in it.
fn size_of(path: &Path) -> u64 {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => std::fs::read_dir(path)
            .map(|d| d.flatten().map(|e| size_of(&e.path())).sum())
            .unwrap_or(0),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

/// The code a bundle runs: what its Info.plist names, else the only file in Contents/MacOS.
fn executable(bundle: &Path) -> Option<PathBuf> {
    let dir = bundle.join("Contents/MacOS");
    let named = plist_string(bundle, "CFBundleExecutable").map(|n| dir.join(n));
    named.filter(|p| p.exists()).or_else(|| {
        std::fs::read_dir(&dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.is_file())
    })
}

/// A string from a bundle's Info.plist (the XML kind).
fn plist_string(bundle: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(bundle.join("Contents/Info.plist")).ok()?;
    let at = text.find(&format!("<key>{key}</key>"))?;
    let rest = &text[at..];
    let open = rest.find("<string>")? + "<string>".len();
    let close = rest[open..].find("</string>")?;
    Some(rest[open..open + close].trim().to_string())
}

/// Whether a Mach-O file has code for Apple silicon (arm64), or only for Intel.
fn has_arm64(file: &Path) -> Option<bool> {
    use std::io::Read;
    let mut head = [0u8; 4096];
    let n = std::fs::File::open(file).ok()?.read(&mut head).ok()?;
    let head = &head[..n];
    let be = |i: usize| u32::from_be_bytes(head.get(i..i + 4)?.try_into().ok()?).into();
    let le = |i: usize| -> Option<u32> {
        Some(u32::from_le_bytes(head.get(i..i + 4)?.try_into().ok()?))
    };
    const ARM64: u32 = 0x0100_000c;
    let magic: Option<u32> = be(0);
    match magic? {
        // Universal: a list of architectures (32-bit or 64-bit entries).
        0xcafe_babe | 0xcafe_babf => {
            let n: u32 = be(4)?;
            let step = if magic == Some(0xcafe_babf) { 32 } else { 20 };
            Some((0..n as usize).any(|k| be(8 + k * step) == Some(ARM64)))
        }
        // One architecture, little-endian.
        0xcffa_edfe | 0xcefa_edfe => Some(le(4)? == ARM64),
        _ => None,
    }
}

/// What's known of one plugin file, read from disk.
pub fn file_facts(p: &PluginInfo) -> FileFacts {
    let path = &p.path;
    let system = path.starts_with("/System");
    if system {
        return FileFacts {
            system,
            ..Default::default()
        };
    }
    let installed = std::fs::metadata(path)
        .and_then(|m| m.created().or_else(|_| m.modified()))
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    let bundle = path.is_dir();
    let code = match bundle {
        true => executable(path),
        false => Some(path.clone()),
    };
    FileFacts {
        size: size_of(path),
        intel_only: cfg!(target_arch = "aarch64")
            && code.as_deref().and_then(has_arm64) == Some(false),
        version: bundle
            .then(|| plist_string(path, "CFBundleShortVersionString"))
            .flatten(),
        installed,
        system,
    }
}

/// Every plugin reference in a song file: (format, path, class).
fn song_plugins(json: &serde_json::Value, out: &mut Vec<(String, String, String)>) {
    match json {
        serde_json::Value::Object(m) => {
            if let (Some(c), Some(f)) = (m.get("class"), m.get("format")) {
                let s = |v: Option<&serde_json::Value>| {
                    v.and_then(|v| v.as_str()).unwrap_or_default().to_string()
                };
                out.push((s(Some(f)), s(m.get("path")), s(Some(c))));
                return;
            }
            m.values().for_each(|v| song_plugins(v, out));
        }
        serde_json::Value::Array(a) => a.iter().for_each(|v| song_plugins(v, out)),
        _ => {}
    }
}

/// What the songs in a songs folder use: each device by how many songs have it, and plugins
/// they name that aren't installed.
pub fn songs_use(songs: &Path, devices: &[(String, String, Vec<PathBuf>)]) -> SongsUse {
    let mut out = SongsUse::default();
    let Ok(dirs) = std::fs::read_dir(songs) else {
        return out;
    };
    for dir in dirs.flatten() {
        let file = dir.path().join("project.json");
        let Some(json) = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        else {
            continue;
        };
        out.songs += 1;
        let song = dir.file_name().to_string_lossy().into_owned();
        let mut refs = Vec::new();
        song_plugins(&json, &mut refs);
        let mut here = HashSet::new();
        for (_, path, class) in refs {
            let found = devices.iter().find(|(_, name, paths)| {
                paths.iter().any(|p| p.as_os_str() == path.as_str())
                    || name.eq_ignore_ascii_case(&class)
            });
            match found {
                Some((key, ..)) => {
                    here.insert(key.clone());
                }
                None => {
                    let songs = out.missing.entry(class).or_default();
                    if !songs.contains(&song) {
                        songs.push(song.clone());
                    }
                }
            }
        }
        for key in here {
            *out.used.entry(key).or_default() += 1;
        }
    }
    out
}

impl Workspace {
    /// Looks at the plugin files and the songs again, in the background: sizes, Rosetta,
    /// versions, and what the songs use.
    pub fn refresh_plugin_facts(&mut self, cx: &mut Context<Self>) {
        let plugins: Vec<PluginInfo> = self
            .catalog
            .devices
            .iter()
            .flat_map(|d| d.formats.iter().cloned())
            .collect();
        let devices: Vec<(String, String, Vec<PathBuf>)> = self
            .catalog
            .devices
            .iter()
            .filter(|d| d.own.is_none())
            .map(|d| {
                (
                    d.key.clone(),
                    d.name.clone(),
                    d.formats.iter().map(|p| p.path.clone()).collect(),
                )
            })
            .collect();
        // Tests never read the person's songs.
        let songs = (!cfg!(test)).then(pina_engine::paths::songs);
        let work = cx.background_spawn(async move {
            let mut files = HashMap::new();
            for p in &plugins {
                files.entry(p.path.clone()).or_insert_with(|| file_facts(p));
            }
            let songs = songs.map(|s| songs_use(&s, &devices)).unwrap_or_default();
            (files, songs)
        });
        cx.spawn(async move |this, cx| {
            let (files, songs) = work.await;
            this.update(cx, |ws, cx| {
                ws.plugin_page.files = files;
                ws.plugin_page.songs = Some(songs);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// A device's size on disk: every format's file.
    pub(crate) fn device_size(&self, d: &Device) -> u64 {
        d.formats
            .iter()
            .filter_map(|p| self.plugin_page.files.get(&p.path))
            .map(|f| f.size)
            .sum()
    }
}

/// A size as people read it: 1.2 GB, 340 MB.
pub fn size_text(bytes: u64) -> String {
    let mb = bytes as f64 / 1_000_000.0;
    match mb {
        m if m >= 1000.0 => format!("{:.1} GB", m / 1000.0),
        m if m >= 10.0 => format!("{m:.0} MB"),
        m if m >= 0.1 => format!("{m:.1} MB"),
        _ => format!("{} KB", (bytes / 1000).max(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_silicon_code_is_found() {
        let dir = std::env::temp_dir().join(format!("pina-macho-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = |name: &str, bytes: Vec<u8>| {
            let p = dir.join(name);
            std::fs::write(&p, bytes).unwrap();
            has_arm64(&p)
        };
        let thin = |cpu: u32| [0xcffa_edfe_u32.to_be_bytes(), cpu.to_le_bytes()].concat();
        // Universal: Intel, then Apple silicon; and Intel alone.
        let fat = |cpus: &[u32]| {
            let mut b = [
                0xcafe_babe_u32.to_be_bytes(),
                (cpus.len() as u32).to_be_bytes(),
            ]
            .concat();
            for c in cpus {
                b.extend(c.to_be_bytes());
                b.extend([0; 16]);
            }
            b
        };
        assert_eq!(file("arm", thin(0x0100_000c)), Some(true));
        assert_eq!(file("intel", thin(0x0100_0007)), Some(false));
        assert_eq!(file("both", fat(&[0x0100_0007, 0x0100_000c])), Some(true));
        assert_eq!(file("old", fat(&[0x0100_0007])), Some(false));
        assert_eq!(file("text", b"#!/bin/sh".to_vec()), None);
        std::fs::remove_dir_all(dir).unwrap();
        assert_eq!(
            [999, 340_000, 12_600_000, 1_234_000_000].map(size_text),
            ["1 KB", "0.3 MB", "13 MB", "1.2 GB"]
        );
    }
}
