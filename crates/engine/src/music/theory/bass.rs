//! Basslines, in a style, on a song's chords.

use super::*;

/// How a bass plays the chords.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BassStyle {
    /// Each chord's root, held.
    #[default]
    Roots,
    /// Root on one, fifth on three.
    RootFifth,
    /// Quarter notes walking to each next root.
    Walking,
    /// Eighths jumping octaves, disco and house.
    Octaves,
    /// On the offbeats, as house bass is.
    Offbeat,
    /// Funk and neo-soul: on one, the and of two, three, and a half step into the next root.
    Syncopated,
    /// On the kick, its root held to the next: how a trap 808 plays.
    Kick,
}

/// The pitch for `pc` nearest `near`, within `lo..=hi`.
pub(crate) fn nearest(pc: u8, near: i32, lo: u8, hi: u8) -> u8 {
    let mut best = None;
    for octave in 0..11 {
        let p = octave * 12 + pc as i32;
        if p < lo as i32 || p > hi as i32 {
            continue;
        }
        if best.is_none_or(|b: i32| (p - near).abs() < (b - near).abs()) {
            best = Some(p);
        }
    }
    best.unwrap_or((lo as i32 / 12) * 12 + pc as i32) as u8
}

/// A bassline for the chords. `kicks` (ticks from `origin`) are where the kick plays, for the
/// Kick style. Ticks are from `origin`.
#[allow(clippy::too_many_arguments)]
pub fn bassline(
    spans: &[Span],
    style: BassStyle,
    register: (u8, u8),
    beat: u64,
    origin: u64,
    kicks: &[(u64, u8)],
    key: &Key,
    seed: u64,
) -> Vec<Note> {
    let mut rng = Rng::new(seed);
    let (lo, hi) = register;
    let mut last = (lo as i32 + hi as i32) / 2 - 4;
    let mut out = Vec::new();
    let bar = beat * 4;
    let chord_at = |t: u64| {
        spans
            .iter()
            .find(|s| s.start - origin <= t && t < s.end - origin)
    };
    let push = |out: &mut Vec<Note>, last: &mut i32, t: u64, len: u64, pc: u8, vel: u8| {
        let p = nearest(pc, *last, lo, hi);
        *last = p as i32;
        out.push(note(t, len, p, vel));
    };
    match style {
        BassStyle::Kick => {
            for (i, &(t, v)) in kicks.iter().enumerate() {
                let Some(s) = chord_at(t) else { continue };
                let next = kicks.get(i + 1).map_or(s.end - origin, |k| k.0);
                let len = (next - t).min(bar).saturating_sub(20);
                let root = s.chord.bass.unwrap_or(s.chord.root);
                push(&mut out, &mut last, t, len, root, v.max(80));
            }
        }
        _ => {
            for (i, s) in spans.iter().enumerate() {
                let (start, end) = (s.start - origin, s.end - origin);
                let root = s.chord.bass.unwrap_or(s.chord.root);
                let fifth = (s.chord.root + 7) % 12;
                let third = (s.chord.root + if s.chord.quality.minor() { 3 } else { 4 }) % 12;
                let next_root = spans
                    .get(i + 1)
                    .map_or(root, |n| n.chord.bass.unwrap_or(n.chord.root));
                // The note under the next root, a half step: up into it or down.
                let approach = if rng.chance(0.5) {
                    (next_root + 11) % 12
                } else {
                    (next_root + 1) % 12
                };
                let v = |rng: &mut Rng, base: u8| {
                    base.saturating_add(rng.below(7) as u8).saturating_sub(3)
                };
                match style {
                    BassStyle::Roots => push(
                        &mut out,
                        &mut last,
                        start,
                        end - start - 20,
                        root,
                        v(&mut rng, 100),
                    ),
                    BassStyle::RootFifth
                    | BassStyle::Walking
                    | BassStyle::Syncopated
                    | BassStyle::Octaves
                    | BassStyle::Offbeat => {
                        // Bar by bar within the chord (a chord of half a bar gets its half).
                        let mut b0 = start;
                        while b0 < end {
                            let within = |t: u64| t >= start && t < end;
                            let last_bar = b0 + bar >= end;
                            let pattern: Vec<(f32, f32, u8, u8)> = match style {
                                BassStyle::RootFifth => {
                                    vec![(0.0, 1.8, root, 100), (2.0, 1.8, fifth, 88)]
                                }
                                BassStyle::Walking => {
                                    let tone = if rng.chance(0.5) { third } else { fifth };
                                    let step = if key.contains(fifth + 2) {
                                        (fifth + 2) % 12
                                    } else {
                                        fifth
                                    };
                                    vec![
                                        (0.0, 0.92, root, 100),
                                        (1.0, 0.92, tone, 84),
                                        (2.0, 0.92, step, 88),
                                        (3.0, 0.92, if last_bar { approach } else { fifth }, 80),
                                    ]
                                }
                                BassStyle::Syncopated => {
                                    let mut p = vec![
                                        (0.0, 1.35, root, 104),
                                        (1.5, 0.4, if rng.chance(0.5) { third } else { fifth }, 84),
                                        (2.0, 0.9, fifth, 92),
                                    ];
                                    p.push((3.5, 0.4, if last_bar { approach } else { root }, 80));
                                    p
                                }
                                BassStyle::Octaves => (0..8)
                                    .map(|k| {
                                        (
                                            k as f32 * 0.5,
                                            0.42,
                                            root,
                                            if k % 2 == 0 { 100 } else { 86 },
                                        )
                                    })
                                    .collect(),
                                _ => (0..4).map(|k| (k as f32 + 0.5, 0.42, root, 96)).collect(),
                            };
                            for (at, len, pc, base) in pattern {
                                let t = b0 + (at * beat as f32) as u64;
                                if within(t) {
                                    let l = ((len * beat as f32) as u64).min(end - t);
                                    let octave_up = matches!(style, BassStyle::Octaves)
                                        && (at * 2.0) as u32 % 2 == 1;
                                    let before = last;
                                    push(&mut out, &mut last, t, l, pc, v(&mut rng, base));
                                    if octave_up && let Some(n) = out.last_mut() {
                                        n.pitch = (n.pitch + 12).min(hi);
                                        last = before;
                                    }
                                }
                            }
                            b0 += bar;
                        }
                    }
                    BassStyle::Kick => unreachable!(),
                }
            }
        }
    }
    out
}
