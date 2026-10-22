//! Pina Studio's EQ: up to 24 bands, each a bell, shelf, cut, notch, band pass or tilt, over the whole
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

mod dynamics;
mod filters;
mod phase;
mod response;
mod tap;

pub use dynamics::*;
pub use filters::*;
pub use phase::*;
pub use response::*;
pub use tap::*;

use std::f64::consts::{FRAC_1_SQRT_2, PI};

use rustfft::num_complex::Complex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
};

use crate::{
    audio::engine::{MASTER, MAX_BLOCK},
    dsp::eq_linear::{self, Linear},
    dsp::eq_spectral,
    song::project::{Character, Eq, EqBand, MAX_BANDS, Placement, Processing, Shape},
};

/// Samples between a moving band's filter updates.
const STEP: usize = 16;
/// Seconds a band takes to glide to new settings, and to fade in or out.
const GLIDE: f64 = 0.02;
const FADE: f64 = 0.03;

/// An EQ, playing: its bands' filters and memory, which carry on through edits.
pub struct Processor {
    rate: f64,
    live: [Live; LIVE],
    /// The bands' places in `live`, in the order they run: the EQ's, then those fading out.
    order: [u8; LIVE],
    /// Whether it ran last block, and whether its bands did: one coming back on starts with its
    /// bands as they are.
    running: bool,
    bands: bool,
    /// The output's gain and pan, gliding; the make-up auto gain works out (dB) and what for.
    output: f64,
    pan: f64,
    auto: (f64, Option<Eq>),
    /// A band heard alone: its id, the filter it's heard through and that filter's memory.
    solo: (u16, Section, [[f64; 2]; 2]),
    /// The warm character's DC blocker, each side.
    dc: [[f64; 2]; 2],
    /// A copy of the block going in, for the solo and the analyzer.
    input: [[f32; MAX_BLOCK]; 2],
    /// How it treats phase last block; natural phase's bands at twice the rate (none for those
    /// bands themselves); linear phase's convolvers; the spectral dynamics.
    mode: Processing,
    natural: Option<Box<Natural>>,
    linear: eq_linear::Runner,
    spectral: eq_spectral::Runner,
}

/// The make-up gain an EQ wants (dB): as much as it makes the sound louder on average across
/// what's heard, as pink noise would hear it (each octave alike), the other way.
pub fn auto_gain_db(eq: &Eq, rate: f64) -> f64 {
    let n = 48;
    let power: f64 = (0..n)
        .map(|i| {
            let f = 20.0 * 1000f64.powf(i as f64 / (n - 1) as f64);
            let db = eq
                .bands
                .iter()
                .filter(|b| !b.off && b.shape != Shape::AllPass)
                .map(|b| band_response(b, eq.scale, f, rate))
                .sum::<f64>();
            10f64.powf(db / 10.0)
        })
        .sum::<f64>()
        / n as f64;
    -10.0 * power.max(1e-12).log10()
}

impl Processor {
    pub fn new(rate: f64) -> Self {
        Processor::at(rate, true)
    }

    fn at(rate: f64, natural: bool) -> Self {
        Processor {
            rate,
            live: [Live::FREE; LIVE],
            order: [0; LIVE],
            running: false,
            bands: false,
            output: 1.0,
            pan: 0.0,
            auto: (0.0, None),
            solo: (0, Section::default(), [[0.0; 2]; 2]),
            dc: [[0.0; 2]; 2],
            input: [[0.0; MAX_BLOCK]; 2],
            mode: Processing::ZeroLatency,
            natural: natural.then(|| Box::new(Natural::new(rate))),
            linear: eq_linear::Runner::default(),
            spectral: eq_spectral::Runner::default(),
        }
    }

    /// The EQ didn't run this block (it's off, or not there): next time it starts afresh.
    pub fn rest(&mut self) {
        self.running = false;
    }

