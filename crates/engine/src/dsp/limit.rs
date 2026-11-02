//! The channel strip's limiter, running. The sound is driven in by its gain, its DC taken out,
//! and (in the multiband style) its lows, mids and highs each held under the ceiling apart.
//! Then its peaks are rounded off by the clipper. Last comes a lookahead brickwall, which works
//! out what each sample needs taken off to stay under the ceiling: its true peak (read off a 4x
//! interpolation) or just the sample, through the style's knee. That amount is held for as long
//! as it reads ahead, then let go as the style lets go, either with one release or with a fast
//! one for short peaks and a slow one for long squeezes. It eases in over its attack through two
//! moving averages, so the gain is all the way down by the time the peak it saw arrives. No
//! sample can pass the ceiling: a hard stop catches what the averaging's rounding lets by.
//! Oversampled (2, 4 or 8 times), both the clipper and the gain run at that rate, the gain
//! glided across each sample, so neither folds what it makes above the top of the band back
//! into it.

mod fir;
pub mod learn;
mod meter;

pub use meter::Loudness;

use crate::{
    dsp::{
        comp::{ftz, pole},
        sat::{DcBlock, Splitter},
    },
    song::project::{LimitStyle, Limiter, MAX_SAT_BANDS},
};

/// The furthest ahead it reads, ms.
pub const MAX_LOOKAHEAD_MS: f32 = 10.0;
/// The true-peak interpolator's taps, and how many samples late it reads: everything waits
/// that long, true peak or not, so the delay doesn't change with it.
const TAPS: usize = 12;
const TP_DELAY: usize = 6;
/// Where the multiband style splits, Hz.
const SPLITS: [f32; MAX_SAT_BANDS - 1] = [150.0, 4000.0, 0.0, 0.0, 0.0];
/// Oversampled, how far ahead it reads what comes back down, to ease off the little that taking
/// the top off the band leaves over the ceiling.
const CATCH: usize = 12;

/// How many samples it holds the sound back: its lookahead and the interpolator's, and
/// (oversampled) the clipper's and the gain's trips up and back and the catch after.
pub fn latency(l: &Limiter, rate: f64) -> u32 {
    let up = match factor(l) {
        0 => 0,
        f => (2.0 * fir::trip(f)).round() as usize + CATCH,
    };
    (ahead(l, rate) + TP_DELAY + up) as u32
}

/// How many times it doubles the rate: 0 to 3.
fn factor(l: &Limiter) -> usize {
    match l.oversample {
        0..=1 => 0,
        2 => 1,
        3..=4 => 2,
        _ => 3,
    }
}

fn ahead(l: &Limiter, rate: f64) -> usize {
    let ms = l.lookahead_ms.clamp(0.1, MAX_LOOKAHEAD_MS);
    ((ms as f64 * rate / 1000.0).round() as usize).max(1)
}

/// How a style limits: its knee, how much it clips, its attack and release against what's set,
/// and (with auto release) its two stages.
#[derive(Clone, Copy)]
struct Character {
    /// dB wide, centred on the ceiling.
    knee: f32,
    /// Added to the clip that's set.
    clip: f32,
    attack: f32,
    release: f32,
    /// With auto release: the fast stage's and the slow stage's release against the release
    /// set, and how long the slow one takes to come down, ms.
    fast: f32,
    slow: f32,
    slow_attack: f32,
    /// How long it stays down after a peak before letting go, ms.
    hold: f32,
}

fn character(s: LimitStyle) -> Character {
    let c = |knee, clip, attack, release, fast, slow, slow_attack, hold| Character {
        knee,
        clip,
        attack,
        release,
        fast,
        slow,
        slow_attack,
        hold,
    };
    match s {
        LimitStyle::Transparent => c(2.0, 0.0, 1.0, 1.0, 0.4, 4.0, 60.0, 1.0),
        LimitStyle::Punchy => c(0.0, 0.35, 0.25, 0.6, 0.25, 3.0, 120.0, 0.0),
        LimitStyle::Dynamic => c(1.0, 0.1, 0.5, 1.0, 0.15, 6.0, 200.0, 0.5),
        LimitStyle::Allround => c(1.0, 0.1, 0.7, 1.0, 0.35, 3.0, 80.0, 0.5),
        LimitStyle::Aggressive => c(0.0, 0.5, 0.3, 0.4, 0.3, 2.0, 50.0, 0.0),
        LimitStyle::Modern => c(1.5, 0.3, 0.8, 1.0, 0.3, 3.5, 70.0, 1.0),
        LimitStyle::Bus => c(6.0, 0.05, 1.0, 2.0, 0.5, 4.0, 100.0, 2.0),
        LimitStyle::Safe => c(2.0, 0.0, 1.0, 1.5, 0.5, 4.0, 60.0, 2.0),
        LimitStyle::Classic => c(0.0, 0.0, 0.2, 1.0, 0.3, 3.0, 40.0, 0.0),
        LimitStyle::Multiband => c(1.0, 0.1, 0.7, 1.0, 0.35, 3.0, 80.0, 0.5),
    }
}

