//! The library: the folders samples are found in, and the index of what's in them. Scanning
//! analyses only what's new or changed since last time (by modification time and size), on
//! every core, and drops what's gone; the index is kept in `~/Library/Caches/tempo`.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::SystemTime,
};

use serde::{Deserialize, Serialize};

use super::{Sample, analyze};

/// The audio files the library reads.
pub const EXTENSIONS: [&str; 6] = ["wav", "wave", "aif", "aiff", "flac", "mp3"];

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Library {
    /// The folders it looks in.
    pub folders: Vec<PathBuf>,
    #[serde(skip)]
    pub samples: Vec<Sample>,
}

/// What a scan found.
#[derive(Serialize, Debug, Clone, PartialEq, Default)]
pub struct ScanReport {
    pub samples: usize,
    /// Newly analysed (new or changed), and gone since last time.
    pub analysed: usize,
    pub removed: usize,
    /// Files that couldn't be read, and why.
    pub failed: Vec<(PathBuf, String)>,
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// Where the library's folders are kept: `TEMPO_LIBRARY` (for tests), else
/// `~/Library/Application Support/tempo/library.json`.
pub fn config_file() -> PathBuf {
    match std::env::var_os("TEMPO_LIBRARY") {
        Some(dir) => PathBuf::from(dir).join("library.json"),
        None => home().join("Library/Application Support/tempo/library.json"),
    }
}

/// Where the index is kept: beside the config under `TEMPO_LIBRARY`, else
/// `~/Library/Caches/tempo/samples.json`.
pub fn index_file() -> PathBuf {
    match std::env::var_os("TEMPO_LIBRARY") {
        Some(dir) => PathBuf::from(dir).join("samples.json"),
        None => home().join("Library/Caches/tempo/samples.json"),
    }
}

/// The folder a library starts with: where tempo keeps samples it downloads.
pub fn default_folder() -> PathBuf {
    home().join("Music/tempo/Samples")
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

impl Library {
    /// The library as last saved: its folders, and the index (empty until a scan, if there's
    /// none).
    pub fn load() -> Library {
        let mut library: Library = std::fs::read(config_file())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_else(|| Library {
                folders: vec![default_folder()],
                samples: Vec::new(),
            });
        library.samples = std::fs::read(index_file())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        library
    }

    /// Saves its folders and index.
    pub fn save(&self) -> Result<(), String> {
        write(&config_file(), &serde_json::to_vec_pretty(self).unwrap())?;
        write(&index_file(), &serde_json::to_vec(&self.samples).unwrap())
    }

    /// Adds a folder (it's scanned next time).
    pub fn add_folder(&mut self, dir: &Path) -> Result<(), String> {
        let dir = dir
            .canonicalize()
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        if !dir.is_dir() {
            return Err(format!("{} isn't a folder", dir.display()));
        }
        if !self.folders.contains(&dir) {
            self.folders.push(dir);
        }
        Ok(())
    }

    /// Takes a folder out, and its samples from the index.
    pub fn remove_folder(&mut self, dir: &Path) {
        let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        self.folders.retain(|f| *f != dir);
        self.samples.retain(|s| !s.path.starts_with(&dir));
    }

    /// The sample at `path`, if the library has it.
    pub fn get(&self, path: &Path) -> Option<&Sample> {
        self.samples.iter().find(|s| s.path == path)
    }

    /// Brings the index up to date with the folders: analyses what's new or changed (on every
    /// core), drops what's gone, and saves. `progress` hears (done, to do) as it goes, from any
    /// thread.
    pub fn scan(&mut self, progress: &(dyn Fn(usize, usize) + Sync)) -> ScanReport {
        let found: Vec<(PathBuf, PathBuf)> = self
            .folders
            .iter()
            .flat_map(|root| {
                audio_files(root)
                    .into_iter()
                    .map(move |f| (root.clone(), f))
            })
            .collect();
        let known: HashMap<&Path, &Sample> =
            self.samples.iter().map(|s| (s.path.as_path(), s)).collect();
        let mut kept = Vec::new();
        let mut todo = Vec::new();
        for (root, path) in &found {
            let (modified, size) = stamp(path);
            match known.get(path.as_path()) {
                Some(s) if s.modified == modified && s.size == size => kept.push((*s).clone()),
                _ => todo.push((root, path, modified, size)),
            }
        }
        let here: std::collections::HashSet<&Path> =
            found.iter().map(|(_, f)| f.as_path()).collect();
        let removed = self
            .samples
            .iter()
            .filter(|s| !here.contains(s.path.as_path()))
            .count();
        let done = AtomicUsize::new(0);
        let results = Mutex::new(Vec::new());
        let failed = Mutex::new(Vec::new());
        let next = AtomicUsize::new(0);
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(|| {
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&(root, path, modified, size)) = todo.get(i) else {
                            break;
                        };
                        match analyze::analyze_file(path) {
                            Ok(analysis) => results.lock().unwrap().push(Sample {
                                path: path.clone(),
                                name: path
                                    .file_stem()
                                    .map(|s| s.to_string_lossy().into_owned())
                                    .unwrap_or_default(),
                                folder: path
                                    .parent()
                                    .and_then(|p| p.strip_prefix(root).ok())
                                    .map(|p| p.to_string_lossy().into_owned())
                                    .unwrap_or_default(),
                                modified,
                                size,
                                analysis,
                            }),
                            Err(e) => failed.lock().unwrap().push((path.clone(), e)),
                        }
                        progress(done.fetch_add(1, Ordering::Relaxed) + 1, todo.len());
                    }
                });
            }
        });
        let analysed = results.lock().unwrap().len();
        kept.extend(results.into_inner().unwrap());
        kept.sort_by(|a, b| a.path.cmp(&b.path));
        self.samples = kept;
        let report = ScanReport {
            samples: self.samples.len(),
            analysed,
            removed,
            failed: failed.into_inner().unwrap(),
        };
        if let Err(e) = self.save() {
            eprintln!("library: {e}");
        }
        report
    }
}

