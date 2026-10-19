//! Plays a project. Each track has a lane: its instrument and inserts, which keep their state.
//! What the lanes play (note events, audio clips, gains) is an arrangement, compiled from the
//! project on the main thread and swapped in whole after each edit. The engine mixes a block at
//! a time, live from the audio callback or offline for a bounce.
//!
//! Live, the main thread talks to the engine only through queues (`Msg` in, `Garbage` out) and
//! reads what it reports from `Shared`. Anything the engine lets go goes back as garbage, so the
//! audio thread never frees memory.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, Weak,
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::modulate::{
    self, MAX_MODS, ModInput, ModKind, ModRoute, ModState, ModTarget, Modulator, Offsets,
};
use crate::{
    drums::Drums,
    dsp::Strip,
    peaks::Peaks,
    plugin::{self, Control, ControlKind, Libraries, Note, Plugin, Processor},
    project::{
        AutoMode, AutoPoint, Comp, Content, Eq, Flex, Instrument, PanLaw, PanMode, Param, Place,
        PluginRef, Project, Region, Saturator, Which, chase, comp_segments, value_at,
    },
    sampler::Sampler,
    stretch::{self, Fit},
    synth::Synth,
};

/// The most frames one `process` call handles.
pub const MAX_BLOCK: usize = 512;
/// Most note events one lane takes in one block; more are dropped.
const BLOCK_NOTES: usize = 256;
/// Most controller moves an instrument hears a block.
const BLOCK_CONTROLS: usize = 256;
/// Most live or previewed notes the engine takes in one block.
const LIVE_NOTES: usize = 64;
/// The longest delay compensation for one lane, in samples.
const MAX_DELAY: usize = 1 << 14;
/// Lanes the live engine holds.
pub const SLOTS: usize = 256;
/// The master meter's index in `Shared::meters`.
pub const MASTER: usize = SLOTS;

/// An audio file in memory, one or two channels.
// ponytail: whole files in RAM; stream from disk when sessions outgrow memory.
pub struct AudioData {
    pub(crate) channels: Vec<Vec<f32>>,
}

impl AudioData {
    /// Audio from its channels (one or two), all as long.
    pub fn new(channels: Vec<Vec<f32>>) -> Self {
        AudioData { channels }
    }

    pub fn channels(&self) -> &[Vec<f32>] {
        &self.channels
    }

    #[cfg(test)]
    pub(crate) fn mono(samples: Vec<f32>) -> Self {
        AudioData {
            channels: vec![samples],
        }
    }
}

/// A stretched copy of a file: the file, and how it's fitted.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct Stretched {
    file: PathBuf,
    /// The fit's numbers, as bits.
    ratio: u64,
    semitones: u64,
    flex: Flex,
    formants: bool,
    warps: Vec<(usize, usize)>,
}

impl Stretched {
    fn new(file: &Path, fit: &Fit) -> Stretched {
        Stretched {
            file: file.to_path_buf(),
            ratio: fit.ratio.to_bits(),
            semitones: fit.semitones.to_bits(),
            flex: fit.flex,
            formants: fit.formants,
            warps: fit.warps.clone(),
        }
    }

    fn fit(&self) -> Fit {
        Fit {
            ratio: f64::from_bits(self.ratio),
            semitones: f64::from_bits(self.semitones),
            flex: self.flex,
            formants: self.formants,
            warps: self.warps.clone(),
        }
    }
}

/// The audio files a project uses, each loaded once, with its waveform overview.
pub struct AudioFiles {
    dir: PathBuf,
    rate: f64,
    files: HashMap<PathBuf, (Arc<AudioData>, Arc<Peaks>)>,
    /// Stretched copies; those asked for since `begin`, which are kept when the arrangement's
    /// made (with the other copies of the files they're of, a few, to stand in while new ones
    /// are made).
    stretched: HashMap<Stretched, Arc<AudioData>>,
    asked: HashSet<Stretched>,
    /// For a player that can't wait: copies made off its thread.
    renders: Option<Arc<Renders>>,
    /// Where each file's hits are (see `stretch::onsets`), found when first asked for.
    hits: Mutex<HashMap<PathBuf, Arc<Vec<usize>>>>,
}

/// Stretching off the main thread, two at a time, the newest wanted first: what the arrangement
/// wants that isn't made yet, what's being made, and what's made.
#[derive(Default)]
struct Renders {
    state: Mutex<RenderState>,
    work: Condvar,
    /// Something new is made: the arrangement can be made again with it.
    fresh: AtomicBool,
}

#[derive(Default)]
struct RenderState {
    wanted: Vec<(Stretched, Arc<AudioData>)>,
    busy: HashSet<Stretched>,
    made: Vec<(Stretched, Arc<AudioData>)>,
    workers: usize,
}

const RENDER_THREADS: usize = 2;

/// A render thread: makes what's wanted till nothing has been for a while, or the files go.
fn render_thread(renders: Weak<Renders>, rate: f64) {
    loop {
        let Some(r) = renders.upgrade() else {
            return;
        };
        let job = {
            let mut s = r.state.lock().unwrap();
            loop {
                if let Some(i) = s.wanted.iter().rposition(|(k, _)| !s.busy.contains(k)) {
                    let (key, source) = s.wanted.remove(i);
                    s.busy.insert(key.clone());
                    break Some((key, source));
                }
                let (next, waited) = r.work.wait_timeout(s, Duration::from_secs(5)).unwrap();
                s = next;
                if waited.timed_out() && s.wanted.is_empty() {
                    s.workers -= 1;
                    break None;
                }
            }
        };
        let Some((key, source)) = job else {
            return;
        };
        let data = Arc::new(stretch::render(&source, &key.fit(), rate));
        let mut s = r.state.lock().unwrap();
        s.busy.remove(&key);
        s.made.push((key, data));
        r.fresh.store(true, Ordering::Relaxed);
    }
}

/// `data` read faster or slower to `len`, roughly: what plays while a stretch is made.
fn stand_in(data: &AudioData, len: usize) -> AudioData {
    let channels = data
        .channels
        .iter()
        .map(|c| {
            let step = c.len() as f64 / len.max(1) as f64;
            (0..len)
                .map(|i| {
                    let x = i as f64 * step;
                    let at = x as usize;
                    let (a, b) = (c.get(at).copied(), c.get(at + 1).copied());
                    let t = (x - at as f64) as f32;
                    a.map_or(0.0, |a| a + (b.unwrap_or(a) - a) * t)
                })
                .collect()
        })
        .collect();
    AudioData { channels }
}

impl AudioFiles {
    pub fn new(dir: &Path, rate: f64) -> Self {
        AudioFiles {
            dir: dir.to_path_buf(),
            rate,
            files: HashMap::new(),
            stretched: HashMap::new(),
            asked: HashSet::new(),
            renders: None,
            hits: Mutex::default(),
        }
    }

    /// A loaded file's length, in frames at the engine's rate.
    pub fn frames(&self, file: &Path) -> Option<usize> {
        self.files.get(file).map(|(d, _)| d.channels[0].len())
    }

    /// Where a loaded file's hits are, in frames at the engine's rate.
    pub fn hits(&self, file: &Path) -> Option<Arc<Vec<usize>>> {
        let mut hits = self.hits.lock().unwrap();
        if let Some(h) = hits.get(file) {
            return Some(h.clone());
        }
        let (data, _) = self.files.get(file)?;
        let found = Arc::new(stretch::onsets(&data.channels, self.rate));
        hits.insert(file.to_path_buf(), found.clone());
        Some(found)
    }

    /// Stretches off the caller's thread: until a copy is made, a stand-in plays.
    pub fn in_background(mut self) -> Self {
        self.renders = Some(Arc::default());
        self
    }

    /// Stretches still being made, or wanted.
    pub fn rendering(&self) -> bool {
        self.renders.as_ref().is_some_and(|r| {
            let s = r.state.lock().unwrap();
            !s.wanted.is_empty() || !s.busy.is_empty()
        })
    }

    /// A stretch has been made since the last call: the arrangement can use it.
    pub fn fresh(&self) -> bool {
        self.renders
            .as_ref()
            .is_some_and(|r| r.fresh.swap(false, Ordering::Relaxed))
    }

    /// An arrangement's being made: what it asks for is what's wanted.
    fn begin(&mut self) {
        self.asked.clear();
        if let Some(r) = &self.renders {
            r.state.lock().unwrap().wanted.clear();
            let made = std::mem::take(&mut r.state.lock().unwrap().made);
            self.stretched.extend(made);
        }
    }

    /// It's made: copies of files it doesn't play go, and (past a few) those it doesn't ask for.
    fn end(&mut self) {
        let files: HashSet<&PathBuf> = self.asked.iter().map(|k| &k.file).collect();
        self.stretched
            .retain(|k, _| self.asked.contains(k) || files.contains(&k.file));
        if self.stretched.len() > 64 {
            self.stretched.retain(|k, _| self.asked.contains(k));
        }
    }

    /// A file fitted as `fit` says (see `crate::stretch`), made once for each.
    fn stretched(&mut self, file: &Path, mut fit: Fit) -> Result<Arc<AudioData>, String> {
        // Speed can't move pitch without moving time.
        if fit.flex == Flex::Speed {
            fit.semitones = 0.0;
        }
        if fit.ratio == 1.0 && fit.semitones == 0.0 && fit.warps.is_empty() {
            return self.get(file);
        }
        let key = Stretched::new(file, &fit);
        self.asked.insert(key.clone());
        if let Some(data) = self.stretched.get(&key) {
            return Ok(data.clone());
        }
        let source = self.get(file)?;
        let Some(r) = &self.renders else {
            let data = Arc::new(stretch::render(&source, &fit, self.rate));
            self.stretched.insert(key, data.clone());
            return Ok(data);
        };
        {
            let mut s = r.state.lock().unwrap();
            if !s.busy.contains(&key) && !s.wanted.iter().any(|w| w.0 == key) {
                s.wanted.push((key.clone(), source.clone()));
            }
            while s.workers < RENDER_THREADS {
                s.workers += 1;
                let (weak, rate) = (Arc::downgrade(r), self.rate);
                std::thread::spawn(move || render_thread(weak, rate));
            }
            r.work.notify_all();
        }
        // Meanwhile: the nearest copy made (else the file), read faster or slower to fit.
        let len = (source.channels[0].len() as f64 * fit.ratio).round() as usize;
        let alike = |k: &Stretched| {
            k.file == key.file
                && (k.semitones, k.flex, k.formants) == (key.semitones, key.flex, key.formants)
        };
        let nearest = self
            .stretched
            .iter()
            .filter(|(k, _)| alike(k))
            .min_by(|a, b| {
                let off = |k: &Stretched| (f64::from_bits(k.ratio) / fit.ratio).ln().abs();
                off(a.0).total_cmp(&off(b.0))
            })
            .map_or(source, |(_, d)| d.clone());
        Ok(Arc::new(stand_in(&nearest, len)))
    }

    fn get(&mut self, file: &Path) -> Result<Arc<AudioData>, String> {
        if let Some((data, _)) = self.files.get(file) {
            return Ok(data.clone());
        }
        let data = Arc::new(load_wav(&self.dir.join(file), self.rate)?);
        // ponytail: the overview shows the first channel.
        let peaks = Arc::new(Peaks::from_samples(&data.channels[0]));
        self.files.insert(file.to_path_buf(), (data.clone(), peaks));
        Ok(data)
    }

    /// The overview of a file the engine has loaded.
    pub fn peaks(&self, file: &Path) -> Option<Arc<Peaks>> {
        self.files.get(file).map(|(_, p)| p.clone())
    }
}

/// Loaded plugin bundles, by path.
pub type Modules = Libraries;

struct Clip {
    /// Timeline position and length, and where in the file it starts, all in samples.
    start: u64,
    len: u64,
    offset: u64,
    data: Arc<AudioData>,
    /// Its region's fades: where the region starts and how long it fades in; where it ends and
    /// how long it fades out (samples).
    fades: [u64; 4],
}

/// A clip's level at a sample, under its region's fades: an equal-power curve each end.
fn faded(c: &Clip, t: u64) -> f32 {
    let [from, fade_in, to, fade_out] = c.fades;
    let mut g = 1.0;
    if fade_in > 0 && t < from + fade_in {
        g *= ((t.saturating_sub(from) as f32 / fade_in as f32) * std::f32::consts::FRAC_PI_2).sin();
    }
    if fade_out > 0 && t + fade_out > to {
        g *= ((to.saturating_sub(t) as f32 / fade_out as f32) * std::f32::consts::FRAC_PI_2).sin();
    }
    g
}

#[derive(Clone, Copy)]
struct Event {
    at: u64,
    pitch: u8,
    /// 0 is a note off.
    velocity: f32,
}

/// A controller move, pitch bend, pressure or note expression, at a sample.
#[derive(Clone, Copy, Debug)]
struct ControlEvent {
    at: u64,
    kind: ControlKind,
}

/// What every lane plays, and how loud.
pub struct Arrangement {
    tempo: f64,
    master: f32,
    plans: Vec<Plan>,
    auxes: Vec<AuxPlan>,
    /// What tracks send to each aux, this block.
    buses: Vec<[Vec<f32>; 2]>,
    /// How far ahead of what's heard the engine reads, so plugins' latency doesn't make it late.
    lookahead: u64,
    /// The master's built-in EQ and compressor.
    master_strip: StripDsp,
    /// Where playback loops, in samples, while the cycle is on.
    cycle: Option<(u64, u64)>,
    /// The lane holding the master's plugins, and the track each of them hears as a sidechain.
    master_lane: Option<usize>,
    master_sidechains: Vec<Option<usize>>,
    /// The order tracks are worked out in: a sidechain's source before what hears it.
    order: Vec<usize>,
    /// Each track's output this block, kept for the plugins that hear it as a sidechain.
    sidechains: Vec<[Vec<f32>; 2]>,
    /// A beat, in quarter notes, and beats to a bar: for the metronome.
    beat: f64,
    beats_per_bar: u64,
}

impl Arrangement {
    fn empty() -> Self {
        Arrangement {
            tempo: 120.0,
            master: 1.0,
            plans: Vec::new(),
            auxes: Vec::new(),
            buses: Vec::new(),
            lookahead: 0,
            master_strip: StripDsp::default(),
            cycle: None,
            master_lane: None,
            master_sidechains: Vec::new(),
            order: Vec::new(),
            sidechains: Vec::new(),
            beat: 1.0,
            beats_per_bar: 4,
        }
    }
}

/// A channel's built-in EQ, compressor and saturator, unless absent or bypassed.
fn strip_dsp(eq: Option<&Eq>, comp: Option<&Comp>, sat: Option<&Saturator>) -> StripDsp {
    StripDsp {
        eq: eq.filter(|e| !e.bypass).copied(),
        comp: comp.filter(|c| !c.bypass).copied(),
        sat: sat.filter(|s| !s.bypass).copied(),
    }
}

/// What a channel's built-in EQ, compressor and saturator are set to; the lane's `Strip` (or the
/// engine's, for the master) runs them.
#[derive(Default)]
struct StripDsp {
    eq: Option<Eq>,
    comp: Option<Comp>,
    sat: Option<Saturator>,
}

impl Strip {
    /// Runs a block through the strip as `dsp` has it; returns the compressor's gain reduction,
    /// in dB.
    fn run(&mut self, dsp: &StripDsp, l: &mut [f32], r: &mut [f32]) -> f32 {
        self.process(dsp.eq.as_ref(), dsp.comp.as_ref(), dsp.sat.as_ref(), l, r)
    }
}

/// What a Live Loops cell plays, from 0, over `length` samples.
struct CellPlan {
    events: Vec<Event>,
    controls: Vec<ControlEvent>,
    clips: Vec<Clip>,
    length: u64,
}

/// The notes an instrument is holding, counted by pitch: the same pitch can be held twice (two
/// notes overlapping), and each needs its own note-off, or one voice rings on.
#[derive(Clone, Copy)]
struct Held([u8; 128]);

impl Held {
    fn new() -> Self {
        Held([0; 128])
    }

    /// Whether any note is held.
    fn any(&self) -> bool {
        self.0.iter().any(|&c| c > 0)
    }

    fn apply(&mut self, pitch: u8, on: bool) {
        let count = &mut self.0[pitch as usize];
        *count = if on {
            count.saturating_add(1)
        } else {
            count.saturating_sub(1)
        };
    }

    /// Lets go of every note held, at `offset` in the block, unless `keep` says it's still on;
    /// adds the note-offs to `notes`.
    fn release(
        &mut self,
        offset: u32,
        keep: impl Fn(u8) -> bool,
        notes: &mut [Note; BLOCK_NOTES],
        n: &mut usize,
    ) {
        for (pitch, count) in self.0.iter_mut().enumerate() {
            if *count == 0 || keep(pitch as u8) {
                continue;
            }
            for _ in 0..std::mem::take(count) {
                if *n < BLOCK_NOTES {
                    notes[*n] = Note {
                        offset,
                        pitch: pitch as u8,
                        velocity: 0.0,
                    };
                    *n += 1;
                }
            }
        }
    }
}

/// Whether a pitch is sounding at a sample in some events (sorted by time): its last event before
/// then turned it on. Looks back so far, then says no.
fn sounding(events: &[Event], at: u64, pitch: u8) -> bool {
    const LOOK_BACK: usize = 4096;
    let end = events.partition_point(|e| e.at < at);
    events[end.saturating_sub(LOOK_BACK)..end]
        .iter()
        .rev()
        .find(|e| e.pitch == pitch)
        .is_some_and(|e| e.velocity > 0.0)
}

/// Adds what `events` and `clips` play from `from` to `to` (samples) into a block, `into`
/// samples into it.
#[allow(clippy::too_many_arguments)]
/// Where a block's notes and controller moves are gathered.
struct Gathered {
    notes: [Note; BLOCK_NOTES],
    n: usize,
    controls: [Control; BLOCK_CONTROLS],
    m: usize,
}

