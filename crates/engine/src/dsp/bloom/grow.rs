//! Growing a seed to sound like a recording. What it hears of a sound is its loudness and a
//! spectrogram on a hearing-like scale, frame by frame. Four strands, each a population of seeds
//! drawn around a different kind, render their seeds playing the recording's note, hear them, and
//! breed the closest: crossed, a little changed, with strangers let in so a strand doesn't settle
//! too soon. Each new best a strand finds is kept, to be heard and planted.
//!
//! It all runs here, on the machine, a strand to a core: no network, nothing sent anywhere.

use std::{f32::consts::PI, sync::Arc};

use rustfft::{Fft, FftPlanner, num_complex::Complex};

use super::{BloomPlan, BloomSynth};
use crate::{plugin::Note, song::project::*};

/// The rate seeds are heard at while growing: what matters is under 12 kHz.
pub const GROW_RATE: f64 = 24_000.0;
/// The longest stretch of a recording heard, seconds.
pub const MOST_SECONDS: f32 = 2.0;
pub const BANDS: usize = 40;
const WINDOW: usize = 512;
const HOP: usize = 256;
/// How many seeds each strand holds, and how many generations it's given.
const POPULATION: usize = 28;
pub const GENERATIONS: usize = 150;
/// The most finds a strand keeps.
const FINDS: usize = 16;

/// A sound as it's heard: each frame's bands (dB, its loudest frame at 0) and loudness, and
/// how loud that loudest frame really was (dB), to hear several against each other.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Heard {
    pub frames: Vec<[f32; BANDS]>,
    pub level: Vec<f32>,
    pub top: f32,
}

/// Several sounds heard as loud as they are against each other: the loudest's top at 0, the
/// rest under it, so a softer one must be softer.
pub fn together(heard: &mut [Heard]) {
    let most = heard.iter().fold(-200f32, |m, h| m.max(h.top));
    for h in heard.iter_mut() {
        let under = h.top - most;
        if under == 0.0 {
            continue;
        }
        for (f, l) in h.frames.iter_mut().zip(&mut h.level) {
            *l = (*l + under).max(-80.0);
            for b in f.iter_mut() {
                *b = (*b + under).max(-80.0);
            }
        }
        h.top = most;
    }
}

/// What hears: a window, an FFT and the bands it's gathered into; and a synth kept to play seeds
/// on, cleared each time rather than made anew. One per thread.
pub struct Ear {
    synth: Box<BloomSynth>,
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    /// Each band's bins and their weights, and its middle, Hz.
    bank: Vec<Vec<(usize, f32)>>,
    middles: Vec<f32>,
    buf: Vec<Complex<f32>>,
}

fn mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

fn hz(mel: f32) -> f32 {
    700.0 * (10f32.powf(mel / 2595.0) - 1.0)
}

