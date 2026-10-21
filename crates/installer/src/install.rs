//! What the installer does, its window aside: where Pina Studio goes and what's there already,
//! putting it there (the old one swapped out whole, never half-replaced), the `pina` command in
//! Terminal, the example songs, and taking it all out again. Each step is a plain function, so the
//! window can run them one after another off its thread and say how each went.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// The app, as it's named in Applications.
pub const APP: &str = "Pina Studio.app";

/// Where everything comes from and goes.
#[derive(Clone, Debug)]
pub struct Plan {
    /// The Pina Studio.app to install, and the example songs: the installer's own.
    pub payload: PathBuf,
    pub examples: Option<PathBuf>,
    /// Where apps go: /Applications, or ~/Applications if that can't be written to.
    pub apps: PathBuf,
    /// Where the `pina` command goes: on every Mac's PATH.
    pub bin: PathBuf,
    /// Where songs are kept (Pina Studio's own place for them).
    pub songs: PathBuf,
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

impl Plan {
    /// The installer's own: what it carries (beside it in its bundle, or named by
    /// `PINA_INSTALL_PAYLOAD`) and where it puts it (`PINA_INSTALL_APPS`, `PINA_INSTALL_BIN`
    /// and `PINA_SONGS` say elsewhere, to try it out without touching this Mac's).
    pub fn here() -> Plan {
        let var = |k: &str| std::env::var_os(k).map(PathBuf::from);
        let resources = std::env::current_exe()
            .ok()
            .and_then(|e| Some(e.parent()?.parent()?.join("Resources")))
            .unwrap_or_default();
        let payload = var("PINA_INSTALL_PAYLOAD").unwrap_or_else(|| resources.join(APP));
        let examples = payload
            .parent()
            .map(|p| p.join("Examples"))
            .filter(|e| e.is_dir());
        let apps = var("PINA_INSTALL_APPS").unwrap_or_else(|| {
            let system = PathBuf::from("/Applications");
            match writable(&system) {
                true => system,
                false => home().join("Applications"),
            }
        });
        Plan {
            payload,
            examples,
            apps,
            bin: var("PINA_INSTALL_BIN").unwrap_or_else(|| PathBuf::from("/usr/local/bin")),
            songs: var("PINA_SONGS").unwrap_or_else(|| home().join("Music/Pina Studio")),
        }
    }

    /// Where Pina Studio is (or will be) installed.
    pub fn app(&self) -> PathBuf {
        self.apps.join(APP)
    }
}

/// Whether a folder can be written to: a file made in it and taken away again.
fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".pina-installer-{}", std::process::id()));
    match fs::create_dir(&probe) {
        Ok(()) => {
            let _ = fs::remove_dir(&probe);
            true
        }
        Err(_) => false,
    }
}

/// An app's version, from its Info.plist.
pub fn version(app: &Path) -> Option<String> {
    let plist = fs::read_to_string(app.join("Contents/Info.plist")).ok()?;
    let at = plist.find("<key>CFBundleShortVersionString</key>")?;
    let rest = &plist[at..];
    let start = rest.find("<string>")? + "<string>".len();
    let end = rest[start..].find("</string>")?;
    Some(rest[start..start + end].trim().to_string())
}

/// What installing does to what's there: nothing there, an older one, the same, or a newer.
#[derive(Clone, Debug, PartialEq)]
pub enum Over {
    Nothing,
    Older(String),
    Same,
    Newer(String),
}

/// Versions compared number by number ("0.10.2" after "0.9.7"); a suffix ("-dev") comes before
/// the release it's for.
fn numbers(v: &str) -> (Vec<u64>, bool) {
    let (main, suffix) = v.split_once('-').map_or((v, false), |(m, _)| (m, true));
    let n = main.split('.').map(|p| p.parse().unwrap_or(0)).collect();
    (n, !suffix)
}

pub fn over(installed: Option<String>, new: &str) -> Over {
    let Some(old) = installed else {
        return Over::Nothing;
    };
    match numbers(&old).cmp(&numbers(new)) {
        std::cmp::Ordering::Less => Over::Older(old),
        std::cmp::Ordering::Equal => Over::Same,
        std::cmp::Ordering::Greater => Over::Newer(old),
    }
}

