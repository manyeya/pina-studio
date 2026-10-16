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
    time::Instant,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::{
    drums::Drums,
    dsp::Strip,
    peaks::Peaks,
    plugin::{AUDIO_MODULE, Module, Note, Plugin, Processor},
    project::{
        AutoMode, AutoPoint, Comp, Content, Eq, Instrument, PluginRef, Project, Region, Saturator,
        Which, comp_segments, value_at,
    },
    sampler::Sampler,
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
    pub(crate) channels: Vec<Vec<f32>>,
}

impl AudioData {
    #[cfg(test)]
    pub(crate) fn mono(samples: Vec<f32>) -> Self {
        AudioData {
            channels: vec![samples],
        }
    }
}

/// The audio files a project uses, each loaded once, with its waveform overview.
pub struct AudioFiles {
    dir: PathBuf,
    rate: f64,
    files: HashMap<PathBuf, (Arc<AudioData>, Arc<Peaks>)>,
    /// Flexed copies, by file and stretch.
    stretched: HashMap<(PathBuf, u64), Arc<AudioData>>,
}

impl AudioFiles {
    pub fn new(dir: &Path, rate: f64) -> Self {
        AudioFiles {
            dir: dir.to_path_buf(),
            rate,
            files: HashMap::new(),
            stretched: HashMap::new(),
        }
    }