impl Ear {
    pub fn new() -> Ear {
        let fft = FftPlanner::<f32>::new().plan_fft_forward(WINDOW);
        let window = (0..WINDOW)
            .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / WINDOW as f32).cos())
            .collect();
        let (lo, hi) = (mel(40.0), mel(11_000.0));
        let edges: Vec<f32> = (0..BANDS + 2)
            .map(|i| hz(lo + (hi - lo) * i as f32 / (BANDS + 1) as f32))
            .collect();
        let bin = |f: f32| f * WINDOW as f32 / GROW_RATE as f32;
        let bank = (0..BANDS)
            .map(|b| {
                let (a, m, c) = (bin(edges[b]), bin(edges[b + 1]), bin(edges[b + 2]));
                let mut taps: Vec<(usize, f32)> = (a.floor() as usize..=c.ceil() as usize)
                    .filter(|&k| k < WINDOW / 2)
                    .map(|k| {
                        let x = k as f32;
                        let w = match x < m {
                            true => (x - a) / (m - a).max(1e-3),
                            false => (c - x) / (c - m).max(1e-3),
                        };
                        (k, w.max(0.0))
                    })
                    .filter(|t| t.1 > 0.0)
                    .collect();
                // A band too narrow for a bin of its own takes the nearest.
                if taps.is_empty() {
                    taps.push(((m.round() as usize).min(WINDOW / 2 - 1), 1.0));
                }
                taps
            })
            .collect();
        let middles = (0..BANDS).map(|b| edges[b + 1]).collect();
        Ear {
            synth: Box::new(BloomSynth::new(GROW_RATE)),
            middles,
            fft,
            window,
            bank,
            buf: vec![Complex::default(); WINDOW],
        }
    }

    /// `x` (at `GROW_RATE`) heard.
    pub fn hear(&mut self, x: &[f32]) -> Heard {
        let p = self.powers(x);
        finish(p)
    }

    /// Each frame's power in each band, as it is (not in dB).
    fn powers(&mut self, x: &[f32]) -> Vec<[f32; BANDS]> {
        let mut out = Vec::new();
        let mut at = 0;
        while at + WINDOW <= x.len().max(WINDOW) {
            for (i, c) in self.buf.iter_mut().enumerate() {
                *c = Complex::new(x.get(at + i).copied().unwrap_or(0.0) * self.window[i], 0.0);
            }
            self.fft.process(&mut self.buf);
            let mut bands = [0f32; BANDS];
            for (b, taps) in self.bank.iter().enumerate() {
                bands[b] = taps.iter().map(|&(k, w)| w * self.buf[k].norm_sqr()).sum();
            }
            out.push(bands);
            at += HOP;
        }
        out
    }

    /// A seed played (as `render`) and heard. Its echo and space are worked out as they'd
    /// spread its sound's power over time, band by band, rather than run: the seed's played
    /// dry, so hearing one costs a sixth of what it would. Close, not exact: what's tried is
    /// heard again properly before it's shown.
    pub fn hear_seed(&mut self, genes: &[f32; GENES], r: &Recording) -> Heard {
        let mut dry = *genes;
        (dry[44], dry[45]) = (0.0, 0.0);
        let audio = render_on(&mut self.synth, &dry, r);
        let mut p = self.powers(&audio);
        let (echo, space) = (genes[44] * genes[44], genes[45] * genes[45]);
        let frame_s = HOP as f32 / GROW_RATE as f32;
        let n = p.len();
        let mut wet = vec![[0f32; BANDS]; n];
        if space > 1e-4 {
            let size = genes[47];
            let decay = 0.6 * 14f32.powf(size);
            // A longer space gives more back: it rings on.
            let gain = SPACE_GAIN * decay * space * space;
            for b in 0..BANDS {
                // Highs die sooner, past the space's damping.
                let damp = 1.0 + 0.8 * ((self.middles[b] - 6500.0) / 6500.0).max(0.0);
                let fall = 10f32.powf(-6.0 * frame_s / decay * damp);
                let low = match self.middles[b] < 120.0 {
                    true => 0.3,
                    false => 1.0,
                };
                let mut tail = 0f32;
                for f in 0..n {
                    tail = tail * fall + p[f][b] * (1.0 - fall);
                    wet[f][b] += gain * low * tail;
                }
            }
        }
        if echo > 1e-4 {
            let k = ((genes[46] * ECHO_BEATS.len() as f32) as usize).min(ECHO_BEATS.len() - 1);
            // Its two sides, one at the time and one half again as long (at the song's 120).
            for side in [1.0f32, 1.5] {
                let gap = ((ECHO_BEATS[k] * side * 0.5 / frame_s).round() as usize).max(1);
                let gain = 0.5 * ECHO_GAIN * echo * echo;
                for b in 0..BANDS {
                    let keep = match self.middles[b] {
                        m if m > 7000.0 || m < 150.0 => 0.25,
                        _ => 1.0,
                    };
                    let mut each = gain * keep;
                    let mut d = gap;
                    while d < n && each > 1e-6 {
                        for f in d..n {
                            wet[f][b] += each * p[f - d][b];
                        }
                        each *= 0.42 * 0.42 * keep;
                        d += gap;
                    }
                }
            }
        }
        for (f, w) in p.iter_mut().zip(&wet) {
            for (x, y) in f.iter_mut().zip(w) {
                *x += y;
            }
        }
        finish(p)
    }
}

/// How loud the space (a second of it) and echo come back for a send of one, against what
/// went in (power):
/// measured against the real ones by `the_quick_space_is_near_the_real_one`.
const SPACE_GAIN: f32 = 0.37;
const ECHO_GAIN: f32 = 0.41;

/// Powers made into what's heard: dB, the loudest frame at 0, nothing under -80.
fn finish(powers: Vec<[f32; BANDS]>) -> Heard {
    let mut out = Heard::default();
    for p in powers {
        let energy: f32 = p.iter().sum();
        out.frames.push(p.map(|x| 10.0 * (x + 1e-10).log10()));
        out.level.push(10.0 * (energy + 1e-10).log10());
    }
    let top = out.level.iter().fold(-200f32, |m, l| m.max(*l));
    for (f, l) in out.frames.iter_mut().zip(&mut out.level) {
        *l = (*l - top).max(-80.0);
        for b in f.iter_mut() {
            *b = (*b - top).max(-80.0);
        }
    }
    out.top = top;
    out
}

impl Default for Ear {
    fn default() -> Self {
        Ear::new()
    }
}

/// How far apart two sounds are heard: their spectra frame by frame, their loudness twice over,
/// in tens of dB. Quiet frames (both under -50 dB) count less, so a tail's noise floor doesn't.
pub fn distance(a: &Heard, b: &Heard) -> f32 {
    let n = a.frames.len().min(b.frames.len());
    if n == 0 {
        return f32::MAX;
    }
    let (mut sum, mut weight) = (0f32, 0f32);
    for i in 0..n {
        let loud = a.level[i].max(b.level[i]);
        let w = match loud > -50.0 {
            true => 1.0,
            false => 0.25,
        };
        let spectrum: f32 = a.frames[i]
            .iter()
            .zip(&b.frames[i])
            .map(|(x, y)| (x - y).abs())
            .sum::<f32>()
            / BANDS as f32;
        sum += w * (spectrum + 2.0 * (a.level[i] - b.level[i]).abs()) / 10.0;
        weight += w;
    }
    // The whole sound's colour too: smoother to climb than frame by frame.
    let whole = |h: &Heard| -> [f32; BANDS] {
        let mut out = [0f32; BANDS];
        let mut total = 0f32;
        for (f, l) in h.frames[..n].iter().zip(&h.level) {
            let e = 10f32.powf(l / 10.0);
            total += e;
            for (o, b) in out.iter_mut().zip(f) {
                *o += e * 10f32.powf(b / 10.0);
            }
        }
        out.map(|x| 10.0 * (x / total.max(1e-12) + 1e-10).log10())
    };
    let (wa, wb) = (whole(a), whole(b));
    let colour = wa
        .iter()
        .zip(&wb)
        .map(|(x, y)| (x - y).abs().min(40.0))
        .sum::<f32>()
        / BANDS as f32
        / 10.0;
    0.75 * sum / weight.max(1e-6) + 0.5 * colour
}

