//! Windows: a plugin's binary is a DLL, its editor's window a Win32 window.

use std::{
    ffi::{CStr, c_void},
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr,
    sync::{Once, OnceLock},
    thread::ThreadId,
};

use windows_sys::Win32::{
    Foundation::{HMODULE, HWND, LPARAM, LRESULT, RECT, WPARAM},
    System::LibraryLoader::{
        GetModuleHandleW, GetProcAddress, LOAD_WITH_ALTERED_SEARCH_PATH, LoadLibraryExW,
    },
    UI::WindowsAndMessaging::{
        AdjustWindowRectEx, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow,
        IDC_ARROW, LoadCursorW, RegisterClassW, SW_HIDE, SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOZORDER, SetForegroundWindow, SetWindowPos, ShowWindow, WM_CLOSE, WNDCLASSW,
        WS_CAPTION, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
    },
};

/// Proof of being on the main thread, where editors open. The first thread to ask is taken to
/// be it: the app's, which opens the first editor.
// ponytail: first caller wins; ask the UI's thread id at start-up if a plugin calls in first.
#[derive(Clone, Copy)]
pub struct MainThreadMarker(());

static MAIN: OnceLock<ThreadId> = OnceLock::new();

impl MainThreadMarker {
    pub fn new() -> Option<MainThreadMarker> {
        let here = std::thread::current().id();
        (*MAIN.get_or_init(|| here) == here).then_some(MainThreadMarker(()))
    }
}

/// A plugin's binary, loaded: its DLL (a VST3 bundle's is in `Contents/x86_64-win`). Never
/// unloaded.
#[derive(Clone, Copy)]
pub struct Binary(HMODULE);

unsafe impl Send for Binary {}
unsafe impl Sync for Binary {}

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

impl Binary {
    pub fn load(path: &Path) -> Result<Binary, String> {
        let file = match path.is_dir() {
            true => path
                .join("Contents")
                .join("x86_64-win")
                .join(path.file_name().unwrap_or_default()),
            false => path.to_path_buf(),
        };
        // Its own folder is searched for the DLLs it needs.
        let module = unsafe {
            LoadLibraryExW(
                wide(file.as_os_str()).as_ptr(),
                ptr::null_mut(),
                LOAD_WITH_ALTERED_SEARCH_PATH,
            )
        };
        if module.is_null() {
            return Err(format!(
                "{}: couldn't load it ({})",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }
        Ok(Binary(module))
    }

    /// A function it exports, as `F`.
    ///
    /// # Safety
    /// `F` must be the function's real type.
    pub unsafe fn function<F: Copy>(&self, name: &CStr) -> Option<F> {
        let f = self.data(name);
        (!f.is_null()).then(|| unsafe { std::mem::transmute_copy(&f) })
    }

    /// Data (or a function) it exports; null if there's none of that name.
    pub fn data(&self, name: &CStr) -> *const c_void {
        let f = unsafe { GetProcAddress(self.0, name.as_ptr().cast()) };
        f.map_or(ptr::null(), |f| f as *const c_void)
    }

    /// What a format's entry point is handed: the module.
    pub fn handle(&self) -> *const c_void {
        self.0 as *const c_void
    }
}

const STYLE: u32 = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;

/// The class editor windows are made of, registered once: closing one hides it, as on macOS,
/// so the plugin's view stays attached until the editor is dropped.
fn class() -> *const u16 {
    static NAME: OnceLock<Vec<u16>> = OnceLock::new();
    static REGISTER: Once = Once::new();
    let name = NAME.get_or_init(|| wide("PinaStudioPluginEditor".as_ref()));
    REGISTER.call_once(|| unsafe {
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: GetModuleHandleW(ptr::null()),
            lpszClassName: name.as_ptr(),
            hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
            ..std::mem::zeroed()
        };
        RegisterClassW(&class);
    });
    name.as_ptr()
}

unsafe extern "system" fn window_proc(window: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if message == WM_CLOSE {
        unsafe { ShowWindow(window, SW_HIDE) };
        return 0;
    }
    unsafe { DefWindowProcW(window, message, w, l) }
}

/// The outer size of a window `width` by `height` inside.
fn outer(width: f64, height: f64) -> (i32, i32) {
    let mut r = RECT {
        left: 0,
        top: 0,
        right: width.round() as i32,
        bottom: height.round() as i32,
    };
    unsafe { AdjustWindowRectEx(&mut r, STYLE, 0, 0) };
    (r.right - r.left, r.bottom - r.top)
}

/// A plugin editor's window.
// ponytail: sizes are taken as pixels; scale them by the monitor's DPI if editors look small.
#[derive(Clone, Copy)]
pub struct Window(HWND);

unsafe impl Send for Window {}

impl Window {
    /// A window `size` pixels inside, titled with the plugin's name.
    pub fn new(_main: MainThreadMarker, title: &str, size: (f64, f64)) -> Window {
        let (w, h) = outer(size.0, size.1);
        let window = unsafe {
            CreateWindowExW(
                0,
                class(),
                wide(title.as_ref()).as_ptr(),
                STYLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                w,
                h,
                ptr::null_mut(),
                ptr::null_mut(),
                GetModuleHandleW(ptr::null()),
                ptr::null(),
            )
        };
        Window(window)
    }

    /// What the plugin's view goes in: the window itself.
    /// A picture of what the window shows: macOS's for now.
    pub fn picture(&self, _out: &Path) -> Result<(u32, u32), String> {
        Err("pictures of plugins are taken on macOS".into())
    }

    pub fn parent(&self) -> *mut c_void {
        self.0
    }

    pub fn show(&self) {
        unsafe {
            ShowWindow(self.0, SW_SHOW);
            SetForegroundWindow(self.0);
        }
    }

    pub fn hide(&self) {
        unsafe { ShowWindow(self.0, SW_HIDE) };
    }

    pub fn close(&self) {
        unsafe { DestroyWindow(self.0) };
    }

    pub fn set_content_size(&self, width: f64, height: f64) {
        let (w, h) = outer(width, height);
        unsafe {
            SetWindowPos(
                self.0,
                ptr::null_mut(),
                0,
                0,
                w,
                h,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
    }

    /// The window as a pointer, for a callback to resize it by (`resize_raw`).
    pub fn as_raw(&self) -> *mut c_void {
        self.0
    }

    /// Resizes the window `as_raw` gave. Main thread only, while it's open.
    ///
    /// # Safety
    /// `raw` must come from `as_raw` of a window still open.
    pub unsafe fn resize_raw(raw: *mut c_void, width: f64, height: f64) {
        Window(raw).set_content_size(width, height);
    }
}
