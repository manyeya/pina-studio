//! The reverb's and delay's knobs: where each setting sits on its knob (0 to 1), set from one,
//! and said.

use pina_engine::song::project::{Delay, Reverb};

/// A reverb's knob.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReverbField {
    Predelay,
    Size,
    Decay,
    Early,
    Diffusion,
    Damping,
    Bass,
    Modulation,
    LowCut,
    HighCut,
    Width,
    Ducking,
    Shimmer,
    Mix,
    Output,
}

/// A delay's knob: each side's time (`Time(0)` left), and the rest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DelayField {
    Time(u8),
    Feedback,
    Cross,
    LowCut,
    HighCut,
    Drive,
    Wow,
    Flutter,
    Diffusion,
    Ducking,
    Width,
    Mix,
    Output,
}

/// Note values a synced time steps through, in beats, and what they're called.
pub const NOTES: [(f32, &str); 17] = [
    (0.0625, "1/64"),
    (0.0833, "1/32 T"),
    (0.125, "1/32"),
    (0.1667, "1/16 T"),
    (0.1875, "1/32 D"),
    (0.25, "1/16"),
    (0.3333, "1/8 T"),
    (0.375, "1/16 D"),
    (0.5, "1/8"),
    (0.6667, "1/4 T"),
    (0.75, "1/8 D"),
    (1.0, "1/4"),
    (1.3333, "1/2 T"),
    (1.5, "1/4 D"),
    (2.0, "1/2"),
    (3.0, "1/2 D"),
    (4.0, "1 bar"),
];

/// The note value nearest `beats`, by its place in `NOTES`.
pub fn nearest_note(beats: f32) -> usize {
    NOTES
        .iter()
        .enumerate()
        .min_by(|a, b| {
            let d = |x: f32| (x / beats.max(1e-3)).ln().abs();
            d(a.1.0).total_cmp(&d(b.1.0))
        })
        .map_or(8, |(k, _)| k)
}

pub fn note_name(beats: f32) -> &'static str {
    NOTES[nearest_note(beats)].1
}

fn log_pos(v: f32, lo: f32, hi: f32) -> f32 {
    ((v.max(lo) / lo).ln() / (hi / lo).ln()).clamp(0.0, 1.0)
}

fn log_value(p: f32, lo: f32, hi: f32) -> f32 {
    lo * (hi / lo).powf(p.clamp(0.0, 1.0))
}

fn hz(v: f32) -> String {
    match v >= 1000.0 {
        true => format!("{:.1} kHz", v / 1000.0),
        false => format!("{v:.0} Hz"),
    }
}

fn percent(v: f32) -> String {
    format!("{:.0} %", v * 100.0)
}

fn db(v: f32) -> String {
    format!("{v:+.1} dB")
}

pub fn reverb_pos(f: ReverbField, r: &Reverb) -> f32 {
    match f {
        ReverbField::Predelay => match r.predelay_beats {
            Some(b) => nearest_note(b).min(8) as f32 / 8.0,
            None => (r.predelay_ms / 500.0).clamp(0.0, 1.0).sqrt(),
        },
        ReverbField::Size => r.size,
        ReverbField::Decay => log_pos(r.decay_s, 0.1, 30.0),
        ReverbField::Early => r.early,
        ReverbField::Diffusion => r.diffusion,
        ReverbField::Damping => log_pos(r.damping_hz, 500.0, 20_000.0),
        ReverbField::Bass => log_pos(r.bass, 0.25, 4.0),
        ReverbField::Modulation => r.modulation,
        ReverbField::LowCut => log_pos(r.low_cut_hz, 20.0, 2000.0),
        ReverbField::HighCut => log_pos(r.high_cut_hz, 1000.0, 20_000.0),
        ReverbField::Width => r.width / 2.0,
        ReverbField::Ducking => r.ducking,
        ReverbField::Shimmer => r.shimmer,
        ReverbField::Mix => r.mix,
        ReverbField::Output => (r.output_db + 24.0) / 48.0,
    }
    .clamp(0.0, 1.0)
}

pub fn reverb_set(f: ReverbField, r: &mut Reverb, p: f32) {
    let p = p.clamp(0.0, 1.0);
    match f {
        ReverbField::Predelay => match r.predelay_beats {
            // Synced: a 1/64 to an eighth.
            Some(_) => r.predelay_beats = Some(NOTES[(p * 8.0).round() as usize].0),
            None => r.predelay_ms = (p * p * 500.0).round(),
        },
        ReverbField::Size => r.size = p,
        ReverbField::Decay => r.decay_s = (log_value(p, 0.1, 30.0) * 100.0).round() / 100.0,
        ReverbField::Early => r.early = p,
        ReverbField::Diffusion => r.diffusion = p,
        ReverbField::Damping => r.damping_hz = log_value(p, 500.0, 20_000.0).round(),
        ReverbField::Bass => r.bass = (log_value(p, 0.25, 4.0) * 100.0).round() / 100.0,
        ReverbField::Modulation => r.modulation = p,
        ReverbField::LowCut => r.low_cut_hz = log_value(p, 20.0, 2000.0).round(),
        ReverbField::HighCut => r.high_cut_hz = log_value(p, 1000.0, 20_000.0).round(),
        ReverbField::Width => r.width = p * 2.0,
        ReverbField::Ducking => r.ducking = p,
        ReverbField::Shimmer => r.shimmer = p,
        ReverbField::Mix => r.mix = p,
        ReverbField::Output => r.output_db = (p * 48.0 - 24.0).round(),
    }
}

