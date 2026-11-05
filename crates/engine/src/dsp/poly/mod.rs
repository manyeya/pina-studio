//! The synth, running: voices of three wavetable oscillators, a sub and noise through two filters,
//! several times over the song's rate so nothing aliases, added and brought back down.
//!
//! What everything is set to is worked out every `STEP` samples: envelopes, LFOs and the matrix
//! move each voice's own copy of the parameters, which is why a filter can open differently for
//! every note. Nothing here allocates once it's made.

pub mod filter;
pub mod table;

use std::{
    f32::consts::{FRAC_PI_2, LN_2, PI, TAU},
    sync::Arc,
};

use filter::{Stage, Tuned};
use table::{Table, read, table, wave};

use super::modulate::{ModTarget, Offsets};
use crate::{
    plugin::{Control, ControlKind, Note},
    song::project::*,
};

const MAX_VOICES: usize = 32;
const UNISON: usize = 16;
/// How often settings are worked out, samples of the song.
const STEP: usize = 8;
const MOST_TIMES: usize = 4;
const CHUNK: usize = STEP * MOST_TIMES;
const MAX_FRAMES: usize = 4096;
/// One cycle, in the whole-number phases the oscillators turn in.
const TURN: f32 = 4_294_967_296.0;

/// A synth ready to play: its tables built and its matrix sorted, off the audio thread.
pub struct PolyPlan {
    pub settings: Poly,
    tables: [Arc<Table>; OSCS],
    sub: Arc<Table>,
    pos: Vec<f32>,
    /// What moves its own parameters, and which they are.
    routes: Vec<(PolySource, Option<PolySource>, usize, f32)>,
    moved: Vec<usize>,
    /// What rides the effects on its track.
    outward: Vec<(PolySource, Option<PolySource>, ModTarget, f32)>,
    /// What its macros move on their own: the same for every voice, so worked out once a block
    /// and costing nothing while they're down.
    macros: Vec<(usize, usize, f32)>,
    /// Only what something listens to runs: the envelopes, the LFOs each voice has of its own and
    /// those they all share, and which of the oscillators, sub and noise a warp hears.
    envs: Vec<usize>,
    own_lfos: Vec<usize>,
    free_lfos: Vec<usize>,
    heard: [bool; 5],
}

impl PolyPlan {
    /// Where a parameter's knob sits, 0 to 1.
    pub fn pos(&self, i: usize) -> f32 {
        self.pos.get(i).copied().unwrap_or(0.0)
    }

    pub fn new(p: &Poly) -> PolyPlan {
        let mut routes = Vec::new();
        let mut outward = Vec::new();
        let mut moved: Vec<usize> = Vec::new();
        let mut macros = Vec::new();
        for r in p.routes.iter().take(MAX_POLY_ROUTES) {
            match (r.source, r.by, r.dest) {
                (PolySource::Macro(m), None, PolyDest::Param(i))
                    if (i as usize) < PARAM_COUNT && (m as usize) < MACROS =>
                {
                    macros.push((macro_param(m as usize), i as usize, r.depth));
                    continue;
                }
                _ => {}
            }
            match r.dest {
                PolyDest::Param(i) if (i as usize) < PARAM_COUNT => {
                    routes.push((r.source, r.by, i as usize, r.depth));
                    if !moved.contains(&(i as usize)) {
                        moved.push(i as usize);
                    }
                }
                PolyDest::Effect(place, knob) => {
                    outward.push((r.source, r.by, ModTarget::Tool(place, knob), r.depth));
                }
                _ => {}
            }
        }
        let sources = routes
            .iter()
            .flat_map(|r| [Some(r.0), r.1])
            .chain(outward.iter().flat_map(|r| [Some(r.0), r.1]))
            .flatten();
        let (mut env, mut lfo) = ([false; ENVS], [false; LFOS]);
        env[0] = true;
        for s in sources {
            match s {
                PolySource::Env(i) => env[(i as usize).min(ENVS - 1)] = true,
                PolySource::Lfo(i) => lfo[(i as usize).min(LFOS - 1)] = true,
                _ => {}
            }
        }
        let lfos_where = |own: bool| {
            let used = (0..LFOS).filter(|&i| lfo[i] && p.lfos[i].retrigger == own);
            used.collect()
        };
        let heard = std::array::from_fn(|input| {
            let warped =
                |(o, w): (usize, &[Warp; 2])| w.iter().any(|&w| warp_input(w, o) == Some(input));
            p.warps.iter().enumerate().any(warped)
        });
        PolyPlan {
            envs: (0..ENVS).filter(|&i| env[i]).collect(),
            own_lfos: lfos_where(true),
            free_lfos: lfos_where(false),
            heard,
            settings: p.clone(),
            tables: std::array::from_fn(|i| table(&p.tables[i])),
            sub: table("Basic"),
            pos: p.positions(),
            routes,
            moved,
            outward,
            macros,
        }
    }
}

/// An envelope, running.
#[derive(Clone, Copy, Default)]
struct Env {
    /// 0 off, then delay, attack, hold, decay, sustain, release.
    stage: u8,
    /// How far through the stage, 0 to 1.
    through: f32,
    from: f32,
    level: f32,
}

/// A stretch of time bent as a capacitor charges: fast, then slowing.
#[inline]
fn charge(x: f32, curve: f32) -> f32 {
    let c = curve * 5.0;
    if c < 0.01 {
        return x;
    }
    (1.0 - (-c * x).exp()) / (1.0 - (-c).exp())
}

impl Env {
    fn start(&mut self) {
        (self.stage, self.through, self.from) = (1, 0.0, self.level);
    }

    fn release(&mut self) {
        if self.stage != 0 {
            (self.stage, self.through, self.from) = (6, 0.0, self.level);
        }
    }

    /// `ms` on, by the values in `v` from `at`; its level.
    fn step(&mut self, ms: f32, v: &[f32], at: usize) -> f32 {
        let of = |f: EnvField| v[at + f as usize];
        let curve = of(EnvField::Curve);
        let mut left = ms;
        // A stage can end inside the step: the rest runs on into the next.
        for _ in 0..7 {
            let long = match self.stage {
                1 => of(EnvField::Delay),
                2 => of(EnvField::Attack),
                3 => of(EnvField::Hold),
                4 => of(EnvField::Decay),
                6 => of(EnvField::Release),
                _ => break,
            }
            .max(0.05);
            let was = self.through;
            self.through = (was + left / long).min(1.0);
            let x = charge(self.through, curve);
            let sustain = of(EnvField::Sustain);
            self.level = match self.stage {
                1 => self.from,
                2 => self.from + (1.0 - self.from) * x,
                3 => 1.0,
                4 => 1.0 + (sustain - 1.0) * x,
                _ => self.from * (1.0 - x),
            };
            if self.through < 1.0 {
                break;
            }
            left -= (1.0 - was) * long;
            self.through = 0.0;
            self.from = self.level;
            self.stage = match self.stage {
                6 => 0,
                s => s + 1,
            };
            if self.stage == 5 {
                self.level = sustain;
            }
        }
        if self.stage == 5 {
            self.level = of(EnvField::Sustain);
        }
        if self.stage == 0 {
            self.level = 0.0;
        }
        self.level
    }
}

