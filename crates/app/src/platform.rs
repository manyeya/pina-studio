//! What differs between macOS and Windows outside the window: showing and opening files, and
//! finding a command an agent runs as.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// The person's home folder.
pub fn home() -> PathBuf {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    PathBuf::from(std::env::var_os(var).unwrap_or_default())
}

/// Shows a file or folder where it is: in the Finder, or in Explorer.
pub fn reveal(path: &Path) {
    let _ = match cfg!(windows) {
        true => Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn(),
        false => Command::new("open").arg("-R").arg(path).spawn(),
    };
}

/// Opens a file or folder as the system would on a double-click.
pub fn open(path: &Path) -> std::io::Result<()> {
    let mut c = match cfg!(windows) {
        true => {
            let mut c = Command::new("explorer");
            c.arg(path);
            c
        }
        false => {
            let mut c = Command::new("open");
            // A text file in the text editor rather than whatever claims its extension.
            if path.is_file() {
                c.arg("-t");
            }
            c.arg(path);
            c
        }
    };
    c.status().map(|_| ())
}

/// A command's program: on the PATH or in `extra` folders (an app opened from the Finder has a
/// short PATH), and on Windows also as the `.exe` or `.cmd` an installer put there.
pub fn find_command(bin: &str, extra: &[PathBuf]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let names: Vec<String> = match cfg!(windows) {
        true => ["exe", "cmd", "bat"]
            .iter()
            .map(|e| format!("{bin}.{e}"))
            .collect(),
        false => vec![bin.to_string()],
    };
    std::env::split_paths(&path)
        .chain(extra.iter().cloned())
        .flat_map(|d| names.iter().map(move |n| d.join(n)))
        .find(|p| p.is_file())
}

/// Where installers put command-line tools that aren't on an app's PATH.
pub fn tool_folders() -> Vec<PathBuf> {
    let home = home();
    match cfg!(windows) {
        true => {
            let var = |v: &str| std::env::var_os(v).map(PathBuf::from).unwrap_or_default();
            vec![
                var("APPDATA").join("npm"),
                home.join(".local").join("bin"),
                var("LOCALAPPDATA").join("Programs"),
            ]
        }
        false => vec![
            home.join(".local/bin"),
            home.join(".claude/local"),
            "/opt/homebrew/bin".into(),
            "/usr/local/bin".into(),
        ],
    }
}
