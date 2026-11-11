//! What the system knows of a device that cpal doesn't say: whether it's Bluetooth (its
//! microphone opened turns its headphones down to call quality, and it's late by more than it
//! says), and its stream's own latency, which comes after the device's and which cpal leaves
//! out of when a buffer's heard.

use std::time::Duration;

/// Whether the device named `name` is connected by Bluetooth.
pub fn is_bluetooth(name: &str) -> bool {
    imp::transport(name) == Some(u32::from_be_bytes(*b"blue"))
}

/// The output (or input) stream's own latency on the device named `name`.
pub fn stream_latency(name: &str, output: bool) -> Duration {
    imp::stream_latency(name, output).unwrap_or_default()
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::c_void;

    #[repr(C)]
    struct Address {
        selector: u32,
        scope: u32,
        element: u32,
    }

    #[link(name = "CoreAudio", kind = "framework")]
    unsafe extern "C" {
        fn AudioObjectGetPropertyDataSize(
            id: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
        ) -> i32;
        fn AudioObjectGetPropertyData(
            id: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
            data: *mut c_void,
        ) -> i32;
    }

    const SYSTEM: u32 = 1;
    const fn code(s: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*s)
    }

    fn get<T: Copy + Default>(id: u32, selector: &[u8; 4], scope: &[u8; 4]) -> Option<T> {
        let address = Address {
            selector: code(selector),
            scope: code(scope),
            element: 0,
        };
        let mut value = T::default();
        let mut size = std::mem::size_of::<T>() as u32;
        // SAFETY: `value` is a plain value `size` bytes long, as the property's data is.
        let status = unsafe {
            AudioObjectGetPropertyData(
                id,
                &address,
                0,
                std::ptr::null(),
                &mut size,
                &mut value as *mut T as *mut c_void,
            )
        };
        (status == 0).then_some(value)
    }

    fn ids(id: u32, selector: &[u8; 4], scope: &[u8; 4]) -> Vec<u32> {
        let address = Address {
            selector: code(selector),
            scope: code(scope),
            element: 0,
        };
        let mut size = 0u32;
        // SAFETY: CoreAudio writes the list's length in bytes, then at most that many.
        unsafe {
            if AudioObjectGetPropertyDataSize(id, &address, 0, std::ptr::null(), &mut size) != 0 {
                return Vec::new();
            }
            let mut out = vec![0u32; size as usize / 4];
            let ok = AudioObjectGetPropertyData(
                id,
                &address,
                0,
                std::ptr::null(),
                &mut size,
                out.as_mut_ptr() as *mut c_void,
            ) == 0;
            out.truncate(size as usize / 4);
            if ok { out } else { Vec::new() }
        }
    }

    fn name(id: u32) -> String {
        let address = Address {
            selector: code(b"name"),
            scope: code(b"glob"),
            element: 0,
        };
        let mut buf = [0u8; 256];
        let mut size = buf.len() as u32;
        // SAFETY: the name's written as a C string into `buf`, at most its length.
        unsafe {
            AudioObjectGetPropertyData(
                id,
                &address,
                0,
                std::ptr::null(),
                &mut size,
                buf.as_mut_ptr() as *mut c_void,
            )
        };
        let end = buf.iter().position(|&b| b == 0).unwrap_or(0);
        String::from_utf8_lossy(&buf[..end]).into_owned()
    }

    fn device(named: &str) -> Option<u32> {
        ids(SYSTEM, b"dev#", b"glob")
            .into_iter()
            .find(|&id| name(id) == named)
    }

    pub(super) fn transport(named: &str) -> Option<u32> {
        get(device(named)?, b"tran", b"glob")
    }

    pub(super) fn stream_latency(named: &str, output: bool) -> Option<std::time::Duration> {
        let id = device(named)?;
        let scope = if output { b"outp" } else { b"inpt" };
        let stream = *ids(id, b"stm#", scope).first()?;
        let frames: u32 = get(stream, b"ltnc", b"glob")?;
        let rate: f64 = get(id, b"nsrt", b"glob").filter(|r: &f64| *r > 0.0)?;
        Some(std::time::Duration::from_secs_f64(frames as f64 / rate))
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub(super) fn transport(_: &str) -> Option<u32> {
        None
    }

    pub(super) fn stream_latency(_: &str, _: bool) -> Option<std::time::Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_not_there_is_neither_bluetooth_nor_late() {
        assert!(!is_bluetooth("No Such Device 7"));
        assert_eq!(stream_latency("No Such Device 7", true), Duration::ZERO);
    }
}
