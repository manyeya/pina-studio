//! Bloom, running. Each voice plays one branch: its genes worked out from the seed as far as the
//! branch reaches (its length, the plant's growth, the mod wheel, the note's own slide), and
//! worked out again whenever any of those move, so a branch grows under a held note.
//!
//! A voice: wavetable oscillators A (three times over for unison) and B, B frequency-modulating
//! A, A feeding back on itself, a ring between them, a sub and coloured noise; folded, driven
//! into a state-variable filter swept from low- through band- to high-pass, a resonator tuned
//! to the note, crushed, tilted, panned. Two envelopes (the level's and a contour that bends
//! pitch, filter and FM) and an LFO move it. Every voice sends to the plant's echo and space.
//!
//! Settings are worked out every `STEP` samples and the level ramps between, voices run several
//! times over the song's rate when asked (FM aliases otherwise), and nothing allocates once the
//! synth is made.

pub mod grow;

use std::{
    f32::consts::{FRAC_PI_2, LN_2, PI, TAU},
    sync::Arc,
};

use super::{
    delay::Delaying,
    poly::{
        Down, MAX_FRAMES,
        table::{SIZE, Table, Wave, table, wave},
    },
    reverb::Reverbing,
};
use crate::{
    plugin::{Control, ControlKind, Note},
    song::project::*,
};

const MAX_VOICES: usize = 32;
/// How often settings are worked out, samples of the song.
const STEP: usize = 16;
const MOST_TIMES: usize = 4;
const CHUNK: usize = STEP * MOST_TIMES;
/// One cycle, in the whole-number phases the oscillators turn in.
const TURN: f32 = 4_294_967_296.0;
/// The resonator's longest delay, samples at the running rate (a power of two).
const COMB: usize = 8192;
/// How long the echo and space ring on after the last voice stops, seconds.
const FX_TAIL_S: f32 = 12.0;

/// What `BloomSynth::live` gives: each branch's level, then how far it reaches, then how many
/// voices sound and the plant's growth now.
pub const BLOOM_LIVE: usize = 2 * BRANCHES + 2;

/// A plant ready to play, made off the audio thread: its wavetables built.
pub struct BloomPlan {
    pub settings: Bloom,
    pos: Vec<f32>,
    tables: Vec<Arc<Table>>,
}

impl BloomPlan {
    pub fn new(b: &Bloom) -> BloomPlan {
        BloomPlan {
            settings: b.clone(),
            pos: positions(b),
            tables: BLOOM_TABLES.iter().map(|n| table(n)).collect(),
        }
    }

    /// Where a parameter's knob sits, 0 to 1.
    pub fn pos(&self, i: usize) -> f32 {
        self.pos.get(i).copied().unwrap_or(0.0)
    }
}

/// Each parameter's place on its knob, 0 to 1.
pub fn positions(b: &Bloom) -> Vec<f32> {
    BLOOM_PARAMS
        .iter()
        .zip(&b.params.0)
        .map(|(p, v)| p.scale.pos(*v))
        .collect()
}

/// A gene's amount past a still zone at the bottom: grown a little, it does nothing yet.
#[inline]
fn amount(g: f32) -> f32 {
    ((g - 0.08) / 0.92).clamp(0.0, 1.0)
}

/// The nearest of `steps` (all positive) to `x`, by octaves.
fn nearest(x: f32, steps: &[f32]) -> f32 {
    // How many times over, either way: the same order as octaves apart, without a log.
    let off = |s: f32| (x / s).max(s / x);
    let mut best = steps[0];
    for &s in steps {
        if off(s) < off(best) {
            best = s;
        }
    }
    best
}

/// A soft clip: tanh, near enough, held at ±1.
#[inline(always)]
fn soft(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

/// A triangle fold: past ±1 the wave folds back rather than clipping.
#[inline(always)]
fn fold(x: f32) -> f32 {
    let t = x * 0.25 + 0.25;
    4.0 * (t - (t + 0.5).floor()).abs() - 1.0
}

/// One sample of a frame, `q` a phase.
#[inline(always)]
fn tap(w: &Wave, q: u32) -> f32 {
    const SHIFT: u32 = 32 - SIZE.trailing_zeros();
    const PART: f32 = 1.0 / (1u32 << SHIFT) as f32;
    let i = (q >> SHIFT) as usize;
    let t = (q & ((1 << SHIFT) - 1)) as f32 * PART;
    w[i] + (w[i + 1] - w[i]) * t
}

/// Between two frames.
#[inline(always)]
fn tap2(a: &Wave, b: &Wave, mix: f32, q: u32) -> f32 {
    let x = tap(a, q);
    x + (tap(b, q) - x) * mix
}

#[inline(always)]
fn turns(cycles: f32) -> u32 {
    (cycles * TURN) as i64 as u32
}

fn chance(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed >> 8) as f32 / (1u32 << 24) as f32
}

/// An envelope, running: 0 off, 1 rising, 2 falling to its sustain, 3 let go.
#[derive(Clone, Copy, Default)]
struct Env {
    stage: u8,
    level: f32,
}

impl Env {
    fn start(&mut self) {
        self.stage = 1;
    }

    fn release(&mut self) {
        if self.stage != 0 {
            self.stage = 3;
        }
    }

