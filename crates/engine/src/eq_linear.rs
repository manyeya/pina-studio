//! Linear phase for the EQ: its bands' magnitude, nothing done to the phase. A finite impulse
//! response is made from the curve the bands draw (zero phase, centered, windowed), as long as the
//! resolution says (longer: truer low down, more latency); then the sound is convolved with it in
//! partitions of 256 (uniformly partitioned overlap-save), which costs little for even the longest.
//! Bands on the left or right alone get a kernel each side; bands on the mid or side, a second
//! stage on those (twice the latency). A new curve crossfades in over a partition.
//!
//! The kernels and the convolvers' memory are made off the audio thread, when the song's planned
//! (`Linear::new`); the EQ takes the memory the first time it needs it. An EQ moving as it plays
//! (automation, modulation) asks for new kernels as it goes, made on a thread of their own.

use std::{
    sync::{Arc, Mutex, OnceLock, Weak},
    thread,
    time::Duration,
};

use rustfft::{Fft, FftPlanner, num_complex::Complex};

use crate::{
    eq::{Section, band_sections},
    project::{Eq, EqBand, Placement, Shape},
};

/// Samples a partition: what the convolver works on at a time, and adds to the latency.
pub const PART: usize = 256;

/// The kernels' lengths, by resolution: 4096 (low) to 32768 (very high).
pub const LENGTHS: [usize; 4] = [4096, 8192, 16384, 32768];

/// A kernel's partitions, each as a spectrum of `2 * PART` bins.
struct Kernel {
    parts: Vec<Vec<Complex<f32>>>,
}

/// An EQ's linear phase, worked out: the kernels (left and right; mid and side if any band works
/// on those), the EQ they're for and their latency.
pub struct Linear {
    len: usize,
    lr: [Kernel; 2],
    ms: Option<[Kernel; 2]>,
    eq: Eq,
    pub latency: u32,
    /// Memory for the convolvers, made here for the EQ to take.
    spare: Mutex<Option<Box<State>>>,
    remake: Arc<Remake>,
}

/// New kernels for an EQ that moves as it plays: what it's asked for, what's made, and kernels
/// done with (to go here, not on the audio thread).
struct Remake {
    rate: f64,
    want: Mutex<Option<Eq>>,
    ready: Mutex<Option<Arc<Linear>>>,
    spent: Mutex<Vec<Arc<Linear>>>,
}

/// The kernels asked for of every EQ in linear phase, made a hundred times a second at most.
fn remakes() -> &'static Mutex<Vec<Weak<Remake>>> {
    static ALL: OnceLock<Mutex<Vec<Weak<Remake>>>> = OnceLock::new();
    ALL.get_or_init(|| {
        thread::spawn(|| {
            loop {
                thread::sleep(Duration::from_millis(10));
                let live: Vec<Arc<Remake>> = {
                    let mut all = remakes().lock().unwrap_or_else(|e| e.into_inner());
                    all.retain(|w| w.strong_count() > 0);
                    all.iter().filter_map(Weak::upgrade).collect()
                };
                for r in live {
                    let spent =
                        std::mem::take(&mut *r.spent.lock().unwrap_or_else(|e| e.into_inner()));
                    drop(spent);
                    let want = r.want.lock().unwrap_or_else(|e| e.into_inner()).take();
                    if let Some(eq) = want {
                        let made = Arc::new(Linear::make(&eq, r.rate, r.clone(), false));
                        *r.ready.lock().unwrap_or_else(|e| e.into_inner()) = Some(made);
                    }
                }
            }
        });
        Mutex::new(Vec::new())
    })
}

/// Whether two EQs draw the same curve, as kernels have it (static gains, bands that are on).
fn same_curve(a: &Eq, b: &Eq) -> bool {
    let curve = |b: &EqBand| {
        (
            b.shape,
            b.freq,
            b.gain_db,
            b.q,
            b.slope,
            b.placement,
            b.off,
            b.gain_q,
        )
    };
    a.scale == b.scale
        && a.resolution == b.resolution
        && a.bands.iter().map(curve).eq(b.bands.iter().map(curve))
}

