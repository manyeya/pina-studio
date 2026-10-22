//! Chords played in a rhythm.

use super::*;

/// How chords are played in time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChordRhythm {
    /// Held for as long as each lasts.
    #[default]
    Held,
    /// Held, each coming an eighth early: pushed, as in neo-soul and pop.
    Push,
    /// Short hits on the beat and the and of two.
    Stabs,
    /// A player comping: on one, the and of two, and a short one on four.
    Comp,
    /// Eighths, as a pop or trance piano pulses.
    Pulse,
    /// On the offbeats, as house stabs are.
    Offbeat,
    /// Up its notes in sixteenths.
    Arp,
}

/// A note at a tick, as a region wants it.
pub(crate) fn note(start: u64, length: u64, pitch: u8, velocity: u8) -> Note {
    Note {
        start: start as i64,
        length: length.max(1),
        pitch,
        velocity: velocity.clamp(1, 127),
    }
}

/// Chords played: each voiced from the one before, in a rhythm, humanized a little by `seed`.
/// Ticks are from `origin` (the region's start).
pub fn play_chords(
    spans: &[Span],
    voicing: Voicing,
    rhythm: ChordRhythm,
    register: (u8, u8),
    beat: u64,
    origin: u64,
    seed: u64,
) -> Vec<Note> {
    let mut rng = Rng::new(seed);
    let mut prev: Option<Vec<u8>> = None;
    let mut out = Vec::new();
    let eighth = beat / 2;
    let sixteenth = beat / 4;
    for (i, s) in spans.iter().enumerate() {
        let v = voice(&s.chord, prev.as_deref(), voicing, register);
        prev = Some(v.clone());
        let (start, end) = (s.start - origin, s.end - origin);
        // A player leans on some chords more than others.
        let vel =
            |rng: &mut Rng, base: u8| base.saturating_add(rng.below(15) as u8).saturating_sub(7);
        let hit = |out: &mut Vec<Note>, at: u64, len: u64, base: u8, rng: &mut Rng| {
            for (k, &p) in v.iter().enumerate() {
                // A player's hand spreads a chord a hair, low to high.
                let roll = (k as u64) * 4;
                out.push(note(
                    at + roll,
                    len.saturating_sub(roll + 10),
                    p,
                    vel(rng, base),
                ));
            }
        };
        match rhythm {
            ChordRhythm::Held => hit(&mut out, start, end - start, 92, &mut rng),
            ChordRhythm::Push => {
                let at = if i == 0 || start < eighth {
                    start
                } else {
                    start - eighth
                };
                let to = if i + 1 < spans.len() {
                    end - eighth
                } else {
                    end
                };
                hit(&mut out, at, to - at, 92, &mut rng)
            }
            ChordRhythm::Stabs | ChordRhythm::Comp | ChordRhythm::Pulse | ChordRhythm::Offbeat => {
                // Hits within each beat-span of the chord, as (offset in beats, length, velocity).
                let pattern: &[(f32, f32, u8)] = match rhythm {
                    ChordRhythm::Stabs => &[(0.0, 0.4, 100), (1.5, 0.4, 84)],
                    ChordRhythm::Comp => &[(0.0, 1.35, 96), (1.5, 0.45, 74), (3.0, 0.4, 66)],
                    ChordRhythm::Pulse => &[
                        (0.0, 0.45, 100),
                        (0.5, 0.45, 72),
                        (1.0, 0.45, 88),
                        (1.5, 0.45, 72),
                        (2.0, 0.45, 96),
                        (2.5, 0.45, 72),
                        (3.0, 0.45, 88),
                        (3.5, 0.45, 72),
                    ],
                    _ => &[
                        (0.5, 0.35, 96),
                        (1.5, 0.35, 88),
                        (2.5, 0.35, 96),
                        (3.5, 0.35, 88),
                    ],
                };
                let bar = beat * 4;
                let mut b0 = start;
                while b0 < end {
                    for &(at, len, base) in pattern {
                        let t = b0 + (at * beat as f32) as u64;
                        if t >= start && t < end {
                            let l = ((len * beat as f32) as u64).min(end - t);
                            hit(&mut out, t, l, base, &mut rng);
                        }
                    }
                    b0 += bar;
                }
            }
            ChordRhythm::Arp => {
                let mut ups: Vec<u8> = v.clone();
                ups.sort_unstable();
                let mut t = start;
                let mut k = 0;
                while t < end {
                    let p = ups[k % ups.len()];
                    out.push(note(
                        t,
                        sixteenth - 10,
                        p,
                        vel(&mut rng, if k % 4 == 0 { 96 } else { 78 }),
                    ));
                    t += sixteenth;
                    k += 1;
                }
            }
        }
    }
    out
}