/// A distance as a match, 0 to 1.
pub fn closeness(d: f32) -> f32 {
    (-d / 2.2).exp()
}

/// A stretch of a recording to grow toward: mono at `GROW_RATE`, the note it's played at, and
/// how long it's held before it's let go.
#[derive(Clone, Debug)]
pub struct Recording {
    pub audio: Vec<f32>,
    pub note: u8,
    /// The pitch it was heard at, if it has one.
    pub hz: Option<f32>,
    pub hold: usize,
    /// How hard it's played: the loudest of several hardest.
    pub velocity: f32,
}

/// A whole recording, its channels mixed to one, at `GROW_RATE`.
pub fn read_recording(path: &std::path::Path) -> Result<Vec<f32>, String> {
    let (sides, rate) = crate::audio::engine::read_audio(path)?;
    let n = sides.iter().map(Vec::len).max().unwrap_or(0);
    let mono: Vec<f32> = (0..n)
        .map(|i| {
            sides
                .iter()
                .map(|s| s.get(i).copied().unwrap_or(0.0))
                .sum::<f32>()
                / sides.len().max(1) as f32
        })
        .collect();
    Ok(crate::audio::engine::resample(&mono, rate, GROW_RATE))
}

/// The stretch of `audio` (at `GROW_RATE`) from `from` seconds, `long` seconds at most `MOST_SECONDS`:
/// its pitch found, and where it's let go.
pub fn recording(audio: &[f32], from: f32, long: f32) -> Recording {
    let rate = GROW_RATE as f32;
    let start = ((from.max(0.0) * rate) as usize).min(audio.len());
    let len = ((long.clamp(0.1, MOST_SECONDS) * rate) as usize).min(audio.len() - start);
    let piece = audio[start..start + len].to_vec();
    let hz = pitch_of(&piece);
    let note = hz.map_or(60, |f| {
        (69.0 + 12.0 * (f / 440.0).log2())
            .round()
            .clamp(12.0, 108.0) as u8
    });
    // Let go where it's fallen 20 dB under its loudest, or most of the way through.
    let hops: Vec<f32> = piece
        .chunks(HOP)
        .map(|c| (c.iter().map(|x| x * x).sum::<f32>() / c.len().max(1) as f32).sqrt())
        .collect();
    let peak = hops.iter().fold(0f32, |m, x| m.max(*x));
    let at_peak = hops.iter().position(|x| *x == peak).unwrap_or(0);
    let fall = hops
        .iter()
        .enumerate()
        .skip(at_peak)
        .find(|(_, x)| **x < peak * 0.1)
        .map_or(hops.len(), |(i, _)| i);
    let hold = ((fall * HOP) as f32 * 0.9).max(rate * 0.05) as usize;
    Recording {
        audio: piece,
        note,
        hz,
        hold: hold.min(len.max(1)),
        velocity: 0.9,
    }
}

/// Several recordings to grow toward at once (at different notes, or played softer and harder):
/// each played as hard as it's loud against the loudest.
pub fn played_together(rs: &mut [Recording]) {
    let rms = |r: &Recording| {
        (r.audio.iter().map(|x| x * x).sum::<f32>() / r.audio.len().max(1) as f32).sqrt()
    };
    let most = rs.iter().map(rms).fold(1e-9f32, f32::max);
    for r in rs.iter_mut() {
        r.velocity = (0.95 * (rms(r) / most).sqrt()).clamp(0.3, 0.95);
    }
}

/// The pitch of `x`, if it has a clear one: the difference function's first deep dip (after de
/// Cheveigné and Kawahara's YIN), over its loudest 80 ms.
pub fn pitch_of(x: &[f32]) -> Option<f32> {
    let rate = GROW_RATE as f32;
    let n = (0.08 * rate) as usize;
    if x.len() < n + (rate / 30.0) as usize {
        return None;
    }
    let max_lag = (rate / 30.0) as usize;
    let min_lag = (rate / 2000.0) as usize;
    // The loudest stretch, past the attack.
    let step = n / 2;
    let start = (0..x.len().saturating_sub(n + max_lag))
        .step_by(step.max(1))
        .max_by(|&a, &b| {
            let e = |s: usize| x[s..s + n].iter().map(|v| v * v).sum::<f32>();
            e(a).total_cmp(&e(b))
        })?;
    let w = &x[start..start + n + max_lag];
    let mut d = vec![0f32; max_lag + 1];
    for lag in 1..=max_lag {
        d[lag] = (0..n).map(|i| (w[i] - w[i + lag]).powi(2)).sum();
    }
    // Cumulative mean normalised.
    let mut sum = 0f32;
    let mut cm = vec![1f32; max_lag + 1];
    for lag in 1..=max_lag {
        sum += d[lag];
        cm[lag] = d[lag] * lag as f32 / sum.max(1e-12);
    }
    let lag = (min_lag..max_lag).find(|&l| cm[l] < 0.15 && cm[l] <= cm[l + 1])?;
    // Between samples, by a parabola through its neighbours.
    let (a, b, c) = (cm[lag - 1], cm[lag], cm[lag + 1]);
    let shift = 0.5 * (a - c) / (a - 2.0 * b + c).max(1e-9);
    Some(rate / (lag as f32 + shift.clamp(-1.0, 1.0)))
}

