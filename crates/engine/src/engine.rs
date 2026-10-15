//! Plays a project. Each track has a lane: its instrument and inserts, which keep their state.
//! What the lanes play (note events, audio clips, gains) is an arrangement, compiled from the
//! project on the main thread and swapped in whole after each edit. The engine mixes a block at
//! a time, live from the audio callback or offline for a bounce.
//!
//! Live, the main thread talks to the engine only through queues (`Msg` in, `Garbage` out) and
//! reads what it reports from `Shared`. Anything the engine lets go goes back as garbage, so the
//! audio thread never frees memory.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
    },
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::{
    peaks::Peaks,
    plugin::{AUDIO_MODULE, Module, Note, Plugin, Processor},
    project::{AutoPoint, Content, Instrument, PluginRef, Project, value_at},
    synth::Synth,
};

/// The most frames one `process` call handles.
pub const MAX_BLOCK: usize = 512;
/// Most note events one lane takes in one block; more are dropped.
const BLOCK_NOTES: usize = 256;
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
    channels: Vec<Vec<f32>>,
}

/// The audio files a project uses, each loaded once, with its waveform overview.
pub struct AudioFiles {
    dir: PathBuf,
    rate: f64,
    files: HashMap<PathBuf, (Arc<AudioData>, Arc<Peaks>)>,
}

impl AudioFiles {
    pub fn new(dir: &Path, rate: f64) -> Self {
        AudioFiles {
            dir: dir.to_path_buf(),
            rate,
            files: HashMap::new(),
        }
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
pub type Modules = HashMap<PathBuf, Module>;

struct Clip {
    /// Timeline position and length, and where in the file it starts, all in samples.
    start: u64,
    len: u64,
    offset: u64,
    data: Arc<AudioData>,
}

#[derive(Clone, Copy)]
struct Event {
    at: u64,
    pitch: u8,
    /// 0 is a note off.
    velocity: f32,
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
        }
    }
}

/// An aux: it plays its bus through its inserts into the master.
struct AuxPlan {
    slot: usize,
    gain: [f32; 2],
    /// Delay compensation after its inserts.
    delay: usize,
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
    /// Automation, with ticks turned to samples; empty follows the fader.
    volume: Vec<AutoPoint>,
    pans: Vec<AutoPoint>,
    /// Aux index and level.
    sends: Vec<(usize, f32)>,
    /// Delay compensation before the sends, and on the direct path after them.
    delay: usize,
    direct: usize,
}