impl Gathered {
    fn new() -> Self {
        Gathered {
            notes: [Note::default(); BLOCK_NOTES],
            n: 0,
            controls: [Control {
                offset: 0,
                kind: ControlKind::Aftertouch(0.0),
            }; BLOCK_CONTROLS],
            m: 0,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn gather(
    events: &[Event],
    controls: &[ControlEvent],
    clips: &[Clip],
    (from, to): (u64, u64),
    into: usize,
    g: &mut Gathered,
    held: &mut Held,
    [l, r]: [&mut [f32]; 2],
) {
    let first = controls.partition_point(|c| c.at < from);
    for c in controls[first..].iter().take_while(|c| c.at < to) {
        if g.m == BLOCK_CONTROLS {
            break;
        }
        g.controls[g.m] = Control {
            offset: (c.at - from) as u32 + into as u32,
            kind: c.kind,
        };
        g.m += 1;
    }
    let (notes, n) = (&mut g.notes, &mut g.n);
    let first = events.partition_point(|e| e.at < from);
    for e in events[first..].iter().take_while(|e| e.at < to) {
        if *n == BLOCK_NOTES {
            break;
        }
        held.apply(e.pitch, e.velocity > 0.0);
        notes[*n] = Note {
            offset: (e.at - from) as u32 + into as u32,
            pitch: e.pitch,
            velocity: e.velocity,
        };
        *n += 1;
    }
    // ponytail: walks the clips from the first every block; index them by time if a track gets
    // thousands of regions.
    for c in clips.iter().take_while(|c| c.start < to) {
        let (a, b) = (from.max(c.start), to.min(c.start + c.len));
        for (ch, dst) in [&mut *l, &mut *r].into_iter().enumerate() {
            let src = &c.data.channels[ch.min(c.data.channels.len() - 1)];
            let fades = c.fades[1] > 0 || c.fades[3] > 0;
            for t in a..b {
                let i = (c.offset + t - c.start) as usize;
                let v = src.get(i).copied().unwrap_or(0.0);
                dst[(t - from) as usize + into] += if fades { v * faded(c, t) } else { v };
            }
        }
    }
}

/// An aux: it plays its bus through its inserts into the master.
struct AuxPlan {
    slot: usize,
    gain: [f32; 2],
    dsp: StripDsp,
    /// Delay compensation after its inserts.
    delay: usize,
    /// The track each insert hears as a sidechain, if any.
    sidechains: Vec<Option<usize>>,
}

/// The sidechain an insert hears: its source track's output this block.
fn sidechain<'a>(
    buffers: &'a [[Vec<f32>; 2]],
    source: Option<&Option<usize>>,
    frames: usize,
) -> Option<[&'a [f32]; 2]> {
    let [l, r] = buffers.get((*source?)?)?;
    Some([&l[..frames], &r[..frames]])
}

/// Plugin delay compensation: delays that line every track and aux up at the master, and how
/// far ahead the engine reads so that what's heard is on time.
#[derive(Debug, PartialEq)]
struct Compensation {
    lookahead: u64,
    /// Per track, before its sends.
    tracks: Vec<usize>,
    /// On every track's direct path, to wait for the slowest aux.
    direct: usize,
    /// Per aux, after its inserts.
    auxes: Vec<usize>,
}

fn compensate(tracks: &[u32], auxes: &[u32]) -> Compensation {
    let slowest = |l: &[u32]| {
        l.iter()
            .copied()
            .max()
            .unwrap_or(0)
            .min(MAX_DELAY as u32 - 1)
    };
    let (t, a) = (slowest(tracks), slowest(auxes));
    let behind = |max: u32| move |&l: &u32| (max - l.min(max)) as usize;
    Compensation {
        lookahead: (t + a) as u64,
        tracks: tracks.iter().map(behind(t)).collect(),
        direct: a as usize,
        auxes: auxes.iter().map(behind(a)).collect(),
    }
}

/// A stereo delay line, for delay compensation. It keeps its history while the delay changes.
struct Delay {
    buf: [Vec<f32>; 2],
    pos: usize,
}

impl Delay {
    fn new() -> Self {
        Delay {
            buf: [vec![0.0; MAX_DELAY], vec![0.0; MAX_DELAY]],
            pos: 0,
        }
    }

    /// Delays `l` and `r` in place by `d` samples.
    fn process(&mut self, l: &mut [f32], r: &mut [f32], d: usize) {
        let d = d.min(MAX_DELAY - 1);
        for i in 0..l.len() {
            let w = self.pos;
            let rd = (w + MAX_DELAY - d) % MAX_DELAY;
            self.buf[0][w] = l[i];
            self.buf[1][w] = r[i];
            l[i] = self.buf[0][rd];
            r[i] = self.buf[1][rd];
            self.pos = (w + 1) % MAX_DELAY;
        }
    }
}

/// Left and right gain for a level and a balance pan.
fn balance(g: f32, pan: f32) -> [f32; 2] {
    let pan = pan.clamp(-1.0, 1.0);
    [g * (1.0 - pan).min(1.0), g * (1.0 + pan).min(1.0)]
}

/// Left and right gain for a sound at `pan`, by the pan law.
fn law_gains(law: PanLaw, pan: f32) -> [f32; 2] {
    let pan = pan.clamp(-1.0, 1.0);
    let theta = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
    match law {
        PanLaw::Flat => balance(1.0, pan),
        PanLaw::Minus3 => [theta.cos(), theta.sin()],
        PanLaw::Minus3Compensated => [
            theta.cos() * std::f32::consts::SQRT_2,
            theta.sin() * std::f32::consts::SQRT_2,
        ],
        PanLaw::Minus6 => [(1.0 - pan) / 2.0, (1.0 + pan) / 2.0],
    }
}

/// How a lane's left and right become the mix's: out left = in left × m[0] + in right × m[1],
/// out right = in left × m[2] + in right × m[3]; at level `g`, panned by its mode.
pub fn pan_matrix(mode: PanMode, law: PanLaw, pan: f32, spread: f32, g: f32) -> [f32; 4] {
    match mode {
        PanMode::Balance => {
            let [a, b] = law_gains(law, pan);
            [g * a, 0.0, 0.0, g * b]
        }
        // Each side placed by equal power: spread 1 leaves them where they were.
        PanMode::Stereo => {
            let s = spread.clamp(-1.0, 1.0);
            let place = |x: f32| {
                let theta = (x.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
                (theta.cos(), theta.sin())
            };
            let (ll, lr) = place(pan - s);
            let (rl, rr) = place(pan + s);
            [g * ll, g * rl, g * lr, g * rr]
        }
        // Level here; where around the head, after.
        PanMode::Binaural => [g, 0.0, 0.0, g],
    }
}

/// A binaural panner's memory: the sound a moment back (for the far ear's delay), and each
/// ear's filters.
pub struct Binaural {
    line: [f32; 256],
    at: usize,
    /// The far ear's head shadow; each ear's dulling (behind, below, far off); each ear's
    /// presence, lifted above.
    shadow: f32,
    dull: [f32; 2],
    air: [f32; 2],
}

impl Default for Binaural {
    fn default() -> Self {
        Binaural {
            line: [0.0; 256],
            at: 0,
            shadow: 0.0,
            dull: [0.0; 2],
            air: [0.0; 2],
        }
    }
}

impl Binaural {
    /// Places a block round the head, as a head hears it: the far ear later (up to 0.66 ms, by
    /// Woodworth's formula), quieter and darker in the head's shadow; behind and below duller
    /// (the ears face forward, the body's below); above brighter; further off quieter and
    /// duller (the air).
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], place: Place, rate: f32) {
        let [x, y, z] = place.xyz();
        let d = place.distance;
        let side = (x / d).clamp(-1.0, 1.0);
        let around = side.abs();
        let (back, up) = ((-z / d).max(0.0), (y / d).clamp(-1.0, 1.0));
        let delay = (0.000_255 * (around.asin() + around) * rate).min(250.0);
        let pole = |hz: f32| (-std::f32::consts::TAU * hz.min(rate * 0.45) / rate).exp();
        let shadow = pole(20_000.0 - 17_500.0 * around);
        let dulled = (0.55 * back + 0.6 * (-up).max(0.0) + 0.3 * (d - 1.0).max(0.0)).min(0.85);
        let dull = pole(20_000.0 * (1.0 - dulled));
        let air = pole(3_000.0);
        let lift = 0.5 * up.max(0.0);
        let gain = 1.0 / (0.5 + 0.5 * d) * (1.0 - 0.12 * back);
        let (far_gain, near_gain) = (1.0 - 0.45 * around, 1.0 + 0.2 * around);
        for i in 0..l.len().min(r.len()) {
            let mono = 0.5 * (l[i] + r[i]);
            self.line[self.at] = mono;
            let behind = self.at as f32 - delay;
            let k = behind.floor();
            let t = behind - k;
            let idx = |n: f32| (n.rem_euclid(256.0)) as usize;
            let delayed = self.line[idx(k)] * (1.0 - t) + self.line[idx(k + 1.0)] * t;
            self.at = (self.at + 1) % 256;
            self.shadow = delayed + (self.shadow - delayed) * shadow;
            let (near, far) = (mono * near_gain, self.shadow * far_gain);
            let mut ears = if side < 0.0 { [near, far] } else { [far, near] };
            for (e, v) in ears.iter_mut().enumerate() {
                self.dull[e] = *v + (self.dull[e] - *v) * dull;
                let s = self.dull[e];
                self.air[e] = s + (self.air[e] - s) * air;
                *v = (s + (s - self.air[e]) * lift) * gain;
            }
            (l[i], r[i]) = (ears[0], ears[1]);
        }
    }
}

/// What one lane plays.
struct Plan {
    slot: usize,
    /// Sorted by time, offs before ons at the same sample.
    events: Vec<Event>,
    /// Sorted by start.
    clips: Vec<Clip>,
    /// 0 when muted or another track is soloed.
    audible: f32,
    volume_db: f32,
    pan: f32,
    pan_mode: PanMode,
    pan_law: PanLaw,
    spread: f32,
    /// Binaural: its height and distance (the way round follows the pan).
    place: Place,
    /// Its modulators (sends by their place in `sends`), the lane each follower hears, and the
    /// set values of the plugin parameters they move.
    mods: Vec<Modulator>,
    follow: [usize; MAX_MODS],
    param_base: Vec<(Which, u32, f32)>,
    /// Automation, with ticks turned to samples; empty follows the fader.
    volume: Vec<AutoPoint>,
    pans: Vec<AutoPoint>,
    /// 1 where automation mutes it.
    mutes: Vec<AutoPoint>,
    /// Each send's level automation (dB), as `sends`; empty follows its knob.
    send_levels: Vec<Vec<AutoPoint>>,
    /// Plugin parameters' automation, in samples, 0 to 1.
    params: Vec<(Which, u32, Vec<AutoPoint>)>,
    /// Aux index, level, and whether it's taken before the fader.
    sends: Vec<(usize, f32, bool)>,
    /// Controller moves, sorted by time.
    controls: Vec<ControlEvent>,
    /// Live Loops cells, in the track's order.
    cells: Vec<CellPlan>,
    /// The track each insert hears as a sidechain, if any; and whether any plugin hears this
    /// one.
    sidechains: Vec<Option<usize>>,
    feeds: bool,
    /// The aux it plays into instead of the master.
    output: Option<usize>,
    /// The sampler's settings, which it follows as they change.
    sampler: Option<Arc<crate::project::Sampler>>,
    /// Delay compensation before the sends, and on the direct path after them.
    delay: usize,
    direct: usize,
    dsp: StripDsp,
}

impl Plan {
    /// Moves a plugin's automated parameters across a block, `from` to `to` (samples read), and
    /// its modulated ones about where they're automated or set.
    fn automate(&self, which: Which, p: &mut Processor, (from, to): (u64, u64), o: &Offsets) {
        for (_, id, points) in self.params.iter().filter(|a| a.0 == which) {
            let off = o.plugin(which, *id).unwrap_or(0.0);
            let at =
                |s: u64| (value_at(points, s as f64).unwrap_or(0.0) + off).clamp(0.0, 1.0) as f64;
            p.automate(*id, at(from), at(to.saturating_sub(1)));
        }
        for &(w, id, off) in &o.plugins[..o.plugin_count] {
            if w != which || self.params.iter().any(|a| a.0 == w && a.1 == id) {
                continue;
            }
            let base = self
                .param_base
                .iter()
                .find(|b| b.0 == w && b.1 == id)
                .map_or(0.5, |b| b.2);
            let v = (base + off).clamp(0.0, 1.0) as f64;
            p.automate(id, v, v);
        }
    }

    /// The pan at a sample, as automated or set.
    fn pan_at(&self, at: u64) -> f32 {
        value_at(&self.pans, at as f64).unwrap_or(self.pan)
    }

    /// 1 when it sounds, 0 when muted, soloed out or muted by automation.
    fn audible_at(&self, at: u64) -> f32 {
        self.audible
            * (1.0
                - value_at(&self.mutes, at as f64)
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0))
    }

    /// A send's level (as gain) at a sample, as automated or set.
    fn send_at(&self, k: usize, at: u64) -> f32 {
        match self.send_levels.get(k).and_then(|p| value_at(p, at as f64)) {
            Some(level) => db(level),
            None => self.sends[k].1,
        }
    }

    /// How left and right become the mix's at a sample: the fader, and the pan by its mode;
    /// each moved by its modulators (`by`: dB, pan).
    fn matrix(&self, at: u64, by: (f32, f32)) -> [f32; 4] {
        let volume = value_at(&self.volume, at as f64).unwrap_or(self.volume_db);
        let volume = modulate::apply_volume(volume, by.0);
        pan_matrix(
            self.pan_mode,
            self.pan_law,
            modulate::apply_pan(self.pan_at(at), by.1),
            self.spread,
            self.audible_at(at) * db(volume),
        )
    }
}

/// A note from a MIDI keyboard. Velocity 0 is a note off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiveNote {
    pub pitch: u8,
    pub velocity: u8,
}

/// A live note as the engine played it, at the playhead, for recording.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Played {
    pub at: u64,
    pub pitch: u8,
    pub velocity: u8,
}

#[allow(clippy::large_enum_variant)] // one per track
enum Source {
    Audio,
    Synth(Synth),
    Drums(Box<Drums>),
    Sampler(Box<Sampler>),
    Plugin(Processor),
}

/// A track's instrument and inserts: what makes its sound.
pub struct Lane {
    source: Source,
    inserts: Vec<Processor>,
    /// Notes the instrument is holding, to release on stop or locate: those the song played, and
    /// those played live (a MIDI keyboard, a key clicked in the piano roll).
    held: Held,
    live: Held,
    /// Delay compensation: before a track's sends, and after them (or after an aux's inserts).
    pre: Delay,
    post: Delay,
    /// The Live Loops cell playing instead of the track, and the sample its loop began; and a
    /// change waiting for its sample.
    cell: Option<(usize, u64)>,
    queued: Option<(Option<usize>, u64)>,
    strip: Box<Strip>,
    binaural: Box<Binaural>,
    /// Its modulators' memories, and what they moved its volume and pan by last block (to ramp
    /// from).
    mods: [ModState; MAX_MODS],
    mod_last: (f32, f32),
}

/// One plugin of a project (whose files are in `dir`) loaded on its own, as the project sets
/// it: its saved settings, its preset, its parameters.
pub fn load_plugin(r: &PluginRef, dir: &Path, rate: f64) -> Result<(Plugin, Processor), String> {
    let state = r
        .state
        .as_ref()
        .and_then(|f| std::fs::read(dir.join(f)).ok());
    let (plugin, mut processor) =
        plugin::instantiate(&mut Libraries::new(), r, rate, MAX_BLOCK, state.as_deref())?;
    if let (Some(preset), None) = (&r.preset, &r.state) {
        plugin.load_preset(preset)?;
    }
    for (&id, &v) in &r.params {
        plugin.set_param(id, v);
    }
    plugin::settle(&mut processor, MAX_BLOCK);
    Ok((plugin, processor))
}

impl Lane {
    /// Lets its plugins apply what's been set on them, before they're heard.
    pub fn settle(&mut self) {
        if let Source::Plugin(p) = &mut self.source {
            plugin::settle(p, MAX_BLOCK);
        }
        for p in &mut self.inserts {
            plugin::settle(p, MAX_BLOCK);
        }
    }
}

/// The main-thread halves of a lane's plugins.
#[derive(Default)]
pub struct LanePlugins {
    pub instrument: Option<Plugin>,
    pub inserts: Vec<Plugin>,
}