/// How much (dB) a peak `over` the ceiling by that much (dB) is taken down, through a knee
/// `knee` wide around it: never less than it's over.
fn need(over: f32, knee: f32) -> f32 {
    let h = knee / 2.0;
    match over {
        o if o >= h => o,
        o if o <= -h => 0.0,
        o => (o + h) * (o + h) / (2.0 * knee),
    }
}

/// A peak rounded off toward `t`: untouched under 0.7 of it, then curving into it.
fn clip(x: f32, t: f32) -> f32 {
    let (a, k) = (x.abs(), 0.7 * t);
    match a <= k {
        true => x,
        false => (k + (t - k) * ((a - k) / (t - k)).tanh()).copysign(x),
    }
}

/// The interpolator's taps for the three points between two samples (a quarter, half and
/// three quarters along): a windowed sinc, each phase's taps adding to one.
fn phases() -> [[f32; TAPS]; 3] {
    let w = TAPS as f64 / 2.0 + 0.5;
    std::array::from_fn(|j| {
        let t = (j + 1) as f64 / 4.0;
        let taps: [f64; TAPS] = std::array::from_fn(|i| {
            let u = TP_DELAY as f64 - i as f64 - t;
            let sinc = match u.abs() < 1e-9 {
                true => 1.0,
                false => (std::f64::consts::PI * u).sin() / (std::f64::consts::PI * u),
            };
            let a = std::f64::consts::PI * u / w;
            sinc * (0.42 + 0.5 * a.cos() + 0.08 * (2.0 * a).cos())
        });
        let sum: f64 = taps.iter().sum();
        taps.map(|c| (c / sum) as f32)
    })
}

/// A 4x interpolator, for true peaks: the last `TAPS` samples, twice over so they read in a row.
#[derive(Clone, Copy)]
pub(crate) struct Interp {
    hist: [f32; 2 * TAPS],
    at: usize,
}

impl Interp {
    const fn new() -> Self {
        Interp {
            hist: [0.0; 2 * TAPS],
            at: 0,
        }
    }

    /// A sample in: the one `TP_DELAY` back, and the loudest of the three points between it
    /// and the one after.
    fn push(&mut self, x: f32, phases: &[[f32; TAPS]; 3]) -> (f32, f32) {
        self.at = (self.at + 1) % TAPS;
        self.hist[self.at] = x;
        self.hist[self.at + TAPS] = x;
        // Newest first: hist[at + TAPS - i] is i samples back.
        let back = |i: usize| self.hist[self.at + TAPS - i];
        let mut most = 0f32;
        for p in phases {
            let v: f32 = (0..TAPS).map(|i| p[i] * back(i)).sum();
            most = most.max(v.abs());
        }
        (back(TP_DELAY), most)
    }
}

/// The most of the last `len` values, kept as a queue of (when, how much), the largest first.
struct Window {
    buf: Vec<(u64, f32)>,
    head: usize,
    count: usize,
}

impl Window {
    fn new(cap: usize) -> Self {
        Window {
            buf: vec![(0, 0.0); cap],
            head: 0,
            count: 0,
        }
    }

    fn run(&mut self, now: u64, v: f32, len: usize) -> f32 {
        let cap = self.buf.len();
        while self.count > 0 && self.buf[self.head].0 + len as u64 <= now {
            (self.head, self.count) = ((self.head + 1) % cap, self.count - 1);
        }
        while self.count > 0 && self.buf[(self.head + self.count - 1) % cap].1 <= v {
            self.count -= 1;
        }
        if self.count == cap {
            (self.head, self.count) = ((self.head + 1) % cap, self.count - 1);
        }
        self.buf[(self.head + self.count) % cap] = (now, v);
        self.count += 1;
        self.buf[self.head].1
    }

