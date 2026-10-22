//! The frame around a plugin's editor, which lets it resize its window.

use super::*;

/// Lets an editor resize its window.
pub struct PlugFrame {
    pub(crate) window: Retained<NSWindow>,
}

impl Class for PlugFrame {
    type Interfaces = (IPlugFrame,);
}

impl IPlugFrameTrait for PlugFrame {
    unsafe fn resizeView(&self, view: *mut IPlugView, size: *mut ViewRect) -> tresult {
        let (Some(view), false) = (unsafe { ComRef::from_raw(view) }, size.is_null()) else {
            return kInvalidArgument;
        };
        let r = unsafe { *size };
        self.window.setContentSize(NSSize::new(
            (r.right - r.left) as f64,
            (r.bottom - r.top) as f64,
        ));
        unsafe { view.onSize(size) };
        kResultOk
    }
}