    /// `ms` on; its level.
    fn step(&mut self, ms: f32, attack: f32, decay: f32, sustain: f32, release: f32) -> f32 {
        match self.stage {
            1 => {
                self.level += ms / attack.max(0.05);
                if self.level >= 1.0 {
                    (self.level, self.stage) = (1.0, 2);
                }
            }
            2 => {
                let k = (-4.6 * ms / decay.max(0.5)).exp();
                self.level = sustain + (self.level - sustain) * k;
            }
            3 => {
                self.level *= (-6.9 * ms / release.max(0.5)).exp();
                if self.level < 1e-4 {
                    (self.level, self.stage) = (0.0, 0);
                }
            }
            _ => self.level = 0.0,
        }
        self.level
    }
}

/// What a voice is set to for a step, worked out from its genes.
#[derive(Clone, Copy, Default)]
struct Set {
    inc_a: [f32; 3],
    inc_b: f32,
    inc_sub: f32,
    /// Each oscillator's mip and two frames, and how far between.
    a: (usize, usize, usize, f32),
    b: (usize, usize, usize, f32),
    tables: [usize; 2],
    fm: f32,
    feedback: f32,
    ga: f32,
    gb: f32,
    ring: f32,
    sub: f32,
    noise: f32,
    noise_k: f32,
    noise_gain: f32,
    fold: f32,
    unison: [f32; 3],
    /// Each unison voice's share of left and right.
    spread: [[f32; 2]; 3],
    drive: f32,
    makeup: f32,
    svf: [f32; 4],
    modes: [f32; 3],
    comb_fb: f32,
    comb_mix: f32,
    comb_len: f32,
    crush_steps: f32,
    crush_hold: f32,
    tilt_k: f32,
    tilt: [f32; 2],
    level: f32,
    pan: [f32; 2],
    echo: f32,
    space: f32,
}

struct Voice {
    on: bool,
    held: bool,
    note: u8,
    branch: u8,
    gain: f32,
    velocity: f32,
    age: u64,
    pitch: f32,
    target: f32,
    tune: f32,
    slide: f32,
    press: f32,
    /// How far its branch reached when its genes were last worked out (negative: not yet).
    reach: f32,
    genes: [f32; GENES],
    tables: [usize; 2],
    amp: Env,
    contour: Env,
    lfo_phase: f32,
    lfo_held: f32,
    lfo_ms: f32,
    pa: [u32; 3],
    pb: u32,
    psub: u32,
    last_a: f32,
    noise_lp: f32,
    seed: u32,
    svf: [[f32; 2]; 2],
    comb: [Vec<f32>; 2],
    comb_at: usize,
    combed: bool,
    comb_lp: [f32; 2],
    crush_count: f32,
    crushed: [f32; 2],
    tilt_lp: [f32; 2],
    set: Set,
    level_was: f32,
    stereo: bool,
}

impl Voice {
    fn new(seed: u32) -> Voice {
        Voice {
            on: false,
            held: false,
            note: 60,
            branch: 0,
            gain: 1.0,
            velocity: 0.0,
            age: 0,
            pitch: 60.0,
            target: 60.0,
            tune: 0.0,
            slide: 0.0,
            press: 0.0,
            reach: -1.0,
            genes: NEUTRAL,
            tables: [0; 2],
            amp: Env::default(),
            contour: Env::default(),
            lfo_phase: 0.0,
            lfo_held: 0.0,
            lfo_ms: 0.0,
            pa: [0; 3],
            pb: 0,
            psub: 0,
            last_a: 0.0,
            noise_lp: 0.0,
            seed: seed | 1,
            svf: [[0.0; 2]; 2],
            comb: [vec![0.0; COMB], vec![0.0; COMB]],
            comb_at: 0,
            combed: false,
            comb_lp: [0.0; 2],
            crush_count: 0.0,
            crushed: [0.0; 2],
            tilt_lp: [0.0; 2],
            set: Set::default(),
            level_was: 0.0,
            stereo: false,
        }
    }
}

/// What every voice reads for a step.
struct Shared<'a> {
    plan: &'a BloomPlan,
    vals: &'a [f32],
    /// The growth everything has on top of its branch's length, before the note's own slide.
    extra: f32,
    regrow: bool,
    rate: f32,
    ms: f32,
    tempo: f32,
    bend: f32,
    pressure: f32,
}

/// An LFO's rate in time with the song: these note lengths, beats.
const SYNCED: [f32; 11] = [
    8.0, 4.0, 2.0, 1.5, 1.0, 0.75, 0.5, 0.375, 0.25, 0.1875, 0.125,
];

impl Voice {
    fn regrow(&mut self, c: &Shared, reach: f32) {
        let s = &c.plan.settings;
        let b = self.branch as usize;
        let seed: [f32; GENES] = std::array::from_fn(|i| c.vals[gene_param(i)]);
        let g = Growing {
            reach,
            bend: c.vals[branch_bend(b)],
            wild: c.vals[BLOOM_WILD],
            atonal: c.vals[BLOOM_ATONAL],
        };
        grown(
            &seed,
            s.paths[b],
            g,
            s.locks,
            s.edits_of(b),
            &mut self.genes,
        );
        self.reach = reach;
    }

    /// The tables its genes pick, chosen as a note starts so a growing branch never clicks.
    fn latch(&mut self) {
        let pick = |g: f32| (g * BLOOM_TABLES.len() as f32).min(BLOOM_TABLES.len() as f32 - 1.0);
        self.tables = [pick(self.genes[17]) as usize, pick(self.genes[19]) as usize];
    }

