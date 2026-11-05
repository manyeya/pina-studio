//! Making and shaping sound: the built-in instruments (`synth`, `drums`, `sampler`), the
//! channel EQ (`eq`, with its linear-phase and spectral modes), modulation (`modulate`), and the
//! strip's compressor (`comp`), multiband saturator (`sat`) and limiter (`limit`). Here the
//! strip runs them.

pub mod comp;
pub mod delay;
pub mod drums;
pub mod eq;
pub mod eq_linear;
pub mod eq_spectral;
pub mod limit;
pub mod midi_fx;
pub mod modulate;
pub mod pitch;
pub mod poly;
pub mod reverb;
pub mod sampler;
pub mod sat;
pub mod synth;
pub mod tools;

use crate::{
    audio::engine::MAX_BLOCK,
    dsp::eq::{Made, Processor, Tap},
    dsp::{comp::Compressor, delay::Delaying, limit::Limiting, reverb::Reverbing, sat::Saturation},
    song::project::{Comp, Eq, Fx, Limiter, Saturator, Unit},
};

pub use crate::dsp::eq::eq_response;

/// The lowest low cut: at or under this, it's off.
pub const LOW_CUT_OFF: f32 = 20.0;

/// Where the song is, for what moves in time with it.
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    pub bpm: f64,
    /// Beats from the song's start.
    pub beats: f64,
}

/// A stacked-only built-in's state as it runs.
pub trait Running: Send {
    /// Clears its memory: one coming back on starts silent. Allocation-free.
    fn reset(&mut self);
    /// Runs a block through `u` in place; the most it turned down, in dB. Allocation-free.
    fn run(&mut self, u: &Unit, clock: Clock, l: &mut [f32], r: &mut [f32]) -> f32;
    /// What it has to say for its window: each tool its own.
    fn reading(&self) -> [f32; 3] {
        [0.0; 3]
    }
}

impl Running for Reverbing {
    fn reset(&mut self) {
        Reverbing::reset(self);
    }

    fn run(&mut self, u: &Unit, clock: Clock, l: &mut [f32], r: &mut [f32]) -> f32 {
        if let Unit::Reverb(x) = u {
            self.process(x, clock.bpm, l, r);
        }
        0.0
    }
}

impl Running for Delaying {
    fn reset(&mut self) {
        Delaying::reset(self);
    }

    fn run(&mut self, u: &Unit, clock: Clock, l: &mut [f32], r: &mut [f32]) -> f32 {
        if let Unit::Delay(x) = u {
            self.process(x, clock.bpm, l, r);
        }
        0.0
    }
}

/// A channel's built-in effects, running: the EQ, compressor, saturator and limiter, which keep
/// their memory through edits.
pub struct Strip {
    eq: Box<Processor>,
    comp: Box<Compressor>,
    sat: Box<Saturation>,
    pub(crate) limit: Box<Limiting>,
    /// A stacked-only built-in's running state: only a strip made for one has it (a reverb's
    /// lines are long).
    running: Option<Box<dyn Running>>,
    /// Which ran last block: one coming back starts afresh.
    on: [bool; 5],
    /// The compressor's gain reduction, each sample of a block.
    reduction: Vec<f32>,
    /// The loudest sample into and out of the compressor last block, when it ran.
    pub(crate) comp_levels: [f32; 2],
}

impl Strip {
    pub fn new(rate: f64) -> Self {
        Strip {
            eq: Box::new(Processor::new(rate)),
            comp: Box::new(Compressor::new(rate)),
            sat: Box::new(Saturation::new(rate)),
            limit: Box::new(Limiting::new(rate)),
            running: None,
            on: [false; 5],
            reduction: vec![0.0; MAX_BLOCK],
            comp_levels: [0.0; 2],
        }
    }

    /// A strip for one built-in of `kind`, stacked, with what it needs to run.
    pub fn for_kind(kind: Fx, rate: f64) -> Self {
        let running: Option<Box<dyn Running>> = match kind {
            Fx::Reverb => Some(Box::new(Reverbing::new(rate))),
            Fx::Delay => Some(Box::new(Delaying::new(rate))),
            Fx::Filter => Some(Box::new(tools::Filtering::new(rate))),
            Fx::Motion => Some(Box::new(tools::Moving::new(rate))),
            Fx::Shaper => Some(Box::new(tools::Shaping::new(rate))),
            Fx::Gate => Some(Box::new(tools::Gating::new(rate))),
            Fx::DeEsser => Some(Box::new(tools::DeEssing::new(rate))),
            Fx::Stereo => Some(Box::new(tools::Widening::new(rate))),
            Fx::Multiband => Some(Box::new(tools::Multibanding::new(rate))),
            Fx::Pitch => Some(Box::new(pitch::Pitching::new())),
            Fx::TapeStop => Some(Box::new(pitch::Stopping::new(rate))),
            Fx::Tune => Some(Box::new(pitch::Tuning::new(rate))),
            _ => None,
        };
        Strip {
            running,
            ..Strip::new(rate)
        }
    }

