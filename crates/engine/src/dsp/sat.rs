//! The channel strip's saturator, running: the sound split into its bands (Linkwitz-Riley
//! crossovers, the lower bands put through the higher crossovers' allpasses so the bands add
//! back flat), each band driven through its style's curve at 2, 4 or 8 times the sample rate
//! (half-band filters, and ADAA on the curves that allow it), then its tone, dynamics, feedback,
//! mix and level. The dry sound for the mixes takes the same path untouched, so the two line up.

mod filters;
pub mod learn;
mod shapes;

use crate::{
    dsp::comp::{ftz, pole},
    song::project::{MAX_SAT_BANDS, SatBand, SatStyle, Saturator},
};
use filters::{Biquad, OnePole};
pub(crate) use filters::{DcBlock, Halfband, Splitter};
use shapes::{Curve, average, crush, curve, slope};

/// What the tone control tilts around, and by how much each way at its ends.
const TILT_HZ: f32 = 1500.0;
const TILT_DB: f32 = 6.0;
/// Where a band's dynamics start, dBFS, and the most they move it, dB.
const DYN_THRESHOLD: f32 = -24.0;
/// The most of a band fed back.
const FEEDBACK: f32 = 0.9;
/// How many samples its controls (and what's worked out from them: the curve's slope, the
/// gains) hold before they're worked out again: costly sums, done less than every sample.
const CHUNK: usize = 16;

/// A value as it glides to where it's set, as a control does.
#[derive(Clone, Copy, Default)]
struct Glide(f32);

impl Glide {
    fn to(&mut self, target: f32, p: f32) -> f32 {
        self.0 = (1.0 - p) * target + p * self.0;
        self.0
    }
}

/// One side of one band, running.
#[derive(Clone, Copy, Default)]
struct Lane {
    /// Up to three half-band stages up and down, for the saturated sound and for the dry (the
    /// band as it came in, before its style's filters and feedback touch it).
    up: [Halfband; 3],
    wet_down: [Halfband; 3],
    dry_up: [Halfband; 3],
    dry_down: [Halfband; 3],
    /// The last oversampled curve input and its antiderivative's value, for ADAA.
    u_was: f32,
    dc: DcBlock,
    tilt: OnePole,
    pre: [OnePole; 2],
    post: OnePole,
    feedback: Biquad,
    /// What it made last, for the feedback.
    made: f32,
}

/// One band, both sides, and the controls they share.
#[derive(Clone, Copy, Default)]
struct Band {
    lanes: [Lane; 2],
    drive: Glide,
    tone: Glide,
    mix: Glide,
    level: Glide,
    /// The band's level as heard (louder side, linear), and the gain its dynamics give it, dB.
    envelope: f32,
    dynamics: Glide,
    /// What its filters were tuned for: (style, feedback Hz).
    tuned: Option<(SatStyle, f32)>,
}

pub struct Saturation {
    rate: f32,
    bands: [Band; MAX_SAT_BANDS],
    /// Each side split into its bands.
    split: [Splitter; 2],
    mix: Glide,
    output: Glide,
    started: bool,
}

/// What the curve does at a drive (linear), its bias, and the slope there it's divided by.
#[derive(Clone, Copy)]
struct Drive<'a> {
    curve: &'a Curve,
    drive: f32,
    rest: f32,
    gain: f32,
}

impl<'a> Drive<'a> {
    fn new(curve: &'a Curve, style: SatStyle, drive: f32) -> Self {
        let b = curve.bias;
        let gain = match (curve.unity, style) {
            // Two stages: the slope of both at rest.
            (_, SatStyle::Heavy) => {
                let h = 1e-3;
                let g = |s: f32| (curve.f)(3.0 * (curve.f)(drive * s + b));
                ((g(h) - g(-h)) / (2.0 * h)).max(1e-6)
            }
            (true, _) => drive * slope(curve.f, b),
            (false, _) => drive.sqrt(),
        };
        let rest = match style {
            SatStyle::Heavy => (curve.f)(3.0 * (curve.f)(b)),
            _ if curve.unity => (curve.f)(b),
            _ => 0.0,
        };
        Drive {
            curve,
            drive,
            rest,
            gain,
        }
    }
}

impl Saturation {
    pub fn new(rate: f64) -> Self {
        // The oversampling's later stages, with less to filter, filter less.
        let mut band = Band::default();
        for lane in &mut band.lanes {
            for stages in [
                &mut lane.up,
                &mut lane.wet_down,
                &mut lane.dry_up,
                &mut lane.dry_down,
            ] {
                for stage in &mut stages[1..] {
                    stage.light = true;
                }
            }
        }
        Saturation {
            rate: rate as f32,
            bands: [band; MAX_SAT_BANDS],
            split: [Splitter::default(); 2],
            mix: Glide(1.0),
            output: Glide(1.0),
            started: false,
        }
    }

