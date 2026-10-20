//! tempo's EQ: up to 24 bands, each a bell, shelf, cut, notch, band pass or tilt, over the whole
//! sound or part of it (left, right, mid or side), still or moving with the sound (dynamic).
//!
//! Its filters are matched to their analog shapes (Vicanek's matched second-order filters: the
//! poles by impulse invariance, the zeros so the magnitude is the analog's at DC, at the band's
//! frequency and at Nyquist), so a bell or shelf near the top keeps its shape rather than
//! cramping as the bilinear transform's do. Steep cuts are Butterworth cascades, up to 96 dB an
//! octave. A band glides to new settings, fades in and out (taking one out doesn't click), and
//! costs nothing while it's flat. The curve the UI draws is worked out here from the same
//! filters, so what's drawn is what's heard.
//!
//! The Channel EQ before this was Faust (`fi.svf`); a fixed Faust graph runs every band it could
//! have, where this runs only the sections the bands in use need.

use std::f64::consts::{FRAC_1_SQRT_2, PI};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use crate::{
    engine::MAX_BLOCK,
    project::{Eq, EqBand, MAX_BANDS, Placement, Shape},
};

/// The most second-order sections a band takes: a 96 dB an octave cut.
pub const MAX_SECTIONS: usize = 8;

/// Samples between a moving band's filter updates.
const STEP: usize = 16;
/// Seconds a band takes to glide to new settings, and to fade in or out.
const GLIDE: f64 = 0.02;
const FADE: f64 = 0.03;
/// Bands playing at once: the EQ's, and as many again fading out.
const LIVE: usize = MAX_BANDS * 2;
/// A dynamic band's gain moves this much for each dB over its threshold (4:1), up to its range.
const DYNAMIC_RATIO: f64 = 0.75;

/// A second-order section (first-order when `b2` and `a2` are 0), its `a0` 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Section {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

/// sin²(ω/2) for a frequency: the variable the magnitudes are worked out in.
fn half_sin2(freq: f64, rate: f64) -> f64 {
    (PI * freq / rate).sin().powi(2)
}

/// |H|² in terms of a polynomial's values at DC and Nyquist and its middle term: for a
/// numerator or denominator, |B(e^jω)|² = B0 φ0 + B1 φ1 + B2 φ2 with φ1 = sin²(ω/2), φ0 = 1 - φ1
/// and φ2 = 4 φ0 φ1.
fn poly2(c: [f64; 3], s: f64) -> f64 {
    let (p0, p1) = (1.0 - s, s);
    c[0] * p0 + c[1] * p1 + c[2] * 4.0 * p0 * p1
}

impl Section {
    /// Its magnitude squared, at sin²(ω/2) = `s`.
    fn mag2(&self, s: f64) -> f64 {
        let num = [
            (self.b0 + self.b1 + self.b2).powi(2),
            (self.b0 - self.b1 + self.b2).powi(2),
            -4.0 * self.b0 * self.b2,
        ];
        let den = [
            (1.0 + self.a1 + self.a2).powi(2),
            (1.0 - self.a1 + self.a2).powi(2),
            -4.0 * self.a2,
        ];
        (poly2(num, s) / poly2(den, s).max(1e-300)).max(0.0)
    }

    /// Scaled by a gain (linear).
    fn times(self, g: f64) -> Section {
        Section {
            b0: self.b0 * g,
            b1: self.b1 * g,
            b2: self.b2 * g,
            ..self
        }
    }
}

/// A second-order section matched to an analog one: its poles the analog's (`wp` rad/s, `qp`),
/// by impulse invariance; its zeros so its magnitude is the analog's (`mag2`: |H(jΩ)|² at Ω
/// rad/s) at DC, at Nyquist and at `wm` rad/s.
fn matched2(wp: f64, qp: f64, wm: f64, rate: f64, mag2: &dyn Fn(f64) -> f64) -> Section {
    let t = 1.0 / rate;
    // Poles past Nyquist would fold back: kept under it, the magnitude still matches.
    let wp = wp.min(0.95 * PI * rate);
    let zeta = 0.5 / qp.max(1e-3);
    let r = (-zeta * wp * t).exp();
    let a1 = match zeta < 1.0 {
        true => -2.0 * r * (wp * t * (1.0 - zeta * zeta).sqrt()).cos(),
        false => -2.0 * r * (wp * t * (zeta * zeta - 1.0).sqrt()).cosh(),
    };
    let a2 = r * r;
    let den = [(1.0 + a1 + a2).powi(2), (1.0 - a1 + a2).powi(2), -4.0 * a2];
    let wm = (wm * t).clamp(1e-6, 0.9 * PI);
    let s = (wm / 2.0).sin().powi(2);
    let b0_ = den[0] * mag2(0.0);
    let b1_ = den[1] * mag2(PI * rate);
    let want = mag2(wm * rate) * poly2(den, s);
    let (p0, p1) = (1.0 - s, s);
    let b2_ = (want - b0_ * p0 - b1_ * p1) / (4.0 * p0 * p1);
    let (r0, r1) = (b0_.max(0.0).sqrt(), b1_.max(0.0).sqrt());
    let w = 0.5 * (r0 + r1);
    let b0 = 0.5 * (w + (w * w + b2_).max(0.0).sqrt());
    let b2 = if b0 > 0.0 { -b2_ / (4.0 * b0) } else { 0.0 };
    Section {
        b0,
        b1: 0.5 * (r0 - r1),
        b2,
        a1,
        a2,
    }
}

/// A first-order section matched to an analog one: its pole the analog's (`wp` rad/s), its
/// magnitude the analog's at DC and Nyquist.
fn matched1(wp: f64, rate: f64, mag2: &dyn Fn(f64) -> f64) -> Section {
    let a1 = -(-wp.min(0.95 * PI * rate) / rate).exp();
    let dc = (1.0 + a1) * mag2(0.0).sqrt();
    let ny = (1.0 - a1) * mag2(PI * rate).sqrt();
    Section {
        b0: 0.5 * (dc + ny),
        b1: 0.5 * (dc - ny),
        b2: 0.0,
        a1,
        a2: 0.0,
    }
}

/// The analog second-order shapes' |H(jΩ)|², about `w` rad/s.
fn resonance(w: f64, q: f64, x: f64) -> f64 {
    (w * w - x * x).powi(2) + (w * x / q).powi(2)
}

fn lowpass2(w: f64, q: f64) -> impl Fn(f64) -> f64 {
    move |x| w.powi(4) / resonance(w, q, x)
}