impl Plan {
    /// Left and right gain at a sample.
    fn gain(&self, at: u64) -> [f32; 2] {
        let volume = value_at(&self.volume, at as f64).unwrap_or(self.volume_db);
        let pan = value_at(&self.pans, at as f64).unwrap_or(self.pan);
        balance(self.audible * db(volume), pan)
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
    Plugin(Processor),
}

/// A track's instrument and inserts: what makes its sound.
pub struct Lane {
    source: Source,
    inserts: Vec<Processor>,
    /// Notes the instrument is holding, to release on stop or locate.
    held: [bool; 128],
    /// Delay compensation: before a track's sends, and after them (or after an aux's inserts).
    pre: Delay,
    post: Delay,
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
        if !modules.contains_key(&r.path) {
            modules.insert(r.path.clone(), Module::load(&r.path)?);
        }
        let module = &modules[&r.path];
        let classes = module.classes();
        let class = classes
            .iter()
            .find(|c| c.category == AUDIO_MODULE && c.name == r.class)
            .ok_or_else(|| format!("{}: no plugin named {:?}", r.path.display(), r.class))?;
        // A missing settings file just means default settings.
        let state = r
            .state
            .as_ref()
            .and_then(|f| std::fs::read(dir.join(f)).ok());
        Plugin::instantiate(module, class, rate, MAX_BLOCK, state.as_deref())
    };
    let mut plugins = LanePlugins::default();
    let source = match instrument {
        None => Source::Audio,
        Some(Instrument::Synth) => Source::Synth(Synth::new(rate)),
        Some(Instrument::Vst3(r)) => {
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
    } + processors.iter().map(Processor::latency).sum::<u32>();
    let lane = Lane {
        source,
        inserts: processors,
        held: [false; 128],
        pre: Delay::new(),
        post: Delay::new(),
    };
    Ok((lane, plugins, latency))
}

/// Compiles what `project` plays at `rate`. Track `i` is on lane `tracks[i].0`, whose plugins
/// make it `tracks[i].1` samples late; likewise auxes. Regions whose audio can't be read are left
/// out, and the reasons returned, so one missing file doesn't stop the rest playing.
pub fn compile(
    project: &Project,
    rate: f64,
    tracks: &[(usize, u32)],
    auxes: &[(usize, u32)],
    files: &mut AudioFiles,
) -> (Arrangement, Vec<String>) {
    let mut problems = Vec::new();
    let latencies = |l: &[(usize, u32)]| l.iter().map(|&(_, l)| l).collect::<Vec<_>>();
    let pdc = compensate(&latencies(tracks), &latencies(auxes));
    let any_solo = project.tracks.iter().any(|t| t.solo);
    let mut plans = Vec::new();
    for (i, (t, &(slot, _))) in project.tracks.iter().zip(tracks).enumerate() {
        let (mut clips, mut events) = (Vec::new(), Vec::new());
        for region in &t.regions {
            let start = project.samples(region.start, rate);
            let end = project.samples(region.start + region.length, rate);
            match &region.content {
                // Notes starting inside the region play, for their full length.
                Content::Midi { notes } => {
                    for n in notes
                        .iter()
                        .filter(|n| (0..region.length as i64).contains(&n.start))
                    {
                        let on = region.start + n.start as u64;
                        let velocity = n.velocity as f32 / 127.0;
                        let at = project.samples(on, rate);
                        events.push(Event {
                            at,
                            pitch: n.pitch,
                            velocity,
                        });
                        let off = project.samples(on + n.length, rate);
                        events.push(Event {
                            at: off,
                            pitch: n.pitch,
                            velocity: 0.0,
                        });
                    }
                }
                Content::Audio { file, offset } => match files.get(file) {
                    Ok(data) => clips.push(Clip {
                        start,
                        len: end - start,
                        offset: *offset,
                        data,
                    }),
                    Err(e) => problems.push(e),
                },
            }
        }
        // Offs first at the same sample, so back-to-back notes of one pitch both sound.
        events.sort_by_key(|e| (e.at, e.velocity > 0.0));
        clips.sort_by_key(|c| c.start);
        let in_samples = |points: &[AutoPoint]| {
            let at = |p: &AutoPoint| project.samples(p.tick, rate);
            points
                .iter()
                .map(|p| AutoPoint {
                    tick: at(p),
                    value: p.value,
                })
                .collect()
        };
        plans.push(Plan {
            slot,
            events,
            clips,
            audible: if t.mute || (any_solo && !t.solo) {
                0.0
            } else {
                1.0
            },
            volume_db: t.volume_db,
            pan: t.pan,
            volume: in_samples(&t.automation.volume),
            pans: in_samples(&t.automation.pan),
            sends: t
                .sends
                .iter()
                .filter_map(|s| {
                    let aux = project.auxes.iter().position(|a| a.bus == s.bus)?;
                    Some((aux, db(s.level_db)))
                })
                .collect(),
            delay: pdc.tracks[i],
            direct: pdc.direct,
        });
    }
    let auxes = project
        .auxes
        .iter()
        .zip(auxes)
        .zip(&pdc.auxes)
        .map(|((a, &(slot, _)), &delay)| {
            let g = if a.mute { 0.0 } else { db(a.volume_db) };
            AuxPlan {
                slot,
                gain: balance(g, a.pan),
                delay,
            }
        })
        .collect::<Vec<_>>();
    let arrangement = Arrangement {
        tempo: project.tempo,
        master: db(project.master_volume_db),
        plans,
        buses: (0..auxes.len())
            .map(|_| [vec![0.0; MAX_BLOCK], vec![0.0; MAX_BLOCK]])
            .collect(),
        auxes,
        lookahead: pdc.lookahead,
    };
    (arrangement, problems)
}

/// From the main thread to the live engine.
pub enum Msg {
    Arrange(Box<Arrangement>),
    Insert(usize, Box<Lane>),
    Remove(usize),
    Play,
    Stop,
    Locate(u64),
    /// Play or release a note on a lane now, as when clicking a key in the piano roll.
    Note {
        slot: usize,
        pitch: u8,
        velocity: u8,
    },
}

/// What the live engine lets go of, for the main thread to drop.
pub enum Garbage {
    Arrangement(Box<Arrangement>),
    Lane(Box<Lane>),
}

/// What the live engine reports.
pub struct Shared {
    position: AtomicU64,
    playing: AtomicBool,
    /// The loudest sample (as f32 bits) per lane since last read, and the master's last.
    meters: Vec<AtomicU32>,
    /// The lane live notes go to, or `usize::MAX` for none.
    armed: AtomicUsize,
}

impl Shared {
    pub fn new() -> Self {
        Shared {
            position: AtomicU64::new(0),
            playing: AtomicBool::new(false),
            meters: (0..=SLOTS).map(|_| AtomicU32::new(0)).collect(),
            armed: AtomicUsize::new(usize::MAX),
        }
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

    /// The peak level on a lane (or `MASTER`) since the last call.
    pub fn take_meter(&self, slot: usize) -> f32 {
        f32::from_bits(self.meters[slot].swap(0, Ordering::Relaxed))
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
    scratch: [Vec<f32>; 2],
    link: Option<Link>,
    /// Notes asked for by `Msg::Note`, for the next block.
    pending: [(usize, Note); LIVE_NOTES],
    pending_len: usize,
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
            scratch: [vec![0.0; MAX_BLOCK], vec![0.0; MAX_BLOCK]],
            link: None,
            pending: [(0, Note::default()); LIVE_NOTES],
            pending_len: 0,
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
        let mut files = AudioFiles::new(dir, rate);
        let (arrangement, problems) = compile(project, rate, &tracks, &auxes, &mut files);
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

    fn receive(&mut self) {
        let Some(link) = &mut self.link else { return };
        while let Ok(msg) = link.rx.pop() {
            match msg {
                Msg::Arrange(a) => {
                    let old = std::mem::replace(&mut self.arrangement, a);
                    trash(&mut link.garbage, Garbage::Arrangement(old));
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

    /// Writes the next `frames` (at most `MAX_BLOCK`) of the mix into `out` and, if playing, moves
    /// the playhead on. Instruments and effects run while stopped too, so tails ring out.
    /// Allocation-free: safe on the audio thread.
    pub fn process(&mut self, out: [&mut [f32]; 2], frames: usize) {
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
        let (from, to) = (
            heard + self.arrangement.lookahead,
            heard + self.arrangement.lookahead + frames as u64,
        );
        let release = std::mem::take(&mut self.release);
        let (playing, tempo) = (self.playing, self.arrangement.tempo);
        let shared = self.link.as_ref().map(|l| &*l.shared);
        let [l, r] = &mut self.scratch;
        let (l, r) = (&mut l[..frames], &mut r[..frames]);
        let arrangement = &mut *self.arrangement;
        for bus in &mut arrangement.buses {
            bus[0][..frames].fill(0.0);
            bus[1][..frames].fill(0.0);
        }

        for plan in &arrangement.plans {
            let Some(lane) = self.lanes[plan.slot].as_deref_mut() else {
                continue;
            };
            l.fill(0.0);
            r.fill(0.0);
            let mut notes = [Note::default(); BLOCK_NOTES];
            let mut n = 0;
            if release {
                for (pitch, held) in lane.held.iter_mut().enumerate().filter(|(_, h)| **h) {
                    *held = false;
                    notes[n] = Note {
                        offset: 0,
                        pitch: pitch as u8,
                        velocity: 0.0,
                    };
                    n += 1;
                }
            }
            if playing {
                let first = plan.events.partition_point(|e| e.at < from);
                for e in plan.events[first..].iter().take_while(|e| e.at < to) {
                    if n == BLOCK_NOTES {
                        break;
                    }
                    lane.held[e.pitch as usize] = e.velocity > 0.0;
                    notes[n] = Note {
                        offset: (e.at - from) as u32,
                        pitch: e.pitch,
                        velocity: e.velocity,
                    };
                    n += 1;
                }
                // ponytail: walks the lane's clips from the first every block; index them by time
                // if a track gets thousands of regions.
                for c in plan.clips.iter().take_while(|c| c.start < to) {
                    let (a, b) = (from.max(c.start), to.min(c.start + c.len));
                    for (ch, dst) in [&mut *l, &mut *r].into_iter().enumerate() {
                        let src = &c.data.channels[ch.min(c.data.channels.len() - 1)];
                        for t in a..b {
                            let i = (c.offset + t - c.start) as usize;
                            dst[(t - from) as usize] += src.get(i).copied().unwrap_or(0.0);
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
                if n < BLOCK_NOTES {
                    lane.held[note.pitch as usize] = note.velocity > 0.0;
                    notes[n] = *note;
                    n += 1;
                }
            }
            let notes = &notes[..n];
            match &mut lane.source {
                Source::Audio => {}
                Source::Synth(s) => s.process([&mut *l, &mut *r], frames, notes),
                Source::Plugin(p) => {
                    p.set_transport(playing, from, tempo);
                    p.process([&mut *l, &mut *r], frames, notes);
                }
            }
            for p in &mut lane.inserts {
                p.set_transport(playing, from, tempo);
                p.process([&mut *l, &mut *r], frames, &[]);
            }
            lane.pre.process(l, r, plan.delay);
            // Gain ramps across the block, so automation moves smoothly.
            let later = if playing {
                heard + frames as u64
            } else {
                heard
            };
            let (g0, g1) = (plan.gain(heard), plan.gain(later));
            let step = [
                (g1[0] - g0[0]) / frames as f32,
                (g1[1] - g0[1]) / frames as f32,
            ];
            let mut peak = 0f32;
            for i in 0..frames {
                let k = i as f32;
                l[i] *= g0[0] + step[0] * k;
                r[i] *= g0[1] + step[1] * k;
                peak = peak.max(l[i].abs()).max(r[i].abs());
            }
            // Post-fader sends.
            for &(aux, level) in &plan.sends {
                let bus = &mut arrangement.buses[aux];
                for i in 0..frames {
                    bus[0][i] += l[i] * level;
                    bus[1][i] += r[i] * level;
                }
            }
            lane.post.process(l, r, plan.direct);
            for i in 0..frames {
                out_l[i] += l[i];
                out_r[i] += r[i];
            }
            if let Some(s) = shared {
                s.meters[plan.slot].fetch_max(peak.to_bits(), Ordering::Relaxed);
            }
        }

        for (aux, plan) in arrangement.auxes.iter().enumerate() {
            let Some(lane) = self.lanes[plan.slot].as_deref_mut() else {
                continue;
            };
            l.copy_from_slice(&arrangement.buses[aux][0][..frames]);
            r.copy_from_slice(&arrangement.buses[aux][1][..frames]);
            for p in &mut lane.inserts {
                p.set_transport(playing, from, tempo);
                p.process([&mut *l, &mut *r], frames, &[]);
            }
            lane.post.process(l, r, plan.delay);
            let mut peak = 0f32;
            for i in 0..frames {
                let (a, b) = (l[i] * plan.gain[0], r[i] * plan.gain[1]);
                out_l[i] += a;
                out_r[i] += b;
                peak = peak.max(a.abs()).max(b.abs());
            }
            if let Some(s) = shared {
                s.meters[plan.slot].fetch_max(peak.to_bits(), Ordering::Relaxed);
            }
        }

        let mut peak = 0f32;
        for s in out_l[..frames].iter_mut().chain(out_r[..frames].iter_mut()) {
            *s *= arrangement.master;
            peak = peak.max(s.abs());
        }
        if playing {
            self.position = heard + frames as u64;
        }
        if let Some(s) = shared {
            s.meters[MASTER].fetch_max(peak.to_bits(), Ordering::Relaxed);
            s.position.store(self.position, Ordering::Relaxed);
            s.playing.store(self.playing, Ordering::Relaxed);
        }
    }

    /// Plays `frames` from the start into memory.
    pub fn render(&mut self, frames: u64) -> [Vec<f32>; 2] {
        let mut out = [vec![0f32; frames as usize], vec![0f32; frames as usize]];
        self.locate(0);
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

    /// Plays the engine through the default output device, set to the engine's rate.
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

// ponytail: WAV only, at the engine's rate; add symphonia and rubato for AIFF/MP3 and resampling.
fn load_wav(path: &Path, rate: f64) -> Result<AudioData, String> {
    let err = |e: hound::Error| format!("{}: {e}", path.display());
    let mut wav = hound::WavReader::open(path).map_err(err)?;
    let spec = wav.spec();
    if spec.sample_rate as f64 != rate {
        return Err(format!(
            "{}: {} Hz, but the project runs at {rate} Hz",
            path.display(),
            spec.sample_rate
        ));
    }
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
    let channels = (0..n.min(2))
        .map(|c| samples.iter().skip(c).step_by(n).copied().collect())
        .collect();
    Ok(AudioData { channels })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{AuxSend, Note as MidiNote, PPQ, Region, Track};

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
                content: Content::Midi { notes },
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
    fn volume_automation_overrides_the_fader() {
        // A held note, faded from 0 dB to silence over the first beat.
        let mut t = synth_track("a", vec![note(0, PPQ * 4, 69)], PPQ * 4);
        t.volume_db = -60.0;
        t.automation.volume = vec![
            AutoPoint {
                tick: 0,
                value: 0.0,
            },
            AutoPoint {
                tick: PPQ,
                value: -144.0,
            },
        ];
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
        let (arrangement, _) = compile(&p, RATE, &[(0, 100), (1, 0)], &[], &mut files);
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
            },
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
    fn a_missing_audio_file_leaves_out_its_region_only() {
        let region = Region {
            start: 0,
            length: PPQ,
            content: Content::Audio {
                file: "gone.wav".into(),
                offset: 0,
            },
        };
        let audio = Track {
            name: "audio".into(),
            regions: vec![region],
            ..Default::default()
        };
        let synth = synth_track("a", vec![note(0, PPQ, 69)], PPQ);
        let p = project(vec![audio, synth]);
        let mut files = AudioFiles::new(Path::new("."), RATE);
        let (arrangement, problems) = compile(&p, RATE, &[(0, 0), (1, 0)], &[], &mut files);
        assert_eq!(problems.len(), 1);
        assert!(arrangement.plans[0].clips.is_empty());
        assert_eq!(arrangement.plans[1].events.len(), 2);
        assert!(Engine::load(&p, Path::new("."), RATE).is_err()); // a bounce does fail
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
        let (arrangement, _) = compile(&p, RATE, &[(7, 0)], &[], &mut files);
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
        assert!(shared.take_meter(7) > 0.0 && shared.take_meter(MASTER) > 0.0);
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
}