pub fn reverb_text(f: ReverbField, r: &Reverb) -> String {
    match f {
        ReverbField::Predelay => match r.predelay_beats {
            Some(b) => note_name(b).to_string(),
            None => format!("{:.0} ms", r.predelay_ms),
        },
        ReverbField::Size => percent(r.size),
        ReverbField::Decay => match r.decay_s < 10.0 {
            true => format!("{:.2} s", r.decay_s),
            false => format!("{:.1} s", r.decay_s),
        },
        ReverbField::Early => percent(r.early),
        ReverbField::Diffusion => percent(r.diffusion),
        ReverbField::Damping => hz(r.damping_hz),
        ReverbField::Bass => format!("×{:.2}", r.bass),
        ReverbField::Modulation => percent(r.modulation),
        ReverbField::LowCut => hz(r.low_cut_hz),
        ReverbField::HighCut => hz(r.high_cut_hz),
        ReverbField::Width => percent(r.width),
        ReverbField::Ducking => percent(r.ducking),
        ReverbField::Shimmer => percent(r.shimmer),
        ReverbField::Mix => percent(r.mix),
        ReverbField::Output => db(r.output_db),
    }
}

pub fn delay_pos(f: DelayField, d: &Delay) -> f32 {
    match f {
        DelayField::Time(s) => {
            let s = (s as usize).min(1);
            match d.sync {
                true => nearest_note(d.beats[s]) as f32 / (NOTES.len() - 1) as f32,
                false => log_pos(d.ms[s], 1.0, 5000.0),
            }
        }
        DelayField::Feedback => d.feedback / 1.1,
        DelayField::Cross => d.cross,
        DelayField::LowCut => log_pos(d.low_cut_hz, 20.0, 2000.0),
        DelayField::HighCut => log_pos(d.high_cut_hz, 500.0, 20_000.0),
        DelayField::Drive => d.drive,
        DelayField::Wow => d.wow,
        DelayField::Flutter => d.flutter,
        DelayField::Diffusion => d.diffusion,
        DelayField::Ducking => d.ducking,
        DelayField::Width => d.width / 2.0,
        DelayField::Mix => d.mix,
        DelayField::Output => (d.output_db + 24.0) / 48.0,
    }
    .clamp(0.0, 1.0)
}

pub fn delay_set(f: DelayField, d: &mut Delay, p: f32) {
    let p = p.clamp(0.0, 1.0);
    match f {
        DelayField::Time(s) => {
            let s = (s as usize).min(1);
            match d.sync {
                true => {
                    let k = (p * (NOTES.len() - 1) as f32).round() as usize;
                    d.beats[s] = NOTES[k].0;
                }
                false => d.ms[s] = log_value(p, 1.0, 5000.0).round().max(1.0),
            }
        }
        DelayField::Feedback => d.feedback = (p * 110.0).round() / 100.0,
        DelayField::Cross => d.cross = p,
        DelayField::LowCut => d.low_cut_hz = log_value(p, 20.0, 2000.0).round(),
        DelayField::HighCut => d.high_cut_hz = log_value(p, 500.0, 20_000.0).round(),
        DelayField::Drive => d.drive = p,
        DelayField::Wow => d.wow = p,
        DelayField::Flutter => d.flutter = p,
        DelayField::Diffusion => d.diffusion = p,
        DelayField::Ducking => d.ducking = p,
        DelayField::Width => d.width = p * 2.0,
        DelayField::Mix => d.mix = p,
        DelayField::Output => d.output_db = (p * 48.0 - 24.0).round(),
    }
}

pub fn delay_text(f: DelayField, d: &Delay) -> String {
    match f {
        DelayField::Time(s) => {
            let s = (s as usize).min(1);
            match d.sync {
                true => note_name(d.beats[s]).to_string(),
                false => format!("{:.0} ms", d.ms[s]),
            }
        }
        DelayField::Feedback => percent(d.feedback),
        DelayField::Cross => percent(d.cross),
        DelayField::LowCut => hz(d.low_cut_hz),
        DelayField::HighCut => hz(d.high_cut_hz),
        DelayField::Drive => percent(d.drive),
        DelayField::Wow => percent(d.wow),
        DelayField::Flutter => percent(d.flutter),
        DelayField::Diffusion => percent(d.diffusion),
        DelayField::Ducking => percent(d.ducking),
        DelayField::Width => percent(d.width),
        DelayField::Mix => percent(d.mix),
        DelayField::Output => db(d.output_db),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knobs_go_round_and_back() {
        let mut r = Reverb::default();
        for f in [ReverbField::Decay, ReverbField::Damping, ReverbField::Size] {
            let p = reverb_pos(f, &r);
            reverb_set(f, &mut r, p);
            assert!((reverb_pos(f, &r) - p).abs() < 0.01, "{f:?}");
        }
        // Synced, an eighth; a step up the knob, the next note value: a quarter triplet.
        let mut d = Delay::default();
        let p = delay_pos(DelayField::Time(0), &d);
        assert_eq!(delay_text(DelayField::Time(0), &d), "1/8");
        delay_set(DelayField::Time(0), &mut d, p + 1.0 / 16.0);
        assert_eq!(delay_text(DelayField::Time(0), &d), "1/4 T");
        assert_eq!(note_name(0.75), "1/8 D");
    }
}