fn highpass2(w: f64, q: f64) -> impl Fn(f64) -> f64 {
    move |x| x.powi(4) / resonance(w, q, x)
}

fn bandpass2(w: f64, q: f64) -> impl Fn(f64) -> f64 {
    move |x| (w * x / q).powi(2) / resonance(w, q, x)
}

/// A shelf's analog shape (RBJ's), `a` its gain's square root (10^(dB/40)): low or high.
fn shelf2(w: f64, q: f64, a: f64, high: bool) -> impl Fn(f64) -> f64 {
    move |x| {
        let u2 = (x / w).powi(2);
        let k = a / (q * q) * u2;
        let (top, bottom) = ((a - u2).powi(2) + k, (1.0 - a * u2).powi(2) + k);
        match high {
            false => a * a * top / bottom,
            true => a * a * bottom / top,
        }
    }
}

/// A first-order shelf, `g` its gain (linear), half of it (in dB) at `w`.
fn shelf1(w: f64, g: f64, high: bool) -> impl Fn(f64) -> f64 {
    move |x| match high {
        false => (x * x + w * w * g) / (x * x + w * w / g),
        true => (g * g * x * x + w * w * g) / (x * x + w * w * g),
    }
}

/// A band's sections at its settings, its gain as heard `gain_db`: how many it takes (none when
/// it does nothing).
pub fn sections(
    band: &EqBand,
    gain_db: f64,
    rate: f64,
    out: &mut [Section; MAX_SECTIONS],
) -> usize {
    let freq = band.freq as f64;
    if band.shape == Shape::HighCut && freq >= 0.5 * rate {
        return 0;
    }
    let w = 2.0 * PI * freq.clamp(10.0, 0.49 * rate);
    let q = (band.q as f64).clamp(0.025, 40.0);
    match band.shape {
        Shape::Bell => {
            if gain_db.abs() < 1e-4 {
                return 0;
            }
            let a = 10f64.powf(gain_db / 40.0);
            let mag2 = move |x: f64| {
                let d = (w * w - x * x).powi(2);
                (d + (a * w * x / q).powi(2)) / (d + (w * x / (a * q)).powi(2))
            };
            out[0] = matched2(w, a * q, w, rate, &mag2);
            1
        }
        Shape::Notch => {
            out[0] = matched2(w, q, w, rate, &move |x| {
                (w * w - x * x).powi(2) / resonance(w, q, x)
            });
            1
        }
        Shape::BandPass => {
            out[0] = matched2(w, q, w, rate, &bandpass2(w, q));
            1
        }
        Shape::LowCut | Shape::HighCut => {
            let high = band.shape == Shape::LowCut;
            let order = (band.slope as usize / 6).clamp(1, 16);
            let (pairs, odd) = (order / 2, order % 2 == 1);
            let mut n = 0;
            if odd {
                out[n] = match high {
                    true => matched1(w, rate, &move |x| x * x / (x * x + w * w)),
                    false => matched1(w, rate, &move |x| w * w / (x * x + w * w)),
                };
                n += 1;
            }
            for k in 0..pairs {
                // Butterworth's: the last, sharpest pair takes the band's resonance.
                let angle = match odd {
                    true => (k + 1) as f64 * PI / order as f64,
                    false => (2 * k + 1) as f64 * PI / (2 * order) as f64,
                };
                let mut qk = 0.5 / angle.cos();
                if k + 1 == pairs {
                    qk *= q / FRAC_1_SQRT_2;
                }
                out[n] = match high {
                    true => matched2(w, qk, w, rate, &highpass2(w, qk)),
                    false => matched2(w, qk, w, rate, &lowpass2(w, qk)),
                };
                n += 1;
            }
            n
        }
        Shape::LowShelf | Shape::HighShelf | Shape::TiltShelf => {
            if gain_db.abs() < 1e-4 {
                return 0;
            }
            let high = band.shape != Shape::LowShelf;
            let slope = band.slope.clamp(6, 48) as usize;
            let (pairs, odd) = (slope / 12, slope % 12 >= 6);
            let total = (12 * pairs + if odd { 6 } else { 0 }) as f64;
            let mut n = 0;
            if odd {
                let g = 10f64.powf(gain_db * 6.0 / total / 20.0);
                let wp = if high { w * g.sqrt() } else { w / g.sqrt() };
                out[n] = matched1(wp, rate, &shelf1(w, g, high));
                n += 1;
            }
            for _ in 0..pairs {
                let a = 10f64.powf(gain_db * 12.0 / total / 40.0);
                let wp = if high { w * a.sqrt() } else { w / a.sqrt() };
                out[n] = matched2(wp, q, w, rate, &shelf2(w, q, a, high));
                n += 1;
            }
            // A tilt is a high shelf, half its gain down: as much down below as up above.
            if band.shape == Shape::TiltShelf {
                out[0] = out[0].times(10f64.powf(-gain_db / 40.0));
            }
            n
        }
    }
}

/// A band's sections, for drawing: none if it's off.
pub fn band_sections(band: &EqBand, scale: f32, rate: f64) -> Vec<Section> {
    let mut out = [Section::default(); MAX_SECTIONS];
    let n = sections(band, band.gain(scale) as f64, rate, &mut out);
    out[..n].to_vec()
}

/// What sections do at `freq` (Hz), in dB.
pub fn response_db(sections: &[Section], freq: f64, rate: f64) -> f64 {
    let s = half_sin2(freq.min(0.5 * rate), rate);
    let m: f64 = sections.iter().map(|c| c.mag2(s)).product();
    10.0 * m.max(1e-30).log10()
}

/// What a band does at `freq`, dB (as if it were on, and on the whole sound).
pub fn band_response(band: &EqBand, scale: f32, freq: f64, rate: f64) -> f64 {
    response_db(&band_sections(band, scale, rate), freq, rate)
}

/// What a whole EQ does at `freq`, in dB, to the middle of the sound: its bands that are on and
/// work on all of it or its middle, and its output level.
pub fn eq_response(eq: &Eq, freq: f64, rate: f64) -> f64 {
    let bands: f64 = eq
        .bands
        .iter()
        .filter(|b| !b.off && matches!(b.placement, Placement::Stereo | Placement::Mid))
        .map(|b| band_response(b, eq.scale, freq, rate))
        .sum();
    bands + eq.output_db as f64
}

