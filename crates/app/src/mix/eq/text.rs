//! Values as the EQ shows and reads them: frequencies, notes, slopes, widths.

use super::*;

/// A frequency as typed: "100", "2k", "1.5 kHz", "A4", "C#2+13" (cents).
pub(crate) fn parse_freq(t: &str) -> Option<f32> {
    let t = t.trim();
    let hz = t.trim_end_matches("hz").trim();
    if let Some(k) = hz.strip_suffix('k') {
        return k.trim().parse::<f32>().ok().map(|k| k * 1_000.0);
    }
    if let Ok(v) = hz.parse::<f32>() {
        return Some(v);
    }
    // A note: its letter, a sharp or flat, its octave, and cents.
    let mut chars = t.chars().peekable();
    let letter = chars.next()?;
    let mut semis: i32 = match letter {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => return None,
    };
    match chars.peek() {
        Some('#') => {
            semis += 1;
            chars.next();
        }
        Some('b') => {
            semis -= 1;
            chars.next();
        }
        _ => {}
    }
    let rest: String = chars.collect();
    let (octave, cents) = match rest.find(['+', '-'].as_ref()).filter(|&i| i > 0) {
        Some(i) => (&rest[..i], rest[i..].trim().parse::<f32>().ok()?),
        None => (rest.as_str(), 0.0),
    };
    let octave: i32 = octave.trim().parse().ok()?;
    let midi = (octave + 1) * 12 + semis;
    Some(440.0 * 2f32.powf((midi as f32 - 69.0 + cents / 100.0) / 12.0))
}

/// "850 Hz", "1.20 kHz", "12.5 kHz".
pub fn hz_text(hz: f32) -> String {
    match hz {
        h if h < 1_000.0 => format!("{h:.0} Hz"),
        h if h < 10_000.0 => format!("{:.2} kHz", h / 1_000.0),
        h => format!("{:.1} kHz", h / 1_000.0),
    }
}

/// "24 dB/oct", "20.5 dB/oct", or "Brickwall".
pub fn slope_text(s: f32) -> String {
    match s {
        Shape::BRICKWALL => "Brickwall".into(),
        s if s.fract() == 0.0 => format!("{s:.0} dB/oct"),
        s => format!("{} dB/oct", (s * 10.0).round() / 10.0),
    }
}

/// A slope a step steeper or gentler (`by` steps): through its shape's slopes, or with `fine`
/// a dB an octave at a time, fractions between.
pub fn step_slope(b: &mut EqBand, by: i32, fine: bool) {
    let slopes = b.shape.slopes();
    if slopes.is_empty() || by == 0 {
        return;
    }
    if fine && !matches!(b.shape, Shape::Notch | Shape::AllPass) && b.slope < Shape::BRICKWALL {
        b.slope = b.shape.fit_slope((b.slope + by as f32).round());
        return;
    }
    // From a fraction, the next of its slopes that way.
    let at = match by > 0 {
        true => slopes
            .iter()
            .position(|&s| s > b.slope)
            .unwrap_or(slopes.len() - 1),
        false => slopes.iter().rposition(|&s| s < b.slope).unwrap_or(0),
    };
    let to = at as i32 + by - by.signum();
    b.slope = slopes[to.clamp(0, slopes.len() as i32 - 1) as usize];
}

pub(crate) fn q_text(q: f32) -> String {
    if q < 10.0 {
        format!("{q:.2}")
    } else {
        format!("{q:.1}")
    }
}

/// The note a frequency is nearest, and how many cents off it is.
pub(crate) fn note_of(hz: f32) -> (u8, f32) {
    let exact = 69.0 + 12.0 * (hz.max(1.0) / 440.0).log2();
    let note = exact.round().clamp(0.0, 127.0);
    (note as u8, (exact - note) * 100.0)
}

pub(crate) fn note_text(hz: f32) -> String {
    let (note, cents) = note_of(hz);
    let name = pina_engine::samples::note_name(note);
    match cents.round() as i32 {
        0 => name,
        c => format!("{name} {c:+}"),
    }
}

pub(crate) fn note_hz(note: u8) -> f32 {
    440.0 * 2f32.powf((note as f32 - 69.0) / 12.0)
}

/// Where a band's dot sits, in dB: a gain shape's at its gain as heard, a cut's on its curve at
/// its frequency, others' at 0.
pub(crate) fn dot_db(b: &EqBand, scale: f32) -> f32 {
    match b.shape {
        s if s.has_gain() => b.gain(scale),
        Shape::LowCut | Shape::HighCut => {
            let sections = band_sections(b, scale, 96_000.0);
            response_db(&sections, b.freq as f64, 96_000.0) as f32
        }
        _ => 0.0,
    }
}