fn db(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Builds a lane (a track's instrument and inserts, or an aux's inserts) at `rate`, with each
/// plugin's saved settings from `dir`. Also returns the main-thread halves of its plugins, which
/// must live as long as it, and its latency in samples.
pub fn make_lane(
    instrument: Option<&Instrument>,
    inserts: &[PluginRef],
    rate: f64,
    modules: &mut Modules,
    dir: &Path,
) -> Result<(Lane, LanePlugins, u32), String> {
    let mut instantiate = |r: &PluginRef| -> Result<(Plugin, Processor), String> {
        // A missing settings file just means default settings.
        let state = r
            .state
            .as_ref()
            .and_then(|f| std::fs::read(dir.join(f)).ok());
        plugin::instantiate(modules, r, rate, MAX_BLOCK, state.as_deref())
    };
    let mut plugins = LanePlugins::default();
    let source = match instrument {
        None => Source::Audio,
        Some(Instrument::Synth) => Source::Synth(Synth::new(rate)),
        Some(Instrument::Drums) => Source::Drums(Box::new(Drums::new(rate))),
        Some(Instrument::Sampler(s)) => Source::Sampler(Box::new(Sampler::load(s, dir, rate)?)),
        Some(Instrument::Plugin(r)) => {
            let (plugin, processor) = instantiate(r)?;
            plugins.instrument = Some(plugin);
            Source::Plugin(processor)
        }
    };
    let mut processors = Vec::new();
    for r in inserts {
        let (plugin, processor) = instantiate(r)?;
        plugins.inserts.push(plugin);
        processors.push(processor);
    }
    let latency = match &source {
        Source::Plugin(p) => p.latency(),
        _ => 0,
    } + processors.iter().map(|p| p.latency()).sum::<u32>();
    let lane = Lane {
        source,
        inserts: processors,
        held: Held::new(),
        live: Held::new(),
        pre: Delay::new(),
        post: Delay::new(),
        cell: None,
        queued: None,
        strip: Box::new(Strip::new(rate)),
        binaural: Box::default(),
        mods: [ModState::default(); MAX_MODS],
        mod_last: (0.0, 0.0),
    };
    Ok((lane, plugins, latency))
}

/// Compiles what `project` plays at `rate`. Track `i` is on lane `tracks[i].0`, whose plugins
/// make it `tracks[i].1` samples late; likewise auxes; the master's plugins are on lane `master`.
/// Regions whose audio can't be read are left out, and the reasons returned, so one missing file
/// doesn't stop the rest playing.
pub fn compile(
    project: &Project,
    rate: f64,
    tracks: &[(usize, u32)],
    auxes: &[(usize, u32)],
    master: Option<usize>,
    files: &mut AudioFiles,
) -> (Arrangement, Vec<String>) {
    files.begin();
    let mut problems = Vec::new();
    let latencies = |l: &[(usize, u32)]| l.iter().map(|&(_, l)| l).collect::<Vec<_>>();
    let pdc = compensate(&latencies(tracks), &latencies(auxes));
    let any_solo = project.tracks.iter().any(|t| t.solo);
    let mut plans = Vec::new();
    // Which track (by its place) each plugin hears as a sidechain, by the source's id.
    let source = |r: &PluginRef| {
        let id = r.sidechain?;
        project.tracks.iter().position(|t| t.id == id)
    };
    let mut feeds = vec![false; project.tracks.len()];
    let mut sources = |inserts: &[PluginRef]| -> Vec<Option<usize>> {
        let found: Vec<_> = inserts.iter().map(source).collect();
        for &s in found.iter().flatten() {
            feeds[s] = true;
        }
        found
    };
    let track_sidechains: Vec<_> = project.tracks.iter().map(|t| sources(&t.inserts)).collect();
    let aux_sidechains: Vec<_> = project.auxes.iter().map(|a| sources(&a.inserts)).collect();
    let master_sidechains = sources(&project.master_inserts);
    for (i, (t, &(slot, _))) in project.tracks.iter().zip(tracks).enumerate() {
        let (mut clips, mut events, mut controls) = (Vec::new(), Vec::new(), Vec::new());
        for (r, region) in t.regions.iter().enumerate().filter(|(_, r)| !r.mute) {
            // A looped region plays again from each multiple of its length, the last time cut
            // short where the loop stops.
            let until = match region.looped {
                true => project.loop_end((i, r)),
                false => region.start + region.length,
            };
            let mut start = region.start;
            while start < until {
                // ponytail: a copy of the region per pass; fine for loops, not for thousands.
                let pass = Region {
                    start,
                    length: region.length.min(until - start),
                    ..region.clone()
                };
                play_region(
                    project,
                    rate,
                    &pass,
                    &t.modulators,
                    files,
                    (&mut events, &mut controls),
                    &mut clips,
                    &mut problems,
                );
                start += region.length.max(1);
            }
        }
        // A loop cell plays its region from 0, every note ending by the loop's end.
        let cells = t
            .cells
            .iter()
            .map(|cell| {
                let region = Region {
                    start: 0,
                    ..cell.region.clone()
                };
                let (mut events, mut clips, mut controls) = (Vec::new(), Vec::new(), Vec::new());
                play_region(
                    project,
                    rate,
                    &region,
                    &t.modulators,
                    files,
                    (&mut events, &mut controls),
                    &mut clips,
                    &mut problems,
                );
                let length = project.samples(region.length, rate).max(1);
                for e in &mut events {
                    e.at = e.at.min(length - 1);
                }
                events.sort_by_key(|e| (e.at, e.velocity > 0.0));
                controls.retain(|c| c.at < length);
                controls.sort_by_key(|c| c.at);
                CellPlan {
                    events,
                    controls,
                    clips,
                    length,
                }
            })
            .collect();
        // Offs first at the same sample, so back-to-back notes of one pitch both sound.
        events.sort_by_key(|e| (e.at, e.velocity > 0.0));
        controls.sort_by_key(|c| c.at);
        clips.sort_by_key(|c| c.start);
        let in_samples = |points: &[AutoPoint]| {
            let at = |p: &AutoPoint| project.samples(p.tick, rate);
            points
                .iter()
                .map(|p| AutoPoint { tick: at(p), ..*p })
                .collect()
        };
        // With its automation off, a track follows its faders and knobs.
        let auto = (t.automation.mode != AutoMode::Off).then_some(&t.automation);
        plans.push(Plan {
            slot,
            events,
            controls,
            sidechains: track_sidechains[i].clone(),
            feeds: false,
            clips,
            audible: if t.mute || (any_solo && !t.solo) {
                0.0
            } else {
                1.0
            },
            volume_db: t.volume_db,
            pan: t.pan,
            pan_mode: t.pan_mode,
            pan_law: project.pan_law,
            spread: t.spread.unwrap_or(1.0),
            place: Place::of(t, t.pan),
            mods: t
                .modulators
                .iter()
                .take(MAX_MODS)
                .map(|m| Modulator {
                    kind: m.kind.clone(),
                    // A send by its place among those that play.
                    routes: m
                        .routes
                        .iter()
                        .filter_map(|r| match r.target {
                            ModTarget::Send(j) => {
                                let playing = |s: &&crate::project::AuxSend| {
                                    project.auxes.iter().any(|a| a.bus == s.bus)
                                };
                                let k = t.sends.iter().take(j).filter(playing).count();
                                t.sends.get(j).filter(playing).map(|_| ModRoute {
                                    target: ModTarget::Send(k),
                                    ..*r
                                })
                            }
                            _ => Some(*r),
                        })
                        .collect(),
                })
                .collect(),
            follow: std::array::from_fn(|k| match t.modulators.get(k).map(|m| &m.kind) {
                Some(ModKind::Follower { source, .. }) => project
                    .tracks
                    .iter()
                    .position(|o| o.id == *source)
                    .map_or(usize::MAX, |j| tracks[j].0),
                _ => usize::MAX,
            }),
            param_base: {
                let refs = match &t.instrument {
                    Some(Instrument::Plugin(r)) => vec![(Which::Instrument, r)],
                    _ => vec![],
                };
                let inserts = t
                    .inserts
                    .iter()
                    .enumerate()
                    .map(|(i, r)| (Which::Insert(i), r));
                refs.into_iter()
                    .chain(inserts)
                    .flat_map(|(w, r)| r.params.iter().map(move |(&id, &v)| (w, id, v as f32)))
                    .collect()
            },
            volume: auto.map_or_else(Vec::new, |a| in_samples(&a.volume)),
            pans: auto.map_or_else(Vec::new, |a| in_samples(&a.pan)),
            mutes: auto.map_or_else(Vec::new, |a| in_samples(&a.mute)),
            send_levels: t
                .sends
                .iter()
                .filter(|s| project.auxes.iter().any(|a| a.bus == s.bus))
                .map(|s| auto.map_or_else(Vec::new, |a| in_samples(a.points(Param::Send(s.bus)))))
                .collect(),
            params: auto
                .map(|a| &a.plugins[..])
                .unwrap_or_default()
                .iter()
                .filter(|a| !a.points.is_empty())
                .map(|a| (a.plugin, a.param, in_samples(&a.points)))
                .collect(),
            cells,
            sends: t
                .sends
                .iter()
                .filter_map(|s| {
                    let aux = project.auxes.iter().position(|a| a.bus == s.bus)?;
                    Some((aux, db(s.level_db), s.pre))
                })
                .collect(),
            delay: pdc.tracks[i],
            output: t
                .output
                .and_then(|bus| project.auxes.iter().position(|a| a.bus == bus)),
            // Through an aux, the aux's own compensation lines it up.
            direct: if t.output.is_some() { 0 } else { pdc.direct },
            dsp: strip_dsp(t.eq.as_ref(), t.comp.as_ref(), t.sat.as_ref()),
            sampler: match &t.instrument {
                Some(Instrument::Sampler(s)) => Some(Arc::new(s.clone())),
                _ => None,
            },
        });
    }
    let auxes = project
        .auxes
        .iter()
        .zip(auxes)
        .zip(&pdc.auxes)
        .zip(aux_sidechains)
        .map(|(((a, &(slot, _)), &delay), sidechains)| {
            let g = if a.mute { 0.0 } else { db(a.volume_db) };
            AuxPlan {
                slot,
                gain: balance(g, a.pan),
                dsp: strip_dsp(a.eq.as_ref(), a.comp.as_ref(), a.sat.as_ref()),
                delay,
                sidechains,
            }
        })
        .collect::<Vec<_>>();
    for (plan, &f) in plans.iter_mut().zip(&feeds) {
        plan.feeds = f;
    }
    let order = sidechain_order(&track_sidechains);
    let arrangement = Arrangement {
        tempo: project.tempo,
        master: db(project.master_volume_db),
        sidechains: feeds
            .iter()
            .map(|&f| match f {
                true => [vec![0.0; MAX_BLOCK], vec![0.0; MAX_BLOCK]],
                false => [Vec::new(), Vec::new()],
            })
            .collect(),
        master_sidechains,
        order,
        plans,
        buses: (0..auxes.len())
            .map(|_| [vec![0.0; MAX_BLOCK], vec![0.0; MAX_BLOCK]])
            .collect(),
        auxes,
        lookahead: pdc.lookahead,
        master_strip: strip_dsp(
            project.master_eq.as_ref(),
            project.master_comp.as_ref(),
            project.master_sat.as_ref(),
        ),
        cycle: project
            .cycle
            .filter(|c| !c.off && c.end > c.start)
            .map(|c| (project.samples(c.start, rate), project.samples(c.end, rate))),
        master_lane: master,
        beat: project.beat() as f64 / crate::project::PPQ as f64,
        beats_per_bar: project.beats_per_bar(),
    };
    files.end();
    (arrangement, problems)
}

/// The order to work tracks out in, given the track each one's inserts hear as a sidechain:
/// every source before what hears it. Sources in a loop keep their places, a block late.
fn sidechain_order(hears: &[Vec<Option<usize>>]) -> Vec<usize> {
    let mut order = Vec::with_capacity(hears.len());
    let mut state = vec![0u8; hears.len()]; // 0 unseen, 1 on the way, 2 placed
    fn visit(t: usize, hears: &[Vec<Option<usize>>], state: &mut [u8], order: &mut Vec<usize>) {
        if state[t] != 0 {
            return;
        }
        state[t] = 1;
        for &s in hears[t].iter().flatten() {
            if s != t {
                visit(s, hears, state, order);
            }
        }
        state[t] = 2;
        order.push(t);
    }
    for t in 0..hears.len() {
        visit(t, hears, &mut state, &mut order);
    }
    order
}

/// Adds the note events, controller moves and audio clips a region plays, in samples; a pattern's
/// steps as the track's modulators move its sounds.
#[allow(clippy::too_many_arguments)]
fn play_region(
    project: &Project,
    rate: f64,
    region: &Region,
    mods: &[Modulator],
    files: &mut AudioFiles,
    (events, controls): (&mut Vec<Event>, &mut Vec<ControlEvent>),
    clips: &mut Vec<Clip>,
    problems: &mut Vec<String>,
) {
    // Plays `content`, which begins `at` ticks into the region, from `from` to `to` (ticks
    // into the region).
    let mut emit = |content: &Content, at: i64, from: u64, to: u64| match content {
        // Notes starting in their part of the region play, for their full length.
        Content::Midi { .. } | Content::Pattern { .. } => {
            let notes = match content {
                // A pattern as it plays: its lanes, and the modulators on its sounds.
                Content::Pattern { step, rows, swing } => {
                    std::borrow::Cow::Owned(crate::project::pattern_played(
                        *step,
                        rows,
                        (to as i64 - at).max(0) as u64,
                        *swing,
                        (region.start as i64 + at).max(0) as u64,
                        mods,
                        project.tempo,
                    ))
                }
                _ => content.notes((to as i64 - at).max(0) as u64).unwrap(),
            };
            for n in notes
                .iter()
                .map(|n| region.played(n))
                .filter(|n| (from as i64..to as i64).contains(&(at + n.start)))
            {
                let on = region.start + (at + n.start) as u64;
                let velocity = n.velocity as f32 / 127.0;
                let (pitch, start) = (n.pitch, project.samples(on, rate));
                events.push(Event {
                    at: start,
                    pitch,
                    velocity,
                });
                let off = project.samples(on + n.length, rate);
                events.push(Event {
                    at: off,
                    pitch,
                    velocity: 0.0,
                });
            }
            // Controllers as they stand where this part starts, then their moves; a note's own
            // moves follow it when the region transposes it.
            if let Content::Midi {
                controls: points, ..
            } = content
            {
                let moved = points.iter().map(|c| crate::project::ControlPoint {
                    start: at + c.start,
                    ..*c
                });
                for mut c in chase(moved, from as i64, to as i64) {
                    if let Some(p) = c.kind.pitch_mut() {
                        *p = (*p as i16 + region.transpose as i16).clamp(0, 127) as u8;
                    }
                    let tick = region.start + c.start as u64;
                    controls.push(ControlEvent {
                        at: project.samples(tick, rate),
                        kind: c.kind.to_control(),
                    });
                }
            }
        }
        Content::Audio {
            file,
            offset,
            stretch,
            flex,
            formants,
            warps,
            ..
        } => {
            let begin = (from as i64).max(at);
            if begin >= to as i64 {
                return;
            }
            // Warps are at the project's rate; the engine may run at another.
            let scale = rate / project.sample_rate as f64;
            let warps: Vec<(usize, usize)> = warps
                .iter()
                .map(|w| {
                    (
                        (w[0] as f64 * scale) as usize,
                        (w[1] as f64 * scale) as usize,
                    )
                })
                .collect();
            // In the stretched audio, the offset is stretched too (and warped, if it's warped).
            let offset = match (warps.is_empty(), files.frames(file)) {
                (false, Some(n)) => {
                    let len = (n as f64 * stretch).round() as usize;
                    stretch::Map::new(n, len, &warps).at(*offset as usize) as u64
                }
                _ => (*offset as f64 * stretch) as u64,
            };
            let skip = project.samples((begin - at) as u64, rate);
            let start = project.samples(region.start + begin as u64, rate);
            let end = project.samples(region.start + to, rate);
            let fit = Fit {
                ratio: *stretch,
                semitones: region.transpose as f64,
                flex: *flex,
                formants: *formants,
                warps,
            };
            match files.stretched(file, fit) {
                Ok(data) => clips.push(Clip {
                    start,
                    len: end - start,
                    offset: offset + skip,
                    data,
                    fades: [
                        project.samples(region.start, rate),
                        project.samples(region.fade_in.min(region.length), rate),
                        project.samples(region.start + region.length, rate),
                        project.samples(region.fade_out.min(region.length), rate),
                    ],
                }),
                Err(e) => problems.push(e),
            }
        }
        Content::Takes { .. } => {}
    };
    match &region.content {
        Content::Takes { takes, comp } => {
            for (from, to, take) in comp_segments(comp, takes.len(), region.length) {
                emit(&takes[take].content, takes[take].start, from, to);
            }
        }
        content => emit(content, 0, 0, region.length),
    }
}

/// From the main thread to the live engine.
pub enum Msg {
    Arrange(Box<Arrangement>),
    Insert(usize, Box<Lane>),
    Remove(usize),
    Play,
    Stop,
    Locate(u64),
    /// Start a lane's Live Loops cell (or stop it, with None) at a sample: its loop begins there.
    Launch {
        slot: usize,
        cell: Option<usize>,
        at: u64,
    },
    /// Play or release a note on a lane now, as when clicking a key in the piano roll.
    Note {
        slot: usize,
        pitch: u8,
        velocity: u8,
    },
    /// Play a sample now, over everything, as a browser previews it; or stop the one playing.
    Audition(Option<Box<Audition>>),
}

/// A sample being previewed: its audio, where it's got to, and how fast it's read (2 is an
/// octave up).
pub struct Audition {
    data: Arc<AudioData>,
    pos: f64,
    step: f64,
    gain: f32,
    /// A loop at the song's tempo: while the song plays it plays in time with it (from where
    /// the song is in it, over and over).
    synced: bool,
    started: bool,
}

impl Audition {
    pub fn new(data: Arc<AudioData>, semitones: f32, gain_db: f32) -> Audition {
        Audition {
            data,
            pos: 0.0,
            step: 2f64.powf(semitones as f64 / 12.0),
            gain: 10f32.powf(gain_db / 20.0),
            synced: false,
            started: false,
        }
    }

    /// Plays in time with the song while it plays, as a loop at its tempo does.
    pub fn synced(mut self) -> Audition {
        self.synced = true;
        self
    }

    fn done(&self) -> bool {
        self.pos as usize + 1 >= self.data.channels.first().map_or(0, Vec::len)
    }
}

/// What the live engine lets go of, for the main thread to drop.
pub enum Garbage {
    Arrangement(Box<Arrangement>),
    Lane(Box<Lane>),
    Audition(Box<Audition>),
}

/// What the live engine reports.
pub struct Shared {
    position: AtomicU64,
    playing: AtomicBool,
    /// The loudest sample (as f32 bits) per lane since last read, left then right; the master's
    /// last.
    meters: Vec<AtomicU32>,
    /// The most each lane's compressor (and the master's, last) turned down since last read, in
    /// dB as f32 bits.
    reduction: Vec<AtomicU32>,
    /// The lane live notes go to, or `usize::MAX` for none.
    armed: AtomicUsize,
    /// The busiest block since last read, as the share of its time spent working (f32 bits).
    load: AtomicU32,
    /// Frames the output device asks for at a time.
    buffer: AtomicU32,
    /// Click on every beat while playing.
    metronome: AtomicBool,
    /// Each lane's modulators' latest values (f32 bits), `MAX_MODS` a lane.
    mods: Vec<AtomicU32>,
}

impl Shared {
    pub fn new() -> Self {
        Shared {
            position: AtomicU64::new(0),
            playing: AtomicBool::new(false),
            meters: (0..(SLOTS + 1) * 2).map(|_| AtomicU32::new(0)).collect(),
            reduction: (0..=SLOTS).map(|_| AtomicU32::new(0)).collect(),
            armed: AtomicUsize::new(usize::MAX),
            load: AtomicU32::new(0),
            buffer: AtomicU32::new(0),
            metronome: AtomicBool::new(false),
            mods: (0..SLOTS * MAX_MODS).map(|_| AtomicU32::new(0)).collect(),
        }
    }

    /// Records a block's peaks and gain reduction on a lane (or `MASTER`).
    fn report(&self, slot: usize, peak: [f32; 2], reduction: f32) {
        self.meters[slot * 2].fetch_max(peak[0].to_bits(), Ordering::Relaxed);
        self.meters[slot * 2 + 1].fetch_max(peak[1].to_bits(), Ordering::Relaxed);
        self.reduction[slot].fetch_max(reduction.to_bits(), Ordering::Relaxed);
    }

    fn report_mod(&self, slot: usize, k: usize, value: f32) {
        if let Some(m) = self.mods.get(slot * MAX_MODS + k) {
            m.store(value.to_bits(), Ordering::Relaxed);
        }
    }

    /// A lane's modulators' latest values.
    pub fn mod_values(&self, slot: usize) -> [f32; MAX_MODS] {
        std::array::from_fn(|k| {
            self.mods
                .get(slot * MAX_MODS + k)
                .map_or(0.0, |m| f32::from_bits(m.load(Ordering::Relaxed)))
        })
    }

    /// The gain reduction on a lane (or `MASTER`) since the last call, in dB.
    pub fn take_reduction(&self, slot: usize) -> f32 {
        f32::from_bits(self.reduction[slot].swap(0, Ordering::Relaxed))
    }

    /// The busiest the audio thread has been since the last call: 1 is all of its time.
    pub fn take_load(&self) -> f32 {
        f32::from_bits(self.load.swap(0, Ordering::Relaxed))
    }

    /// The output device's buffer, in frames; 0 before it's asked for one.
    pub fn buffer(&self) -> u32 {
        self.buffer.load(Ordering::Relaxed)
    }

    pub fn set_metronome(&self, on: bool) {
        self.metronome.store(on, Ordering::Relaxed);
    }

    pub fn metronome(&self) -> bool {
        self.metronome.load(Ordering::Relaxed)
    }

    pub fn position(&self) -> u64 {
        self.position.load(Ordering::Relaxed)
    }

    pub fn playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    /// Sends live notes to a lane, or nowhere.
    pub fn arm(&self, slot: Option<usize>) {
        self.armed
            .store(slot.unwrap_or(usize::MAX), Ordering::Relaxed);
    }

    /// The peak level on a lane (or `MASTER`) since the last call, left and right.
    pub fn take_meter(&self, slot: usize) -> [f32; 2] {
        let take = |i: usize| f32::from_bits(self.meters[i].swap(0, Ordering::Relaxed));
        [take(slot * 2), take(slot * 2 + 1)]
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

/// The live engine's connections to the main thread and to MIDI input.
pub struct Link {
    pub rx: rtrb::Consumer<Msg>,
    pub garbage: rtrb::Producer<Garbage>,
    pub shared: Arc<Shared>,
    pub live: rtrb::Consumer<LiveNote>,
    pub played: rtrb::Producer<Played>,
}

/// Hands garbage back, or leaks it if the main thread has stopped collecting: freeing it here
/// could stall the audio thread.
fn trash(q: &mut rtrb::Producer<Garbage>, g: Garbage) {
    if let Err(rtrb::PushError::Full(g)) = q.push(g) {
        std::mem::forget(g);
    }
}

pub struct Engine {
    rate: f64,
    arrangement: Box<Arrangement>,
    lanes: Vec<Option<Box<Lane>>>,
    position: u64,
    playing: bool,
    /// Release every held note at the start of the next block.
    release: bool,
    /// What the song plays has changed while playing: notes it no longer has let go.
    rechase: bool,
    scratch: [Vec<f32>; 2],
    link: Option<Link>,
    /// Notes asked for by `Msg::Note`, for the next block.
    pending: [(usize, Note); LIVE_NOTES],
    pending_len: usize,
    master_strip: Box<Strip>,
    /// The metronome's click sounding: samples left, and its pitch.
    click: (u32, f32),
    /// Where the next block reads from if playback carries on; reading anywhere else is a jump.
    next_read: u64,
    /// A sample being previewed.
    audition: Option<Box<Audition>>,
    /// Each lane's peak last block, for the modulators that follow it.
    peaks: Box<[f32]>,
}

impl Engine {
    fn new(rate: f64, arrangement: Arrangement, lanes: Vec<Option<Box<Lane>>>) -> Self {
        Engine {
            rate,
            arrangement: Box::new(arrangement),
            lanes,
            position: 0,
            playing: false,
            release: false,
            rechase: false,
            scratch: [vec![0.0; MAX_BLOCK], vec![0.0; MAX_BLOCK]],
            link: None,
            pending: [(0, Note::default()); LIVE_NOTES],
            pending_len: 0,
            master_strip: Box::new(Strip::new(rate)),
            click: (0, 0.0),
            next_read: 0,
            audition: None,
            peaks: vec![0.0; SLOTS + 1].into_boxed_slice(),
        }
    }

    /// The engine for `project` (whose files are in `dir`) at `rate`, for offline use. The plugins
    /// it returns are the main-thread halves of its plugin processors: keep them as long as it.
    pub fn load(
        project: &Project,
        dir: &Path,
        rate: f64,
    ) -> Result<(Engine, Vec<LanePlugins>), String> {
        let (mut modules, mut plugins, mut lanes) = (Modules::new(), Vec::new(), Vec::new());
        let mut add = |instrument: Option<&Instrument>, inserts: &[PluginRef]| {
            let (lane, p, latency) = make_lane(instrument, inserts, rate, &mut modules, dir)?;
            // The preset and parameters the project sets, as the player gives them live; a
            // preset a saved state already holds isn't loaded again.
            let instrument_ref = match instrument {
                Some(Instrument::Plugin(r)) => Some(r),
                _ => None,
            };
            let refs = instrument_ref.into_iter().zip(&p.instrument);
            for (r, plugin) in refs.chain(inserts.iter().zip(&p.inserts)) {
                if let (Some(preset), None) = (&r.preset, &r.state) {
                    plugin.load_preset(preset)?;
                }
                for (&id, &v) in &r.params {
                    plugin.set_param(id, v);
                }
            }
            let mut lane = lane;
            lane.settle();
            lanes.push(Some(Box::new(lane)));
            plugins.push(p);
            Ok::<_, String>((lanes.len() - 1, latency))
        };
        let tracks = project
            .tracks
            .iter()
            .map(|t| add(t.instrument.as_ref(), &t.inserts))
            .collect::<Result<Vec<_>, _>>()?;
        let auxes = project
            .auxes
            .iter()
            .map(|a| add(None, &a.inserts))
            .collect::<Result<Vec<_>, _>>()?;
        let master = add(None, &project.master_inserts)?.0;
        let mut files = AudioFiles::new(dir, rate);
        let (arrangement, problems) =
            compile(project, rate, &tracks, &auxes, Some(master), &mut files);
        if let Some(p) = problems.into_iter().next() {
            return Err(p);
        }
        Ok((Engine::new(rate, arrangement, lanes), plugins))
    }

    /// An empty engine, filled and driven through its link.
    pub fn live(rate: f64, link: Link) -> Self {
        let mut e = Engine::new(
            rate,
            Arrangement::empty(),
            (0..SLOTS).map(|_| None).collect(),
        );
        e.link = Some(link);
        e
    }

    pub fn rate(&self) -> f64 {
        self.rate
    }

    pub fn position(&self) -> u64 {
        self.position
    }

    pub fn play(&mut self) {
        self.playing = true;
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.release = true;
    }

    pub fn locate(&mut self, sample: u64) {
        self.position = sample;
        self.release = true;
    }

    /// Whether the player driving this engine has gone.
    pub fn abandoned(&self) -> bool {
        self.link.as_ref().is_some_and(|l| l.rx.is_abandoned())
    }

    /// Starts a lane's loop cell, or stops it with None, when the playhead reaches `at`.
    pub fn launch(&mut self, slot: usize, cell: Option<usize>, at: u64) {
        if let Some(lane) = self.lanes.get_mut(slot).and_then(|l| l.as_deref_mut()) {
            lane.queued = Some((cell, at));
        }
    }

    fn receive(&mut self) {
        let Some(link) = &mut self.link else { return };
        if self.audition.as_ref().is_some_and(|a| a.done()) {
            trash(
                &mut link.garbage,
                Garbage::Audition(self.audition.take().unwrap()),
            );
        }
        while let Ok(msg) = link.rx.pop() {
            match msg {
                Msg::Arrange(a) => {
                    let old = std::mem::replace(&mut self.arrangement, a);
                    trash(&mut link.garbage, Garbage::Arrangement(old));
                    self.rechase = true;
                }
                Msg::Insert(slot, lane) => {
                    if let Some(old) = self.lanes[slot].replace(lane) {
                        trash(&mut link.garbage, Garbage::Lane(old));
                    }
                }
                Msg::Remove(slot) => {
                    if let Some(old) = self.lanes[slot].take() {
                        trash(&mut link.garbage, Garbage::Lane(old));
                    }
                }
                Msg::Play => self.playing = true,
                Msg::Stop => {
                    self.playing = false;
                    self.release = true;
                }
                Msg::Locate(at) => {
                    self.position = at;
                    self.release = true;
                }
                Msg::Launch { slot, cell, at } => {
                    if let Some(lane) = self.lanes.get_mut(slot).and_then(|l| l.as_deref_mut()) {
                        lane.queued = Some((cell, at));
                    }
                }
                Msg::Audition(new) => {
                    if let Some(old) = std::mem::replace(&mut self.audition, new) {
                        trash(&mut link.garbage, Garbage::Audition(old));
                    }
                }
                Msg::Note {
                    slot,
                    pitch,
                    velocity,
                } => {
                    if self.pending_len < LIVE_NOTES {
                        let velocity = velocity as f32 / 127.0;
                        self.pending[self.pending_len] = (
                            slot,
                            Note {
                                offset: 0,
                                pitch,
                                velocity,
                            },
                        );
                        self.pending_len += 1;
                    }
                }
            }
        }
    }

    /// Tells the engine how many frames the output device asks for at a time, to report.
    pub fn note_buffer(&self, frames: usize) {
        if let Some(link) = &self.link {
            link.shared.buffer.store(frames as u32, Ordering::Relaxed);
        }
    }

    /// Writes the next `frames` (at most `MAX_BLOCK`) of the mix into `out` and, if playing, moves
    /// the playhead on, back to the cycle's start when it reaches its end. Instruments and effects
    /// run while stopped too, so tails ring out. Allocation-free: safe on the audio thread.
    pub fn process(&mut self, out: [&mut [f32]; 2], frames: usize) {
        let began = Instant::now();
        let [l, r] = out;
        // Past the cycle's end (it was drawn behind the playhead, or play began after it): back
        // to its start, so the cycle is what plays. Before it, play runs on into it.
        if let Some((start, end)) = self.arrangement.cycle
            && self.playing
            && self.position >= end
        {
            self.position = start;
            self.release = true;
        }
        // What's heard wraps here; what's read, ahead of it by the plugins' latency, wraps in
        // `block`.
        let wrap = match self.arrangement.cycle {
            Some((start, end)) if self.playing && self.position < end => {
                let left = (end - self.position) as usize;
                (left < frames).then_some((left, start))
            }
            _ => None,
        };
        match wrap {
            Some((left, start)) => {
                self.block([&mut l[..left], &mut r[..left]], left);
                self.position = start;
                self.block([&mut l[left..], &mut r[left..]], frames - left);
            }
            None => self.block([l, r], frames),
        }
        if let Some(link) = &self.link {
            let load = began.elapsed().as_secs_f64() * self.rate / frames.max(1) as f64;
            link.shared
                .load
                .fetch_max((load as f32).to_bits(), Ordering::Relaxed);
        }
    }

    fn block(&mut self, out: [&mut [f32]; 2], frames: usize) {
        let rate = self.rate as f32;
        self.receive();
        // Live notes go to the armed lane, and back to the main thread as played.
        // ponytail: they land at the start of the block, up to MAX_BLOCK samples late; use
        // CoreMIDI timestamps for sample accuracy.
        let mut live = [Note::default(); LIVE_NOTES];
        let mut live_len = 0;
        let mut armed = usize::MAX;
        if let Some(link) = &mut self.link {
            armed = link.shared.armed.load(Ordering::Relaxed);
            while live_len < LIVE_NOTES {
                let Ok(n) = link.live.pop() else { break };
                let velocity = n.velocity as f32 / 127.0;
                live[live_len] = Note {
                    offset: 0,
                    pitch: n.pitch,
                    velocity,
                };
                live_len += 1;
                let played = Played {
                    at: self.position,
                    pitch: n.pitch,
                    velocity: n.velocity,
                };
                let _ = link.played.push(played);
            }
        }
        let pending = std::mem::take(&mut self.pending_len);
        let [out_l, out_r] = out;
        out_l[..frames].fill(0.0);
        out_r[..frames].fill(0.0);
        // What's heard now is `position`; the engine reads ahead by the plugins' latency, which
        // delay compensation then makes up, so it's heard on time.
        // ponytail: the first `lookahead` samples after starting play are skipped.
        let heard = self.position;
        let (playing, tempo) = (self.playing, self.arrangement.tempo);
        // Inside the cycle, reading ahead wraps round to its start, and a block whose reading
        // crosses its end reads the rest from the start.
        let cycle = self
            .arrangement
            .cycle
            .filter(|&(start, end)| playing && heard < end && end > start);
        let mut from = heard + self.arrangement.lookahead;
        if let Some((start, end)) = cycle
            && from >= end
        {
            from = start + (from - end) % (end - start);
        }
        let to = from + frames as u64;
        let split = cycle
            .filter(|&(_, end)| from < end && to > end)
            .map(|(start, end)| ((end - from) as usize, start));
        // Reading somewhere new (a locate, or the cycle going round) lets held notes go.
        let jumped = playing && from != self.next_read;
        self.next_read = match split {
            Some((left, start)) => start + (frames - left) as u64,
            None => to,
        };
        let release = std::mem::take(&mut self.release) || jumped;
        let rechase = std::mem::take(&mut self.rechase) && playing;
        let shared = self.link.as_ref().map(|l| &*l.shared);
        let [l, r] = &mut self.scratch;
        let (l, r) = (&mut l[..frames], &mut r[..frames]);
        let arrangement = &mut *self.arrangement;
        for bus in &mut arrangement.buses {
            bus[0][..frames].fill(0.0);
            bus[1][..frames].fill(0.0);
        }

        for &t in &arrangement.order {
            let plan = &arrangement.plans[t];
            let Some(lane) = self.lanes[plan.slot].as_deref_mut() else {
                continue;
            };
            l.fill(0.0);
            r.fill(0.0);
            let mut g = Gathered::new();
            if release {
                lane.held.release(0, |_| false, &mut g.notes, &mut g.n);
                lane.live.release(0, |_| false, &mut g.notes, &mut g.n);
            } else if rechase && lane.cell.is_none() {
                // An edit while playing: what the song still has sounding now rings on.
                let still = |p: u8| sounding(&plan.events, from, p);
                lane.held.release(0, still, &mut g.notes, &mut g.n);
            }
            // A launched cell takes over when its sample comes, held notes let go.
            if let Some((cell, at)) = lane.queued
                && at < to
            {
                lane.cell = cell.map(|c| (c, at));
                lane.queued = None;
                lane.held.release(0, |_| false, &mut g.notes, &mut g.n);
            }
            if playing {
                let into = [&mut *l, &mut *r];
                match lane
                    .cell
                    .and_then(|(c, start)| Some((plan.cells.get(c)?, start)))
                {
                    // Looping: the part of the loop this block covers, wrapping round, from the
                    // sample it starts on if that's in this block.
                    Some((cell, start)) => {
                        let lead = start.saturating_sub(from).min(frames as u64);
                        let span = frames as u64 - lead;
                        let p0 = from.max(start).saturating_sub(start) % cell.length;
                        let first = (cell.length - p0).min(span);
                        let held = &mut lane.held;
                        let (e, k, c) = (&cell.events, &cell.controls, &cell.clips);
                        let lead = lead as usize;
                        gather(e, k, c, (p0, p0 + first), lead, &mut g, held, into);
                        if first < span {
                            let into = [&mut *l, &mut *r];
                            let at = lead + first as usize;
                            gather(e, k, c, (0, span - first), at, &mut g, held, into);
                        }
                    }
                    None => {
                        let (e, k, c) = (&plan.events, &plan.controls, &plan.clips);
                        let held = &mut lane.held;
                        match split {
                            Some((left, start)) => {
                                let end = (from, from + left as u64);
                                gather(e, k, c, end, 0, &mut g, held, into);
                                // Round to the cycle's start: what's held lets go there.
                                held.release(left as u32, |_| false, &mut g.notes, &mut g.n);
                                let into = [&mut *l, &mut *r];
                                let rest = (start, start + (frames - left) as u64);
                                gather(e, k, c, rest, left, &mut g, held, into);
                            }
                            None => gather(e, k, c, (from, to), 0, &mut g, held, into),
                        }
                    }
                }
            }
            let now = self.pending[..pending]
                .iter()
                .filter(|(s, _)| *s == plan.slot)
                .map(|(_, n)| n);
            let live = if plan.slot == armed {
                &live[..live_len]
            } else {
                &[][..]
            };
            for note in now.chain(live) {
                if g.n < BLOCK_NOTES {
                    lane.live.apply(note.pitch, note.velocity > 0.0);
                    g.notes[g.n] = *note;
                    g.n += 1;
                }
            }
            // Notes from the song, a cell's loop and live playing can interleave.
            g.notes[..g.n].sort_by_key(|n| n.offset);
            g.controls[..g.m].sort_by_key(|c| c.offset);
            let (notes, controls) = (&g.notes[..g.n], &g.controls[..g.m]);
            // Its modulators, this block: in time with the song while it plays, set off by its
            // notes, hearing the tracks they follow as they were last block.
            let mut moved = Offsets::default();
            if !plan.mods.is_empty() {
                let input = ModInput {
                    beats: playing.then(|| from as f64 * tempo / (60.0 * self.rate)),
                    seconds: frames as f64 / self.rate,
                    tempo,
                    trigger: notes.iter().any(|n| n.velocity > 0.0),
                    gate: lane.held.any() || lane.live.any(),
                    follow: 0.0,
                };
                for (k, m) in plan.mods.iter().enumerate().take(MAX_MODS) {
                    let heard = self.peaks.get(plan.follow[k]).copied().unwrap_or(0.0);
                    let input = ModInput {
                        follow: heard,
                        ..input
                    };
                    let v = modulate::step(&m.kind, &mut lane.mods[k], &input);
                    if let Some(s) = shared {
                        s.report_mod(plan.slot, k, v);
                    }
                    for route in &m.routes {
                        moved.add(route.target, route.depth * v);
                    }
                }
            }
            match &mut lane.source {
                Source::Audio => {}
                Source::Synth(s) => s.process([&mut *l, &mut *r], frames, notes),
                Source::Drums(d) => d.process([&mut *l, &mut *r], frames, notes),
                Source::Sampler(s) => {
                    if let Some(settings) = &plan.sampler {
                        s.follow(settings, Arc::as_ptr(settings) as usize);
                        s.modulate(&settings.voice, &moved);
                    }
                    s.process([&mut *l, &mut *r], frames, notes, controls)
                }
                Source::Plugin(p) => {
                    plan.automate(Which::Instrument, p, (from, to), &moved);
                    p.set_transport(playing, from, tempo);
                    p.process([&mut *l, &mut *r], frames, notes, controls, None);
                }
            }
            let reduction = match moved.strip {
                false => lane.strip.run(&plan.dsp, l, r),
                true => {
                    let mut dsp = StripDsp {
                        eq: plan.dsp.eq,
                        comp: plan.dsp.comp,
                        sat: plan.dsp.sat,
                    };
                    dsp.eq.iter_mut().for_each(|e| moved.eq(e));
                    dsp.comp.iter_mut().for_each(|c| moved.comp(c));
                    dsp.sat.iter_mut().for_each(|s| moved.sat(s));
                    lane.strip.run(&dsp, l, r)
                }
            };
            let sidechains = &arrangement.sidechains;
            for (i, p) in lane.inserts.iter_mut().enumerate() {
                plan.automate(Which::Insert(i), p, (from, to), &moved);
                p.set_transport(playing, from, tempo);
                let sc = sidechain(sidechains, plan.sidechains.get(i), frames);
                p.process([&mut *l, &mut *r], frames, &[], &[], sc);
            }
            lane.pre.process(l, r, plan.delay);
            // Gain ramps across the block, so automation moves smoothly.
            let later = if playing {
                heard + frames as u64
            } else {
                heard
            };
            // Pre-fader sends: muting still silences them.
            for (k, &(aux, _, _)) in plan.sends.iter().enumerate().filter(|s| s.1.2) {
                let bus = &mut arrangement.buses[aux];
                let moved = db(moved.sends.get(k).copied().unwrap_or(0.0));
                let at = |s: u64| plan.send_at(k, s) * plan.audible_at(s) * moved;
                let (a, step) = (at(heard), (at(later) - at(heard)) / frames as f32);
                for i in 0..frames {
                    let level = a + step * i as f32;
                    bus[0][i] += l[i] * level;
                    bus[1][i] += r[i] * level;
                }
            }
            let now = (moved.volume, moved.pan);
            let (m0, m1) = (plan.matrix(heard, lane.mod_last), plan.matrix(later, now));
            lane.mod_last = now;
            let step: [f32; 4] = std::array::from_fn(|k| (m1[k] - m0[k]) / frames as f32);
            for i in 0..frames {
                let k = i as f32;
                let m: [f32; 4] = std::array::from_fn(|j| m0[j] + step[j] * k);
                let (a, b) = (l[i], r[i]);
                l[i] = a * m[0] + b * m[1];
                r[i] = a * m[2] + b * m[3];
            }
            if plan.pan_mode == PanMode::Binaural {
                let place = Place {
                    azimuth: modulate::apply_pan(plan.pan_at(heard), moved.pan) * 180.0,
                    ..plan.place
                };
                lane.binaural
                    .process(&mut l[..frames], &mut r[..frames], place, rate);
            }
            let mut peak = [0f32; 2];
            for i in 0..frames {
                peak = [peak[0].max(l[i].abs()), peak[1].max(r[i].abs())];
            }
            // What plugins hearing it as a sidechain get: it, after its fader.
            if plan.feeds {
                let [sl, sr] = &mut arrangement.sidechains[t];
                sl[..frames].copy_from_slice(l);
                sr[..frames].copy_from_slice(r);
            }
            // Post-fader sends.
            for (k, &(aux, _, _)) in plan.sends.iter().enumerate().filter(|s| !s.1.2) {
                let moved = db(moved.sends.get(k).copied().unwrap_or(0.0));
                let at = |s: u64| plan.send_at(k, s) * moved;
                let (a, step) = (at(heard), (at(later) - at(heard)) / frames as f32);
                let bus = &mut arrangement.buses[aux];
                for i in 0..frames {
                    let level = a + step * i as f32;
                    bus[0][i] += l[i] * level;
                    bus[1][i] += r[i] * level;
                }
            }
            lane.post.process(l, r, plan.direct);
            let (dest_l, dest_r) = match plan.output {
                Some(aux) => {
                    let [bl, br] = &mut arrangement.buses[aux];
                    (&mut bl[..frames], &mut br[..frames])
                }
                None => (&mut out_l[..frames], &mut out_r[..frames]),
            };
            for i in 0..frames {
                dest_l[i] += l[i];
                dest_r[i] += r[i];
            }
            if let Some(s) = shared {
                s.report(plan.slot, peak, reduction);
            }
            if let Some(p) = self.peaks.get_mut(plan.slot) {
                *p = peak[0].max(peak[1]);
            }
        }

        for (aux, plan) in arrangement.auxes.iter().enumerate() {
            let Some(lane) = self.lanes[plan.slot].as_deref_mut() else {
                continue;
            };
            l.copy_from_slice(&arrangement.buses[aux][0][..frames]);
            r.copy_from_slice(&arrangement.buses[aux][1][..frames]);
            let reduction = lane.strip.run(&plan.dsp, l, r);
            for (i, p) in lane.inserts.iter_mut().enumerate() {
                p.set_transport(playing, from, tempo);
                let sc = sidechain(&arrangement.sidechains, plan.sidechains.get(i), frames);
                p.process([&mut *l, &mut *r], frames, &[], &[], sc);
            }
            lane.post.process(l, r, plan.delay);
            let mut peak = [0f32; 2];
            for i in 0..frames {
                let (a, b) = (l[i] * plan.gain[0], r[i] * plan.gain[1]);
                out_l[i] += a;
                out_r[i] += b;
                peak = [peak[0].max(a.abs()), peak[1].max(b.abs())];
            }
            if let Some(s) = shared {
                s.report(plan.slot, peak, reduction);
            }
        }

        let reduction = self.master_strip.run(
            &arrangement.master_strip,
            &mut out_l[..frames],
            &mut out_r[..frames],
        );
        if let Some(lane) = arrangement
            .master_lane
            .and_then(|s| self.lanes[s].as_deref_mut())
        {
            for (i, p) in lane.inserts.iter_mut().enumerate() {
                p.set_transport(playing, from, tempo);
                let sides = arrangement.master_sidechains.get(i);
                let sc = sidechain(&arrangement.sidechains, sides, frames);
                p.process(
                    [&mut out_l[..frames], &mut out_r[..frames]],
                    frames,
                    &[],
                    &[],
                    sc,
                );
            }
        }
        // A preview plays over the mix, after the master's fader.
        if let Some(a) = self.audition.as_deref_mut().filter(|a| !a.done()) {
            let channels = &a.data.channels;
            let len = channels[0].len();
            // In time with the song: from where the song is in the loop.
            if a.synced && !a.started && playing && len > 1 {
                a.pos = (heard % len as u64) as f64;
            }
            a.started = true;
            for i in 0..frames {
                if a.synced && playing && a.pos as usize + 1 >= len {
                    a.pos -= (len - 1) as f64;
                }
                let at = a.pos as usize;
                if at + 1 >= len {
                    break;
                }
                let t = (a.pos - at as f64) as f32;
                for (c, out) in [&mut *out_l, &mut *out_r].into_iter().enumerate() {
                    let ch = &channels[c.min(channels.len() - 1)];
                    out[i] += (ch[at] + (ch[at + 1] - ch[at]) * t) * a.gain
                        / arrangement.master.max(1e-6);
                }
                a.pos += a.step;
            }
        }
        let mut peak = [0f32; 2];
        for (side, buf) in [&mut *out_l, &mut *out_r].into_iter().enumerate() {
            for s in buf[..frames].iter_mut() {
                *s *= arrangement.master;
                peak[side] = peak[side].max(s.abs());
            }
        }
        let metronome = shared.is_some_and(Shared::metronome);
        if playing && metronome {
            click(
                &mut self.click,
                heard,
                (
                    self.rate * 60.0 / tempo * arrangement.beat,
                    arrangement.beats_per_bar,
                ),
                self.rate,
                [&mut out_l[..frames], &mut out_r[..frames]],
            );
        }
        if playing {
            self.position = heard + frames as u64;
        }
        if let Some(s) = shared {
            s.report(MASTER, peak, reduction);
            s.position.store(self.position, Ordering::Relaxed);
            s.playing.store(self.playing, Ordering::Relaxed);
        }
    }

    /// Plays `frames` from the start into memory, straight through.
    pub fn render(&mut self, frames: u64) -> [Vec<f32>; 2] {
        self.render_from(0, frames)
    }

    /// Starts a render at sample `start`: the song plays from there, once through, cycle or not.
    pub fn start_render(&mut self, start: u64) {
        self.arrangement.cycle = None;
        self.locate(start);
        self.play();
    }

    /// The next `frames` of a render begun with `start_render`.
    pub fn render_on(&mut self, frames: u64) -> [Vec<f32>; 2] {
        let mut out = [vec![0f32; frames as usize], vec![0f32; frames as usize]];
        for at in (0..frames as usize).step_by(MAX_BLOCK) {
            let n = MAX_BLOCK.min(frames as usize - at);
            let [l, r] = &mut out;
            self.process([&mut l[at..], &mut r[at..]], n);
        }
        out
    }

    /// `frames` of the song from sample `start`, as a bounce plays them (notes begun before it
    /// aside).
    pub fn render_from(&mut self, start: u64, frames: u64) -> [Vec<f32>; 2] {
        let mut out = [vec![0f32; frames as usize], vec![0f32; frames as usize]];
        // A bounce plays the song through once, cycle or not.
        self.arrangement.cycle = None;
        self.locate(start);
        self.play();
        for at in (0..frames as usize).step_by(MAX_BLOCK) {
            let n = MAX_BLOCK.min(frames as usize - at);
            let [l, r] = &mut out;
            self.process([&mut l[at..], &mut r[at..]], n);
        }
        self.stop();
        out
    }

    /// Plays `frames` from the start into a stereo 32-bit float WAV.
    pub fn bounce(&mut self, path: &Path, frames: u64) -> Result<(), String> {
        let err = |e: hound::Error| format!("{}: {e}", path.display());
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: self.rate as u32,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut wav = hound::WavWriter::create(path, spec).map_err(err)?;
        let (mut l, mut r) = (vec![0f32; MAX_BLOCK], vec![0f32; MAX_BLOCK]);
        // A bounce plays the song through once, cycle or not.
        self.arrangement.cycle = None;
        self.locate(0);
        self.play();
        let mut left = frames;
        while left > 0 {
            let n = left.min(MAX_BLOCK as u64) as usize;
            self.process([&mut l, &mut r], n);
            for i in 0..n {
                wav.write_sample(l[i]).map_err(err)?;
                wav.write_sample(r[i]).map_err(err)?;
            }
            left -= n as u64;
        }
        self.stop();
        wav.finalize().map_err(err)
    }
}

/// The rate to play at through the default output: `wanted` if the device can run at it, else
/// the device's own (the live engine runs at that, converting audio as it loads it).
pub fn output_rate(wanted: u32) -> Result<u32, String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("no audio output device")?;
    let can = device
        .supported_output_configs()
        .map_err(|e| e.to_string())?
        .any(|c| {
            c.sample_format() == cpal::SampleFormat::F32
                && c.channels() >= 2
                && (c.min_sample_rate()..=c.max_sample_rate()).contains(&wanted)
        });
    match can {
        true => Ok(wanted),
        false => device
            .default_output_config()
            .map(|c| c.sample_rate())
            .map_err(|e| e.to_string()),
    }
}

impl Engine {
    /// Plays the engine through the default output device, set to the engine's rate (see
    /// `output_rate`).
    pub fn start_output(mut self) -> Result<cpal::Stream, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("no audio output device")?;
        let rate = self.rate as u32;
        let config = device
            .supported_output_configs()
            .map_err(|e| e.to_string())?
            .find(|c| {
                c.sample_format() == cpal::SampleFormat::F32
                    && c.channels() >= 2
                    && (c.min_sample_rate()..=c.max_sample_rate()).contains(&rate)
            })
            .ok_or_else(|| format!("the output device can't run at {rate} Hz"))?
            .with_sample_rate(rate)
            .config();
        let ch = config.channels as usize;
        let (mut l, mut r) = ([0f32; MAX_BLOCK], [0f32; MAX_BLOCK]);
        let stream = device
            .build_output_stream(
                config,
                move |out: &mut [f32], _| {
                    self.note_buffer(out.len() / ch);
                    for chunk in out.chunks_mut(MAX_BLOCK * ch) {
                        let frames = chunk.len() / ch;
                        self.process([&mut l, &mut r], frames);
                        for (f, frame) in chunk.chunks_mut(ch).enumerate() {
                            frame[0] = l[f];
                            frame[1] = r[f];
                            frame[2..].fill(0.0);
                        }
                    }
                },
                |e| eprintln!("audio: {e}"),
                None,
            )
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(stream)
    }
}

/// The metronome: a short blip on every beat from `heard`, higher on the bar, added to `out`.
/// `click` is the blip still sounding: samples left and its pitch.
fn click(
    click: &mut (u32, f32),
    heard: u64,
    (beat, per_bar): (f64, u64),
    rate: f64,
    [l, r]: [&mut [f32]; 2],
) {
    const LENGTH: f64 = 0.03;
    let length = (LENGTH * rate) as u32;
    let mut next = (heard as f64 / beat).ceil();
    for i in 0..l.len() {
        let t = heard + i as u64;
        if t as f64 >= next * beat - 0.5 {
            let bar = (next as u64).is_multiple_of(per_bar);
            *click = (length, if bar { 1_760.0 } else { 1_320.0 });
            next += 1.0;
        }
        if click.0 > 0 {
            let age = (length - click.0) as f32 / rate as f32;
            let s = (std::f32::consts::TAU * click.1 * age).sin() * (-age * 140.0).exp() * 0.3;
            l[i] += s;
            r[i] += s;
            click.0 -= 1;
        }
    }
}

/// An audio file's audio (WAV, AIFF, FLAC or MP3) at `rate`: converted, if it was recorded at
/// another.
pub(crate) fn load_wav(path: &Path, rate: f64) -> Result<AudioData, String> {
    let (sides, from) = read_audio(path)?;
    let channels = sides
        .iter()
        .map(|side| resample(side, from, rate))
        .collect();
    Ok(AudioData { channels })
}

/// An audio file's audio at `rate`, converted if it's at another: for previews read off the
/// audio and main threads.
pub fn load_audio(path: &Path, rate: f64) -> Result<AudioData, String> {
    load_wav(path, rate)
}

/// An audio file's first two channels (one, if it's mono) at its own rate, and that rate. WAV is
/// read with hound; AIFF, FLAC, MP3, and WAVs hound can't read, with Symphonia.
pub fn read_audio(path: &Path) -> Result<(Vec<Vec<f32>>, f64), String> {
    let wav = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("wav") || e.eq_ignore_ascii_case("wave"));
    match wav.then(|| read_wav(path)) {
        Some(Ok(audio)) => Ok(audio),
        Some(Err(e)) => decode(path).map_err(|_| e),
        None => decode(path),
    }
}