/// Runs samples through sections, one lane's state.
#[inline]
fn cascade(sections: &[Section], state: &mut [[f64; 2]], mut x: f64) -> f64 {
    for (c, st) in sections.iter().zip(state.iter_mut()) {
        let y = c.b0 * x + st[0];
        st[0] = c.b1 * x - c.a1 * y + st[1];
        st[1] = c.b2 * x - c.a2 * y;
        x = y;
    }
    x
}

/// No denormals in what's left ringing.
fn flush(state: &mut [[[f64; 2]; MAX_SECTIONS]; 2]) {
    for st in state.iter_mut().flatten().flatten() {
        if st.abs() < 1e-30 {
            *st = 0.0;
        }
    }
}

/// Both sides through sections at once: two chains the processor can run side by side.
#[inline]
fn cascade2(
    sections: &[Section],
    sl: &mut [[f64; 2]],
    sr: &mut [[f64; 2]],
    mut a: f64,
    mut b: f64,
) -> (f64, f64) {
    for ((c, p), q) in sections.iter().zip(sl.iter_mut()).zip(sr.iter_mut()) {
        let ya = c.b0 * a + p[0];
        let yb = c.b0 * b + q[0];
        p[0] = c.b1 * a - c.a1 * ya + p[1];
        q[0] = c.b1 * b - c.a1 * yb + q[1];
        p[1] = c.b2 * a - c.a2 * ya;
        q[1] = c.b2 * b - c.a2 * yb;
        (a, b) = (ya, yb);
    }
    (a, b)
}

/// What a dynamic band listens to: what's around its frequency.
fn detector(band: &EqBand, rate: f64) -> Section {
    let w = 2.0 * PI * (band.freq as f64).clamp(10.0, 0.49 * rate);
    let q = (band.q as f64).clamp(0.3, 40.0);
    match band.shape {
        Shape::LowShelf => matched2(w, FRAC_1_SQRT_2, w, rate, &lowpass2(w, FRAC_1_SQRT_2)),
        Shape::HighShelf | Shape::TiltShelf => {
            matched2(w, FRAC_1_SQRT_2, w, rate, &highpass2(w, FRAC_1_SQRT_2))
        }
        _ => matched2(w, q, w, rate, &bandpass2(w, q)),
    }
}

/// A band as it plays.
#[derive(Clone, Copy)]
struct Live {
    /// The band's id; 0 a free place.
    id: u16,
    /// Taken out of the EQ: fading to nothing, then freed.
    gone: bool,
    band: EqBand,
    /// Where it is on its glide to `band`: log2 Hz, dB, log2 Q.
    at: [f64; 3],
    /// How much of it is heard, 0 to 1: it fades in and out.
    mix: f64,
    sections: [Section; MAX_SECTIONS],
    n: usize,
    /// Each lane's sections' memory.
    state: [[[f64; 2]; MAX_SECTIONS]; 2],
    /// Its sections need working out again.
    stale: bool,
    /// A dynamic band's ear, its memory, its level (linear) and how far it's moved now (dB).
    det: Section,
    det_state: [[f64; 2]; 2],
    env: f64,
    moved: f64,
}

impl Live {
    const FREE: Live = Live {
        id: 0,
        gone: false,
        band: EqBand {
            shape: Shape::Bell,
            freq: 1_000.0,
            gain_db: 0.0,
            q: 1.0,
            slope: 12,
            placement: Placement::Stereo,
            dynamic: None,
            off: false,
            id: 0,
        },
        at: [0.0; 3],
        mix: 0.0,
        sections: [Section {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }; MAX_SECTIONS],
        n: 0,
        state: [[[0.0; 2]; MAX_SECTIONS]; 2],
        stale: true,
        det: Section {
            b0: 0.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        },
        det_state: [[0.0; 2]; 2],
        env: 0.0,
        moved: 0.0,
    };

    fn target(band: &EqBand) -> [f64; 3] {
        [
            (band.freq as f64).max(1.0).log2(),
            band.gain_db as f64,
            (band.q as f64).max(0.001).log2(),
        ]
    }

    fn start(band: EqBand, mix: f64, rate: f64) -> Live {
        Live {
            id: band.id,
            band,
            at: Live::target(&band),
            mix,
            det: detector(&band, rate),
            ..Live::FREE
        }
    }

    /// The band as it is on its glide.
    fn now(&self) -> EqBand {
        EqBand {
            freq: self.at[0].exp2() as f32,
            gain_db: self.at[1] as f32,
            q: self.at[2].exp2() as f32,
            ..self.band
        }
    }

    /// Takes new settings: a new shape, slope or placement starts its filters afresh.
    fn set(&mut self, band: EqBand, rate: f64) {
        let o = &self.band;
        if (o.shape, o.slope, o.placement) != (band.shape, band.slope, band.placement) {
            self.state = [[[0.0; 2]; MAX_SECTIONS]; 2];
            self.det_state = [[0.0; 2]; 2];
            self.at = Live::target(&band);
            self.stale = true;
        }
        if (o.freq, o.q, o.shape) != (band.freq, band.q, band.shape) {
            self.det = detector(&band, rate);
        }
        self.band = band;
    }
}

/// An EQ, playing: its bands' filters and memory, which carry on through edits.
pub struct Processor {
    rate: f64,
    live: [Live; LIVE],
    /// The bands' places in `live`, in the order they run: the EQ's, then those fading out.
    order: [u8; LIVE],
    /// Whether it ran last block: one coming back on starts with its bands as they are.
    running: bool,
    output: f64,
    /// A band heard alone: its id, the filter it's heard through and that filter's memory.
    solo: (u16, Section, [[f64; 2]; 2]),
    /// A copy of the block going in, for the solo and the analyzer.
    input: [[f32; MAX_BLOCK]; 2],
}

impl Processor {
    pub fn new(rate: f64) -> Self {
        Processor {
            rate,
            live: [Live::FREE; LIVE],
            order: [0; LIVE],
            running: false,
            output: 1.0,
            solo: (0, Section::default(), [[0.0; 2]; 2]),
            input: [[0.0; MAX_BLOCK]; 2],
        }
    }

    /// The EQ didn't run this block (it's off, or not there): next time it starts afresh.
    pub fn rest(&mut self) {
        self.running = false;
    }

