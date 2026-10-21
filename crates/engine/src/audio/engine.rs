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

use crate::dsp::modulate::{
    self, MAX_MODS, ModInput, ModKind, ModOver, ModParam, ModRoute, ModState, ModTarget, Modulator,
    Offsets,
};
use crate::{
    audio::peaks::Peaks,
    audio::stretch::{self, Fit},
    dsp::Strip,
    dsp::drums::Drums,
    dsp::eq::Made,
    dsp::sampler::Sampler,
    dsp::synth::Synth,
    plugin::{self, Control, ControlKind, Libraries, Note, Plugin, Processor},
    song::project::{
        AutoMode, AutoPoint, Comp, Content, Eq, EqField, Flex, Instrument, PanLaw, PanMode, Param,
        Place, PluginRef, Project, Region, Saturator, Which, chase, comp_segments, value_at,
    },
};

/// The most frames one `process` call handles.
pub const MAX_BLOCK: usize = 512;
/// Most note events one lane takes in one block; more are dropped.
const BLOCK_NOTES: usize = 256;
/// Most controller moves an instrument hears a block.
const BLOCK_CONTROLS: usize = 256;
/// Most live or previewed notes the engine takes in one block.
const LIVE_NOTES: usize = 64;
/// The longest delay compensation for one lane, in samples: room for an EQ's longest linear
/// phase (two stages at very high resolution) and spectral dynamics, with plugins after.
const MAX_DELAY: usize = 1 << 16;
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

    /// A file fitted as `fit` says (see `crate::audio::stretch`), made once for each.
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
    /// Each stack's shared modulators' values this block, by aux.
    stack_mods: Vec<[f32; MAX_MODS]>,
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
            stack_mods: Vec::new(),
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

/// A channel's built-in EQ, compressor and saturator, unless absent or bypassed; and what the
/// EQ's linear phase or spectral dynamics need, made here, off the audio thread.
fn strip_dsp(eq: Option<&Eq>, comp: Option<&Comp>, sat: Option<&Saturator>, rate: f64) -> StripDsp {
    let eq = eq.filter(|e| !e.bypass).copied();
    StripDsp {
        made: eq.as_ref().map(|e| Arc::new(Made::new(e, rate))),
        eq,
        comp: comp.filter(|c| !c.bypass).copied(),
        sat: sat.filter(|s| !s.bypass).copied(),
    }
}

/// What a channel's built-in EQ, compressor and saturator are set to; the lane's `Strip` (or the
/// engine's, for the master) runs them.
#[derive(Default)]
struct StripDsp {
    eq: Option<Eq>,
    made: Option<Arc<Made>>,
    comp: Option<Comp>,
    sat: Option<Saturator>,
}

