//! Each track's lane: its instrument or plugins, its strip, and their delay.

use super::*;

impl Strip {
    /// Runs a block through one of the strip's effects as `dsp` has it; returns the
    /// compressor's or limiter's gain reduction, in dB (0 for the others). The EQ window, if it
    /// watches lane `slot`, hears what its EQ does; live, the limiter's meter reports there.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run(
        &mut self,
        fx: Fx,
        dsp: &StripDsp,
        l: &mut [f32],
        r: &mut [f32],
        shared: Option<&Shared>,
        slot: usize,
        sidechain: Option<[&[f32]; 2]>,
    ) -> f32 {
        match fx {
            Fx::Eq => {
                let tap = shared
                    .map(|s| &s.eq_tap)
                    .filter(|t| dsp.eq.is_some() && t.watches_eq(slot, None));
                self.eq(dsp.eq.as_ref(), dsp.made.as_deref(), l, r, sidechain, tap);
                0.0
            }
            Fx::Comp => {
                let most = self.comp(dsp.comp.as_ref(), l, r);
                if let Some(s) = shared.filter(|s| dsp.comp.is_some() && s.meters_fx(None)) {
                    s.report_comp(slot, self.comp_levels);
                }
                most
            }
            Fx::Sat => {
                self.sat(dsp.sat.as_ref(), l, r);
                0.0
            }
            Fx::Limit => {
                let metered = shared.filter(|s| s.meters_fx(None));
                if metered.is_some_and(|s| s.take_loudness_reset(slot)) {
                    self.limit.meter.reset();
                }
                let most = self.limit(dsp.limit.as_ref(), l, r, shared.is_some());
                if let Some(s) = metered.filter(|_| dsp.limit.is_some()) {
                    s.report_limit(slot, &self.limit);
                }
                most
            }
            // A plugin, or another built-in: the lane runs those.
            _ => 0.0,
        }
    }

    /// Runs a block through the one built-in `dsp` has: the `more`th of lane `slot`'s other
    /// built-ins, stacked. Its window, if open, hears it (the EQ's analyzer, the compressor's
    /// and limiter's meters). Returns its gain reduction in dB, if it's a compressor or
    /// limiter. Allocation-free.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run_alone(
        &mut self,
        dsp: &StripDsp,
        l: &mut [f32],
        r: &mut [f32],
        sidechain: Option<[&[f32]; 2]>,
        shared: Option<&Shared>,
        (slot, more): (usize, usize),
        clock: Clock,
    ) -> f32 {
        let unit = self.unit(dsp.unit.as_ref(), clock, l, r);
        let tap = shared
            .map(|s| &s.eq_tap)
            .filter(|t| dsp.eq.is_some() && t.watches_eq(slot, Some(more)));
        self.eq(dsp.eq.as_ref(), dsp.made.as_deref(), l, r, sidechain, tap);
        let metered = shared.filter(|s| s.meters_fx(Some(more)));
        let comp = self.comp(dsp.comp.as_ref(), l, r);
        if let Some(s) = metered.filter(|_| dsp.comp.is_some() || dsp.unit.is_some()) {
            s.report_comp(slot, self.comp_levels);
        }
        self.sat(dsp.sat.as_ref(), l, r);
        if metered.is_some_and(|s| dsp.limit.is_some() && s.take_loudness_reset(slot)) {
            self.limit.meter.reset();
        }
        let limit = self.limit(dsp.limit.as_ref(), l, r, shared.is_some());
        if let Some(s) = metered.filter(|_| dsp.limit.is_some()) {
            s.report_limit(slot, &self.limit);
        }
        if let Some(s) = metered {
            s.write_scope(l, r);
            s.write_reading(self.reading());
        }
        comp.max(limit).max(unit)
    }
}

/// A stereo delay line, for delay compensation. It keeps its history while the delay changes.
pub(crate) struct Delay {
    pub(crate) buf: [Vec<f32>; 2],
    pub(crate) pos: usize,
}

impl Delay {
    pub(crate) fn new() -> Self {
        Delay {
            buf: [vec![0.0; MAX_DELAY], vec![0.0; MAX_DELAY]],
            pos: 0,
        }
    }

