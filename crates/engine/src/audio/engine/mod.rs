//! Plays a project. Each track has a lane: its instrument and inserts, which keep their state.
//! What the lanes play (note events, audio clips, gains) is an arrangement, compiled from the
//! project on the main thread and swapped in whole after each edit. The engine mixes a block at
//! a time, live from the audio callback or offline for a bounce.
//!
//! Live, the main thread talks to the engine only through queues (`Msg` in, `Garbage` out) and
//! reads what it reports from `Shared`. Anything the engine lets go goes back as garbage, so the
//! audio thread never frees memory.

mod block;
mod decode;
mod files;
mod lanes;
mod notes;
mod output;
mod pan;
mod plan;
mod render;
mod shared;

pub use decode::*;
pub use files::*;
pub use lanes::*;
pub use notes::*;
pub use output::*;
pub use pan::*;
pub use plan::*;
pub use shared::*;

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

const RENDER_THREADS: usize = 2;

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
}

#[cfg(test)]
mod tests;