/// A seed playing `r`'s note as long as `r` holds it: what it makes, mono, as long as `r`.
pub fn render(genes: &[f32; GENES], r: &Recording) -> Vec<f32> {
    render_on(&mut BloomSynth::new(GROW_RATE), genes, r)
}

fn render_on(s: &mut BloomSynth, genes: &[f32; GENES], r: &Recording) -> Vec<f32> {
    s.reset();
    let mut b = Bloom::default();
    b.set_seed(genes);
    for k in 0..BRANCHES {
        b.params[branch_length(k)] = 0.0;
    }
    b.keys = BloomKeys::One;
    b.quality = PolyQuality::Draft;
    let plan = BloomPlan::new(&b);
    let len = r.audio.len();
    let (mut l, mut rr) = (vec![0f32; len], vec![0f32; len]);
    let mut at = 0;
    while at < len {
        let n = 256.min(len - at);
        let mut notes = Vec::new();
        if at == 0 {
            notes.push(Note {
                offset: 0,
                pitch: r.note,
                velocity: r.velocity,
                release: 0.0,
            });
        }
        if at <= r.hold && r.hold < at + n {
            notes.push(Note {
                offset: (r.hold - at) as u32,
                pitch: r.note,
                velocity: 0.0,
                release: 0.0,
            });
        }
        s.begin(&plan, std::iter::empty());
        s.process(
            &plan,
            [&mut l[at..at + n], &mut rr[at..at + n]],
            n,
            &notes,
            &[],
        );
        at += n;
    }
    l.iter().zip(&rr).map(|(a, b)| 0.5 * (a + b)).collect()
}

/// Something a strand found: the seed, how closely it matches, and the generation it came in.
#[derive(Clone, Debug, PartialEq)]
pub struct Find {
    pub genes: [f32; GENES],
    pub close: f32,
    pub generation: usize,
}

/// How many generations a strand explores, breeding, before it narrows in on its best.
const EXPLORE: usize = 20;

/// One strand: its kind, its seeds as last heard (closest first), what it's found. It explores
/// as a population breeding, then narrows in on its best by an evolution strategy that learns
/// how far, and along which genes, to step (Hansen's CMA-ES, its covariance kept diagonal, as
/// Ros and Hansen's separable form).
pub struct Strand {
    pub kind: SeedKind,
    pop: Vec<(f32, [f32; GENES])>,
    pub finds: Vec<Find>,
    luck: u32,
    best: (f32, [f32; GENES]),
    narrow: Option<Narrowing>,
}

/// The evolution strategy's state: its mean, step, each gene's spread, and its two paths.
struct Narrowing {
    mean: [f32; GENES],
    sigma: f32,
    spread: [f32; GENES],
    ps: [f32; GENES],
    pc: [f32; GENES],
    /// Each sample's step from the mean, in the mean's own spread (z), as last drawn.
    steps: Vec<[f32; GENES]>,
    generation: usize,
}

fn chance(luck: &mut u32) -> f32 {
    *luck = mix32(luck.wrapping_add(0x9e37_79b9));
    (*luck >> 8) as f32 / (1u32 << 24) as f32
}

/// A normal draw, by Box and Muller.
fn normal(luck: &mut u32) -> f32 {
    let (u, v) = (chance(luck).max(1e-7), chance(luck));
    (-2.0 * u.ln()).sqrt() * (std::f32::consts::TAU * v).cos()
}

/// The genes the strategy moves: not the whole plant's.
fn free(i: usize) -> bool {
    !GENE_TABLE[i].plant
}

impl Strand {
    fn new(kind: SeedKind, luck: u32) -> Strand {
        let mut l = luck;
        let pop = (0..POPULATION)
            .map(|_| (f32::MAX, seed_of(kind, mix32(chance(&mut l).to_bits()))))
            .collect();
        Strand {
            kind,
            pop,
            finds: Vec::new(),
            luck: l,
            best: (f32::MAX, NEUTRAL),
            narrow: None,
        }
    }

    /// One generation: the unheard heard, the best kept, the next drawn.
    fn evolve(&mut self, ear: &mut Ear, targets: &[Heard], rs: &[Recording], generation: usize) {
        for (score, genes) in self.pop.iter_mut().filter(|p| p.0 == f32::MAX) {
            *score = apart(ear, genes, targets, rs);
        }
        // The strategy learns from its samples in the order it drew them, before they're sorted.
        let drawn: Vec<(f32, [f32; GENES])> = self.pop.clone();
        self.pop.sort_by(|a, b| a.0.total_cmp(&b.0));
        if self.pop[0].0 < self.best.0 {
            self.best = self.pop[0];
        }
        let best = self.best;
        if self
            .finds
            .last()
            .is_none_or(|f| closeness(best.0) > f.close + 0.002)
        {
            self.finds.push(Find {
                genes: best.1,
                close: closeness(best.0),
                generation,
            });
            if self.finds.len() > FINDS {
                self.finds.remove(1);
            }
        }
        match generation + 1 < EXPLORE {
            true => self.breed(generation),
            false => self.narrow_in(drawn),
        }
    }

