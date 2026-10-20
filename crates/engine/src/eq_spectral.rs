//! The EQ's spectral stage: dynamic bands worked frequency by frequency, in short overlapping
//! windows (Hann, a quarter apart; overlap-add), so what they do is linear phase. A spectral band
//! turns down (or up) only what stands out in it, each frequency on its own level and threshold
//! (its density how wide a stretch moves together; its tilt hearing the highs louder, as a mix's
//! pink spectrum needs); in linear phase, every dynamic band moves here, the whole band by what it
//! hears across itself. It holds the sound back a window's length: its latency.
//!
//! Its memory is made off the audio thread, when the song's planned (`Plan::new`).

use std::sync::{Arc, Mutex};

use rustfft::{Fft, FftPlanner, num_complex::Complex};

use crate::{
    eq::{MAX_SECTIONS, Section, auto_times, response_db, sections},
    project::{Eq, EqBand, MAX_BANDS, Placement, Processing, Shape},
};

/// The window, by resolution.
pub const SIZES: [usize; 4] = [1024, 2048, 4096, 4096];

/// A dynamic band's gain moves this much for each dB over its threshold (4:1), up to its range.
const RATIO: f32 = 0.75;
const KNEE: f32 = 6.0;

/// Whether a band moves in the spectral stage: a spectral one; in linear phase, any dynamic one.
pub fn takes(b: &EqBand, eq: &Eq) -> bool {
    !b.off
        && b.shape.can_be_dynamic()
        && b.dynamic.is_some_and(|d| {
            !d.bypass && d.range_db != 0.0 && (d.spectral || eq.processing == Processing::Linear)
        })
}

/// Whether an EQ needs the spectral stage.
pub fn needed(eq: &Eq) -> bool {
    eq.bands.iter().any(|b| takes(b, eq))
}

/// The latency the stage adds: a window.
pub fn latency(eq: &Eq) -> u32 {
    match needed(eq) {
        true => SIZES[eq.resolution.min(3) as usize] as u32,
        false => 0,
    }
}

/// The stage's memory, made off the audio thread for the EQ to take.
pub struct Plan {
    n: usize,
    spare: Mutex<Option<Box<State>>>,
}

impl Plan {
    pub fn new(eq: &Eq) -> Plan {
        let n = SIZES[eq.resolution.min(3) as usize];
        Plan {
            n,
            spare: Mutex::new(Some(Box::new(State::new(n)))),
        }
    }
}

/// What a band has heard: at each frequency its level (dB) and what it usually is; across it all
/// the same; and how far it's moved (dB, its most). With its weights, and the shape they're for.
struct Heard {
    id: u16,
    weight: Vec<f32>,
    shaped: Option<(Shape, f32, f32, f32, bool)>,
    env: Vec<f32>,
    usual: Vec<f32>,
    wide: (f32, f32),
    moved: f32,
}

pub struct State {
    n: usize,
    hop: usize,
    window: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    scratch: Vec<Complex<f32>>,
    /// Each side: the last window in, the samples of the hop coming, those going out, and what's
    /// being added up.
    input: [Vec<f32>; 2],
    queue: [Vec<f32>; 2],
    out: [Vec<f32>; 2],
    sum: [Vec<f32>; 2],
    fill: usize,
    spec: [Vec<Complex<f32>>; 2],
    /// The EQ's sidechain alike, for bands that listen to it: the last window, the hop coming,
    /// its spectrum.
    sc_input: [Vec<f32>; 2],
    sc_queue: [Vec<f32>; 2],
    sc_spec: [Vec<Complex<f32>>; 2],
    heard: Vec<Heard>,
    /// Scratch, a value a frequency: levels; the gains (dB) on left, right, mid, side.
    level: Vec<f32>,
    smooth: Vec<f32>,
    sums: Vec<f64>,
    gains: [Vec<f32>; 4],
}

