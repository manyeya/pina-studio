//! What a sample is, from its audio, and its name for hints.
// ponytail: a first pass (length, level, outline, a category from the name); the real analysis
// replaces it.

use std::path::Path;

use super::{Analysis, Category, EMBEDDING, Kind, WAVEFORM};

/// Analyses a sample file. Its folder and name give hints ("Kick_03", "Loops/120bpm").
pub fn analyze_file(path: &Path) -> Result<Analysis, String> {
    let data = crate::engine::read_audio(path)?;
    let hint = path.to_string_lossy().to_lowercase();
    Ok(analyze(&data.0, data.1, &hint))
}

/// Analyses audio: `channels` at `rate`, with `hint` (its path, lower-cased) for naming hints.
pub fn analyze(channels: &[Vec<f32>], rate: f64, hint: &str) -> Analysis {
    let len = channels.first().map_or(0, Vec::len);
    let seconds = len as f32 / rate as f32;
    let peak = channels.iter().flatten().fold(0f32, |m, s| m.max(s.abs()));
    let waveform = (0..WAVEFORM)
        .map(|i| {
            let (a, b) = (
                i * len / WAVEFORM,
                ((i + 1) * len / WAVEFORM).max(i * len / WAVEFORM + 1),
            );
            let p = channels
                .iter()
                .flat_map(|c| c.get(a..b.min(c.len())).unwrap_or(&[]))
                .fold(0f32, |m, s| m.max(s.abs()));
            (p / peak.max(1e-9) * 255.0) as u8
        })
        .collect();
    let word = |w: &str| hint.contains(w);
    let category = if word("kick") || word("bd") {
        Category::Kick
    } else if word("snare") {
        Category::Snare
    } else if word("clap") {
        Category::Clap
    } else if word("hat") {
        Category::HatClosed
    } else if word("808") || word("sub") {
        Category::Bass808
    } else if word("bass") {
        Category::Bass
    } else {
        Category::Other
    };
    Analysis {
        seconds,
        sample_rate: rate as u32,
        channels: channels.len() as u16,
        kind: if word("loop") || seconds > 4.0 {
            Kind::Loop
        } else {
            Kind::OneShot
        },
        category,
        confidence: 0.3,
        peak_db: 20.0 * peak.max(1e-9).log10(),
        loudness: -70.0,
        embedding: vec![0.0; EMBEDDING],
        waveform,
        ..Default::default()
    }
}
