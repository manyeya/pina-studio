//! What moves a sound's pitch: a pitch shifter, a tape stop and pitch correction.
//!
//! The shifter is a phase vocoder that keeps each spectral peak's neighbours locked to it (Laroche
//! and Dolson), so a chord stays a chord instead of going watery. Correction hears a voice's pitch
//! with YIN and moves it by overlapping pitch periods (PSOLA): the periods keep their shape, so
//! the voice keeps its character, where a vocoder's would go chipmunk. Each allocates nothing as
//! it runs.

use std::{
    f32::consts::{PI, TAU},
    sync::Arc,
};

use rustfft::{Fft, FftPlanner, num_complex::Complex};

use super::{Clock, Running, comp::ftz, sampler::analysis::yin_in};
use crate::song::project::Unit;

/// Long enough to tell a minor third's notes apart down to about A3.
const SIZE: usize = 4096;
const HOP: usize = SIZE / 4;
/// How late the shifter's sound comes out, samples: a whole window.
pub const PITCH_LATENCY: u32 = SIZE as u32;

fn amp(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// A one-pole's coefficient to get most of the way in `ms`, moved `rate` times a second.
fn time(ms: f32, rate: f32) -> f32 {
    1.0 - (-1000.0 / (ms.max(0.01) * rate)).exp()
}

/// An angle brought within a half turn of nothing.
fn wrap(a: f32) -> f32 {
    a - TAU * (a / TAU).round()
}

/// Four-point Hermite: `x` between its middle two.
fn hermite([a, b, c, d]: [f32; 4], x: f32) -> f32 {
    let (c0, c1) = (b, 0.5 * (c - a));
    let c2 = a - 2.5 * b + 2.0 * c - 0.5 * d;
    let c3 = 0.5 * (d - a) + 1.5 * (b - c);
    ((c3 * x + c2) * x + c1) * x + c0
}

/// One side of the shifter.
struct Shifted {
    /// What came in, and what's going out, each a window long and round and round.
    input: Vec<f32>,
    output: Vec<f32>,
    bins: Vec<Complex<f32>>,
    moved: Vec<Complex<f32>>,
    /// Each bin's phase last time, and how far its peak has been turned so far.
    phase: Vec<f32>,
    turned: Vec<f32>,
    turning: Vec<f32>,
    peaks: Vec<usize>,
}

impl Shifted {
    fn new() -> Self {
        let half = SIZE / 2 + 1;
        Shifted {
            input: vec![0.0; SIZE],
            output: vec![0.0; SIZE],
            bins: vec![Complex::default(); SIZE],
            moved: vec![Complex::default(); SIZE],
            phase: vec![0.0; half],
            turned: vec![0.0; half],
            turning: vec![0.0; half],
            peaks: Vec::with_capacity(half),
        }
    }

    /// The last window moved by `ratio` and laid over what's going out from `at`.
    fn frame(
        &mut self,
        at: usize,
        ratio: f32,
        (fft, ifft): (&dyn Fft<f32>, &dyn Fft<f32>),
        window: &[f32],
        scratch: &mut [Complex<f32>],
    ) {
        for (k, b) in self.bins.iter_mut().enumerate() {
            *b = Complex::new(self.input[(at + k) % SIZE] * window[k], 0.0);
        }
        fft.process_with_scratch(&mut self.bins, scratch);
        let half = SIZE / 2;
        let size = |k: usize| self.bins[k].norm_sqr();
        self.peaks.clear();
        for k in 1..half - 1 {
            let m = size(k);
            if m > 1e-12 && m > size(k - 1) && m > size(k + 1) {
                self.peaks.push(k);
            }
        }
        self.moved.fill(Complex::default());
        let hop = HOP as f32;
        for (i, &p) in self.peaks.iter().enumerate() {
            let centre = TAU * p as f32 / SIZE as f32;
            let was = self.bins[p].arg();
            let heard = centre + wrap(was - self.phase[p] - hop * centre) / hop;
            let turn = wrap(self.turned[p] + hop * heard * (ratio - 1.0));
            let by = (p as f32 * ratio).round() as isize - p as isize;
            let from = if i == 0 {
                0
            } else {
                (self.peaks[i - 1] + p) / 2
            };
            let to = self.peaks.get(i + 1).map_or(half, |&n| (n + p) / 2);
            let spin = Complex::from_polar(1.0, turn);
            // The bins round a peak go with it, turned as one: they stay locked to it.
            for k in from..to {
                self.turning[k] = turn;
                let t = k as isize + by;
                if (1..half as isize).contains(&t) {
                    self.moved[t as usize] += self.bins[k] * spin;
                }
            }
        }
        for k in 0..=half {
            self.phase[k] = self.bins[k].arg();
        }
        std::mem::swap(&mut self.turned, &mut self.turning);
        for k in 1..half {
            self.moved[SIZE - k] = self.moved[k].conj();
        }
        ifft.process_with_scratch(&mut self.moved, scratch);
        // Hann twice over at a quarter's hop adds to one and a half.
        let scale = 1.0 / (SIZE as f32 * 1.5);
        for (k, b) in self.moved.iter().enumerate() {
            self.output[(at + k) % SIZE] += b.re * window[k] * scale;
        }
    }
}

pub struct Pitching {
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    scratch: Vec<Complex<f32>>,
    sides: [Shifted; 2],
    at: usize,
    since: usize,
    back: [f32; 2],
}

impl Pitching {
    pub fn new() -> Self {
        let mut planner = FftPlanner::new();
        let (fft, ifft) = (
            planner.plan_fft_forward(SIZE),
            planner.plan_fft_inverse(SIZE),
        );
        let n = fft
            .get_inplace_scratch_len()
            .max(ifft.get_inplace_scratch_len());
        Pitching {
            fft,
            ifft,
            window: (0..SIZE)
                .map(|i| 0.5 - 0.5 * (TAU * i as f32 / SIZE as f32).cos())
                .collect(),
            scratch: vec![Complex::default(); n],
            sides: [Shifted::new(), Shifted::new()],
            at: 0,
            since: 0,
            back: [0.0; 2],
        }
    }
}

impl Default for Pitching {
    fn default() -> Self {
        Self::new()
    }
}

impl Running for Pitching {
    fn reset(&mut self) {
        for s in &mut self.sides {
            s.input.fill(0.0);
            s.output.fill(0.0);
            s.phase.fill(0.0);
            s.turned.fill(0.0);
        }
        (self.at, self.since, self.back) = (0, 0, [0.0; 2]);
    }

    fn run(&mut self, u: &Unit, _: Clock, l: &mut [f32], r: &mut [f32]) -> f32 {
        let Unit::Pitch(p) = u else { return 0.0 };
        let ratio = 2f32.powf((p.semitones.clamp(-24.0, 24.0) + p.cents / 100.0) / 12.0);
        let (mix, gain) = (p.mix.clamp(0.0, 1.0), amp(p.output_db.clamp(-24.0, 24.0)));
        let feedback = p.feedback.clamp(0.0, 0.9);
        for (x, y) in l.iter_mut().zip(r.iter_mut()) {
            for (s, v) in [&mut *x, &mut *y].into_iter().enumerate() {
                let side = &mut self.sides[s];
                let dry = side.input[self.at];
                let wet = side.output[self.at];
                side.output[self.at] = 0.0;
                side.input[self.at] = *v + self.back[s] * feedback;
                self.back[s] = ftz(wet);
                *v = (dry + (wet - dry) * mix) * gain;
            }
            self.at = (self.at + 1) % SIZE;
            self.since += 1;
            if self.since == HOP {
                self.since = 0;
                let plans = (self.fft.as_ref(), self.ifft.as_ref());
                for side in &mut self.sides {
                    side.frame(self.at, ratio, plans, &self.window, &mut self.scratch);
                }
            }
        }
        0.0
    }
}

/// How long a tape stop's memory is, s: past its longest stop and start at the slowest tempo.
const TAPE_S: f32 = 10.0;

pub struct Stopping {
    rate: f32,
    tape: [Vec<f32>; 2],
    write: usize,
    /// Where it's playing the tape from, behind where it's writing.
    read: f64,
    speed: f32,
    /// Back at speed: what's left of the fade from the tape to the sound as it comes.
    join: f32,
}

impl Stopping {
    pub fn new(rate: f64) -> Self {
        let n = (TAPE_S as f64 * rate) as usize;
        Stopping {
            rate: rate as f32,
            tape: [vec![0.0; n], vec![0.0; n]],
            write: 0,
            read: 0.0,
            speed: 1.0,
            join: 0.0,
        }
    }

    fn at(&self, s: usize, pos: f64) -> f32 {
        let n = self.tape[s].len();
        let whole = pos.floor();
        let i = whole as usize;
        let p = |k: usize| self.tape[s][(i + n + k - 1) % n];
        hermite([p(0), p(1), p(2), p(3)], (pos - whole) as f32)
    }
}

impl Running for Stopping {
    fn reset(&mut self) {
        (self.speed, self.join) = (1.0, 0.0);
        self.read = self.write as f64;
    }

    fn reading(&self) -> [f32; 3] {
        [self.speed, 0.0, 0.0]
    }

    fn run(&mut self, u: &Unit, clock: Clock, l: &mut [f32], r: &mut [f32]) -> f32 {
        let Unit::TapeStop(t) = u else { return 0.0 };
        let n = self.tape[0].len();
        let beat = 60.0 / clock.bpm.max(1.0) as f32 * self.rate;
        let target = if t.stop > 0.5 { 0.0 } else { 1.0 };
        let (up, down) = (
            1.0 / (t.start_beats.max(0.01) * beat),
            1.0 / (t.stop_beats.max(0.01) * beat),
        );
        let bend = 1.0 + 2.0 * t.curve.clamp(0.0, 1.0);
        let (mix, gain) = (t.mix.clamp(0.0, 1.0), amp(t.output_db.clamp(-24.0, 24.0)));
        let fade = 1.0 / (0.01 * self.rate);
        for (x, y) in l.iter_mut().zip(r.iter_mut()) {
            self.tape[0][self.write] = *x;
            self.tape[1][self.write] = *y;
            let was = self.speed;
            self.speed = (was + (target - was).clamp(-down, up)).clamp(0.0, 1.0);
            // Up to speed with the tape behind: fade across to the sound as it comes.
            if self.speed >= 1.0 && was < 1.0 {
                self.join = 1.0;
            }
            let wet = |s: &Self, side: usize, live: f32| match (s.speed >= 1.0, s.join > 0.0) {
                (true, false) => live,
                (true, true) => live + (s.at(side, s.read) - live) * s.join,
                // It dies as it stops, not on a held sample.
                _ => s.at(side, s.read) * (s.speed / 0.04).min(1.0),
            };
            let out = [wet(self, 0, *x), wet(self, 1, *y)];
            *x = (*x + (out[0] - *x) * mix) * gain;
            *y = (*y + (out[1] - *y) * mix) * gain;
            let lag = (self.write as f64 - self.read).rem_euclid(n as f64);
            match (self.speed, self.join > 0.0) {
                (s, false) if s >= 1.0 => self.read = self.write as f64,
                (s, _) if s <= 0.0 => self.read = self.write as f64,
                (s, joining) => {
                    let v = if joining { 1.0 } else { s.powf(bend) };
                    self.read += v as f64;
                    // Never run off the end of the tape.
                    if lag > (n - 8) as f64 {
                        self.read = self.write as f64;
                    }
                    self.read %= n as f64;
                }
            }
            self.join = (self.join - fade).max(0.0);
            self.write = (self.write + 1) % n;
        }
        0.0
    }
}

/// How long correction's memory is, samples: comfortably over its delay and a period.
const TUNE_RING: usize = 1 << 14;
/// How often it listens, samples of the detector's rate.
const LISTEN: usize = 64;
/// The detector's rate, and how long a stretch it listens to.
const EAR_RATE: f32 = 12_000.0;
const EAR: usize = 480;

/// How late correction's sound comes out: two of the longest periods it hears.
pub fn tune_latency(rate: f64) -> u32 {
    (rate * 0.04) as u32
}

pub struct Tuning {
    rate: f32,
    delay: usize,
    ring: [Vec<f32>; 2],
    out: [Vec<f32>; 2],
    now: usize,
    /// The detector: the voice at its rate, its memory, and YIN's scratch.
    every: usize,
    sum: f32,
    summed: usize,
    ear: Vec<f32>,
    heard: usize,
    d: Vec<f32>,
    cmnd: Vec<f32>,
    /// The period it's working at (samples of the song), whether that's a voice, the note it's
    /// landing on, and how far it's moving it (semitones).
    period: f32,
    voiced: bool,
    note: f32,
    shift: f32,
    sung: f32,
    /// Where the next period's taken from, and where it's laid.
    mark: f64,
    laid: f64,
}

impl Tuning {
    pub fn new(rate: f64) -> Self {
        let every = ((rate as f32 / EAR_RATE).round() as usize).max(1);
        let lags = EAR / 2 + 2;
        Tuning {
            rate: rate as f32,
            delay: tune_latency(rate) as usize,
            ring: [vec![0.0; TUNE_RING], vec![0.0; TUNE_RING]],
            out: [vec![0.0; TUNE_RING], vec![0.0; TUNE_RING]],
            now: TUNE_RING,
            every,
            sum: 0.0,
            summed: 0,
            ear: vec![0.0; EAR],
            heard: 0,
            d: vec![0.0; lags],
            cmnd: vec![0.0; lags],
            period: rate as f32 * 0.005,
            voiced: false,
            note: 0.0,
            shift: 0.0,
            sung: 0.0,
            mark: TUNE_RING as f64,
            laid: TUNE_RING as f64,
        }
    }

    /// One period of the voice, from `mark`, laid at `laid` under a window two periods long.
    fn lay(&mut self, t: usize, gain: f32) {
        let whole = self.laid.round();
        let from = self.mark + (whole - self.laid);
        let (first, part) = (from.floor(), (from - from.floor()) as f32);
        let (first, whole) = (first as usize, whole as usize);
        let across = PI / t as f32;
        for i in 1..2 * t {
            let w = 0.5 - 0.5 * (across * i as f32).cos();
            let at = first + i - t;
            let to = (whole + i - t) & (TUNE_RING - 1);
            for s in 0..2 {
                let p = |k: usize| self.ring[s][(at + k + TUNE_RING - 1) & (TUNE_RING - 1)];
                let v = hermite([p(0), p(1), p(2), p(3)], part);
                self.out[s][to] += v * w * gain;
            }
        }
    }

    /// The voice heard, and what it should be moved by.
    fn listen(&mut self, t: &crate::song::project::Tune) {
        let (lo, hi) = (t.low_hz.max(40.0), t.high_hz.min(3000.0));
        let (a, b) = self.ear.split_at(self.heard);
        // Oldest first, in one piece: it's a short copy, not worth a second scratch.
        let mut line = [0f32; EAR];
        line[..b.len()].copy_from_slice(b);
        line[b.len()..].copy_from_slice(a);
        let hz = match line.iter().fold(0f32, |m, x| m.max(x.abs())) > 3e-3 {
            true => yin_in(&line, EAR_RATE, (lo, hi), &mut self.d, &mut self.cmnd),
            false => None,
        };
        let speed = time(t.speed_ms, self.rate / (LISTEN * self.every) as f32);
        let Some(hz) = hz else {
            (self.voiced, self.sung) = (false, 0.0);
            self.period = self.rate * 0.005;
            self.shift += speed * (0.0 - self.shift);
            return;
        };
        let midi = 69.0 + 12.0 * (hz / 440.0).log2();
        self.period = self.rate / hz;
        (self.voiced, self.sung) = (true, midi);
        // It stays on a note until the voice has clearly left it.
        let on = |n: f32| t.notes[(n.round() as i32).rem_euclid(12) as usize];
        if !on(self.note) || (midi - self.note).abs() > 0.6 {
            let near = (0..=6)
                .flat_map(|d| [midi.round() + d as f32, midi.round() - d as f32])
                .filter(|&n| on(n))
                .min_by(|a, b| (a - midi).abs().total_cmp(&(b - midi).abs()));
            if let Some(n) = near {
                self.note = n;
            }
        }
        let want = match t.notes.iter().any(|&n| n) {
            true => (self.note - midi) * t.amount.clamp(0.0, 1.0),
            false => 0.0,
        };
        self.shift += speed * (want - self.shift);
    }
}

impl Running for Tuning {
    fn reset(&mut self) {
        for s in 0..2 {
            self.ring[s].fill(0.0);
            self.out[s].fill(0.0);
        }
        self.ear.fill(0.0);
        (self.voiced, self.shift, self.sung) = (false, 0.0, 0.0);
        self.period = self.rate * 0.005;
        (self.mark, self.laid) = (self.now as f64, self.now as f64);
    }

    fn reading(&self) -> [f32; 3] {
        match self.voiced {
            true => [self.sung, self.sung + self.shift, 0.0],
            false => [0.0; 3],
        }
    }

    fn run(&mut self, u: &Unit, _: Clock, l: &mut [f32], r: &mut [f32]) -> f32 {
        let Unit::Tune(t) = u else { return 0.0 };
        let gain = amp(t.output_db.clamp(-24.0, 24.0));
        let longest = self.delay / 2 - 4;
        for (x, y) in l.iter_mut().zip(r.iter_mut()) {
            let now = self.now & (TUNE_RING - 1);
            self.ring[0][now] = *x;
            self.ring[1][now] = *y;
            self.sum += (*x + *y) * 0.5;
            self.summed += 1;
            if self.summed == self.every {
                self.ear[self.heard] = self.sum / self.every as f32;
                self.heard = (self.heard + 1) % EAR;
                (self.sum, self.summed) = (0.0, 0);
                if self.heard.is_multiple_of(LISTEN) {
                    self.listen(t);
                }
            }
            let going = self.now - self.delay;
            let ratio = 2f32.powf(self.shift / 12.0);
            let period = self.period.clamp(8.0, longest as f32);
            let span = period.round() as usize;
            // Each period laid ahead of what's going out, closer together to go up.
            while self.laid <= (going + span + 2) as f64 {
                while self.mark + (period * 0.5) as f64 <= self.laid {
                    self.mark += period as f64;
                }
                self.lay(span, 1.0 / ratio);
                self.laid += (period / ratio) as f64;
            }
            let at = going & (TUNE_RING - 1);
            *x = self.out[0][at] * gain;
            *y = self.out[1][at] * gain;
            self.out[0][at] = 0.0;
            self.out[1][at] = 0.0;
            self.now += 1;
        }
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song::project::{PitchShift, TapeStop, Tune};

    const RATE: f64 = 48_000.0;
    const CLOCK: Clock = Clock {
        bpm: 120.0,
        beats: 0.0,
    };

    fn sine(hz: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| 0.4 * (TAU * hz * i as f32 / RATE as f32).sin())
            .collect()
    }

    fn run(t: &mut dyn Running, u: Unit, x: &[f32]) -> Vec<f32> {
        let (mut a, mut b) = (x.to_vec(), x.to_vec());
        for (p, q) in a.chunks_mut(256).zip(b.chunks_mut(256)) {
            t.run(&u, CLOCK, p, q);
        }
        a
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    /// How much of `hz` is in `x`.
    fn level_at(x: &[f32], hz: f32) -> f32 {
        let (mut re, mut im) = (0f32, 0f32);
        for (i, s) in x.iter().enumerate() {
            let ph = TAU * hz * i as f32 / RATE as f32;
            (re, im) = (re + s * ph.cos(), im + s * ph.sin());
        }
        (re * re + im * im).sqrt() / x.len() as f32 * 2.0
    }

    /// The pitch of `x` by where it crosses zero rising, between crossings.
    fn pitch(x: &[f32]) -> f32 {
        let ups: Vec<f32> = x
            .windows(2)
            .enumerate()
            .filter(|(_, w)| w[0] <= 0.0 && w[1] > 0.0)
            .map(|(i, w)| i as f32 + w[0] / (w[0] - w[1]))
            .collect();
        (ups.len() - 1) as f32 / (ups[ups.len() - 1] - ups[0]) * RATE as f32
    }

    #[test]
    fn the_shifter_moves_a_tone_keeps_its_level_and_does_nothing_at_nothing() {
        let x = sine(440.0, 48_000);
        let shifted = |semitones| {
            let u = Unit::Pitch(PitchShift {
                semitones,
                ..PitchShift::default()
            });
            run(&mut Pitching::new(), u, &x)
        };
        let up = shifted(12.0);
        let tail = &up[12_000..];
        assert!(level_at(tail, 880.0) > 0.3, "{}", level_at(tail, 880.0));
        assert!(level_at(tail, 440.0) < 0.02, "{}", level_at(tail, 440.0));
        let fifth = shifted(7.0);
        let tail = &fifth[12_000..];
        assert!((pitch(tail) - 659.26).abs() < 3.0, "{}", pitch(tail));
        // Nothing moved: exactly what came in, a window late.
        let same = shifted(0.0);
        let late = SIZE;
        let off: f32 = same[late + 4096..]
            .iter()
            .zip(&x[4096..])
            .fold(0.0, |m, (a, b)| m.max((a - b).abs()));
        assert!(off < 2e-3, "{off}");
    }

    #[test]
    fn a_chord_stays_a_chord_when_shifted() {
        let (a, b) = (sine(220.0, 48_000), sine(277.18, 48_000));
        let x: Vec<f32> = a.iter().zip(&b).map(|(p, q)| p + q).collect();
        let u = Unit::Pitch(PitchShift {
            semitones: 7.0,
            ..PitchShift::default()
        });
        let y = run(&mut Pitching::new(), u, &x);
        let tail = &y[12_000..];
        assert!(level_at(tail, 329.63) > 0.28, "{}", level_at(tail, 329.63));
        assert!(level_at(tail, 415.3) > 0.28, "{}", level_at(tail, 415.3));
        assert!(level_at(tail, 220.0) < 0.03, "{}", level_at(tail, 220.0));
    }

    #[test]
    fn a_tape_stop_slows_to_nothing_and_comes_back() {
        let x = sine(1000.0, 96_000);
        let (mut a, mut b) = (x.clone(), x.clone());
        let mut t = Stopping::new(RATE);
        let (mid, _) = (48_000, 0);
        let on = Unit::TapeStop(TapeStop {
            stop: 1.0,
            stop_beats: 1.0,
            start_beats: 0.25,
            curve: 0.0,
            ..TapeStop::default()
        });
        let off = Unit::TapeStop(TapeStop {
            stop: 0.0,
            stop_beats: 1.0,
            start_beats: 0.25,
            curve: 0.0,
            ..TapeStop::default()
        });
        for (i, (p, q)) in a.chunks_mut(256).zip(b.chunks_mut(256)).enumerate() {
            let u = if i * 256 < mid { on } else { off };
            t.run(&u, CLOCK, p, q);
        }
        // A beat at 120 is half a second: halfway down, about half the pitch.
        let falling = &a[11_000..13_000];
        assert!((pitch(falling) - 500.0).abs() < 90.0, "{}", pitch(falling));
        // Stopped: silence. Back at speed: the sound as it comes, in time.
        assert!(
            rms(&a[30_000..47_000]) < 1e-3,
            "{}",
            rms(&a[30_000..47_000])
        );
        let back = &a[70_000..];
        assert!(rms(back) > 0.26, "{}", rms(back));
        let off: f32 = back
            .iter()
            .zip(&x[70_000..])
            .fold(0.0, |m, (p, q)| m.max((p - q).abs()));
        assert!(off < 1e-3, "{off}");
    }

    #[test]
    fn correction_brings_a_voice_to_its_nearest_note() {
        // A# and B are out: 225 Hz, between A and A#, lands on A.
        let mut notes = [true; 12];
        notes[10] = false;
        notes[11] = false;
        let u = Unit::Tune(Tune {
            notes,
            speed_ms: 0.0,
            ..Tune::default()
        });
        let x = sine(225.0, 48_000);
        let mut t = Tuning::new(RATE);
        let y = run(&mut t, u, &x);
        let p = pitch(&y[24_000..]);
        assert!((p - 220.0).abs() < 0.7, "{p}");
        assert!((rms(&y[24_000..]) / rms(&x) - 1.0).abs() < 0.2);
        // A slow one's still on its way, a little.
        let slow = Unit::Tune(Tune {
            notes,
            speed_ms: 400.0,
            ..Tune::default()
        });
        let y = run(&mut Tuning::new(RATE), slow, &x);
        assert!(
            pitch(&y[4_000..12_000]) > 220.5,
            "{}",
            pitch(&y[4_000..12_000])
        );
        // Amount at nothing: it stays where it was sung.
        let none = Unit::Tune(Tune {
            notes,
            speed_ms: 0.0,
            amount: 0.0,
            ..Tune::default()
        });
        let y = run(&mut Tuning::new(RATE), none, &x);
        assert!((pitch(&y[24_000..]) - 225.0).abs() < 0.7);
    }

    #[test]
    fn correction_leaves_a_voice_its_shape_and_silence_silent() {
        // A buzzy voice a semitone flat of its note: the harmonics it had stay in their place
        // relative to each other, its period simply shorter.
        let voice: Vec<f32> = (0..48_000)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                (1..=8)
                    .map(|k| 0.15 / k as f32 * (TAU * 233.08 * k as f32 * t).sin())
                    .sum()
            })
            .collect();
        let mut notes = [false; 12];
        notes[11] = true;
        let u = Unit::Tune(Tune {
            notes,
            speed_ms: 0.0,
            ..Tune::default()
        });
        let y = run(&mut Tuning::new(RATE), u, &voice);
        let tail = &y[24_000..];
        let (a, b) = (level_at(tail, 246.94), level_at(tail, 233.08));
        assert!(a > 0.1 && b < a * 0.1, "{a} {b}");
        let z = run(&mut Tuning::new(RATE), u, &vec![0.0; 9600]);
        assert_eq!(rms(&z), 0.0);
    }
}