/// How many samples linear phase holds an EQ back: half its kernel and a partition, a stage.
pub fn latency(eq: &Eq) -> u32 {
    let len = LENGTHS[eq.resolution.min(3) as usize];
    let stages = if two_stages(eq) { 2 } else { 1 };
    ((len / 2 + PART) * stages) as u32
}

fn two_stages(eq: &Eq) -> bool {
    eq.bands
        .iter()
        .any(|b| !b.off && matches!(b.placement, Placement::Mid | Placement::Side))
}

impl Linear {
    pub fn new(eq: &Eq, rate: f64) -> Linear {
        let remake = Arc::new(Remake {
            rate,
            want: Mutex::new(None),
            ready: Mutex::new(None),
            spent: Mutex::new(Vec::with_capacity(SPENT)),
        });
        remakes()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Arc::downgrade(&remake));
        Linear::make(eq, rate, remake, true)
    }

    fn make(eq: &Eq, rate: f64, remake: Arc<Remake>, memory: bool) -> Linear {
        let len = LENGTHS[eq.resolution.min(3) as usize];
        let mut planner = FftPlanner::<f32>::new();
        let make = |on: &dyn Fn(Placement) -> bool, planner: &mut FftPlanner<f32>| {
            kernel(eq, on, len, rate, planner)
        };
        let lr = [
            make(
                &|p| matches!(p, Placement::Stereo | Placement::Left),
                &mut planner,
            ),
            make(
                &|p| matches!(p, Placement::Stereo | Placement::Right),
                &mut planner,
            ),
        ];
        let ms = two_stages(eq).then(|| {
            [
                make(&|p| p == Placement::Mid, &mut planner),
                make(&|p| p == Placement::Side, &mut planner),
            ]
        });
        let state = memory.then(|| Box::new(State::new(len / PART, ms.is_some(), &mut planner)));
        Linear {
            len,
            lr,
            ms,
            eq: *eq,
            latency: latency(eq),
            spare: Mutex::new(state),
            remake,
        }
    }
}

/// Kernels done with that the audio thread keeps for the remaking thread to drop.
const SPENT: usize = 8;

/// A kernel for the bands `on` takes: their magnitude at each frequency (as their static gains
/// have it; what moves is the dynamic stage's), zero phase, centered and windowed, in partitions.
fn kernel(
    eq: &Eq,
    on: &dyn Fn(Placement) -> bool,
    len: usize,
    rate: f64,
    planner: &mut FftPlanner<f32>,
) -> Kernel {
    let sections: Vec<Vec<Section>> = eq
        .bands
        .iter()
        .filter(|b| !b.off && on(b.placement) && b.shape != Shape::AllPass)
        .map(|b| band_sections(b, eq.scale, rate))
        .collect();
    let half = len / 2;
    let mut spectrum: Vec<Complex<f32>> = vec![Complex::new(0.0, 0.0); len];
    for k in 0..=half {
        let f = k as f64 * rate / len as f64;
        let db: f64 = sections
            .iter()
            .map(|s| crate::eq::response_db(s, f.max(1.0), rate))
            .sum();
        let m = 10f64.powf(db / 20.0) as f32;
        spectrum[k] = Complex::new(m, 0.0);
        if k > 0 && k < half {
            spectrum[len - k] = Complex::new(m, 0.0);
        }
    }
    planner.plan_fft_inverse(len).process(&mut spectrum);
    // Centered (half its length on), windowed, scaled: an impulse response.
    let h: Vec<f32> = (0..len)
        .map(|i| {
            let c = spectrum[(i + half) % len].re / len as f32;
            let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / len as f32).cos();
            c * w
        })
        .collect();
    let fft = planner.plan_fft_forward(2 * PART);
    let parts = h
        .chunks(PART)
        .map(|chunk| {
            let mut buf = vec![Complex::new(0.0, 0.0); 2 * PART];
            for (b, &x) in buf.iter_mut().zip(chunk) {
                *b = Complex::new(x, 0.0);
            }
            fft.process(&mut buf);
            buf
        })
        .collect();
    Kernel { parts }
}

/// One channel's convolver: its input spectra, newest first round a ring, and its partitions in
/// and out.
struct Conv {
    fdl: Vec<Vec<Complex<f32>>>,
    head: usize,
    input: Vec<f32>,
    out: Vec<f32>,
}

