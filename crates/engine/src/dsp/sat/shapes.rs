//! The saturator's curves, one a style: what a sample comes out at for how hard it's driven. The
//! smooth ones come with their antiderivatives, for first-order antiderivative antialiasing
//! (ADAA): the curve's average between one oversampled sample and the next rather than its value
//! at each, so what the samples skip over isn't lost to aliasing.

use crate::song::project::SatStyle;

/// log cosh, tanh's antiderivative, without overflowing.
fn logcosh(u: f32) -> f32 {
    u.abs() + (1.0 + (-2.0 * u.abs()).exp()).ln() - std::f32::consts::LN_2
}

/// How much more the low side of a tube's curve gives before it clips.
const TUBE: f32 = 0.5;

/// A curve: its function, its antiderivative (when it has one to hand), and the bias it's driven
/// around, for the even harmonics an asymmetry brings.
pub struct Curve {
    pub f: fn(f32) -> f32,
    pub antiderivative: Option<fn(f32) -> f32>,
    pub bias: f32,
    /// Whether quiet sounds come out at their own level (the curve's slope at the bias divided
    /// out); the folding and crushing styles don't, being changes of sound, not of level.
    pub unity: bool,
}

fn tanh(u: f32) -> f32 {
    u.tanh()
}

fn tube(u: f32) -> f32 {
    match u >= 0.0 {
        true => u.tanh(),
        false => (TUBE * u).tanh() / TUBE,
    }
}

fn tube_ad(u: f32) -> f32 {
    match u >= 0.0 {
        true => logcosh(u),
        false => logcosh(TUBE * u) / (TUBE * TUBE),
    }
}

/// The gentlest: u - u³/3 to its peak at 1, flat past it.
fn cubic(u: f32) -> f32 {
    match u.abs() < 1.0 {
        true => u - u * u * u / 3.0,
        false => u.signum() * 2.0 / 3.0,
    }
}

fn cubic_ad(u: f32) -> f32 {
    match u.abs() < 1.0 {
        true => u * u / 2.0 - u * u * u * u / 12.0,
        false => 2.0 / 3.0 * u.abs() - 0.25,
    }
}

/// Rectified: tanh's magnitude, both halves above zero (the DC this leaves is blocked after).
fn rectify(u: f32) -> f32 {
    u.tanh().abs()
}

fn rectify_ad(u: f32) -> f32 {
    u.signum() * logcosh(u)
}

fn fold(u: f32) -> f32 {
    u.sin()
}

fn fold_ad(u: f32) -> f32 {
    -u.cos()
}

/// A guitar amp's stage: harder than tanh, its low side a little softer.
fn amp(u: f32) -> f32 {
    let u = if u < 0.0 { u * 0.8 } else { u };
    u / (1.0 + u.abs().powf(2.5)).powf(0.4)
}

pub fn curve(style: SatStyle) -> Curve {
    let smooth = |f: fn(f32) -> f32, ad: fn(f32) -> f32, bias: f32| Curve {
        f,
        antiderivative: Some(ad),
        bias,
        unity: true,
    };
    match style {
        SatStyle::WarmTape => smooth(tanh, logcosh, 0.15),
        SatStyle::CleanTape => smooth(tanh, logcosh, 0.0),
        SatStyle::WarmTube => smooth(tube, tube_ad, 0.1),
        SatStyle::CleanTube => smooth(cubic, cubic_ad, 0.0),
        SatStyle::Transformer => smooth(tanh, logcosh, 0.05),
        SatStyle::Crunch | SatStyle::Heavy => Curve {
            f: amp,
            antiderivative: None,
            bias: 0.2,
            unity: true,
        },
        SatStyle::Rectify => Curve {
            f: rectify,
            antiderivative: Some(rectify_ad),
            bias: 0.0,
            unity: false,
        },
        SatStyle::Foldback => Curve {
            f: fold,
            antiderivative: Some(fold_ad),
            bias: 0.0,
            unity: false,
        },
        // Crushed rather than curved: see `crush`.
        SatStyle::Lofi => Curve {
            f: |u| u,
            antiderivative: None,
            bias: 0.0,
            unity: false,
        },
    }
}

/// A curve's slope at `x`, numerically, to divide out so quiet sounds keep their level.
pub fn slope(f: fn(f32) -> f32, x: f32) -> f32 {
    let h = 1e-3;
    ((f(x + h) - f(x - h)) / (2.0 * h)).max(1e-6)
}

/// The curve's average between `u0` and `u1` (by its antiderivative, where they're far enough
/// apart), else its value midway: the ADAA sample.
pub fn average(c: &Curve, u1: f32, u0: f32) -> f32 {
    match c.antiderivative {
        Some(ad) if (u1 - u0).abs() >= 0.001 => (ad(u1) - ad(u0)) / (u1 - u0),
        _ => (c.f)((u1 + u0) / 2.0),
    }
}

/// Lo-fi: `x` held to as many levels as its bits give: 12 undriven, down to 2 at full drive.
pub fn crush(x: f32, drive_db: f32) -> f32 {
    let bits = 12.0 - drive_db.clamp(0.0, 24.0) * 10.0 / 24.0;
    let levels = 2f32.powf(bits - 1.0);
    (x * levels).round() / levels
}
