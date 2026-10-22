//! The unit's editor: its own view, or the generic one.

use super::*;

/// The unit's own Cocoa view, made by the factory class its view bundle names.
pub(crate) unsafe fn cocoa_view(au: AudioUnit) -> Option<Retained<NSView>> {
    // An `AudioUnitCocoaViewInfo`: the view bundle's URL, then class names, ours to release.
    let words = unsafe { property_list(au, kAudioUnitProperty_CocoaUI, ptr::null::<c_void>()) }?;
    let (url, classes) = words.split_first()?;
    let url = owned(url.cast::<CFURL>());
    let classes: Vec<CFRetained<CFString>> = classes
        .iter()
        .filter_map(|&p| owned(p.cast::<CFString>()))
        .collect();
    let path = url?.to_file_path()?;
    let bundle = NSBundle::bundleWithPath(&NSString::from_str(path.to_str()?))?;
    let class: &AnyClass = bundle.classNamed(&NSString::from_str(&classes.first()?.to_string()))?;
    let pick = sel!(uiViewForAudioUnit:withSize:);
    if !class.responds_to(pick) {
        return None;
    }
    let factory: Retained<AnyObject> = unsafe { objc2::msg_send![class, new] };
    let view_for: unsafe extern "C-unwind" fn(
        *const AnyObject,
        Sel,
        AudioUnit,
        NSSize,
    ) -> *mut NSView = unsafe { send() };
    let view = unsafe { view_for(&*factory, pick, au, NSSize::new(800.0, 600.0)) };
    unsafe { Retained::retain(view) }
}

/// Apple's generic editor, a slider for each parameter: what units without one of their own get.
pub(crate) unsafe fn generic_view(au: AudioUnit) -> Option<Retained<NSView>> {
    let class = AnyClass::get(c"AUGenericView")?;
    let alloc: unsafe extern "C-unwind" fn(*const AnyClass, Sel) -> *mut AnyObject =
        unsafe { send() };
    let init: unsafe extern "C-unwind" fn(*mut AnyObject, Sel, AudioUnit) -> *mut NSView =
        unsafe { send() };
    let view = unsafe { init(alloc(class, sel!(alloc)), sel!(initWithAudioUnit:), au) };
    unsafe { Retained::from_raw(view) }
}