    /// The next generation bred: the closest kept, crossed and changed, strangers let in.
    fn breed(&mut self, generation: usize) {
        let late = generation as f32 / EXPLORE as f32;
        let spread = 0.22 * (1.0 - late) + 0.05;
        let keep = POPULATION / 4;
        let mut next: Vec<(f32, [f32; GENES])> = self.pop[..keep].to_vec();
        while next.len() < POPULATION {
            let l = &mut self.luck;
            let pick = |l: &mut u32| {
                // The closer of two at random.
                let (a, b) = (
                    (chance(l) * keep as f32 * 2.0) as usize,
                    (chance(l) * POPULATION as f32) as usize,
                );
                a.min(b).min(POPULATION - 1)
            };
            let child = match next.len() >= POPULATION - 2 {
                // Strangers, so it doesn't settle too soon.
                true => seed_of(self.kind, mix32(chance(l).to_bits())),
                false => {
                    let (a, b) = (pick(l), pick(l));
                    let mut c = crossed(&self.pop[a].1, &self.pop[b].1, mix32(chance(l).to_bits()));
                    for (i, g) in c.iter_mut().enumerate() {
                        if free(i) && chance(l) < 0.18 {
                            *g = reflect(*g + spread * (2.0 * chance(l) - 1.0));
                        }
                    }
                    c
                }
            };
            next.push((f32::MAX, child));
        }
        self.pop = next;
    }

    /// The strategy's step: its mean moved toward the closest half of what it drew, its step
    /// and spreads learnt from how they went, and the next generation drawn round it.
    fn narrow_in(&mut self, drawn: Vec<(f32, [f32; GENES])>) {
        let n = (0..GENES).filter(|&i| free(i)).count() as f32;
        let lambda = POPULATION;
        let mu = lambda / 2;
        let raw: Vec<f32> = (0..mu)
            .map(|i| ((mu as f32 + 0.5).ln() - ((i + 1) as f32).ln()).max(0.0))
            .collect();
        let total: f32 = raw.iter().sum();
        let w: Vec<f32> = raw.iter().map(|x| x / total).collect();
        let mueff = 1.0 / w.iter().map(|x| x * x).sum::<f32>();
        let cs = (mueff + 2.0) / (n + mueff + 5.0);
        let ds = 1.0 + 2.0 * (((mueff - 1.0) / (n + 1.0)).sqrt() - 1.0).max(0.0) + cs;
        let cc = (4.0 + mueff / n) / (n + 4.0 + 2.0 * mueff / n);
        let sep = (n + 2.0) / 3.0;
        let c1 = (2.0 / ((n + 1.3).powi(2) + mueff) * sep).min(0.9);
        let cmu =
            (2.0 * (mueff - 2.0 + 1.0 / mueff) / ((n + 2.0).powi(2) + mueff) * sep).min(1.0 - c1);
        let chi = n.sqrt() * (1.0 - 1.0 / (4.0 * n) + 1.0 / (21.0 * n * n));
        let started = self.narrow.is_none();
        let st = self.narrow.get_or_insert_with(|| Narrowing {
            mean: self.best.1,
            sigma: 0.12,
            spread: [1.0; GENES],
            ps: [0.0; GENES],
            pc: [0.0; GENES],
            steps: Vec::new(),
            generation: 0,
        });
        if !started && st.steps.len() == drawn.len() {
            // Ranked by how close each came, with the step each was drawn by.
            let mut order: Vec<usize> = (0..drawn.len()).collect();
            order.sort_by(|&a, &b| drawn[a].0.total_cmp(&drawn[b].0));
            let old = st.mean;
            let mut zw = [0f32; GENES];
            let mut mean = [0f32; GENES];
            for (k, &i) in order.iter().take(mu).enumerate() {
                for g in 0..GENES {
                    mean[g] += w[k] * drawn[i].1[g];
                    zw[g] += w[k] * st.steps[i][g];
                }
            }
            for g in (0..GENES).filter(|&g| free(g)) {
                st.mean[g] = mean[g].clamp(0.0, 1.0);
                st.ps[g] = (1.0 - cs) * st.ps[g] + (cs * (2.0 - cs) * mueff).sqrt() * zw[g];
            }
            let norm = st.ps.iter().map(|x| x * x).sum::<f32>().sqrt();
            st.generation += 1;
            let h = norm / (1.0 - (1.0 - cs).powi(2 * st.generation as i32)).sqrt() / chi
                < 1.4 + 2.0 / (n + 1.0);
            for g in (0..GENES).filter(|&g| free(g)) {
                let moved = (st.mean[g] - old[g]) / st.sigma;
                st.pc[g] = (1.0 - cc) * st.pc[g]
                    + if h {
                        (cc * (2.0 - cc) * mueff).sqrt() * moved
                    } else {
                        0.0
                    };
                let rank: f32 = order
                    .iter()
                    .take(mu)
                    .enumerate()
                    .map(|(k, &i)| w[k] * ((drawn[i].1[g] - old[g]) / st.sigma).powi(2))
                    .sum();
                let c = st.spread[g] * st.spread[g];
                let c = (1.0 - c1 - cmu) * c + c1 * st.pc[g] * st.pc[g] + cmu * rank;
                st.spread[g] = c.max(1e-6).sqrt().min(4.0);
            }
            st.sigma = (st.sigma * ((cs / ds) * (norm / chi - 1.0)).exp()).clamp(0.005, 0.4);
            // Settled: it starts again from its best, wide, to look past where it's stuck.
            if st.sigma < 0.02 {
                (st.mean, st.sigma) = (self.best.1, 0.15);
                (st.spread, st.ps, st.pc, st.generation) =
                    ([1.0; GENES], [0.0; GENES], [0.0; GENES], 0);
            }
        }
        // The next generation, round the mean; the best so far always among them.
        let l = &mut self.luck;
        let mut pop = Vec::with_capacity(lambda);
        let mut steps = Vec::with_capacity(lambda);
        for k in 0..lambda {
            let mut z = [0f32; GENES];
            let mut x = st.mean;
            if k == 0 {
                x = self.best.1;
                for g in (0..GENES).filter(|&g| free(g)) {
                    z[g] = (x[g] - st.mean[g]) / (st.sigma * st.spread[g]);
                }
            } else {
                for g in (0..GENES).filter(|&g| free(g)) {
                    z[g] = normal(l);
                    let v = st.mean[g] + st.sigma * st.spread[g] * z[g];
                    x[g] = reflect(v);
                    // What it was moved to, as a step, so the strategy learns what was heard.
                    z[g] = (x[g] - st.mean[g]) / (st.sigma * st.spread[g]);
                }
            }
            pop.push((f32::MAX, x));
            steps.push(z);
        }
        st.steps = steps;
        self.pop = pop;
    }
}

