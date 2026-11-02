//! The built-in delay, running: a line each side, read where the time asked for (in time with
//! the song, or in ms) puts it, gliding there when it moves, as tape does; wobbled by wow and
//! flutter. What comes round again crosses sides as much as it's asked (ping-pong), loses its
//! lows and highs, is driven (a soft clip, so even past full feedback it holds) and blurred, then
//! goes back in. The echoes are widened, ducked under what goes in and mixed. Its lines are made
//! when it is, for the longest it waits, so it allocates nothing while it runs.

use std::f32::consts::TAU;

use crate::{
    dsp::{
        comp::ftz,
        reverb::{Allpass, Biquad, Line, pole},
    },
    song::project::{Delay, DelayStyle, MAX_DELAY_S},
};

/// The echoes' blur, ms.
const DIFFUSERS_MS: [f32; 2] = [7.3, 11.9];
/// Wow and flutter: their rates (Hz) and the most they move the time (ms).
const WOW: (f32, f32) = (0.55, 3.5);
const FLUTTER: (f32, f32) = (7.1, 0.35);

/// Untouched up to 0.8, then softly up to full scale and never past it.
pub(crate) fn knee(x: f32) -> f32 {
    const K: f32 = 0.8;
    match x.abs() <= K {
        true => x,
        false => x.signum() * (K + (1.0 - K) * ((x.abs() - K) / (1.0 - K)).tanh()),
    }
}

/// A delay, running.
pub struct Delaying {
    rate: f32,
    lines: [Line; 2],
    /// Each side's time now, samples, gliding to where it's set.
    time: [f32; 2],
    phase: [f32; 2],
    /// A slow random walk for the flutter.
    noise: (u32, f32),
    cuts: [[Biquad; 2]; 2],
    diffusers: [[Allpass; 2]; 2],
    envelope: f32,
    duck: f32,
    started: bool,
}

impl Delaying {
    pub fn new(rate: f64) -> Self {
        let rate = rate as f32;
        let line = || Line::new(((MAX_DELAY_S + 0.02) * rate) as usize + 8);
        let diffuser = |k: usize| Allpass {
            line: Line::new((DIFFUSERS_MS[k] / 1000.0 * rate) as usize + 4),
        };
        Delaying {
            rate,
            lines: [line(), line()],
            time: [0.0; 2],
            phase: [0.0, 0.25],
            noise: (0x9e37_79b9, 0.0),
            cuts: [[Biquad::default(); 2]; 2],
            diffusers: std::array::from_fn(|_| std::array::from_fn(diffuser)),
            envelope: 0.0,
            duck: 1.0,
            started: false,
        }
    }

    /// Clears its memory: a delay come back on starts silent.
    pub fn reset(&mut self) {
        self.lines.iter_mut().for_each(Line::clear);
        self.diffusers
            .iter_mut()
            .flatten()
            .for_each(|d| d.line.clear());
        self.cuts = [[Biquad::default(); 2]; 2];
        (self.envelope, self.duck, self.started) = (0.0, 1.0, false);
    }

