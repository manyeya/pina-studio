//! Notes written from nothing, over a span: a rhythm spread as evenly as it goes (Euclidean), a
//! melody grown from a seed in the key, and one following a shape.

use serde::{Deserialize, Serialize};

use super::Key;
use crate::song::project::Note;

/// Which of `steps` are hit, `pulses` of them spread as evenly as they go, turned `rotate`
/// steps on: 3 of 8 is the tresillo.
pub fn euclid(pulses: u8, steps: u8, rotate: u8) -> Vec<bool> {
    let (k, n) = (pulses.min(steps) as usize, steps.max(1) as usize);
    (0..n)
        .map(|i| ((i + rotate as usize) * k) % n < k)
        .collect()
}

/// A rhythm on one pitch: the pattern's hits a `grid` apart, over `span` ticks.
pub fn rhythm(
    (pulses, steps, rotate): (u8, u8, u8),
    grid: u64,
    span: u64,
    pitch: u8,
    velocity: u8,
) -> Vec<Note> {
    let hits = euclid(pulses, steps, rotate);
    let grid = grid.max(1);
    (0..span / grid)
        .filter(|&k| hits[k as usize % hits.len()])
        .map(|k| Note {
            start: (k * grid) as i64,
            length: grid,
            pitch,
            // The first of each pattern leans in.
            velocity: match (k as usize).is_multiple_of(hits.len()) {
                true => velocity,
                false => (velocity as u32 * 85 / 100) as u8,
            },
            ..Default::default()
        })
        .collect()
}

/// The scale's pitches from `low` to `high` (every pitch, with no key).
fn ladder(key: Option<Key>, low: u8, high: u8) -> Vec<u8> {
    let (low, high) = (low.min(high), high.max(low));
    let all: Vec<u8> = (low..=high)
        .filter(|&p| key.is_none_or(|k| k.contains(p)))
        .collect();
    match all.is_empty() {
        true => vec![low],
        false => all,
    }
}

/// A melody grown from `seed`: a note on `density` percent of the steps a `grid` apart, each a
/// small step or two up or down the scale from the last, now and then a leap.
pub fn seeded(
    key: Option<Key>,
    (low, high): (u8, u8),
    grid: u64,
    span: u64,
    density: u8,
    seed: u64,
) -> Vec<Note> {
    let ladder = ladder(key, low, high);
    let mut state = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407)
        | 1;
    let mut roll = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    let grid = grid.max(1);
    let mut at = ladder.len() / 2;
    let mut out: Vec<Note> = Vec::new();
    for k in 0..span / grid {
        if roll() * 100.0 >= density as f64 {
            continue;
        }
        let r = roll();
        let step: i64 = match r {
            r if r < 0.3 => 1,
            r if r < 0.6 => -1,
            r if r < 0.75 => 2,
            r if r < 0.9 => -2,
            r if r < 0.95 => 4,
            _ => 0,
        };
        at = (at as i64 + step).clamp(0, ladder.len() as i64 - 1) as usize;
        // Held until the next note, at most a beat's worth of steps.
        if let Some(last) = out.last_mut() {
            last.length = ((k * grid) as i64 - last.start).min(4 * grid as i64) as u64;
        }
        out.push(Note {
            start: (k * grid) as i64,
            length: grid,
            pitch: ladder[at],
            velocity: 80 + (roll() * 30.0) as u8,
            ..Default::default()
        });
    }
    out
}

/// A melody's shape over its span.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Contour {
    Rise,
    Fall,
    Arch,
    Valley,
    Wave,
}

impl Contour {
    pub const ALL: [Contour; 5] = [
        Contour::Rise,
        Contour::Fall,
        Contour::Arch,
        Contour::Valley,
        Contour::Wave,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Contour::Rise => "Rise",
            Contour::Fall => "Fall",
            Contour::Arch => "Arch",
            Contour::Valley => "Valley",
            Contour::Wave => "Wave",
        }
    }

    /// How high it is `t` of the way through, 0 to 1.
    fn at(self, t: f64) -> f64 {
        use std::f64::consts::PI;
        match self {
            Contour::Rise => t,
            Contour::Fall => 1.0 - t,
            Contour::Arch => (t * PI).sin(),
            Contour::Valley => 1.0 - (t * PI).sin(),
            Contour::Wave => 0.5 + 0.5 * (t * 2.0 * PI).sin(),
        }
    }
}

/// A note a `grid` apart over `span`, each as high on the scale as the shape is there.
pub fn shaped(
    contour: Contour,
    key: Option<Key>,
    (low, high): (u8, u8),
    grid: u64,
    span: u64,
) -> Vec<Note> {
    let ladder = ladder(key, low, high);
    let grid = grid.max(1);
    let n = (span / grid).max(1);
    (0..n)
        .map(|k| {
            let t = match n {
                1 => 0.0,
                n => k as f64 / (n - 1) as f64,
            };
            let at = (contour.at(t) * (ladder.len() - 1) as f64).round() as usize;
            Note {
                start: (k * grid) as i64,
                length: grid,
                pitch: ladder[at],
                velocity: 96,
                ..Default::default()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song::project::PPQ;

    #[test]
    fn euclidean_rhythms_spread_their_hits_evenly() {
        let x = |s: &str| s.chars().map(|c| c == 'x').collect::<Vec<_>>();
        assert_eq!(euclid(3, 8, 0), x("x..x..x."));
        assert_eq!(euclid(4, 16, 0), x("x...x...x...x..."));
        assert_eq!(euclid(3, 8, 1).iter().filter(|&&h| h).count(), 3);
        let hats = rhythm((3, 8, 0), PPQ / 4, PPQ * 4, 42, 100);
        let at: Vec<i64> = hats.iter().map(|n| n.start).collect();
        let s = (PPQ / 4) as i64;
        assert_eq!(at, [0, 3 * s, 6 * s, 8 * s, 11 * s, 14 * s]);
        assert!(hats[1].velocity < hats[0].velocity);
    }

    #[test]
    fn a_seeded_melody_stays_in_key_and_range_and_repeats_for_its_seed() {
        let key = Key {
            root: 0,
            minor: false,
        };
        let c = Some(key);
        let grow = |seed| seeded(c, (60, 72), PPQ / 2, PPQ * 16, 70, seed);
        let a = grow(3);
        assert_eq!(a, grow(3));
        assert_ne!(a, grow(4));
        assert!(a.len() > 10);
        assert!(
            a.iter()
                .all(|n| (60..=72).contains(&n.pitch) && key.contains(n.pitch))
        );
        // Mostly steps, not leaps.
        let leaps = a
            .windows(2)
            .filter(|w| w[0].pitch.abs_diff(w[1].pitch) > 5)
            .count();
        assert!(leaps * 4 < a.len(), "{leaps} of {}", a.len());
    }

    #[test]
    fn a_shaped_melody_follows_its_contour() {
        let up = shaped(Contour::Rise, None, (60, 67), PPQ, PPQ * 8);
        let pitches: Vec<u8> = up.iter().map(|n| n.pitch).collect();
        assert_eq!(pitches, [60, 61, 62, 63, 64, 65, 66, 67]);
        let arch = shaped(Contour::Arch, None, (60, 72), PPQ, PPQ * 9);
        let top = arch.iter().map(|n| n.pitch).max().unwrap();
        assert_eq!(arch[4].pitch, top);
        assert!(arch[0].pitch < top && arch[8].pitch < top);
    }
}
