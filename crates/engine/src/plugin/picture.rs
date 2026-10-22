//! A plugin's picture, taken in a child process (`pina __picture`), as the scan is: opened, timed
//! for how hard it works the computer, its window shown and photographed, closed. A plugin that
//! crashes or hangs takes only its picture down.

use std::{
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::{plugin, song::project::PluginRef};

/// How long a plugin gets to open, draw its window and be photographed.
const TIMEOUT: Duration = Duration::from_secs(25);
/// How long its window has to draw itself first.
const SETTLE: f64 = 1.5;
const RATE: f64 = 48_000.0;
const BLOCK: usize = 512;

/// What taking a plugin's picture found: its load (0 to 1 of real time), and its picture's size
/// (none if it has no window).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct Snapshot {
    pub load: f32,
    pub w: u32,
    pub h: u32,
    pub windowless: bool,
}

/// Opens the plugin, times it, shows its window and photographs it to `out`. What
/// `pina __picture` runs, in a process of its own.
#[cfg(target_os = "macos")]
pub fn take(r: &PluginRef, instrument: bool, out: &Path) -> Result<Snapshot, String> {
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    use objc2_foundation::{NSDate, NSRunLoop};
    let mtm = plugin::MainThreadMarker::new().ok_or("pictures are taken on the main thread")?;
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let (plugin, mut processor) =
        plugin::instantiate(&mut plugin::Libraries::new(), r, RATE, BLOCK, None)?;
    let load = plugin::load(&mut processor, instrument, RATE, BLOCK);
    let Ok(editor) = plugin.open_editor(mtm) else {
        return Ok(Snapshot {
            load,
            windowless: true,
            ..Default::default()
        });
    };
    editor.show();
    NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(SETTLE));
    let (w, h) = editor.window.picture(out)?;
    drop(editor);
    drop(plugin);
    drop(processor);
    Ok(Snapshot {
        load,
        w,
        h,
        windowless: false,
    })
}

#[cfg(not(target_os = "macos"))]
pub fn take(_r: &PluginRef, _instrument: bool, _out: &Path) -> Result<Snapshot, String> {
    Err("pictures of plugins are taken on macOS".into())
}

/// Takes a plugin's picture with `exe __picture`, giving up if it crashes or takes too long.
pub fn snapshot(
    exe: &Path,
    r: &PluginRef,
    instrument: bool,
    out: &Path,
) -> Result<Snapshot, String> {
    let r = serde_json::to_string(r).map_err(|e| e.to_string())?;
    let mut child = Command::new(exe)
        .arg("__picture")
        .arg(r)
        .arg(if instrument { "instrument" } else { "effect" })
        .arg(out)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("its window didn't finish drawing".into());
            }
        }
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr);
        let said = said
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("");
        return Err(match said {
            "" => "it stopped while opening".into(),
            s => s.to_string(),
        });
    }
    serde_json::from_slice(&out.stdout).map_err(|_| "it said nothing back".into())
}