    /// Starts afresh, as an effect that's just come on: its memory cleared, its controls where
    /// they're set rather than gliding there.
    pub fn reset(&mut self, s: &Saturator) {
        *self = Saturation::new(self.rate as f64);
        for (band, b) in self.bands.iter_mut().zip(s.bands.iter()) {
            band.drive = Glide(b.drive_db);
            band.tone = Glide(b.tone);
            band.mix = Glide(b.mix);
            band.level = Glide(b.level_db);
            let c = curve(b.style);
            for lane in &mut band.lanes {
                lane.u_was = c.bias;
            }
        }
        self.mix = Glide(s.mix);
        self.output = Glide(s.output_db);
        self.started = true;
    }

    /// Tunes the crossovers and each band's filters to `s`, keeping their memory.
    fn tune(&mut self, s: &Saturator) {
        let n = s.bands.len();
        let rate = self.rate;
        for split in &mut self.split {
            split.tune(n, &s.crossovers, s.slope, rate);
        }
        for (band, b) in self.bands.iter_mut().zip(s.bands.iter()) {
            let want = (b.style, b.feedback_hz);
            if band.tuned == Some(want) {
                continue;
            }
            for lane in &mut band.lanes {
                lane.tilt.retune(OnePole::low(TILT_HZ, rate));
                let (pre, post) = match b.style {
                    SatStyle::Transformer => (OnePole::low(160.0, rate), OnePole::low(160.0, rate)),
                    SatStyle::Crunch => (OnePole::high(90.0, rate), OnePole::low(5_500.0, rate)),
                    SatStyle::Heavy => (OnePole::high(140.0, rate), OnePole::low(4_200.0, rate)),
                    _ => (OnePole::low(20_000.0, rate), OnePole::low(20_000.0, rate)),
                };
                lane.pre[0].retune(pre);
                lane.post.retune(post);
                lane.feedback
                    .retune(Biquad::band(b.feedback_hz.clamp(20.0, 18_000.0), 2.0, rate));
            }
            band.tuned = Some(want);
        }
    }

    /// Runs a block through it in place.
    pub fn process(&mut self, s: &Saturator, l: &mut [f32], r: &mut [f32]) {
        if !self.started {
            self.reset(s);
        }
        self.tune(s);
        let n = s.bands.len();
        let rate = self.rate;
        let glide = 1.0 - 44.1 / rate;
        let (attack, release) = (pole(0.010, rate), pole(0.150, rate));
        let factor = match s.quality {
            8 => 3,
            4 => 2,
            _ => 1,
        };
        let soloed = s.bands.iter().any(|b| b.solo);
        let curves: [Curve; MAX_SAT_BANDS] =
            std::array::from_fn(|k| curve(s.bands.get(k).map_or(SatStyle::WarmTape, |b| b.style)));
        let len = l.len().min(r.len());
        let mut at = 0;
        while at < len {
            let end = (at + CHUNK).min(len);
            // The controls glide as far as the chunk's samples would take them.
            let p = glide.powi((end - at) as i32);
            let mut ctl: [Option<Ctl>; MAX_SAT_BANDS] = [None; MAX_SAT_BANDS];
            for (k, b) in s.bands.iter().enumerate() {
                let band = &mut self.bands[k];
                let drive_db = band.drive.to(b.drive_db.clamp(0.0, 24.0), p);
                let tone = band.tone.to(b.tone.clamp(-1.0, 1.0), p);
                let mix = band.mix.to(b.mix.clamp(0.0, 1.0), p);
                let level = band.level.to(b.level_db.clamp(-24.0, 24.0), p);
                // Its dynamics: over the threshold, down (compressing) or up (expanding).
                let over = (20.0 * (band.envelope + 1e-9).log10() - DYN_THRESHOLD).max(0.0);
                let amount = b.dynamics.clamp(-1.0, 1.0);
                let strength = 1.0 - 1.0 / (1.0 + 3.0 * amount.abs());
                let target = (-amount.signum() * over * strength).clamp(-24.0, 12.0);
                let dynamics = band.dynamics.to(target, p);
                ctl[k] = Some(Ctl {
                    drive: Drive::new(&curves[k], b.style, 10f32.powf(drive_db / 20.0)),
                    lows: 10f32.powf(-tone * TILT_DB / 20.0),
                    highs: 10f32.powf(tone * TILT_DB / 20.0),
                    mix,
                    gain: 10f32.powf((dynamics + level) / 20.0),
                    heard: !b.mute && (!soloed || b.solo),
                });
            }
            let mix = self.mix.to(s.mix.clamp(0.0, 1.0), p);
            let output = 10f32.powf(self.output.to(s.output_db.clamp(-24.0, 24.0), p) / 20.0);
            for i in at..end {
                let input = [l[i], r[i]];
                let parts = [
                    self.split[0].split(input[0], n),
                    self.split[1].split(input[1], n),
                ];
                let (mut wet_sum, mut dry_sum) = ([0f32; 2], [0f32; 2]);
                for (k, b) in s.bands.iter().enumerate() {
                    let c = ctl[k].expect("every band's controls are worked out");
                    let band = &mut self.bands[k];
                    let loud = parts[0][k].abs().max(parts[1][k].abs());
                    let q = if loud > band.envelope {
                        attack
                    } else {
                        release
                    };
                    band.envelope = ftz((1.0 - q) * loud + q * band.envelope);
                    for side in 0..2 {
                        let lane = &mut band.lanes[side];
                        let (wet, dry) = run(lane, &c.drive, b, parts[side][k], factor);
                        // What it adds tilted darker or brighter around the tilt's frequency.
                        let lp = lane.tilt.run(wet);
                        let wet = lp * c.lows + (wet - lp) * c.highs;
                        if c.heard {
                            wet_sum[side] += (dry + (wet - dry) * c.mix) * c.gain;
                        }
                        dry_sum[side] += dry;
                    }
                }
                l[i] = (dry_sum[0] + (wet_sum[0] - dry_sum[0]) * mix) * output;
                r[i] = (dry_sum[1] + (wet_sum[1] - dry_sum[1]) * mix) * output;
            }
            at = end;
        }
    }
}