impl Conv {
    fn new(k: usize) -> Conv {
        Conv {
            fdl: (0..k)
                .map(|_| vec![Complex::new(0.0, 0.0); 2 * PART])
                .collect(),
            head: 0,
            input: vec![0.0; 2 * PART],
            out: vec![0.0; PART],
        }
    }
}

/// The convolvers' memory: a channel each for left and right (and mid and side), the samples so
/// far of the partition coming, and the FFTs.
pub struct State {
    k: usize,
    convs: Vec<Conv>,
    fill: usize,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    buf: Vec<Complex<f32>>,
    acc: Vec<Complex<f32>>,
    old: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
}

impl State {
    fn new(k: usize, ms: bool, planner: &mut FftPlanner<f32>) -> State {
        let fft = planner.plan_fft_forward(2 * PART);
        let ifft = planner.plan_fft_inverse(2 * PART);
        let scratch = vec![
            Complex::new(0.0, 0.0);
            fft.get_inplace_scratch_len()
                .max(ifft.get_inplace_scratch_len())
        ];
        State {
            k,
            convs: (0..if ms { 4 } else { 2 }).map(|_| Conv::new(k)).collect(),
            fill: 0,
            fft,
            ifft,
            buf: vec![Complex::new(0.0, 0.0); 2 * PART],
            acc: vec![Complex::new(0.0, 0.0); 2 * PART],
            old: vec![Complex::new(0.0, 0.0); 2 * PART],
            scratch,
        }
    }

    /// A partition through a channel: its spectrum into the ring, the ring times the kernel
    /// (crossfading from the last kernel, if it's new), back out.
    fn partition(&mut self, c: usize, kernel: &Kernel, from: Option<&Kernel>) {
        let k = self.k;
        let conv = &mut self.convs[c];
        for (b, &x) in self.buf.iter_mut().zip(&conv.input) {
            *b = Complex::new(x, 0.0);
        }
        self.fft
            .process_with_scratch(&mut self.buf, &mut self.scratch);
        conv.fdl[conv.head].copy_from_slice(&self.buf);
        let sum = |kernel: &Kernel,
                   acc: &mut Vec<Complex<f32>>,
                   fdl: &[Vec<Complex<f32>>],
                   head: usize| {
            acc.iter_mut().for_each(|a| *a = Complex::new(0.0, 0.0));
            for (j, part) in kernel.parts.iter().enumerate().take(k) {
                let x = &fdl[(head + k - j) % k];
                for ((a, &p), &v) in acc.iter_mut().zip(part).zip(x) {
                    *a += p * v;
                }
            }
        };
        sum(kernel, &mut self.acc, &conv.fdl, conv.head);
        self.ifft
            .process_with_scratch(&mut self.acc, &mut self.scratch);
        let n = (2 * PART) as f32;
        match from {
            Some(old) => {
                sum(old, &mut self.old, &conv.fdl, conv.head);
                self.ifft
                    .process_with_scratch(&mut self.old, &mut self.scratch);
                for i in 0..PART {
                    let t = (i + 1) as f32 / PART as f32;
                    let (a, b) = (self.old[PART + i].re, self.acc[PART + i].re);
                    conv.out[i] = (a + (b - a) * t) / n;
                }
            }
            None => {
                for i in 0..PART {
                    conv.out[i] = self.acc[PART + i].re / n;
                }
            }
        }
        conv.input.copy_within(PART.., 0);
        conv.head = (conv.head + 1) % k;
    }
}

/// An EQ's linear phase running: its convolvers, the kernels the song was planned with, those
/// playing and those they had last (to crossfade from when new ones come).
#[derive(Default)]
pub struct Runner {
    state: Option<Box<State>>,
    planned: Option<Arc<Linear>>,
    current: Option<Arc<Linear>>,
    fading: Option<Arc<Linear>>,
}

impl Runner {
    /// The EQ didn't run in linear phase: next time starts clean.
    pub fn rest(&mut self) {
        self.planned = None;
        self.current = None;
        self.fading = None;
        if let Some(s) = self.state.as_mut() {
            for c in &mut s.convs {
                c.fdl
                    .iter_mut()
                    .flatten()
                    .for_each(|v| *v = Complex::new(0.0, 0.0));
                c.input.fill(0.0);
                c.out.fill(0.0);
            }
            s.fill = 0;
        }
    }