    /// Moves on a step: its genes if its branch has grown, envelopes, the LFO, and what that all
    /// sets it to.
    fn control(&mut self, c: &Shared) {
        let s = &c.plan.settings;
        let b = self.branch as usize;
        let slide = self.slide * c.vals[BLOOM_SLIDE];
        let len = c.vals[branch_length(b)].clamp(0.0, 1.0);
        let grow = (c.extra + slide).clamp(0.0, 1.0);
        let reach = len + grow * (1.0 - len);
        if c.regrow || (reach - self.reach).abs() > 1e-4 {
            self.regrow(c, reach);
        }
        let g = &self.genes;
        let atonal = c.vals[BLOOM_ATONAL];
        let shape = |i: usize| GENE_TABLE[i].shape.value(g[i]);
        let ms = c.ms;
        let release = shape(3) * 4f32.powf(c.vals[BLOOM_RELEASE]);
        let env = self.amp.step(ms, shape(0), shape(1), g[2], release);
        // The contour rises and falls back to nothing, held or not.
        let contour = self.contour.step(ms, shape(4), shape(5), 0.0, shape(5));
        // The LFO, faded in.
        let rate = match s.sync {
            true => {
                let k = ((g[9] * SYNCED.len() as f32) as usize).min(SYNCED.len() - 1);
                c.tempo / 60.0 / SYNCED[k]
            }
            false => shape(9),
        };
        let was = self.lfo_phase;
        self.lfo_phase = (self.lfo_phase + rate * ms / 1000.0).fract();
        if self.lfo_phase < was {
            self.lfo_held = 2.0 * chance(&mut self.seed) - 1.0;
        }
        self.lfo_ms += ms;
        let rise = (self.lfo_ms / shape(15)).min(1.0);
        let p = self.lfo_phase;
        let waves = [
            (TAU * p).sin(),
            1.0 - 4.0 * (p - 0.5).abs(),
            soft(3.0 * (TAU * p).sin()),
            self.lfo_held,
        ];
        let at = g[10].clamp(0.0, 1.0) * 2.999;
        let (k, part) = (at as usize, at.fract());
        let lfo = (waves[k] + (waves[k + 1] - waves[k]) * part) * rise;
        // Glide, in mono.
        let glide = c.vals[BLOOM_GLIDE];
        self.pitch += (self.target - self.pitch) * (1.0 - (-ms / glide.max(0.1)).exp());

        // Pitch.
        let coarse = shape(27);
        let coarse = match atonal {
            a if a < 0.34 => (coarse / 12.0).round() * 12.0,
            a if a < 0.67 => nearest_semis(coarse, &[0.0, 5.0, 7.0, 12.0, 17.0, 19.0, 24.0]),
            _ => coarse,
        };
        let vibrato = amount(g[11]).powi(2) * (0.6 + 11.4 * atonal * atonal);
        let semis = self.pitch
            + c.vals[BLOOM_TUNE]
            + coarse
            + contour * shape(6)
            + lfo * vibrato
            + c.bend * s.voicing.bend_semitones
            + self.tune;
        let hz = 440.0 * ((semis - 69.0) / 12.0 * LN_2).exp();
        let inc = hz / c.rate;
        let set = &mut self.set;
        let unison = amount(g[28]);
        self.stereo = unison > 0.0;
        let spread = unison * unison * 30.0;
        let cents = |x: f32| (x / 1200.0 * LN_2).exp();
        set.inc_a = [inc, inc * cents(-spread), inc * cents(spread)];
        let side = (unison * 3.0).min(1.0) * 0.8;
        let norm = 1.0 / (1.0 + 2.0 * side * side).sqrt();
        set.unison = [norm, side * norm, side * norm];
        let w = g[41];
        let pan_of = |p: f32| {
            let a = (p.clamp(-1.0, 1.0) + 1.0) * FRAC_PI_2 * 0.5;
            [
                a.cos() * std::f32::consts::SQRT_2,
                a.sin() * std::f32::consts::SQRT_2,
            ]
        };
        set.spread = [pan_of(0.0), pan_of(-w), pan_of(w)];
        let ratio = shape(20);
        let ratio = match atonal < 0.5 {
            true => nearest(ratio, &[0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]),
            false => ratio,
        };
        let detune = shape(21) * (0.3 + 0.7 * atonal);
        set.inc_b = inc * ratio * cents(detune);
        set.inc_sub = inc * 0.5;
        let frames = |t: &Table, pos: f32, inc: f32| {
            let at = pos.clamp(0.0, 1.0) * (t.frames - 1) as f32;
            let f0 = (at as usize).min(t.frames - 1);
            (
                Table::mip(inc),
                f0,
                (f0 + 1).min(t.frames - 1),
                at - f0 as f32,
            )
        };
        let tables = &c.plan.tables;
        set.tables = self.tables;
        set.a = frames(&tables[self.tables[0]], g[16], inc);
        set.b = frames(&tables[self.tables[1]], g[18], set.inc_b);
        let fm = amount(g[23]);
        set.fm = 3.0 * fm * fm + 3.0 * contour * g[8] * g[8];
        set.feedback = 1.2 * amount(g[24]).powi(2);
        let bal = g[22] * FRAC_PI_2;
        (set.ga, set.gb) = (bal.cos(), bal.sin());
        set.ring = amount(g[30]);
        set.sub = 0.8 * amount(g[31]).powi(2);
        let noise = amount(g[25]);
        set.noise = 0.9 * noise * noise;
        let color = g[26];
        set.noise_k = 0.01 + 0.99 * color * color;
        set.noise_gain = 1.0 + (1.0 - color) * 2.0;
        set.fold = amount(g[29]);
        let d = g[38];
        set.drive = 1.0 + 6.0 * d * d;
        set.makeup = 1.0 / (1.0 + 2.0 * d * d).sqrt();

        // The filter.
        let key = shape(35) * (semis - 60.0) / 12.0;
        let vel = shape(36) * 3.0 * (self.velocity - 0.7);
        let press = self.press.max(c.pressure);
        let octaves = contour * shape(7) + lfo * 3.0 * amount(g[12]) + key + vel + press;
        let fc = (shape(32) * (octaves * LN_2).exp()).clamp(16.0, c.rate * 0.45);
        let res = g[33];
        let fg = (PI * fc / c.rate).tan();
        let k = 2.0 * (1.0 - 0.97 * res);
        let a1 = 1.0 / (1.0 + fg * (fg + k));
        set.svf = [k, a1, fg * a1, fg * fg * a1];
        let m = g[34];
        set.modes = [
            (1.0 - 2.0 * m).max(0.0),
            1.0 - (2.0 * m - 1.0).abs(),
            (2.0 * m - 1.0).max(0.0),
        ];
        // The resonator, at the note (an octave up at a time where its line is too short).
        let comb = amount(g[39]);
        set.comb_mix = comb;
        if comb > 0.0 {
            let mut len = c.rate / hz.max(1.0);
            while len > (COMB - 4) as f32 {
                len *= 0.5;
            }
            set.comb_len = len.max(2.0);
            set.comb_fb = 0.6 + 0.38 * comb.sqrt();
        }
        let crush = amount(g[43]);
        set.crush_steps = match crush > 0.0 {
            true => (2f32).powf(12.0 - 9.0 * crush),
            false => 0.0,
        };
        set.crush_hold = 1.0 + crush * crush * 30.0;
        let tilt = shape(42);
        set.tilt_k = 1.0 - (-TAU * 700.0 / c.rate).exp();
        let n = 1.0 / (1.0 + 0.3 * tilt.abs());
        set.tilt = [(1.0 - 0.6 * tilt) * n, (1.0 + 0.9 * tilt) * n];
        // Level, pan and what goes to the plant's effects.
        let soft_notes = 1.0 - shape(37) * (1.0 - self.velocity);
        let tremolo = 1.0 - amount(g[13]) * 0.5 * (1.0 + lfo);
        // A low-pass shut under the note loses it: brought back up some way, so a seed with a
        // dark filter is still heard (a seed is made to sound right however it's drawn).
        let under = (hz / fc).max(1.0).log2().min(3.0) * set.modes[0];
        set.level = env * soft_notes * tremolo * self.gain * 0.8 * (1.0 + 0.5 * under);
        let pan = shape(40) + lfo * amount(g[14]);
        set.pan = pan_of(pan);
        let effect = 2.0 * c.vals[BLOOM_EFFECT];
        set.echo = g[44] * g[44] * effect;
        set.space = g[45] * g[45] * effect;
    }