/// An LFO, running.
#[derive(Clone, Copy)]
struct Lfo {
    phase: f32,
    value: f32,
    held: [f32; 2],
    /// Where its attractor is.
    at: [f32; 3],
    age_ms: f32,
    done: bool,
}

impl Default for Lfo {
    fn default() -> Self {
        Lfo {
            phase: 0.0,
            value: 0.0,
            held: [0.0; 2],
            at: [0.1, 0.0, 0.0],
            age_ms: 0.0,
            done: false,
        }
    }
}

#[inline]
fn chance(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed as f32 / u32::MAX as f32
}

impl Lfo {
    fn start(&mut self, phase: f32) {
        (self.phase, self.age_ms, self.done) = (phase.fract(), 0.0, false);
    }

    /// `ms` on; -1 to 1.
    fn step(
        &mut self,
        ms: f32,
        i: usize,
        s: &PolyLfo,
        v: &[f32],
        tempo: f32,
        seed: &mut u32,
    ) -> f32 {
        let of = |f: LfoField| v[lfo_param(i, f)];
        let hz = match s.beats > 0.0 {
            true => tempo / 60.0 / s.beats,
            false => of(LfoField::Rate),
        };
        self.age_ms += ms;
        let cycles = hz * ms / 1000.0;
        let next = self.phase + cycles;
        let wrapped = next >= 1.0;
        if wrapped && s.one_shot {
            self.done = true;
        }
        self.phase = if self.done { 0.9999 } else { next.fract() };
        if wrapped {
            self.held = [self.held[1], chance(seed) * 2.0 - 1.0];
        }
        let p = (self.phase + of(LfoField::Phase)).fract();
        let raw = match &s.wave {
            LfoWave::Sine => (TAU * p).sin(),
            LfoWave::Triangle => 1.0 - 4.0 * ((p + 0.25).fract() - 0.5).abs(),
            LfoWave::SawUp => 2.0 * p - 1.0,
            LfoWave::SawDown => 1.0 - 2.0 * p,
            LfoWave::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoWave::Hold => self.held[1],
            LfoWave::Smooth => {
                let t = 0.5 - 0.5 * (PI * p).cos();
                self.held[0] + (self.held[1] - self.held[0]) * t
            }
            LfoWave::Lorenz => {
                let dt = (cycles * 0.6).min(0.02);
                let [x, y, z] = self.at;
                self.at = [
                    x + dt * 10.0 * (y - x),
                    y + dt * (x * (28.0 - z) - y),
                    z + dt * (x * y - 8.0 / 3.0 * z),
                ];
                (self.at[0] / 20.0).clamp(-1.0, 1.0)
            }
            LfoWave::Rossler => {
                let dt = (cycles * 3.0).min(0.05);
                let [x, y, z] = self.at;
                self.at = [
                    x + dt * (-y - z),
                    y + dt * (x + 0.2 * y),
                    z + dt * (0.2 + z * (x - 5.7)),
                ];
                (self.at[0] / 11.0).clamp(-1.0, 1.0)
            }
            LfoWave::Drawn(points) => drawn(points, p) * 2.0 - 1.0,
        };
        let smooth = of(LfoField::Smooth);
        let k = 1.0 - smooth * 0.995;
        self.value += (raw - self.value) * k;
        let rise = (self.age_ms / of(LfoField::Rise).max(0.1)).min(1.0);
        self.value * rise
    }
}

/// A drawn wave at `p`, between its points.
fn drawn(points: &[(f32, f32)], p: f32) -> f32 {
    let at = points.partition_point(|q| q.0 <= p);
    match (
        at.checked_sub(1).and_then(|i| points.get(i)),
        points.get(at),
    ) {
        (Some(a), Some(b)) => {
            let t = (p - a.0) / (b.0 - a.0).max(1e-5);
            a.1 + (b.1 - a.1) * t
        }
        (Some(a), None) => a.1,
        (None, Some(b)) => b.1,
        _ => 0.5,
    }
}

/// An oscillator's unison voices.
#[derive(Clone, Copy)]
struct Osc {
    phase: [u32; UNISON],
    /// Each unison voice's own wander, cents.
    wander: [f32; UNISON],
}

struct Voice {
    on: bool,
    held: bool,
    note: u8,
    /// The pitch it's at, and gliding to, semitones.
    pitch: f32,
    target: f32,
    velocity: f32,
    random: f32,
    age: u64,
    amp: f32,
    envs: [Env; ENVS],
    env_now: [f32; ENVS],
    lfos: [Lfo; LFOS],
    lfo_now: [f32; LFOS],
    oscs: [Osc; OSCS],
    sub: u32,
    noise: f32,
    filters: [[Stage; 2]; FILTERS],
    values: Vec<f32>,
    moved: Vec<f32>,
    seed: u32,
    drift: f32,
    drift_cut: f32,
    /// Its note's own expression: how far it's bent (semitones), pressed and slid (0 to 1).
    tune: f32,
    press: f32,
    slide: f32,
}

impl Voice {
    fn new(seed: u32) -> Voice {
        Voice {
            on: false,
            held: false,
            note: 60,
            pitch: 60.0,
            target: 60.0,
            velocity: 0.0,
            random: 0.0,
            age: 0,
            amp: 0.0,
            envs: Default::default(),
            env_now: [0.0; ENVS],
            lfos: Default::default(),
            lfo_now: [0.0; LFOS],
            oscs: [Osc {
                phase: [0; UNISON],
                wander: [0.0; UNISON],
            }; OSCS],
            sub: 0,
            noise: 0.0,
            filters: std::array::from_fn(|_| [Stage::new(), Stage::new()]),
            values: vec![0.0; PARAM_COUNT],
            moved: vec![0.0; PARAM_COUNT],
            seed: seed | 1,
            drift: 0.0,
            drift_cut: 0.0,
            tune: 0.0,
            press: 0.0,
            slide: 0.0,
        }
    }
}

