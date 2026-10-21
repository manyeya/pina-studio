//! The COM objects Pina Studio hands to a plugin: the host, the component handler that hears its
//! editor's parameter edits, and the stream, event list and parameter changes it reads.
//!
//! The lists are filled and read on the audio thread, so they never allocate after creation.

use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
};

use vst3::{
    Class, ComWrapper,
    Steinberg::{IBStream_::IStreamSeekMode_::*, Vst::*, *},
};

use crate::plugin::EditSender;

pub struct HostApplication;

impl Class for HostApplication {
    type Interfaces = (IHostApplication,);
}

impl IHostApplicationTrait for HostApplication {
    unsafe fn getName(&self, name: *mut String128) -> tresult {
        let name = unsafe { &mut *name };
        for (dst, src) in name.iter_mut().zip("Pina Studio".encode_utf16().chain([0])) {
            *dst = src;
        }
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        _cid: *mut TUID,
        _iid: *mut TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        // ponytail: no host-made IMessage/IAttributeList; add when a plugin needs them.
        unsafe { *obj = std::ptr::null_mut() };
        kNotImplemented
    }
}

/// Hears parameter edits made in a plugin's editor and passes them to its processor.
pub struct ComponentHandler {
    edits: EditSender,
}

impl ComponentHandler {
    pub fn new(edits: EditSender) -> Self {
        ComponentHandler { edits }
    }

    /// Passes an edit made from Pina Studio itself to the processor.
    pub fn edit(&self, id: ParamID, value: ParamValue) {
        self.edits.push(id, value);
    }
}

impl Class for ComponentHandler {
    type Interfaces = (IComponentHandler,);
}

impl IComponentHandlerTrait for ComponentHandler {
    unsafe fn beginEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }

    unsafe fn performEdit(&self, id: ParamID, value: ParamValue) -> tresult {
        self.edits.push(id, value);
        kResultOk
    }

    unsafe fn endEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }

    unsafe fn restartComponent(&self, _flags: int32) -> tresult {
        // ponytail: ignores latency and parameter-list changes; handle with delay compensation.
        kResultOk
    }
}

/// A plugin's saved state, in memory.
#[derive(Default)]
pub struct MemoryStream {
    data: RefCell<Vec<u8>>,
    pos: Cell<usize>,
}

impl MemoryStream {
    pub fn rewind(&self) {
        self.pos.set(0);
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.data.borrow().clone()
    }
}

impl From<Vec<u8>> for MemoryStream {
    fn from(data: Vec<u8>) -> Self {
        MemoryStream {
            data: RefCell::new(data),
            pos: Cell::new(0),
        }
    }
}

impl Class for MemoryStream {
    type Interfaces = (IBStream,);
}

impl IBStreamTrait for MemoryStream {
    unsafe fn read(&self, buffer: *mut c_void, n: int32, read: *mut int32) -> tresult {
        let data = self.data.borrow();
        let pos = self.pos.get().min(data.len());
        let n = (n.max(0) as usize).min(data.len() - pos);
        unsafe { std::ptr::copy_nonoverlapping(data[pos..].as_ptr(), buffer.cast(), n) };
        self.pos.set(pos + n);
        if !read.is_null() {
            unsafe { *read = n as int32 };
        }
        kResultOk
    }

    unsafe fn write(&self, buffer: *mut c_void, n: int32, written: *mut int32) -> tresult {
        let n = n.max(0) as usize;
        let src = unsafe { std::slice::from_raw_parts(buffer.cast::<u8>(), n) };
        let mut data = self.data.borrow_mut();
        let pos = self.pos.get();
        if data.len() < pos + n {
            data.resize(pos + n, 0);
        }
        data[pos..pos + n].copy_from_slice(src);
        self.pos.set(pos + n);
        if !written.is_null() {
            unsafe { *written = n as int32 };
        }
        kResultOk
    }

    #[allow(non_upper_case_globals)]
    unsafe fn seek(&self, pos: int64, mode: int32, result: *mut int64) -> tresult {
        let base = match mode as _ {
            kIBSeekSet => 0,
            kIBSeekCur => self.pos.get() as int64,
            kIBSeekEnd => self.data.borrow().len() as int64,
            _ => return kInvalidArgument,
        };
        let pos = (base + pos).max(0);
        self.pos.set(pos as usize);
        if !result.is_null() {
            unsafe { *result = pos };
        }
        kResultOk
    }

    unsafe fn tell(&self, pos: *mut int64) -> tresult {
        if pos.is_null() {
            return kInvalidArgument;
        }
        unsafe { *pos = self.pos.get() as int64 };
        kResultOk
    }
}

/// Note events for one `process` call.
pub struct EventList {
    events: RefCell<Vec<Event>>,
}

impl EventList {
    pub fn with_capacity(n: usize) -> Self {
        EventList {
            events: RefCell::new(Vec::with_capacity(n)),
        }
    }

    pub fn clear(&self) {
        self.events.borrow_mut().clear();
    }