    /// Delays `l` and `r` in place by `d` samples.
    pub(crate) fn process(&mut self, l: &mut [f32], r: &mut [f32], d: usize) {
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

#[allow(clippy::large_enum_variant)] // one per track
pub(crate) enum Source {
    Audio,
    Synth(Synth),
    Drums(Box<Drums>),
    Sampler(Box<Sampler>),
    Poly(Box<crate::dsp::poly::PolySynth>),
    Plugin(Processor),
}

/// A track's instrument and inserts: what makes its sound.
pub struct Lane {
    pub(crate) source: Source,
    pub(crate) inserts: Vec<Processor>,
    /// Notes the instrument is holding, to release on stop or locate: those the song played, and
    /// those played live (a MIDI keyboard, a key clicked in the piano roll).
    pub(crate) held: Held,
    pub(crate) live: Held,
    /// Its MIDI effects' memories: what they hold and have waiting.
    pub(crate) midi_fx: Box<crate::dsp::midi_fx::MidiFxRunner>,
    /// Delay compensation: before a track's sends, and after them (or after an aux's inserts).
    pub(crate) pre: Delay,
    pub(crate) post: Delay,
    /// The Live Loops cell playing instead of the track, and the sample its loop began; and a
    /// change waiting for its sample.
    pub(crate) cell: Option<(usize, u64)>,
    pub(crate) queued: Option<(Option<usize>, u64)>,
    pub(crate) strip: Box<Strip>,
    /// Its other built-ins' running state, one each, in the order the chain takes them: made
    /// off the audio thread and handed over (`Msg::Strips`), never fewer than it has.
    pub(crate) more: Vec<Strip>,
    pub(crate) binaural: Box<Binaural>,
    /// Its modulators' memories, and what they moved its volume and pan by last block (to ramp
    /// from).
    pub(crate) mods: [ModState; MAX_MODS],
    pub(crate) mod_last: (f32, f32),
    /// What it made this block, before its fader, and what its modulators moved and its effects
    /// took off on the way: left here for the audio thread to share out.
    pub(crate) buf: Box<[[f32; MAX_BLOCK]; 2]>,
    pub(crate) moved: Offsets,
    pub(crate) reduction: f32,
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

/// The main-thread halves of a lane's plugins, each insert's latency (samples), and what
/// wouldn't load (stood in for by a plugin that passes the sound through).
#[derive(Default)]
pub struct LanePlugins {
    pub instrument: Option<Plugin>,
    pub inserts: Vec<Plugin>,
    pub latencies: Vec<u32>,
    pub problems: Vec<String>,
    /// Where its built-in sampler's voices are playing, if it has one.
    pub sampler: Option<Arc<crate::dsp::sampler::SamplerTap>>,
}

/// Loads a plugin at `rate` with `held` settings (a running one's, so nothing since the song
/// was saved is lost) or else its saved ones from `dir`. One that won't load is stood in for,
/// keeping its saved settings, and says why.
pub fn load_running(
    r: &PluginRef,
    held: Option<Vec<u8>>,
    rate: f64,
    modules: &mut Modules,
    dir: &Path,
) -> (Plugin, Processor, Option<String>) {
    // A missing settings file just means default settings.
    let state = held.or_else(|| {
        r.state
            .as_ref()
            .and_then(|f| std::fs::read(dir.join(f)).ok())
    });
    match plugin::instantiate(modules, r, rate, MAX_BLOCK, state.as_deref()) {
        Ok((plugin, processor)) => (plugin, processor, None),
        Err(e) => {
            let (plugin, processor) = plugin::missing(r, state);
            (plugin, processor, Some(e))
        }
    }
}

/// Builds a lane (a track's instrument and inserts, or an aux's inserts) at `rate`, each plugin
/// with its `held` settings if it has them (by place among the inserts) or else its saved ones
/// from `dir`. Also returns the main-thread halves of its plugins, which must live as long as
/// it, and its latency in samples. A plugin that won't load is stood in for (see
/// `LanePlugins::problems`); only samples that can't be read are an error.
pub fn make_lane(
    instrument: Option<&Instrument>,
    inserts: &[PluginRef],
    held: &mut [Option<Vec<u8>>],
    rate: f64,
    modules: &mut Modules,
    dir: &Path,
) -> Result<(Lane, LanePlugins, u32), String> {
    let mut plugins = LanePlugins::default();
    let source = match instrument {
        // A kit sounds nothing itself: its pads play its notes.
        None | Some(Instrument::Kit) => Source::Audio,
        Some(Instrument::Synth) => Source::Synth(Synth::new(rate)),
        Some(Instrument::Drums) => Source::Drums(Box::new(Drums::new(rate))),
        Some(Instrument::Sampler(s)) => {
            let sampler = Sampler::load(s, dir, rate)?;
            plugins.sampler = Some(sampler.tap());
            Source::Sampler(Box::new(sampler))
        }
        Some(Instrument::Poly(_)) => Source::Poly(Box::new(crate::dsp::poly::PolySynth::new(rate))),
        Some(Instrument::Plugin(r)) => {
            let (plugin, processor, problem) = load_running(r, None, rate, modules, dir);
            plugins.problems.extend(problem);
            plugins.instrument = Some(plugin);
            Source::Plugin(processor)
        }
    };
    let mut processors = Vec::new();
    for (i, r) in inserts.iter().enumerate() {
        let state = held.get_mut(i).and_then(Option::take);
        let (plugin, processor, problem) = load_running(r, state, rate, modules, dir);
        plugins.problems.extend(problem);
        plugins.inserts.push(plugin);
        plugins.latencies.push(processor.latency());
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
        midi_fx: Box::default(),
        pre: Delay::new(),
        post: Delay::new(),
        cell: None,
        queued: None,
        strip: Box::new(Strip::new(rate)),
        more: Vec::new(),
        binaural: Box::default(),
        mods: [ModState::default(); MAX_MODS],
        mod_last: (0.0, 0.0),
        buf: Box::new([[0.0; MAX_BLOCK]; 2]),
        moved: Offsets::default(),
        reduction: 0.0,
    };
    Ok((lane, plugins, latency))
}