/// What the synth's window watches, in the order `PolySynth::live` gives them.
pub const LIVE_SOURCES: [PolySource; ENVS + LFOS + 7] = {
    let mut all = [PolySource::Velocity; ENVS + LFOS + 7];
    let mut i = 0;
    while i < ENVS {
        all[i] = PolySource::Env(i as u8);
        i += 1;
    }
    while i < ENVS + LFOS {
        all[i] = PolySource::Lfo((i - ENVS) as u8);
        i += 1;
    }
    all[i + 1] = PolySource::Note;
    all[i + 2] = PolySource::Wheel;
    all[i + 3] = PolySource::Pressure;
    all[i + 4] = PolySource::Bend;
    all[i + 5] = PolySource::Random;
    all[i + 6] = PolySource::Slide;
    all
};

/// How many numbers `PolySynth::live` gives: its sources, then the voices sounding.
pub const POLY_LIVE: usize = LIVE_SOURCES.len() + 1;

/// A wave as an oscillator plays it, for its picture: `n` points of table `t` at `position`
/// through its warps. What a warp would hear from another oscillator is drawn as a sine.
pub fn drawn_wave(
    t: &Table,
    position: f32,
    warps: [Warp; 2],
    amounts: [f32; 2],
    osc: usize,
    n: usize,
) -> Vec<f32> {
    let at = position.clamp(0.0, 1.0) * (t.frames - 1) as f32;
    let first = (at as usize).min(t.frames - 1);
    let second = (first + 1).min(t.frames - 1);
    let (a, b, part) = (t.frame(first), t.frame(second), at - first as f32);
    let plain: Vec<f32> = (0..n).map(|i| i as f32 / n as f32).collect();
    let tone: Vec<f32> = plain.iter().map(|p| (TAU * p).sin()).collect();
    let silence = vec![0.0; n];
    let heard = |k: usize| match warp_input(warps[k], osc) {
        Some(_) => &tone,
        None => &silence,
    };
    let mut bent = plain.clone();
    for k in 0..2 {
        let ratio = match warps[k] {
            Warp::Sync => 1.0 + 15.0 * amounts[k] * amounts[k],
            _ => 1.0,
        };
        warp_phases(warps[k], amounts[k], ratio, heard(k), &mut bent);
    }
    let size = a.len() as f32;
    let mut x: Vec<f32> = bent
        .iter()
        .map(|p| {
            let at = p.clamp(0.0, 0.99999) * size;
            let (i, f) = (at as usize, at.fract());
            let j = (i + 1) % a.len();
            let (u, v) = (a[i] + (a[j] - a[i]) * f, b[i] + (b[j] - b[i]) * f);
            u + (v - u) * part
        })
        .collect();
    for k in 0..2 {
        warp_wave(warps[k], amounts[k], &plain, heard(k), &mut x);
    }
    x
}

/// What a voice reads from the synth for a step.
struct Shared<'a> {
    plan: &'a PolyPlan,
    pos: &'a [f32],
    vals: &'a [f32],
    wheel: f32,
    pressure: f32,
    bend: f32,
    rate: f32,
    ms: f32,
    tempo: f32,
    global: &'a [f32; LFOS],
}

#[inline]
fn source(s: PolySource, v: &Voice, c: &Shared) -> f32 {
    match s {
        PolySource::Env(i) => v.env_now[(i as usize).min(ENVS - 1)],
        PolySource::Lfo(i) => {
            let i = (i as usize).min(LFOS - 1);
            match c.plan.settings.lfos[i].retrigger {
                true => v.lfo_now[i],
                false => c.global[i],
            }
        }
        PolySource::Macro(i) => c.vals[macro_param((i as usize).min(MACROS - 1))],
        PolySource::Velocity => v.velocity,
        PolySource::Note => v.note as f32 / 127.0,
        PolySource::Wheel => c.wheel,
        // The channel's pressure, or the note's own if it's pressed harder.
        PolySource::Pressure => c.pressure.max(v.press),
        PolySource::Bend => c.bend,
        PolySource::Random => v.random,
        PolySource::Slide => v.slide,
    }
}

#[inline]
fn each(x: &mut [f32], f: impl Fn(f32) -> f32) {
    for v in x {
        *v = f(*v);
    }
}

#[inline]
fn each_with(x: &mut [f32], with: &[f32], f: impl Fn(f32, f32) -> f32) {
    for (v, w) in x.iter_mut().zip(with) {
        *v = f(*v, *w);
    }
}

/// Bends and folds done to the phases before the wave's read, each kind its own loop: they're
/// decided once a chunk, not for every sample of every unison voice.
fn warp_phases(w: Warp, a: f32, ratio: f32, input: &[f32], p: &mut [f32]) {
    match w {
        Warp::Sync => each(p, |p| (p * ratio).fract()),
        Warp::BendUp => each(p, |p| p + a * (p * p - p)),
        Warp::BendDown => each(p, |p| p - a * (p * p - p)),
        Warp::BendBoth => each(p, |p| p + a * (p * p * (3.0 - 2.0 * p) - p)),
        Warp::Pwm => {
            let w = 1.0 - 0.98 * a;
            each(p, |p| if p < w { p / w } else { 0.0 });
        }
        Warp::Asym => {
            let k = 0.5 + 0.47 * a;
            each(p, |p| {
                if p < k {
                    0.5 * p / k
                } else {
                    0.5 + 0.5 * (p - k) / (1.0 - k)
                }
            });
        }
        Warp::Mirror => each(p, |p| p + a * (1.0 - (2.0 * p - 1.0).abs() - p)),
        Warp::Quantize => {
            let steps = 2.0 + 254.0 * (1.0 - a) * (1.0 - a);
            each(p, |p| (p * steps).floor() / steps);
        }
        Warp::PhaseDist => each(p, |p| {
            let q = p + a * 0.25 * (TAU * p).sin();
            q - q.floor()
        }),
        w if is_fm(w) => each_with(p, input, |p, i| {
            let q = p + a * 2.0 * i;
            q - q.floor()
        }),
        _ => {}
    }
}

/// What's done to the wave once it's read: `at` the phases before they were bent.
fn warp_wave(w: Warp, a: f32, at: &[f32], input: &[f32], x: &mut [f32]) {
    match w {
        Warp::Flip => {
            let from = 1.0 - a * 0.5;
            each_with(x, at, |x, p| if p > from { -x } else { x });
        }
        Warp::Fold => each(x, |x| (x * (1.0 + 7.0 * a) * FRAC_PI_2).sin()),
        Warp::RingFromA | Warp::RingFromB => each_with(x, input, |x, i| x + a * (x * i - x)),
        Warp::AmFromA | Warp::AmFromB => {
            each_with(x, input, |x, i| x * (1.0 - a + a * (0.5 + 0.5 * i)));
        }
        _ => {}
    }
}