/// How far a seed is from recordings `rs` (heard as `targets`): each played at its note and as
/// hard, heard against the others, and the distances averaged.
pub fn apart(ear: &mut Ear, genes: &[f32; GENES], targets: &[Heard], rs: &[Recording]) -> f32 {
    let mut heard: Vec<Heard> = rs.iter().map(|r| ear.hear_seed(genes, r)).collect();
    together(&mut heard);
    let total: f32 = targets
        .iter()
        .zip(&heard)
        .map(|(t, h)| distance(t, h))
        .sum();
    total / targets.len().max(1) as f32
}

/// The most recordings grown toward at once.
pub const MOST_RECORDINGS: usize = 4;

/// Four strands growing toward one recording or several.
pub struct Grower {
    pub recordings: Vec<Recording>,
    pub targets: Vec<Heard>,
    pub strands: Vec<Strand>,
    pub generation: usize,
}

/// The kinds a recording's strands are drawn around: what its shape suggests, and anything.
pub fn kinds_for(r: &Recording) -> [SeedKind; 4] {
    use SeedKind::*;
    let secs = r.hold as f32 / GROW_RATE as f32;
    let low = r.note < 48;
    // Bright and unpitched is a hat; low and unpitched a kick.
    let bright = r.note >= 72;
    // Struck or plucked: it falls away while it's still held, however long it rings.
    let level = |from: usize, n: usize| {
        let x = &r.audio[from.min(r.audio.len())..(from + n).min(r.audio.len())];
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    };
    let n = (0.04 * GROW_RATE) as usize;
    let peak = (0..r.hold.max(1))
        .step_by(n / 2)
        .map(|i| level(i, n))
        .fold(0f32, f32::max);
    let late = level((r.hold as f32 * 0.8) as usize, n);
    let falls = late < peak * 0.5;
    match (r.hz.is_some(), secs < 0.35 || falls, low) {
        (false, true, true) => [Kick, Perc, Snare, Any],
        (false, true, false) if bright => [Hat, Snare, Perc, Any],
        (false, true, false) => [Snare, Perc, Zap, Any],
        (false, false, _) => [Texture, Atmosphere, Riser, Any],
        (true, true, true) => [Bass, Pluck, Acid, Any],
        (true, true, false) if secs >= 0.35 => [Bell, Pluck, Keys, Mallet],
        (true, true, false) => [Pluck, Bell, Mallet, Any],
        (true, false, true) => [Bass, Sub, Reese, Any],
        (true, false, false) => [Pad, Lead, Strings, Any],
    }
}

impl Grower {
    /// Growing toward `recordings` (at most `MOST_RECORDINGS`, one at least), drawn around the
    /// kinds the first suggests.
    pub fn new(mut recordings: Vec<Recording>, luck: u32) -> Grower {
        recordings.truncate(MOST_RECORDINGS);
        played_together(&mut recordings);
        let mut ear = Ear::new();
        let mut targets: Vec<Heard> = recordings.iter().map(|r| ear.hear(&r.audio)).collect();
        together(&mut targets);
        let kinds = recordings.first().map_or([SeedKind::Any; 4], kinds_for);
        let strands = kinds
            .iter()
            .enumerate()
            .map(|(i, &k)| Strand::new(k, mix32(luck ^ (i as u32 * 0x51ed))))
            .collect();
        Grower {
            recordings,
            targets,
            strands,
            generation: 0,
        }
    }

    pub fn done(&self) -> bool {
        self.generation >= GENERATIONS
    }