    /// Runs a block through it in place, the song at `bpm`.
    pub fn process(&mut self, d: &Delay, bpm: f64, l: &mut [f32], r: &mut [f32]) {
        let rate = self.rate;
        let target = d.times_s(bpm).map(|s| s * rate);
        if !self.started {
            self.time = target;
            self.started = true;
        }
        // Tape and analog glide slowly to a new time (their pitch bends); digital quickly.
        let glide = pole(
            match d.style {
                DelayStyle::Tape | DelayStyle::Analog | DelayStyle::Dub => 2.0,
                _ => 12.0,
            },
            rate,
        );
        for s in 0..2 {
            self.cuts[s][0].tune(d.low_cut_hz.clamp(20.0, 2000.0), rate, true);
            self.cuts[s][1].tune(d.high_cut_hz.clamp(500.0, 20_000.0), rate, false);
        }
        let feedback = if d.freeze {
            1.0
        } else {
            d.feedback.clamp(0.0, 1.1)
        };
        let cross = match d.style {
            DelayStyle::PingPong => d.cross.clamp(0.0, 1.0).max(0.5),
            _ => d.cross.clamp(0.0, 1.0),
        };
        let drive = 1.0 + 3.0 * d.drive.clamp(0.0, 1.0);
        let tilt = match d.style {
            DelayStyle::Tape => 0.15 * d.drive,
            _ => 0.0,
        };
        let wow = d.wow.clamp(0.0, 1.0) * WOW.1 / 1000.0 * rate;
        let flutter = d.flutter.clamp(0.0, 1.0) * FLUTTER.1 / 1000.0 * rate;
        let diffusion = d.diffusion.clamp(0.0, 1.0) * 0.6;
        let blur: [usize; 2] =
            std::array::from_fn(|k| ((DIFFUSERS_MS[k] / 1000.0 * rate) as usize).max(1));
        let width = d.width.clamp(0.0, 2.0);
        let mix = d.mix.clamp(0.0, 1.0);
        let out_gain = 10f32.powf(d.output_db.clamp(-24.0, 24.0) / 20.0);
        let (attack, release) = (pole(1.0 / 0.005 / TAU, rate), pole(1.0 / 0.3 / TAU, rate));
        let smooth = pole(20.0, rate);
        let ping = d.style == DelayStyle::PingPong;
        let n = l.len().min(r.len());
        for i in 0..n {
            let (dl, dr) = (l[i], r[i]);
            self.phase[0] = (self.phase[0] + WOW.0 / rate) % 1.0;
            self.phase[1] = (self.phase[1] + FLUTTER.0 / rate) % 1.0;
            // The flutter's roughness: noise, slowed.
            self.noise.0 = self
                .noise
                .0
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
            let white = (self.noise.0 >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
            self.noise.1 += (white - self.noise.1) * 0.0005;
            let moved = wow * (TAU * self.phase[0]).sin()
                + flutter * (0.7 * (TAU * self.phase[1]).sin() + 6.0 * self.noise.1);
            let mut y = [0.0f32; 2];
            for s in 0..2 {
                self.time[s] += (target[s] - self.time[s]) * glide;
                // The two sides wobble against each other, as two tape heads do.
                let m = if s == 0 { moved } else { -moved * 0.8 };
                y[s] = self.lines[s].read((self.time[s] + m).max(2.0));
            }
            // Round again: crossed, cut, driven, blurred.
            let mut back = [
                y[0] * (1.0 - cross) + y[1] * cross,
                y[1] * (1.0 - cross) + y[0] * cross,
            ];
            for (s, b) in back.iter_mut().enumerate() {
                if !d.freeze {
                    let low_cut = self.cuts[s][0].run(*b);
                    *b = self.cuts[s][1].run(low_cut);
                }
                // Driven (tape a little lopsided), then a soft knee: untouched under it, so a
                // frozen echo keeps its level, and feedback past 1 holds at full scale.
                *b = match drive > 1.0 {
                    true => ((*b * drive + tilt).tanh() - tilt.tanh()) / drive.sqrt(),
                    false => *b,
                };
                *b = knee(*b);
                if diffusion > 0.0 {
                    for (diffuser, &d) in self.diffusers[s].iter_mut().zip(&blur) {
                        *b = diffuser.run(*b, d, diffusion);
                    }
                }
            }
            let input = match (d.freeze, ping) {
                (true, _) => [0.0, 0.0],
                // Ping-pong starts on one side and bounces.
                (false, true) => [0.5 * (dl + dr), 0.0],
                (false, false) => [dl, dr],
            };
            for s in 0..2 {
                self.lines[s].push(ftz(input[s] + back[s] * feedback));
            }
            // Widened, ducked while the dry sound plays, mixed.
            let (m, sd) = ((y[0] + y[1]) * 0.5, (y[0] - y[1]) * 0.5 * width);
            let level = dl.abs().max(dr.abs());
            let a = if level > self.envelope {
                attack
            } else {
                release
            };
            self.envelope = ftz(self.envelope + a * (level - self.envelope));
            let over = ((20.0 * (self.envelope + 1e-9).log10() + 50.0) / 40.0).clamp(0.0, 1.0);
            let want = 10f32.powf(-d.ducking.clamp(0.0, 1.0) * 18.0 * over / 20.0);
            self.duck += (want - self.duck) * smooth;
            let (wl, wr) = ((m + sd) * self.duck, (m - sd) * self.duck);
            l[i] = (dl * (1.0 - mix) + wl * mix) * out_gain;
            r[i] = (dr * (1.0 - mix) + wr * mix) * out_gain;
        }
    }
}

#[cfg(test)]
mod tests;