    /// `n` samples (at most `CHUNK`) added into the plant's buses: dry, echo and space.
    fn render(&mut self, plan: &BloomPlan, n: usize, out: &mut [[f32; CHUNK]; 2]) {
        let s = self.set;
        let ta = &plan.tables[s.tables[0]];
        let tb = &plan.tables[s.tables[1]];
        let la = ta.level(s.a.0);
        let lb = tb.level(s.b.0);
        let (wa0, wa1) = (wave(la, s.a.1), wave(la, s.a.2));
        let (wb0, wb1) = (wave(lb, s.b.1), wave(lb, s.b.2));
        let sine_level = plan.tables[0].level(Table::mip(s.inc_sub));
        let sine = wave(sine_level, 0);
        let [ia0, ia1, ia2] = s.inc_a.map(turns);
        let (ib, isub) = (turns(s.inc_b), turns(s.inc_sub));
        let chans = if self.stereo { 2 } else { 1 };
        let [k, a1, a2, a3] = s.svf;
        let [lp, bp, hp] = s.modes;
        let from = self.level_was;
        let step = (s.level - from) / n as f32;
        let mut x = [[0f32; CHUNK]; 2];
        #[allow(clippy::needless_range_loop)] // each sample writes every channel's
        for i in 0..n {
            let b = tap2(wb0, wb1, s.b.3, self.pb);
            self.pb = self.pb.wrapping_add(ib);
            let bend = turns(s.fm * b + s.feedback * self.last_a);
            let a0 = tap2(wa0, wa1, s.a.3, self.pa[0].wrapping_add(bend));
            self.last_a = a0;
            self.pa[0] = self.pa[0].wrapping_add(ia0);
            let mut sides = [a0, a0];
            if self.stereo {
                let a_1 = tap2(wa0, wa1, s.a.3, self.pa[1].wrapping_add(bend));
                let a_2 = tap2(wa0, wa1, s.a.3, self.pa[2].wrapping_add(bend));
                self.pa[1] = self.pa[1].wrapping_add(ia1);
                self.pa[2] = self.pa[2].wrapping_add(ia2);
                let u = s.unison;
                for (c, side) in sides.iter_mut().enumerate() {
                    *side = a0 * u[0] * s.spread[0][c]
                        + a_1 * u[1] * s.spread[1][c]
                        + a_2 * u[2] * s.spread[2][c];
                }
            }
            let mut shared = s.gb * b;
            if s.sub > 0.0 {
                shared += s.sub * tap(sine, self.psub);
                self.psub = self.psub.wrapping_add(isub);
            }
            if s.noise > 0.0 {
                let white = 2.0 * chance(&mut self.seed) - 1.0;
                self.noise_lp += s.noise_k * (white - self.noise_lp);
                shared += s.noise * self.noise_lp * s.noise_gain;
            }
            for c in 0..chans {
                let a = sides[c];
                let mut v = s.ga * a + shared;
                if s.ring > 0.0 {
                    v += s.ring * (1.4 * a * b - v);
                }
                if s.fold > 0.0 {
                    v += (fold(v * (1.0 + 5.0 * s.fold)) - v) * (s.fold * 4.0).min(1.0);
                }
                let v = soft(v * s.drive) * s.makeup;
                // A state-variable filter, solved without a sample's delay.
                let [ic1, ic2] = self.svf[c];
                let v3 = v - ic2;
                let v1 = a1 * ic1 + a2 * v3;
                let v2 = ic2 + a2 * ic1 + a3 * v3;
                self.svf[c] = [2.0 * v1 - ic1, 2.0 * v2 - ic2];
                x[c][i] = lp * v2 + bp * k * v1 + hp * (v - k * v1 - v2);
            }
        }
        // What follows the filter runs a block at a time, each only if it's on.
        if s.comb_mix > 0.0 {
            if !self.combed {
                self.comb[0].fill(0.0);
                self.comb[1].fill(0.0);
                self.combed = true;
            }
            let (len, fb, mix) = (s.comb_len, s.comb_fb, s.comb_mix);
            let (whole, part) = (len as usize, len.fract());
            for (c, side) in x.iter_mut().enumerate().take(chans) {
                let line = &mut self.comb[c];
                let mut at = self.comb_at;
                for v in &mut side[..n] {
                    let r0 = line[(at + COMB - whole) & (COMB - 1)];
                    let r1 = line[(at + COMB - whole - 1) & (COMB - 1)];
                    let back = r0 + (r1 - r0) * part;
                    self.comb_lp[c] += 0.5 * (back - self.comb_lp[c]);
                    let y = *v + fb * self.comb_lp[c];
                    line[at] = y.clamp(-4.0, 4.0);
                    at = (at + 1) & (COMB - 1);
                    *v += (y * (1.0 - fb) * 1.6 - *v) * mix;
                }
            }
            self.comb_at = (self.comb_at + n) & (COMB - 1);
        }
        if s.crush_steps > 0.0 {
            for (c, side) in x.iter_mut().enumerate().take(chans) {
                let mut count = self.crush_count;
                for v in &mut side[..n] {
                    count += 1.0;
                    if count >= s.crush_hold {
                        count -= s.crush_hold;
                        self.crushed[c] = (*v * s.crush_steps).round() / s.crush_steps;
                    }
                    *v = self.crushed[c];
                }
                if c + 1 == chans {
                    self.crush_count = count;
                }
            }
        }
        for (c, side) in x.iter_mut().enumerate().take(chans) {
            let mut lo = self.tilt_lp[c];
            for v in &mut side[..n] {
                lo += s.tilt_k * (*v - lo);
                *v = lo * s.tilt[0] + (*v - lo) * s.tilt[1];
            }
            self.tilt_lp[c] = lo;
        }
        // Level and pan.
        let mut level = from;
        #[allow(clippy::needless_range_loop)] // both sides, from one or two
        for i in 0..n {
            level += step;
            let (l, r) = match chans {
                2 => (x[0][i], x[1][i]),
                _ => (x[0][i], x[0][i]),
            };
            out[0][i] = l * level * s.pan[0];
            out[1][i] = r * level * s.pan[1];
        }
        self.level_was = s.level;
    }
}

