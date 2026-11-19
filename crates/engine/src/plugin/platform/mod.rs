//! What hosting plugins needs of the platform: a plugin's binary loaded, and a window for its
//! editor. On macOS those are a bundle and an `NSWindow`; on Windows a DLL and a Win32 window.

#[cfg(target_os = "macos")]
mod mac;
#[cfg(not(any(target_os = "macos", windows)))]
mod none;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "macos")]
pub use mac::*;
#[cfg(not(any(target_os = "macos", windows)))]
pub use none::*;
#[cfg(windows)]
pub use windows::*;