    /// The bands' places, matched to the EQ's by id: new ones take a free place, those gone fade.
    fn reconcile(&mut self, eq: &Eq) -> usize {
        let fresh = !self.running;
        if fresh {
            self.live = [Live::FREE; LIVE];
            self.output = 10f64.powf(eq.output_db as f64 / 20.0);
        }
        for l in self.live.iter_mut().filter(|l| l.id != 0) {
            l.gone = !eq.bands.iter().any(|b| b.id == l.id);
        }
        let mut n = 0;
        for band in eq.bands.iter() {
            let found = self.live.iter().position(|l| l.id == band.id && l.id != 0);
            let at = match found {
                Some(i) => {
                    self.live[i].set(*band, self.rate);
                    i
                }
                None => {
                    // A free place, else the quietest of those fading out.
                    let i = self.live.iter().position(|l| l.id == 0).unwrap_or_else(|| {
                        (0..LIVE)
                            .filter(|&i| self.live[i].gone)
                            .min_by(|&a, &b| self.live[a].mix.total_cmp(&self.live[b].mix))
                            .unwrap_or(0)
                    });
                    let mix = if fresh && !band.off { 1.0 } else { 0.0 };
                    self.live[i] = Live::start(*band, mix, self.rate);
                    i
                }
            };
            self.order[n] = at as u8;
            n += 1;
        }
        for i in 0..LIVE {
            if self.live[i].id != 0 && self.live[i].gone && n < LIVE {
                self.order[n] = i as u8;
                n += 1;
            }
        }
        self.running = true;
        n
    }

    /// Runs a block (at most `MAX_BLOCK`) through the EQ in place. `solo` is a band to hear
    /// alone, by id, and its listening level; `tap` gets what went in and came out, and the
    /// dynamic bands' moves.
    pub fn process(
        &mut self,
        eq: &Eq,
        solo: Option<(u16, f32)>,
        l: &mut [f32],
        r: &mut [f32],
        tap: Option<&Tap>,
    ) {
        let frames = l.len().min(r.len()).min(MAX_BLOCK);
        let (l, r) = (&mut l[..frames], &mut r[..frames]);
        let count = self.reconcile(eq);
        let keep = solo.is_some() || tap.is_some();
        if keep {
            self.input[0][..frames].copy_from_slice(l);
            self.input[1][..frames].copy_from_slice(r);
        }
        let rate = self.rate;
        let glide = 1.0 - (-(STEP as f64) / (GLIDE * rate)).exp();
        let fade = STEP as f64 / (FADE * rate);
        let scale = eq.scale as f64;
        let mut start = 0;
        while start < frames {
            let end = (start + STEP).min(frames);
            let len = end - start;
            for k in 0..count {
                let live = &mut self.live[self.order[k] as usize];
                let target_mix: f64 = if live.gone || live.band.off { 0.0 } else { 1.0 };
                if live.mix == 0.0 && target_mix == 0.0 {
                    if live.gone {
                        live.id = 0;
                    }
                    continue;
                }
                // Glide.
                let want = Live::target(&live.band);
                for (a, w) in live.at.iter_mut().zip(want) {
                    if (*a - w).abs() > 1e-4 {
                        *a += (w - *a) * glide;
                        live.stale = true;
                    } else if *a != w {
                        *a = w;
                        live.stale = true;
                    }
                }
                // A dynamic band's move, from what it's heard.
                let dynamic = live.band.dynamic.filter(|_| live.band.shape.has_gain());
                if let Some(d) = dynamic {
                    let over = 20.0 * live.env.max(1e-9).log10() - d.threshold_db as f64;
                    let range = d.range_db as f64;
                    let moved = (over * DYNAMIC_RATIO).clamp(0.0, range.abs()) * range.signum();
                    if (moved - live.moved).abs() > 0.01 {
                        live.moved = moved;
                        live.stale = true;
                    }
                } else if live.moved != 0.0 {
                    live.moved = 0.0;
                    live.stale = true;
                }
                if live.stale {
                    let now = live.now();
                    let gain = (now.gain_db as f64 + live.moved) * scale;
                    let gain = if now.shape.has_gain() { gain } else { 0.0 };
                    live.n = sections(&now, gain, rate, &mut live.sections);
                    live.stale = false;
                }
                let m0 = live.mix;
                let m1 = match target_mix.total_cmp(&m0) {
                    std::cmp::Ordering::Greater => (m0 + fade).min(1.0),
                    std::cmp::Ordering::Less => (m0 - fade).max(0.0),
                    std::cmp::Ordering::Equal => m0,
                };
                live.mix = m1;
                let lanes = match live.band.placement {
                    Placement::Stereo => [true, true],
                    Placement::Left | Placement::Mid => [true, false],
                    Placement::Right | Placement::Side => [false, true],
                };
                let ms = matches!(live.band.placement, Placement::Mid | Placement::Side);
                let (attack, release) = match dynamic {
                    Some(d) => (
                        1.0 - (-1.0 / (d.attack_ms.max(0.1) as f64 * 1e-3 * rate)).exp(),
                        1.0 - (-1.0 / (d.release_ms.max(1.0) as f64 * 1e-3 * rate)).exp(),
                    ),
                    None => (0.0, 0.0),
                };
                let sections = &live.sections[..live.n];
                // Most bands: on both sides or one, still, all in. Each side a tight run.
                if !ms && dynamic.is_none() && m0 == 1.0 && m1 == 1.0 && lanes == [true, true] {
                    let [sl, sr] = &mut live.state;
                    for i in start..end {
                        let (a, b) = cascade2(sections, sl, sr, l[i] as f64, r[i] as f64);
                        (l[i], r[i]) = (a as f32, b as f32);
                    }
                    flush(&mut live.state);
                    continue;
                }
                if !ms && dynamic.is_none() && m0 == 1.0 && m1 == 1.0 {
                    for (lane, side) in [&mut *l, &mut *r].into_iter().enumerate() {
                        if lanes[lane] {
                            for x in &mut side[start..end] {
                                *x = cascade(sections, &mut live.state[lane], *x as f64) as f32;
                            }
                        }
                    }
                    flush(&mut live.state);
                    continue;
                }
                for i in start..end {
                    let (x0, x1) = match ms {
                        true => (
                            (l[i] as f64 + r[i] as f64) * 0.5,
                            (l[i] as f64 - r[i] as f64) * 0.5,
                        ),
                        false => (l[i] as f64, r[i] as f64),
                    };
                    let x = [x0, x1];
                    let mut y = x;
                    let mut level = 0f64;
                    for lane in 0..2 {
                        if !lanes[lane] {
                            continue;
                        }
                        if dynamic.is_some() {
                            let d = cascade(
                                std::slice::from_ref(&live.det),
                                std::slice::from_mut(&mut live.det_state[lane]),
                                x[lane],
                            );
                            level = level.max(d.abs());
                        }
                        y[lane] = cascade(sections, &mut live.state[lane], x[lane]);
                    }
                    if dynamic.is_some() {
                        let k = if level > live.env { attack } else { release };
                        live.env += (level - live.env) * k;
                    }
                    if m0 < 1.0 || m1 < 1.0 {
                        let m = m0 + (m1 - m0) * ((i - start + 1) as f64 / len as f64);
                        y = [x[0] + (y[0] - x[0]) * m, x[1] + (y[1] - x[1]) * m];
                    }
                    let (a, b) = match ms {
                        true => (y[0] + y[1], y[0] - y[1]),
                        false => (y[0], y[1]),
                    };
                    l[i] = a as f32;
                    r[i] = b as f32;
                }
                flush(&mut live.state);
            }
            // The output level, gliding.
            let to = 10f64.powf(eq.output_db as f64 / 20.0);
            let from = self.output;
            let next = from + (to - from) * glide;
            let next = if (next - to).abs() < 1e-6 { to } else { next };
            if from != 1.0 || next != 1.0 {
                for i in start..end {
                    let g = from + (next - from) * ((i - start + 1) as f64 / len as f64);
                    l[i] = (l[i] as f64 * g) as f32;
                    r[i] = (r[i] as f64 * g) as f32;
                }
            }
            self.output = next;
            start = end;
        }
        self.hear_alone(eq, solo, l, r);
        if let Some(tap) = tap {
            tap.write(&self.input[0][..frames], &self.input[1][..frames], l, r);
            for (i, band) in eq.bands.iter().enumerate() {
                let moved = self
                    .live
                    .iter()
                    .find(|x| x.id == band.id)
                    .map_or(0.0, |x| x.moved * scale);
                tap.moved[i].store((moved as f32).to_bits(), Ordering::Relaxed);
            }
        }
    }

