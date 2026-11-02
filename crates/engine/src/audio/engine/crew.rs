//! The audio thread's helpers. Tracks that don't hear each other can be worked out at once, so each
//! block they're shared out between it and these threads, one for each fast core it hasn't got.
//!
//! Nothing here allocates or locks once it's made. Whoever is free takes the next task, so one
//! heavy track doesn't hold up the rest, and a helper that wakes late just finds less to do.

#[cfg(target_os = "macos")]
use std::ffi::c_void;
use std::{
    cell::UnsafeCell,
    hint::spin_loop,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    thread::{self, Thread},
};

/// How long a helper keeps looking before it sleeps, in tries: tens of microseconds, enough to
/// catch the next round of the same block without being woken.
const LOOKS: u32 = 2_000;

/// A closure, and how to call it with a task's number.
type Job = (*const (), unsafe fn(*const (), usize));

struct Work {
    /// The call, its tasks and how many are taken, in one word: nobody can take a task from a call
    /// that's over.
    ticket: AtomicU64,
    done: AtomicU32,
    /// The call's. Written only when every task of the last call is
    /// finished, read only by whoever holds a ticket of this one.
    job: UnsafeCell<Job>,
    stop: AtomicBool,
}

// `job` is guarded by the tickets, as it says; what it points at is `Sync`.
unsafe impl Sync for Work {}
unsafe impl Send for Work {}

/// Counts a task as finished however it ends: the audio thread is waiting on it.
struct Finished<'a>(&'a AtomicU32);

impl Drop for Finished<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Release);
    }
}

unsafe fn nothing(_: *const (), _: usize) {}

impl Work {
    /// Does the next task of the call if there is one. False once there are none left.
    fn take(&self) -> bool {
        let t = self.ticket.load(Ordering::Acquire);
        let (tasks, taken) = ((t >> 16) as u16, t as u16);
        if taken >= tasks {
            return false;
        }
        if self
            .ticket
            .compare_exchange_weak(t, t + 1, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            let _finished = Finished(&self.done);
            // Holding a ticket, the call can't return until this task is counted.
            unsafe {
                let (closure, call) = *self.job.get();
                call(closure, taken as usize);
            }
        }
        true
    }
}

pub struct Crew {
    work: Arc<Work>,
    helpers: Vec<Thread>,
    resting: bool,
}

impl Default for Crew {
    /// Nobody: everything is done by whoever asks.
    fn default() -> Self {
        Crew::new(0, 0.0)
    }
}

impl Crew {
    /// `helpers` threads, working to a deadline of `period` seconds.
    pub fn new(helpers: usize, period: f64) -> Crew {
        let work = Arc::new(Work {
            ticket: AtomicU64::new(0),
            done: AtomicU32::new(0),
            job: UnsafeCell::new((std::ptr::null(), nothing)),
            stop: AtomicBool::new(false),
        });
        let spawn = |_| {
            let work = work.clone();
            let run = move || help(&work, period);
            let thread = thread::Builder::new().name("pina audio".into()).spawn(run);
            thread.ok().map(|t| t.thread().clone())
        };
        let helpers = (0..helpers).filter_map(spawn).collect();
        let resting = false;
        Crew {
            work,
            helpers,
            resting,
        }
    }

    /// While resting, whoever asks does the work: the helpers aren't woken.
    pub fn rest(&mut self, resting: bool) {
        self.resting = resting;
    }

    /// Calls `task` once for each of `0..n`, across the crew, and returns when they're all done.
    pub fn run<F: Fn(usize) + Sync>(&self, n: usize, task: F) {
        if n < 2 || self.resting || self.helpers.is_empty() || n > u16::MAX as usize {
            (0..n).for_each(task);
            return;
        }
        unsafe fn call<F: Fn(usize)>(closure: *const (), i: usize) {
            unsafe { (*closure.cast::<F>())(i) }
        }
        let work = &*self.work;
        work.done.store(0, Ordering::Relaxed);
        // Every ticket of the last call was finished, so nobody is reading this.
        unsafe { *work.job.get() = ((&raw const task).cast(), call::<F>) };
        let this = (work.ticket.load(Ordering::Relaxed) >> 32).wrapping_add(1);
        work.ticket
            .store(this << 32 | (n as u64) << 16, Ordering::Release);
        self.helpers.iter().for_each(Thread::unpark);
        while work.take() {}
        while work.done.load(Ordering::Acquire) < n as u32 {
            spin_loop();
        }
    }
}

impl Drop for Crew {
    fn drop(&mut self) {
        self.work.stop.store(true, Ordering::Relaxed);
        self.helpers.iter().for_each(Thread::unpark);
    }
}

fn help(work: &Work, period: f64) {
    let mut token = [0; 10];
    let _joined = realtime(period, &mut token);
    while !work.stop.load(Ordering::Relaxed) {
        let mut looks = 0;
        while looks < LOOKS {
            looks = if work.take() { 0 } else { looks + 1 };
            spin_loop();
        }
        thread::park();
    }
}

/// How many fast cores there are, besides the audio thread's: what's worth having helpers for.
pub fn helpers_wanted() -> usize {
    let cores = thread::available_parallelism().map_or(1, |n| n.get());
    fast_cores().unwrap_or(cores / 2).saturating_sub(1).min(7)
}