    fn clear(&mut self) {
        self.count = 0;
    }
}

/// A moving average over the last `len` values, `len` changeable as it runs.
struct Average {
    buf: Vec<f32>,
    len: usize,
    at: usize,
    sum: f64,
}

impl Average {
    fn new(cap: usize) -> Self {
        Average {
            buf: vec![1.0; cap],
            len: 1,
            at: 0,
            sum: 1.0,
        }
    }

    fn run(&mut self, x: f32, len: usize) -> f32 {
        if len != self.len {
            // Filled with the least it holds, so it can only be lower than it should while it
            // settles, never higher.
            let low = self.buf[..self.len].iter().fold(1f32, |m, &v| m.min(v));
            self.len = len.clamp(1, self.buf.len());
            self.at = 0;
            self.buf[..self.len].fill(low);
            self.sum = low as f64 * self.len as f64;
        }
        self.sum += x as f64 - self.buf[self.at] as f64;
        self.buf[self.at] = x;
        self.at = (self.at + 1) % self.len;
        (self.sum / self.len as f64).min(1.0) as f32
    }

    fn fill(&mut self, v: f32) {
        self.buf.fill(v);
        self.sum = v as f64 * self.len as f64;
    }
}

/// A delay line: what goes in comes out `d` samples later.
struct Line {
    buf: Vec<f32>,
    at: usize,
}

impl Line {
    fn new(cap: usize) -> Self {
        Line {
            buf: vec![0.0; cap],
            at: 0,
        }
    }

    fn run(&mut self, x: f32, d: usize) -> f32 {
        let cap = self.buf.len();
        self.buf[self.at] = x;
        let out = self.buf[(self.at + cap - d.min(cap - 1)) % cap];
        self.at = (self.at + 1) % cap;
        out
    }
}

/// One side, running.
struct Side {
    peaks: Interp,
    /// The loudest between the sample read last and the one it reads now.
    between: f32,
    most: Window,
    /// How much it's turned down (dB): the fast stage, the slow one, and how long it holds yet.
    fast: f32,
    slow: f32,
    held: usize,
    ease: [Average; 2],
    sound: Line,
    /// The sound before the clipper, for hearing what's taken away.
    dry: Line,
    /// Up and back down: the clipper's trip (which the limiter hears after, so it hears the
    /// sound already as the top of the band it can carry) and the gain's, and the dry sound's
    /// through the same. The gain's held back as long as the sound takes to get up there, and
    /// glided across each sample from the last.
    clip: [fir::Stage; 3],
    gain: [fir::Stage; 3],
    dry_clip: [fir::Stage; 3],
    dry_gain: [fir::Stage; 3],
    late: Line,
    was: f32,
    /// The catch after: what comes back down read for its true peaks as the limiter reads, how
    /// far over each of the next few went, the gain that takes them under eased in, and the
    /// sound waiting for it (and the dry sound).
    catch_peaks: Interp,
    catch_between: f32,
    over: Window,
    ease_catch: [Average; 2],
    caught: Line,
    dry_caught: Line,
    dc: DcBlock,
    split: Splitter,
    /// Each band's level, for the multiband style.
    bands: [f32; 3],
}

impl Side {
    fn new(cap: usize) -> Self {
        Side {
            peaks: Interp::new(),
            between: 0.0,
            most: Window::new(cap),
            fast: 0.0,
            slow: 0.0,
            held: 0,
            ease: [Average::new(cap), Average::new(cap)],
            sound: Line::new(cap + TP_DELAY + 80),
            dry: Line::new(cap + TP_DELAY + 80),
            clip: fir::stages(),
            gain: fir::stages(),
            dry_clip: fir::stages(),
            dry_gain: fir::stages(),
            late: Line::new(32),
            was: 1.0,
            catch_peaks: Interp::new(),
            catch_between: 0.0,
            over: Window::new(CATCH + 2),
            ease_catch: [Average::new(CATCH + 2), Average::new(CATCH + 2)],
            caught: Line::new(CATCH + 2),
            dry_caught: Line::new(CATCH + 2),
            dc: DcBlock::default(),
            split: Splitter::default(),
            bands: [0.0; 3],
        }
    }

