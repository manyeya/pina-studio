//! Natural and linear phase: running the bands at twice the rate, and their latency.

use super::*;

/// Runs samples through sections, one lane's state.
#[inline]
pub(crate) fn cascade(sections: &[Section], state: &mut [[f64; 2]], mut x: f64) -> f64 {
    for (c, st) in sections.iter().zip(state.iter_mut()) {
        let y = c.b0 * x + st[0];
        st[0] = c.b1 * x - c.a1 * y + st[1];
        st[1] = c.b2 * x - c.a2 * y;
        x = y;
    }
    x
}

/// No denormals in what's left ringing.
pub(crate) fn flush(state: &mut [[[f64; 2]; MAX_SECTIONS]; 2]) {
    for st in state.iter_mut().flatten().flatten() {
        if st.abs() < 1e-30 {
            *st = 0.0;
        }
    }
}

/// Both sides through sections at once: two chains the processor can run side by side.
#[inline]
pub(crate) fn cascade2(
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

/// What an EQ in linear phase or with spectral dynamics needs made off the audio thread, when the
/// song's planned: kernels, and the spectral stage's memory.
#[derive(Default)]
pub struct Made {
    pub(crate) linear: Option<Arc<Linear>>,
    pub(crate) spectral: Option<eq_spectral::Plan>,
}

impl Made {
    pub fn new(eq: &Eq, rate: f64) -> Made {
        Made {
            linear: (eq.processing == Processing::Linear).then(|| Arc::new(Linear::new(eq, rate))),
            spectral: eq_spectral::needed(eq).then(|| eq_spectral::Plan::new(eq)),
        }
    }
}

/// How many samples an EQ holds the sound back (none bypassed): natural phase's filters, linear
/// phase's kernels; the spectral stage's window, for spectral dynamics (or any dynamics, in
/// linear phase).
pub fn latency(eq: &Eq) -> u32 {
    if eq.bypass {
        return 0;
    }
    let mode = match eq.processing {
        Processing::ZeroLatency => 0,
        Processing::Natural => NATURAL_LATENCY,
        Processing::Linear => eq_linear::latency(eq),
    };
    mode + eq_spectral::latency(eq)
}

/// Natural phase's filters, up to twice the rate and back: half band, linear phase, so they add
/// their latency and nothing else (flat to 20 kHz at 44.1 kHz, 80 dB down where they cut).
pub(crate) const NATURAL_TAPS: usize = 127;

pub(crate) const NATURAL_LATENCY: u32 = (NATURAL_TAPS as u32 - 1) / 2;

/// The filters' taps that aren't 0 (a half band's every other one is), each its place and
/// value, for a DC gain of 1.
pub(crate) fn halfband() -> Vec<(usize, f32)> {
    let mid = (NATURAL_TAPS / 2) as f64;
    let i0 = |x: f64| {
        let (mut sum, mut term) = (1.0, 1.0);
        for k in 1..40 {
            term *= (x / (2.0 * k as f64)).powi(2);
            sum += term;
        }
        sum
    };
    let beta = 8.0;
    let h: Vec<f64> = (0..NATURAL_TAPS)
        .map(|i| {
            let t = i as f64 - mid;
            let sinc = if t == 0.0 {
                1.0
            } else {
                (PI * t / 2.0).sin() / (PI * t / 2.0)
            };
            sinc * i0(beta * (1.0 - (t / mid).powi(2)).max(0.0).sqrt()) / i0(beta)
        })
        .collect();
    let sum: f64 = h.iter().sum();
    h.iter()
        .enumerate()
        .filter(|(_, v)| v.abs() > 1e-12)
        .map(|(i, v)| (i, (v / sum) as f32))
        .collect()
}

/// A filter's last samples, twice over so the newest `NATURAL_TAPS` sit side by side.
pub(crate) struct Fir {
    pub(crate) hist: Vec<f32>,
    pub(crate) at: usize,
}

impl Fir {
    pub(crate) fn new() -> Fir {
        Fir {
            hist: vec![0.0; 2 * NATURAL_TAPS],
            at: 0,
        }
    }

    pub(crate) fn push(&mut self, x: f32) {
        self.hist[self.at] = x;
        self.hist[self.at + NATURAL_TAPS] = x;
        self.at = (self.at + 1) % NATURAL_TAPS;
    }

    pub(crate) fn out(&self, taps: &[(usize, f32)]) -> f32 {
        let w = &self.hist[self.at..self.at + NATURAL_TAPS];
        taps.iter().map(|&(i, h)| w[i] * h).sum()
    }
}

/// Natural phase: the bands run at twice the rate, where their phase is an analog EQ's up to the
/// top of what's heard rather than bent by the sample rate.
pub(crate) struct Natural {
    pub(crate) inner: Processor,
    pub(crate) taps: Vec<(usize, f32)>,
    pub(crate) up: [Fir; 2],
    pub(crate) down: [Fir; 2],
    /// The block, and the sidechain, at twice the rate.
    pub(crate) twice: [Vec<f32>; 2],
    pub(crate) sc: [Vec<f32>; 2],
}

impl Natural {
    pub(crate) fn new(rate: f64) -> Natural {
        let block = || vec![0.0; 2 * MAX_BLOCK];
        Natural {
            inner: Processor::at(2.0 * rate, false),
            taps: halfband(),
            up: [Fir::new(), Fir::new()],
            down: [Fir::new(), Fir::new()],
            twice: [block(), block()],
            sc: [block(), block()],
        }
    }

    pub(crate) fn clear(&mut self) {
        self.inner.bands = false;
        for f in self.up.iter_mut().chain(&mut self.down) {
            f.hist.fill(0.0);
        }
    }

    pub(crate) fn process(
        &mut self,
        eq: &Eq,
        l: &mut [f32],
        r: &mut [f32],
        sc: Option<[&[f32]; 2]>,
    ) {
        let n = l.len();
        for (c, side) in [&*l, &*r].into_iter().enumerate() {
            let (up, out) = (&mut self.up[c], &mut self.twice[c]);
            for (i, &x) in side.iter().enumerate() {
                // Every other sample 0, the filter's gain doubled for them.
                up.push(x);
                out[2 * i] = 2.0 * up.out(&self.taps);
                up.push(0.0);
                out[2 * i + 1] = 2.0 * up.out(&self.taps);
            }
            if let Some(s) = sc {
                for (i, &x) in s[c][..n].iter().enumerate() {
                    self.sc[c][2 * i] = x;
                    self.sc[c][2 * i + 1] = x;
                }
            }
        }
        let [a, b] = &mut self.twice;
        let sc = sc.map(|_| [&self.sc[0][..2 * n], &self.sc[1][..2 * n]]);
        self.inner
            .run_bands(eq, &mut a[..2 * n], &mut b[..2 * n], sc);
        for (c, side) in [l, r].into_iter().enumerate() {
            let down = &mut self.down[c];
            for (i, y) in side.iter_mut().enumerate() {
                down.push(self.twice[c][2 * i]);
                *y = down.out(&self.taps);
                down.push(self.twice[c][2 * i + 1]);
            }
        }
    }
}
