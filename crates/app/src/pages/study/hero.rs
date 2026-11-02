//! What's made of a study: the reference a song's mix aims at, and words for its numbers.

use super::*;

/// The study's mix, as a song's reference.
pub(crate) fn reference(s: &Study) -> Reference {
    let a = &s.mix.overall;
    let sp = &a.spectrum_db;
    Reference {
        file: s.file.clone(),
        lufs: a.lufs,
        crest_db: a.crest_db,
        spectrum: [sp.sub, sp.bass, sp.low_mid, sp.mid, sp.presence, sp.air],
        width_db: a.stereo.width_db,
        key: s.key.name.clone(),
    }
}

/// How steadily a song keeps its tempo, in a word.
pub(crate) fn steadiness(c: f32) -> &'static str {
    match c {
        c if c > 0.6 => "steady",
        c if c > 0.35 => "fairly steady",
        _ => "loose",
    }
}
