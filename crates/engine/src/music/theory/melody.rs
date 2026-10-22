//! Melodies from motifs, in a style, over a song's chords.

use super::*;

/// How a melody moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MelodyStyle {
    /// A hook: a motif, said again changed, answered, and resolved; chord tones where it lands.
    #[default]
    Hook,
    /// A one-bar riff, its notes following each chord.
    Riff,
    /// Up and down each chord in sixteenths.
    Arp,
    /// Long notes moving by the chords' thirds and sevenths: a counter-line or a pad's top.
    Counter,
}

/// Rhythms a melody's bar can take: (start, length) in sixteenths.
pub(crate) const MOTIFS: &[&[(u8, u8)]] = &[
    &[(0, 3), (3, 1), (4, 2), (6, 2), (8, 4)],
    &[(0, 2), (2, 2), (4, 3), (7, 1), (8, 6)],
    &[(2, 2), (4, 2), (6, 4), (10, 2), (12, 4)],
    &[(0, 1), (1, 1), (2, 2), (4, 4), (10, 2), (12, 2)],
    &[(0, 6), (6, 2), (8, 2), (10, 6)],
    &[(0, 3), (3, 3), (6, 2), (8, 3), (11, 5)],
    &[(4, 2), (6, 2), (8, 2), (10, 2), (12, 4)],
    &[(0, 2), (3, 2), (6, 2), (8, 8)],
];

/// A melody over the chords. `density` (0 to 1) thins its notes; `register` is its range.
#[allow(clippy::too_many_arguments)]
pub fn melody(
    spans: &[Span],
    key: &Key,
    style: MelodyStyle,
    register: (u8, u8),
    density: f32,
    beat: u64,
    origin: u64,
    bars: u64,
    seed: u64,
) -> Vec<Note> {
    let mut rng = Rng::new(seed);
    let (lo, hi) = register;
    let sixteenth = beat / 4;
    let bar = beat * 4;
    let chord_at = |t: u64| {
        spans
            .iter()
            .find(|s| s.start - origin <= t && t < s.end - origin)
    };
    let scale: Vec<u8> = (0..12u8).filter(|&p| key.contains(p)).collect();
    let mut out = Vec::new();
    let mut last = (lo as i32 + hi as i32) / 2;
    match style {
        MelodyStyle::Arp => {
            let mut t = 0;
            let mut k = 0usize;
            while t < bars * bar {
                if let Some(s) = chord_at(t) {
                    let mut tones: Vec<u8> = s
                        .chord
                        .classes()
                        .iter()
                        .map(|&pc| nearest(pc, (lo as i32 + hi as i32) / 2, lo, hi))
                        .collect();
                    tones.sort_unstable();
                    let up_down: Vec<u8> = tones
                        .iter()
                        .chain(
                            tones
                                .iter()
                                .rev()
                                .skip(1)
                                .take(tones.len().saturating_sub(2)),
                        )
                        .copied()
                        .collect();
                    let p = up_down[k % up_down.len()];
                    if rng.unit() <= density.max(0.3) {
                        out.push(note(
                            t,
                            sixteenth - 10,
                            p,
                            if k.is_multiple_of(4) { 96 } else { 76 },
                        ));
                    }
                }
                t += sixteenth;
                k += 1;
            }
        }
        MelodyStyle::Counter => {
            for s in spans {
                let guide = s.chord.guide_tones();
                let pick = guide
                    .iter()
                    .map(|&pc| nearest(pc, last, lo, hi))
                    .min_by_key(|&p| (p as i32 - last).abs())
                    .unwrap_or_else(|| nearest(s.chord.root, last, lo, hi));
                last = pick as i32;
                out.push(note(s.start - origin, s.end - s.start - 30, pick, 84));
            }
        }
        MelodyStyle::Hook | MelodyStyle::Riff => {
            // The motif: a bar's rhythm, and the steps it takes (in scale steps from where it
            // lands).
            let motif = MOTIFS[rng.below(MOTIFS.len())];
            let steps: Vec<i32> = motif.iter().map(|_| rng.below(5) as i32 - 2).collect();
            let answer = MOTIFS[rng.below(MOTIFS.len())];
            let to_scale = |p: i32| -> i32 {
                // The scale's pitch at or under p.
                (0..12)
                    .map(|d| p - d)
                    .find(|&q| scale.contains(&(q.rem_euclid(12) as u8)))
                    .unwrap_or(p)
            };
            let step_from = |p: i32, n: i32| -> i32 {
                let mut q = to_scale(p);
                let dir = n.signum();
                for _ in 0..n.abs() {
                    q += dir;
                    while !scale.contains(&(q.rem_euclid(12) as u8)) {
                        q += dir;
                    }
                }
                q
            };
            for b in 0..bars {
                let b0 = b * bar;
                // Phrases of four bars: A, A changed, A, an answer that resolves.
                let phrase = b % 4;
                let rhythm = match (style, phrase) {
                    (MelodyStyle::Hook, 3) => answer,
                    _ => motif,
                };
                for (k, &(at, len)) in rhythm.iter().enumerate() {
                    if style == MelodyStyle::Hook
                        && phrase != 3
                        && rng.unit() > density.clamp(0.2, 1.0) + 0.25
                    {
                        continue;
                    }
                    let t = b0 + at as u64 * sixteenth;
                    let Some(s) = chord_at(t) else { continue };
                    let strong = at % 4 == 0;
                    let pitch = if strong || k == 0 {
                        // On the beat: the chord's tone nearest the line.
                        let target = step_from(
                            last,
                            steps[k % steps.len()] + if phrase == 1 { 1 } else { 0 },
                        );
                        s.chord
                            .classes()
                            .iter()
                            .map(|&pc| nearest(pc, target, lo, hi))
                            .min_by_key(|&p| (p as i32 - target).abs())
                            .unwrap_or(target as u8) as i32
                    } else {
                        // Between: a step of the scale.
                        step_from(last, steps[k % steps.len()].signum()).clamp(lo as i32, hi as i32)
                    };
                    // A phrase ends on the chord's root or third, held.
                    let last_note = phrase == 3 && k + 1 == rhythm.len();
                    let pitch = if last_note {
                        let resolve = [
                            s.chord.root,
                            (s.chord.root + if s.chord.quality.minor() { 3 } else { 4 }) % 12,
                        ];
                        resolve
                            .iter()
                            .map(|&pc| nearest(pc, pitch, lo, hi))
                            .min_by_key(|&p| (p as i32 - pitch).abs())
                            .unwrap() as i32
                    } else {
                        pitch
                    };
                    let pitch = pitch.clamp(lo as i32, hi as i32) as u8;
                    last = pitch as i32;
                    let vel = if strong { 98 } else { 82 } + rng.below(7) as u8;
                    out.push(note(t, len as u64 * sixteenth - 20, pitch, vel));
                }
            }
        }
    }
    out
}
