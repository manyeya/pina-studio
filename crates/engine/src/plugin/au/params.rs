//! A parameter's range and curve, 0 to 1 and back, and its units.

use super::*;

/// How a parameter's 0 to 1 maps onto its values, as its display flags say: steps for an indexed
/// or on/off one, else a curve from its minimum to its maximum.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Range {
    pub(crate) min: f32,
    pub(crate) max: f32,
    pub(crate) steps: u32,
    pub(crate) curve: Curve,
    /// Whether the unit follows ramps scheduled for it.
    pub(crate) ramps: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Curve {
    Linear,
    Log,
    /// The value's share of the range is the position to this power.
    Power(f64),
}

impl Range {
    pub(crate) fn of(info: &AudioUnitParameterInfo) -> Range {
        use AudioUnitParameterOptions as F;
        let (min, max) = (info.minValue, info.maxValue.max(info.minValue));
        let steps = match info.unit {
            AudioUnitParameterUnit::Indexed | AudioUnitParameterUnit::Boolean => {
                (max - min).round().max(0.0) as u32
            }
            _ => 0,
        };
        let flags = info.flags;
        let curve = if flags.contains(F::Flag_DisplayLogarithmic) && min > 0.0 {
            Curve::Log
        } else {
            match F(flags.0 & F::Flag_DisplayMask.0) {
                F::Flag_DisplaySquareRoot => Curve::Power(2.0),
                F::Flag_DisplaySquared => Curve::Power(0.5),
                F::Flag_DisplayCubed => Curve::Power(1.0 / 3.0),
                F::Flag_DisplayCubeRoot => Curve::Power(3.0),
                _ => Curve::Linear,
            }
        };
        Range {
            min,
            max,
            steps,
            curve,
            ramps: flags.contains(F::Flag_CanRamp),
        }
    }

    /// The value at `x`, 0 to 1.
    pub(crate) fn value(&self, x: f64) -> f32 {
        let (min, max, x) = (self.min as f64, self.max as f64, x.clamp(0.0, 1.0));
        let v = if self.steps > 0 {
            min + (x * self.steps as f64).round()
        } else {
            match self.curve {
                Curve::Log => min * (max / min).powf(x),
                Curve::Power(p) => min + (max - min) * x.powf(p),
                Curve::Linear => min + (max - min) * x,
            }
        };
        v as f32
    }

    /// Where `v` sits, 0 to 1.
    pub(crate) fn position(&self, v: f32) -> f64 {
        let (min, max, v) = (self.min as f64, self.max as f64, v as f64);
        if max <= min {
            return 0.0;
        }
        let x = if self.steps > 0 {
            (v - min).round() / self.steps as f64
        } else {
            let share = (v - min) / (max - min);
            match self.curve {
                Curve::Log => (v.max(min) / min).ln() / (max / min).ln(),
                Curve::Power(p) => share.max(0.0).powf(1.0 / p),
                Curve::Linear => share,
            }
        };
        x.clamp(0.0, 1.0)
    }
}

/// A parameter unit's name, as Pina Studio shows it after a value.
pub(crate) fn units(unit: AudioUnitParameterUnit) -> &'static str {
    use AudioUnitParameterUnit as U;
    match unit {
        U::Percent | U::EqualPowerCrossfade => "%",
        U::Seconds => "s",
        U::SampleFrames => "samples",
        U::Phase | U::Degrees => "°",
        U::Rate => "x",
        U::Hertz => "Hz",
        U::Cents | U::AbsoluteCents => "cents",
        U::RelativeSemiTones => "semitones",
        U::Decibels => "dB",
        U::Meters => "m",
        U::Octaves => "octaves",
        U::BPM => "BPM",
        U::Beats => "beats",
        U::Milliseconds => "ms",
        U::Ratio => ":1",
        _ => "",
    }
}

/// A value and its unit, as Pina Studio shows a parameter the unit doesn't put into words: "2400 Hz",
/// "-6.0 dB", "50%".
pub(crate) fn with_units(v: f32, units: &str) -> String {
    let v = v as f64;
    let digits = match v.abs() {
        a if a >= 100.0 => 0,
        a if a >= 10.0 || a == 0.0 => 1,
        a if a >= 1.0 => 2,
        _ => 3,
    };
    let n = format!("{v:.digits$}");
    match units {
        "" => n,
        "%" | "°" | ":1" | "x" => format!("{n}{units}"),
        _ => format!("{n} {units}"),
    }
}

/// A typed value as `with_units` shows it, a number in the parameter's units: "850 Hz", "2.4
/// kHz", "-6 dB".
pub(crate) fn typed(text: &str) -> Option<f32> {
    let text = text.trim();
    let end = text
        .find(|c: char| !(c.is_ascii_digit() || "+-.".contains(c)))
        .unwrap_or(text.len());
    let n: f64 = text[..end].parse().ok()?;
    let kilo = text[end..].trim_start().starts_with(['k', 'K']);
    Some((if kilo { n * 1e3 } else { n }) as f32)
}
