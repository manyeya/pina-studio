//! Finding the plugins installed: VST3, CLAP and VST2 bundles, and Audio Units. Each bundle is
//! opened in a child process (`pina __scan <bundle>`, or `pina __scan au` for the Audio
//! Units), so one that crashes takes only its scan down, and results are cached by bundle and
//! modification time, so a scan only opens what's new.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime},
};

use serde::{Deserialize, Serialize};

use crate::{plugin, project::Format};

/// How long one bundle gets to report what's in it.
const TIMEOUT: Duration = Duration::from_secs(30);

/// A plugin, as `PluginRef` names it: format, bundle, name and (for some formats) id.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PluginInfo {
    #[serde(default)]
    pub format: Format,
    pub path: PathBuf,
    pub class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    pub vendor: String,
    pub instrument: bool,
    /// What it says it is, in its format's words: VST3's subcategories ("Fx", "Reverb"),
    /// CLAP's features ("audio-effect", "delay"), VST2's category; for an Audio Unit, its type
    /// and subtype codes are in `uid`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
}

impl PluginInfo {
    /// The reference a project keeps to it.
    pub fn plugin_ref(&self) -> crate::project::PluginRef {
        crate::project::PluginRef {
            format: self.format,
            path: self.path.clone(),
            class: self.class.clone(),
            uid: self.uid.clone(),
            ..Default::default()
        }
    }
}

/// What `scan` passes for the Audio Units, which aren't found as bundles.
pub const AUDIO_UNITS: &str = "au";

#[derive(Serialize, Deserialize, Default)]
struct Cached {
    modified: u64,
    plugins: Vec<PluginInfo>,
    /// What the scan that wrote it read: a bundle scanned by an older one is scanned again.
    #[serde(default)]
    version: u32,
}

/// What a scan reads; 2 added each plugin's categories.
const VERSION: u32 = 2;

/// Where VST3, CLAP and VST2 plugins are installed, and the folders in `PINA_PLUGIN_PATH` (or,
/// as before, `PINA_VST3_PATH`; colon-separated).
pub fn folders() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let mut dirs = Vec::new();
    for kind in ["VST3", "CLAP", "VST"] {
        dirs.push(PathBuf::from("/Library/Audio/Plug-Ins").join(kind));
        dirs.push(home.join("Library/Audio/Plug-Ins").join(kind));
    }
    for var in ["PINA_PLUGIN_PATH", "PINA_VST3_PATH"] {
        if let Some(extra) = std::env::var_os(var) {
            dirs.extend(std::env::split_paths(&extra));
        }
    }
    dirs
}

/// A bundle's format, by its extension.
pub fn format_of(bundle: &Path) -> Option<Format> {
    match bundle.extension()?.to_str()? {
        "vst3" => Some(Format::Vst3),
        "clap" => Some(Format::Clap),
        "vst" => Some(Format::Vst2),
        _ => None,
    }
}

/// Every plugin bundle (`.vst3`, `.clap`, `.vst`) in `dirs`, in vendor subfolders too.
pub fn bundles(dirs: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for path in entries.flatten().map(|e| e.path()) {
            if format_of(&path).is_some() {
                out.push(path);
            } else if path.is_dir() {
                walk(&path, out);
            }
        }
    }
    let mut out = Vec::new();
    dirs.iter().for_each(|d| walk(d, &mut out));
    out.sort();
    out
}

/// The plugins in one bundle (or with `AUDIO_UNITS`, the Audio Units), opening it in this
/// process. What `pina __scan` runs.
pub fn inspect(bundle: &Path) -> Result<Vec<PluginInfo>, String> {
    if bundle == Path::new(AUDIO_UNITS) {
        return Ok(plugin::au::list());
    }
    match format_of(bundle) {
        Some(Format::Vst3) => plugin::vst3::inspect(bundle),
        Some(Format::Clap) => plugin::clap::inspect(bundle),
        Some(Format::Vst2) => plugin::vst2::inspect(bundle),
        _ => Err(format!("{}: not a plugin bundle", bundle.display())),
    }
}

/// When the Audio Units last changed, as far as their folders say.
fn units_modified() -> u64 {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    [
        PathBuf::from("/Library/Audio/Plug-Ins/Components"),
        home.join("Library/Audio/Plug-Ins/Components"),
        PathBuf::from("/System/Library/Components"),
    ]
    .iter()
    .map(|d| modified(d))
    .max()
    .unwrap_or(0)
}

fn modified(path: &Path) -> u64 {
    let time = std::fs::metadata(path).and_then(|m| m.modified());
    time.ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

/// Asks `exe __scan <bundle>` what a bundle holds. A crash or a hang means nothing usable.
fn scan_one(exe: &Path, bundle: &Path) -> Vec<PluginInfo> {
    let child = Command::new(exe)
        .arg("__scan")
        .arg(bundle)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        return Vec::new();
    };
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Vec::new();
            }
        }
    }
    let Ok(out) = child.wait_with_output() else {
        return Vec::new();
    };
    serde_json::from_slice(&out.stdout).unwrap_or_default()
}

/// Every plugin in `bundles` (and the Audio Units, if `AUDIO_UNITS` is among them), scanning
/// (with `exe`) only what's new or changed since the cache at `cache` was written, and
/// updating it.
// ponytail: one bundle at a time; scan in parallel if first scans of big collections drag.
pub fn scan(exe: &Path, bundles: &[PathBuf], cache: &Path) -> Vec<PluginInfo> {
    let old: HashMap<PathBuf, Cached> = std::fs::read(cache)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let mut new = HashMap::new();
    for bundle in bundles {
        let modified = match bundle == Path::new(AUDIO_UNITS) {
            true => units_modified(),
            false => modified(bundle),
        };
        let entry = match old.get(bundle) {
            Some(c) if c.modified == modified && c.version == VERSION => Cached {
                modified,
                plugins: c.plugins.clone(),
                version: VERSION,
            },
            _ => Cached {
                modified,
                plugins: scan_one(exe, bundle),
                version: VERSION,
            },
        };
        new.insert(bundle.clone(), entry);
    }
    if let Some(dir) = cache.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(cache, serde_json::to_vec(&new).unwrap());
    let mut plugins: Vec<PluginInfo> = new.into_values().flat_map(|c| c.plugins).collect();
    plugins.sort_by_key(|p| p.class.to_lowercase());
    plugins
}

/// What `scan` looks through: every bundle in `folders`, and the Audio Units.
pub fn everything() -> Vec<PathBuf> {
    let mut all = bundles(&folders());
    all.push(PathBuf::from(AUDIO_UNITS));
    all
}

/// Where the scan cache lives.
pub fn cache_file() -> PathBuf {
    crate::paths::caches().join("plugins.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_bundles_in_vendor_folders_and_survives_a_bad_scanner() {
        let dir = std::env::temp_dir().join(format!("pina-test-scan-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Vendor/Thing.vst3/Contents")).unwrap();
        std::fs::create_dir_all(dir.join("Other.vst3")).unwrap();
        std::fs::create_dir_all(dir.join("not a plugin")).unwrap();
        let found = bundles(std::slice::from_ref(&dir));
        assert_eq!(
            found,
            [dir.join("Other.vst3"), dir.join("Vendor/Thing.vst3")]
        );

        // `false` stands in for a scanner that fails: no plugins, but the scan finishes and the
        // failure is cached.
        let cache = dir.join("cache.json");
        assert!(scan(Path::new("false"), &found, &cache).is_empty());
        let cached: HashMap<PathBuf, Cached> =
            serde_json::from_slice(&std::fs::read(&cache).unwrap()).unwrap();
        assert_eq!(cached.len(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