#[cfg(target_os = "macos")]
fn fast_cores() -> Option<usize> {
    unsafe extern "C" {
        fn sysctlbyname(
            name: *const std::ffi::c_char,
            old: *mut std::ffi::c_void,
            old_size: *mut usize,
            new: *mut std::ffi::c_void,
            new_size: usize,
        ) -> i32;
    }
    let (mut n, mut size) = (0i32, size_of::<i32>());
    let got = unsafe {
        sysctlbyname(
            c"hw.perflevel0.logicalcpu".as_ptr(),
            (&raw mut n).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (got == 0 && n > 0).then_some(n as usize)
}

#[cfg(not(target_os = "macos"))]
fn fast_cores() -> Option<usize> {
    None
}

/// Makes this thread as urgent as the audio device's own: the audio thread waits on it, so one
/// that an ordinary thread could push aside would be a dropout. Its deadlines are the device's,
/// which also tells the system to keep it on a fast core.
#[cfg(target_os = "macos")]
fn realtime(period: f64, token: &mut [u32; 10]) -> Option<Joined<'_>> {
    #[repr(C)]
    struct Constraint {
        period: u32,
        computation: u32,
        constraint: u32,
        preemptible: u32,
    }
    #[repr(C)]
    struct Timebase {
        numer: u32,
        denom: u32,
    }
    #[repr(C)]
    struct Address {
        selector: u32,
        scope: u32,
        element: u32,
    }
    const TIME_CONSTRAINT: u32 = 2;
    const fn code(c: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*c)
    }
    const GLOBAL: u32 = code(b"glob");
    const MAIN: u32 = 0;
    const SYSTEM: u32 = 1;
    #[link(name = "CoreAudio", kind = "framework")]
    unsafe extern "C" {
        fn AudioObjectGetPropertyData(
            object: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
            data: *mut c_void,
        ) -> i32;
    }
    unsafe extern "C" {
        fn pthread_self() -> *mut c_void;
        fn pthread_mach_thread_np(thread: *mut c_void) -> u32;
        fn mach_timebase_info(info: *mut Timebase) -> i32;
        fn thread_policy_set(thread: u32, flavor: u32, policy: *const u32, count: u32) -> i32;
        fn os_workgroup_join(group: *mut c_void, token: *mut [u32; 10]) -> i32;
    }
    let mut base = Timebase { numer: 1, denom: 1 };
    unsafe { mach_timebase_info(&mut base) };
    let ticks = |seconds: f64| {
        (seconds * 1e9 * base.denom as f64 / base.numer.max(1) as f64).min(u32::MAX as f64) as u32
    };
    let period = period.clamp(0.001, 0.1);
    let policy = Constraint {
        period: ticks(period),
        computation: ticks(period * 0.5),
        constraint: ticks(period),
        preemptible: 1,
    };
    let count = (size_of::<Constraint>() / size_of::<u32>()) as u32;
    let thread = unsafe { pthread_mach_thread_np(pthread_self()) };
    unsafe { thread_policy_set(thread, TIME_CONSTRAINT, (&raw const policy).cast(), count) };
    // The output device's workgroup: if it has none, or this fails, the deadline above still holds.
    // shortcut: it's the system's output's, even if Settings chose another: upgrade by finding the
    // chosen device's id if helpers lag on an interface.
    let of = |object: u32, selector: u32, data: *mut c_void, size: u32| {
        let address = Address {
            selector,
            scope: GLOBAL,
            element: MAIN,
        };
        let mut size = size;
        let null = std::ptr::null();
        unsafe { AudioObjectGetPropertyData(object, &address, 0, null, &mut size, data) == 0 }
    };
    let mut device = 0u32;
    let mut group: *mut c_void = std::ptr::null_mut();
    let found = of(SYSTEM, code(b"dOut"), (&raw mut device).cast(), 4)
        && of(device, code(b"oswg"), (&raw mut group).cast(), 8)
        && !group.is_null();
    (found && unsafe { os_workgroup_join(group, token) } == 0).then_some(Joined { group, token })
}

/// In the device's workgroup. A thread that ends still in one is stopped dead by the system.
#[cfg(target_os = "macos")]
struct Joined<'a> {
    group: *mut c_void,
    token: &'a mut [u32; 10],
}

#[cfg(target_os = "macos")]
impl Drop for Joined<'_> {
    fn drop(&mut self) {
        unsafe extern "C" {
            fn os_workgroup_leave(group: *mut c_void, token: *mut [u32; 10]);
            fn os_release(object: *mut c_void);
        }
        unsafe {
            os_workgroup_leave(self.group, self.token);
            os_release(self.group);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn realtime(_period: f64, _token: &mut [u32; 10]) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn every_task_runs_once_on_every_call() {
        let crew = Crew::new(3, 0.01);
        let seen: Vec<AtomicUsize> = (0..64).map(|_| AtomicUsize::new(0)).collect();
        for round in 1..=2000 {
            let n = 2 + round % 62;
            crew.run(n, |i| {
                seen[i].fetch_add(1, Ordering::Relaxed);
            });
            let counts = seen.iter().map(|s| s.swap(0, Ordering::Relaxed));
            for (i, c) in counts.enumerate() {
                assert_eq!(c, (i < n) as usize, "task {i} of {n}, call {round}");
            }
        }
    }

    #[test]
    fn with_nobody_to_help_it_does_it_itself() {
        let mut order = std::sync::Mutex::new(Vec::new());
        Crew::default().run(4, |i| order.lock().unwrap().push(i));
        assert_eq!(*order.get_mut().unwrap(), [0, 1, 2, 3]);
    }
}