/// Whether a parameter changes what the branches' genes are (growth moves them by their reach).
fn shapes_genes(i: usize) -> bool {
    i == BLOOM_WILD
        || i == BLOOM_ATONAL
        || (branch_bend(0)..branch_bend(BRANCHES)).contains(&i)
        || param_gene(i).is_some()
}

fn nearest_semis(x: f32, steps: &[f32]) -> f32 {
    let mut best = 0.0f32;
    for &s in steps {
        for v in [s, -s] {
            if (x - v).abs() < (x - best).abs() {
                best = v;
            }
        }
    }
    best
}

pub struct BloomSynth {
    rate: f32,
    tempo: f32,
    voices: Vec<Voice>,
    pos: Vec<f32>,
    vals: Vec<f32>,
    touched: Vec<usize>,
    seen: usize,
    /// The seed or a branch moved this block: every voice's genes are worked out again.
    regrow: bool,
    wheel: f32,
    pressure: f32,
    bend: f32,
    pedal: bool,
    held: Vec<u8>,
    age: u64,
    turn: usize,
    /// Dry, echo and space, left and right, at the running rate.
    mix: [Vec<f32>; 6],
    down: [[Down; 2]; 6],
    echo: Box<Delaying>,
    space: Box<Reverbing>,
    echo_set: crate::song::project::Delay,
    space_set: Reverb,
    /// Samples the effects have left to ring, and whether anything went to them this block.
    fx_tail: usize,
    sent: bool,
    tail: usize,
    newest: usize,
}