    /// A generation, every strand on a core of its own, `ears` one each.
    pub fn step(&mut self, ears: &mut [Ear]) {
        let (target, r, g) = (&self.targets[..], &self.recordings[..], self.generation);
        std::thread::scope(|s| {
            for (strand, ear) in self.strands.iter_mut().zip(ears.iter_mut()) {
                s.spawn(move || strand.evolve(ear, target, r, g));
            }
        });
        self.generation += 1;
    }

    /// The closest find of all.
    pub fn best(&self) -> Option<&Find> {
        self.strands
            .iter()
            .flat_map(|s| s.finds.last())
            .max_by(|a, b| a.close.total_cmp(&b.close))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ringing_bell_is_grown_from_struck_kinds_and_a_pad_from_held() {
        let make = |kind, note| {
            let mut b = Bloom::default();
            b.plant(kind, 7);
            let r = Recording {
                audio: vec![0.0; 36_000],
                note,
                hz: Some(330.0),
                hold: 20_000,
                velocity: 0.9,
            };
            Recording {
                audio: render(&b.seed(), &r),
                ..r
            }
        };
        let mut bell = make(SeedKind::Bell, 64);
        bell.hold = 20_000;
        assert!(
            kinds_for(&bell).contains(&SeedKind::Bell),
            "{:?}",
            kinds_for(&bell)
        );
        let mut pad = make(SeedKind::Pad, 64);
        // A pad that's fully in by the time it's measured.
        pad.audio = (0..36_000)
            .map(|i| 0.3 * (i as f32 * 0.086).sin())
            .collect();
        assert!(
            kinds_for(&pad).contains(&SeedKind::Pad),
            "{:?}",
            kinds_for(&pad)
        );
    }

    #[test]
    fn a_pitch_is_heard_where_it_is() {
        let x: Vec<f32> = (0..24_000)
            .map(|i| (2.0 * PI * 220.0 * i as f32 / GROW_RATE as f32).sin() * 0.5)
            .collect();
        let hz = pitch_of(&x).unwrap();
        assert!((hz - 220.0).abs() < 1.0, "{hz}");
        let r = recording(&x, 0.0, 1.0);
        assert_eq!(r.note, 57);
        let mut luck = 5u32;
        let noise: Vec<f32> = (0..24_000).map(|_| chance(&mut luck) - 0.5).collect();
        assert_eq!(pitch_of(&noise), None);
    }

    #[test]
    fn several_recordings_are_heard_against_each_other_and_grown_toward_together() {
        let mut b = Bloom::default();
        b.plant(SeedKind::Keys, 21);
        let genes = b.seed();
        let take = |note: u8, velocity: f32| {
            let r = Recording {
                audio: vec![0.0; 12_000],
                note,
                hz: None,
                hold: 7000,
                velocity,
            };
            Recording {
                audio: render(&genes, &r),
                ..r
            }
        };
        let mut rs = vec![take(48, 0.9), take(72, 0.5)];
        // Heard against each other: the loudest frame of all at 0, the other's no louder than it
        // really was.
        played_together(&mut rs);
        assert!(rs.iter().all(|r| (0.3..=0.95).contains(&r.velocity)));
        let mut ear = Ear::new();
        let mut heard: Vec<Heard> = rs.iter().map(|r| ear.hear(&r.audio)).collect();
        let (tops, alone) = ([heard[0].top, heard[1].top], heard.clone());
        together(&mut heard);
        let peak = |h: &Heard| h.level.iter().fold(-80f32, |m, l| m.max(*l));
        let (loud, soft) = if tops[0] >= tops[1] { (0, 1) } else { (1, 0) };
        assert!(peak(&heard[loud]).abs() < 1e-3);
        assert!((peak(&heard[soft]) - (tops[soft] - tops[loud])).abs() < 0.5);
        assert_eq!(heard[loud], alone[loud]);
        // The seed that made them is nearer them than another; growing gets no further off.
        let targets = heard;
        let mine = apart(&mut ear, &genes, &targets, &rs);
        let other = apart(&mut ear, &seed_of(SeedKind::Drone, 4), &targets, &rs);
        assert!(mine < other, "{mine} {other}");
        let mut g = Grower::new(rs, 9);
        assert_eq!(g.targets.len(), 2);
        let mut ears: Vec<Ear> = (0..4).map(|_| Ear::new()).collect();
        g.step(&mut ears);
        let first = g.best().unwrap().close;
        for _ in 0..6 {
            g.step(&mut ears);
        }
        assert!(g.best().unwrap().close >= first);
    }

    #[test]
    fn a_sound_is_nearest_itself_and_growing_gets_closer() {
        let mut ear = Ear::new();
        let mut b = Bloom::default();
        b.plant(SeedKind::Pluck, 42);
        let genes = b.seed();
        // A recording made by a seed, so a perfect match exists.
        let made = Recording {
            audio: vec![0.0; 12_000],
            note: 60,
            hz: Some(261.6),
            hold: 6000,
            velocity: 0.9,
        };
        let audio = render(&genes, &made);
        let r = Recording { audio, ..made };
        let heard = ear.hear(&r.audio);
        assert!(distance(&heard, &heard) < 1e-6);
        let other = seed_of(SeedKind::Pad, 9);
        assert!(distance(&heard, &ear.hear(&render(&other, &r))) > 0.5);
        let mut g = Grower::new(vec![r], 3);
        let mut ears: Vec<Ear> = (0..4).map(|_| Ear::new()).collect();
        g.step(&mut ears);
        let first = g.best().unwrap().close;
        for _ in 0..12 {
            g.step(&mut ears);
        }
        let later = g.best().unwrap().close;
        assert!(later >= first, "{first} then {later}");
        assert!(g.strands.iter().all(|s| !s.finds.is_empty()));
    }
}

/// How close growing gets to sounds a seed made (so a perfect match exists): run with --ignored.
#[test]
#[ignore]
fn how_close_it_gets() {
    let mut total = 0.0;
    let kinds = [
        SeedKind::Pluck,
        SeedKind::Bell,
        SeedKind::Bass,
        SeedKind::Pad,
        SeedKind::Perc,
        SeedKind::Keys,
    ];
    let t = std::time::Instant::now();
    for (i, k) in kinds.iter().enumerate() {
        let made = seed_of(*k, 1000 + i as u32 * 77);
        let r0 = Recording {
            audio: vec![0.0; 36_000],
            note: 52 + i as u8 * 3,
            hz: None,
            hold: 14_000,
            velocity: 0.9,
        };
        let audio = render(&made, &r0);
        let r = recording(&audio, 0.0, 1.5);
        let mut g = Grower::new(vec![r], 5 + i as u32);
        let mut ears: Vec<Ear> = (0..4).map(|_| Ear::new()).collect();
        while !g.done() {
            g.step(&mut ears);
        }
        let c = g.best().unwrap().close;
        total += c;
        eprintln!("{k:?} at {}: {:.0}%", g.recordings[0].note, c * 100.0);
    }
    eprintln!(
        "mean {:.0}%, {:?}",
        total / kinds.len() as f32 * 100.0,
        t.elapsed()
    );
}

#[test]
#[ignore]
fn what_a_render_costs() {
    let r = Recording {
        audio: vec![0.0; 36_000],
        note: 60,
        hz: None,
        hold: 14_000,
        velocity: 0.9,
    };
    let mut ear = Ear::new();
    for k in [SeedKind::Pluck, SeedKind::Pad, SeedKind::Texture] {
        let g = seed_of(k, 3);
        let t = std::time::Instant::now();
        let mut audio = Vec::new();
        for _ in 0..20 {
            ear.synth.reset();
            audio = render_on(&mut ear.synth, &g, &r);
        }
        let render = t.elapsed() / 20;
        let t = std::time::Instant::now();
        for _ in 0..20 {
            ear.hear(&audio);
        }
        eprintln!("{k:?}: render {render:?}, hear {:?}", t.elapsed() / 20);
    }
    let mut dry = seed_of(SeedKind::Pad, 3);
    dry[44] = 0.0;
    dry[45] = 0.0;
    let t = std::time::Instant::now();
    for _ in 0..20 {
        render_on(&mut ear.synth, &dry, &r);
    }
    eprintln!("Pad without echo or space: {:?}", t.elapsed() / 20);
}

/// The quick space and echo against the real ones, for seeds sending only to them.
#[test]
#[ignore]
fn the_quick_space_is_near_the_real_one() {
    let r = Recording {
        audio: vec![0.0; 36_000],
        note: 60,
        hz: None,
        hold: 8_000,
        velocity: 0.9,
    };
    let mut ear = Ear::new();
    for (kind, send) in [
        (SeedKind::Pluck, 45),
        (SeedKind::Pad, 45),
        (SeedKind::Pluck, 44),
        (SeedKind::Keys, 44),
    ] {
        let mut g = seed_of(kind, 9);
        (g[44], g[45]) = (0.0, 0.0);
        g[send] = 0.9;
        let real = ear.hear(&render(&g, &r));
        let quick = ear.hear_seed(&g, &r);
        let sum = |h: &Heard| h.level.iter().map(|l| 10f32.powf(l / 10.0)).sum::<f32>();
        eprintln!(
            "{kind:?} send {send}: {:.0}% alike, power real/quick {:.2}",
            closeness(distance(&real, &quick)) * 100.0,
            sum(&real) / sum(&quick)
        );
    }
}

#[test]
#[ignore]
fn what_the_space_and_echo_give_back() {
    let r = Recording {
        audio: vec![0.0; 48_000],
        note: 60,
        hz: None,
        hold: 8_000,
        velocity: 0.9,
    };
    let mut ear = Ear::new();
    for (kind, send, size) in [
        (SeedKind::Pluck, 45, 0.2),
        (SeedKind::Pluck, 45, 0.8),
        (SeedKind::Pad, 45, 0.5),
        (SeedKind::Pluck, 44, 0.5),
        (SeedKind::Keys, 44, 0.5),
    ] {
        let mut g = seed_of(kind, 9);
        (g[44], g[45]) = (0.0, 0.0);
        g[47] = size;
        let dry: f32 = ear.powers(&render(&g, &r)).iter().flatten().sum();
        g[send] = 1.0;
        let all: f32 = ear.powers(&render(&g, &r)).iter().flatten().sum();
        eprintln!(
            "{kind:?} send {send} size {size}: wet/dry power {:.3}",
            (all - dry) / dry
        );
    }
}
