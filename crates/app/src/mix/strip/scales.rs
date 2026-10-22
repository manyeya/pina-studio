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
        CompField::Release => log_pos(c.release_ms, 10.0, 1000.0),
        CompField::Makeup => c.makeup_db / 24.0,
    }
    .clamp(0.0, 1.0)
}

pub fn comp_set(field: CompField, c: &mut Comp, v: f32) {
    match field {
        CompField::Threshold => c.threshold_db = (v * 60.0 - 60.0).round(),
        CompField::Ratio => c.ratio = (log_value(v, 1.0, 20.0) * 10.0).round() / 10.0,
        CompField::Attack => c.attack_ms = (log_value(v, 0.1, 100.0) * 10.0).round() / 10.0,
        CompField::Release => c.release_ms = log_value(v, 10.0, 1000.0).round(),
        CompField::Makeup => c.makeup_db = (v * 48.0).round() / 2.0,
    }
}

pub fn comp_text(field: CompField, c: &Comp) -> String {
    match field {
        CompField::Threshold => format!("{} dB", db_text(c.threshold_db)),
        CompField::Ratio => format!("{:.1}:1", c.ratio),
        CompField::Attack => format!("{} ms", trim(c.attack_ms)),
        CompField::Release => format!("{} ms", trim(c.release_ms)),
        CompField::Makeup => format!("{} dB", db_text(c.makeup_db)),
    }
}

/// A saturator knob's position, 0 to 1.
pub fn sat_pos(field: SatField, s: &Saturator) -> f32 {
    match field {
        SatField::Drive => s.drive_db / 24.0,
        SatField::Tone => (s.tone + 1.0) / 2.0,
        SatField::Mix => s.mix,
        SatField::Output => (s.output_db + 12.0) / 24.0,
    }
    .clamp(0.0, 1.0)
}

pub fn sat_set(field: SatField, s: &mut Saturator, v: f32) {
    match field {
        SatField::Drive => s.drive_db = (v * 48.0).round() / 2.0,
        SatField::Tone => s.tone = ((v * 2.0 - 1.0) * 100.0).round() / 100.0,
        SatField::Mix => s.mix = (v * 100.0).round() / 100.0,
        SatField::Output => s.output_db = (v * 48.0).round() / 2.0 - 12.0,
    }
}

pub fn sat_text(field: SatField, s: &Saturator) -> String {
    match field {
        SatField::Drive => format!("{} dB", trim(s.drive_db)),
        SatField::Tone if s.tone.abs() < 0.005 => "Neutral".into(),
        SatField::Tone => format!(
            "{} {:.0}",
            if s.tone < 0.0 { "Dark" } else { "Bright" },
            s.tone.abs() * 100.0
        ),
        SatField::Mix => format!("{:.0}%", s.mix * 100.0),
        SatField::Output => format!("{} dB", db_text(s.output_db)),
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
