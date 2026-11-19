//! A platform that hosts no plugins (the web): a main thread to say so on, and editor windows
//! that are never shown. Loading a plugin says it can't be done here.

use std::{ffi::c_void, path::Path};

/// Proof of being on the main thread: there's only the one.
#[derive(Clone, Copy)]
pub struct MainThreadMarker(());

impl MainThreadMarker {
    pub fn new() -> Option<MainThreadMarker> {
        Some(MainThreadMarker(()))
    }
}

/// An editor's window: none here, so nothing shows.
pub struct Window(());

impl Window {
    pub fn new(_main: MainThreadMarker, _title: &str, _size: (f64, f64)) -> Window {
        Window(())
    }

    pub fn picture(&self, _out: &Path) -> Result<(u32, u32), String> {
        Err("Plugins aren't hosted here".into())
    }

    pub fn parent(&self) -> *mut c_void {
        std::ptr::null_mut()
    }

    pub fn show(&self) {}

    pub fn hide(&self) {}

    pub fn close(&self) {}

    pub fn set_content_size(&self, _width: f64, _height: f64) {}

    pub fn as_raw(&self) -> *mut c_void {
        std::ptr::null_mut()
    }
}
