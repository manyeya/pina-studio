//! Panning: the pan law, balance and stereo matrices, and the binaural panner.

use super::*;

/// Left and right gain for a level and a balance pan.
pub(crate) fn balance(g: f32, pan: f32) -> [f32; 2] {
    let pan = pan.clamp(-1.0, 1.0);
    [g * (1.0 - pan).min(1.0), g * (1.0 + pan).min(1.0)]
}

/// Left and right gain for a sound at `pan`, by the pan law.
pub(crate) fn law_gains(law: PanLaw, pan: f32) -> [f32; 2] {
    let pan = pan.clamp(-1.0, 1.0);
    let theta = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
    match law {
        PanLaw::Flat => balance(1.0, pan),
        PanLaw::Minus3 => [theta.cos(), theta.sin()],
        PanLaw::Minus3Compensated => [
            theta.cos() * std::f32::consts::SQRT_2,
            theta.sin() * std::f32::consts::SQRT_2,
        ],
        PanLaw::Minus6 => [(1.0 - pan) / 2.0, (1.0 + pan) / 2.0],
    }
}

/// How a lane's left and right become the mix's: out left = in left × m[0] + in right × m[1],
/// out right = in left × m[2] + in right × m[3]; at level `g`, panned by its mode.
pub fn pan_matrix(mode: PanMode, law: PanLaw, pan: f32, spread: f32, g: f32) -> [f32; 4] {
    match mode {
        PanMode::Balance => {
            let [a, b] = law_gains(law, pan);
            [g * a, 0.0, 0.0, g * b]
        }
        // Each side placed by equal power: spread 1 leaves them where they were.
        PanMode::Stereo => {
            let s = spread.clamp(-1.0, 1.0);
            let place = |x: f32| {
                let theta = (x.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
                (theta.cos(), theta.sin())
            };
            let (ll, lr) = place(pan - s);
            let (rl, rr) = place(pan + s);
            [g * ll, g * rl, g * lr, g * rr]
        }
        // Level here; where around the head, after.
        PanMode::Binaural => [g, 0.0, 0.0, g],
    }
}

/// A binaural panner's memory: the sound a moment back (for the far ear's delay), and each
/// ear's filters.
pub struct Binaural {
    pub(crate) line: [f32; 256],
    pub(crate) at: usize,
    /// The far ear's head shadow; each ear's dulling (behind, below, far off); each ear's
    /// presence, lifted above.
    pub(crate) shadow: f32,
    pub(crate) dull: [f32; 2],
    pub(crate) air: [f32; 2],
}

impl Default for Binaural {
    fn default() -> Self {
        Binaural {
            line: [0.0; 256],
            at: 0,
            shadow: 0.0,
            dull: [0.0; 2],
            air: [0.0; 2],
        }
    }
}

impl Binaural {
    /// Places a block round the head, as a head hears it: the far ear later (up to 0.66 ms, by
    /// Woodworth's formula), quieter and darker in the head's shadow; behind and below duller
    /// (the ears face forward, the body's below); above brighter; further off quieter and
    /// duller (the air).
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], place: Place, rate: f32) {
        let [x, y, z] = place.xyz();
        let d = place.distance;
        let side = (x / d).clamp(-1.0, 1.0);
        let around = side.abs();
        let (back, up) = ((-z / d).max(0.0), (y / d).clamp(-1.0, 1.0));
        let delay = (0.000_255 * (around.asin() + around) * rate).min(250.0);
        let pole = |hz: f32| (-std::f32::consts::TAU * hz.min(rate * 0.45) / rate).exp();
        let shadow = pole(20_000.0 - 17_500.0 * around);
        let dulled = (0.55 * back + 0.6 * (-up).max(0.0) + 0.3 * (d - 1.0).max(0.0)).min(0.85);
        let dull = pole(20_000.0 * (1.0 - dulled));
        let air = pole(3_000.0);
        let lift = 0.5 * up.max(0.0);
        let gain = 1.0 / (0.5 + 0.5 * d) * (1.0 - 0.12 * back);
        let (far_gain, near_gain) = (1.0 - 0.45 * around, 1.0 + 0.2 * around);
        for i in 0..l.len().min(r.len()) {
            let mono = 0.5 * (l[i] + r[i]);
            self.line[self.at] = mono;
            let behind = self.at as f32 - delay;
            let k = behind.floor();
            let t = behind - k;
            let idx = |n: f32| (n.rem_euclid(256.0)) as usize;
            let delayed = self.line[idx(k)] * (1.0 - t) + self.line[idx(k + 1.0)] * t;
            self.at = (self.at + 1) % 256;
            self.shadow = delayed + (self.shadow - delayed) * shadow;
            let (near, far) = (mono * near_gain, self.shadow * far_gain);
            let mut ears = if side < 0.0 { [near, far] } else { [far, near] };
            for (e, v) in ears.iter_mut().enumerate() {
                self.dull[e] = *v + (self.dull[e] - *v) * dull;
                let s = self.dull[e];
                self.air[e] = s + (self.air[e] - s) * air;
                *v = (s + (s - self.air[e]) * lift) * gain;
            }
            (l[i], r[i]) = (ears[0], ears[1]);
        }
    }
}