/// A warp's source: which earlier signal it listens to, if any: 0-2 an oscillator, 3 the sub, 4
/// the noise.
fn warp_input(w: Warp, osc: usize) -> Option<usize> {
    let earlier = |i: usize| (i < osc).then_some(i);
    match w {
        Warp::FmFromA | Warp::RingFromA | Warp::AmFromA => earlier(0),
        Warp::FmFromB | Warp::RingFromB | Warp::AmFromB => earlier(1),
        Warp::FmFromSub => Some(3),
        Warp::FmFromNoise => Some(4),
        _ => None,
    }
}

fn is_fm(w: Warp) -> bool {
    matches!(
        w,
        Warp::FmFromA | Warp::FmFromB | Warp::FmFromNoise | Warp::FmFromSub
    )
}

type Buffer = [f32; CHUNK];

impl Voice {
    /// Moves on a step: its envelopes and LFOs, then the matrix.
    fn control(&mut self, c: &Shared) {
        let s = &c.plan.settings;
        for &i in &c.plan.envs {
            let at = env_param(i, EnvField::Delay);
            self.env_now[i] = self.envs[i].step(c.ms, &self.values, at);
        }
        for &i in &c.plan.own_lfos {
            let (lfos, seed) = (&mut self.lfos, &mut self.seed);
            self.lfo_now[i] = lfos[i].step(c.ms, i, &s.lfos[i], &self.values, c.tempo, seed);
        }
        if !c.plan.routes.is_empty() {
            for &i in &c.plan.moved {
                self.moved[i] = 0.0;
            }
            for &(src, by, dest, depth) in &c.plan.routes {
                let scale = by.map_or(1.0, |b| source(b, self, c));
                self.moved[dest] += depth * source(src, self, c) * scale;
            }
            for &i in &c.plan.moved {
                self.values[i] = POLY_PARAMS[i].scale.value(c.pos[i] + self.moved[i]);
            }
        }
        // Glide, and the slow wander old parts have.
        let glide = self.values[POLY_GLIDE];
        let k = 1.0 - (-c.ms / glide.max(0.1)).exp();
        self.pitch += (self.target - self.pitch) * k;
        let wander = 0.01 * (chance(&mut self.seed) - 0.5);
        self.drift = (self.drift * 0.999 + wander).clamp(-1.0, 1.0);
        self.drift_cut =
            (self.drift_cut * 0.999 + 0.01 * (chance(&mut self.seed) - 0.5)).clamp(-1.0, 1.0);
        if self.values[POLY_DRIFT] > 0.0 {
            for o in 0..OSCS {
                let unison = self.values[osc_param(o, OscField::Unison)].round() as usize;
                for w in &mut self.oscs[o].wander[..unison.clamp(1, UNISON)] {
                    let step = 0.04 * (chance(&mut self.seed) - 0.5);
                    *w = (*w * 0.998 + step).clamp(-1.0, 1.0);
                }
            }
        }
    }

    /// `n` samples at `c.rate` added into `out`.
    fn render(&mut self, c: &Shared, n: usize, out: [&mut [f32]; 2]) {
        let s = &c.plan.settings;
        let v = &self.values;
        let drift = v[POLY_DRIFT];
        let bent = c.bend * s.voicing.bend_semitones;
        let pitch = self.pitch + bent + self.tune + self.drift * drift * 0.15;
        let hz_of = |st: f32| 440.0 * ((st - 69.0) / 12.0 * LN_2).exp();
        let mut bus: [[Buffer; 2]; 3] = [[[0.0; CHUNK]; 2]; 3];
        // The last is silence, for a warp with nothing earlier to hear.
        let mut heard: [Buffer; 6] = [[0.0; CHUNK]; 6];
        // The sub and the noise first: an oscillator can listen to them.
        let sub_level = v[SUB_LEVEL];
        let noise_level = v[NOISE_LEVEL];
        let wants = |input: usize| c.plan.heard[input];
        let mut turns = [0u32; CHUNK];
        if sub_level > 1e-4 || wants(3) {
            let hz = hz_of(pitch - 12.0 * s.sub_octave as f32);
            let inc = hz / c.rate;
            let t = &c.plan.sub;
            let a = wave(
                t.level(Table::mip(inc)),
                (s.sub_wave as usize).min(t.frames - 1),
            );
            self.sub = advance(self.sub, inc, &mut turns[..n]);
            read(a, a, 0.0, &turns[..n], &mut heard[3][..n]);
            add_panned(
                &mut bus,
                s.to_filter[3],
                s.routing,
                &heard[3],
                sub_level,
                v[SUB_PAN],
                n,
            );
        }
        if noise_level > 1e-4 || wants(4) {
            let color = v[NOISE_COLOR];
            let k = 0.15;
            let mut low = self.noise;
            for x in &mut heard[4][..n] {
                let w = chance(&mut self.seed) * 2.0 - 1.0;
                low += k * (w - low);
                *x = match color < 0.0 {
                    true => w + (low * 2.0 - w) * -color,
                    false => w + (w - low - w) * color,
                };
            }
            self.noise = low;
            add_panned(
                &mut bus,
                s.to_filter[4],
                s.routing,
                &heard[4],
                noise_level,
                v[NOISE_PAN],
                n,
            );
        }
        for o in 0..OSCS {
            let f = |x: OscField| self.values[osc_param(o, x)];
            let level = f(OscField::Level);
            let needed = wants(o);
            if level < 1e-4 && !needed {
                continue;
            }
            let st = pitch
                + f(OscField::Octave).round() * 12.0
                + f(OscField::Semi).round()
                + f(OscField::Fine) / 100.0;
            let inc = hz_of(st) / c.rate;
            let unison = (f(OscField::Unison).round() as usize).clamp(1, UNISON);
            let (warps, amounts) = (s.warps[o], [f(OscField::WarpA), f(OscField::WarpB)]);
            let sync = |i: usize| match warps[i] {
                Warp::Sync => 1.0 + 15.0 * amounts[i] * amounts[i],
                _ => 1.0,
            };
            let ratio = [sync(0), sync(1)];
            let t = &c.plan.tables[o];
            let at = f(OscField::Position).clamp(0.0, 1.0) * (t.frames - 1) as f32;
            let first = (at as usize).min(t.frames - 1);
            let part = at - first as f32;
            let second = (first + 1).min(t.frames - 1);
            let lv = t.level(Table::mip(inc * ratio[0] * ratio[1]));
            let (a, b) = (wave(lv, first), wave(lv, second));
            let inputs = [warp_input(warps[0], o), warp_input(warps[1], o)];
            let warped = warps != [Warp::None; 2];
            let (depth, wide, blend, pan) = (
                f(OscField::Detune),
                f(OscField::Width),
                f(OscField::Blend),
                f(OscField::Pan),
            );
            let mut mono: Buffer = [0.0; CHUNK];
            let (mut at, mut bent, mut one): (Buffer, Buffer, Buffer) =
                ([0.0; CHUNK], [0.0; CHUNK], [0.0; CHUNK]);
            let mut left: Buffer = [0.0; CHUNK];
            let mut right: Buffer = [0.0; CHUNK];
            // Evenly over the voices, louder at the edges for a fuller stack.
            let norm = 1.0 / (unison as f32).sqrt();
            for u in 0..unison {
                let d = match unison {
                    1 => 0.0,
                    k => 2.0 * u as f32 / (k - 1) as f32 - 1.0,
                };
                let cents = d * depth + self.oscs[o].wander[u] * drift * 3.0;
                let x = cents * LN_2 / 1200.0;
                let step = inc * (1.0 + x + 0.5 * x * x);
                let edge = d.abs();
                let g = match unison {
                    1 => 1.0,
                    _ => (1.0 - edge) * (1.0 - blend * 0.7) + edge * blend.max(0.05),
                };
                let gain = g * norm;
                let side = (pan + d * wide).clamp(-1.0, 1.0);
                let angle = (side + 1.0) * 0.25 * PI;
                let (gl, gr) = (
                    angle.cos() * gain * std::f32::consts::SQRT_2,
                    angle.sin() * gain * std::f32::consts::SQRT_2,
                );
                self.oscs[o].phase[u] = advance(self.oscs[o].phase[u], step, &mut turns[..n]);
                let input = |k: usize| inputs[k].map_or(&heard[5], |w| &heard[w]);
                if warped {
                    for (p, q) in at[..n].iter_mut().zip(&turns) {
                        *p = *q as f32 / TURN;
                    }
                    bent[..n].copy_from_slice(&at[..n]);
                    for k in 0..2 {
                        warp_phases(warps[k], amounts[k], ratio[k], input(k), &mut bent[..n]);
                    }
                    for (q, p) in turns[..n].iter_mut().zip(&bent) {
                        *q = (p.clamp(0.0, 0.99999) * TURN) as u32;
                    }
                }
                read(a, b, part, &turns[..n], &mut one[..n]);
                if warped {
                    for k in 0..2 {
                        warp_wave(warps[k], amounts[k], &at[..n], input(k), &mut one[..n]);
                    }
                }
                if needed {
                    for (m, x) in mono[..n].iter_mut().zip(&one) {
                        *m += x * gain;
                    }
                }
                for ((l, r), x) in left[..n].iter_mut().zip(&mut right).zip(&one) {
                    *l += x * gl;
                    *r += x * gr;
                }
            }
            if needed {
                heard[o][..n].copy_from_slice(&mono[..n]);
            }
            if level >= 1e-4 {
                add_stereo(
                    &mut bus,
                    s.to_filter[o],
                    s.routing,
                    (&left, &right),
                    level,
                    n,
                );
            }
        }
        self.filter(c, n, &mut bus);
        // The amp's envelope, ramped across the step.
        let velocity = self.values[POLY_VELOCITY];
        let gain = 1.0 - velocity + velocity * self.velocity * self.velocity;
        let to = self.env_now[0] * gain;
        let from = self.amp;
        let step = (to - from) / n as f32;
        let [l, r] = out;
        for i in 0..n {
            let g = from + step * (i + 1) as f32;
            l[i] += bus[2][0][i] * g;
            r[i] += bus[2][1][i] * g;
        }
        self.amp = to;
    }

