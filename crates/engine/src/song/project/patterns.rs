//! Step patterns: rows of steps, each step's own settings, and the notes they play.

use super::*;

pub(crate) fn sixteenth() -> u64 {
    PPQ / 4
}

pub(crate) fn straight() -> f32 {
    50.0
}

pub(crate) fn is_straight(v: &f32) -> bool {
    *v == 50.0
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct PatternRow {
    pub pitch: u8,
    /// A velocity per step; 0 is a rest. A row shorter than the others loops on its own length
    /// (a polymeter: three steps against sixteen).
    pub cells: Vec<u8>,
    /// Silent, its steps kept.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mute: bool,
    /// Each step's own settings (parameter locks): the
    /// chance it plays (0 to 100), semitones up or down, how many times it hits in its step (a
    /// ratchet, 1 to 4), how long it holds (% of a step) and how far off the grid it is (% of a
    /// step, -50 to 50). Missing steps (or an empty lane) are as usual: 100, 0, 1, 75, 0.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chance: Vec<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transpose: Vec<i8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repeat: Vec<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gate: Vec<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nudge: Vec<i8>,
}

impl PatternRow {
    pub fn chance_at(&self, i: usize) -> u8 {
        self.chance.get(i).copied().unwrap_or(CHANCE).min(100)
    }
    pub fn transpose_at(&self, i: usize) -> i8 {
        self.transpose.get(i).copied().unwrap_or(0)
    }
    pub fn repeat_at(&self, i: usize) -> u8 {
        self.repeat.get(i).copied().unwrap_or(1).clamp(1, 4)
    }
    pub fn gate_at(&self, i: usize) -> u8 {
        self.gate.get(i).copied().unwrap_or(GATE).clamp(5, 200)
    }
    pub fn nudge_at(&self, i: usize) -> i8 {
        self.nudge.get(i).copied().unwrap_or(0).clamp(-50, 50)
    }

    /// Every lane as long as its steps (filled as usual), to edit together.
    pub fn fill_lanes(&mut self) {
        let n = self.cells.len();
        let fill = |v: &mut Vec<u8>, d: u8| v.resize(n, d);
        fill(&mut self.chance, CHANCE);
        fill(&mut self.repeat, 1);
        fill(&mut self.gate, GATE);
        self.transpose.resize(n, 0);
        self.nudge.resize(n, 0);
    }

    /// Lanes as usual throughout taken away, so the song keeps only what's set.
    pub fn tidy_lanes(&mut self) {
        let n = self.cells.len();
        let all = |v: &[u8], d: u8| v.iter().take(n).all(|&x| x == d);
        if all(&self.chance, CHANCE) {
            self.chance.clear();
        }
        if all(&self.repeat, 1) {
            self.repeat.clear();
        }
        if all(&self.gate, GATE) {
            self.gate.clear();
        }
        if self.transpose.iter().all(|&x| x == 0) {
            self.transpose.clear();
        }
        if self.nudge.iter().all(|&x| x == 0) {
            self.nudge.clear();
        }
    }
}

/// A MIDI region's notes as a pattern's rows: a row a pitch (lowest last, as drums read: kick at
/// the bottom), each note on the step nearest its start, `steps` long, at its velocity.
pub fn to_pattern(notes: &[Note], step: u64, steps: usize) -> Vec<PatternRow> {
    let mut pitches: Vec<u8> = notes.iter().map(|n| n.pitch).collect();
    pitches.sort_unstable_by(|a, b| b.cmp(a));
    pitches.dedup();
    pitches
        .into_iter()
        .map(|pitch| {
            let mut cells = vec![0u8; steps];
            for n in notes.iter().filter(|n| n.pitch == pitch && n.start >= 0) {
                let k = ((n.start as f64 / step.max(1) as f64).round() as usize) % steps.max(1);
                cells[k] = cells[k].max(n.velocity.max(1));
            }
            PatternRow {
                pitch,
                cells,
                mute: false,
                ..Default::default()
            }
        })
        .collect()
}

/// The notes a pattern plays over `length` ticks, each row's steps repeating, every second step
/// late by its swing.
pub fn pattern_notes(step: u64, rows: &[PatternRow], length: u64, swing: f32) -> Vec<Note> {
    pattern_played(step, rows, length, swing, 0, &[], 120.0)
}

/// A pattern's notes as they play from `start` (ticks into the song): each step's lanes (its
/// chance, transpose, ratchet, gate and nudge), and what the track's modulators do to each sound
/// (its velocity, pitch, chance), worked out where each step falls in the song. A step's chance
/// is rolled afresh each time round, the same each time the song plays.
pub fn pattern_played(
    step: u64,
    rows: &[PatternRow],
    length: u64,
    swing: f32,
    start: u64,
    mods: &[crate::dsp::modulate::Modulator],
    tempo: f64,
) -> Vec<Note> {
    use crate::dsp::modulate::{ModTarget, SoundParam};
    // Swing is where in a pair of steps the second lands: at 50% exactly between.
    let late = ((swing.clamp(50.0, 75.0) - 50.0) / 50.0 * step as f32) as i64;
    let mut notes = Vec::new();
    if step == 0 {
        return notes;
    }
    let first = (start / step) as i64;
    for k in 0..length.div_ceil(step) {
        for (r, row) in rows.iter().enumerate() {
            if row.mute || row.cells.is_empty() {
                continue;
            }
            let i = (k % row.cells.len() as u64) as usize;
            let velocity = row.cells[i];
            if velocity == 0 {
                continue;
            }
            // What the modulators on this sound make of this step.
            let (mut dv, mut dp, mut dc) = (0f32, 0f32, 0f32);
            let beats = (start + k * step) as f64 / PPQ as f64;
            for m in mods {
                for route in &m.routes {
                    if let ModTarget::Sound(on, param) = route.target
                        && on == r
                        && let Some(v) = crate::dsp::modulate::value_at(&m.kind, beats, tempo)
                    {
                        let by = route.depth * v;
                        match param {
                            SoundParam::Velocity => dv += by,
                            SoundParam::Pitch => dp += by,
                            SoundParam::Chance => dc += by,
                        }
                    }
                }
            }
            let chance = (row.chance_at(i) as f32 + dc).clamp(0.0, 100.0);
            if chance < 100.0 && crate::dsp::modulate::roll(first + k as i64, r) >= chance {
                continue;
            }
            let velocity = (velocity as f32 + dv).round().clamp(1.0, 127.0) as u8;
            let pitch = (row.pitch as i32 + row.transpose_at(i) as i32 + dp.round() as i32)
                .clamp(0, 127) as u8;
            let repeat = row.repeat_at(i) as u64;
            let sub = (step / repeat).max(1);
            let held = ((sub as f32 * row.gate_at(i) as f32 / 100.0).round() as u64).max(1);
            let swung = if k % 2 == 1 { late } else { 0 };
            let nudge = (step as f32 * row.nudge_at(i) as f32 / 100.0).round() as i64;
            for j in 0..repeat {
                notes.push(Note {
                    start: (k * step + j * sub) as i64 + swung + nudge,
                    length: held,
                    pitch,
                    velocity,
                });
            }
        }
    }
    notes.sort_by_key(|n| n.start);
    notes
}
