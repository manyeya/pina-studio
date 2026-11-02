//! A strip's controls on their scales: positions to values and back, and as text.

use super::*;

/// Fader position (0 to 1) for a gain in dB: an audio taper, 0 dB at about 0.7, +6 dB at the top.
/// A pan as a mixer prints it: L30, C, R45.
pub fn pan_label(pan: f32) -> String {
    match (pan * 100.0).round() as i32 {
        0 => "C".to_string(),
        v if v < 0 => format!("L{}", -v),
        v => format!("R{v}"),
    }
}

/// A pan typed: "L30", "r 45", "C", or -100 to 100.
pub fn parse_pan(text: &str) -> Option<f32> {
    let t = text.trim().to_uppercase().replace('−', "-");
    let (side, rest) = match t.chars().next()? {
        'L' => (-1.0, &t[1..]),
        'R' => (1.0, &t[1..]),
        'C' if t.len() == 1 => return Some(0.0),
        _ => (1.0, t.as_str()),
    };
    let v: f32 = rest.trim().parse().ok()?;
    (v.is_finite() && v.abs() <= 100.0).then(|| side * v / 100.0)
}

/// A level typed: "-6", "−6 dB", "+2", "-inf"; up to the fader's top (+6).
pub fn parse_db(text: &str) -> Option<f32> {
    let t = text.trim().to_lowercase().replace('−', "-");
    let t = t.trim_end_matches("db").trim();
    if matches!(t, "-inf" | "-∞" | "off") {
        return Some(SILENT);
    }
    let db: f32 = t.parse().ok()?;
    db.is_finite().then(|| db.clamp(SILENT, 6.0))
}

pub fn fader_pos(db: f32) -> f32 {
    10f32.powf((db - 6.0) / 40.0).clamp(0.0, 1.0)
}

pub fn fader_db(pos: f32) -> f32 {
    if pos <= 1e-4 {
        SILENT
    } else {
        (40.0 * pos.log10() + 6.0).max(SILENT)
    }
}

/// "80 Hz", "2.4 kHz".
pub fn hz(f: f32) -> String {
    if f >= 1000.0 {
        format!("{:.1} kHz", f / 1000.0)
    } else {
        format!("{f:.0} Hz")
    }
}

// Knob scales for the built-in EQ and compressor: value (0 to 1) to setting and back.
pub fn low_cut_pos(hz: f32) -> f32 {
    if hz <= LOW_CUT_OFF {
        0.0
    } else {
        (hz / LOW_CUT_OFF).ln() / 50f32.ln()
    }
}

pub fn low_cut_hz(v: f32) -> f32 {
    if v <= 0.005 {
        0.0
    } else {
        (LOW_CUT_OFF * 50f32.powf(v)).round()
    }
}

pub fn eq_gain_pos(db: f32) -> f32 {
    (db + 12.0) / 24.0
}

pub fn eq_gain_db(v: f32) -> f32 {
    ((v * 24.0 - 12.0) * 2.0).round() / 2.0
}

pub(crate) fn log_pos(v: f32, lo: f32, hi: f32) -> f32 {
    (v / lo).ln() / (hi / lo).ln()
}

pub(crate) fn log_value(p: f32, lo: f32, hi: f32) -> f32 {
    lo * (hi / lo).powf(p)
}

pub fn comp_pos(field: CompField, c: &Comp) -> f32 {
    match field {
        CompField::Threshold => (c.threshold_db + 60.0) / 60.0,
        CompField::Ratio => log_pos(c.ratio, 1.0, 20.0),
        CompField::Attack => log_pos(c.attack_ms, 0.1, 100.0),
        CompField::Release => log_pos(c.release_ms, 10.0, 2000.0),
        CompField::Makeup => c.makeup_db / 24.0,
        CompField::Mix => c.mix,
        CompField::Sidechain if c.sidechain_hz < 20.0 => 0.0,
        CompField::Sidechain => log_pos(c.sidechain_hz, 20.0, 500.0),
        CompField::Spectral => c.spectral,
    }
    .clamp(0.0, 1.0)
}