impl Strip {
    /// Runs a block through the strip as `dsp` has it; returns the compressor's gain reduction,
    /// in dB. The EQ window, if it watches lane `slot`, hears what its EQ does.
    #[allow(clippy::too_many_arguments)]
    fn run(
        &mut self,
        dsp: &StripDsp,
        l: &mut [f32],
        r: &mut [f32],
        shared: Option<&Shared>,
        slot: usize,
        sidechain: Option<[&[f32]; 2]>,
    ) -> f32 {
        let tap = shared
            .map(|s| &s.eq_tap)
            .filter(|t| dsp.eq.is_some() && t.watches(slot));
        self.process(
            dsp.eq.as_ref(),
            dsp.made.as_deref(),
            dsp.comp.as_ref(),
            dsp.sat.as_ref(),
            l,
            r,
            sidechain,
            tap,
        )
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
    /// A stack's shared modulators, the lane each follower hears, and their settings'
    /// automation (from the first track in it with a lane for one).
    mods: Vec<ModKind>,
    follow: [usize; MAX_MODS],
    mod_auto: Vec<(u8, ModParam, Vec<AutoPoint>)>,
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
    /// The EQ's bands' automation, in samples: Hz, dB, Q.
    eq_auto: Vec<(u8, EqField, Vec<AutoPoint>)>,
    /// Its modulators' settings' automation, in samples: (modulator, setting, points).
    mod_auto: Vec<(u8, ModParam, Vec<AutoPoint>)>,
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
    sampler: Option<Arc<crate::song::project::Sampler>>,
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
    // Each channel's plugins' latency, and its EQ's.
    let eq_latency = |eq: &Option<Eq>| eq.as_ref().map_or(0, crate::dsp::eq::latency);
    let latencies = |l: &[(usize, u32)], eqs: &mut dyn Iterator<Item = &Option<Eq>>| {
        l.iter()
            .zip(eqs)
            .map(|(&(_, l), eq)| l + eq_latency(eq))
            .collect::<Vec<_>>()
    };
    let pdc = compensate(
        &latencies(tracks, &mut project.tracks.iter().map(|t| &t.eq)),
        &latencies(auxes, &mut project.auxes.iter().map(|a| &a.eq)),
    );
    let any_solo = project.tracks.iter().any(|t| t.solo);
    let mut plans = Vec::new();
    // Which track (by its place) each plugin hears as a sidechain, by the source's id.
    let source = |r: &PluginRef| {
        let id = r.sidechain?;
        project.tracks.iter().position(|t| t.id == id)
    };
    let mut feeds = vec![false; project.tracks.len()];
    // Each channel's: its inserts', then its EQ's (for its dynamic bands), last.
    let eq_source = |eq: &Option<Eq>| {
        let id = eq.as_ref()?.sidechain?;
        project.tracks.iter().position(|t| t.id == id)
    };
    let mut with_eq = |inserts: &[PluginRef], eq: &Option<Eq>| -> Vec<Option<usize>> {
        let mut found: Vec<_> = inserts.iter().map(source).collect();
        found.push(eq_source(eq));
        for &s in found.iter().flatten() {
            feeds[s] = true;
        }
        found
    };
    let track_sidechains: Vec<_> = project
        .tracks
        .iter()
        .map(|t| with_eq(&t.inserts, &t.eq))
        .collect();
    let aux_sidechains: Vec<_> = project
        .auxes
        .iter()
        .map(|a| with_eq(&a.inserts, &a.eq))
        .collect();
    let master_sidechains = with_eq(&project.master_inserts, &project.master_eq);
    // The lane a follower hears.
    let follows = |kind: Option<&ModKind>| match kind {
        Some(ModKind::Follower { source, .. }) => project
            .tracks
            .iter()
            .position(|o| o.id == *source)
            .map_or(usize::MAX, |j| tracks[j].0),
        _ => usize::MAX,
    };
    for (i, (t, &(slot, _))) in project.tracks.iter().zip(tracks).enumerate() {
        let (mut clips, mut events, mut controls) = (Vec::new(), Vec::new(), Vec::new());
        // Its modulators as patterns' sounds hear them: shared ones as the stack's.
        let ahead = project.mods_of(i);
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
                    &ahead,
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
                    &ahead,
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
                                let playing = |s: &&crate::song::project::AuxSend| {
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
            follow: std::array::from_fn(|k| follows(t.modulators.get(k).map(|m| &m.kind))),
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
            eq_auto: auto
                .map(|a| &a.eq[..])
                .unwrap_or_default()
                .iter()
                .filter(|a| !a.points.is_empty())
                .map(|a| (a.band, a.field, in_samples(&a.points)))
                .collect(),
            mod_auto: auto
                .map(|a| &a.mods[..])
                .unwrap_or_default()
                .iter()
                .filter(|a| !a.points.is_empty())
                .map(|a| (a.modulator, a.field, in_samples(&a.points)))
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
            dsp: strip_dsp(t.eq.as_ref(), t.comp.as_ref(), t.sat.as_ref(), rate),
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
            let mods: Vec<ModKind> = match a.stack {
                true => a
                    .modulators
                    .iter()
                    .take(MAX_MODS)
                    .map(|m| m.kind.clone())
                    .collect(),
                false => Vec::new(),
            };
            // A shared modulator's settings as a track in it automates them.
            let mut mod_auto: Vec<(u8, ModParam, Vec<AutoPoint>)> = Vec::new();
            let members = project.tracks.iter().filter(|t| {
                !mods.is_empty() && t.output == Some(a.bus) && t.automation.mode != AutoMode::Off
            });
            for t in members {
                for m in t.automation.mods.iter().filter(|m| !m.points.is_empty()) {
                    let Some(ModKind::Shared { modulator: s }) =
                        t.modulators.get(m.modulator as usize).map(|m| &m.kind)
                    else {
                        continue;
                    };
                    if !mod_auto.iter().any(|(k, f, _)| k == s && *f == m.field) {
                        let points = m
                            .points
                            .iter()
                            .map(|p| AutoPoint {
                                tick: project.samples(p.tick, rate),
                                ..*p
                            })
                            .collect();
                        mod_auto.push((*s, m.field, points));
                    }
                }
            }
            AuxPlan {
                slot,
                follow: std::array::from_fn(|k| follows(mods.get(k))),
                mods,
                mod_auto,
                gain: balance(g, a.pan),
                dsp: strip_dsp(a.eq.as_ref(), a.comp.as_ref(), a.sat.as_ref(), rate),
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
        stack_mods: vec![[0.0; MAX_MODS]; auxes.len()],
        buses: (0..auxes.len())
            .map(|_| [vec![0.0; MAX_BLOCK], vec![0.0; MAX_BLOCK]])
            .collect(),
        auxes,
        // The master's EQ holds back everything heard: the engine reads that much further ahead.
        lookahead: pdc.lookahead + eq_latency(&project.master_eq) as u64,
        master_strip: strip_dsp(
            project.master_eq.as_ref(),
            project.master_comp.as_ref(),
            project.master_sat.as_ref(),
            rate,
        ),
        cycle: project
            .cycle
            .filter(|c| !c.off && c.end > c.start)
            .map(|c| (project.samples(c.start, rate), project.samples(c.end, rate))),
        master_lane: master,
        beat: project.beat() as f64 / crate::song::project::PPQ as f64,
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
                    std::borrow::Cow::Owned(crate::song::project::pattern_played(
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
                let moved = points.iter().map(|c| crate::song::project::ControlPoint {
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
    /// The EQ the EQ window shows: what it hears and plays, for its analyzer.
    pub eq_tap: crate::dsp::eq::Tap,
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
            eq_tap: crate::dsp::eq::Tap::new(),
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
        // Stacks' shared modulators, before the tracks they move. With no notes of their own,
        // an envelope on a stack never starts.
        for (aux, plan) in arrangement.auxes.iter().enumerate() {
            let Some(lane) = self.lanes[plan.slot].as_deref_mut() else {
                continue;
            };
            for (k, kind) in plan.mods.iter().enumerate() {
                let input = ModInput {
                    beats: playing.then(|| from as f64 * tempo / (60.0 * self.rate)),
                    seconds: frames as f64 / self.rate,
                    tempo,
                    trigger: false,
                    gate: false,
                    follow: self.peaks.get(plan.follow[k]).copied().unwrap_or(0.0),
                };
                let mut over = ModOver::default();
                for (_, field, points) in plan.mod_auto.iter().filter(|a| a.0 as usize == k) {
                    if let Some(v) = value_at(points, from as f64) {
                        over.set(*field, v);
                    }
                }
                arrangement.stack_mods[aux][k] =
                    modulate::step_over(kind, &mut lane.mods[k], &input, &over);
            }
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
                    // Its settings where their automation has them at the block's start.
                    let mut over = ModOver::default();
                    for (_, field, points) in plan.mod_auto.iter().filter(|a| a.0 as usize == k) {
                        if let Some(v) = value_at(points, from as f64) {
                            over.set(*field, v);
                        }
                    }
                    let v = match m.kind {
                        // Its stack's, as worked out this block.
                        ModKind::Shared { modulator } => plan
                            .output
                            .and_then(|aux| arrangement.stack_mods.get(aux))
                            .and_then(|v| v.get(modulator as usize))
                            .copied()
                            .unwrap_or(0.0),
                        _ => modulate::step_over(&m.kind, &mut lane.mods[k], &input, &over),
                    };
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
            // What the EQ's dynamic bands can listen to: a track it names, as a plugin would.
            let eq_sc = sidechain(&arrangement.sidechains, plan.sidechains.last(), frames);
            let automated = !plan.eq_auto.is_empty() && plan.dsp.eq.is_some();
            let reduction = match moved.strip || automated {
                false => lane.strip.run(&plan.dsp, l, r, shared, plan.slot, eq_sc),
                true => {
                    let mut dsp = StripDsp {
                        eq: plan.dsp.eq,
                        made: plan.dsp.made.clone(),
                        comp: plan.dsp.comp,
                        sat: plan.dsp.sat,
                    };
                    // The EQ's automation where the block starts, then its modulators.
                    if let Some(e) = dsp.eq.as_mut() {
                        for (band, field, points) in &plan.eq_auto {
                            let (Some(b), Some(v)) = (
                                e.bands.get_mut(*band as usize),
                                value_at(points, from as f64),
                            ) else {
                                continue;
                            };
                            match field {
                                EqField::Freq => b.freq = v.clamp(10.0, 30_000.0),
                                EqField::Gain => b.gain_db = v.clamp(-30.0, 30.0),
                                EqField::Q => b.q = v.clamp(0.025, 40.0),
                            }
                        }
                    }
                    dsp.eq.iter_mut().for_each(|e| moved.eq(e));
                    dsp.comp.iter_mut().for_each(|c| moved.comp(c));
                    dsp.sat.iter_mut().for_each(|s| moved.sat(s));
                    lane.strip.run(&dsp, l, r, shared, plan.slot, eq_sc)
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
                s.eq_tap.write_ext(plan.slot, l, r);
                s.eq_tap.write_lane(plan.slot, l, r);
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
            let eq_sc = sidechain(&arrangement.sidechains, plan.sidechains.last(), frames);
            let reduction = lane.strip.run(&plan.dsp, l, r, shared, plan.slot, eq_sc);
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
                s.eq_tap.write_ext(plan.slot, l, r);
                s.eq_tap.write_lane(plan.slot, l, r);
                s.report(plan.slot, peak, reduction);
            }
        }

        let eq_sc = sidechain(
            &arrangement.sidechains,
            arrangement.master_sidechains.last(),
            frames,
        );
        let reduction = self.master_strip.run(
            &arrangement.master_strip,
            &mut out_l[..frames],
            &mut out_r[..frames],
            shared,
            MASTER,
            eq_sc,
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
        // The master's sound, for the instance list: the mix through its EQ and inserts.
        if let Some(s) = shared {
            s.eq_tap
                .write_lane(MASTER, &out_l[..frames], &out_r[..frames]);
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
mod tests;