impl State {
    fn new(n: usize) -> State {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(n);
        let ifft = planner.plan_fft_inverse(n);
        let scratch = vec![
            Complex::new(0.0, 0.0);
            fft.get_inplace_scratch_len()
                .max(ifft.get_inplace_scratch_len())
        ];
        let hop = n / 4;
        let bins = n / 2 + 1;
        let zeros = |len: usize| vec![0f32; len];
        State {
            n,
            hop,
            window: (0..n)
                .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos())
                .collect(),
            fft,
            ifft,
            scratch,
            input: [zeros(n), zeros(n)],
            queue: [zeros(hop), zeros(hop)],
            out: [zeros(hop), zeros(hop)],
            sum: [zeros(n), zeros(n)],
            fill: 0,
            spec: [
                vec![Complex::new(0.0, 0.0); n],
                vec![Complex::new(0.0, 0.0); n],
            ],
            sc_input: [zeros(n), zeros(n)],
            sc_queue: [zeros(hop), zeros(hop)],
            sc_spec: [
                vec![Complex::new(0.0, 0.0); n],
                vec![Complex::new(0.0, 0.0); n],
            ],
            heard: (0..MAX_BANDS)
                .map(|_| Heard {
                    id: 0,
                    weight: zeros(bins),
                    shaped: None,
                    env: zeros(bins),
                    usual: vec![-120.0; bins],
                    wide: (0.0, -120.0),
                    moved: 0.0,
                })
                .collect(),
            level: zeros(bins),
            smooth: zeros(bins),
            sums: vec![0.0; bins + 1],
            gains: [zeros(bins), zeros(bins), zeros(bins), zeros(bins)],
        }
    }
}

/// The stage running.
#[derive(Default)]
pub struct Runner {
    state: Option<Box<State>>,
}

impl Runner {
    /// The stage not needed: what it held goes.
    pub fn rest(&mut self) {
        if let Some(s) = self.state.as_mut() {
            let all = s
                .input
                .iter_mut()
                .chain(&mut s.sum)
                .chain(&mut s.queue)
                .chain(&mut s.out);
            for v in all.chain(&mut s.sc_input).chain(&mut s.sc_queue) {
                v.fill(0.0);
            }
            s.fill = 0;
            for h in &mut s.heard {
                h.id = 0;
            }
        }
    }

    /// How far a band's moved now and what it hears across itself (dB), if this stage moves it.
    pub fn moved(&self, id: u16) -> Option<(f32, f32)> {
        let s = self.state.as_ref()?;
        let h = s.heard.iter().find(|h| h.id == id && id != 0)?;
        Some((h.moved, h.wide.0))
    }

    /// A block through the stage, in place (a window late); `sidechain` what bands listening to
    /// the EQ's sidechain hear. Allocation-free.
    pub fn process(
        &mut self,
        plan: &Plan,
        eq: &Eq,
        rate: f64,
        l: &mut [f32],
        r: &mut [f32],
        sidechain: Option<[&[f32]; 2]>,
    ) {
        if !self.state.as_deref().is_some_and(|s| s.n == plan.n) {
            let Some(state) = plan.spare.try_lock().ok().and_then(|mut s| s.take()) else {
                return;
            };
            self.state = Some(state);
        }
        let s = self.state.as_mut().expect("taken above");
        for i in 0..l.len() {
            let f = s.fill;
            s.queue[0][f] = l[i];
            s.queue[1][f] = r[i];
            let sc = sidechain.and_then(|[a, b]| Some((*a.get(i)?, *b.get(i)?)));
            (s.sc_queue[0][f], s.sc_queue[1][f]) = sc.unwrap_or((0.0, 0.0));
            l[i] = s.out[0][f];
            r[i] = s.out[1][f];
            s.fill += 1;
            if s.fill == s.hop {
                s.fill = 0;
                frame(s, eq, rate);
            }
        }
    }
}

/// A band's weight at each frequency, 0 to 1: how much of its move a frequency gets (its shape
/// as a 12 dB boost has it, over 12).
fn weights(b: &EqBand, rate: f64, n: usize, out: &mut [f32]) {
    let shape = EqBand {
        gain_db: 12.0,
        dynamic: None,
        ..*b
    };
    let mut all = [Section::default(); MAX_SECTIONS];
    let count = sections(&shape, 12.0, rate, &mut all);
    for (k, w) in out.iter_mut().enumerate() {
        let f = (k as f64 * rate / n as f64).max(1.0);
        *w = (response_db(&all[..count], f, rate) as f32 / 12.0).clamp(0.0, 1.0);
    }
}

