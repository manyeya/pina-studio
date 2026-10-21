//! Where Pina Studio keeps things on this Mac: songs in `~/Music/Pina Studio`, settings in
//! `~/Library/Application Support/Pina Studio`, caches in `~/Library/Caches/Pina Studio`.
//!
//! Before it was Pina Studio it was called tempo, and kept them under that name. `migrate` moves
//! them across once, puts the new places in the paths it had written down (songs opened lately,
//! sample folders), and leaves a link at each old place, so anything else that knew it still
//! finds it.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// The name its folders go by.
pub const NAME: &str = "Pina Studio";

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

/// Where songs are kept (`PINA_SONGS` says elsewhere).
pub fn songs() -> PathBuf {
    match std::env::var_os("PINA_SONGS") {
        Some(d) => d.into(),
        None => home().join("Music").join(NAME),
    }
}

/// Settings, and what's learned and kept: agents, playbooks, presets, the sample library's
/// folders, songs opened lately.
pub fn support() -> PathBuf {
    home().join("Library/Application Support").join(NAME)
}

/// What can be made again: the plugin scan, the sample index, studies.
pub fn caches() -> PathBuf {
    home().join("Library/Caches").join(NAME)
}

/// tempo's places, and Pina Studio's for each.
fn moves(home: &Path) -> [(PathBuf, PathBuf); 3] {
    [
        (home.join("Music/tempo"), home.join("Music").join(NAME)),
        (
            home.join("Library/Application Support/tempo"),
            home.join("Library/Application Support").join(NAME),
        ),
        (
            home.join("Library/Caches/tempo"),
            home.join("Library/Caches").join(NAME),
        ),
    ]
}

/// Moves what tempo kept to Pina Studio's places, once (a place already there is left as it is),
/// writes the songs' new place into the lists that named the old one, and leaves a link behind.
/// What moved.
pub fn migrate() -> Vec<PathBuf> {
    migrate_in(&home())
}

fn migrate_in(home: &Path) -> Vec<PathBuf> {
    let mut moved = Vec::new();
    for (old, new) in moves(home) {
        // A real folder (not the link a move left).
        if !fs::symlink_metadata(&old).is_ok_and(|m| m.is_dir()) {
            continue;
        }
        if !new.exists() {
            if let Some(parent) = new.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if fs::rename(&old, &new).is_err() {
                continue;
            }
        } else {
            // Begun there already (the installer's example songs): what isn't there goes in
            // beside it. Something of the same name in both, and the old place stays as it is.
            for entry in fs::read_dir(&old).into_iter().flatten().flatten() {
                let to = new.join(entry.file_name());
                match to.exists() {
                    true if entry.file_name() == ".DS_Store" => {
                        let _ = fs::remove_file(entry.path());
                    }
                    true => {}
                    false => {
                        let _ = fs::rename(entry.path(), to);
                    }
                }
            }
            if fs::remove_dir(&old).is_err() {
                continue;
            }
        }
        let _ = std::os::unix::fs::symlink(&new, &old);
        moved.push(new);
    }
    // Songs opened lately and sample folders were written down by their whole path.
    let (from, to) = (home.join("Music/tempo"), home.join("Music").join(NAME));
    if !moved.contains(&to) {
        return moved;
    }
    let (from, to) = (from.to_string_lossy(), to.to_string_lossy());
    for dir in [
        home.join("Library/Application Support").join(NAME),
        home.join("Library/Caches").join(NAME),
    ] {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = entry.path();
            if p.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&p) else {
                continue;
            };
            let fixed = text
                .replace(&format!("{from}/"), &format!("{to}/"))
                .replace(&format!("{from}\""), &format!("{to}\""));
            if fixed != text {
                let _ = fs::write(&p, fixed);
            }
        }
    }
    moved
}

#[cfg(test)]
mod tests;
