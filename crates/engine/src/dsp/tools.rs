//! The stacked tools, running: a filter swept by an LFO and the sound's own level; chorus,
//! ensemble, flanger, phaser, vibrato and tremolo; a transient shaper; a gate; a de-esser; a
//! stereo tool; a multiband compressor. Each is made off the audio thread and allocates nothing
//! as it runs.

use std::f32::consts::{PI, TAU};

use super::{
    Clock, Running,
    comp::ftz,
    delay::knee,
    reverb::{Biquad, Line},
    sampler::{
        filter::{COMB, Filter, coefs},
        modulation::wave,
    },
};
use crate::song::project::{MotionStyle, Multiband, Unit, Wave};

/// How many samples a moving cutoff or sweep holds before it's worked out again.
const CHUNK: usize = 16;
/// The longest a motion's delay reaches, s.
const MOST_MOTION: f32 = 0.06;

/// A one-pole's coefficient to get most of the way in `ms`.
fn time(ms: f32, rate: f32) -> f32 {
    1.0 - (-1000.0 / (ms.max(0.01) * rate)).exp()
}

fn db(x: f32) -> f32 {
    20.0 * (x + 1e-9).log10()
}

fn amp(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// An LFO's place at a block's start: in time with the song (`beats` a cycle) it's where the
/// song is, so it lands the same every play; else it runs on from `phase`.
fn lfo_start(phase: f64, beats: f32, clock: Clock) -> f64 {
    match beats > 0.0 {
        true => clock.beats / beats as f64,
        false => phase,
    }
}

/// Cycles a sample at `hz`, or `beats` a cycle at the song's tempo.
fn lfo_step(hz: f32, beats: f32, clock: Clock, rate: f32) -> f64 {
    let cycles = match beats > 0.0 {
        true => clock.bpm / 60.0 / beats as f64,
        false => hz as f64,
    };
    cycles / rate as f64
}

pub struct Filtering {
    rate: f32,
    filter: Filter,
    comb: [Vec<f32>; 2],
    phase: f64,
    envelope: f32,
}

impl Filtering {
    pub fn new(rate: f64) -> Self {
        Filtering {
            rate: rate as f32,
            filter: Filter::default(),
            comb: [vec![0.0; COMB], vec![0.0; COMB]],
            phase: 0.0,
            envelope: 0.0,
        }
    }
}

impl Running for Filtering {
    fn reset(&mut self) {
        self.filter = Filter::default();
        self.comb.iter_mut().for_each(|c| c.fill(0.0));
        self.envelope = 0.0;
    }

    fn run(&mut self, u: &Unit, clock: Clock, l: &mut [f32], r: &mut [f32]) -> f32 {
        let Unit::Filter(f) = u else { return 0.0 };
        let rate = self.rate;
        let step = lfo_step(f.lfo_hz, f.lfo_beats, clock, rate);
        self.phase = lfo_start(self.phase, f.lfo_beats, clock);
        let (attack, release) = (time(f.env_attack_ms, rate), time(f.env_release_ms, rate));
        let spread = f.lfo_stereo.clamp(0.0, 1.0) as f64 * 0.5;
        let (mix, gain) = (f.mix.clamp(0.0, 1.0), amp(f.output_db.clamp(-24.0, 24.0)));
        for (a, b) in l.chunks_mut(CHUNK).zip(r.chunks_mut(CHUNK)) {
            for (x, y) in a.iter().zip(b.iter()) {
                let level = x.abs().max(y.abs());
                let k = if level > self.envelope {
                    attack
                } else {
                    release
                };
                self.envelope = ftz(self.envelope + k * (level - self.envelope));
            }
            // The sound's level, 0 at -50 dB to 1 at full scale.
            let loud = ((db(self.envelope) + 50.0) / 50.0).clamp(0.0, 1.0);
            let at = |s: usize| {
                let lfo = wave(f.lfo_wave, self.phase + s as f64 * spread, 7);
                let octaves = f.lfo_depth * lfo + f.env_depth * loud;
                let hz = f.cutoff_hz * 2f32.powf(octaves.clamp(-10.0, 10.0));
                coefs(f.mode, hz, f.resonance, f.drive, rate)
            };
            let c = [at(0), at(1)];
            self.phase += step * a.len() as f64;
            for (x, y) in a.iter_mut().zip(b.iter_mut()) {
                let wet = self.filter.run(&c[0], 0, *x, &mut self.comb[0]);
                *x = (*x + (wet - *x) * mix) * gain;
                let wet = self.filter.run(&c[1], 1, *y, &mut self.comb[1]);
                *y = (*y + (wet - *y) * mix) * gain;
            }
        }
        0.0
    }
}

pub struct Moving {
    rate: f32,
    lines: [Line; 2],
    /// What came out last, each side, to feed back.
    last: [f32; 2],
    /// A phaser's stages' memory, each side.
    stages: [[f32; 12]; 2],
    phase: f64,
}

impl Moving {
    pub fn new(rate: f64) -> Self {
        let line = || Line::new((MOST_MOTION * rate as f32) as usize + 8);
        Moving {
            rate: rate as f32,
            lines: [line(), line()],
            last: [0.0; 2],
            stages: [[0.0; 12]; 2],
            phase: 0.0,
        }
    }
}

impl Running for Moving {
    fn reset(&mut self) {
        self.lines.iter_mut().for_each(Line::clear);
        self.last = [0.0; 2];
        self.stages = [[0.0; 12]; 2];
    }

    fn run(&mut self, u: &Unit, clock: Clock, l: &mut [f32], r: &mut [f32]) -> f32 {
        let Unit::Motion(m) = u else { return 0.0 };
        let rate = self.rate;
        let step = lfo_step(m.rate_hz, m.beats, clock, rate);
        self.phase = lfo_start(self.phase, m.beats, clock);
        let spread = m.spread.clamp(0.0, 1.0) as f64 * 0.5;
        let depth = m.depth.clamp(0.0, 1.0);
        let feedback = m.feedback.clamp(-0.95, 0.95);
        let (mix, gain) = (m.mix.clamp(0.0, 1.0), amp(m.output_db.clamp(-24.0, 24.0)));
        let stages = (m.stages.clamp(2, 12) as usize) & !1;
        let middle = m.delay_ms.clamp(0.5, 40.0);
        // How many copies, and how far each swings either way, ms.
        let (voices, swing) = match m.style {
            MotionStyle::Chorus => (2, (middle - 1.0).min(8.0)),
            MotionStyle::Ensemble => (3, (middle - 1.0).min(4.0)),
            MotionStyle::Flanger => (1, middle - 0.2),
            _ => (1, (middle - 0.5).min(6.0)),
        };
        let ms = rate / 1000.0;
        let sides = [l, r];
        let [l, r] = sides;
        for (a, b) in l.chunks_mut(CHUNK).zip(r.chunks_mut(CHUNK)) {
            // A phaser's stages, tuned where its sweep is.
            let tune = |s: usize, phase: f64| {
                let lfo = wave(Wave::Sine, phase + s as f64 * spread, 0);
                let hz = 800.0 * 8f32.powf(depth * lfo);
                let t = (PI * hz.min(rate * 0.45) / rate).tan();
                (t - 1.0) / (t + 1.0)
            };
            let tuned = [tune(0, self.phase), tune(1, self.phase)];
            for (k, (x, y)) in a.iter_mut().zip(b.iter_mut()).enumerate() {
                let phase = self.phase + step * k as f64;
                for (s, v) in [x, y].into_iter().enumerate() {
                    let dry = *v;
                    let at = phase + s as f64 * spread;
                    let wet = match m.style {
                        MotionStyle::Tremolo => {
                            let lfo = (TAU * at.fract() as f32).sin();
                            dry * (1.0 - depth * (0.5 + 0.5 * lfo))
                        }
                        MotionStyle::Phaser => {
                            let mut w = dry + feedback * self.last[s];
                            for z in &mut self.stages[s][..stages] {
                                let out = tuned[s] * w + *z;
                                *z = ftz(w - tuned[s] * out);
                                w = out;
                            }
                            self.last[s] = ftz(w);
                            w
                        }
                        _ => {
                            self.lines[s].push(ftz(dry + feedback * self.last[s]));
                            let mut sum = 0.0;
                            for voice in 0..voices {
                                let turn = at + voice as f64 / voices as f64;
                                let mut lfo = (TAU * turn.fract() as f32).sin();
                                if m.style == MotionStyle::Ensemble {
                                    // The fast shimmer over the slow drift.
                                    lfo += 0.2 * (TAU * (turn * 7.3).fract() as f32).sin();
                                }
                                let d = (middle + swing * depth * lfo).max(0.1) * ms;
                                sum += self.lines[s].read(d);
                            }
                            let w = sum / (voices as f32).sqrt();
                            self.last[s] = ftz(w);
                            w
                        }
                    };
                    *v = (dry + (wet - dry) * mix) * gain;
                }
            }
            self.phase += step * a.len() as f64;
        }
        0.0
    }
}

pub struct Shaping {
    rate: f32,
    /// A front's two followers, and a tail's: each pair apart when there's one.
    envelopes: [f32; 4],
    gain_db: f32,
}

impl Shaping {
    pub fn new(rate: f64) -> Self {
        Shaping {
            rate: rate as f32,
            envelopes: [0.0; 4],
            gain_db: 0.0,
        }
    }
}

impl Running for Shaping {
    fn reset(&mut self) {
        (self.envelopes, self.gain_db) = ([0.0; 4], 0.0);
    }

    fn run(&mut self, u: &Unit, _: Clock, l: &mut [f32], r: &mut [f32]) -> f32 {
        let Unit::Shaper(p) = u else { return 0.0 };
        let rate = self.rate;
        let speed = p.speed_ms.clamp(5.0, 500.0);
        // (attack, release) of each: fast and slow to rise; quick and long to fall.
        let times = [
            (time(0.5, rate), time(speed, rate)),
            (time(speed * 0.4, rate), time(speed, rate)),
            (time(0.5, rate), time(speed, rate)),
            (time(0.5, rate), time(speed * 6.0, rate)),
        ];
        let smooth = time(1.0, rate);
        let (attack, sustain) = (p.attack.clamp(-1.0, 1.0), p.sustain.clamp(-1.0, 1.0));
        let (mix, gain) = (p.mix.clamp(0.0, 1.0), amp(p.output_db.clamp(-24.0, 24.0)));
        let mut most = 0f32;
        for (x, y) in l.iter_mut().zip(r.iter_mut()) {
            let level = x.abs().max(y.abs());
            for (e, &(up, down)) in self.envelopes.iter_mut().zip(&times) {
                let k = if level > *e { up } else { down };
                *e = ftz(*e + k * (level - *e));
            }
            let [fast, slow, quick, long] = self.envelopes.map(db);
            let front = ((fast - slow) / 12.0).clamp(0.0, 1.0);
            let tail = ((long - quick) / 12.0).clamp(0.0, 1.0);
            let want = 18.0 * (attack * front + sustain * tail);
            self.gain_db += smooth * (want - self.gain_db);
            most = most.max(-self.gain_db);
            let g = amp(self.gain_db);
            for v in [x, y] {
                let wet = *v * g;
                let wet = if p.clip { knee(wet) } else { wet };
                *v = (*v + (wet - *v) * mix) * gain;
            }
        }
        most
    }
}

pub struct Gating {
    rate: f32,
    ear: [Biquad; 2],
    envelope: f32,
    hold: u32,
    gain_db: f32,
}

impl Gating {
    pub fn new(rate: f64) -> Self {
        Gating {
            rate: rate as f32,
            ear: [Biquad::default(); 2],
            envelope: 0.0,
            hold: 0,
            gain_db: 0.0,
        }
    }
}

impl Running for Gating {
    fn reset(&mut self) {
        self.ear = [Biquad::default(); 2];
        (self.envelope, self.hold, self.gain_db) = (0.0, 0, 0.0);
    }

    fn run(&mut self, u: &Unit, _: Clock, l: &mut [f32], r: &mut [f32]) -> f32 {
        let Unit::Gate(g) = u else { return 0.0 };
        let rate = self.rate;
        self.ear[0].tune(g.low_cut_hz, rate, true);
        self.ear[1].tune(g.high_cut_hz, rate, false);
        let range = g.range_db.clamp(0.0, 90.0);
        let slope = (g.ratio.clamp(1.0, 100.0) - 1.0).max(0.0);
        let (open, close) = (time(g.attack_ms, rate), time(g.release_ms, rate));
        let fall = time(20.0, rate);
        let hold = (g.hold_ms.max(0.0) / 1000.0 * rate) as u32;
        let out = amp(g.output_db.clamp(-24.0, 24.0));
        let mut most = 0f32;
        for (x, y) in l.iter_mut().zip(r.iter_mut()) {
            let ear = self.ear[0].run(0.5 * (*x + *y));
            let heard = self.ear[1].run(ear).abs();
            self.envelope = match heard > self.envelope {
                true => heard,
                false => ftz(self.envelope + fall * (heard - self.envelope)),
            };
            // Open, it closes 3 dB lower than it opens, so a level on the edge doesn't chatter.
            let edge = g.threshold_db - if self.gain_db > -1.0 { 3.0 } else { 0.0 };
            let under = edge - db(self.envelope);
            let want = match (under <= 0.0, self.hold > 0) {
                (true, _) => {
                    self.hold = hold;
                    0.0
                }
                (_, true) => {
                    self.hold -= 1;
                    0.0
                }
                _ => -(under * slope).min(range),
            };
            let k = if want > self.gain_db { open } else { close };
            self.gain_db += k * (want - self.gain_db);
            most = most.max(-self.gain_db);
            let a = amp(self.gain_db) * out;
            (*x, *y) = (*x * a, *y * a);
        }
        most
    }
}

pub struct DeEssing {
    rate: f32,
    band: [[Biquad; 2]; 2],
    envelope: f32,
    gain_db: f32,
}

impl DeEssing {
    pub fn new(rate: f64) -> Self {
        DeEssing {
            rate: rate as f32,
            band: [[Biquad::default(); 2]; 2],
            envelope: 0.0,
            gain_db: 0.0,
        }
    }
}

impl Running for DeEssing {
    fn reset(&mut self) {
        self.band = [[Biquad::default(); 2]; 2];
        (self.envelope, self.gain_db) = (0.0, 0.0);
    }

    fn run(&mut self, u: &Unit, _: Clock, l: &mut [f32], r: &mut [f32]) -> f32 {
        let Unit::DeEsser(d) = u else { return 0.0 };
        let rate = self.rate;
        let half = 2f32.powf(d.width.clamp(0.2, 4.0) / 2.0);
        for side in &mut self.band {
            side[0].tune(d.freq_hz / half, rate, true);
            side[1].tune(d.freq_hz * half, rate, false);
        }
        let (rise, fall) = (time(0.5, rate), time(60.0, rate));
        let smooth = time(2.0, rate);
        let range = d.range_db.clamp(0.0, 40.0);
        let mut most = 0f32;
        for (x, y) in l.iter_mut().zip(r.iter_mut()) {
            let mut band = [0f32; 2];
            for (s, v) in [*x, *y].into_iter().enumerate() {
                let high = self.band[s][0].run(v);
                band[s] = self.band[s][1].run(high);
            }
            let level = band[0].abs().max(band[1].abs());
            let k = if level > self.envelope { rise } else { fall };
            self.envelope = ftz(self.envelope + k * (level - self.envelope));
            let want = -((db(self.envelope) - d.threshold_db) * 0.8).clamp(0.0, range);
            self.gain_db += smooth * (want - self.gain_db);
            most = most.max(-self.gain_db);
            let g = amp(self.gain_db);
            (*x, *y) = match (d.listen, d.wide) {
                (true, _) => (band[0], band[1]),
                (_, true) => (*x * g, *y * g),
                // The band alone taken down: with nothing to do, the sound exactly as it was.
                _ => (*x + (g - 1.0) * band[0], *y + (g - 1.0) * band[1]),
            };
        }
        most
    }
}

pub struct Widening {
    rate: f32,
    /// The sides' filters: a high-pass at `mono_hz`, a low-pass at each crossover.
    side: [Biquad; 3],
}

impl Widening {
    pub fn new(rate: f64) -> Self {
        Widening {
            rate: rate as f32,
            side: [Biquad::default(); 3],
        }
    }
}

impl Running for Widening {
    fn reset(&mut self) {
        self.side = [Biquad::default(); 3];
    }

    fn run(&mut self, u: &Unit, _: Clock, l: &mut [f32], r: &mut [f32]) -> f32 {
        let Unit::Stereo(s) = u else { return 0.0 };
        let rate = self.rate;
        self.side[0].tune(s.mono_hz.max(20.0), rate, true);
        self.side[1].tune(s.crossovers[0], rate, false);
        self.side[2].tune(s.crossovers[1].max(s.crossovers[0]), rate, false);
        let width = s.width.map(|w| w.clamp(0.0, 2.0));
        let (mid, side) = (
            amp(s.mid_db.clamp(-24.0, 24.0)),
            amp(s.side_db.clamp(-24.0, 24.0)),
        );
        let out = amp(s.output_db.clamp(-24.0, 24.0));
        let balance = s.balance.clamp(-1.0, 1.0);
        let sides = [
            out * (1.0 - balance).min(1.0),
            out * (1.0 + balance).min(1.0),
        ];
        let invert = [
            if s.invert_left { -1.0 } else { 1.0 },
            if s.invert_right { -1.0 } else { 1.0 },
        ];
        for (x, y) in l.iter_mut().zip(r.iter_mut()) {
            let (a, b) = match s.swap {
                true => (*y * invert[0], *x * invert[1]),
                false => (*x * invert[0], *y * invert[1]),
            };
            let (m, mut d) = ((a + b) * 0.5, (a - b) * 0.5);
            if s.mono_hz > 0.0 {
                d = self.side[0].run(d);
            }
            // Split by taking each low-pass off what's left, so they add back exactly.
            let low = self.side[1].run(d);
            let middle = self.side[2].run(d - low);
            let high = d - low - middle;
            let d = (low * width[0] + middle * width[1] + high * width[2]) * side;
            let m = m * mid;
            (*x, *y) = ((m + d) * sides[0], (m - d) * sides[1]);
        }
        0.0
    }
}

/// A Linkwitz-Riley split: two Butterworths each way. A low and a high added back are flat,
/// only their phase turned.
#[derive(Clone, Copy, Default)]
struct Split {
    low: [Biquad; 2],
    high: [Biquad; 2],
}

impl Split {
    fn tune(&mut self, hz: f32, rate: f32) {
        self.low.iter_mut().for_each(|b| b.tune(hz, rate, false));
        self.high.iter_mut().for_each(|b| b.tune(hz, rate, true));
    }

    fn run(&mut self, x: f32) -> [f32; 2] {
        let [a, b] = &mut self.low;
        let [c, d] = &mut self.high;
        [b.run(a.run(x)), d.run(c.run(x))]
    }
}

pub struct Multibanding {
    rate: f32,
    /// Each side's first split, its second, and what turns the lows' phase as the second turns
    /// the rest's, so they add back true.
    splits: [[Split; 3]; 2],
    levels: [f32; 3],
    gains: [f32; 3],
    most: [f32; 3],
}

impl Multibanding {
    pub fn new(rate: f64) -> Self {
        Multibanding {
            rate: rate as f32,
            splits: [[Split::default(); 3]; 2],
            levels: [0.0; 3],
            gains: [0.0; 3],
            most: [0.0; 3],
        }
    }
}

/// What a band's gain wants at `level` dB, and how much of that is turning it down.
fn band_gain(m: &Multiband, band: usize, level: f32) -> (f32, f32) {
    let over = level - m.threshold_db[band];
    let down = over.max(0.0) * (1.0 - 1.0 / m.ratio[band].max(1.0));
    // Quiet parts come up three quarters of the way; silence stays silent.
    let room = ((level + 80.0) / 20.0).clamp(0.0, 1.0);
    let up = (m.lift[band].clamp(0.0, 1.0) * 0.75 * (-over).max(0.0) * room).min(24.0);
    (m.gain_db[band].clamp(-24.0, 24.0) + up - down, down)
}

impl Running for Multibanding {
    fn reset(&mut self) {
        self.splits = [[Split::default(); 3]; 2];
        (self.levels, self.gains) = ([0.0; 3], [0.0; 3]);
    }

    fn reading(&self) -> [f32; 3] {
        self.most
    }

    fn run(&mut self, u: &Unit, _: Clock, l: &mut [f32], r: &mut [f32]) -> f32 {
        let Unit::Multiband(m) = u else { return 0.0 };
        let rate = self.rate;
        let (low, high) = (m.crossovers[0], m.crossovers[1].max(m.crossovers[0] * 1.2));
        for side in &mut self.splits {
            side[0].tune(low, rate);
            side[1].tune(high, rate);
            side[2].tune(high, rate);
        }
        let (rise, fall) = (time(m.attack_ms, rate), time(m.release_ms, rate));
        let smooth = time(1.0, rate);
        let (mix, gain) = (m.mix.clamp(0.0, 1.0), amp(m.output_db.clamp(-24.0, 24.0)));
        self.most = [0.0; 3];
        for (x, y) in l.iter_mut().zip(r.iter_mut()) {
            let mut bands = [[0f32; 3]; 2];
            for (s, v) in [*x, *y].into_iter().enumerate() {
                let [lows, rest] = self.splits[s][0].run(v);
                let [middle, highs] = self.splits[s][1].run(rest);
                let [a, b] = self.splits[s][2].run(lows);
                bands[s] = [a + b, middle, highs];
            }
            let mut wet = [0f32; 2];
            let [left, right] = bands;
            for (b, (a, c)) in left.into_iter().zip(right).enumerate() {
                let heard = a.abs().max(c.abs());
                let k = if heard > self.levels[b] { rise } else { fall };
                self.levels[b] = ftz(self.levels[b] + k * (heard - self.levels[b]));
                let (want, down) = band_gain(m, b, db(self.levels[b]));
                self.gains[b] += smooth * (want - self.gains[b]);
                self.most[b] = self.most[b].max(down);
                let g = amp(self.gains[b]);
                wet[0] += a * g;
                wet[1] += c * g;
            }
            // Dry is the bands added back, not what came in: the same phase, so no comb.
            let dry = [bands[0].iter().sum::<f32>(), bands[1].iter().sum::<f32>()];
            *x = (dry[0] + (wet[0] - dry[0]) * mix) * gain;
            *y = (dry[1] + (wet[1] - dry[1]) * mix) * gain;
        }
        self.most.iter().fold(0f32, |a, b| a.max(*b))
    }
}

#[cfg(test)]
mod tests;