    /// A band heard alone, at a listening level (dB): what it works on of what's going in. A
    /// bell's, notch's or band pass's stretch round its frequency; a shelf's side of it; for a
    /// cut, what it takes away, so you hear whether it's cutting the right thing.
    fn hear_alone(&mut self, eq: &Eq, solo: Option<(u16, f32)>, l: &mut [f32], r: &mut [f32]) {
        let Some((band, level)) =
            solo.and_then(|(id, db)| eq.bands.iter().find(|b| b.id == id).map(|b| (b, db)))
        else {
            self.solo.0 = 0;
            return;
        };
        let w = 2.0 * PI * (band.freq as f64).clamp(10.0, 0.49 * self.rate);
        let q = (band.q as f64).clamp(0.3, 40.0);
        let r2 = FRAC_1_SQRT_2;
        let filter = match band.shape {
            Shape::LowCut | Shape::LowShelf => matched2(w, r2, w, self.rate, &lowpass2(w, r2)),
            Shape::HighCut | Shape::HighShelf | Shape::TiltShelf => {
                matched2(w, r2, w, self.rate, &highpass2(w, r2))
            }
            _ => matched2(w, q, w, self.rate, &bandpass2(w, q)),
        };
        if self.solo.0 != band.id {
            self.solo.2 = [[0.0; 2]; 2];
        }
        self.solo.0 = band.id;
        self.solo.1 = filter;
        let gain = 10f64.powf(level as f64 / 20.0);
        let frames = l.len();
        for (lane, out) in [l, r].into_iter().enumerate() {
            for (i, o) in out.iter_mut().enumerate().take(frames) {
                let x = self.input[lane][i] as f64;
                *o = (cascade(
                    std::slice::from_ref(&filter),
                    std::slice::from_mut(&mut self.solo.2[lane]),
                    x,
                ) * gain) as f32;
            }
        }
    }
}

/// Samples the analyzer keeps: enough for 6 Hz bins at 48 kHz.
pub const TAP_LEN: usize = 8192;

/// What the EQ window watches, shared with the audio thread: which lane's EQ, a band to hear
/// alone and how loud, and that EQ's latest sound going in (mono) and coming out (each side),
/// and its dynamic bands' moves.
pub struct Tap {
    /// The lane watched (`usize::MAX`: none).
    pub slot: AtomicUsize,
    /// A band of it heard alone, by id (0: none), and the level it's heard at (dB, f32 bits).
    pub solo: AtomicU32,
    pub solo_db: AtomicU32,
    pre: Vec<AtomicU32>,
    post: [Vec<AtomicU32>; 2],
    /// Samples written, ever: where the next goes.
    written: AtomicUsize,
    /// Each band's dynamic move now, dB (f32 bits), by its place in the EQ.
    moved: Vec<AtomicU32>,
}

impl Default for Tap {
    fn default() -> Self {
        Tap::new()
    }
}

impl Tap {
    pub fn new() -> Self {
        let ring = || (0..TAP_LEN).map(|_| AtomicU32::new(0)).collect();
        Tap {
            slot: AtomicUsize::new(usize::MAX),
            solo: AtomicU32::new(0),
            solo_db: AtomicU32::new(0),
            pre: ring(),
            post: [ring(), ring()],
            written: AtomicUsize::new(0),
            moved: (0..MAX_BANDS).map(|_| AtomicU32::new(0)).collect(),
        }
    }

    /// Whether `slot`'s EQ is watched.
    pub fn watches(&self, slot: usize) -> bool {
        self.slot.load(Ordering::Relaxed) == slot
    }

    /// The band heard alone, and its level.
    pub fn solo(&self) -> Option<(u16, f32)> {
        match self.solo.load(Ordering::Relaxed) {
            0 => None,
            id => Some((
                id as u16,
                f32::from_bits(self.solo_db.load(Ordering::Relaxed)),
            )),
        }
    }

    fn write(&self, pre_l: &[f32], pre_r: &[f32], l: &[f32], r: &[f32]) {
        let mut at = self.written.load(Ordering::Relaxed);
        for i in 0..l.len() {
            let k = at % TAP_LEN;
            let pre = 0.5 * (pre_l[i] + pre_r[i]);
            self.pre[k].store(pre.to_bits(), Ordering::Relaxed);
            self.post[0][k].store(l[i].to_bits(), Ordering::Relaxed);
            self.post[1][k].store(r[i].to_bits(), Ordering::Relaxed);
            at += 1;
        }
        self.written.store(at, Ordering::Release);
    }