    #[allow(clippy::needless_range_loop)] // each side indexes several buses at once
    fn filter(&mut self, c: &Shared, n: usize, bus: &mut [[Buffer; 2]; 3]) {
        let s = &c.plan.settings;
        let tune = |v: &Voice, i: usize, side: f32| {
            let f = |x: FilterField| v.values[filter_param(i, x)];
            let keys = (v.pitch - 60.0) / 12.0 * f(FilterField::KeyTrack);
            let wander = v.drift_cut * v.values[POLY_DRIFT] * 0.05;
            let oct = keys + wander + side * f(FilterField::Spread) * 0.5;
            let hz = f(FilterField::Cutoff) * (oct * LN_2).exp();
            Tuned::new(
                s.filters[i],
                hz,
                f(FilterField::Resonance),
                f(FilterField::Drive),
                f(FilterField::Bass),
                c.rate,
            )
        };
        let mut tuned = [[Tuned::default(); 2]; FILTERS];
        for i in (0..FILTERS).filter(|&i| s.filters[i] != PolyFilter::None) {
            tuned[i][0] = tune(self, i, -1.0);
            tuned[i][1] = match self.values[filter_param(i, FilterField::Spread)] == 0.0 {
                true => tuned[i][0],
                false => tune(self, i, 1.0),
            };
        }
        let wet = |v: &Voice, i: usize| v.values[filter_param(i, FilterField::Mix)];
        let through = |v: &mut Voice, i: usize, [l, r]: &mut [Buffer; 2]| {
            let kind = s.filters[i];
            if kind == PolyFilter::None {
                return;
            }
            let (mix, tuned) = (wet(v, i), [&tuned[i][0], &tuned[i][1]]);
            let dry = (mix < 0.999).then_some([*l, *r]);
            let sides = [&mut l[..n], &mut r[..n]];
            Stage::run_both(kind, &mut v.filters[i], tuned, sides);
            if let Some([a, b]) = dry {
                for (y, d) in l[..n].iter_mut().zip(&a).chain(r[..n].iter_mut().zip(&b)) {
                    *y = d + (*y - d) * mix;
                }
            }
        };
        let [first, second, out] = bus;
        let add = |to: &mut [Buffer; 2], from: &[Buffer; 2]| {
            for (a, b) in to.iter_mut().zip(from) {
                for (y, x) in a[..n].iter_mut().zip(b) {
                    *y += x;
                }
            }
        };
        match s.routing {
            FilterRouting::Series => {
                through(self, 0, first);
                add(second, first);
                through(self, 1, second);
                add(out, second);
            }
            FilterRouting::Parallel => {
                through(self, 0, first);
                through(self, 1, second);
                add(out, first);
                add(out, second);
            }
            // Each side its own filter, which may be another kind.
            FilterRouting::Stereo => {
                add(first, second);
                for side in 0..2 {
                    let (kind, mix) = (s.filters[side], wet(self, side));
                    let x = &mut first[side];
                    let dry = *x;
                    self.filters[side][side].run(kind, &tuned[side][side], &mut x[..n]);
                    if mix < 0.999 && kind != PolyFilter::None {
                        for (y, d) in x[..n].iter_mut().zip(&dry) {
                            *y = d + (*y - d) * mix;
                        }
                    }
                }
                add(out, first);
            }
        }
    }
}

