//! macOS: a plugin's binary is a bundle, its editor's window an `NSWindow`.

use std::{
    ffi::{CStr, CString, c_char, c_void},
    path::Path,
    ptr,
};

pub use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSBackingStoreType, NSWindow, NSWindowStyleMask};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFURLCreateFromFileSystemRepresentation(
        a: *const c_void,
        buf: *const u8,
        len: isize,
        dir: u8,
    ) -> *const c_void;
    fn CFBundleCreate(a: *const c_void, url: *const c_void) -> *const c_void;
    fn CFBundleLoadExecutable(bundle: *const c_void) -> u8;
    fn CFBundleGetFunctionPointerForName(
        bundle: *const c_void,
        name: *const c_void,
    ) -> *const c_void;
    fn CFBundleGetDataPointerForName(bundle: *const c_void, name: *const c_void) -> *const c_void;
    fn CFStringCreateWithCString(
        a: *const c_void,
        s: *const c_char,
        encoding: u32,
    ) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

/// A plugin's binary, loaded: its bundle. Never unloaded.
#[derive(Clone, Copy)]
pub struct Binary(*const c_void);

unsafe impl Send for Binary {}
unsafe impl Sync for Binary {}

impl Binary {
    pub fn load(path: &Path) -> Result<Binary, String> {
        let fail = |why: &str| format!("{}: {why}", path.display());
        let p = CString::new(path.as_os_str().as_encoded_bytes()).map_err(|_| fail("bad path"))?;
        unsafe {
            let url = CFURLCreateFromFileSystemRepresentation(
                ptr::null(),
                p.as_ptr().cast(),
                p.as_bytes().len() as isize,
                1,
            );
            if url.is_null() {
                return Err(fail("bad path"));
            }
            let bundle = CFBundleCreate(ptr::null(), url);
            CFRelease(url);
            if bundle.is_null() {
                return Err(fail("not a bundle"));
            }
            if CFBundleLoadExecutable(bundle) == 0 {
                return Err(fail("couldn't load its binary (wrong architecture?)"));
            }
            Ok(Binary(bundle))
        }
    }

    /// A function it exports, as `F`.
    ///
    /// # Safety
    /// `F` must be the function's real type.
    pub unsafe fn function<F: Copy>(&self, name: &CStr) -> Option<F> {
        let f = self.lookup(name, CFBundleGetFunctionPointerForName);
        (!f.is_null()).then(|| unsafe { std::mem::transmute_copy(&f) })
    }

    /// Data it exports (null if there's none of that name).
    pub fn data(&self, name: &CStr) -> *const c_void {
        self.lookup(name, CFBundleGetDataPointerForName)
    }

    /// What a format's entry point is handed: the bundle.
    pub fn handle(&self) -> *const c_void {
        self.0
    }

    fn lookup(
        &self,
        name: &CStr,
        get: unsafe extern "C" fn(*const c_void, *const c_void) -> *const c_void,
    ) -> *const c_void {
        const UTF8: u32 = 0x0800_0100;
        unsafe {
            let s = CFStringCreateWithCString(ptr::null(), name.as_ptr(), UTF8);
            let p = get(self.0, s);
            CFRelease(s);
            p
        }
    }
}

/// A plugin editor's window.
#[derive(Clone)]
pub struct Window(Retained<NSWindow>);

impl Window {
    /// A window `size` points inside, titled with the plugin's name.
    pub fn new(mtm: MainThreadMarker, title: &str, size: (f64, f64)) -> Window {
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                mtm.alloc(),
                NSRect::new(NSPoint::new(200.0, 200.0), NSSize::new(size.0, size.1)),
                NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::Miniaturizable,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        unsafe { window.setReleasedWhenClosed(false) };
        window.setTitle(&NSString::from_str(title));
        Window(window)
    }

    /// What the plugin's view goes in: the window's content view.
    pub fn parent(&self) -> *mut c_void {
        self.0
            .contentView()
            .map_or(ptr::null_mut(), |v| Retained::as_ptr(&v) as *mut c_void)
    }

    pub fn show(&self) {
        self.0.makeKeyAndOrderFront(None);
    }

    pub fn hide(&self) {
        self.0.orderOut(None);
    }

    pub fn close(&self) {
        self.0.close();
    }

    pub fn set_content_size(&self, width: f64, height: f64) {
        self.0.setContentSize(NSSize::new(width, height));
    }

    /// The window as a pointer, for a callback to resize it by (`resize_raw`).
    pub fn as_raw(&self) -> *mut c_void {
        Retained::as_ptr(&self.0) as *mut c_void
    }

    /// Resizes the window `as_raw` gave. Main thread only, while it's open.
    ///
    /// # Safety
    /// `raw` must come from `as_raw` of a window still open.
    pub unsafe fn resize_raw(raw: *mut c_void, width: f64, height: f64) {
        unsafe { (*(raw as *mut NSWindow)).setContentSize(NSSize::new(width, height)) };
    }

    /// The `NSWindow` itself, for what only macOS does with it.
    pub fn ns(&self) -> &NSWindow {
        &self.0
    }
}