/// A bus compressor's ratio, attack and release go in its steps.
pub fn comp_set(field: CompField, c: &mut Comp, v: f32) {
    let bus = c.style == CompStyle::Bus;
    let step = |x: f32, steps: &[f32]| if bus { nearest_step(x, steps) } else { x };
    match field {
        CompField::Threshold => c.threshold_db = (v * 60.0 - 60.0).round(),
        CompField::Ratio => {
            c.ratio = step(
                (log_value(v, 1.0, 20.0) * 10.0).round() / 10.0,
                &CompStyle::BUS_RATIOS,
            )
        }
        CompField::Attack => {
            c.attack_ms = step(
                (log_value(v, 0.1, 100.0) * 10.0).round() / 10.0,
                &CompStyle::BUS_ATTACKS,
            )
        }
        CompField::Release => {
            c.release_ms = step(log_value(v, 10.0, 2000.0).round(), &CompStyle::BUS_RELEASES)
        }
        CompField::Makeup => c.makeup_db = (v * 48.0).round() / 2.0,
        CompField::Mix => c.mix = (v * 100.0).round() / 100.0,
        CompField::Sidechain if v <= 0.005 => c.sidechain_hz = 0.0,
        CompField::Sidechain => c.sidechain_hz = log_value(v, 20.0, 500.0).round(),
        CompField::Spectral => c.spectral = (v * 100.0).round() / 100.0,
    }
}

pub fn comp_text(field: CompField, c: &Comp) -> String {
    match field {
        CompField::Threshold => format!("{} dB", db_text(c.threshold_db)),
        CompField::Ratio => format!("{}:1", trim(c.ratio)),
        CompField::Attack => format!("{} ms", trim(c.attack_ms)),
        CompField::Release if c.auto_release => "Auto".into(),
        CompField::Release => format!("{} ms", trim(c.release_ms)),
        CompField::Makeup => format!("{} dB", db_text(c.makeup_db)),
        CompField::Mix => format!("{:.0} %", c.mix * 100.0),
        CompField::Sidechain if c.sidechain_hz < 20.0 => "Off".into(),
        CompField::Sidechain => hz(c.sidechain_hz),
        CompField::Spectral if c.spectral < 0.005 => "Off".into(),
        CompField::Spectral => format!("{:.0} %", c.spectral * 100.0),
    }
}

/// A saturator knob's position, 0 to 1. Drive and tone are the first band's, and set every
/// band's: the strip's knobs move them all together.
pub fn sat_pos(field: SatField, s: &Saturator) -> f32 {
    let b = &s.bands[0];
    match field {
        SatField::Drive => b.drive_db / 24.0,
        SatField::Tone => (b.tone + 1.0) / 2.0,
        SatField::Mix => s.mix,
        SatField::Output => (s.output_db + 12.0) / 24.0,
        SatField::Band(i, k) => match (s.bands.get(i as usize), k) {
            (None, _) => 0.0,
            (Some(b), SatKnob::Drive) => b.drive_db / 24.0,
            (Some(b), SatKnob::Tone) => (b.tone + 1.0) / 2.0,
            (Some(b), SatKnob::Dynamics) => (b.dynamics + 1.0) / 2.0,
            (Some(b), SatKnob::Feedback) => b.feedback,
            (Some(b), SatKnob::FeedbackHz) => log_pos(b.feedback_hz, 20.0, 18_000.0),
            (Some(b), SatKnob::Mix) => b.mix,
            (Some(b), SatKnob::Level) => (b.level_db + 24.0) / 48.0,
        },
    }
    .clamp(0.0, 1.0)
}

pub fn sat_set(field: SatField, s: &mut Saturator, v: f32) {
    let bipolar = |v: f32| ((v * 2.0 - 1.0) * 100.0).round() / 100.0;
    match field {
        SatField::Drive => {
            for b in s.bands.iter_mut() {
                b.drive_db = (v * 48.0).round() / 2.0;
            }
        }
        SatField::Tone => {
            for b in s.bands.iter_mut() {
                b.tone = bipolar(v);
            }
        }
        SatField::Mix => s.mix = (v * 100.0).round() / 100.0,
        SatField::Output => s.output_db = (v * 48.0).round() / 2.0 - 12.0,
        SatField::Band(i, k) => {
            let Some(b) = s.bands.get_mut(i as usize) else {
                return;
            };
            match k {
                SatKnob::Drive => b.drive_db = (v * 240.0).round() / 10.0,
                SatKnob::Tone => b.tone = bipolar(v),
                SatKnob::Dynamics => b.dynamics = bipolar(v),
                SatKnob::Feedback => b.feedback = (v * 100.0).round() / 100.0,
                SatKnob::FeedbackHz => b.feedback_hz = log_value(v, 20.0, 18_000.0).round(),
                SatKnob::Mix => b.mix = (v * 100.0).round() / 100.0,
                SatKnob::Level => b.level_db = (v * 96.0).round() / 2.0 - 24.0,
            }
        }
    }
}