/// `turns` filled from `phase` on by `step` cycles a sample; the phase after them.
#[inline]
fn advance(phase: u32, step: f32, turns: &mut [u32]) -> u32 {
    // Through u64 so a note past the rate folds round as it should rather than sticking.
    let step = (step * TURN) as u64 as u32;
    let mut ph = phase;
    for q in turns {
        *q = ph;
        ph = ph.wrapping_add(step);
    }
    ph
}

/// Which bus a sound goes on: into filter 1, into filter 2, or past them both.
fn bus_of(to: [bool; FILTERS]) -> usize {
    match to {
        [true, _] => 0,
        [false, true] => 1,
        _ => 2,
    }
}

fn add_stereo(
    bus: &mut [[Buffer; 2]; 3],
    to: [bool; FILTERS],
    routing: FilterRouting,
    (l, r): (&Buffer, &Buffer),
    level: f32,
    n: usize,
) {
    let mut put = |b: usize, g: f32| {
        for i in 0..n {
            bus[b][0][i] += l[i] * g;
            bus[b][1][i] += r[i] * g;
        }
    };
    match (routing, to) {
        // Side by side, one sound into both shares itself between them.
        (FilterRouting::Parallel, [true, true]) => {
            put(0, level * 0.5);
            put(1, level * 0.5);
        }
        _ => put(bus_of(to), level),
    }
}

fn add_panned(
    bus: &mut [[Buffer; 2]; 3],
    to: [bool; FILTERS],
    routing: FilterRouting,
    x: &Buffer,
    level: f32,
    pan: f32,
    n: usize,
) {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * 0.25 * PI;
    let (gl, gr) = (
        angle.cos() * std::f32::consts::SQRT_2,
        angle.sin() * std::f32::consts::SQRT_2,
    );
    let mut l: Buffer = [0.0; CHUNK];
    let mut r: Buffer = [0.0; CHUNK];
    for i in 0..n {
        l[i] = x[i] * gl;
        r[i] = x[i] * gr;
    }
    add_stereo(bus, to, routing, (&l, &r), level, n);
}

const TAPS: usize = 63;

/// A half-band low-pass that keeps every other sample: back down an octave of rate.
struct Down {
    /// One side's taps, either side of the middle's half: every other tap of a half-band is nothing.
    sides: [f32; TAPS / 4 + 1],
    middle: f32,
    /// What it last heard, then the block, end to end so a window never wraps.
    line: Vec<f32>,
}

impl Down {
    fn new() -> Down {
        const N: usize = TAPS;
        let mid = (N / 2) as f32;
        let mut taps: Vec<f32> = (0..N)
            .map(|i| {
                let t = i as f32 - mid;
                let sinc = match t == 0.0 {
                    true => 0.5,
                    false => (FRAC_PI_2 * t).sin() / (PI * t),
                };
                // Blackman-Harris: over 90 dB gone past the band.
                let w = TAU * i as f32 / (N - 1) as f32;
                let window = 0.35875 - 0.48829 * w.cos() + 0.14128 * (2.0 * w).cos()
                    - 0.01168 * (3.0 * w).cos();
                sinc * window
            })
            .collect();
        let sum: f32 = taps.iter().sum();
        taps.iter_mut().for_each(|t| *t /= sum);
        Down {
            sides: std::array::from_fn(|k| taps[2 * k]),
            middle: taps[N / 2],
            line: vec![0.0; TAPS - 1 + MAX_FRAMES],
        }
    }

    /// `x` in, half as many out, in place.
    fn run(&mut self, x: &mut [f32]) -> usize {
        let n = x.len();
        self.line[TAPS - 1..TAPS - 1 + n].copy_from_slice(x);
        let out = n / 2;
        for (m, y) in x[..out].iter_mut().enumerate() {
            let w: &[f32; TAPS] = self.line[2 * m + 1..][..TAPS].try_into().unwrap();
            let mut sum = self.middle * w[TAPS / 2];
            for (k, t) in self.sides.iter().enumerate() {
                sum += t * (w[2 * k] + w[TAPS - 1 - 2 * k]);
            }
            *y = sum;
        }
        self.line.copy_within(n..n + TAPS - 1, 0);
        out
    }
}

pub struct PolySynth {
    rate: f32,
    tempo: f32,
    voices: Vec<Voice>,
    /// Where each parameter's knob is, this block, and what that comes to.
    pos: Vec<f32>,
    vals: Vec<f32>,
    /// Those automation and modulators moved last block, to put back.
    touched: Vec<usize>,
    lfos: [Lfo; LFOS],
    lfo_now: [f32; LFOS],
    wheel: f32,
    pressure: f32,
    bend: f32,
    pedal: bool,
    /// What's held, oldest first.
    held: Vec<u8>,
    age: u64,
    seed: u32,
    seen: usize,
    mix: [Vec<f32>; 2],
    down: [[Down; 2]; 2],
    /// Samples the decimators still ring for, once the last voice has gone.
    tail: usize,
    /// The arpeggiator: where it is in its pattern, samples to its next step and the note's end,
    /// and the note sounding.
    arp_step: usize,
    arp_next: f64,
    arp_off: f64,
    arp_note: Option<u8>,
    arp_swung: bool,
    arp_up: bool,
    newest: usize,
}

impl PolySynth {
    pub fn new(rate: f64) -> PolySynth {
        PolySynth {
            rate: rate as f32,
            tempo: 120.0,
            voices: (0..MAX_VOICES)
                .map(|i| Voice::new(0x1357_9bdf ^ (i as u32).wrapping_mul(2_654_435_761)))
                .collect(),
            pos: vec![0.0; PARAM_COUNT],
            vals: vec![0.0; PARAM_COUNT],
            touched: Vec::with_capacity(PARAM_COUNT),
            lfos: Default::default(),
            lfo_now: [0.0; LFOS],
            wheel: 0.0,
            pressure: 0.0,
            bend: 0.0,
            pedal: false,
            held: Vec::with_capacity(128),
            age: 0,
            seed: 0xdead_beef,
            seen: 0,
            mix: [vec![0.0; MAX_FRAMES], vec![0.0; MAX_FRAMES]],
            down: std::array::from_fn(|_| [Down::new(), Down::new()]),
            tail: 0,
            arp_step: 0,
            arp_next: 0.0,
            arp_off: f64::MAX,
            arp_note: None,
            arp_swung: false,
            arp_up: true,
            newest: 0,
        }
    }

    pub fn set_tempo(&mut self, bpm: f32) {
        self.tempo = bpm.max(1.0);
    }