    /// Kernels to crossfade to; those they replace, to the remaking thread (if it has room).
    fn change(&mut self, to: Arc<Linear>) {
        let old = self.current.replace(to);
        if let Some(gone) = std::mem::replace(&mut self.fading, old) {
            let remake = gone.remake.clone();
            if let Ok(mut spent) = remake.spent.try_lock()
                && spent.len() < spent.capacity()
            {
                spent.push(gone);
            }
        }
    }

    /// A block through the kernels, in place: left and right, then (if there are) mid and side.
    /// `linear` the kernels the song was planned with; `eq` the EQ as it is now, moved by
    /// automation or modulation (new kernels are asked for, and crossfade in when made).
    /// Allocation-free (its memory taken from `linear`, made off the audio thread).
    pub fn process(&mut self, linear: &Arc<Linear>, eq: &Eq, l: &mut [f32], r: &mut [f32]) {
        let ms = linear.ms.is_some();
        let fits = |s: &State| s.k == linear.len / PART && s.convs.len() == if ms { 4 } else { 2 };
        if !self.state.as_deref().is_some_and(fits) {
            let Some(state) = linear.spare.try_lock().ok().and_then(|mut s| s.take()) else {
                return;
            };
            self.state = Some(state);
            self.planned = None;
            self.current = None;
            self.fading = None;
        }
        // Planned again: its kernels, unless they draw what's playing.
        if !self
            .planned
            .as_ref()
            .is_some_and(|p| Arc::ptr_eq(p, linear))
        {
            self.planned = Some(linear.clone());
            if !self
                .current
                .as_ref()
                .is_some_and(|c| same_curve(&c.eq, &linear.eq))
            {
                self.change(linear.clone());
            }
        }
        // Moving: new kernels asked for; those made, in.
        let remake = &linear.remake;
        if let Ok(mut ready) = remake.ready.try_lock()
            && let Some(made) = ready.take()
        {
            drop(ready);
            if made.len == linear.len && made.ms.is_some() == ms {
                self.change(made);
            }
        }
        if !self.current.as_ref().is_some_and(|c| same_curve(&c.eq, eq))
            && let Ok(mut want) = remake.want.try_lock()
        {
            *want = Some(*eq);
        }
        let state = self.state.as_mut().expect("taken above");
        let current = self.current.clone().expect("set above");
        for i in 0..l.len() {
            let f = state.fill;
            // Left and right in, what came out a partition ago out.
            let (a, b) = (l[i], r[i]);
            state.convs[0].input[PART + f] = a;
            state.convs[1].input[PART + f] = b;
            let (mut a, mut b) = (state.convs[0].out[f], state.convs[1].out[f]);
            if ms {
                let (m, s) = ((a + b) * 0.5, (a - b) * 0.5);
                state.convs[2].input[PART + f] = m;
                state.convs[3].input[PART + f] = s;
                let (m, s) = (state.convs[2].out[f], state.convs[3].out[f]);
                (a, b) = (m + s, m - s);
            }
            l[i] = a;
            r[i] = b;
            state.fill += 1;
            if state.fill == PART {
                state.fill = 0;
                let old = self.fading.take();
                // A crossfade only from kernels of the same make.
                let from = old
                    .as_deref()
                    .filter(|o| o.len == current.len && o.ms.is_some() == ms);
                for c in 0..2 {
                    state.partition(c, &current.lr[c], from.map(|o| &o.lr[c]));
                }
                if let Some(kernels) = &current.ms {
                    for c in 0..2 {
                        let from = from.and_then(|o| o.ms.as_ref()).map(|k| &k[c]);
                        state.partition(2 + c, &kernels[c], from);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Bands, EqBand};
    use std::f64::consts::PI;

    const RATE: f64 = 48_000.0;

    fn eq(bands: &[EqBand], resolution: u8) -> Eq {
        Eq {
            bands: Bands::from_slice(bands),
            resolution,
            ..Eq::default()
        }
    }

    /// An impulse through linear phase: what comes out.
    fn impulse(e: &Eq, n: usize) -> Vec<f32> {
        let lin = Arc::new(Linear::new(e, RATE));
        let mut run = Runner::default();
        let mut l = vec![0f32; n];
        l[0] = 1.0;
        let mut r = l.clone();
        for at in (0..n).step_by(512) {
            let end = (at + 512).min(n);
            run.process(&lin, e, &mut l[at..end], &mut r[at..end]);
        }
        l
    }

    #[test]
    fn linear_phase_is_the_curve_late_and_symmetric() {
        let bell = EqBand::new(Shape::Bell, 1_000.0, 6.0);
        let e = eq(&[bell], 1);
        let lat = latency(&e) as usize;
        assert_eq!(lat, 8192 / 2 + PART);
        let out = impulse(&e, lat * 2 + 2048);
        // Its peak where the latency says, and the same either side: linear phase.
        let peak = (0..out.len())
            .max_by(|&a, &b| out[a].abs().total_cmp(&out[b].abs()))
            .unwrap();
        assert_eq!(peak, lat);
        for d in [1, 10, 100, 1_000] {
            assert!((out[lat - d] - out[lat + d]).abs() < 1e-4, "{d}");
        }
        // And its magnitude the bands': the impulse's spectrum at 1 kHz up 6 dB, at 100 Hz flat.
        let at = |f: f64| {
            let (mut re, mut im) = (0.0, 0.0);
            for (i, &x) in out.iter().enumerate() {
                let w = 2.0 * PI * f * i as f64 / RATE;
                re += x as f64 * w.cos();
                im -= x as f64 * w.sin();
            }
            20.0 * (re * re + im * im).sqrt().log10()
        };
        assert!((at(1_000.0) - 6.0).abs() < 0.1, "{}", at(1_000.0));
        assert!(at(100.0).abs() < 0.1, "{}", at(100.0));
        assert!(at(10_000.0).abs() < 0.1, "{}", at(10_000.0));
    }

    #[test]
    fn mid_or_side_bands_take_a_second_stage() {
        let mut side = EqBand::new(Shape::Bell, 1_000.0, -12.0);
        side.placement = Placement::Side;
        let e = eq(&[side], 0);
        assert_eq!(latency(&e) as usize, 2 * (4096 / 2 + PART));
        // A mono sine comes through untouched (late); one all side, 12 dB down.
        let n = 24_000;
        for (sign, want) in [(1.0f32, 0.0), (-1.0, -12.0)] {
            let lin = Arc::new(Linear::new(&e, RATE));
            let mut run = Runner::default();
            let mut l: Vec<f32> = (0..n)
                .map(|i| (2.0 * PI * 1_000.0 * i as f64 / RATE).sin() as f32 * 0.5)
                .collect();
            let mut r: Vec<f32> = l.iter().map(|x| x * sign).collect();
            for at in (0..n).step_by(512) {
                let end = (at + 512).min(n);
                run.process(&lin, &e, &mut l[at..end], &mut r[at..end]);
            }
            let peak = l[n / 2..].iter().fold(0f32, |m, x| m.max(x.abs()));
            let db = 20.0 * (peak as f64 / 0.5).log10();
            assert!((db - want).abs() < 0.3, "{sign}: {db}");
        }
    }

    #[test]
    fn moving_it_remakes_the_kernels() {
        // Planned 6 dB up at 1 kHz; automation takes it back to flat as it plays: new kernels
        // come, made on their own thread, a few blocks on.
        let planned = eq(&[EqBand::new(Shape::Bell, 1_000.0, 6.0)], 0);
        let now = eq(&[EqBand::new(Shape::Bell, 1_000.0, 0.0)], 0);
        let lin = Arc::new(Linear::new(&planned, RATE));
        let mut run = Runner::default();
        let n = 512 * 80;
        let mut l: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * 1_000.0 * i as f64 / RATE).sin() as f32 * 0.5)
            .collect();
        let mut r = l.clone();
        for at in (0..n).step_by(512) {
            run.process(&lin, &now, &mut l[at..at + 512], &mut r[at..at + 512]);
            thread::sleep(Duration::from_millis(3));
        }
        let peak = l[n - 4096..].iter().fold(0f32, |m, x| m.max(x.abs()));
        let db = 20.0 * (peak as f64 / 0.5).log10();
        assert!(db.abs() < 0.2, "{db}");
    }
}