/// Whether Pina Studio's window is open from this app (not its command line, run by an agent).
pub fn running(app: &Path) -> bool {
    let exe = app.join("Contents/MacOS/pina");
    let exe = exe.to_string_lossy();
    let Ok(out) = Command::new("ps").args(["-axo", "command="]).output() else {
        return false;
    };
    String::from_utf8_lossy(&out.stdout).lines().any(|l| {
        let l = l.trim();
        l == exe
            || l.strip_prefix(exe.as_ref())
                .is_some_and(|r| r.starts_with(" /"))
    })
}

/// Puts Pina Studio where it goes: copied beside the old one, then swapped in, so one is there
/// whole at every moment. Not marked as downloaded: the installer was the one opened. tempo, as
/// it was called before, goes to the Trash.
pub fn copy_app(plan: &Plan) -> Result<PathBuf, String> {
    fs::create_dir_all(&plan.apps)
        .map_err(|e| format!("Couldn't make {}: {e}", plan.apps.display()))?;
    let dest = plan.app();
    let staging = plan.apps.join(format!(".{APP}.installing"));
    let old = plan.apps.join(format!(".{APP}.old"));
    let _ = fs::remove_dir_all(&staging);
    let _ = fs::remove_dir_all(&old);
    let copied = Command::new("ditto")
        .arg("--noqtn")
        .arg(&plan.payload)
        .arg(&staging)
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;
    if !copied.status.success() {
        let _ = fs::remove_dir_all(&staging);
        return Err(format!(
            "Couldn't copy Pina Studio into {}: {}",
            plan.apps.display(),
            String::from_utf8_lossy(&copied.stderr).trim()
        ));
    }
    if dest.exists() {
        fs::rename(&dest, &old)
            .map_err(|e| format!("Couldn't move the old Pina Studio aside: {e}"))?;
    }
    if let Err(e) = fs::rename(&staging, &dest) {
        // The old one back, as it was.
        let _ = fs::rename(&old, &dest);
        return Err(format!("Couldn't put Pina Studio in place: {e}"));
    }
    let _ = fs::remove_dir_all(&old);
    let tempo = plan.apps.join("tempo.app");
    if fs::read_to_string(tempo.join("Contents/Info.plist"))
        .is_ok_and(|p| p.contains("dev.tempo.daw"))
    {
        let _ = trash(&tempo);
    }
    Ok(dest)
}

/// Tells Launch Services about it: the Finder, Spotlight and the Dock see the new one now.
pub fn register(app: &Path) {
    let tool = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
    let _ = Command::new(tool).arg("-f").arg(app).status();
}

/// A path as one word for the shell.
fn quoted(p: &Path) -> String {
    format!("'{}'", p.to_string_lossy().replace('\'', "'\\''"))
}

/// Runs a shell command as an administrator: macOS asks for the password.
fn as_admin(command: &str) -> Result<(), String> {
    let escaped = command.replace('\\', "\\\\").replace('"', "\\\"");
    let out = Command::new("osascript")
        .arg("-e")
        .arg(format!(
            "do shell script \"{escaped}\" with administrator privileges"
        ))
        .output()
        .map_err(|e| e.to_string())?;
    match out.status.success() {
        true => Ok(()),
        false => {
            let why = String::from_utf8_lossy(&out.stderr);
            Err(match why.contains("-128") {
                true => "Cancelled".to_string(),
                false => why.trim().to_string(),
            })
        }
    }
}

/// Whether a link in `bin` is one an installer made: to a Pina Studio, or to tempo, as it was
/// called before.
fn ours(link: &Path) -> bool {
    fs::read_link(link).is_ok_and(|to| {
        let to = to.to_string_lossy();
        to.contains(&format!("{APP}/")) || to.contains("tempo.app/")
    })
}

