//! A `.vst3` bundle: loaded once, its classes listed, one made.

use super::*;

/// A loaded `.vst3` bundle.
// ponytail: never unloaded (no bundleExit); add when the plugin browser rescans.
pub struct Module {
    pub(crate) factory: ComPtr<IPluginFactory>,
}

pub struct ClassInfo {
    pub name: String,
    pub category: String,
    pub vendor: String,
    /// An instrument rather than an effect, by its subcategories.
    pub instrument: bool,
    /// Its subcategories: "Fx", "Reverb"; "Instrument", "Synth".
    pub sub: Vec<String>,
    pub(crate) cid: TUID,
}

impl Module {
    pub fn load(path: &Path) -> Result<Module, String> {
        let fail = |why: &str| format!("{}: {why}", path.display());
        unsafe {
            let p =
                CString::new(path.as_os_str().as_encoded_bytes()).map_err(|_| fail("bad path"))?;
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
            let entry: unsafe extern "C" fn(*const c_void) -> bool =
                symbol(bundle, c"bundleEntry").ok_or_else(|| fail("no bundleEntry"))?;
            let get_factory: unsafe extern "C" fn() -> *mut IPluginFactory =
                symbol(bundle, c"GetPluginFactory").ok_or_else(|| fail("no GetPluginFactory"))?;
            if !entry(bundle) {
                return Err(fail("bundleEntry failed"));
            }
            let factory =
                ComPtr::from_raw(get_factory()).ok_or_else(|| fail("no plugin factory"))?;
            Ok(Module { factory })
        }
    }

    pub fn classes(&self) -> Vec<ClassInfo> {
        let factory2 = self.factory.cast::<IPluginFactory2>();
        unsafe {
            (0..self.factory.countClasses())
                .filter_map(|i| {
                    if let Some(f) = &factory2 {
                        let mut info: PClassInfo2 = std::mem::zeroed();
                        if f.getClassInfo2(i, &mut info) == kResultOk {
                            return Some(ClassInfo {
                                name: text(&info.name),
                                category: text(&info.category),
                                vendor: text(&info.vendor),
                                instrument: text(&info.subCategories).contains("Instrument"),
                                sub: text(&info.subCategories)
                                    .split('|')
                                    .filter(|s| !s.is_empty())
                                    .map(str::to_string)
                                    .collect(),
                                cid: info.cid,
                            });
                        }
                    }
                    let mut info: PClassInfo = std::mem::zeroed();
                    (self.factory.getClassInfo(i, &mut info) == kResultOk).then(|| ClassInfo {
                        name: text(&info.name),
                        category: text(&info.category),
                        vendor: String::new(),
                        instrument: false,
                        sub: Vec::new(),
                        cid: info.cid,
                    })
                })
                .collect()
        }
    }

    pub(crate) unsafe fn create<I: Interface>(&self, cid: &TUID) -> Option<ComPtr<I>> {
        let mut obj = ptr::null_mut();
        let iid = I::IID;
        unsafe {
            (self
                .factory
                .createInstance(cid.as_ptr(), iid.as_ptr().cast(), &mut obj)
                == kResultOk)
                .then(|| ComPtr::from_raw(obj.cast()))
                .flatten()
        }
    }
}

pub(crate) unsafe fn symbol<F: Copy>(bundle: *const c_void, name: &CStr) -> Option<F> {
    const UTF8: u32 = 0x0800_0100;
    unsafe {
        let s = CFStringCreateWithCString(ptr::null(), name.as_ptr(), UTF8);
        let f = CFBundleGetFunctionPointerForName(bundle, s);
        CFRelease(s);
        (!f.is_null()).then(|| std::mem::transmute_copy(&f))
    }
}

pub(crate) fn text(chars: &[c_char]) -> String {
    let bytes: Vec<u8> = chars
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

pub(crate) fn com_ptr<C: Class, I: Interface>(obj: &ComWrapper<C>) -> *mut I {
    obj.as_com_ref::<I>().unwrap().as_ptr()
}

pub(crate) fn text16(chars: &[TChar]) -> String {
    let end = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
    String::from_utf16_lossy(&chars[..end])
}

/// The plugins in a `.vst3` bundle, opening it in this process.
pub fn inspect(bundle: &Path) -> Result<Vec<PluginInfo>, String> {
    let module = Module::load(bundle)?;
    Ok(module
        .classes()
        .into_iter()
        .filter(|c| c.category == AUDIO_MODULE)
        .map(|c| PluginInfo {
            format: crate::song::project::Format::Vst3,
            path: bundle.to_path_buf(),
            class: c.name,
            uid: None,
            vendor: c.vendor,
            instrument: c.instrument,
            categories: c.sub,
        })
        .collect())
}