impl BloomSynth {
    pub fn new(rate: f64) -> BloomSynth {
        BloomSynth {
            rate: rate as f32,
            tempo: 120.0,
            voices: (0..MAX_VOICES)
                .map(|i| Voice::new(0x9e37_79b9 ^ (i as u32 * 7919)))
                .collect(),
            pos: vec![0.0; BLOOM_PARAM_COUNT],
            vals: vec![0.0; BLOOM_PARAM_COUNT],
            touched: Vec::with_capacity(BLOOM_PARAM_COUNT),
            seen: 0,
            regrow: true,
            wheel: 0.0,
            pressure: 0.0,
            bend: 0.0,
            pedal: false,
            held: Vec::with_capacity(128),
            age: 0,
            turn: 0,
            mix: std::array::from_fn(|_| vec![0.0; MAX_FRAMES]),
            down: std::array::from_fn(|_| [Down::new(), Down::new()]),
            echo: Box::new(Delaying::new(rate)),
            space: Box::new(Reverbing::new(rate)),
            echo_set: Default::default(),
            space_set: Reverb::default(),
            fx_tail: 0,
            sent: false,
            tail: 0,
            newest: 0,
        }
    }

    /// Back as it was made: silent, every voice and memory cleared, so the same notes play the
    /// same again (the same noise and unison phases too).
    pub fn reset(&mut self) {
        for (i, v) in self.voices.iter_mut().enumerate() {
            v.on = false;
            v.held = false;
            v.amp = Env::default();
            v.contour = Env::default();
            v.combed = false;
            v.seed = (0x9e37_79b9 ^ (i as u32 * 7919)) | 1;
        }
        self.held.clear();
        (self.wheel, self.pressure, self.bend, self.pedal) = (0.0, 0.0, 0.0, false);
        (self.age, self.turn, self.seen, self.newest) = (0, 0, 0, 0);
        (self.tail, self.fx_tail, self.sent) = (0, 0, false);
        self.touched.clear();
        self.echo.reset();
        self.space.reset();
        for d in self.down.iter_mut().flatten() {
            d.reset();
        }
    }

    pub fn set_tempo(&mut self, bpm: f32) {
        self.tempo = bpm.max(1.0);
    }

    /// The block's settings: `plan`'s, then automation (`auto`, positions), then modulators.
    pub fn begin(&mut self, plan: &BloomPlan, auto: impl Iterator<Item = (usize, f32)>) {
        let id = plan as *const BloomPlan as usize;
        if id != self.seen {
            self.pos.copy_from_slice(&plan.pos);
            self.vals.copy_from_slice(&plan.settings.params.0);
            self.seen = id;
            self.regrow = true;
        }
        for &i in &self.touched {
            self.pos[i] = plan.pos[i];
            self.vals[i] = plan.settings.params[i];
            self.regrow |= shapes_genes(i);
        }
        self.touched.clear();
        for (i, by) in auto {
            if i < BLOOM_PARAM_COUNT {
                let was = plan.pos[i];
                if !self.touched.contains(&i) {
                    self.touched.push(i);
                }
                self.pos[i] = (was + by).clamp(0.0, 1.0);
                self.vals[i] = BLOOM_PARAMS[i].scale.value(self.pos[i]);
                self.regrow |= shapes_genes(i);
            }
        }
        // The echo and space, as the plant's own genes have them.
        let e = &mut self.echo_set;
        let k = ((self.vals[gene_param(46)] * ECHO_BEATS.len() as f32) as usize)
            .min(ECHO_BEATS.len() - 1);
        e.sync = true;
        e.beats = [ECHO_BEATS[k], ECHO_BEATS[k] * 1.5];
        e.feedback = 0.42;
        e.cross = 0.6;
        e.high_cut_hz = 7000.0;
        e.low_cut_hz = 150.0;
        e.mix = 1.0;
        e.bypass = false;
        let size = self.vals[gene_param(47)];
        let r = &mut self.space_set;
        r.style = ReverbStyle::Hall;
        r.size = 0.25 + 0.75 * size;
        r.decay_s = 0.6 * 14f32.powf(size);
        r.predelay_ms = 12.0;
        r.modulation = 0.5;
        r.mix = 1.0;
        r.bypass = false;
    }

    /// Each branch's level and reach now, then how many voices sound and the growth.
    pub fn live(&self) -> [f32; BLOOM_LIVE] {
        let mut out = [0.0; BLOOM_LIVE];
        for v in self.voices.iter().filter(|v| v.on) {
            let b = v.branch as usize % BRANCHES;
            let level = v.amp.level * v.gain;
            if level >= out[b] {
                out[b] = level;
                out[BRANCHES + b] = v.reach.max(0.0);
            }
        }
        out[2 * BRANCHES] = self.voices.iter().filter(|v| v.on).count() as f32;
        out[2 * BRANCHES + 1] = self.extra();
        out
    }

    fn extra(&self) -> f32 {
        (self.vals[BLOOM_GROWTH] + self.wheel * self.vals[BLOOM_WHEEL]).clamp(0.0, 1.0)
    }

    fn free_voice(&mut self, limit: usize) -> usize {
        let on = self.voices.iter().filter(|v| v.on).count();
        if on < limit
            && let Some(i) = self.voices.iter().position(|v| !v.on)
        {
            return i;
        }
        let by = |v: &Voice| (v.on && v.held, v.age);
        (0..self.voices.len())
            .filter(|&i| self.voices[i].on)
            .min_by_key(|&i| by(&self.voices[i]))
            .unwrap_or(0)
    }