    /// The block's settings: `plan`'s, then automation (`auto`, positions), then modulators.
    pub fn begin(
        &mut self,
        plan: &PolyPlan,
        auto: impl Iterator<Item = (usize, f32)>,
        by: &Offsets,
    ) {
        let id = plan as *const PolyPlan as usize;
        let all = id != self.seen;
        if all {
            self.pos.copy_from_slice(&plan.pos);
            self.vals.copy_from_slice(&plan.settings.params.0);
            self.seen = id;
        }
        for &i in &self.touched {
            self.pos[i] = plan.pos[i];
            self.vals[i] = plan.settings.params[i];
        }
        self.touched.clear();
        for (i, p) in auto.chain(by.polys()) {
            if i < PARAM_COUNT {
                self.shift(plan, i, p);
            }
        }
        for &(from, i, depth) in &plan.macros {
            let turned = self.pos[from];
            if turned != 0.0 {
                self.shift(plan, i, turned * depth);
            }
        }
        // A voice's copy of what isn't moved within it follows.
        for v in self.voices.iter_mut().filter(|v| v.on) {
            v.values.copy_from_slice(&self.vals);
        }
    }

    /// Moves a parameter's knob this block.
    fn shift(&mut self, plan: &PolyPlan, i: usize, by: f32) {
        let was = if self.touched.contains(&i) {
            self.pos[i]
        } else {
            plan.pos[i]
        };
        // Automation sets the knob; a modulator's offset comes through negative or small.
        self.pos[i] = (was + by).clamp(0.0, 1.0);
        self.vals[i] = POLY_PARAMS[i].scale.value(self.pos[i]);
        if !self.touched.contains(&i) {
            self.touched.push(i);
        }
    }

    /// Each source as its newest voice has it now, by `live_slot`, and how many voices sound.
    pub fn live(&self, plan: &PolyPlan) -> [f32; POLY_LIVE] {
        let shared = self.shared(plan, self.rate, 0.0);
        let v = &self.voices[self.newest];
        let mut out = [0.0; POLY_LIVE];
        for (k, slot) in out[..POLY_LIVE - 1].iter_mut().enumerate() {
            let s = LIVE_SOURCES[k];
            *slot = match (v.on, s) {
                (false, PolySource::Lfo(_) | PolySource::Wheel | PolySource::Bend) => {
                    source(s, v, &shared)
                }
                (false, PolySource::Pressure) => self.pressure,
                (false, _) => 0.0,
                (true, _) => source(s, v, &shared),
            };
        }
        out[POLY_LIVE - 1] = self.voices.iter().filter(|v| v.on).count() as f32;
        out
    }

    /// What the synth's matrix does to the effects on its track, from its newest voice.
    pub fn outward(&self, plan: &PolyPlan, moved: &mut Offsets) {
        if plan.outward.is_empty() {
            return;
        }
        let shared = self.shared(plan, self.rate, 0.0);
        let v = &self.voices[self.newest];
        for &(src, by, target, depth) in &plan.outward {
            let scale = by.map_or(1.0, |b| source(b, v, &shared));
            moved.add(target, depth * source(src, v, &shared) * scale);
        }
    }

    fn shared<'a>(&'a self, plan: &'a PolyPlan, rate: f32, ms: f32) -> Shared<'a> {
        Shared {
            plan,
            pos: &self.pos,
            vals: &self.vals,
            wheel: self.wheel,
            pressure: self.pressure,
            bend: self.bend,
            rate,
            ms,
            tempo: self.tempo,
            global: &self.lfo_now,
        }
    }

    fn free_voice(&mut self, limit: usize) -> usize {
        let on = self.voices.iter().filter(|v| v.on).count();
        if on < limit
            && let Some(i) = self.voices.iter().position(|v| !v.on)
        {
            return i;
        }
        // The longest let go, else the oldest.
        let by = |v: &Voice| (v.on && v.held, v.age);
        (0..self.voices.len())
            .filter(|&i| self.voices[i].on)
            .min_by_key(|&i| by(&self.voices[i]))
            .unwrap_or(0)
    }

    fn start(&mut self, plan: &PolyPlan, note: u8, velocity: f32, legato: bool) {
        let s = &plan.settings;
        let limit = match s.voicing.mono {
            true => 1,
            false => (s.voicing.polyphony as usize).clamp(1, MAX_VOICES),
        };
        let i = match s.voicing.mono {
            true => self.voices.iter().position(|v| v.on).unwrap_or(0),
            false => self.free_voice(limit),
        };
        self.age += 1;
        self.newest = i;
        let age = self.age;
        let random = chance(&mut self.seed);
        let vals = &self.vals;
        let v = &mut self.voices[i];
        let was_on = v.on;
        let was = v.pitch;
        v.age = age;
        v.note = note;
        v.held = true;
        v.target = note as f32;
        // Glide from where it was, or only between held notes if that's how it's set.
        let slide = was_on && (!s.voicing.glide_held || legato);
        v.pitch = if slide && vals[POLY_GLIDE] > 0.2 {
            was
        } else {
            note as f32
        };
        if legato && was_on {
            return;
        }
        v.on = true;
        v.velocity = velocity;
        v.random = random;
        (v.tune, v.press, v.slide) = (0.0, 0.0, 0.0);
        v.values.copy_from_slice(vals);
        if !was_on {
            v.amp = 0.0;
            for f in v.filters.iter_mut().flatten() {
                f.reset();
            }
            for e in &mut v.envs {
                e.level = 0.0;
            }
        }
        for e in &mut v.envs {
            e.start();
        }
        for l in &mut v.lfos {
            l.start(0.0);
        }
        for o in 0..OSCS {
            let phase = vals[osc_param(o, OscField::Phase)];
            let loose = vals[osc_param(o, OscField::Random)];
            for u in 0..UNISON {
                let p = phase + loose * chance(&mut v.seed);
                v.oscs[o].phase[u] = ((p - p.floor()) * TURN) as u64 as u32;
            }
        }
    }

    fn stop(&mut self, note: u8) {
        for v in self
            .voices
            .iter_mut()
            .filter(|v| v.on && v.note == note && v.held)
        {
            v.held = false;
            for e in &mut v.envs {
                e.release();
            }
        }
    }