/// The `pina` command, for Terminal and for agents: a link in `bin` to the app's own (and
/// tempo's old `tempo` command taken away). One that isn't Pina Studio's is left alone; where
/// `bin` needs an administrator, macOS asks.
pub fn command_line(app: &Path, bin: &Path) -> Result<PathBuf, String> {
    let link = bin.join("pina");
    let tempo = bin.join("tempo");
    let tempo = ours(&tempo).then_some(tempo);
    let target = app.join("Contents/MacOS/pina");
    match fs::symlink_metadata(&link) {
        Ok(m) if m.file_type().is_symlink() => {
            let to = fs::read_link(&link).unwrap_or_default();
            if to == target {
                if let Some(t) = &tempo {
                    let _ = fs::remove_file(t);
                }
                return Ok(link);
            }
            if !ours(&link) {
                return Err(format!(
                    "{} is another program's: left as it is",
                    link.display()
                ));
            }
        }
        Ok(_) => {
            return Err(format!(
                "{} is another program's: left as it is",
                link.display()
            ));
        }
        Err(_) => {}
    }
    let direct = fs::create_dir_all(bin).and_then(|_| {
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(&target, &link)?;
        tempo.as_ref().map_or(Ok(()), fs::remove_file)
    });
    match direct {
        Ok(()) => Ok(link),
        Err(e) if e.kind() == ErrorKind::PermissionDenied => {
            let tempo = tempo.map(|t| format!(" && rm -f {}", quoted(&t)));
            as_admin(&format!(
                "mkdir -p {} && ln -sf {} {}{}",
                quoted(bin),
                quoted(&target),
                quoted(&link),
                tempo.unwrap_or_default()
            ))?;
            Ok(link)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// The example songs, into where songs are kept: those not there already. How many went in.
pub fn examples(from: &Path, songs: &Path) -> Result<usize, String> {
    fs::create_dir_all(songs).map_err(|e| e.to_string())?;
    let mut added = 0;
    for entry in fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        let src = entry.path();
        let Some(name) = src.file_name() else {
            continue;
        };
        let to = songs.join(name);
        if !src.join("project.json").is_file() || to.exists() {
            continue;
        }
        let ok = Command::new("ditto")
            .arg("--noqtn")
            .arg(&src)
            .arg(&to)
            .status()
            .map_err(|e| e.to_string())?
            .success();
        if !ok {
            return Err(format!("Couldn't copy {}", name.to_string_lossy()));
        }
        added += 1;
    }
    Ok(added)
}

/// To the Trash, as the Finder sends things (tests take it away instead: they never touch this
/// Mac's Trash).
fn trash(p: &Path) -> Result<(), String> {
    if cfg!(test) {
        return fs::remove_dir_all(p).map_err(|e| e.to_string());
    }
    use objc2_foundation::{NSFileManager, NSString, NSURL};
    let url = NSURL::fileURLWithPath(&NSString::from_str(&p.to_string_lossy()));
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|e| e.localizedDescription().to_string())
}

/// Pina Studio's own settings and caches (never its songs), and the links tempo's old places are
/// now.
pub fn settings() -> Vec<PathBuf> {
    let lib = home().join("Library");
    let mut all = Vec::new();
    for name in ["Pina Studio", "tempo"] {
        all.push(lib.join("Application Support").join(name));
        all.push(lib.join("Caches").join(name));
    }
    all
}

/// Takes Pina Studio out: the app to the Trash, its `pina` command, and (if asked) its settings
/// and caches. Songs stay.
pub fn uninstall(plan: &Plan, and_settings: bool) -> Result<(), String> {
    let app = plan.app();
    if app.exists() {
        trash(&app).map_err(|e| format!("Couldn't move Pina Studio to the Trash: {e}"))?;
    }
    let link = plan.bin.join("pina");
    if ours(&link) && fs::remove_file(&link).is_err() {
        as_admin(&format!("rm -f {}", quoted(&link)))?;
    }
    if and_settings {
        for p in settings()
            .iter()
            .filter(|p| fs::symlink_metadata(p).is_ok())
        {
            trash(p)?;
        }
    }
    Ok(())
}

/// Opens Pina Studio.
pub fn launch(app: &Path) {
    let _ = Command::new("open").arg(app).status();
}

/// The disk image the installer came on, put away once it's gone.
pub fn eject_later() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut parts = exe.components().skip(1);
    let (Some(first), Some(volume)) = (parts.next(), parts.next()) else {
        return;
    };
    if first.as_os_str() != "Volumes" {
        return;
    }
    let volume = Path::new("/Volumes").join(volume.as_os_str());
    let _ = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "sleep 2; hdiutil detach {} -quiet",
            quoted(&volume)
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests;