    /// The branches a note plays.
    fn branches(&mut self, plan: &BloomPlan, note: u8, velocity: f32) -> ([u8; BRANCHES], usize) {
        let s = &plan.settings;
        let mut out = [0u8; BRANCHES];
        let one = |b: usize| -> ([u8; BRANCHES], usize) {
            let mut o = [0u8; BRANCHES];
            o[0] = (b % BRANCHES) as u8;
            (o, 1)
        };
        match s.keys {
            BloomKeys::Octave => one(note as usize % 12),
            BloomKeys::Spread => one((note.saturating_sub(24) as usize * 12 / 85).min(11)),
            BloomKeys::Velocity => one(((velocity * 12.0) as usize).min(11)),
            BloomKeys::One => one(s.chosen as usize),
            BloomKeys::Cycle => {
                let grown: Vec<usize> = (0..BRANCHES)
                    .filter(|&b| self.vals[branch_length(b)] > 0.01)
                    .collect();
                self.turn += 1;
                match grown.is_empty() {
                    true => one(self.turn),
                    false => one(grown[self.turn % grown.len()]),
                }
            }
            BloomKeys::Layer => {
                let mut n = 0;
                for b in (0..BRANCHES).filter(|&b| s.layered(b)) {
                    out[n] = b as u8;
                    n += 1;
                }
                match n {
                    0 => one(s.chosen as usize),
                    _ => (out, n),
                }
            }
        }
    }

    fn start(&mut self, plan: &BloomPlan, note: u8, velocity: f32, legato: bool) {
        let s = &plan.settings;
        let (branches, n) = match s.voicing.mono {
            true => {
                let (b, _) = self.branches(plan, note, velocity);
                (b, 1)
            }
            false => self.branches(plan, note, velocity),
        };
        let limit = match s.voicing.mono {
            true => 1,
            false => (s.voicing.polyphony as usize).clamp(1, MAX_VOICES),
        };
        let gain = 1.0 / (n as f32).sqrt();
        for &branch in &branches[..n] {
            let i = match s.voicing.mono {
                true => self.voices.iter().position(|v| v.on).unwrap_or(0),
                false => self.free_voice(limit * n.max(1)),
            };
            self.age += 1;
            self.newest = i;
            let age = self.age;
            let v = &mut self.voices[i];
            let was_on = v.on;
            v.age = age;
            v.note = note;
            v.held = true;
            v.target = note as f32;
            let glide = self.vals[BLOOM_GLIDE] > 0.2 && s.voicing.mono;
            if !(was_on && glide) {
                v.pitch = note as f32;
            }
            if legato && was_on {
                continue;
            }
            v.on = true;
            v.branch = branch;
            v.gain = gain;
            v.velocity = velocity;
            (v.tune, v.slide, v.press) = (0.0, 0.0, 0.0);
            v.reach = -1.0;
            if !was_on {
                v.amp.level = 0.0;
                v.contour.level = 0.0;
                v.svf = [[0.0; 2]; 2];
                v.level_was = 0.0;
                v.comb_lp = [0.0; 2];
                v.combed = false;
                v.tilt_lp = [0.0; 2];
                v.noise_lp = 0.0;
            }
            v.amp.start();
            v.contour.start();
            v.lfo_ms = 0.0;
            v.lfo_phase = 0.0;
            // Each voice its own unison phases, so stacks don't comb; A and B start together.
            v.pa = [0, turns(chance(&mut v.seed)), turns(chance(&mut v.seed))];
            (v.pb, v.psub, v.last_a) = (0, 0, 0.0);
            // Its genes now, so its tables are chosen for the note.
            let shared = Shared {
                plan,
                vals: &self.vals,
                extra: (self.vals[BLOOM_GROWTH] + self.wheel * self.vals[BLOOM_WHEEL])
                    .clamp(0.0, 1.0),
                regrow: true,
                rate: self.rate,
                ms: 0.0,
                tempo: self.tempo,
                bend: self.bend,
                pressure: self.pressure,
            };
            let len = shared.vals[branch_length(branch as usize)].clamp(0.0, 1.0);
            let reach = len + shared.extra * (1.0 - len);
            v.regrow(&shared, reach);
            v.latch();
        }
    }

    fn stop(&mut self, note: u8) {
        for v in self
            .voices
            .iter_mut()
            .filter(|v| v.on && v.note == note && v.held)
        {
            v.held = false;
            v.amp.release();
        }
    }

    fn note(&mut self, plan: &BloomPlan, n: &Note) {
        let s = &plan.settings;
        let on = n.velocity > 0.0;
        self.held.retain(|&p| p != n.pitch);
        if on && self.held.len() < self.held.capacity() {
            self.held.push(n.pitch);
        }
        if s.voicing.mono {
            match on {
                true => {
                    let legato = s.voicing.legato && self.held.len() > 1;
                    self.start(plan, n.pitch, n.velocity, legato);
                }
                false => match self.held.last().copied() {
                    Some(p) => {
                        let v = self.voices[self.newest].velocity;
                        self.start(plan, p, v, s.voicing.legato);
                    }
                    None => self.stop(n.pitch),
                },
            }
            return;
        }
        match on {
            true => self.start(plan, n.pitch, n.velocity, false),
            false if !self.pedal => self.stop(n.pitch),
            false => {}
        }
    }