    /// Adds an event, unless the list is full: it never grows on the audio thread.
    pub fn push(&self, e: Event) {
        let mut events = self.events.borrow_mut();
        if events.len() < events.capacity() {
            events.push(e);
        }
    }
}

impl Class for EventList {
    type Interfaces = (IEventList,);
}

impl IEventListTrait for EventList {
    unsafe fn getEventCount(&self) -> int32 {
        self.events.borrow().len() as int32
    }

    unsafe fn getEvent(&self, index: int32, e: *mut Event) -> tresult {
        match self.events.borrow().get(index as usize) {
            Some(ev) if !e.is_null() => {
                unsafe { *e = *ev };
                kResultOk
            }
            _ => kInvalidArgument,
        }
    }

    unsafe fn addEvent(&self, e: *mut Event) -> tresult {
        if e.is_null() {
            return kInvalidArgument;
        }
        self.push(unsafe { *e });
        kResultOk
    }
}

const POINTS: usize = 16;

/// One parameter's changes within a block.
pub struct ParamQueue {
    id: Cell<ParamID>,
    points: RefCell<Vec<(int32, ParamValue)>>,
}

impl Class for ParamQueue {
    type Interfaces = (IParamValueQueue,);
}

impl IParamValueQueueTrait for ParamQueue {
    unsafe fn getParameterId(&self) -> ParamID {
        self.id.get()
    }

    unsafe fn getPointCount(&self) -> int32 {
        self.points.borrow().len() as int32
    }

    unsafe fn getPoint(&self, index: int32, offset: *mut int32, value: *mut ParamValue) -> tresult {
        match self.points.borrow().get(index as usize) {
            Some(&(o, v)) if !offset.is_null() && !value.is_null() => {
                unsafe {
                    *offset = o;
                    *value = v;
                }
                kResultOk
            }
            _ => kInvalidArgument,
        }
    }

    unsafe fn addPoint(&self, offset: int32, value: ParamValue, index: *mut int32) -> tresult {
        let mut points = self.points.borrow_mut();
        // A later point at the same offset replaces the earlier one.
        if let Some(last) = points.last_mut().filter(|p| p.0 == offset) {
            last.1 = value;
        } else if points.len() < POINTS {
            points.push((offset, value));
        } else {
            return kResultFalse;
        }
        if !index.is_null() {
            unsafe { *index = points.len() as int32 - 1 };
        }
        kResultOk
    }
}

/// Parameter changes for one `process` call: a fixed pool of queues, one per changed parameter.
pub struct ParameterChanges {
    queues: Vec<ComWrapper<ParamQueue>>,
    used: Cell<usize>,
}

impl ParameterChanges {
    pub fn with_capacity(n: usize) -> Self {
        let queues = (0..n)
            .map(|_| {
                ComWrapper::new(ParamQueue {
                    id: Cell::new(0),
                    points: RefCell::new(Vec::with_capacity(POINTS)),
                })
            })
            .collect();
        ParameterChanges {
            queues,
            used: Cell::new(0),
        }
    }

    pub fn clear(&self) {
        self.used.set(0);
    }

    pub fn add(&self, id: ParamID, offset: int32, value: ParamValue) {
        let q = unsafe { self.addParameterData(&id, std::ptr::null_mut()) };
        if !q.is_null() {
            let i = (0..self.used.get())
                .find(|&i| self.queues[i].id.get() == id)
                .unwrap();
            unsafe { self.queues[i].addPoint(offset, value, std::ptr::null_mut()) };
        }
    }
}

impl Class for ParameterChanges {
    type Interfaces = (IParameterChanges,);
}

impl IParameterChangesTrait for ParameterChanges {
    unsafe fn getParameterCount(&self) -> int32 {
        self.used.get() as int32
    }

    unsafe fn getParameterData(&self, index: int32) -> *mut IParamValueQueue {
        match self.queues[..self.used.get()].get(index as usize) {
            Some(q) => q.as_com_ref::<IParamValueQueue>().unwrap().as_ptr(),
            None => std::ptr::null_mut(),
        }
    }

    unsafe fn addParameterData(
        &self,
        id: *const ParamID,
        index: *mut int32,
    ) -> *mut IParamValueQueue {
        if id.is_null() {
            return std::ptr::null_mut();
        }
        let id = unsafe { *id };
        let used = self.used.get();
        let i = match (0..used).find(|&i| self.queues[i].id.get() == id) {
            Some(i) => i,
            None if used < self.queues.len() => {
                self.queues[used].id.set(id);
                self.queues[used].points.borrow_mut().clear();
                self.used.set(used + 1);
                used
            }
            None => return std::ptr::null_mut(),
        };
        if !index.is_null() {
            unsafe { *index = i as int32 };
        }
        self.queues[i]
            .as_com_ref::<IParamValueQueue>()
            .unwrap()
            .as_ptr()
    }
}

#[cfg(test)]
mod tests;