pub fn sat_text(field: SatField, s: &Saturator) -> String {
    let b = &s.bands[0];
    let tone = |t: f32| match t.abs() < 0.005 {
        true => "Neutral".to_string(),
        false => format!(
            "{} {:.0}",
            if t < 0.0 { "Dark" } else { "Bright" },
            t.abs() * 100.0
        ),
    };
    match field {
        SatField::Drive => format!("{} dB", trim(b.drive_db)),
        SatField::Tone => tone(b.tone),
        SatField::Mix => format!("{:.0}%", s.mix * 100.0),
        SatField::Output => format!("{} dB", db_text(s.output_db)),
        SatField::Band(i, k) => {
            let Some(b) = s.bands.get(i as usize) else {
                return String::new();
            };
            match k {
                SatKnob::Drive => format!("{} dB", trim(b.drive_db)),
                SatKnob::Tone => tone(b.tone),
                SatKnob::Dynamics if b.dynamics.abs() < 0.005 => "Off".into(),
                SatKnob::Dynamics => format!(
                    "{} {:.0}",
                    if b.dynamics > 0.0 { "Squeeze" } else { "Punch" },
                    b.dynamics.abs() * 100.0
                ),
                SatKnob::Feedback if b.feedback < 0.005 => "Off".into(),
                SatKnob::Feedback => format!("{:.0} %", b.feedback * 100.0),
                SatKnob::FeedbackHz => hz(b.feedback_hz),
                SatKnob::Mix => format!("{:.0} %", b.mix * 100.0),
                SatKnob::Level => format!("{} dB", db_text(b.level_db)),
            }
        }
    }
}

/// A limiter knob's position, 0 to 1.
pub fn limit_pos(field: LimitField, l: &Limiter) -> f32 {
    match field {
        LimitField::Gain => l.gain_db / 30.0,
        LimitField::Ceiling => (l.ceiling_db + 12.0) / 12.0,
        LimitField::Release => log_pos(l.release_ms, 1.0, 2000.0),
        LimitField::Lookahead => log_pos(l.lookahead_ms, 0.1, 10.0),
        LimitField::Attack => log_pos(l.attack_ms, 0.05, 10.0),
        LimitField::Link => l.link,
        LimitField::Clip => l.clip,
    }
    .clamp(0.0, 1.0)
}

pub fn limit_set(field: LimitField, l: &mut Limiter, v: f32) {
    let v = v.clamp(0.0, 1.0);
    let tenth = |x: f32| (x * 10.0).round() / 10.0;
    match field {
        LimitField::Gain => l.gain_db = tenth(v * 30.0),
        LimitField::Ceiling => l.ceiling_db = tenth(v * 12.0 - 12.0),
        LimitField::Release => l.release_ms = log_value(v, 1.0, 2000.0).round().max(1.0),
        LimitField::Lookahead => l.lookahead_ms = (log_value(v, 0.1, 10.0) * 100.0).round() / 100.0,
        LimitField::Attack => l.attack_ms = (log_value(v, 0.05, 10.0) * 100.0).round() / 100.0,
        LimitField::Link => l.link = (v * 100.0).round() / 100.0,
        LimitField::Clip => l.clip = (v * 100.0).round() / 100.0,
    }
}

pub fn limit_text(field: LimitField, l: &Limiter) -> String {
    let ms = |v: f32| match v < 1.0 {
        true => format!("{v:.2} ms"),
        false => format!("{} ms", trim(v)),
    };
    match field {
        LimitField::Gain => format!("+{:.1} dB", l.gain_db),
        LimitField::Ceiling => format!("{} dB", db_text(l.ceiling_db)),
        LimitField::Release if l.auto_release => format!("Auto {}", ms(l.release_ms)),
        LimitField::Release => ms(l.release_ms),
        LimitField::Lookahead => ms(l.lookahead_ms),
        LimitField::Attack => ms(l.attack_ms.min(l.lookahead_ms)),
        LimitField::Link => format!("{:.0} %", l.link * 100.0),
        LimitField::Clip if l.clip < 0.005 => "Off".into(),
        LimitField::Clip => format!("{:.0} %", l.clip * 100.0),
    }
}

pub(crate) fn trim(v: f32) -> String {
    if v >= 10.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

/// An EQ's curve across `width` points, as gains in dB from 20 Hz to 20 kHz.
pub fn eq_curve(eq: &Eq, rate: f64, width: usize) -> Vec<f32> {
    (0..width)
        .map(|i| {
            let f = 20.0 * 1000f64.powf(i as f64 / (width - 1).max(1) as f64);
            eq_response(eq, f, rate) as f32
        })
        .collect()
}