    /// A file stretched in time, its pitch kept; computed once per stretch.
    fn stretched(&mut self, file: &Path, stretch: f64) -> Result<Arc<AudioData>, String> {
        if stretch == 1.0 {
            return self.get(file);
        }
        let key = (file.to_path_buf(), stretch.to_bits());
        if let Some(data) = self.stretched.get(&key) {
            return Ok(data.clone());
        }
        let data = Arc::new(time_stretch(&*self.get(file)?, stretch, self.rate));
        self.stretched.insert(key, data.clone());
        Ok(data)
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
    /// The master's built-in EQ and compressor.
    master_strip: StripDsp,
    /// Where playback loops, in samples, while the cycle is on.
    cycle: Option<(u64, u64)>,
    /// The lane holding the master's plugins.
    master_lane: Option<usize>,
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
fn gather(
    events: &[Event],
    clips: &[Clip],
    (from, to): (u64, u64),
    into: usize,
    notes: &mut [Note; BLOCK_NOTES],
    n: &mut usize,
    held: &mut Held,
    [l, r]: [&mut [f32]; 2],
) {
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
            for t in a..b {
                let i = (c.offset + t - c.start) as usize;
                dst[(t - from) as usize + into] += src.get(i).copied().unwrap_or(0.0);
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
    /// Plugin parameters' automation, in samples, 0 to 1.
    params: Vec<(Which, u32, Vec<AutoPoint>)>,
    /// Aux index, level, and whether it's taken before the fader.
    sends: Vec<(usize, f32, bool)>,
    /// Live Loops cells, in the track's order.
    cells: Vec<CellPlan>,
    /// The aux it plays into instead of the master.
    output: Option<usize>,
    /// Delay compensation before the sends, and on the direct path after them.
    delay: usize,
    direct: usize,
    dsp: StripDsp,
}

impl Plan {
    /// Moves a plugin's automated parameters across a block, `from` to `to` (samples read).
    fn automate(&self, which: Which, p: &mut Processor, (from, to): (u64, u64)) {
        for (_, id, points) in self.params.iter().filter(|a| a.0 == which) {
            let at = |s: u64| value_at(points, s as f64).unwrap_or(0.0) as f64;
            p.automate(*id, at(from), at(to.saturating_sub(1)));
        }
    }

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
        Some(Instrument::Drums) => Source::Drums(Box::new(Drums::new(rate))),
        Some(Instrument::Sampler(s)) => {
            Source::Sampler(Box::new(Sampler::load(&s.zones, dir, rate)?))
        }
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
        held: Held::new(),
        live: Held::new(),
        pre: Delay::new(),
        post: Delay::new(),
        cell: None,
        queued: None,
        strip: Box::new(Strip::new(rate)),
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
    let mut problems = Vec::new();
    let latencies = |l: &[(usize, u32)]| l.iter().map(|&(_, l)| l).collect::<Vec<_>>();
    let pdc = compensate(&latencies(tracks), &latencies(auxes));
    let any_solo = project.tracks.iter().any(|t| t.solo);
    let mut plans = Vec::new();
    for (i, (t, &(slot, _))) in project.tracks.iter().zip(tracks).enumerate() {
        let (mut clips, mut events) = (Vec::new(), Vec::new());
        for (r, region) in t.regions.iter().enumerate() {
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
                    files,
                    &mut events,
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
                let (mut events, mut clips) = (Vec::new(), Vec::new());
                play_region(
                    project,
                    rate,
                    &region,
                    files,
                    &mut events,
                    &mut clips,
                    &mut problems,
                );
                let length = project.samples(region.length, rate).max(1);
                for e in &mut events {
                    e.at = e.at.min(length - 1);
                }
                events.sort_by_key(|e| (e.at, e.velocity > 0.0));
                CellPlan {
                    events,
                    clips,
                    length,
                }
            })
            .collect();
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
        // With its automation off, a track follows its faders and knobs.
        let auto = (t.automation.mode != AutoMode::Off).then_some(&t.automation);
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
            volume: auto.map_or_else(Vec::new, |a| in_samples(&a.volume)),
            pans: auto.map_or_else(Vec::new, |a| in_samples(&a.pan)),
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
                dsp: strip_dsp(a.eq.as_ref(), a.comp.as_ref(), a.sat.as_ref()),
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
    (arrangement, problems)
}

/// Adds the note events and audio clips a region plays, in samples.
fn play_region(
    project: &Project,
    rate: f64,
    region: &Region,
    files: &mut AudioFiles,
    events: &mut Vec<Event>,
    clips: &mut Vec<Clip>,
    problems: &mut Vec<String>,
) {
    // Plays `content`, which begins `at` ticks into the region, from `from` to `to` (ticks
    // into the region).
    let mut emit = |content: &Content, at: i64, from: u64, to: u64| match content {
        // Notes starting in their part of the region play, for their full length.
        Content::Midi { .. } | Content::Pattern { .. } => {
            let notes = content.notes((to as i64 - at).max(0) as u64).unwrap();
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
        }
        Content::Audio {
            file,
            offset,
            stretch,
        } => {
            let begin = (from as i64).max(at);
            if begin >= to as i64 {
                return;
            }
            // In the stretched audio, the offset is stretched too.
            let offset = (*offset as f64 * stretch) as u64;
            let skip = project.samples((begin - at) as u64, rate);
            let start = project.samples(region.start + begin as u64, rate);
            let end = project.samples(region.start + to, rate);
            match files.stretched(file, *stretch) {
                Ok(data) => clips.push(Clip {
                    start,
                    len: end - start,
                    offset: offset + skip,
                    data,
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
        }
    }

    /// Records a block's peaks and gain reduction on a lane (or `MASTER`).
    fn report(&self, slot: usize, peak: [f32; 2], reduction: f32) {
        self.meters[slot * 2].fetch_max(peak[0].to_bits(), Ordering::Relaxed);
        self.meters[slot * 2 + 1].fetch_max(peak[1].to_bits(), Ordering::Relaxed);
        self.reduction[slot].fetch_max(reduction.to_bits(), Ordering::Relaxed);
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
            // The parameters the project sets, as the player gives them live.
            let instrument_ref = match instrument {
                Some(Instrument::Vst3(r)) => Some(r),
                _ => None,
            };
            let refs = instrument_ref.into_iter().zip(&p.instrument);
            for (r, plugin) in refs.chain(inserts.iter().zip(&p.inserts)) {
                for (&id, &v) in &r.params {
                    plugin.set_param(id, v);
                }
            }
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

        for plan in &arrangement.plans {
            let Some(lane) = self.lanes[plan.slot].as_deref_mut() else {
                continue;
            };
            l.fill(0.0);
            r.fill(0.0);
            let mut notes = [Note::default(); BLOCK_NOTES];
            let mut n = 0;
            if release {
                lane.held.release(0, |_| false, &mut notes, &mut n);
                lane.live.release(0, |_| false, &mut notes, &mut n);
            } else if rechase && lane.cell.is_none() {
                // An edit while playing: what the song still has sounding now rings on.
                let still = |p: u8| sounding(&plan.events, from, p);
                lane.held.release(0, still, &mut notes, &mut n);
            }
            // A launched cell takes over when its sample comes, held notes let go.
            if let Some((cell, at)) = lane.queued
                && at < to
            {
                lane.cell = cell.map(|c| (c, at));
                lane.queued = None;
                lane.held.release(0, |_| false, &mut notes, &mut n);
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
                        let (e, c) = (&cell.events, &cell.clips);
                        gather(
                            e,
                            c,
                            (p0, p0 + first),
                            lead as usize,
                            &mut notes,
                            &mut n,
                            held,
                            into,
                        );
                        if first < span {
                            let into = [&mut *l, &mut *r];
                            let at = (lead + first) as usize;
                            gather(e, c, (0, span - first), at, &mut notes, &mut n, held, into);
                        }
                    }
                    None => {
                        let (e, c, held) = (&plan.events, &plan.clips, &mut lane.held);
                        match split {
                            Some((left, start)) => {
                                let end = (from, from + left as u64);
                                gather(e, c, end, 0, &mut notes, &mut n, held, into);
                                // Round to the cycle's start: what's held lets go there.
                                held.release(left as u32, |_| false, &mut notes, &mut n);
                                let into = [&mut *l, &mut *r];
                                let rest = (start, start + (frames - left) as u64);
                                gather(e, c, rest, left, &mut notes, &mut n, held, into);
                            }
                            None => gather(e, c, (from, to), 0, &mut notes, &mut n, held, into),
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
                    lane.live.apply(note.pitch, note.velocity > 0.0);
                    notes[n] = *note;
                    n += 1;
                }
            }
            let notes = &notes[..n];
            match &mut lane.source {
                Source::Audio => {}
                Source::Synth(s) => s.process([&mut *l, &mut *r], frames, notes),
                Source::Drums(d) => d.process([&mut *l, &mut *r], frames, notes),
                Source::Sampler(s) => s.process([&mut *l, &mut *r], frames, notes),
                Source::Plugin(p) => {
                    plan.automate(Which::Instrument, p, (from, to));
                    p.set_transport(playing, from, tempo);
                    p.process([&mut *l, &mut *r], frames, notes);
                }
            }
            let reduction = lane.strip.run(&plan.dsp, l, r);
            for (i, p) in lane.inserts.iter_mut().enumerate() {
                plan.automate(Which::Insert(i), p, (from, to));
                p.set_transport(playing, from, tempo);
                p.process([&mut *l, &mut *r], frames, &[]);
            }
            lane.pre.process(l, r, plan.delay);
            // Pre-fader sends: muting still silences them.
            for &(aux, level, _) in plan.sends.iter().filter(|s| s.2) {
                let bus = &mut arrangement.buses[aux];
                let level = level * plan.audible;
                for i in 0..frames {
                    bus[0][i] += l[i] * level;
                    bus[1][i] += r[i] * level;
                }
            }
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
            let mut peak = [0f32; 2];
            for i in 0..frames {
                let k = i as f32;
                l[i] *= g0[0] + step[0] * k;
                r[i] *= g0[1] + step[1] * k;
                peak = [peak[0].max(l[i].abs()), peak[1].max(r[i].abs())];
            }
            // Post-fader sends.
            for &(aux, level, _) in plan.sends.iter().filter(|s| !s.2) {
                let bus = &mut arrangement.buses[aux];
                for i in 0..frames {
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
        }

        for (aux, plan) in arrangement.auxes.iter().enumerate() {
            let Some(lane) = self.lanes[plan.slot].as_deref_mut() else {
                continue;
            };
            l.copy_from_slice(&arrangement.buses[aux][0][..frames]);
            r.copy_from_slice(&arrangement.buses[aux][1][..frames]);
            let reduction = lane.strip.run(&plan.dsp, l, r);
            for p in &mut lane.inserts {
                p.set_transport(playing, from, tempo);
                p.process([&mut *l, &mut *r], frames, &[]);
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
            for p in &mut lane.inserts {
                p.set_transport(playing, from, tempo);
                p.process([&mut out_l[..frames], &mut out_r[..frames]], frames, &[]);
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
        let mut out = [vec![0f32; frames as usize], vec![0f32; frames as usize]];
        // A bounce plays the song through once, cycle or not.
        self.arrangement.cycle = None;
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

/// Stretches audio by `ratio` in time with Signalsmith Stretch, keeping its pitch.
fn time_stretch(data: &AudioData, ratio: f64, rate: f64) -> AudioData {
    let channels = data.channels.len();
    let frames = data.channels[0].len();
    let input: Vec<f32> = (0..frames)
        .flat_map(|i| data.channels.iter().map(move |c| c[i]))
        .collect();
    let out_frames = (frames as f64 * ratio).round() as usize;
    let mut output = vec![0f32; out_frames * channels];
    let mut stretch = signalsmith_stretch::Stretch::preset_default(channels as u32, rate as u32);
    stretch.exact(&input, &mut output);
    let channels = (0..channels)
        .map(|c| output.iter().skip(c).step_by(channels).copied().collect())
        .collect();
    AudioData { channels }
}

// ponytail: WAV only, at the engine's rate; add symphonia and rubato for AIFF/MP3 and resampling.
/// A WAV file's audio at `rate`: converted, if it was recorded at another.
pub(crate) fn load_wav(path: &Path, rate: f64) -> Result<AudioData, String> {
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
    let channels = (0..n.min(2))
        .map(|c| {
            let side: Vec<f32> = samples.iter().skip(c).step_by(n).copied().collect();
            resample(&side, spec.sample_rate as f64, rate)
        })
        .collect();
    Ok(AudioData { channels })
}

/// Audio at `from` Hz, at `to` Hz, band-limited: a Blackman-windowed sinc, 32 taps a side, its
/// cutoff a little under the lower Nyquist, so going down doesn't alias and going up doesn't
/// image.
// ponytail: computed straight, 64 taps an output sample; a polyphase table if loading gets slow.
pub(crate) fn resample(samples: &[f32], from: f64, to: f64) -> Vec<f32> {
    if from == to || samples.is_empty() {
        return samples.to_vec();
    }
    const TAPS: i64 = 32;
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
    let n = (samples.len() as f64 / ratio).round() as usize;
    (0..n)
        .map(|i| {
            let x = i as f64 * ratio;
            let c = x.floor() as i64;
            let mut acc = 0.0;
            for k in (c - TAPS + 1).max(0)..=(c + TAPS).min(samples.len() as i64 - 1) {
                let d = x - k as f64;
                acc += samples[k as usize] as f64
                    * cutoff
                    * sinc(cutoff * d)
                    * blackman(d / TAPS as f64);
            }
            acc as f32
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
                content: Content::Midi { notes },
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
    fn a_missing_audio_file_leaves_out_its_region_only() {
        let region = Region {
            start: 0,
            length: PPQ,
            content: Content::Audio {
                file: "gone.wav".into(),
                offset: 0,
                stretch: 1.0,
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
    fn a_launched_cell_loops_instead_of_the_track() {
        use crate::project::Cell;
        // The track's own region is silent; its cell is a beat-long loop with a short note.
        let mut t = synth_track("a", vec![], PPQ * 8);
        let notes = vec![note(0, PPQ / 4, 69)];
        let region = Region {
            start: 0,
            length: PPQ,
            content: Content::Midi { notes },
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
        if let Content::Midi { notes } = &mut edited.tracks[0].regions[0].content {
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
    fn region_playback_settings_change_what_plays_not_the_notes() {
        let mut t = synth_track("a", vec![note(100, 240, 60), note(350, 240, 64)], PPQ);
        let r = &mut t.regions[0];
        r.transpose = 12;
        r.velocity = -27;
        r.quantize = Some(240);
        let p = project(vec![t]);
        let sixteenth = BEAT as u64 / 4;
        assert_eq!(note_ons(&p), [(0, 72, 100), (sixteenth, 76, 100)]);
        let Content::Midi { notes } = &p.tracks[0].regions[0].content else {
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
        t.instrument = Some(Instrument::Vst3(plugin));
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
                points: vec![AutoPoint {
                    tick: 0,
                    value: 0.0,
                }],
            });
        let soft = peak(&render(&quiet, Path::new("."), BEAT * 2)[0][BEAT..]);
        assert!(loud > 0.01 && soft < loud / 4.0, "{loud} {soft}");
        // A value the project sets, as Smart Controls and agents do, holds in a render too.
        let mut set = p.clone();
        if let Some(Instrument::Vst3(r)) = &mut set.tracks[0].instrument {
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
