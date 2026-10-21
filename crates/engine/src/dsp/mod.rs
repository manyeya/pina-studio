//! Making and shaping sound: the built-in instruments (`synth`, `drums`, `sampler`), the
//! channel EQ (`eq`, with its linear-phase and spectral modes), modulation (`modulate`), and here
//! the strip's soft-knee compressor and saturator. Their sound is Faust's
//! (`crates/engine/faust/*.dsp`, compiled into `faust/`); this runs them, and works out the
//! saturator's curve for the UI.

pub mod drums;
pub mod eq;
pub mod eq_linear;
pub mod eq_spectral;
pub(crate) mod faust;
pub mod modulate;
pub mod sampler;
pub mod synth;

use crate::{
    audio::engine::MAX_BLOCK,
    dsp::eq::{Made, Processor, Tap},
    dsp::faust::{FaustDsp, ParamIndex, comp::CompDsp, controls, sat::SatDsp},
    song::project::{Comp, Eq, Saturator},
};

pub use crate::dsp::eq::eq_response;

/// The lowest low cut: at or under this, it's off.
pub const LOW_CUT_OFF: f32 = 20.0;

/// Blocks of silence run through an effect that's just come on, so its smoothed controls start
/// where they're set rather than gliding there.
const PRIME_BLOCKS: usize = 8;

/// A channel's built-in effects, running: the EQ, compressor and saturator, which keep their
/// memory through edits.
pub struct Strip {
    eq: Box<Processor>,
    comp: Box<CompDsp>,
    sat: Box<SatDsp>,
    comp_controls: [ParamIndex; 6],
    sat_controls: [ParamIndex; 4],
    /// Which ran last block: one coming back starts afresh.
    on: [bool; 3],
    /// A copy of the block going in; and room for what comes out while priming, the third the
    /// compressor's gain reduction.
    input: [Vec<f32>; 2],
    out: [Vec<f32>; 3],
}

impl Strip {
    pub fn new(rate: f64) -> Self {
        let eq = Box::new(Processor::new(rate));
        let rate = rate as i32;
        let mut comp = Box::new(CompDsp::new());
        let mut sat = Box::new(SatDsp::new());
        comp.init(rate);
        sat.init(rate);
        let comp_controls = controls(
            &*comp,
            ["threshold", "ratio", "attack", "release", "makeup", "rms"],
        );
        let sat_controls = controls(&*sat, ["drive", "tone", "mix", "output"]);
        let block = || vec![0.0; MAX_BLOCK];
        Strip {
            eq,
            comp,
            sat,
            comp_controls,
            sat_controls,
            on: [false; 3],
            input: [block(), block()],
            out: [block(), block(), block()],
        }
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
        let n = l.len().min(MAX_BLOCK);
        let (l, r) = (&mut l[..n], &mut r[..n]);
        match eq {
            Some(e) => {
                let solo = tap.and_then(Tap::solo);
                self.eq.process(e, made, solo, l, r, sidechain, tap);
            }
            None => self.eq.rest(),
        }
        let mut most = 0f32;
        if let Some(c) = comp {
            let values = [
                c.threshold_db,
                c.ratio,
                c.attack_ms,
                c.release_ms,
                c.makeup_db,
                c.rms as u8 as f32,
            ];
            set(&mut *self.comp, &self.comp_controls, &values);
            if !self.on[1] {
                prime(&mut *self.comp, &mut self.input, &mut self.out);
            }
            self.input[0][..n].copy_from_slice(l);
            self.input[1][..n].copy_from_slice(r);
            let [il, ir] = &self.input;
            let gr = &mut self.out[2][..n];
            self.comp
                .compute(n, &[&il[..n], &ir[..n]], &mut [&mut *l, &mut *r, &mut *gr]);
            most = gr.iter().fold(0.0, |m, &g| m.max(g));
        }
        if let Some(s) = sat {
            let values = [s.drive_db, s.tone, s.mix, s.output_db];
            set(&mut *self.sat, &self.sat_controls, &values);
            if !self.on[2] {
                prime(&mut *self.sat, &mut self.input, &mut self.out);
            }
            self.input[0][..n].copy_from_slice(l);
            self.input[1][..n].copy_from_slice(r);
            let [il, ir] = &self.input;
            self.sat.compute(n, &[&il[..n], &ir[..n]], &mut [l, r]);
        }
        self.on = [eq.is_some(), comp.is_some(), sat.is_some()];
        most
    }
}

fn set(dsp: &mut dyn FaustDsp<T = f32>, controls: &[ParamIndex], values: &[f32]) {
    for (&c, &v) in controls.iter().zip(values) {
        dsp.set_param(c, v);
    }
}

/// Clears an effect that's just come on and runs silence through it, its controls already set,
/// so it starts settled.
fn prime(dsp: &mut dyn FaustDsp<T = f32>, silence: &mut [Vec<f32>; 2], out: &mut [Vec<f32>; 3]) {
    dsp.instance_clear();
    let outputs = dsp.get_num_outputs() as usize;
    for _ in 0..PRIME_BLOCKS {
        silence[0].fill(0.0);
        silence[1].fill(0.0);
        let [a, b, c] = out;
        let outs: &mut [&mut [f32]] = &mut [&mut a[..], &mut b[..], &mut c[..]];
        dsp.compute(
            MAX_BLOCK as i32,
            &[&silence[0][..], &silence[1][..]],
            &mut outs[..outputs],
        );
    }
}

/// The saturator's curve, what a steady level `x` comes out at, mix and all: for drawing it.
/// (The Faust code averages the same curve between samples, at twice the rate.)
pub fn sat_curve(s: &Saturator, x: f32) -> f32 {
    const BIAS: f32 = 0.15;
    let drive = 10f32.powf(s.drive_db.clamp(0.0, 24.0) / 20.0);
    let b = BIAS.tanh();
    let wet = ((drive * x + BIAS).tanh() - b) / (drive * (1.0 - b * b));
    x + (wet - x) * s.mix.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod cost;
