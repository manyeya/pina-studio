//! Each track's lane: its instrument or plugins, its strip, and their delay.

use super::*;

impl Strip {
    /// Runs a block through the strip as `dsp` has it; returns the compressor's gain reduction,
    /// in dB. The EQ window, if it watches lane `slot`, hears what its EQ does.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run(
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
    /// Delay compensation: before a track's sends, and after them (or after an aux's inserts).
    pub(crate) pre: Delay,
    pub(crate) post: Delay,
    /// The Live Loops cell playing instead of the track, and the sample its loop began; and a
    /// change waiting for its sample.
    pub(crate) cell: Option<(usize, u64)>,
    pub(crate) queued: Option<(Option<usize>, u64)>,
    pub(crate) strip: Box<Strip>,
    pub(crate) binaural: Box<Binaural>,
    /// Its modulators' memories, and what they moved its volume and pan by last block (to ramp
    /// from).
    pub(crate) mods: [ModState; MAX_MODS],
    pub(crate) mod_last: (f32, f32),
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