    fn control(&mut self, kind: ControlKind) {
        match kind {
            ControlKind::Cc { number: 1, value } => self.wheel = value,
            ControlKind::Cc { number: 64, value } => {
                let down = value >= 0.5;
                if self.pedal && !down {
                    let held = std::mem::take(&mut self.held);
                    for i in 0..self.voices.len() {
                        let v = &self.voices[i];
                        if v.on && v.held && !held.contains(&v.note) {
                            self.stop(self.voices[i].note);
                        }
                    }
                    self.held = held;
                }
                self.pedal = down;
            }
            ControlKind::PitchBend(b) => self.bend = b,
            ControlKind::Aftertouch(p) => self.pressure = p,
            ControlKind::PolyPressure { pitch, value } => {
                for v in self.voices.iter_mut().filter(|v| v.on && v.note == pitch) {
                    v.press = value;
                }
            }
            ControlKind::Expression { pitch, kind, value } => {
                for v in self.voices.iter_mut().filter(|v| v.on && v.note == pitch) {
                    match kind {
                        ExpressionKind::Tuning => v.tune = value,
                        ExpressionKind::Brightness => v.slide = value,
                        ExpressionKind::Expression => v.press = value,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    /// `n` samples of the song, into the buses from `at`, at the running rate.
    fn stretch(&mut self, plan: &BloomPlan, at: usize, n: usize) {
        let times = plan.settings.quality.times();
        let rate = self.rate * times as f32;
        let mut done = 0;
        while done < n {
            let take = STEP.min(n - done);
            let ms = take as f32 * 1000.0 / self.rate;
            let out = take * times;
            let from = (at + done) * times;
            let mut voices = std::mem::take(&mut self.voices);
            let mut mix = std::mem::take(&mut self.mix);
            {
                let shared = Shared {
                    plan,
                    vals: &self.vals,
                    extra: self.extra(),
                    regrow: self.regrow,
                    rate,
                    ms,
                    tempo: self.tempo,
                    bend: self.bend,
                    pressure: self.pressure,
                };
                let mut buf = [[0f32; CHUNK]; 2];
                for v in voices.iter_mut().filter(|v| v.on) {
                    v.control(&shared);
                    v.render(plan, out, &mut buf);
                    let (echo, space) = (v.set.echo, v.set.space);
                    self.sent |= echo + space > 1e-4;
                    for c in 0..2 {
                        let y = &buf[c][..out];
                        for (k, s) in y.iter().enumerate() {
                            mix[c][from + k] += s;
                        }
                        if echo > 1e-4 {
                            for (k, s) in y.iter().enumerate() {
                                mix[2 + c][from + k] += s * echo;
                            }
                        }
                        if space > 1e-4 {
                            for (k, s) in y.iter().enumerate() {
                                mix[4 + c][from + k] += s * space;
                            }
                        }
                    }
                    if v.amp.stage == 0 {
                        v.on = false;
                    }
                }
            }
            self.regrow = false;
            self.voices = voices;
            self.mix = mix;
            done += take;
        }
    }

    pub fn process(
        &mut self,
        plan: &BloomPlan,
        [l, r]: [&mut [f32]; 2],
        frames: usize,
        notes: &[Note],
        controls: &[Control],
    ) {
        let frames = frames
            .min(MAX_FRAMES / MOST_TIMES)
            .min(l.len())
            .min(r.len());
        let times = plan.settings.quality.times();
        let sounding =
            !notes.is_empty() || !self.held.is_empty() || self.voices.iter().any(|v| v.on);
        if sounding {
            self.tail = super::poly::TAPS;
        }
        if !sounding && self.tail == 0 && self.fx_tail == 0 {
            controls.iter().for_each(|c| self.control(c.kind));
            l[..frames].fill(0.0);
            r[..frames].fill(0.0);
            return;
        }
        self.tail = self.tail.saturating_sub(frames);
        for m in &mut self.mix {
            m[..frames * times].fill(0.0);
        }
        let mut next = notes.iter().peekable();
        let mut moves = controls.iter().peekable();
        let mut at = 0;
        while at < frames {
            while let Some(c) = moves.next_if(|c| c.offset as usize <= at) {
                self.control(c.kind);
            }
            while let Some(n) = next.next_if(|n| n.offset as usize <= at) {
                self.note(plan, n);
            }
            let end = [
                next.peek().map(|n| n.offset as usize),
                moves.peek().map(|c| c.offset as usize),
            ]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(frames)
            .clamp(at + 1, frames);
            self.stretch(plan, at, end - at);
            at = end;
        }
        // Back down to the song's rate: the dry sound always, the sends while they're used.
        if std::mem::take(&mut self.sent) {
            self.fx_tail = (FX_TAIL_S * self.rate) as usize;
        }
        let wet = self.fx_tail > 0;
        for (bus, m) in self.mix.iter_mut().enumerate() {
            if bus >= 2 && !wet {
                continue;
            }
            let mut n = frames * times;
            let mut stage = 0;
            while (1usize << stage) < times {
                n = self.down[bus][stage].run(&mut m[..n]);
                stage += 1;
            }
        }
        let gain = 10f32.powf(self.vals[BLOOM_VOLUME] / 20.0);
        if wet {
            let [_, _, el, er, sl, sr] = &mut self.mix;
            let bpm = self.tempo as f64;
            self.echo
                .process(&self.echo_set, bpm, &mut el[..frames], &mut er[..frames]);
            self.space
                .process(&self.space_set, bpm, &mut sl[..frames], &mut sr[..frames]);
            self.fx_tail = self.fx_tail.saturating_sub(frames);
        }
        let m = &self.mix;
        for i in 0..frames {
            let (mut a, mut b) = (m[0][i], m[1][i]);
            if wet {
                a += m[2][i] + m[4][i];
                b += m[3][i] + m[5][i];
            }
            l[i] = a * gain;
            r[i] = b * gain;
        }
    }
}

#[cfg(test)]
mod tests;