/// A file's modification time (seconds) and size.
fn stamp(path: &Path) -> (u64, u64) {
    let meta = std::fs::metadata(path).ok();
    let modified = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    (modified, meta.map_or(0, |m| m.len()))
}

/// Every audio file under `dir`, in subfolders too, skipping hidden ones and macOS's resource
/// forks ("._Kick.wav").
pub fn audio_files(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for path in entries.flatten().map(|e| e.path()) {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            if name.as_deref().is_none_or(|n| n.starts_with('.')) {
                continue;
            }
            if path.is_dir() {
                walk(&path, out);
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()))
            {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(path: &Path, seconds: f32) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..(44_100.0 * seconds) as usize {
            let s = (i as f32 * 0.05).sin() * 8000.0;
            w.write_sample(s as i16).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn a_scan_analyses_whats_new_and_forgets_whats_gone() {
        let base = std::env::temp_dir().join(format!("tempo-test-library-{}", std::process::id()));
        let pack = base.join("Pack/Drums");
        std::fs::create_dir_all(&pack).unwrap();
        tone(&pack.join("Kick_01.wav"), 0.3);
        tone(&pack.join("Snare_01.wav"), 0.3);
        std::fs::write(pack.join("._Kick_01.wav"), b"resource fork").unwrap();
        std::fs::write(pack.join("notes.txt"), b"not audio").unwrap();
        let mut library = Library {
            folders: vec![base.clone()],
            samples: Vec::new(),
        };
        // SAFETY: tests touching TEMPO_LIBRARY run in this one test.
        unsafe { std::env::set_var("TEMPO_LIBRARY", base.join("config")) };
        let report = library.scan(&|_, _| {});
        assert_eq!((report.samples, report.analysed, report.removed), (2, 2, 0));
        let kick = library.get(&pack.join("Kick_01.wav")).unwrap();
        assert_eq!(
            (kick.name.as_str(), kick.folder.as_str()),
            ("Kick_01", "Pack/Drums")
        );
        // Nothing changed: nothing analysed again. One gone: forgotten.
        assert_eq!(library.scan(&|_, _| {}).analysed, 0);
        std::fs::remove_file(pack.join("Snare_01.wav")).unwrap();
        let report = library.scan(&|_, _| {});
        assert_eq!((report.samples, report.removed), (1, 1));
        // Saved, it loads back.
        let again = Library::load();
        assert_eq!(again.samples.len(), 1);
        assert_eq!(again.folders, [base.clone()]);
        std::fs::remove_dir_all(base).unwrap();
    }
}