    /// Lets go of everything it was turning down; with `sound`, of what it was holding back too.
    fn reset(&mut self, sound: bool) {
        self.between = 0.0;
        self.most.clear();
        (self.fast, self.slow, self.held, self.was) = (0.0, 0.0, 0, 1.0);
        self.ease.iter_mut().for_each(|a| a.fill(1.0));
        self.over.clear();
        self.catch_between = 0.0;
        self.ease_catch.iter_mut().for_each(|a| a.fill(1.0));
        self.bands = [0.0; 3];
        if sound {
            self.peaks = Interp::new();
            self.sound.buf.fill(0.0);
            self.dry.buf.fill(0.0);
        }
    }
}

/// What a block through the limiter came to, for its meters.
#[derive(Clone, Copy, Default, Debug)]
pub struct LimitReport {
    /// The loudest sample in (driven) and out.
    pub input: f32,
    pub output: f32,
    /// The most it turned down, dB.
    pub reduction: f32,
}

/// A limiter's meter, as the window reads it: its peaks in and out (linear) and the most it
/// turned down (dB) since last read, and its loudness (LUFS; -inf before it's heard enough) and
/// loudest true peak (linear).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LimitMeter {
    pub input: f32,
    pub output: f32,
    pub reduction: f32,
    pub momentary: f32,
    pub short: f32,
    pub integrated: f32,
    pub true_peak: f32,
}

impl Default for LimitMeter {
    fn default() -> Self {
        LimitMeter {
            input: 0.0,
            output: 0.0,
            reduction: 0.0,
            momentary: f32::NEG_INFINITY,
            short: f32::NEG_INFINITY,
            integrated: f32::NEG_INFINITY,
            true_peak: 0.0,
        }
    }
}

/// The limiter, running: both sides and its loudness meter.
pub struct Limiting {
    rate: f32,
    sides: [Side; 2],
    phases: [[f32; TAPS]; 3],
    now: u64,
    pub meter: Loudness,
    /// What the last block came to.
    pub last: LimitReport,
    /// Samples the hard stop had to catch: rounding's, a hair over.
    pub(crate) caught: u64,
    /// The furthest over it went before the hard stop, against the ceiling.
    pub(crate) worst: f32,
}

impl Limiting {
    pub fn new(rate: f64) -> Self {
        // Room for the longest lookahead and the hold's margin when oversampled.
        let cap = (MAX_LOOKAHEAD_MS as f64 * rate / 1000.0).ceil() as usize + 4;
        Limiting {
            rate: rate as f32,
            sides: [Side::new(cap), Side::new(cap)],
            phases: phases(),
            now: 0,
            meter: Loudness::new(rate),
            last: LimitReport::default(),
            caught: 0,
            worst: 1.0,
        }
    }

    /// Starts afresh; `sound`, forgetting what it was holding back too (it's been off).
    pub fn reset(&mut self, sound: bool) {
        self.sides.iter_mut().for_each(|s| s.reset(sound));
    }

