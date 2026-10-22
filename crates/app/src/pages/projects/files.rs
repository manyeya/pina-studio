//! Songs on disk: where they live, the recent ones, and making, duplicating, renaming and trashing them.

use super::*;

/// Where songs are kept: `PINA_SONGS`, else `~/Music/Pina Studio`.
pub fn root() -> PathBuf {
    pina_engine::paths::songs()
}

pub(crate) fn recent_file() -> PathBuf {
    pina_engine::paths::support().join("recent.json")
}

/// The songs opened lately, newest first.
pub fn recent() -> Vec<PathBuf> {
    std::fs::read(recent_file())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Remembers a song as just opened.
pub fn opened(dir: &Path) {
    if cfg!(test) {
        return;
    }
    let mut list = recent();
    list.retain(|d| d != dir);
    list.insert(0, dir.to_path_buf());
    list.truncate(20);
    let file = recent_file();
    let _ = std::fs::create_dir_all(file.parent().unwrap());
    let _ = std::fs::write(file, serde_json::to_vec_pretty(&list).unwrap());
}

/// What a song is, read from its folder.
pub fn summary(dir: &Path) -> Option<Song> {
    let p = Project::load(dir).ok()?;
    let modified = std::fs::metadata(dir.join(FILE))
        .and_then(|m| m.modified())
        .ok()?;
    let end = p.end().max(p.bar()) as f32;
    let none = |_: &Path| None;
    let mut seen: Vec<(Family, usize)> = Vec::new();
    let lanes = p
        .tracks
        .iter()
        .map(|t| {
            let l = look::look(t, &none);
            let n = match seen.iter_mut().find(|(f, _)| *f == l.family) {
                Some((_, n)) => {
                    *n += 1;
                    *n
                }
                None => {
                    seen.push((l.family, 0));
                    0
                }
            };
            let color = t
                .color
                .as_deref()
                .and_then(look::parse_color)
                .unwrap_or_else(|| family_shade(l.family, n));
            let spans = t
                .regions
                .iter()
                .map(|r| {
                    (
                        r.start as f32 / end,
                        ((r.start + r.length) as f32 / end).min(1.0),
                    )
                })
                .collect();
            (color, spans)
        })
        .collect();
    Some(Song {
        dir: dir.to_path_buf(),
        name: dir.file_name()?.to_string_lossy().into_owned(),
        modified,
        tempo: p.tempo,
        key: p.key.clone(),
        signature: p.time_signature,
        tracks: p.tracks.len(),
        seconds: p.end() as f64 / pina_engine::song::project::PPQ as f64 * 60.0 / p.tempo,
        lanes,
    })
}

/// Every song: in `root` (its folders that hold a project), and those opened from elsewhere;
/// newest first.
pub fn list(root: &Path, elsewhere: &[PathBuf]) -> Vec<Song> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|d| d.join(FILE).is_file())
        .collect();
    for d in elsewhere {
        if d.join(FILE).is_file() && !dirs.contains(d) {
            dirs.push(d.clone());
        }
    }
    let mut songs: Vec<Song> = dirs.iter().filter_map(|d| summary(d)).collect();
    songs.sort_by_key(|s| std::cmp::Reverse(s.modified));
    songs
}

/// A name no song in `root` has: `base`, else "base 2", "base 3"…
pub fn unique_name(root: &Path, base: &str) -> String {
    let base = base.trim();
    let base = if base.is_empty() { "Untitled" } else { base };
    (1..)
        .map(|n| match n {
            1 => base.to_string(),
            n => format!("{base} {n}"),
        })
        .find(|name| !root.join(name).exists())
        .unwrap()
}

/// Starts a song in `root`: its name made unique, its tempo, key and time, and a template's
/// tracks. Answers its folder.
pub fn create(
    root: &Path,
    name: &str,
    tempo: f64,
    key: Option<Key>,
    signature: [u64; 2],
    template: Template,
) -> Result<PathBuf, String> {
    if !(20.0..=999.0).contains(&tempo) {
        return Err(format!("a tempo of {tempo} BPM is out of 20 to 999"));
    }
    let dir = root.join(unique_name(root, name));
    let mut p = Project::new(48_000);
    p.tempo = tempo;
    p.key = key.map(|k| k.name());
    p.time_signature = signature;
    for (name, instrument) in template.tracks() {
        p.tracks.push(Track {
            name: name.into(),
            instrument,
            ..Default::default()
        });
    }
    p.assign_ids();
    p.save(&dir)?;
    Ok(dir)
}