    /// The latest `TAP_LEN` samples going in, and coming out on each side, oldest first; and
    /// how many have been written ever (unchanged: nothing new).
    pub fn read(
        &self,
        pre: &mut [f32; TAP_LEN],
        left: &mut [f32; TAP_LEN],
        right: &mut [f32; TAP_LEN],
    ) -> usize {
        let at = self.written.load(Ordering::Acquire);
        for i in 0..TAP_LEN {
            let k = (at + i) % TAP_LEN;
            pre[i] = f32::from_bits(self.pre[k].load(Ordering::Relaxed));
            left[i] = f32::from_bits(self.post[0][k].load(Ordering::Relaxed));
            right[i] = f32::from_bits(self.post[1][k].load(Ordering::Relaxed));
        }
        at
    }

    /// The dynamic move of the EQ's band at `i`, dB.
    pub fn moved(&self, i: usize) -> f32 {
        self.moved
            .get(i)
            .map_or(0.0, |m| f32::from_bits(m.load(Ordering::Relaxed)))
    }
}

/// A sound's spectrum for the analyzer: its level (dB, a full-scale sine 0) at each of `freqs`
/// (rising), the loudest FFT bin between it and its neighbours, or between bins where they're
/// sparse. Hann windowed.
pub fn spectrum_db(samples: &[f32], rate: f64, freqs: &[f64]) -> Vec<f32> {
    use rustfft::{FftPlanner, num_complex::Complex};
    thread_local! {
        static PLANNER: std::cell::RefCell<FftPlanner<f32>> = std::cell::RefCell::new(FftPlanner::new());
    }
    let n = samples.len();
    if n < 4 || freqs.is_empty() {
        return vec![-120.0; freqs.len()];
    }
    let fft = PLANNER.with(|p| p.borrow_mut().plan_fft_forward(n));
    let mut sum = 0.0;
    let mut buf: Vec<Complex<f32>> = samples
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos();
            sum += w;
            Complex::new(x * w, 0.0)
        })
        .collect();
    fft.process(&mut buf);
    let mag: Vec<f32> = buf[..n / 2].iter().map(|c| c.norm() * 2.0 / sum).collect();
    let bin = rate / n as f64;
    let at = |f: f64| {
        let k = f / bin;
        let i = (k.floor() as usize).min(mag.len() - 2);
        let t = (k - i as f64).clamp(0.0, 1.0) as f32;
        mag[i] * (1.0 - t) + mag[i + 1] * t
    };
    (0..freqs.len())
        .map(|c| {
            let f = freqs[c];
            let lo = if c > 0 { (freqs[c - 1] * f).sqrt() } else { f };
            let hi = freqs.get(c + 1).map_or(f, |&g| (g * f).sqrt());
            let (a, b) = ((lo / bin).ceil() as usize, (hi / bin).floor() as usize);
            let m = match a <= b && b < mag.len() {
                true => mag[a..=b].iter().fold(0f32, |m, &x| m.max(x)),
                false => at(f),
            };
            20.0 * m.max(1e-7).log10()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Bands, Dynamic};

    const RATE: f64 = 48_000.0;

    fn eq(bands: &[EqBand]) -> Eq {
        Eq {
            bands: Bands::from_slice(bands),
            ..Eq::default()
        }
    }

    fn band(shape: Shape, freq: f32, gain: f32) -> EqBand {
        EqBand::new(shape, freq, gain)
    }

    fn at(b: &EqBand, freq: f64) -> f64 {
        band_response(b, 1.0, freq, RATE)
    }

    fn close(a: f64, b: f64, by: f64) -> bool {
        (a - b).abs() <= by
    }

    #[test]
    fn bells_shelves_and_cuts_have_their_analog_shapes() {
        let bell = band(Shape::Bell, 1_000.0, 6.0);
        assert!(
            close(at(&bell, 1_000.0), 6.0, 0.01),
            "{}",
            at(&bell, 1_000.0)
        );
        assert!(close(at(&bell, 30.0), 0.0, 0.05) && close(at(&bell, 18_000.0), 0.0, 0.1));
        // Near the top a bell keeps its shape: at 15 kHz, its gain where it should be and an
        // octave below the same as an analog bell's (a bilinear one's narrows).
        let top = band(Shape::Bell, 15_000.0, 9.0);
        assert!(close(at(&top, 15_000.0), 9.0, 0.01));
        let w = 2.0 * PI * 15_000.0;
        let a = 10f64.powf(9.0 / 40.0);
        let x = 2.0 * PI * 7_500.0;
        let d = (w * w - x * x).powi(2);
        let analog = 10.0 * ((d + (a * w * x).powi(2)) / (d + (w * x / a).powi(2))).log10();
        assert!(
            close(at(&top, 7_500.0), analog, 0.7),
            "{} {analog}",
            at(&top, 7_500.0)
        );
        // Cuts: -3 dB at the corner, then their slope an octave on.
        for (slope, octave) in [
            (6, -7.0),
            (12, -12.3),
            (24, -24.1),
            (48, -48.2),
            (96, -96.3),
        ] {
            let mut cut = band(Shape::HighCut, 1_000.0, 0.0);
            cut.slope = slope;
            assert!(
                close(at(&cut, 1_000.0), -3.01, 0.05),
                "{slope}: {}",
                at(&cut, 1_000.0)
            );
            assert!(
                close(at(&cut, 2_000.0), octave, 0.6),
                "{slope}: {}",
                at(&cut, 2_000.0)
            );
            assert!(close(at(&cut, 50.0), 0.0, 0.02));
            cut.shape = Shape::LowCut;
            assert!(
                close(at(&cut, 1_000.0), -3.01, 0.05),
                "{slope}: {}",
                at(&cut, 1_000.0)
            );
            assert!(
                close(at(&cut, 500.0), octave, 0.6),
                "{slope}: {}",
                at(&cut, 500.0)
            );
        }
        // A high cut near Nyquist is still -3 dB at its corner (a bilinear one's is pulled down).
        let mut top_cut = band(Shape::HighCut, 18_000.0, 0.0);
        top_cut.slope = 24;
        assert!(
            close(at(&top_cut, 18_000.0), -3.01, 0.2),
            "{}",
            at(&top_cut, 18_000.0)
        );
        // A resonant cut peaks.
        let mut resonant = band(Shape::LowCut, 100.0, 0.0);
        resonant.q = 4.0;
        assert!(at(&resonant, 100.0) > 10.0);
        // Shelves: their gain at the far end, half of it at their frequency.
        for slope in [6, 12, 18, 24, 36, 48] {
            let mut low = band(Shape::LowShelf, 500.0, 6.0);
            low.slope = slope;
            assert!(
                close(at(&low, 15.0), 6.0, 0.25),
                "{slope}: {}",
                at(&low, 15.0)
            );
            assert!(
                close(at(&low, 500.0), 3.0, 0.1),
                "{slope}: {}",
                at(&low, 500.0)
            );
            assert!(close(at(&low, 15_000.0), 0.0, 0.1));
            let mut high = band(Shape::HighShelf, 4_000.0, -6.0);
            high.slope = slope;
            assert!(
                close(at(&high, 4_000.0), -3.0, 0.1),
                "{slope}: {}",
                at(&high, 4_000.0)
            );
            assert!(
                close(at(&high, 23_000.0), -6.0, 0.4),
                "{slope}: {}",
                at(&high, 23_000.0)
            );
            assert!(close(at(&high, 50.0), 0.0, 0.05));
        }
        let tilt = band(Shape::TiltShelf, 1_000.0, 6.0);
        assert!(close(at(&tilt, 20.0), -3.0, 0.1) && close(at(&tilt, 20_000.0), 3.0, 0.15));
        assert!(close(at(&tilt, 1_000.0), 0.0, 0.05));
        let notch = band(Shape::Notch, 1_000.0, 0.0);
        assert!(at(&notch, 1_000.0) < -60.0 && close(at(&notch, 4_000.0), 0.0, 0.2));
        let pass = band(Shape::BandPass, 1_000.0, 0.0);
        assert!(close(at(&pass, 1_000.0), 0.0, 0.01) && at(&pass, 100.0) < -15.0);
        // Flat bands cost nothing.
        assert!(band_sections(&band(Shape::Bell, 1_000.0, 0.0), 1.0, RATE).is_empty());
    }

    /// A sine's level through an EQ, once it's settled, in dB.
    fn through(e: &Eq, freq: f64, side: bool) -> ([f64; 2], [f32; 2]) {
        let mut p = Processor::new(RATE);
        let n = 48_000;
        let sine = |i: usize| (2.0 * PI * freq * i as f64 / RATE).sin() as f32 * 0.25;
        let (mut peak, mut last) = ([0f64; 2], [0f32; 2]);
        let mut i = 0;
        while i < n {
            let frames = MAX_BLOCK.min(n - i);
            let mut l: Vec<f32> = (i..i + frames).map(sine).collect();
            let mut r: Vec<f32> = l.iter().map(|&x| if side { -x } else { x }).collect();
            p.process(e, None, &mut l, &mut r, None);
            if i > n / 2 {
                for k in 0..frames {
                    peak = [
                        peak[0].max(l[k].abs() as f64),
                        peak[1].max(r[k].abs() as f64),
                    ];
                }
            }
            last = [l[frames - 1], r[frames - 1]];
            i += frames;
        }
        (
            [
                20.0 * (peak[0] / 0.25).log10(),
                20.0 * (peak[1] / 0.25).log10(),
            ],
            last,
        )
    }

    #[test]
    fn what_plays_is_what_is_drawn() {
        let mut cut = band(Shape::LowCut, 200.0, 0.0);
        cut.slope = 48;
        let e = eq(&[
            band(Shape::Bell, 1_000.0, 6.0),
            cut,
            band(Shape::HighShelf, 8_000.0, -4.0),
        ]);
        for freq in [80.0, 200.0, 1_000.0, 3_000.0, 12_345.0] {
            let (db, _) = through(&e, freq, false);
            let drawn = eq_response(&e, freq, RATE);
            assert!(close(db[0], drawn, 0.15), "{freq}: {} vs {drawn}", db[0]);
            assert!(close(db[1], drawn, 0.15));
        }
        // The output level and the gain scale.
        let mut louder = e;
        louder.output_db = -6.0;
        louder.scale = 0.5;
        let (db, _) = through(&louder, 1_000.0, false);
        assert!(close(db[0], 3.0 - 6.0, 0.2), "{}", db[0]);
    }

    #[test]
    fn a_band_can_work_on_one_side_or_the_middle_or_the_edges() {
        let mut b = band(Shape::Bell, 1_000.0, -12.0);
        b.placement = Placement::Left;
        let (db, _) = through(&eq(&[b]), 1_000.0, false);
        assert!(
            close(db[0], -12.0, 0.2) && close(db[1], 0.0, 0.05),
            "{db:?}"
        );
        b.placement = Placement::Side;
        // A mono sound has no side: untouched; one all side is cut.
        let (mono, _) = through(&eq(&[b]), 1_000.0, false);
        assert!(
            close(mono[0], 0.0, 0.05) && close(mono[1], 0.0, 0.05),
            "{mono:?}"
        );
        let (side, _) = through(&eq(&[b]), 1_000.0, true);
        assert!(close(side[0], -12.0, 0.2), "{side:?}");
        b.placement = Placement::Mid;
        let (mid, _) = through(&eq(&[b]), 1_000.0, false);
        assert!(close(mid[0], -12.0, 0.2), "{mid:?}");
    }

    #[test]
    fn a_dynamic_band_moves_with_how_loud_it_is() {
        let mut b = band(Shape::Bell, 1_000.0, 0.0);
        b.dynamic = Some(Dynamic {
            range_db: -6.0,
            threshold_db: -40.0,
            ..Dynamic::default()
        });
        // Loud (-12 dBFS, 28 over): down its whole range.
        let (db, _) = through(&eq(&[b]), 1_000.0, false);
        assert!(close(db[0], -6.0, 0.3), "{db:?}");
        // Under the threshold: untouched.
        b.dynamic.as_mut().unwrap().threshold_db = -6.0;
        let (db, _) = through(&eq(&[b]), 1_000.0, false);
        assert!(close(db[0], 0.0, 0.05), "{db:?}");
        // It listens round its frequency: three octaves away, the same sine is 18 dB quieter to
        // it, under a threshold the sine at its frequency is well over.
        b.dynamic.as_mut().unwrap().threshold_db = -24.0;
        let (db, _) = through(&eq(&[b]), 1_000.0, false);
        assert!(close(db[0], -6.0, 0.3), "{db:?}");
        let (db, _) = through(&eq(&[b]), 8_123.0, false);
        assert!(close(db[0], 0.0, 0.05), "{db:?}");
    }

    #[test]
    fn bands_come_and_go_without_a_click() {
        let mut p = Processor::new(RATE);
        let mut e = eq(&[band(Shape::Bell, 1_000.0, 12.0)]);
        let mut phase = 0usize;
        let mut biggest_step = 0f32;
        let mut prev = 0f32;
        for block in 0..200 {
            // Halfway: the band's taken out, then another put in.
            if block == 80 {
                e.bands.remove(0);
            }
            if block == 140 {
                e.bands.push(band(Shape::HighCut, 400.0, 0.0));
            }
            let mut l: Vec<f32> = (0..MAX_BLOCK)
                .map(|i| (2.0 * PI * 1_000.0 * (phase + i) as f64 / RATE).sin() as f32 * 0.1)
                .collect();
            let mut r = l.clone();
            phase += MAX_BLOCK;
            p.process(&e, None, &mut l, &mut r, None);
            if block > 20 {
                for &x in &l {
                    biggest_step = biggest_step.max((x - prev).abs());
                    prev = x;
                }
            } else {
                prev = l[MAX_BLOCK - 1];
            }
        }
        // A 1 kHz sine at 0.4 (12 dB up from 0.1) moves at most 2π·1000/48000·0.4 ≈ 0.052 a
        // sample: a click would jump far more.
        assert!(biggest_step < 0.06, "{biggest_step}");
    }

    #[test]
    fn the_eq_tempo_had_before_opens_as_bands() {
        let old = r#"{"low_cut": 80, "low_mid": {"freq": 300, "gain_db": -2, "q": 1.2},
            "high_mid": {"freq": 3000, "gain_db": 2, "q": 1},
            "high_shelf": {"freq": 9000, "gain_db": 1.5, "q": 0.7071}, "bypass": true}"#;
        let eq: Eq = serde_json::from_str(old).unwrap();
        let got: Vec<_> = eq
            .bands
            .iter()
            .map(|b| (b.shape, b.freq, b.gain_db, b.off))
            .collect();
        assert_eq!(
            got,
            [
                (Shape::LowCut, 80.0, 0.0, false),
                (Shape::Bell, 300.0, -2.0, false),
                (Shape::Bell, 3_000.0, 2.0, false),
                (Shape::HighShelf, 9_000.0, 1.5, false),
            ]
        );
        assert!(eq.bypass && eq.bands[1].q == 1.2);
        // Saved as bands, and read back the same.
        let json = serde_json::to_string(&eq).unwrap();
        assert!(
            !json.contains("low_mid") && json.contains("\"bands\""),
            "{json}"
        );
        assert_eq!(serde_json::from_str::<Eq>(&json).unwrap(), eq);
        // A low cut that was off is kept, off; an empty EQ has no bands.
        let off: Eq =
            serde_json::from_str(r#"{"low_cut": 0, "high_shelf": {"freq": 10000}}"#).unwrap();
        assert!(off.bands[0].off && off.bands.len() == 4);
        assert!(serde_json::from_str::<Eq>("{}").unwrap().bands.is_empty());
        // And it sounds as it did: the old curve was RBJ's bells and shelf, the same as these
        // but for the very top.
        for freq in [100.0, 300.0, 1_000.0, 3_000.0] {
            let db = eq_response(&eq, freq, RATE);
            let rbj = band_response(&eq.bands[1], 1.0, freq, RATE)
                + band_response(&eq.bands[2], 1.0, freq, RATE)
                + band_response(&eq.bands[3], 1.0, freq, RATE)
                + band_response(&eq.bands[0], 1.0, freq, RATE);
            assert!(close(db, rbj, 1e-9));
        }
    }

    #[test]
    fn the_analyzer_finds_a_sine_at_its_level() {
        let sine: Vec<f32> = (0..TAP_LEN)
            .map(|i| (2.0 * PI * 1_234.0 * i as f64 / RATE).sin() as f32 * 0.5)
            .collect();
        let freqs: Vec<f64> = (0..200)
            .map(|i| 20.0 * 1000f64.powf(i as f64 / 199.0))
            .collect();
        let db = spectrum_db(&sine, RATE, &freqs);
        let (loudest, at) = db
            .iter()
            .enumerate()
            .fold((f32::MIN, 0), |m, (i, &d)| if d > m.0 { (d, i) } else { m });
        // A sine between bins reads up to 1.4 dB low through a Hann window.
        assert!(close(loudest as f64, -6.02, 1.5), "{loudest}");
        assert!((freqs[at] / 1_234.0 - 1.0).abs() < 0.05, "{}", freqs[at]);
        assert!(db[10] < -60.0 && db[190] < -60.0);
    }

    #[test]
    fn steep_cuts_stay_steady() {
        let mut low = band(Shape::LowCut, 20.0, 0.0);
        low.slope = 96;
        let mut high = band(Shape::HighCut, 20_000.0, 0.0);
        high.slope = 96;
        high.q = 10.0;
        let e = eq(&[low, high]);
        let mut p = Processor::new(RATE);
        let mut seed = 1u32;
        for _ in 0..400 {
            let mut l: Vec<f32> = (0..MAX_BLOCK)
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (seed >> 8) as f32 / (1 << 24) as f32 - 0.5
                })
                .collect();
            let mut r = l.clone();
            p.process(&e, None, &mut l, &mut r, None);
            assert!(l.iter().chain(&r).all(|x| x.is_finite() && x.abs() < 50.0));
        }
    }

    #[test]
    fn a_band_heard_alone_and_the_analyzer_tapped() {
        let mut e = eq(&[band(Shape::Bell, 4_000.0, 0.0)]);
        e.bands.push(band(Shape::Bell, 200.0, 3.0));
        let id = e.bands[0].id;
        let (low, _) = {
            // A 200 Hz sine, the 4 kHz band soloed: mostly gone.
            let mut p = Processor::new(RATE);
            let tap = Tap::new();
            let mut peak = 0f32;
            for k in 0..40 {
                let mut l: Vec<f32> = (0..MAX_BLOCK)
                    .map(|i| {
                        (2.0 * PI * 200.0 * (k * MAX_BLOCK + i) as f64 / RATE).sin() as f32 * 0.5
                    })
                    .collect();
                let mut r = l.clone();
                p.process(&e, Some((id, 0.0)), &mut l, &mut r, Some(&tap));
                if k > 20 {
                    peak = l.iter().fold(peak, |m, x| m.max(x.abs()));
                }
            }
            let (mut pre, mut post, mut right) =
                ([0f32; TAP_LEN], [0f32; TAP_LEN], [0f32; TAP_LEN]);
            let written = tap.read(&mut pre, &mut post, &mut right);
            assert_eq!(written, 40 * MAX_BLOCK);
            let loudest = |s: &[f32]| s.iter().fold(0f32, |m, x| m.max(x.abs()));
            assert!(close(loudest(&pre) as f64, 0.5, 0.01), "{}", loudest(&pre));
            assert!(loudest(&post) < 0.05);
            (peak, ())
        };
        assert!(low < 0.05, "{low}");
    }
}