/// A band's controls for a chunk, and what's worked out from them.
#[derive(Clone, Copy)]
struct Ctl<'a> {
    drive: Drive<'a>,
    lows: f32,
    highs: f32,
    mix: f32,
    /// Its dynamics and level together.
    gain: f32,
    heard: bool,
}

/// `x` up 2^factor times through `stages`, each stage taking the last's in time order.
fn upsample(stages: &mut [Halfband; 3], x: f32, factor: usize) -> [f32; 8] {
    let mut up = [0f32; 8];
    up[0] = x;
    let mut count = 1;
    for stage in stages.iter_mut().take(factor) {
        let was = up;
        for (j, &v) in was[..count].iter().enumerate() {
            let [e, o] = stage.up(v);
            (up[2 * j], up[2 * j + 1]) = (e, o);
        }
        count *= 2;
    }
    up
}

/// 2^factor samples down to one through `stages`.
fn downsample(stages: &mut [Halfband; 3], mut s: [f32; 8], factor: usize) -> f32 {
    let mut count = 1 << factor;
    for stage in stages[..factor].iter_mut().rev() {
        count /= 2;
        for j in 0..count {
            s[j] = stage.down([s[2 * j], s[2 * j + 1]]);
        }
    }
    s[0]
}

/// One sample of one side of a band: (saturated, dry), both through the same oversampling.
fn run(lane: &mut Lane, d: &Drive, b: &SatBand, band_in: f32, factor: usize) -> (f32, f32) {
    // A band with no feedback and no filter of its style's ahead of the curve goes up once, the
    // dry and the saturated sharing it.
    let plain = b.feedback == 0.0
        && !matches!(
            b.style,
            SatStyle::Transformer | SatStyle::Crunch | SatStyle::Heavy
        );
    let x = band_in + lane.feedback.run(lane.made) * b.feedback.clamp(0.0, 1.0) * FEEDBACK;
    let x = match b.style {
        // The lows pushed in harder, and taken back after.
        SatStyle::Transformer => x + 1.5 * lane.pre[0].run(x),
        SatStyle::Crunch | SatStyle::Heavy => lane.pre[0].run(x),
        _ => x,
    };
    let up = upsample(&mut lane.up, x, factor);
    let dry_up = match plain {
        true => up,
        false => upsample(&mut lane.dry_up, band_in, factor),
    };
    let dry = downsample(&mut lane.dry_down, dry_up, factor);
    let count = 1 << factor;
    let mut shaped = [0f32; 8];
    for (j, &s) in up[..count].iter().enumerate() {
        shaped[j] = match b.style {
            SatStyle::Lofi => crush(s, b.drive_db),
            SatStyle::Heavy => {
                let first = (d.curve.f)(d.drive * s + d.curve.bias);
                ((d.curve.f)(3.0 * first) - d.rest) / d.gain
            }
            _ => {
                let u = d.drive * s + d.curve.bias;
                let y = average(d.curve, u, lane.u_was);
                lane.u_was = u;
                (y - d.rest) / d.gain
            }
        };
    }
    let wet = downsample(&mut lane.wet_down, shaped, factor);
    let mut wet = lane.dc.run(wet);
    wet = match b.style {
        SatStyle::Transformer => wet - 0.6 * lane.post.run(wet),
        SatStyle::Crunch | SatStyle::Heavy => lane.post.run(wet),
        _ => wet,
    };
    lane.made = wet;
    (wet, dry)
}

#[cfg(test)]
mod tests;

/// What a steady level `x` comes out at through a band's curve (its tone, dynamics and mix
/// aside), as the saturator shapes each sample: for drawing it.
pub fn static_curve(b: &SatBand, x: f32) -> f32 {
    let c = curve(b.style);
    let d = Drive::new(&c, b.style, 10f32.powf(b.drive_db.clamp(0.0, 24.0) / 20.0));
    let wet = match b.style {
        SatStyle::Lofi => crush(x, b.drive_db),
        SatStyle::Heavy => ((c.f)(3.0 * (c.f)(d.drive * x + c.bias)) - d.rest) / d.gain,
        _ => ((c.f)(d.drive * x + c.bias) - d.rest) / d.gain,
    };
    x + (wet - x) * b.mix.clamp(0.0, 1.0)
}