    /// What the stacked-only built-in here has to say for its window.
    pub(crate) fn reading(&self) -> [f32; 3] {
        self.running.as_ref().map_or([0.0; 3], |r| r.reading())
    }

    /// A stacked-only built-in on a block, in place; the most it turned down, in dB.
    pub fn unit(&mut self, u: Option<&Unit>, clock: Clock, l: &mut [f32], r: &mut [f32]) -> f32 {
        let n = l.len().min(r.len()).min(MAX_BLOCK);
        let (l, r) = (&mut l[..n], &mut r[..n]);
        let mut most = 0.0;
        if let (Some(x), Some(state)) = (u, self.running.as_mut()) {
            if !self.on[4] {
                state.reset();
            }
            let peak = |l: &[f32], r: &[f32]| l.iter().chain(r).fold(0f32, |m, x| m.max(x.abs()));
            let before = peak(l, r);
            most = state.run(x, clock, l, r);
            self.comp_levels = [before, peak(l, r)];
        }
        self.on[4] = u.is_some();
        most
    }

    /// Runs a block (at most `MAX_BLOCK`) through whichever of the EQ, compressor and saturator
    /// are given, in place; returns the most the compressor turned it down, in dB. `made` is
    /// what the EQ's linear phase and spectral dynamics need; `sidechain` what its dynamic bands
    /// can listen to; `tap`, when the EQ window watches this
    /// channel, gets what the EQ heard and played (and says which band to hear alone).
    /// Allocation-free.
    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        eq: Option<&Eq>,
        made: Option<&Made>,
        comp: Option<&Comp>,
        sat: Option<&Saturator>,
        l: &mut [f32],
        r: &mut [f32],
        sidechain: Option<[&[f32]; 2]>,
        tap: Option<&Tap>,
    ) -> f32 {
        self.eq(eq, made, l, r, sidechain, tap);
        let most = self.comp(comp, l, r);
        self.sat(sat, l, r);
        most
    }

    /// The EQ on a block (at most `MAX_BLOCK`), in place, as `process` runs it.
    pub fn eq(
        &mut self,
        eq: Option<&Eq>,
        made: Option<&Made>,
        l: &mut [f32],
        r: &mut [f32],
        sidechain: Option<[&[f32]; 2]>,
        tap: Option<&Tap>,
    ) {
        let n = l.len().min(r.len()).min(MAX_BLOCK);
        match eq {
            Some(e) => {
                let solo = tap.and_then(Tap::solo);
                self.eq
                    .process(e, made, solo, &mut l[..n], &mut r[..n], sidechain, tap);
            }
            None => self.eq.rest(),
        }
        self.on[0] = eq.is_some();
    }

    /// The compressor on a block, in place; the most it turned down, in dB.
    pub fn comp(&mut self, comp: Option<&Comp>, l: &mut [f32], r: &mut [f32]) -> f32 {
        let n = l.len().min(r.len()).min(MAX_BLOCK);
        let (l, r) = (&mut l[..n], &mut r[..n]);
        let mut most = 0f32;
        if let Some(c) = comp {
            if !self.on[1] {
                self.comp.reset(c);
            }
            let peak = |l: &[f32], r: &[f32]| l.iter().chain(r).fold(0f32, |m, x| m.max(x.abs()));
            let before = peak(l, r);
            let gr = &mut self.reduction[..n];
            self.comp.process(c, l, r, gr);
            most = gr.iter().fold(0.0, |m, &g| m.max(g));
            self.comp_levels = [before, peak(l, r)];
        }
        self.on[1] = comp.is_some();
        most
    }

    /// The saturator on a block, in place.
    pub fn sat(&mut self, sat: Option<&Saturator>, l: &mut [f32], r: &mut [f32]) {
        let n = l.len().min(r.len()).min(MAX_BLOCK);
        if let Some(s) = sat {
            if !self.on[2] {
                self.sat.reset(s);
            }
            self.sat.process(s, &mut l[..n], &mut r[..n]);
        }
        self.on[2] = sat.is_some();
    }

    /// The limiter on a block, in place; the most it turned down, in dB. `live`, its listening
    /// aids are heard and its meter runs.
    pub fn limit(
        &mut self,
        lim: Option<&Limiter>,
        l: &mut [f32],
        r: &mut [f32],
        live: bool,
    ) -> f32 {
        let n = l.len().min(r.len()).min(MAX_BLOCK);
        let mut most = 0.0;
        if let Some(x) = lim {
            if !self.on[3] {
                self.limit.reset(true);
            }
            most = self.limit.process(x, &mut l[..n], &mut r[..n], live);
        }
        self.on[3] = lim.is_some();
        most
    }
}

/// The saturator's curve, what a steady level `x` comes out at through its first band, mix and
/// all: for drawing it.
pub fn sat_curve(s: &Saturator, x: f32) -> f32 {
    let wet = sat::static_curve(&s.bands[0], x);
    x + (wet - x) * s.mix.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod cost;