fn read_wav(path: &Path) -> Result<(Vec<Vec<f32>>, f64), String> {
    let err = |e: hound::Error| format!("{}: {e}", path.display());
    let mut wav = hound::WavReader::open(path).map_err(err)?;
    let spec = wav.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => wav
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(err)?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            wav.samples::<i32>()
                .map(|s| s.map(|s| s as f32 * scale))
                .collect::<Result<_, _>>()
                .map_err(err)?
        }
    };
    let n = spec.channels as usize;
    let sides = (0..n.min(2))
        .map(|c| samples.iter().skip(c).step_by(n).copied().collect())
        .collect();
    Ok((sides, spec.sample_rate as f64))
}

/// Decodes a file with Symphonia: its first audio track's first two channels.
fn decode(path: &Path) -> Result<(Vec<Vec<f32>>, f64), String> {
    use symphonia::core::{
        codecs::audio::AudioDecoderOptions,
        errors::Error,
        formats::{FormatOptions, TrackType, probe::Hint},
        io::MediaSourceStream,
        meta::MetadataOptions,
    };
    let err = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let file = std::fs::File::open(path).map_err(|e| err(&e))?;
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| err(&e))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| err(&"no audio in it"))?;
    let id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| err(&"no audio in it"))?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| err(&e))?;
    let (mut sides, mut block, mut rate) = (Vec::<Vec<f32>>::new(), Vec::new(), 0);
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            // A file cut short: keep what was read.
            Err(Error::IoError(_)) if !sides.is_empty() => break,
            Err(e) => return Err(err(&e)),
        };
        if packet.track_id != id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(audio) => {
                rate = audio.spec().rate();
                audio.copy_to_vecs_planar::<f32>(&mut block);
                if sides.is_empty() {
                    sides = vec![Vec::new(); block.len().min(2)];
                }
                for (side, part) in sides.iter_mut().zip(&block) {
                    side.extend_from_slice(part);
                }
            }
            Err(Error::DecodeError(_)) => continue,
            Err(e) => return Err(err(&e)),
        }
    }
    if sides.is_empty() || rate == 0 {
        return Err(err(&"no audio in it"));
    }
    Ok((sides, rate as f64))
}

