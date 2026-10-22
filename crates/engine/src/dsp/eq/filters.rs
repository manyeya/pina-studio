//! Each band's filters: matched to the analog shapes, every slope, as sections of biquads.

use super::*;

/// The most second-order sections a band takes: a shelf between its two steepest slopes.
pub const MAX_SECTIONS: usize = 16;

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
pub(crate) fn half_sin2(freq: f64, rate: f64) -> f64 {
    (PI * freq / rate).sin().powi(2)
}

/// |H|² in terms of a polynomial's values at DC and Nyquist and its middle term: for a
/// numerator or denominator, |B(e^jω)|² = B0 φ0 + B1 φ1 + B2 φ2 with φ1 = sin²(ω/2), φ0 = 1 - φ1
/// and φ2 = 4 φ0 φ1.
pub(crate) fn poly2(c: [f64; 3], s: f64) -> f64 {
    let (p0, p1) = (1.0 - s, s);
    c[0] * p0 + c[1] * p1 + c[2] * 4.0 * p0 * p1
}

impl Section {
    /// Its magnitude squared, at sin²(ω/2) = `s`.
    pub(crate) fn mag2(&self, s: f64) -> f64 {
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
    pub(crate) fn times(self, g: f64) -> Section {
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
pub(crate) fn matched2(wp: f64, qp: f64, wm: f64, rate: f64, mag2: &dyn Fn(f64) -> f64) -> Section {
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
pub(crate) fn matched1(wp: f64, rate: f64, mag2: &dyn Fn(f64) -> f64) -> Section {
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
pub(crate) fn resonance(w: f64, q: f64, x: f64) -> f64 {
    (w * w - x * x).powi(2) + (w * x / q).powi(2)
}

pub(crate) fn lowpass2(w: f64, q: f64) -> impl Fn(f64) -> f64 {
    move |x| w.powi(4) / resonance(w, q, x)
}

pub(crate) fn highpass2(w: f64, q: f64) -> impl Fn(f64) -> f64 {
    move |x| x.powi(4) / resonance(w, q, x)
}

pub(crate) fn bandpass2(w: f64, q: f64) -> impl Fn(f64) -> f64 {
    move |x| (w * x / q).powi(2) / resonance(w, q, x)
}

/// A shelf's analog shape (RBJ's), `a` its gain's square root (10^(dB/40)): low or high.
pub(crate) fn shelf2(w: f64, q: f64, a: f64, high: bool) -> impl Fn(f64) -> f64 {
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
pub(crate) fn shelf1(w: f64, g: f64, high: bool) -> impl Fn(f64) -> f64 {
    move |x| match high {
        false => (x * x + w * w * g) / (x * x + w * w / g),
        true => (g * g * x * x + w * w * g) / (x * x + w * w * g),
    }
}

/// Two first-order sections as one second-order.
pub(crate) fn pair(a: Section, b: Section) -> Section {
    Section {
        b0: a.b0 * b.b0,
        b1: a.b0 * b.b1 + a.b1 * b.b0,
        b2: a.b1 * b.b1,
        a1: a.a1 + b.a1,
        a2: a.a1 * b.a1,
    }
}

/// A slope's fraction: `db` dB an octave more, past `w` rad/s, below it (a low cut's side) or
/// `above`. First-order shelves an octave apart (wider if they'd take more than `room`
/// sections), each an octave's worth, from `w` down to 10 Hz or up to the top, in pairs; after
/// `n` in `out`; how many sections there are then.
pub(crate) fn rolloff(
    w: f64,
    db: f64,
    above: bool,
    rate: f64,
    room: usize,
    out: &mut [Section; MAX_SECTIONS],
    mut n: usize,
) -> usize {
    let end = if above {
        0.45 * 2.0 * PI * rate
    } else {
        2.0 * PI * 10.0
    };
    let span = (end / w).log2().abs();
    if db < 0.01 || span < 0.5 || room == 0 {
        return n;
    }
    let count = (span.ceil() as usize).min(2 * room);
    let octaves = span / count as f64;
    let g = 10f64.powf(-db * octaves / 20.0);
    let mut odd: Option<Section> = None;
    for k in 0..count {
        let at = (k as f64 + 0.5) * octaves;
        let wk = if above { w * at.exp2() } else { w / at.exp2() };
        let wp = if above { wk * g.sqrt() } else { wk / g.sqrt() };
        let one = matched1(wp, rate, &shelf1(wk, g, above));
        match odd.take() {
            None => odd = Some(one),
            Some(first) => {
                out[n] = pair(first, one);
                n += 1;
            }
        }
    }
    if let Some(last) = odd {
        out[n] = last;
        n += 1;
    }
    n
}

/// Butterworth's angles for an order `m`: each pole pair's once (the upper), then the real
/// pole's (π) if `m` is odd.
pub(crate) fn butterworth(m: usize) -> impl Iterator<Item = f64> {
    (0..m.div_ceil(2)).map(move |k| PI / 2.0 + (2 * k + 1) as f64 * PI / (2 * m) as f64)
}

/// A root of an analog prototype (normalized, its band ±1) taken to a band round `w0` rad/s,
/// `b` wide: the two roots of s² - r b s + w0² it makes, the higher first.
pub(crate) fn to_band(r: Complex<f64>, b: f64, w0: f64) -> [Complex<f64>; 2] {
    let rb = r * b;
    let d = (rb * rb - 4.0 * w0 * w0).sqrt();
    let (x, y) = ((rb + d) * 0.5, (rb - d) * 0.5);
    if x.norm() >= y.norm() { [x, y] } else { [y, x] }
}

/// What a second-order factor with roots `r` and its conjugate does, |·|² at Ω rad/s: from a
/// root, its frequency and Q; and the factor's |·|².
pub(crate) fn factor(r: Complex<f64>) -> (f64, f64) {
    let w = r.norm();
    (w, w / (-2.0 * r.re).max(1e-9))
}

pub(crate) fn factor2(r: Complex<f64>, x: f64) -> f64 {
    (r.norm_sqr() - x * x).powi(2) + (2.0 * r.re * x).powi(2)
}

/// A prototype of order `m` taken to a band round `w0`, `b` wide, as matched second-order
/// sections after `n`: its poles Butterworth's at radius `rp`; its zeros by `zeros` (at radius
/// `rz` on the same angles; at the band's middle, a notch's; or none but the band pass's s).
/// How many sections there are then.
#[allow(clippy::too_many_arguments)]
pub(crate) fn band_filter(
    m: usize,
    w0: f64,
    b: f64,
    rp: f64,
    zeros: Zeros,
    rate: f64,
    out: &mut [Section; MAX_SECTIONS],
    mut n: usize,
) -> usize {
    for theta in butterworth(m) {
        let unit = Complex::from_polar(1.0, theta);
        let poles = to_band(unit * rp, b, w0);
        let real = (theta - PI).abs() < 1e-9;
        // A real prototype root makes one second-order section; a pair, two.
        let count = if real { 1 } else { 2 };
        let bell_zeros = match zeros {
            Zeros::Radius(rz) => to_band(unit * rz, b, w0),
            _ => poles,
        };
        for (&pole, &zero) in poles.iter().zip(&bell_zeros).take(count) {
            let (wp, qp) = match real {
                true => (w0, w0 / (rp * b)),
                false => factor(pole),
            };
            let num = move |x: f64| match zeros {
                Zeros::Radius(rz) if real => (w0 * w0 - x * x).powi(2) + (rz * b * x).powi(2),
                Zeros::Radius(_) => factor2(zero, x),
                Zeros::Middle => (w0 * w0 - x * x).powi(2),
                Zeros::Origin => (b * x).powi(2),
            };
            let den = move |x: f64| (wp * wp - x * x).powi(2) + (wp * x / qp).powi(2);
            out[n] = matched2(wp, qp, wp, rate, &move |x| num(x) / den(x));
            n += 1;
        }
    }
    n
}

/// Where a band filter's zeros are.
#[derive(Clone, Copy)]
pub(crate) enum Zeros {
    /// On the poles' angles at this radius: a bell.
    Radius(f64),
    /// At the band's middle: a notch.
    Middle,
    /// None but s, one a pole: a band pass.
    Origin,
}

/// A Butterworth bell of order `m` (1 the usual second-order one): `gain_db` at `w`, half of it
/// a band `w / q` wide, its sides steeper the higher the order.
pub(crate) fn bell(
    m: usize,
    w: f64,
    q: f64,
    gain_db: f64,
    rate: f64,
    out: &mut [Section; MAX_SECTIONS],
    n: usize,
) -> usize {
    if gain_db.abs() < 1e-4 {
        return n;
    }
    let g = 10f64.powf(gain_db / 20.0);
    let (rp, rz) = (g.powf(-0.5 / m as f64), g.powf(0.5 / m as f64));
    band_filter(m, w, w / q, rp, Zeros::Radius(rz), rate, out, n)
}

/// A shelf of `order` first-order steps (6 dB an octave each): `gain_db` past `w`, in second-
/// order sections each with the band's Q (RBJ's), and one first-order if the order's odd.
#[allow(clippy::too_many_arguments)]
pub(crate) fn shelf(
    order: usize,
    w: f64,
    q: f64,
    gain_db: f64,
    high: bool,
    rate: f64,
    out: &mut [Section; MAX_SECTIONS],
    mut n: usize,
) -> usize {
    if gain_db.abs() < 1e-4 || order == 0 {
        return n;
    }
    let (pairs, odd) = (order / 2, order % 2 == 1);
    if odd {
        let g = 10f64.powf(gain_db / order as f64 / 20.0);
        let wp = if high { w * g.sqrt() } else { w / g.sqrt() };
        out[n] = matched1(wp, rate, &shelf1(w, g, high));
        n += 1;
    }
    for _ in 0..pairs {
        let a = 10f64.powf(gain_db * 2.0 / order as f64 / 40.0);
        let wp = if high { w * a.sqrt() } else { w / a.sqrt() };
        out[n] = matched2(wp, q, w, rate, &shelf2(w, q, a, high));
        n += 1;
    }
    n
}

/// How many sections a band takes as it's set.
pub(crate) fn count(band: &EqBand, rate: f64) -> usize {
    let mut out = [Section::default(); MAX_SECTIONS];
    sections(band, band.gain_db as f64, rate, &mut out)
}

/// A slope's whole steps and fraction: `slope` in steps of `step` dB an octave.
pub(crate) fn steps(slope: f32, step: f64, most: usize) -> (usize, f64) {
    let x = (slope.max(0.0) as f64 / step).min(most as f64);
    let whole = x.floor();
    (whole as usize, x - whole)
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
    let mut q = (band.q as f64).clamp(0.025, 40.0);
    // Gain-Q interaction: the more it boosts or cuts, the narrower.
    if band.gain_q && band.shape.has_gain() {
        q = (q * 10f64.powf(gain_db.abs() / 60.0)).min(40.0);
    }
    match band.shape {
        Shape::Bell => {
            // Between two orders, the gain shared: the curve between theirs.
            let (m, t) = steps(band.slope, 12.0, 8);
            let m = m.max(1);
            let n = bell(m, w, q, gain_db * (1.0 - t), rate, out, 0);
            bell(m + 1, w, q, gain_db * t, rate, out, n)
        }
        Shape::Notch => {
            let m = (band.slope / 12.0).round().clamp(1.0, 8.0) as usize;
            band_filter(m, w, w / q, 1.0, Zeros::Middle, rate, out, 0)
        }
        Shape::BandPass => {
            // Whole orders a band pass; a fraction, its sides that much steeper beyond its
            // edges (where it's 3 dB down).
            let (m, t) = steps(band.slope, 12.0, 8);
            let n = band_filter(m, w, w / q, 1.0, Zeros::Origin, rate, out, 0);
            let half = 0.5 / q;
            let low = w * ((1.0 + half * half).sqrt() - half);
            let room = (MAX_SECTIONS - n) / 2;
            let n = rolloff(low, 6.0 * t, false, rate, room, out, n);
            let n = rolloff(low + w / q, 6.0 * t, true, rate, room, out, n);
            // All of it at its middle, as a band pass is.
            if n > 0 {
                let middle = half_sin2(freq.min(0.49 * rate), rate);
                let at: f64 = out[..n].iter().map(|c| c.mag2(middle)).product();
                out[0] = out[0].times(1.0 / at.max(1e-12).sqrt());
            }
            n
        }
        Shape::LowCut | Shape::HighCut => {
            let high = band.shape == Shape::LowCut;
            let (order, t) = match band.slope >= Shape::BRICKWALL {
                true => (24, 0.0),
                false => steps(band.slope, 6.0, 16),
            };
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
            // A fractional slope: that much more past the corner.
            rolloff(w, 6.0 * t, !high, rate, MAX_SECTIONS - n, out, n)
        }
        Shape::LowShelf | Shape::HighShelf | Shape::TiltShelf => {
            if gain_db.abs() < 1e-4 {
                return 0;
            }
            let high = band.shape != Shape::LowShelf;
            // Between two orders, the gain shared: the curve between theirs.
            let (order, t) = steps(band.slope, 6.0, 16);
            let order = order.max(1);
            let n = shelf(order, w, q, gain_db * (1.0 - t), high, rate, out, 0);
            let n = shelf(order + 1, w, q, gain_db * t, high, rate, out, n);
            // A tilt is a high shelf, half its gain down: as much down below as up above.
            if band.shape == Shape::TiltShelf {
                out[0] = out[0].times(10f64.powf(-gain_db / 40.0));
            }
            n
        }
        Shape::FlatTilt => {
            if gain_db.abs() < 1e-4 {
                return 0;
            }
            // Ten first-order shelves an octave apart, 20 Hz to 20 kHz, each a tenth of the
            // tilt: a straight line, near enough; then level at its frequency.
            for (k, section) in out.iter_mut().enumerate().take(10) {
                let f = 20.0 * 2f64.powf(k as f64 + 0.5);
                let wk = 2.0 * PI * f.min(0.49 * rate);
                let g = 10f64.powf(gain_db / 10.0 / 20.0);
                *section = matched1(wk * g.sqrt(), rate, &shelf1(wk, g, true));
            }
            let pivot = half_sin2(freq.min(0.49 * rate), rate);
            let at: f64 = out[..10].iter().map(|c| c.mag2(pivot)).product();
            out[0] = out[0].times(1.0 / at.max(1e-12).sqrt());
            10
        }
        Shape::AllPass => {
            // Its poles as a resonance's at the frequency; its zeros their mirror: level flat.
            // Steeper, more of them: more turn of the phase.
            let p = matched2(w, q, w, rate, &|_| 1.0);
            let m = (band.slope / 12.0).round().clamp(1.0, 8.0) as usize;
            for section in out.iter_mut().take(m) {
                *section = Section {
                    b0: p.a2,
                    b1: p.a1,
                    b2: 1.0,
                    a1: p.a1,
                    a2: p.a2,
                };
            }
            m
        }
    }
}

/// A band's sections, for drawing: none if it's off.
pub fn band_sections(band: &EqBand, scale: f32, rate: f64) -> Vec<Section> {
    let mut out = [Section::default(); MAX_SECTIONS];
    let n = sections(band, band.gain(scale) as f64, rate, &mut out);
    out[..n].to_vec()
}