    /// The output gain it's going to: its level, and its make-up if it makes its own.
    fn output_target(&mut self, eq: &Eq) -> f64 {
        let mut db = eq.output_db as f64;
        if eq.auto_gain {
            // Worked out again only when the EQ's changed.
            let same = self
                .auto
                .1
                .as_ref()
                .is_some_and(|e| e.bands == eq.bands && e.scale == eq.scale);
            if !same {
                self.auto = (auto_gain_db(eq, self.rate), Some(*eq));
            }
            db += self.auto.0;
        }
        10f64.powf(db / 20.0)
    }

    /// The bands' places, matched to the EQ's by id: new ones take a free place, those gone fade.
    fn reconcile(&mut self, eq: &Eq) -> usize {
        let fresh = !self.bands;
        if fresh {
            self.live = [Live::FREE; LIVE];
        }
        for l in self.live.iter_mut().filter(|l| l.id != 0) {
            l.gone = l.ghost || !eq.bands.iter().any(|b| b.id == l.id);
        }
        let mut n = 0;
        for band in eq.bands.iter() {
            let found = self
                .live
                .iter()
                .position(|l| l.id == band.id && l.id != 0 && !l.ghost);
            let at = match found {
                Some(i) => {
                    let was = self.live[i].band;
                    if was.slope != band.slope
                        && self.live[i].mix > 0.0
                        && count(&was, self.rate) != count(band, self.rate)
                    {
                        self.fade_over(i);
                    }
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
        self.bands = true;
        n
    }

    /// A band's filters made another way (its slope takes more sections or fewer): what it had
    /// fades out as the new ones fade in, so the change doesn't click.
    fn fade_over(&mut self, i: usize) {
        if let Some(j) = self.live.iter().position(|l| l.id == 0) {
            self.live[j] = Live {
                ghost: true,
                gone: true,
                ..self.live[i]
            };
        }
        let live = &mut self.live[i];
        live.state = [[[0.0; 2]; MAX_SECTIONS]; 2];
        live.mix = 0.0;
        live.stale = true;
    }

    /// Runs a block (at most `MAX_BLOCK`) through the EQ in place. `made` is what its linear
    /// phase and spectral dynamics need (without it, they're left out); `solo` a band to hear
    /// alone, by id, and its listening level; `sidechain` what its dynamic bands can listen to
    /// instead; `tap` gets what went in and came out, and the dynamic bands' moves and levels.
    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        eq: &Eq,
        made: Option<&Made>,
        solo: Option<(u16, f32)>,
        l: &mut [f32],
        r: &mut [f32],
        sidechain: Option<[&[f32]; 2]>,
        tap: Option<&Tap>,
    ) {
        let frames = l.len().min(r.len()).min(MAX_BLOCK);
        let (l, r) = (&mut l[..frames], &mut r[..frames]);
        let keep = solo.is_some() || tap.is_some();
        if keep {
            self.input[0][..frames].copy_from_slice(l);
            self.input[1][..frames].copy_from_slice(r);
        }
        let sc = sidechain.filter(|s| s[0].len() >= frames && s[1].len() >= frames);
        // Back on, or treating phase another way: everything starts afresh.
        if !self.running || self.mode != eq.processing {
            if !self.running {
                self.output = self.output_target(eq);
                self.pan = eq.pan as f64;
                self.spectral.rest();
            }
            self.dc = [[0.0; 2]; 2];
            self.mode = eq.processing;
            self.bands = false;
            self.linear.rest();
            if let Some(n) = self.natural.as_mut() {
                n.clear();
            }
            self.running = true;
        }
        // Spectral dynamics first, hearing what comes in.
        match made.and_then(|m| m.spectral.as_ref()) {
            Some(plan) if eq_spectral::needed(eq) => {
                self.spectral.process(plan, eq, self.rate, l, r, sc)
            }
            _ => self.spectral.rest(),
        }
        match eq.processing {
            Processing::ZeroLatency => self.run_bands(eq, l, r, sc),
            Processing::Natural => {
                if let Some(n) = self.natural.as_mut() {
                    n.process(eq, l, r, sc);
                }
            }
            Processing::Linear => {
                if let Some(linear) = made.and_then(|m| m.linear.as_ref()) {
                    self.linear.process(linear, eq, l, r);
                }
            }
        }
        self.run_output(eq, l, r);
        // The warm character leans its wave: no DC from it.
        if eq.character == Character::Warm && eq.processing != Processing::Linear {
            let k = 1.0 - (-2.0 * PI * 5.0 / self.rate).exp();
            for (lane, side) in [&mut *l, &mut *r].into_iter().enumerate() {
                let [avg, _] = &mut self.dc[lane];
                for x in side.iter_mut() {
                    *avg += (*x as f64 - *avg) * k;
                    *x = (*x as f64 - *avg) as f32;
                }
            }
        }
        self.hear_alone(eq, solo, l, r);
        if let Some(tap) = tap {
            tap.write(&self.input[0][..frames], &self.input[1][..frames], l, r);
            let bands = match (eq.processing, &self.natural) {
                (Processing::Natural, Some(n)) => &n.inner.live,
                _ => &self.live,
            };
            let scale = eq.scale as f64;
            for (i, band) in eq.bands.iter().enumerate() {
                let (moved, level) = match self.spectral.moved(band.id) {
                    Some((m, level)) => (m as f64, level as f64),
                    None => {
                        let live = bands.iter().find(|x| x.id == band.id && !x.ghost);
                        (
                            live.map_or(0.0, |x| x.moved * scale),
                            live.map_or(-120.0, |x| 20.0 * x.env.max(1e-6).log10()),
                        )
                    }
                };
                tap.moved[i].store((moved as f32).to_bits(), Ordering::Relaxed);
                tap.level[i].store((level as f32).to_bits(), Ordering::Relaxed);
            }
        }
    }

    /// The bands' filters on a block, in place.
    fn run_bands(&mut self, eq: &Eq, l: &mut [f32], r: &mut [f32], sc: Option<[&[f32]; 2]>) {
        let frames = l.len();
        let count = self.reconcile(eq);
        let rate = self.rate;
        let glide = 1.0 - (-(STEP as f64) / (GLIDE * rate)).exp();
        let fade = STEP as f64 / (FADE * rate);
        let scale = eq.scale as f64;
        let character = eq.character;
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
                        live.ghost = false;
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
                // A dynamic band's move, from what it's heard: over its threshold (its own, a
                // little over what it usually hears), through a soft knee, by 4:1, up to its
                // range.
                let dynamic = live.band.dynamic.filter(|d| {
                    live.band.shape.can_be_dynamic()
                        && !d.bypass
                        && d.range_db != 0.0
                        && !eq_spectral::takes(&live.band, eq)
                });
                if let Some(d) = dynamic {
                    // What it usually hears: up to a louder sound in under half a second,
                    // down from one over two, so what stands out is a jump or a peak.
                    let level = 20.0 * live.env.max(1e-9).log10();
                    let tau = if level > live.usual { 0.4 } else { 2.0 };
                    let k = 1.0 - (-(len as f64) / (tau * rate)).exp();
                    live.usual += (level - live.usual) * k;
                    let threshold = d.threshold_db.map_or(live.usual + 3.0, f64::from);
                    let over = level - threshold;
                    const KNEE: f64 = 6.0;
                    let over = match over {
                        o if o <= -KNEE / 2.0 => 0.0,
                        o if o >= KNEE / 2.0 => o,
                        o => (o + KNEE / 2.0).powi(2) / (2.0 * KNEE),
                    };
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
                    let n = sections(&now, gain, rate, &mut live.sections);
                    if n != live.n {
                        live.state = [[[0.0; 2]; MAX_SECTIONS]; 2];
                    }
                    live.n = n;
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
                    Some(d) => {
                        let (a, z) = auto_times(live.band.freq);
                        let coef = |ms: f64| 1.0 - (-1.0 / (ms.max(0.05) * 1e-3 * rate)).exp();
                        (
                            coef(d.attack_ms.map_or(a, f64::from)),
                            coef(d.release_ms.map_or(z, f64::from)),
                        )
                    }
                    None => (0.0, 0.0),
                };
                let coloured = character != Character::Clean && live.band.shape.has_gain();
                let sections = &live.sections[..live.n];
                let plain = !ms && dynamic.is_none() && !coloured && m0 == 1.0 && m1 == 1.0;
                // Most bands: on both sides or one, still, clean, all in. Each side a tight run.
                if plain && lanes == [true, true] {
                    let [sl, sr] = &mut live.state;
                    for i in start..end {
                        let (a, b) = cascade2(sections, sl, sr, l[i] as f64, r[i] as f64);
                        (l[i], r[i]) = (a as f32, b as f32);
                    }
                    flush(&mut live.state);
                    continue;
                }
                if plain {
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
                let external = dynamic.is_some_and(|d| d.external) && sc.is_some();
                for i in start..end {
                    let split = |a: f64, b: f64| match ms {
                        true => ((a + b) * 0.5, (a - b) * 0.5),
                        false => (a, b),
                    };
                    let (x0, x1) = split(l[i] as f64, r[i] as f64);
                    let x = [x0, x1];
                    let heard = match (external, sc) {
                        (true, Some([a, b])) => {
                            let (h0, h1) = split(a[i] as f64, b[i] as f64);
                            [h0, h1]
                        }
                        _ => x,
                    };
                    let mut y = x;
                    let mut level = 0f64;
                    for lane in 0..2 {
                        if !lanes[lane] {
                            continue;
                        }
                        if dynamic.is_some() {
                            let d = cascade(
                                &live.det[..live.det_n],
                                &mut live.det_state[lane],
                                heard[lane],
                            );
                            level = level.max(d.abs());
                        }
                        y[lane] = cascade(sections, &mut live.state[lane], x[lane]);
                        if coloured {
                            y[lane] = x[lane] + colour(y[lane] - x[lane], character);
                        }
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
                for st in live.det_state.iter_mut().flatten().flatten() {
                    if st.abs() < 1e-30 {
                        *st = 0.0;
                    }
                }
            }
            start = end;
        }
    }

    /// The output's level and pan on a block, gliding; upside down if asked.
    fn run_output(&mut self, eq: &Eq, l: &mut [f32], r: &mut [f32]) {
        let frames = l.len();
        let glide = 1.0 - (-(STEP as f64) / (GLIDE * self.rate)).exp();
        let mut start = 0;
        while start < frames {
            let end = (start + STEP).min(frames);
            let len = end - start;
            let to = self.output_target(eq);
            let from = self.output;
            let next = from + (to - from) * glide;
            let next = if (next - to).abs() < 1e-6 { to } else { next };
            let (p0, pan_to) = (self.pan, eq.pan as f64);
            let p1 = match (pan_to - p0).abs() < 1e-4 {
                true => pan_to,
                false => p0 + (pan_to - p0) * glide,
            };
            let sign = if eq.invert { -1.0 } else { 1.0 };
            if from != 1.0 || next != 1.0 || p0 != 0.0 || p1 != 0.0 || eq.invert {
                for i in start..end {
                    let t = (i - start + 1) as f64 / len as f64;
                    let g = (from + (next - from) * t) * sign;
                    let p = p0 + (p1 - p0) * t;
                    let (a, b) = (l[i] as f64 * g, r[i] as f64 * g);
                    // Balance: the side it's turned from, down; or mid against side.
                    let (ga, gb) = ((1.0 - p).min(1.0), (1.0 + p).min(1.0));
                    let (a, b) = match eq.pan_ms {
                        false => (a * ga, b * gb),
                        true => {
                            let (m, s) = ((a + b) * 0.5 * ga, (a - b) * 0.5 * gb);
                            (m + s, m - s)
                        }
                    };
                    l[i] = a as f32;
                    r[i] = b as f32;
                }
            }
            self.output = next;
            self.pan = p1;
            start = end;
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
            Shape::HighCut | Shape::HighShelf | Shape::TiltShelf | Shape::FlatTilt => {
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

#[cfg(test)]
mod tests;