    fn note(&mut self, plan: &PolyPlan, n: &Note) {
        let s = &plan.settings;
        let on = n.velocity > 0.0;
        if on {
            self.held.retain(|&p| p != n.pitch);
            if self.held.len() < self.held.capacity() {
                self.held.push(n.pitch);
            }
        } else {
            self.held.retain(|&p| p != n.pitch);
        }
        if s.arp.mode != ArpMode::Off {
            if on && self.held.len() == 1 {
                // The first note starts it, on the step.
                self.arp_next = 0.0;
                self.arp_step = 0;
                self.arp_swung = false;
                self.arp_up = true;
            }
            if self.held.is_empty()
                && let Some(p) = self.arp_note.take()
            {
                self.stop(p);
            }
            return;
        }
        if s.voicing.mono {
            match on {
                true => {
                    let legato = s.voicing.legato && self.held.len() > 1;
                    self.start(plan, n.pitch, n.velocity, legato);
                }
                // Back to the note under it, if one's still held.
                false => match self.held.last().copied() {
                    Some(p) if s.voicing.legato => {
                        let v = self.voices[self.newest].velocity;
                        self.start(plan, p, v, true);
                    }
                    Some(p) => {
                        let v = self.voices[self.newest].velocity;
                        self.start(plan, p, v, false);
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
            // One note's own: every voice playing it (a unison stack, a glide) follows.
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

    /// The arpeggiator's next note from what's held.
    fn arp_pick(&mut self, plan: &PolyPlan) -> Option<u8> {
        let a = &plan.settings.arp;
        let mut notes = [0u8; 128];
        let n = self.held.len().min(128);
        notes[..n].copy_from_slice(&self.held[..n]);
        if n == 0 {
            return None;
        }
        if a.mode != ArpMode::Played {
            notes[..n].sort_unstable();
        }
        let octaves = (a.octaves as usize).clamp(1, 4);
        let total = n * octaves;
        let at = |k: usize| notes[k % n].saturating_add(12 * (k / n) as u8);
        let i = match a.mode {
            ArpMode::Down => total - 1 - self.arp_step % total,
            ArpMode::UpDown if total > 1 => {
                let p = self.arp_step % (2 * total - 2);
                if p < total { p } else { 2 * total - 2 - p }
            }
            ArpMode::Random => (chance(&mut self.seed) * total as f32) as usize % total,
            _ => self.arp_step % total,
        };
        self.arp_step += 1;
        Some(at(i).min(127))
    }

    /// Plays the arpeggiator on, `frames` samples into a stretch; samples until it next acts.
    fn arp(&mut self, plan: &PolyPlan) -> f64 {
        let a = plan.settings.arp;
        if a.mode == ArpMode::Off || self.held.is_empty() {
            return f64::MAX;
        }
        let step = (a.beats.max(0.01) * 60.0 / self.tempo * self.rate) as f64;
        if self.arp_off <= 0.0 {
            self.arp_off = f64::MAX;
            if let Some(p) = self.arp_note.take() {
                self.stop(p);
            }
        }
        if self.arp_next <= 0.0 {
            if let Some(p) = self.arp_note.take() {
                self.stop(p);
            }
            if a.mode == ArpMode::Chord {
                let n = self.held.len();
                for k in 0..n {
                    let p = self.held[k];
                    self.stop(p);
                    self.start(plan, p, 0.8, false);
                }
                self.arp_note = self.held.last().copied();
            } else if let Some(p) = self.arp_pick(plan) {
                self.start(plan, p, 0.8, false);
                self.arp_note = Some(p);
            }
            // Every other step late, and the one after it early to make up.
            let swung = a.swing.clamp(0.0, 0.75) as f64 * 0.5;
            let long = match self.arp_swung {
                true => step * (1.0 - swung),
                false => step * (1.0 + swung),
            };
            self.arp_swung = !self.arp_swung;
            self.arp_next += long;
            self.arp_off = long * a.gate.clamp(0.05, 1.0) as f64;
        }
        self.arp_next.min(self.arp_off)
    }

    /// A stretch of `n` samples of the song, into `mix` from `at`, at the running rate.
    fn stretch(&mut self, plan: &PolyPlan, at: usize, n: usize) {
        let times = plan.settings.quality.times();
        let rate = self.rate * times as f32;
        let (mut done, chunk) = (0, STEP);
        while done < n {
            let take = chunk.min(n - done);
            let ms = take as f32 * 1000.0 / self.rate;
            for &i in &plan.free_lfos {
                let s = &plan.settings.lfos[i];
                self.lfo_now[i] =
                    self.lfos[i].step(ms, i, s, &self.vals, self.tempo, &mut self.seed);
            }
            let out = take * times;
            let from = (at + done) * times;
            let mut voices = std::mem::take(&mut self.voices);
            let mut mix = std::mem::take(&mut self.mix);
            {
                let shared = self.shared(plan, rate, ms);
                for v in voices.iter_mut().filter(|v| v.on) {
                    v.control(&shared);
                    let [l, r] = &mut mix;
                    v.render(
                        &shared,
                        out,
                        [&mut l[from..from + out], &mut r[from..from + out]],
                    );
                    if v.env_now[0] <= 1e-5 && v.envs[0].stage == 0 {
                        v.on = false;
                    }
                }
            }
            self.voices = voices;
            self.mix = mix;
            done += take;
        }
    }

    pub fn process(
        &mut self,
        plan: &PolyPlan,
        buf: [&mut [f32]; 2],
        frames: usize,
        notes: &[Note],
        controls: &[Control],
    ) {
        let [l, r] = buf;
        let times = plan.settings.quality.times();
        let frames = frames
            .min(MAX_FRAMES / MOST_TIMES)
            .min(l.len())
            .min(r.len());
        // Nothing playing, held or ringing, and nothing moving: an idle synth costs nothing.
        let sounding =
            !notes.is_empty() || !self.held.is_empty() || self.voices.iter().any(|v| v.on);
        if sounding {
            self.tail = TAPS;
        } else if self.tail == 0 && plan.free_lfos.is_empty() {
            controls.iter().for_each(|c| self.control(c.kind));
            l[..frames].fill(0.0);
            r[..frames].fill(0.0);
            return;
        }
        self.tail = self.tail.saturating_sub(frames);
        self.mix[0][..frames * times].fill(0.0);
        self.mix[1][..frames * times].fill(0.0);
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
            let wait = self.arp(plan);
            let end = [
                next.peek().map(|n| n.offset as usize),
                moves.peek().map(|c| c.offset as usize),
                (wait < 1e12).then(|| at + wait.ceil().max(1.0) as usize),
            ]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(frames)
            .clamp(at + 1, frames);
            self.stretch(plan, at, end - at);
            let passed = (end - at) as f64;
            self.arp_next -= passed;
            if self.arp_off < 1e12 {
                self.arp_off -= passed;
            }
            at = end;
        }
        // Back down to the song's rate, and out.
        let gain = 10f32.powf(self.vals[POLY_MASTER] / 20.0);
        for (side, out) in [l, r].into_iter().enumerate() {
            let mut n = frames * times;
            let x = &mut self.mix[side];
            let mut stage = 0;
            while (1usize << stage) < times {
                n = self.down[side][stage].run(&mut x[..n]);
                stage += 1;
            }
            for (o, v) in out[..frames].iter_mut().zip(&x[..frames]) {
                *o = v * gain;
            }
        }
    }
}

#[cfg(test)]
mod tests;
