//! A map of MIDI files: near ones play alike, whatever key they're in. It's how they play that
//! places them: where their notes land in the bar, how busy, long, wide and thick they are, the
//! part they're for and whether they're minor.

use super::*;
use crate::samples::search::layout;

/// The parts, as `ROLE_PLACES` numbers them on a file's profile.
const PARTS: [&str; 9] = [
    "drums", "bass", "chords", "pad", "pluck", "keys", "lead", "arp", "fx",
];

/// A file as numbers that stand for how it plays, each about 0 to 1.
fn profile(m: &MidiSample) -> Vec<f32> {
    let s = &m.study;
    let parts = &s.parts;
    let notes = parts.iter().map(|p| p.notes.max(1)).sum::<usize>().max(1) as f32;
    let mean = |f: fn(&PartStudy) -> f32| {
        let of = |p: &PartStudy| f(p) * p.notes.max(1) as f32;
        parts.iter().map(of).sum::<f32>() / notes
    };
    let squash = |x: f32, top: f32| (1.0 + x.max(0.0)).ln() / (1.0 + top).ln();
    let mut v = vec![0.0; 16];
    for p in parts {
        for (to, step) in v.iter_mut().zip(&p.steps) {
            *to += step * p.notes.max(1) as f32 / notes;
        }
    }
    // The part it's for counts most: basses gather with basses.
    v.extend(PARTS.map(|r| {
        let plays = parts.iter().any(|p| p.role == r);
        1.5 * (s.role == r) as u8 as f32 + 0.5 * plays as u8 as f32
    }));
    v.extend([
        squash(mean(|p| p.density), 32.0),
        squash(mean(|p| p.length), 16.0),
        mean(|p| p.syncopation),
        (mean(|p| p.swing) - 50.0).max(0.0) / 25.0,
        mean(|p| p.legato),
        squash(mean(|p| p.polyphony), 6.0),
        mean(|p| p.velocity[2] as f32) / 127.0,
        mean(|p| p.contour.as_ref().map_or(0.0, |c| c.leaps)),
        mean(|p| p.contour.as_ref().map_or(0.0, |c| c.span as f32 / 36.0)),
        s.key
            .as_deref()
            .and_then(Key::parse)
            .map_or(0.0, |k| k.minor as u8 as f32 * 0.5),
    ]);
    v
}

/// Every file's place on a map, -1 to 1 each way. Too few to lay out: round a circle by part.
pub fn places(midi: &[MidiSample]) -> Vec<[f32; 2]> {
    if midi.len() < 4 {
        let around = |(i, m): (usize, &MidiSample)| {
            let part = PARTS.iter().position(|r| *r == m.study.role).unwrap_or(i);
            let angle = part as f32 / PARTS.len() as f32 * std::f32::consts::TAU;
            [angle.cos() * 0.6, angle.sin() * 0.6]
        };
        return midi.iter().enumerate().map(around).collect();
    }
    let profiles: Vec<Vec<f32>> = midi.iter().map(profile).collect();
    let vectors: Vec<&[f32]> = profiles.iter().map(|p| &p[..]).collect();
    layout(&vectors)
}