    /// A block through it, in place. `live`, it's heard as set to be heard (unity, delta) and
    /// metered; otherwise (a bounce) it's just limited. Returns the most it turned down, dB.
    pub fn process(&mut self, lim: &Limiter, l: &mut [f32], r: &mut [f32], live: bool) -> f32 {
        let rate = self.rate as f64;
        let d = ahead(lim, rate);
        let delay = d + TP_DELAY;
        let factor = factor(lim);
        if lim.bypass {
            let late = latency(lim, rate) as usize;
            for (side, s) in self.sides.iter_mut().zip([&mut *l, &mut *r]) {
                for x in s.iter_mut() {
                    *x = side.sound.run(*x, late);
                }
                side.reset(false);
            }
            self.last = LimitReport::default();
            return 0.0;
        }
        let c = character(lim.style);
        let safe = lim.style == LimitStyle::Safe;
        let gain = 10f32.powf(lim.gain_db.clamp(0.0, 30.0) / 20.0);
        let ceiling_db = lim.ceiling_db.clamp(-30.0, 0.0);
        let ceiling = 10f32.powf(ceiling_db / 20.0);
        let knee_from = ceiling * 10f32.powf(-c.knee / 40.0);
        let link = lim.link.clamp(0.0, 1.0);
        let true_peak = lim.true_peak || safe;
        let amount = match safe {
            true => 0.0,
            false => (lim.clip.clamp(0.0, 1.0) + c.clip).min(1.0),
        };
        let clip_at = ceiling * 10f32.powf(6.0 * (1.0 - amount) / 20.0);
        // The window it holds over (oversampled, a little longer: the sound reaches the gain up
        // there a fraction of a sample off), and the attack it eases in over: two averages, end
        // to end.
        let hold = d + 1 + if factor > 0 { 5 } else { 0 };
        // The gain a couple of samples ahead of the sound up there, the hold that much longer:
        // all the way down before a peak, whatever the fraction of a sample between them.
        let up_late = ((fir::trip(factor) / 2.0) as usize).saturating_sub(2);
        let ramp = match safe {
            true => d + 1,
            false => {
                let ms = lim.attack_ms.max(0.01) * c.attack;
                ((ms * self.rate / 1000.0).round() as usize).clamp(1, d + 1)
            }
        };
        let subs = 1 << factor;
        let eases = [ramp.div_ceil(2), ramp + 1 - ramp.div_ceil(2)];
        let ms = |v: f32| pole(v.max(0.01) * 0.001, self.rate);
        let release = lim.release_ms.clamp(1.0, 2000.0) * c.release;
        let (fast, slow, slow_attack) = match lim.auto_release {
            true => (
                ms(release * c.fast),
                ms(release * c.slow),
                ms(c.slow_attack),
            ),
            false => (ms(release), 0.0, 0.0),
        };
        let held = (c.hold * self.rate / 1000.0) as usize;
        let multiband = lim.style == LimitStyle::Multiband;
        if multiband {
            for s in &mut self.sides {
                s.split.tune(3, &SPLITS, 24, self.rate);
            }
        }
        // Lows let go slowest, highs quickest.
        let band_attack = [ms(2.0), ms(1.0), ms(0.5)];
        let band_release = [ms(release * 2.0), ms(release), ms(release * 0.6)];
        let unity = live && lim.unity;
        let delta = live && lim.delta;
        let back = match unity {
            true => 1.0 / gain,
            false => 1.0,
        };

        let n = l.len().min(r.len());
        let mut report = LimitReport::default();
        for i in 0..n {
            let mut x = [l[i] * gain, r[i] * gain];
            if lim.dc {
                for (v, s) in x.iter_mut().zip(&mut self.sides) {
                    *v = s.dc.run(*v);
                }
            }
            if multiband {
                x = self.bands(x, ceiling, link, &band_attack, &band_release);
            }
            report.input = report.input.max(x[0].abs()).max(x[1].abs());
            let dry = x;
            // The clipper's trip: oversampled, always taken, so the delay doesn't change as
            // it comes and goes.
            if amount > 0.0 || factor > 0 {
                for (v, s) in x.iter_mut().zip(&mut self.sides) {
                    let mut up = fir::upsample(&mut s.clip, *v, factor);
                    if amount > 0.0 {
                        for u in &mut up[..subs] {
                            *u = clip(*u, clip_at);
                        }
                    }
                    *v = fir::downsample(&mut s.clip, up, factor);
                }
            }
            // What each side needs taken off, its true peak or its sample.
            let mut peak = [0f32; 2];
            for (p, (v, s)) in peak.iter_mut().zip(x.iter().zip(&mut self.sides)) {
                let (at, ahead) = s.peaks.push(*v, &self.phases);
                *p = match true_peak {
                    true => at.abs().max(ahead).max(s.between),
                    false => at.abs(),
                };
                s.between = ahead;
            }
            let both = peak[0].max(peak[1]);
            for k in 0..2 {
                let s = &mut self.sides[k];
                let p = peak[k] + link * (both - peak[k]);
                let want = match p > knee_from {
                    true => need(20.0 * p.log10() - ceiling_db, c.knee),
                    false => 0.0,
                };
                let t = s.most.run(self.now, want, hold);
                if t >= s.fast {
                    (s.fast, s.held) = (t, held);
                } else if s.held > 0 {
                    s.held -= 1;
                } else {
                    s.fast = ftz(t + (s.fast - t) * fast);
                }
                if lim.auto_release {
                    let p = if t > s.slow { slow_attack } else { slow };
                    s.slow = ftz(t + (s.slow - t) * p);
                }
                let down = s.fast.max(s.slow);
                let g = match down > 1e-6 {
                    true => 10f32.powf(-down / 20.0),
                    false => 1.0,
                };
                let g = s.ease[0].run(g, eases[0]);
                let g = s.ease[1].run(g, eases[1]);
                let v = s.sound.run(x[k], delay);
                let out = match factor {
                    0 => v * g,
                    // Up, the gain (as late as the sound up there) glided across the samples
                    // between and stopped at the ceiling there, and back down.
                    _ => {
                        let g = s.late.run(g, up_late);
                        let mut up = fir::upsample(&mut s.gain, v, factor);
                        for (j, u) in up[..subs].iter_mut().enumerate() {
                            let at = s.was + (g - s.was) * (j + 1) as f32 / subs as f32;
                            *u = (*u * at).clamp(-ceiling, ceiling);
                        }
                        s.was = g;
                        let y = fir::downsample(&mut s.gain, up, factor);
                        // Caught: the most over in the next few samples, its gain eased in
                        // over them, so it's under by the time it's heard.
                        // Read as the limiter reads (the interpolator `TP_DELAY` late), the
                        // rest of the catch's lookahead to ease in over.
                        let (at, ahead) = s.catch_peaks.push(y, &self.phases);
                        let peak = match true_peak {
                            true => at.abs().max(ahead).max(s.catch_between),
                            false => at.abs(),
                        };
                        s.catch_between = ahead;
                        let over = (peak / ceiling).max(1.0);
                        let ahead = CATCH - TP_DELAY;
                        let most = s.over.run(self.now, over, ahead + 1);
                        let g = s.ease_catch[0].run(1.0 / most, ahead / 2);
                        let g = s.ease_catch[1].run(g, ahead / 2 + 1);
                        s.caught.run(y, CATCH) * g
                    }
                };
                if out.abs() > ceiling * 1.000_01 {
                    self.caught += 1;
                    self.worst = self.worst.max(out.abs() / ceiling);
                }
                let out = out.clamp(-ceiling, ceiling);
                report.output = report.output.max(out.abs());
                report.reduction = report.reduction.max(-20.0 * g.max(1e-9).log10());
                let heard = match delta {
                    // The sound as it came, through the same trips as the limited sound, so
                    // the two line up.
                    true => {
                        let mut v = dry[k];
                        if factor > 0 {
                            let up = fir::upsample(&mut s.dry_clip, v, factor);
                            v = fir::downsample(&mut s.dry_clip, up, factor);
                        }
                        v = s.dry.run(v, delay);
                        if factor > 0 {
                            let up = fir::upsample(&mut s.dry_gain, v, factor);
                            v = fir::downsample(&mut s.dry_gain, up, factor);
                            v = s.dry_caught.run(v, CATCH);
                        }
                        v - out
                    }
                    false => out,
                };
                match k {
                    0 => l[i] = heard * back,
                    _ => r[i] = heard * back,
                }
                if live {
                    self.meter.push(k, out);
                }
            }
            self.now += 1;
        }
        self.last = report;
        report.reduction
    }

    /// One sample's sides with each band held under the ceiling apart, then added back.
    fn bands(
        &mut self,
        x: [f32; 2],
        ceiling: f32,
        link: f32,
        attack: &[f32; 3],
        release: &[f32; 3],
    ) -> [f32; 2] {
        let parts = [
            self.sides[0].split.split(x[0], 3),
            self.sides[1].split.split(x[1], 3),
        ];
        let mut out = [0f32; 2];
        for b in 0..3 {
            let p = [parts[0][b].abs(), parts[1][b].abs()];
            let both = p[0].max(p[1]);
            for k in 0..2 {
                let v = p[k] + link * (both - p[k]);
                let e = &mut self.sides[k].bands[b];
                let a = if v > *e { attack[b] } else { release[b] };
                *e = ftz(v + (*e - v) * a);
                // Not a wall: the rest is the limiter's.
                let g = match *e > ceiling {
                    true => (ceiling / *e).powf(0.7),
                    false => 1.0,
                };
                out[k] += parts[k][b] * g;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests;