/// How far a level over its threshold moves a band: through a soft knee, by 4:1, up to its
/// range (dB).
fn move_for(over: f32, range: f32) -> f32 {
    let over = match over {
        o if o <= -KNEE / 2.0 => 0.0,
        o if o >= KNEE / 2.0 => o,
        o => (o + KNEE / 2.0).powi(2) / (2.0 * KNEE),
    };
    (over * RATIO).clamp(0.0, range.abs()) * range.signum()
}

/// A window's worth: in, each band's move worked out and laid on the frequencies, out, added.
fn frame(s: &mut State, eq: &Eq, rate: f64) {
    let (n, hop) = (s.n, s.hop);
    let bins = n / 2 + 1;
    for c in 0..2 {
        s.input[c].copy_within(hop.., 0);
        s.input[c][n - hop..].copy_from_slice(&s.queue[c]);
        for (x, (&v, &w)) in s.spec[c].iter_mut().zip(s.input[c].iter().zip(&s.window)) {
            *x = Complex::new(v * w, 0.0);
        }
        s.fft.process_with_scratch(&mut s.spec[c], &mut s.scratch);
    }
    let external = eq
        .bands
        .iter()
        .any(|b| takes(b, eq) && b.dynamic.is_some_and(|d| d.external));
    for c in 0..2 {
        s.sc_input[c].copy_within(hop.., 0);
        s.sc_input[c][n - hop..].copy_from_slice(&s.sc_queue[c]);
        if external {
            for (x, (&v, &w)) in s.sc_spec[c]
                .iter_mut()
                .zip(s.sc_input[c].iter().zip(&s.window))
            {
                *x = Complex::new(v * w, 0.0);
            }
            s.fft
                .process_with_scratch(&mut s.sc_spec[c], &mut s.scratch);
        }
    }
    for g in &mut s.gains {
        g.fill(0.0);
    }
    // A full-scale sine reads 0 dB.
    let norm = 4.0 / n as f32;
    let frame_s = hop as f64 / rate;
    // Bands gone, forgotten; bands come, a place each.
    for h in &mut s.heard {
        if h.id != 0 && !eq.bands.iter().any(|b| b.id == h.id && takes(b, eq)) {
            h.id = 0;
        }
    }
    for b in eq.bands.iter().filter(|b| takes(b, eq)) {
        let d = b.dynamic.expect("takes it");
        let slot = match s.heard.iter().position(|h| h.id == b.id) {
            Some(i) => i,
            None => {
                let Some(i) = s.heard.iter().position(|h| h.id == 0) else {
                    continue;
                };
                let h = &mut s.heard[i];
                h.id = b.id;
                h.env.fill(0.0);
                h.usual.fill(-120.0);
                h.wide = (0.0, -120.0);
                i
            }
        };
        let h = &mut s.heard[slot];
        let shaped = Some((b.shape, b.freq, b.q, b.slope, b.gain_q));
        if h.shaped != shaped {
            h.shaped = shaped;
            weights(b, rate, n, &mut h.weight);
        }
        // What it hears: the side of the sound it works on (or of the sidechain).
        let heard = if d.external { &s.sc_spec } else { &s.spec };
        let pairs = heard[0].iter().zip(&heard[1]);
        for (level, (&a, &c)) in s.level.iter_mut().zip(pairs) {
            let m = match b.placement {
                Placement::Stereo => a.norm().max(c.norm()),
                Placement::Left => a.norm(),
                Placement::Right => c.norm(),
                Placement::Mid => ((a + c) * 0.5).norm(),
                Placement::Side => ((a - c) * 0.5).norm(),
            };
            *level = m * norm;
        }
        let (auto_a, auto_r) = auto_times(b.freq);
        let coef = |ms: f64| 1.0 - (-frame_s / (ms.max(0.05) * 1e-3)).exp() as f32;
        let (attack, release) = (
            coef(d.attack_ms.map_or(auto_a, f64::from).max(frame_s * 1e3)),
            coef(d.release_ms.map_or(auto_r, f64::from).max(frame_s * 1e3)),
        );
        let follow = |env: &mut f32, level: f32| {
            let k = if level > *env { attack } else { release };
            *env += (level - *env) * k;
        };
        let usual = |u: &mut f32, level: f32| {
            let tau = if level > *u { 0.4 } else { 2.0 };
            *u += (level - *u) * (1.0 - (-frame_s / tau).exp()) as f32;
        };
        let lane = match b.placement {
            Placement::Stereo => [0usize, 1],
            Placement::Left => [0, 0],
            Placement::Right => [1, 1],
            Placement::Mid => [2, 2],
            Placement::Side => [3, 3],
        };
        if d.spectral {
            // Each frequency on its own: its level (smoothed over a stretch as wide as the
            // density leaves it, tilted if asked), followed, against its threshold.
            // The power so far at each frequency: any stretch's sum is two of them.
            s.sums[0] = 0.0;
            for k in 0..bins {
                s.sums[k + 1] = s.sums[k] + (s.level[k] as f64).powi(2);
            }
            for k in 0..bins {
                let reach = ((1.0 - d.density.clamp(0.0, 1.0)) * k as f32 * 0.12) as usize;
                let (lo, hi) = (k.saturating_sub(reach), (k + reach).min(bins - 1));
                let power = ((s.sums[hi + 1] - s.sums[lo]) / (hi - lo + 1) as f64) as f32;
                let f = (k as f64 * rate / n as f64).max(20.0);
                let tilt = if d.tilt {
                    3.0 * (f / 1_000.0).log2() as f32
                } else {
                    0.0
                };
                s.smooth[k] = 10.0 * power.max(1e-14).log10() + tilt;
            }
            let (mut power, mut weight) = (0f32, 0f32);
            for k in 0..bins {
                power += h.weight[k] * s.level[k] * s.level[k];
                weight += h.weight[k];
            }
            follow(
                &mut h.wide.0,
                10.0 * (power / weight.max(1e-6)).max(1e-14).log10() + 3.0,
            );
            let mut most = 0f32;
            for k in 0..bins {
                if h.weight[k] < 0.01 {
                    continue;
                }
                let level = s.smooth[k];
                follow(&mut h.env[k], level);
                usual(&mut h.usual[k], level);
                let threshold = d.threshold_db.unwrap_or(h.usual[k] + 3.0);
                let g = move_for(h.env[k] - threshold, d.range_db) * eq.scale * h.weight[k];
                for &ln in &lane[..if lane[0] == lane[1] { 1 } else { 2 }] {
                    s.gains[ln][k] += g;
                }
                if g.abs() > most.abs() {
                    most = g;
                }
            }
            h.moved = most;
        } else {
            // The whole band by what it hears across itself.
            let (mut power, mut weight) = (0f32, 0f32);
            for k in 0..bins {
                power += h.weight[k] * s.level[k] * s.level[k];
                weight += h.weight[k];
            }
            let level = 10.0 * (power / weight.max(1e-6)).max(1e-14).log10() + 3.0;
            follow(&mut h.wide.0, level);
            usual(&mut h.wide.1, level);
            let threshold = d.threshold_db.unwrap_or(h.wide.1 + 3.0);
            let g = move_for(h.wide.0 - threshold, d.range_db) * eq.scale;
            for k in 0..bins {
                for &ln in &lane[..if lane[0] == lane[1] { 1 } else { 2 }] {
                    s.gains[ln][k] += g * h.weight[k];
                }
            }
            h.moved = g;
        }
    }
    // The gains on the sound: left and right, then mid and side; each frequency's mirror alike.
    let gain = |db: f32| 10f32.powf(db / 20.0);
    let ms = s.gains[2].iter().chain(&s.gains[3]).any(|&g| g != 0.0);
    for k in 0..bins {
        let (mut a, mut c) = (
            s.spec[0][k] * gain(s.gains[0][k]),
            s.spec[1][k] * gain(s.gains[1][k]),
        );
        if ms {
            let (m, d) = (
                (a + c) * 0.5 * gain(s.gains[2][k]),
                (a - c) * 0.5 * gain(s.gains[3][k]),
            );
            (a, c) = (m + d, m - d);
        }
        s.spec[0][k] = a;
        s.spec[1][k] = c;
        if k > 0 && k < n / 2 {
            s.spec[0][n - k] = a.conj();
            s.spec[1][n - k] = c.conj();
        }
    }
    // Back, windowed again, added in: Hann twice a quarter apart sums to 1.5.
    let scale = 1.0 / (n as f32 * 1.5);
    for c in 0..2 {
        s.ifft.process_with_scratch(&mut s.spec[c], &mut s.scratch);
        s.sum[c].copy_within(hop.., 0);
        s.sum[c][n - hop..].fill(0.0);
        for (j, (acc, x)) in s.sum[c].iter_mut().zip(&s.spec[c]).enumerate() {
            *acc += x.re * s.window[j] * scale;
        }
        s.out[c].copy_from_slice(&s.sum[c][..hop]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Bands, Dynamic, Shape};
    use std::f64::consts::PI;

    const RATE: f64 = 48_000.0;

    fn run(e: &Eq, l: &[f32]) -> Vec<f32> {
        let plan = Plan::new(e);
        let mut stage = Runner::default();
        let (mut a, mut b) = (l.to_vec(), l.to_vec());
        for at in (0..l.len()).step_by(512) {
            let end = (at + 512).min(l.len());
            stage.process(&plan, e, RATE, &mut a[at..end], &mut b[at..end], None);
        }
        a
    }

    fn level(s: &[f32], f: f64) -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &x) in s.iter().enumerate() {
            let w = 2.0 * PI * f * i as f64 / RATE;
            re += x as f64 * w.cos();
            im += x as f64 * w.sin();
        }
        20.0 * ((re * re + im * im).sqrt() * 2.0 / s.len() as f64).log10()
    }

    #[test]
    fn untouched_it_is_the_sound_a_window_late() {
        let mut b = EqBand::new(Shape::Bell, 1_000.0, 0.0);
        b.dynamic = Some(Dynamic {
            range_db: -6.0,
            threshold_db: Some(0.0),
            spectral: true,
            ..Dynamic::default()
        });
        let e = Eq {
            bands: Bands::from_slice(&[b]),
            resolution: 1,
            ..Eq::default()
        };
        assert_eq!(latency(&e), 2048);
        let x: Vec<f32> = (0..20_000)
            .map(|i| ((i * 7919) % 1000) as f32 / 1000.0 - 0.5)
            .collect();
        let y = run(&e, &x);
        for i in 4_000..20_000 {
            assert!(
                (y[i] - x[i - 2048]).abs() < 1e-4,
                "{i}: {} vs {}",
                y[i],
                x[i - 2048]
            );
        }
    }

    #[test]
    fn a_spectral_band_takes_down_what_stands_out_and_leaves_the_rest() {
        // Two sines in the band's reach, one loud: it comes down, the quiet one hardly.
        let mut b = EqBand::new(Shape::Bell, 2_000.0, 0.0);
        b.q = 0.5;
        b.dynamic = Some(Dynamic {
            range_db: -9.0,
            threshold_db: Some(-30.0),
            spectral: true,
            density: 1.0,
            tilt: false,
            ..Dynamic::default()
        });
        let e = Eq {
            bands: Bands::from_slice(&[b]),
            resolution: 2,
            ..Eq::default()
        };
        let n = 96_000;
        let x: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f64 / RATE;
                ((2.0 * PI * 1_500.0 * t).sin() * 0.4 + (2.0 * PI * 2_700.0 * t).sin() * 0.01)
                    as f32
            })
            .collect();
        let y = run(&e, &x);
        let tail = |s: &[f32]| s[n / 2..].to_vec();
        let (yl, xl) = (tail(&y), tail(&x));
        let loud = level(&yl, 1_500.0) - level(&xl, 1_500.0);
        let quiet = level(&yl, 2_700.0) - level(&xl, 2_700.0);
        assert!(loud < -5.0, "{loud}");
        assert!(quiet > -1.5, "{quiet}");
    }

    #[test]
    fn in_linear_phase_a_dynamic_band_moves_as_a_whole() {
        let mut b = EqBand::new(Shape::Bell, 1_000.0, 0.0);
        b.dynamic = Some(Dynamic {
            range_db: -6.0,
            threshold_db: Some(-40.0),
            ..Dynamic::default()
        });
        let e = Eq {
            bands: Bands::from_slice(&[b]),
            processing: Processing::Linear,
            resolution: 1,
            ..Eq::default()
        };
        assert!(needed(&e));
        let n = 72_000;
        let x: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * 1_000.0 * i as f64 / RATE).sin() as f32 * 0.25)
            .collect();
        let y = run(&e, &x);
        let moved = level(&y[n / 2..], 1_000.0) - level(&x[n / 2..], 1_000.0);
        assert!((moved + 6.0).abs() < 0.6, "{moved}");
    }
}