/// Audio at `from` Hz, at `to` Hz, band-limited: a Blackman-windowed sinc, 32 taps a side, its
/// cutoff a little under the lower Nyquist, so going down doesn't alias and going up doesn't
/// image. The filter is worked out once, at 512 points between samples, and read between them.
pub(crate) fn resample(samples: &[f32], from: f64, to: f64) -> Vec<f32> {
    if from == to || samples.is_empty() {
        return samples.to_vec();
    }
    const TAPS: usize = 32;
    const PHASES: usize = 512;
    let ratio = from / to;
    let cutoff = (to / from).min(1.0) * 0.96;
    let sinc = |x: f64| match x.abs() < 1e-9 {
        true => 1.0,
        false => (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x),
    };
    let blackman = |t: f64| {
        let a = std::f64::consts::PI * t;
        0.42 + 0.5 * a.cos() + 0.08 * (2.0 * a).cos()
    };
    // At a phase p (p / PHASES past a sample), tap j is the sample j - TAPS + 1 from it.
    let table: Vec<[f32; 2 * TAPS]> = (0..=PHASES)
        .map(|p| {
            std::array::from_fn(|j| {
                let d = p as f64 / PHASES as f64 + TAPS as f64 - 1.0 - j as f64;
                (cutoff * sinc(cutoff * d) * blackman(d / TAPS as f64)) as f32
            })
        })
        .collect();
    let n = (samples.len() as f64 / ratio).round() as usize;
    (0..n)
        .map(|i| {
            let x = i as f64 * ratio;
            let c = x.floor();
            let at = (x - c) * PHASES as f64;
            let p = (at.floor() as usize).min(PHASES - 1);
            let w = (at - p as f64) as f32;
            let (lo, hi) = (&table[p], &table[p + 1]);
            let first = c as i64 - TAPS as i64 + 1;
            let mut acc = 0f32;
            for j in 0..2 * TAPS {
                let k = first + j as i64;
                if k >= 0 && (k as usize) < samples.len() {
                    acc += samples[k as usize] * (lo[j] + (hi[j] - lo[j]) * w);
                }
            }
            acc
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{AuxSend, Note as MidiNote, PPQ, Track};

    const RATE: f64 = 48_000.0;
    const BEAT: usize = 24_000; // samples per beat at 120 bpm
    const RELEASE: usize = 2_400; // the synth's 50 ms

    fn synth_track(name: &str, notes: Vec<MidiNote>, length: u64) -> Track {
        Track {
            name: name.into(),
            instrument: Some(Instrument::Synth),
            regions: vec![Region {
                start: 0,
                length,
                content: Content::Midi {
                    notes,
                    controls: Vec::new(),
                },
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn note(start: i64, length: u64, pitch: u8) -> MidiNote {
        MidiNote {
            start,
            length,
            pitch,
            velocity: 127,
        }
    }

    fn run(engine: &mut Engine, frames: usize) -> [Vec<f32>; 2] {
        let mut out = [vec![0f32; frames], vec![0f32; frames]];
        for at in (0..frames).step_by(MAX_BLOCK) {
            let n = MAX_BLOCK.min(frames - at);
            let [l, r] = &mut out;
            engine.process([&mut l[at..], &mut r[at..]], n);
        }
        out
    }

    fn render(project: &Project, dir: &Path, frames: usize) -> [Vec<f32>; 2] {
        let (mut engine, _) = Engine::load(project, dir, RATE).unwrap();
        engine.play();
        run(&mut engine, frames)
    }

    fn project(tracks: Vec<Track>) -> Project {
        Project {
            tracks,
            ..Project::new(RATE as u32)
        }
    }

    fn peak(s: &[f32]) -> f32 {
        s.iter().fold(0.0, |m, x| m.max(x.abs()))
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tempo-test-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn notes_start_on_their_beat_and_play_past_the_region_end() {
        // A beat-long note from beat 2 in a region that ends halfway through it, and a note
        // starting after the region ends, which doesn't play.
        let notes = vec![note(PPQ as i64, PPQ, 69), note(PPQ as i64 * 3, PPQ, 69)];
        let p = project(vec![synth_track("a", notes, PPQ * 3 / 2)]);
        let [l, _] = render(&p, Path::new("."), BEAT * 4);
        assert_eq!(peak(&l[..BEAT + 1]), 0.0);
        assert!(peak(&l[BEAT..BEAT + 1000]) > 0.1);
        assert!(peak(&l[BEAT * 2 - 1000..BEAT * 2]) > 0.1);
        assert_eq!(peak(&l[BEAT * 2 + RELEASE + 10..]), 0.0);
    }

    #[test]
    fn volume_and_pan() {
        let mut t = synth_track("a", vec![note(0, PPQ, 69)], PPQ);
        let loud = render(&project(vec![t.clone()]), Path::new("."), BEAT);
        t.volume_db = -20.0 * 2f32.log10(); // half
        t.pan = -1.0;
        let [l, r] = render(&project(vec![t]), Path::new("."), BEAT);
        assert!((peak(&l) / peak(&loud[0]) - 0.5).abs() < 1e-4);
        assert_eq!(peak(&r), 0.0);
    }

    #[test]
    fn pan_laws_and_stereo_pan_place_each_side() {
        let close = |a: [f32; 4], b: [f32; 4]| a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-3);
        let h = std::f32::consts::FRAC_1_SQRT_2;
        // Balance, as it was: the middle untouched; and by the laws.
        assert!(close(
            pan_matrix(PanMode::Balance, PanLaw::Flat, 0.0, 1.0, 1.0),
            [1.0, 0.0, 0.0, 1.0]
        ));
        assert!(close(
            pan_matrix(PanMode::Balance, PanLaw::Minus3, 0.0, 1.0, 1.0),
            [h, 0.0, 0.0, h]
        ));
        assert!(close(
            pan_matrix(PanMode::Balance, PanLaw::Minus3Compensated, -1.0, 1.0, 1.0),
            [std::f32::consts::SQRT_2, 0.0, 0.0, 0.0]
        ));
        assert!(close(
            pan_matrix(PanMode::Balance, PanLaw::Minus6, 0.0, 1.0, 1.0),
            [0.5, 0.0, 0.0, 0.5]
        ));
        // Stereo pan: full spread leaves the sides where they were; none folds them to the
        // middle; panned hard left with no spread, both sides go left.
        assert!(close(
            pan_matrix(PanMode::Stereo, PanLaw::Flat, 0.0, 1.0, 1.0),
            [1.0, 0.0, 0.0, 1.0]
        ));
        assert!(close(
            pan_matrix(PanMode::Stereo, PanLaw::Flat, 0.0, 0.0, 1.0),
            [h, h, h, h]
        ));
        assert!(close(
            pan_matrix(PanMode::Stereo, PanLaw::Flat, -1.0, 0.0, 1.0),
            [1.0, 1.0, 0.0, 0.0]
        ));
    }

    #[test]
    fn binaural_pan_puts_the_far_ear_later_and_quieter_and_a_muted_region_is_silent() {
        let mut t = synth_track("a", vec![note(0, PPQ, 69)], PPQ);
        t.pan = -0.5; // 90° left
        t.pan_mode = PanMode::Binaural;
        let [l, r] = render(&project(vec![t.clone()]), Path::new("."), BEAT);
        let first = |x: &[f32]| x.iter().position(|s| s.abs() > 1e-4).unwrap();
        assert!(peak(&l) > peak(&r) && peak(&r) > 0.0);
        assert!(first(&r) > first(&l), "{} {}", first(&l), first(&r));
        t.regions[0].mute = true;
        let [l, _] = render(&project(vec![t]), Path::new("."), BEAT);
        assert_eq!(peak(&l), 0.0);
    }

    #[test]
    fn an_lfo_moves_the_volume_and_a_follower_ducks_one_track_under_another() {
        use crate::modulate::{LfoShape, ModRate};
        // A square LFO a beat long, 24 dB down while high: the first half of each beat quiet.
        let mut pad = synth_track("pad", vec![note(0, PPQ * 4, 60)], PPQ * 4);
        pad.modulators = vec![Modulator {
            kind: ModKind::Lfo {
                shape: LfoShape::Square,
                rate: ModRate::Beats(1.0),
                phase: 0.0,
                unipolar: false,
            },
            routes: vec![ModRoute {
                target: ModTarget::Volume,
                depth: -24.0,
            }],
        }];
        let [l, _] = render(&project(vec![pad.clone()]), Path::new("."), BEAT * 2);
        let (low, high) = (
            peak(&l[BEAT + 3000..BEAT + 9000]),
            peak(&l[BEAT + 15000..BEAT + 21000]),
        );
        assert!(high > low * 10.0, "{low} {high}");

        // The pad follows a kick: ducked just after it hits, back up a beat on.
        let kick = Track {
            id: 7,
            ..synth_track("kick", vec![note(0, PPQ / 8, 36)], PPQ * 4)
        };
        let mut pad = Track {
            id: 8,
            ..synth_track("pad", vec![note(0, PPQ * 4, 72)], PPQ * 4)
        };
        let both = |pad: &Track| {
            let [l, _] = render(
                &project(vec![kick.clone(), pad.clone()]),
                Path::new("."),
                BEAT * 2,
            );
            // After the kick's note and its release: the pad alone.
            (peak(&l[6_500..8_000]), peak(&l[BEAT + 6_500..BEAT + 8_000]))
        };
        let (plain, _) = both(&pad);
        pad.modulators = vec![Modulator {
            kind: ModKind::follower(7),
            routes: vec![ModRoute {
                target: ModTarget::Volume,
                depth: -30.0,
            }],
        }];
        let (ducked, later) = both(&pad);
        assert!(ducked < plain * 0.5, "{ducked} {plain}");
        assert!(later > ducked * 2.0, "{later} {ducked}");
    }

    #[test]
    fn binaural_behind_is_duller_above_brighter_and_far_off_quieter() {
        // A high note (8.4 kHz), where the ears' filters tell.
        let level = |pan: f32, elevation: Option<f32>, distance: Option<f32>| {
            let mut t = synth_track("a", vec![note(0, PPQ, 120)], PPQ);
            (t.pan, t.pan_mode, t.elevation, t.distance) =
                (pan, PanMode::Binaural, elevation, distance);
            let [l, r] = render(&project(vec![t]), Path::new("."), BEAT);
            peak(&l[2_000..]).max(peak(&r[2_000..]))
        };
        let front = level(0.0, None, None);
        assert!(level(1.0, None, None) < front * 0.8, "behind");
        assert!(level(0.0, Some(90.0), None) > front * 1.2, "above");
        assert!(level(0.0, None, Some(3.0)) < front * 0.6, "far");
        // Its place, there and back.
        let p = Place::from_xyz(
            Place {
                azimuth: -120.0,
                elevation: 30.0,
                distance: 2.0,
            }
            .xyz(),
        );
        assert!((p.azimuth + 120.0).abs() < 0.01 && (p.elevation - 30.0).abs() < 0.01);
    }

    #[test]
    fn volume_automation_overrides_the_fader() {
        // A held note, faded from 0 dB to silence over the first beat.
        let mut t = synth_track("a", vec![note(0, PPQ * 4, 69)], PPQ * 4);
        t.volume_db = -60.0;
        t.automation.volume = vec![AutoPoint::new(0, 0.0), AutoPoint::new(PPQ, -144.0)];
        let [l, _] = render(&project(vec![t]), Path::new("."), BEAT * 2);
        // Straight in dB: about -6 dB a 24th of the way in, about -30 dB at a fifth.
        let early = peak(&l[500..1_500]);
        let later = peak(&l[4_500..5_500]);
        assert!(early > 0.08, "{early}"); // the fader alone would give 0.00025
        assert!(later < early / 4.0 && later > 1e-4, "{early} {later}");
        assert!(peak(&l[BEAT + MAX_BLOCK..]) < 1e-6);
    }

    #[test]
    fn compensation_lines_everything_up() {
        let c = compensate(&[0, 100, 30], &[50, 0]);
        assert_eq!(
            c,
            Compensation {
                lookahead: 150,
                tracks: vec![100, 0, 70],
                direct: 50,
                auxes: vec![0, 50]
            }
        );
        assert_eq!(compensate(&[], &[]).lookahead, 0);
    }

    #[test]
    fn delay_lines_delay() {
        let mut d = Delay::new();
        let (mut l, mut r) = ([1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0]);
        d.process(&mut l, &mut r, 2);
        assert_eq!((l, r), ([0.0, 0.0, 1.0, 2.0], [0.0, 0.0, 5.0, 6.0]));
    }

    #[test]
    fn sends_reach_their_aux_after_the_fader() {
        // A unity send to a unity aux: the track is heard twice as loud, direct and through it.
        let track = synth_track("a", vec![note(0, PPQ, 69)], PPQ);
        let [alone, _] = render(&project(vec![track.clone()]), Path::new("."), BEAT);
        let mut p = project(vec![track]);
        p.tracks[0].sends = vec![AuxSend {
            bus: 1,
            level_db: 0.0,
            pre: false,
        }];
        p.add_aux();
        let [l, _] = render(&p, Path::new("."), BEAT);
        assert!(peak(&alone) > 0.1);
        assert_eq!(l, alone.iter().map(|x| x * 2.0).collect::<Vec<_>>());

        p.auxes[0].mute = true;
        assert_eq!(render(&p, Path::new("."), BEAT)[0], alone);
        p.auxes[0].mute = false;
        p.tracks[0].volume_db = -144.0; // post-fader: the send goes quiet too
        assert!(peak(&render(&p, Path::new("."), BEAT)[0]) < 1e-6);
    }

    #[test]
    fn mute_and_send_automation_play() {
        // Muted by automation for its second beat: silent there, sounding either side.
        let track = synth_track("a", vec![note(0, PPQ * 3, 69)], PPQ * 3);
        let mut p = project(vec![track.clone()]);
        p.tracks[0].automation.mute = vec![
            AutoPoint {
                step: true,
                ..AutoPoint::new(0, 0.0)
            },
            AutoPoint {
                step: true,
                ..AutoPoint::new(PPQ, 1.0)
            },
            AutoPoint {
                step: true,
                ..AutoPoint::new(PPQ * 2, 0.0)
            },
        ];
        let [l, _] = render(&p, Path::new("."), BEAT * 3);
        assert!(peak(&l[..BEAT - MAX_BLOCK]) > 0.1);
        assert!(peak(&l[BEAT + MAX_BLOCK..BEAT * 2 - MAX_BLOCK]) < 1e-6);
        assert!(peak(&l[BEAT * 2 + MAX_BLOCK..]) > 0.1);

        // A send automated from silence up to unity over the first beat: the aux hears nothing,
        // then all of it.
        let mut p = project(vec![track]);
        p.tracks[0].sends = vec![AuxSend {
            bus: 1,
            level_db: 0.0,
            pre: false,
        }];
        p.add_aux();
        p.tracks[0].automation.sends = vec![crate::project::SendAuto {
            bus: 1,
            points: vec![
                AutoPoint::new(0, -144.0),
                AutoPoint::new(PPQ, -144.0),
                AutoPoint::new(PPQ + 1, 0.0),
            ],
        }];
        let [l, _] = render(&p, Path::new("."), BEAT * 2);
        let [alone, _] = render(
            &project(vec![p.tracks[0].clone()]),
            Path::new("."),
            BEAT * 2,
        );
        let (a, b) = (
            peak(&l[..BEAT - MAX_BLOCK]),
            peak(&alone[..BEAT - MAX_BLOCK]),
        );
        assert!((a - b).abs() < 1e-4, "{a} {b}");
        let (a, b) = (
            peak(&l[BEAT + MAX_BLOCK..]),
            peak(&alone[BEAT + MAX_BLOCK..]),
        );
        assert!((a - b * 2.0).abs() < 1e-3, "{a} {b}");
    }

    #[test]
    fn a_track_routed_to_a_stack_plays_through_it() {
        let track = synth_track("a", vec![note(0, PPQ, 69)], PPQ);
        let [alone, _] = render(&project(vec![track.clone()]), Path::new("."), BEAT);
        let mut p = project(vec![track]);
        p.add_stack(&[0]);
        p.auxes[0].volume_db = -20.0 * 2f32.log10(); // half
        let [l, _] = render(&p, Path::new("."), BEAT);
        assert!(peak(&alone) > 0.1);
        assert!((peak(&l) / peak(&alone) - 0.5).abs() < 1e-4);
        p.auxes[0].mute = true; // the stack's channel carries it all
        assert_eq!(peak(&render(&p, Path::new("."), BEAT)[0]), 0.0);
    }

    #[test]
    fn delay_compensation_waits_for_the_slowest_track() {
        // Two synth tracks, left and right; say the left one's plugins are 100 samples late.
        // The right one is delayed to match, and the engine reads 100 samples ahead. The synth
        // isn't really late, so the left is heard 100 samples early and the right on time.
        let mut a = synth_track("a", vec![note(PPQ as i64, PPQ, 69)], PPQ * 2);
        a.pan = -1.0;
        let mut b = a.clone();
        b.pan = 1.0;
        let p = project(vec![a, b]);
        let mut lanes = Vec::new();
        for t in &p.tracks {
            let (lane, _, _) = make_lane(
                t.instrument.as_ref(),
                &[],
                RATE,
                &mut Modules::new(),
                Path::new("."),
            )
            .unwrap();
            lanes.push(Some(Box::new(lane)));
        }
        let mut files = AudioFiles::new(Path::new("."), RATE);
        let (arrangement, _) = compile(&p, RATE, &[(0, 100), (1, 0)], &[], None, &mut files);
        let mut engine = Engine::new(RATE, arrangement, lanes);
        engine.play();
        let [l, r] = run(&mut engine, BEAT * 2);
        let onset = |s: &[f32]| s.iter().position(|x| x.abs() > 0.0).unwrap();
        assert_eq!(onset(&r) - onset(&l), 100);
        assert!((onset(&r) as i64 - BEAT as i64).abs() <= 1);
    }

    #[test]
    fn mute_and_solo() {
        let a = synth_track("a", vec![note(0, PPQ, 60)], PPQ);
        let b = synth_track("b", vec![note(0, PPQ, 72)], PPQ);
        let alone = |t: &Track| render(&project(vec![t.clone()]), Path::new("."), BEAT);
        let (a_alone, b_alone) = (alone(&a), alone(&b));

        let mut solo_b = b.clone();
        solo_b.solo = true;
        assert_eq!(
            render(&project(vec![a.clone(), solo_b]), Path::new("."), BEAT),
            b_alone
        );

        let mut mute_b = b;
        mute_b.mute = true;
        assert_eq!(
            render(&project(vec![a, mute_b]), Path::new("."), BEAT),
            a_alone
        );
    }

    #[test]
    fn audio_regions_play_their_slice_of_the_file() {
        let dir = scratch_dir("audio");
        let ramp: Vec<f32> = (0..BEAT * 2)
            .map(|i| i as f32 / (BEAT * 2) as f32)
            .collect();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: RATE as u32,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(dir.join("ramp.wav"), spec).unwrap();
        ramp.iter().for_each(|&s| w.write_sample(s).unwrap());
        w.finalize().unwrap();

        // Half a beat in, one beat long, starting 100 samples into the file.
        let region = Region {
            start: PPQ / 2,
            length: PPQ,
            content: Content::Audio {
                file: "ramp.wav".into(),
                offset: 100,
                stretch: 1.0,
                flex: Flex::default(),
                follow: false,
                formants: false,
                warps: Vec::new(),
            },
            ..Default::default()
        };
        let track = Track {
            name: "audio".into(),
            regions: vec![region],
            ..Default::default()
        };
        let [l, r] = render(&project(vec![track]), &dir, BEAT * 2);
        let start = BEAT / 2;
        assert_eq!(peak(&l[..start]), 0.0);
        assert_eq!(&l[start..start + BEAT], &ramp[100..100 + BEAT]);
        assert_eq!(peak(&l[start + BEAT..]), 0.0);
        assert_eq!(l, r);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_audio_regions_fades_bring_it_in_and_take_it_out() {
        let dir = scratch_dir("fades");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: RATE as u32,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(dir.join("flat.wav"), spec).unwrap();
        (0..BEAT * 2).for_each(|_| w.write_sample(0.5f32).unwrap());
        w.finalize().unwrap();
        // Two beats, faded in over the first half beat and out over the last.
        let region = Region {
            start: 0,
            length: PPQ * 2,
            fade_in: PPQ / 2,
            fade_out: PPQ / 2,
            content: Content::Audio {
                file: "flat.wav".into(),
                offset: 0,
                stretch: 1.0,
                flex: Flex::default(),
                follow: false,
                formants: false,
                warps: Vec::new(),
            },
            ..Default::default()
        };
        let track = Track {
            name: "audio".into(),
            regions: vec![region],
            ..Default::default()
        };
        let [l, _] = render(&project(vec![track]), &dir, BEAT * 2);
        let half = BEAT / 2;
        assert!(l[0].abs() < 1e-3); // silent at the start
        assert!(l[half / 2] > 0.3 && l[half / 2] < 0.4, "{}", l[half / 2]); // sin 45°: 0.707 of 0.5
        assert!((l[BEAT] - 0.5).abs() < 1e-6); // full in the middle
        assert!(l[BEAT * 2 - 1] < 0.01); // gone by the end
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_missing_audio_file_leaves_out_its_region_only() {
        let region = Region {
            start: 0,
            length: PPQ,
            content: Content::Audio {
                file: "gone.wav".into(),
                offset: 0,
                stretch: 1.0,
                flex: Flex::default(),
                follow: false,
                formants: false,
                warps: Vec::new(),
            },
            ..Default::default()
        };
        let audio = Track {
            name: "audio".into(),
            regions: vec![region],
            ..Default::default()
        };
        let synth = synth_track("a", vec![note(0, PPQ, 69)], PPQ);
        let p = project(vec![audio, synth]);
        let mut files = AudioFiles::new(Path::new("."), RATE);
        let (arrangement, problems) = compile(&p, RATE, &[(0, 0), (1, 0)], &[], None, &mut files);
        assert_eq!(problems.len(), 1);
        assert!(arrangement.plans[0].clips.is_empty());
        assert_eq!(arrangement.plans[1].events.len(), 2);
        assert!(Engine::load(&p, Path::new("."), RATE).is_err()); // a bounce does fail
    }

    #[test]
    fn a_pattern_loops_over_its_region() {
        use crate::project::PatternRow;
        // Kick on steps 1 and 3 of four sixteenths, over a beat and a half: it comes round again.
        let rows = vec![PatternRow {
            pitch: 36,
            cells: vec![100, 0, 90, 0],
            mute: false,
            ..Default::default()
        }];
        let region = Region {
            start: 0,
            length: PPQ * 3 / 2,
            content: Content::Pattern {
                step: PPQ / 4,
                rows,
                swing: 50.0,
            },
            ..Default::default()
        };
        let track = Track {
            name: "d".into(),
            instrument: Some(Instrument::Drums),
            regions: vec![region],
            ..Default::default()
        };
        let mut files = AudioFiles::new(Path::new("."), RATE);
        let (arrangement, _) = compile(
            &project(vec![track.clone()]),
            RATE,
            &[(0, 0)],
            &[],
            None,
            &mut files,
        );
        let ons: Vec<u64> = arrangement.plans[0]
            .events
            .iter()
            .filter(|e| e.velocity > 0.0)
            .map(|e| e.at)
            .collect();
        let sixteenth = BEAT as u64 / 4;
        assert_eq!(ons, [0, 2 * sixteenth, 4 * sixteenth]);
        assert!(peak(&render(&project(vec![track]), Path::new("."), BEAT)[0]) > 0.1); // the kit plays it
    }

    #[test]
    fn a_take_folder_plays_its_comp() {
        use crate::project::{CompPiece, Take};
        // Two takes, an octave apart; the comp plays the first for a beat, then the second.
        let take = |pitch| Take {
            start: 0,
            content: Content::Midi {
                notes: vec![note(0, PPQ / 2, pitch), note(PPQ as i64, PPQ / 2, pitch)],
                controls: Vec::new(),
            },
        };
        let comp = vec![
            CompPiece { from: 0, take: 0 },
            CompPiece { from: PPQ, take: 1 },
        ];
        let region = Region {
            start: 0,
            length: PPQ * 2,
            content: Content::Takes {
                takes: vec![take(57), take(69)],
                comp,
            },
            ..Default::default()
        };
        let track = Track {
            name: "t".into(),
            instrument: Some(Instrument::Synth),
            regions: vec![region],
            ..Default::default()
        };
        let mut files = AudioFiles::new(Path::new("."), RATE);
        let (arrangement, _) = compile(
            &project(vec![track]),
            RATE,
            &[(0, 0)],
            &[],
            None,
            &mut files,
        );
        let ons: Vec<(u64, u8)> = arrangement.plans[0]
            .events
            .iter()
            .filter(|e| e.velocity > 0.0)
            .map(|e| (e.at, e.pitch))
            .collect();
        assert_eq!(ons, [(0, 57), (BEAT as u64, 69)]);
    }

    #[test]
    fn flex_stretches_audio_and_keeps_its_pitch() {
        // A second of 441 Hz, flexed to fill two seconds (four beats at 120 bpm).
        let dir = scratch_dir("flex");
        let tone: Vec<f32> = (0..48_000)
            .map(|i| 0.5 * (std::f32::consts::TAU * 441.0 * i as f32 / 48_000.0).sin())
            .collect();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(dir.join("tone.wav"), spec).unwrap();
        tone.iter().for_each(|&s| w.write_sample(s).unwrap());
        w.finalize().unwrap();
        let content = Content::Audio {
            file: "tone.wav".into(),
            offset: 0,
            stretch: 1.0,
            flex: Flex::default(),
            follow: false,
            formants: false,
            warps: Vec::new(),
        };
        let region = Region {
            start: 0,
            length: PPQ * 2,
            content,
            ..Default::default()
        };
        let mut p = project(vec![Track {
            name: "a".into(),
            regions: vec![region],
            ..Default::default()
        }]);
        p.flex((0, 0), PPQ * 4).unwrap();
        assert!(matches!(
            p.region((0, 0)).content,
            Content::Audio { stretch: 2.0, .. }
        ));
        let [l, _] = render(&p, &dir, BEAT * 5);
        // Sound to the end of the second second, and silence after.
        assert!(peak(&l[BEAT * 3..BEAT * 4 - 4_000]) > 0.2);
        assert!(peak(&l[BEAT * 4 + 2_000..]) < 0.05);
        // Still 441 Hz: about 441 upward zero crossings a second.
        let second = &l[24_000..72_000];
        let ups = second
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        assert!((ups as i64 - 441).abs() <= 3, "{ups}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_player_plays_a_stand_in_while_a_stretch_is_made_off_its_thread() {
        let dir = scratch_dir("background");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(dir.join("tone.wav"), spec).unwrap();
        for i in 0..48_000 {
            let s = 0.5 * (std::f32::consts::TAU * 441.0 * i as f32 / 48_000.0).sin();
            w.write_sample(s).unwrap();
        }
        w.finalize().unwrap();
        let mut files = AudioFiles::new(&dir, 48_000.0).in_background();
        let get = |files: &mut AudioFiles| {
            files.begin();
            let data = files
                .stretched(Path::new("tone.wav"), stretch::Fit::ratio(1.5))
                .unwrap();
            files.end();
            data
        };
        // At once: as long as the stretch will be.
        let stand_in = get(&mut files);
        assert_eq!(stand_in.channels[0].len(), 72_000);
        assert!(files.rendering());
        let deadline = Instant::now() + Duration::from_secs(10);
        while !files.fresh() {
            assert!(Instant::now() < deadline, "never made");
            std::thread::sleep(Duration::from_millis(5));
        }
        // Made: the next arrangement has it, and nothing's left to make.
        let made = get(&mut files);
        assert!(!Arc::ptr_eq(&stand_in, &made));
        assert_eq!(made.channels[0].len(), 72_000);
        assert!(!files.rendering());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_transposed_audio_region_moves_in_pitch_not_length() {
        // A second of 441 Hz, an octave up: 882 Hz, still a second.
        let dir = scratch_dir("transpose");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(dir.join("tone.wav"), spec).unwrap();
        for i in 0..48_000 {
            w.write_sample(0.5 * (std::f32::consts::TAU * 441.0 * i as f32 / 48_000.0).sin())
                .unwrap();
        }
        w.finalize().unwrap();
        let region = Region {
            start: 0,
            length: PPQ * 2,
            transpose: 12,
            content: Content::Audio {
                file: "tone.wav".into(),
                offset: 0,
                stretch: 1.0,
                flex: Flex::default(),
                follow: false,
                formants: false,
                warps: Vec::new(),
            },
            ..Default::default()
        };
        let p = project(vec![Track {
            name: "a".into(),
            regions: vec![region],
            ..Default::default()
        }]);
        let [l, _] = render(&p, &dir, BEAT * 3);
        let second = &l[6_000..42_000];
        let ups = second
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        assert!((ups as i64 - 661).abs() <= 8, "{ups}"); // 882 Hz over 0.75 s
        assert!(peak(&l[BEAT * 2 - 4_000..BEAT * 2 - 1_000]) > 0.2);
        assert!(peak(&l[BEAT * 2 + 3_000..]) < 0.05);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_launched_cell_loops_instead_of_the_track() {
        use crate::project::Cell;
        // The track's own region is silent; its cell is a beat-long loop with a short note.
        let mut t = synth_track("a", vec![], PPQ * 8);
        let notes = vec![note(0, PPQ / 4, 69)];
        let region = Region {
            start: 0,
            length: PPQ,
            content: Content::Midi {
                notes,
                controls: Vec::new(),
            },
            ..Default::default()
        };
        t.cells.push(Cell { scene: 0, region });
        let (mut engine, _) = Engine::load(&project(vec![t]), Path::new("."), RATE).unwrap();
        engine.play();
        assert_eq!(peak(&run(&mut engine, BEAT)[0]), 0.0);
        // Launched at beat 2: it plays there, rests, and comes round again at beat 3.
        engine.launch(0, Some(0), BEAT as u64 * 2);
        let [l, _] = run(&mut engine, BEAT * 3);
        let beat = |b: usize, from: f32, to: f32| {
            peak(
                &l[b * BEAT + (BEAT as f32 * from) as usize
                    ..b * BEAT + (BEAT as f32 * to) as usize],
            )
        };
        assert_eq!(beat(0, 0.0, 1.0), 0.0); // beat 2 of the song is before the launch
        assert!(beat(1, 0.02, 0.2) > 0.1 && beat(1, 0.5, 0.95) == 0.0);
        assert!(beat(2, 0.02, 0.2) > 0.1);
        // Stopped, the track's own (silent) region plays again.
        engine.launch(0, None, engine.position());
        let [l, _] = run(&mut engine, BEAT);
        assert_eq!(peak(&l[RELEASE + MAX_BLOCK..]), 0.0);
    }

    /// A plugin stand-in: it notes the controls it hears (at the sample they fall on) and the
    /// first sample its sidechain isn't silent; as an effect it passes audio through.
    #[derive(Default, Clone)]
    struct Probe {
        heard: std::sync::Arc<std::sync::Mutex<Vec<(u64, ControlKind)>>>,
        sidechain_from: std::sync::Arc<std::sync::Mutex<Option<u64>>>,
        at: u64,
    }

    impl crate::plugin::ProcessorApi for Probe {
        fn latency(&self) -> u32 {
            0
        }
        fn set_transport(&mut self, _: bool, position: u64, _: f64) {
            self.at = position;
        }
        fn automate(&mut self, _: u32, _: f64, _: f64) {}
        fn sidechain(&self) -> bool {
            true
        }
        fn pending(&self) -> bool {
            false
        }
        fn process(
            &mut self,
            _: [&mut [f32]; 2],
            frames: usize,
            _: &[Note],
            controls: &[Control],
            sidechain: Option<[&[f32]; 2]>,
        ) {
            let mut heard = self.heard.lock().unwrap();
            heard.extend(controls.iter().map(|c| (self.at + c.offset as u64, c.kind)));
            if let Some([l, _]) = sidechain
                && let Some(i) = l[..frames].iter().position(|x| x.abs() > 1e-3)
            {
                let mut from = self.sidechain_from.lock().unwrap();
                from.get_or_insert(self.at + i as u64);
            }
            self.at += frames as u64;
        }
    }

    /// An engine for `p` whose tracks have their built-in instruments, but `probe` as the
    /// instrument of the tracks with a plugin and the insert of tracks with inserts.
    fn probed(p: &Project, probe: &Probe) -> Engine {
        let mut lanes = Vec::new();
        for t in &p.tracks {
            let built_in = t
                .instrument
                .as_ref()
                .filter(|i| !matches!(i, Instrument::Plugin(_)));
            let made = make_lane(built_in, &[], RATE, &mut Modules::new(), Path::new("."));
            let mut lane = made.unwrap().0;
            if matches!(t.instrument, Some(Instrument::Plugin(_))) {
                lane.source = Source::Plugin(Box::new(probe.clone()));
            }
            if !t.inserts.is_empty() {
                lane.inserts.push(Box::new(probe.clone()));
            }
            lanes.push(Some(Box::new(lane)));
        }
        let tracks: Vec<_> = (0..p.tracks.len()).map(|i| (i, 0)).collect();
        let mut files = AudioFiles::new(Path::new("."), RATE);
        let (arrangement, problems) = compile(p, RATE, &tracks, &[], None, &mut files);
        assert!(problems.is_empty(), "{problems:?}");
        Engine::new(RATE, arrangement, lanes)
    }

    #[test]
    fn controllers_reach_the_instrument_as_they_stand_at_each_pass() {
        use crate::project::{ControlPoint, ControlValue::Cc};
        // A beat-long region from beat 2, looping: the mod wheel at 10 from before it starts,
        // then 100 halfway through.
        let controls = vec![
            ControlPoint {
                start: -100,
                kind: Cc {
                    number: 1,
                    value: 10,
                },
            },
            ControlPoint {
                start: PPQ as i64 / 2,
                kind: Cc {
                    number: 1,
                    value: 100,
                },
            },
        ];
        let region = Region {
            start: PPQ,
            length: PPQ,
            looped: true,
            content: Content::Midi {
                notes: vec![],
                controls,
            },
            ..Default::default()
        };
        let t = Track {
            name: "p".into(),
            instrument: Some(Instrument::Plugin(PluginRef::default())),
            regions: vec![
                region,
                Region {
                    start: PPQ * 3,
                    length: PPQ,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let probe = Probe::default();
        let mut engine = probed(&project(vec![t]), &probe);
        engine.play();
        run(&mut engine, BEAT * 4);
        let wheel = |v: u8| ControlKind::Cc {
            number: 1,
            value: v as f32 / 127.0,
        };
        let half = BEAT as u64 / 2;
        let (b1, b2) = (BEAT as u64, BEAT as u64 * 2);
        // Each pass of the loop starts where the mod wheel stood before it, and moves halfway.
        assert_eq!(
            *probe.heard.lock().unwrap(),
            [
                (b1, wheel(10)),
                (b1 + half, wheel(100)),
                (b2, wheel(10)),
                (b2 + half, wheel(100))
            ]
        );
    }

    #[test]
    fn a_sidechain_hears_its_source_in_the_same_block() {
        // The track hearing the sidechain comes first, so the source must be worked out before it.
        let mut source = synth_track("kick", vec![note(PPQ as i64, PPQ, 45)], PPQ * 4);
        source.id = 7;
        let mut hears = synth_track("bass", vec![], PPQ * 4);
        hears.inserts = vec![PluginRef {
            sidechain: Some(7),
            ..Default::default()
        }];
        let probe = Probe::default();
        let mut engine = probed(&project(vec![hears, source]), &probe);
        engine.play();
        run(&mut engine, BEAT * 3);
        let from = probe
            .sidechain_from
            .lock()
            .unwrap()
            .expect("the sidechain was silent");
        assert!((BEAT as u64..BEAT as u64 + 64).contains(&from), "{from}");
    }

    #[test]
    fn sidechain_sources_are_worked_out_first() {
        assert_eq!(
            sidechain_order(&[vec![Some(2)], vec![], vec![Some(1)]]),
            [1, 2, 0]
        );
        // A loop keeps everyone, each once.
        let mut order = sidechain_order(&[vec![Some(1)], vec![Some(0)]]);
        order.sort();
        assert_eq!(order, [0, 1]);
    }

    #[test]
    fn a_launched_audio_cell_plays_its_loop() {
        use crate::project::Cell;
        // A quarter second of tone, as a beat-long loop on an audio track with no regions.
        let dir = scratch_dir("audio-cell");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(dir.join("loop.wav"), spec).unwrap();
        (0..12_000).for_each(|i| w.write_sample((i as f32 * 0.06).sin() * 0.5).unwrap());
        w.finalize().unwrap();
        let region = Region {
            start: 0,
            length: PPQ,
            content: Content::Audio {
                file: "loop.wav".into(),
                offset: 0,
                stretch: 1.0,
                flex: Flex::default(),
                follow: false,
                formants: false,
                warps: Vec::new(),
            },
            ..Default::default()
        };
        let t = Track {
            name: "a".into(),
            cells: vec![Cell { scene: 0, region }],
            ..Default::default()
        };
        let (mut engine, _) = Engine::load(&project(vec![t]), &dir, RATE).unwrap();
        engine.play();
        engine.launch(0, Some(0), BEAT as u64);
        let [l, _] = run(&mut engine, BEAT * 3);
        assert_eq!(peak(&l[..BEAT]), 0.0);
        // Each beat: the tone, then the rest of the beat quiet.
        for b in 1..3 {
            assert!(peak(&l[b * BEAT + 100..b * BEAT + 11_000]) > 0.4);
            assert!(peak(&l[b * BEAT + 13_000..(b + 1) * BEAT]) < 0.01);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stopping_releases_held_notes() {
        let p = project(vec![synth_track("a", vec![note(0, PPQ * 8, 69)], PPQ * 8)]);
        let (mut engine, _) = Engine::load(&p, Path::new("."), RATE).unwrap();
        engine.play();
        assert!(peak(&run(&mut engine, MAX_BLOCK * 10)[0]) > 0.1);
        engine.stop();
        run(&mut engine, RELEASE + MAX_BLOCK);
        assert_eq!(peak(&run(&mut engine, MAX_BLOCK)[0]), 0.0);
        assert_eq!(engine.position(), (MAX_BLOCK * 10) as u64);
    }

    #[test]
    fn stopping_releases_overlapping_notes_of_one_pitch() {
        // Two notes on A, the second starting while the first still sounds: two voices.
        let notes = vec![note(0, PPQ * 8, 69), note(PPQ as i64, PPQ * 8, 69)];
        let p = project(vec![synth_track("a", notes, PPQ * 8)]);
        let (mut engine, _) = Engine::load(&p, Path::new("."), RATE).unwrap();
        engine.play();
        run(&mut engine, BEAT * 2);
        engine.stop();
        run(&mut engine, RELEASE + MAX_BLOCK);
        assert_eq!(peak(&run(&mut engine, MAX_BLOCK)[0]), 0.0);
    }

    #[test]
    fn an_edit_while_playing_lets_go_of_notes_the_song_no_longer_has() {
        let (mut tx, rx) = rtrb::RingBuffer::new(16);
        let (gtx, _garbage) = rtrb::RingBuffer::new(16);
        let (_live, live_rx) = rtrb::RingBuffer::new(16);
        let (played_tx, _played) = rtrb::RingBuffer::new(16);
        let link = Link {
            rx,
            garbage: gtx,
            shared: Arc::new(Shared::new()),
            live: live_rx,
            played: played_tx,
        };
        let mut engine = Engine::live(RATE, link);
        // A long A, and a long C that the edit keeps.
        let notes = vec![note(0, PPQ * 8, 69), note(0, PPQ * 8, 60)];
        let p = project(vec![synth_track("a", notes, PPQ * 8)]);
        let lane = make_lane(
            Some(&Instrument::Synth),
            &[],
            RATE,
            &mut Modules::new(),
            Path::new("."),
        )
        .unwrap()
        .0;
        let mut files = AudioFiles::new(Path::new("."), RATE);
        let arrange = |p: &Project, files: &mut AudioFiles| {
            Box::new(compile(p, RATE, &[(3, 0)], &[], None, files).0)
        };
        for m in [
            Msg::Insert(3, Box::new(lane)),
            Msg::Arrange(arrange(&p, &mut files)),
            Msg::Play,
        ] {
            assert!(tx.push(m).is_ok());
        }
        run(&mut engine, BEAT);
        // The A goes (as a key change or a deleted note would take it); the C stays.
        let mut edited = p.clone();
        if let Content::Midi { notes, .. } = &mut edited.tracks[0].regions[0].content {
            notes.retain(|n| n.pitch == 60);
        }
        assert!(tx.push(Msg::Arrange(arrange(&edited, &mut files))).is_ok());
        run(&mut engine, RELEASE + MAX_BLOCK);
        let [l, _] = run(&mut engine, BEAT / 2);
        // Only the C rings on: its 261.6 Hz, and no 440 Hz.
        let level = |f: f64| {
            let (mut re, mut im) = (0.0, 0.0);
            for (i, &x) in l.iter().enumerate() {
                let w = 2.0 * std::f64::consts::PI * f * i as f64 / RATE;
                re += x as f64 * w.cos();
                im += x as f64 * w.sin();
            }
            (re * re + im * im).sqrt() * 2.0 / l.len() as f64
        };
        let (c, a) = (level(261.63), level(440.0));
        assert!(c > 0.05 && a < c / 20.0, "C {c}, A {a}"); // what's left of A is the C's spill
    }

    #[test]
    fn the_live_engine_follows_its_messages() {
        let (mut tx, rx) = rtrb::RingBuffer::new(16);
        let (gtx, mut garbage) = rtrb::RingBuffer::new(16);
        let (mut live, live_rx) = rtrb::RingBuffer::new(16);
        let (played_tx, mut played) = rtrb::RingBuffer::new(16);
        let shared = Arc::new(Shared::new());
        let link = Link {
            rx,
            garbage: gtx,
            shared: shared.clone(),
            live: live_rx,
            played: played_tx,
        };
        let mut engine = Engine::live(RATE, link);
        let p = project(vec![synth_track("a", vec![note(0, PPQ * 8, 69)], PPQ * 8)]);
        let t = &p.tracks[0];
        let (lane, _, _) = make_lane(
            t.instrument.as_ref(),
            &[],
            RATE,
            &mut Modules::new(),
            Path::new("."),
        )
        .unwrap();
        let mut files = AudioFiles::new(Path::new("."), RATE);
        let (arrangement, _) = compile(&p, RATE, &[(7, 0)], &[], None, &mut files);
        for m in [
            Msg::Insert(7, Box::new(lane)),
            Msg::Arrange(Box::new(arrangement)),
            Msg::Play,
        ] {
            assert!(tx.push(m).is_ok());
        }
        assert!(peak(&run(&mut engine, MAX_BLOCK)[0]) > 0.0);
        assert!(shared.playing());
        assert_eq!(shared.position(), MAX_BLOCK as u64);
        assert!(shared.take_meter(7)[0] > 0.0 && shared.take_meter(MASTER)[1] > 0.0);
        // The empty arrangement it started with comes back.
        assert!(matches!(garbage.pop(), Ok(Garbage::Arrangement(_))));

        // A live note on the armed lane plays, and comes back as played at the playhead.
        assert!(tx.push(Msg::Stop).is_ok());
        run(&mut engine, RELEASE + MAX_BLOCK);
        shared.arm(Some(7));
        assert!(
            live.push(LiveNote {
                pitch: 72,
                velocity: 100
            })
            .is_ok()
        );
        assert!(peak(&run(&mut engine, MAX_BLOCK)[0]) > 0.0);
        let at = MAX_BLOCK as u64;
        assert_eq!(
            played.pop(),
            Ok(Played {
                at,
                pitch: 72,
                velocity: 100
            })
        );
        assert!(
            tx.push(Msg::Note {
                slot: 7,
                pitch: 72,
                velocity: 0
            })
            .is_ok()
        );

        assert!(tx.push(Msg::Remove(7)).is_ok());
        assert_eq!(peak(&run(&mut engine, MAX_BLOCK)[0]), 0.0);
        assert!(matches!(garbage.pop(), Ok(Garbage::Lane(_))));
    }

    #[test]
    fn bounce_writes_a_stereo_float_wav() {
        let dir = scratch_dir("bounce");
        let p = project(vec![synth_track("a", vec![note(0, PPQ, 69)], PPQ)]);
        let (mut engine, _) = Engine::load(&p, &dir, RATE).unwrap();
        engine.bounce(&dir.join("out.wav"), 30_000).unwrap();
        let wav = hound::WavReader::open(dir.join("out.wav")).unwrap();
        assert_eq!((wav.spec().channels, wav.duration()), (2, 30_000));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_cycle_loops_playback() {
        let mut p = project(vec![synth_track("a", vec![note(0, PPQ * 4, 69)], PPQ * 4)]);
        p.cycle = Some(crate::project::Cycle {
            start: PPQ,
            end: PPQ * 2,
            off: false,
        });
        let (mut engine, _) = Engine::load(&p, Path::new("."), RATE).unwrap();
        engine.locate(BEAT as u64);
        engine.play();
        run(&mut engine, BEAT + 1000);
        assert_eq!(engine.position(), BEAT as u64 + 1000);
        // After it, it goes back to its start rather than playing on past it.
        let (mut engine, _) = Engine::load(&p, Path::new("."), RATE).unwrap();
        engine.locate(3 * BEAT as u64);
        engine.play();
        run(&mut engine, 1000);
        assert!((BEAT as u64..2 * BEAT as u64).contains(&engine.position()));
        // Before the cycle, it plays on into it; switched off, it plays straight through.
        p.cycle.as_mut().unwrap().off = true;
        let (mut engine, _) = Engine::load(&p, Path::new("."), RATE).unwrap();
        engine.locate(BEAT as u64);
        engine.play();
        run(&mut engine, BEAT + 1000);
        assert_eq!(engine.position(), 2 * BEAT as u64 + 1000);
    }

    #[test]
    fn the_channel_eq_and_compressor_shape_the_sound() {
        let t = synth_track("a", vec![note(0, PPQ * 2, 69)], PPQ * 2);
        let dry = peak(&render(&project(vec![t.clone()]), Path::new("."), BEAT)[0][BEAT / 2..]);
        let mut boosted = t.clone();
        boosted.eq = Some(Eq {
            low_mid: crate::project::Band {
                freq: 440.0,
                gain_db: 6.0,
                q: 1.0,
            },
            ..Eq::default()
        });
        let louder =
            peak(&render(&project(vec![boosted.clone()]), Path::new("."), BEAT)[0][BEAT / 2..]);
        assert!((louder / dry - 2.0).abs() < 0.05, "{dry} {louder}");
        // Bypassed, it's as if it weren't there.
        boosted.eq.as_mut().unwrap().bypass = true;
        let same = peak(&render(&project(vec![boosted]), Path::new("."), BEAT)[0][BEAT / 2..]);
        assert!((same - dry).abs() < 1e-6);

        let mut squashed = t;
        squashed.comp = Some(Comp {
            threshold_db: -40.0,
            ratio: 20.0,
            ..Comp::default()
        });
        let quieter = peak(&render(&project(vec![squashed]), Path::new("."), BEAT)[0][BEAT / 2..]);
        assert!(quieter < dry / 4.0, "{dry} {quieter}");
    }

    #[test]
    fn an_audition_plays_over_the_mix_and_lets_go_at_its_end() {
        let (mut tx, rx) = rtrb::RingBuffer::new(16);
        let (gtx, mut garbage) = rtrb::RingBuffer::new(16);
        let (_live, live_rx) = rtrb::RingBuffer::new(16);
        let (played_tx, _played) = rtrb::RingBuffer::new(16);
        let link = Link {
            rx,
            garbage: gtx,
            shared: Arc::new(Shared::new()),
            live: live_rx,
            played: played_tx,
        };
        let mut engine = Engine::live(RATE, link);
        // A thousand samples of 0.5, an octave up: five hundred samples of it, stopped or not.
        let data = Arc::new(AudioData::mono(vec![0.5; 1000]));
        let a = Audition::new(data, 12.0, 0.0);
        assert!(tx.push(Msg::Audition(Some(Box::new(a)))).is_ok());
        let [l, r] = run(&mut engine, MAX_BLOCK * 2);
        assert!((l[10] - 0.5).abs() < 1e-6 && (r[490] - 0.5).abs() < 1e-6);
        assert_eq!(peak(&l[502..]), 0.0);
        // Done, it goes back to the main thread to be dropped.
        run(&mut engine, MAX_BLOCK);
        assert!(matches!(garbage.pop(), Ok(Garbage::Audition(_))));
    }

    #[test]
    fn the_metronome_clicks_on_the_beat_while_playing() {
        let (mut tx, rx) = rtrb::RingBuffer::new(16);
        let (gtx, _garbage) = rtrb::RingBuffer::new(16);
        let (_live, live_rx) = rtrb::RingBuffer::new(16);
        let (played_tx, _played) = rtrb::RingBuffer::new(16);
        let shared = Arc::new(Shared::new());
        let link = Link {
            rx,
            garbage: gtx,
            shared: shared.clone(),
            live: live_rx,
            played: played_tx,
        };
        let mut engine = Engine::live(RATE, link);
        assert!(tx.push(Msg::Play).is_ok());
        assert_eq!(peak(&run(&mut engine, MAX_BLOCK)[0]), 0.0);
        shared.set_metronome(true);
        // The next beat, at 120 bpm, is half a second in.
        let [l, _] = run(&mut engine, BEAT);
        let at = BEAT - MAX_BLOCK;
        assert_eq!(peak(&l[..at - 1]), 0.0);
        assert!(peak(&l[at..at + 500]) > 0.1);
        assert!(shared.take_load() > 0.0);
    }

    fn note_ons(p: &Project) -> Vec<(u64, u8, u8)> {
        let mut files = AudioFiles::new(Path::new("."), RATE);
        let (a, _) = compile(p, RATE, &[(0, 0)], &[], None, &mut files);
        a.plans[0]
            .events
            .iter()
            .filter(|e| e.velocity > 0.0)
            .map(|e| (e.at, e.pitch, (e.velocity * 127.0).round() as u8))
            .collect()
    }

    #[test]
    fn a_looped_region_plays_until_the_next_region() {
        let mut t = synth_track("a", vec![note(0, 240, 60)], PPQ);
        t.regions[0].looped = true;
        t.regions.push(Region {
            start: PPQ * 3,
            length: PPQ,
            content: Content::Midi {
                notes: vec![note(0, 240, 72)],
                controls: Vec::new(),
            },
            ..Default::default()
        });
        let b = BEAT as u64;
        assert_eq!(
            note_ons(&project(vec![t])),
            [
                (0, 60, 127),
                (b, 60, 127),
                (2 * b, 60, 127),
                (3 * b, 72, 127)
            ]
        );
    }

    #[test]
    fn loops_dragged_out_stop_where_they_were_dragged_and_stretching_spreads_notes() {
        // Looped to three beats, with the next region at five: three plays.
        let mut t = synth_track("a", vec![note(0, 240, 60)], PPQ);
        t.regions[0].looped = true;
        t.regions[0].loop_length = Some(PPQ * 3);
        t.regions.push(Region {
            start: PPQ * 5,
            length: PPQ,
            content: Content::Midi {
                notes: vec![note(0, 240, 72)],
                controls: Vec::new(),
            },
            ..Default::default()
        });
        let b = BEAT as u64;
        let mut p = project(vec![t]);
        assert_eq!(
            note_ons(&p),
            [
                (0, 60, 127),
                (b, 60, 127),
                (2 * b, 60, 127),
                (5 * b, 72, 127)
            ]
        );
        // Trimming its start keeps where its loops stop.
        p.trim_start((0, 0), PPQ / 2);
        assert_eq!(p.loop_end((0, 0)), PPQ * 3);
        // Stretched to twice as long: the note starts and lasts twice as late and long.
        p.tracks[0].regions[1].start = PPQ * 5;
        p.flex((0, 1), PPQ * 2).unwrap();
        let r = &p.tracks[0].regions[1];
        let Content::Midi { notes, .. } = &r.content else {
            unreachable!()
        };
        assert_eq!(
            (r.length, notes[0].start, notes[0].length),
            (PPQ * 2, 0, 480)
        );
    }

    #[test]
    fn region_playback_settings_change_what_plays_not_the_notes() {
        let mut t = synth_track("a", vec![note(100, 240, 60), note(350, 240, 64)], PPQ);
        let r = &mut t.regions[0];
        r.transpose = 12;
        r.velocity = -27;
        r.quantize = Some(240);
        let p = project(vec![t]);
        let sixteenth = BEAT as u64 / 4;
        assert_eq!(note_ons(&p), [(0, 72, 100), (sixteenth, 76, 100)]);
        let Content::Midi { notes, .. } = &p.tracks[0].regions[0].content else {
            unreachable!()
        };
        assert_eq!((notes[0].start, notes[0].pitch), (100, 60));
    }

    #[test]
    fn pre_fader_sends_ignore_the_fader_but_not_mute() {
        let track = synth_track("a", vec![note(0, PPQ, 69)], PPQ);
        let mut p = project(vec![track]);
        p.tracks[0].sends = vec![AuxSend {
            bus: 1,
            level_db: 0.0,
            pre: true,
        }];
        p.add_aux();
        p.tracks[0].volume_db = -144.0;
        assert!(peak(&render(&p, Path::new("."), BEAT)[0]) > 0.1);
        p.tracks[0].mute = true;
        assert!(peak(&render(&p, Path::new("."), BEAT)[0]) < 1e-6);
    }

    #[test]
    fn the_cycle_wraps_what_plugins_read_ahead() {
        // A track 64 samples late (as a plugin's latency would make it): the engine reads 64
        // ahead, so each pass of the cycle must read its start before the last one's heard end.
        let mut p = project(vec![synth_track("a", vec![note(0, 240, 69)], PPQ)]);
        p.cycle = Some(crate::project::Cycle {
            start: 0,
            end: PPQ,
            off: false,
        });
        let mut files = AudioFiles::new(Path::new("."), RATE);
        let (arrangement, _) = compile(&p, RATE, &[(0, 64)], &[], None, &mut files);
        let lane = make_lane(
            Some(&Instrument::Synth),
            &[],
            RATE,
            &mut Modules::new(),
            Path::new("."),
        )
        .unwrap()
        .0;
        let mut engine = Engine::new(RATE, arrangement, vec![Some(Box::new(lane))]);
        engine.play();
        let [l, _] = run(&mut engine, 2 * BEAT);
        // The note sounds again as the second pass begins, and not long before.
        assert!(peak(&l[BEAT - 64..BEAT + 2000]) > 0.1);
        assert!(peak(&l[BEAT / 2..BEAT - 100]) < 1e-3);
    }

    #[test]
    fn the_master_saturator_rounds_off_the_mix() {
        let t = synth_track("a", vec![note(0, PPQ * 2, 69)], PPQ * 2);
        let p = project(vec![t]);
        let dry = peak(&render(&p, Path::new("."), BEAT)[0][BEAT / 2..]);
        let mut hot = p.clone();
        hot.master_sat = Some(Saturator {
            drive_db: 24.0,
            ..Saturator::default()
        });
        let wet = peak(&render(&hot, Path::new("."), BEAT)[0][BEAT / 2..]);
        assert!(wet < dry * 0.6, "{dry} {wet}");
    }

    /// With a real instrument, which CI doesn't have:
    /// `TEMPO_TEST_VST3="/path/Some Synth.vst3" cargo test -p tempo-engine -- --ignored`
    #[test]
    #[ignore]
    fn a_plugin_parameter_follows_its_automation() {
        let path = std::env::var("TEMPO_TEST_VST3").expect("set TEMPO_TEST_VST3");
        let class = crate::scan::inspect(Path::new(&path))
            .unwrap()
            .remove(0)
            .class;
        let plugin = PluginRef {
            path: path.into(),
            class,
            ..Default::default()
        };
        let mut t = synth_track("s", vec![note(0, PPQ * 8, 57)], PPQ * 8);
        t.instrument = Some(Instrument::Plugin(plugin));
        let p = project(vec![t]);
        // Its loudest-sounding level parameter, turned right down by automation.
        let (_engine, lanes) = Engine::load(&p, Path::new("."), RATE).unwrap();
        let params = lanes[0].instrument.as_ref().unwrap().params();
        let volume = params
            .iter()
            .find(|q| q.name.to_lowercase().contains("volume"))
            .unwrap_or_else(|| panic!("no volume among {} params", params.len()));
        let loud = peak(&render(&p, Path::new("."), BEAT * 2)[0][BEAT..]);
        let mut quiet = p.clone();
        quiet.tracks[0]
            .automation
            .plugins
            .push(crate::project::PluginAuto {
                plugin: Which::Instrument,
                param: volume.id,
                name: volume.name.clone(),
                points: vec![AutoPoint::new(0, 0.0)],
            });
        let soft = peak(&render(&quiet, Path::new("."), BEAT * 2)[0][BEAT..]);
        assert!(loud > 0.01 && soft < loud / 4.0, "{loud} {soft}");
        // A value the project sets, as Smart Controls and agents do, holds in a render too.
        let mut set = p.clone();
        if let Some(Instrument::Plugin(r)) = &mut set.tracks[0].instrument {
            r.params.insert(volume.id, 0.0);
        }
        let held = peak(&render(&set, Path::new("."), BEAT * 2)[0][BEAT..]);
        assert!(held < loud / 4.0, "{loud} {held}");
        // Off, the automation is ignored.
        quiet.tracks[0].automation.mode = AutoMode::Off;
        let off = peak(&render(&quiet, Path::new("."), BEAT * 2)[0][BEAT..]);
        assert!((off - loud).abs() < loud * 0.1, "{loud} {off}");
    }

    #[test]
    fn audio_at_another_rate_is_converted() {
        // A 1 kHz sine at 44.1 kHz, converted to 48 kHz: the same second, the same pitch.
        let sine: Vec<f32> = (0..44_100)
            .map(|i| (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / 44_100.0).sin() as f32)
            .collect();
        let out = resample(&sine, 44_100.0, 48_000.0);
        assert_eq!(out.len(), 48_000);
        let level = |f: f64| {
            let (mut re, mut im) = (0.0, 0.0);
            let tail = &out[1000..47_000];
            for (i, &x) in tail.iter().enumerate() {
                let w = 2.0 * std::f64::consts::PI * f * (i + 1000) as f64 / 48_000.0;
                re += x as f64 * w.cos();
                im += x as f64 * w.sin();
            }
            (re * re + im * im).sqrt() * 2.0 / tail.len() as f64
        };
        assert!((level(1000.0) - 1.0).abs() < 0.01, "{}", level(1000.0));
        assert!(level(1100.0) < 0.01);
        // And down: 48 kHz to 44.1 kHz keeps the length in time.
        assert_eq!(resample(&out, 48_000.0, 44_100.0).len(), 44_100);
    }
}