/// A copy of a song beside it, "<name> copy": cloned, so it takes no space until it changes.
pub fn duplicate(dir: &Path) -> Result<PathBuf, String> {
    let parent = dir.parent().ok_or("no folder above it")?;
    let name = dir.file_name().ok_or("no name")?.to_string_lossy();
    let to = parent.join(unique_name(parent, &format!("{name} copy")));
    let status = std::process::Command::new("cp")
        .arg("-cR")
        .arg(dir)
        .arg(&to)
        .status()
        .map_err(|e| e.to_string())?;
    // Unsaved edits kept from the original stay with it.
    let _ = std::fs::remove_dir_all(to.join(pina_engine::song::project::RECOVERY_DIR));
    match status.success() {
        true => Ok(to),
        false => Err(format!("couldn't copy {}", dir.display())),
    }
}

/// Renames a song's folder (its name is its folder's).
pub fn rename(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() || name.contains('/') || name.starts_with('.') {
        return Err(format!("\u{201c}{name}\u{201d} can't name a song"));
    }
    let to = dir.parent().ok_or("no folder above it")?.join(name);
    if to == dir {
        return Ok(to);
    }
    if to.exists() {
        return Err(format!("there's a song called {name} already"));
    }
    std::fs::rename(dir, &to).map_err(|e| e.to_string())?;
    Ok(to)
}

/// Moves a song to the Trash (on Windows, the Recycle Bin), where it can be taken back from: as
/// the Finder or Explorer does, so it works on any volume and without special access. (Tests
/// remove it instead: they never touch the person's Trash.)
pub fn trash(dir: &Path) -> Result<(), String> {
    if cfg!(test) {
        return std::fs::remove_dir_all(dir).map_err(|e| e.to_string());
    }
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSFileManager, NSString, NSURL};
        let url = NSURL::fileURLWithPath(&NSString::from_str(&dir.to_string_lossy()));
        NSFileManager::defaultManager()
            .trashItemAtURL_resultingItemURL_error(&url, None)
            .map_err(|e| e.localizedDescription().to_string())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::{
            FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT,
            SHFILEOPSTRUCTW, SHFileOperationW,
        };
        // A list of one path: two nulls end it.
        let from: Vec<u16> = dir.as_os_str().encode_wide().chain([0, 0]).collect();
        let mut op = SHFILEOPSTRUCTW {
            wFunc: FO_DELETE,
            pFrom: from.as_ptr(),
            fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT) as u16,
            ..unsafe { std::mem::zeroed() }
        };
        match unsafe { SHFileOperationW(&mut op) } {
            0 => Ok(()),
            e => Err(format!(
                "{}: couldn't move it to the Recycle Bin ({e})",
                dir.display()
            )),
        }
    }
}

/// A new song not made yet, where songs are kept: what a window with no song open has.
pub fn scratch() -> PathBuf {
    let root = root();
    root.join(unique_name(&root, "Untitled"))
}

/// Shows a song's folder in the Finder (on Windows, Explorer).
pub fn reveal(dir: &Path) {
    crate::platform::reveal(dir);
}

/// "Today 08:23", "Yesterday", "12 Oct".
pub(crate) fn when(t: SystemTime) -> String {
    let ago = t.elapsed().map_or(0, |d| d.as_secs());
    match ago {
        0..60 => "Just now".into(),
        60..3_600 => format!("{} min ago", ago / 60),
        3_600..86_400 => format!("{} h ago", ago / 3_600),
        86_400..172_800 => "Yesterday".into(),
        _ => format!("{} days ago", ago / 86_400),
    }
}

pub(crate) fn length(seconds: f64) -> String {
    let s = seconds.round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}
