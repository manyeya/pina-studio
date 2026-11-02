//! A recording's timing, as an audio region needs it to keep time with the song: its tempo,
//! where its beats and downbeats are, and whether it's a loop. A loop (as the sample library
//! hears one: short, a whole number of bars) is even, its beats on a steady grid from its start;
//! anything longer is followed beat by beat, as a played take drifts.

use crate::{
    samples::{Kind, analyze},
    song::project::Grid,
};

use super::{beats, features};

/// What listening found.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    /// Quarter notes a minute, on average.
    pub tempo: f64,
    /// Its beats, in frames at the project's rate.
    pub grid: Grid,
    pub looped: bool,
}

/// The timing of audio (`channels` at `rate`), its frames counted at `project_rate`; `hint` is
/// its name (a tempo in it is taken). None for a one-shot, or anything without a beat to follow.
pub fn find(channels: &[Vec<f32>], rate: f64, project_rate: f64, hint: &str) -> Option<Found> {
    let n = channels.first()?.len();
    if n == 0 {
        return None;
    }
    let seconds = n as f64 / rate;
    let a = analyze::analyze(channels, rate, hint);
    if a.kind == Kind::Loop
        && let Some(bpm) = a.bpm.map(|b| b as f64).filter(|b| *b > 0.0)
    {
        let beat = 60.0 / bpm * project_rate;
        let count = (seconds * bpm / 60.0).round().max(1.0) as u64;
        return Some(Found {
            tempo: bpm,
            grid: Grid {
                beats: (0..count)
                    .map(|k| (k as f64 * beat).round() as u64)
                    .collect(),
                meter: 4,
                downbeat: 0,
            },
            looped: true,
        });
    }
    // Followed beat by beat: a take, a song.
    if seconds < 4.0 {
        return None;
    }
    let mono: Vec<f32> = (0..n)
        .map(|i| channels.iter().map(|c| c[i]).sum::<f32>() / channels.len() as f32)
        .collect();
    let f = features::frames(&mono, rate as f32);
    let b = beats::beats(&f);
    if b.beats.len() < 4 || b.confidence < 0.15 {
        return None;
    }
    let grid = Grid {
        beats: b
            .beats
            .iter()
            .map(|&t| (t as f64 * project_rate).round().max(0.0) as u64)
            .collect(),
        meter: b.per_bar.clamp(1, 16) as u32,
        downbeat: b.first as u32,
    };
    Some(Found {
        tempo: grid.tempo(project_rate).unwrap_or(b.bpm as f64),
        grid,
        looped: false,
    })
}
