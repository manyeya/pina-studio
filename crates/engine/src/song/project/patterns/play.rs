//! How a pattern plays: each row through its loop, in its order and at its rate, skipping the
//! steps it skips; the notes its steps play (ties, repeats, gates, offsets, chance and conditions,
//! swing) and the automation its automation rows set.

use super::*;
use crate::dsp::modulate::{ModTarget, Modulator, SoundParam};

/// A step as a row plays it: which, when (ticks from the pattern's start), for how long, and on
/// which pass through the row's loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub step: usize,
    pub at: u64,
    pub length: u64,
    pub pass: usize,
}

/// A number from two, the same each time (splitmix64).
fn hash(a: u64, b: u64) -> u64 {
    let mut x = a ^ b.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// `0..n` in an order of its own for `seed`.
fn shuffled(n: usize, seed: u64) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = (hash(seed, i as u64) % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
    v
}

/// The steps row `r` plays over `length` ticks, in order. Its random ways (wander, shuffle,
/// random) go by `seed`, so the same song plays the same each time.
pub fn row_hits(p: &Pattern, r: usize, length: u64, seed: u64) -> Vec<Hit> {
    let Some(row) = p.rows.get(r) else {
        return Vec::new();
    };
    let (from, to) = row.loop_range(p.steps());
    let order: Vec<usize> = (from..to).filter(|&i| !row.skip_at(i)).collect();
    let m = order.len();
    if m == 0 || length == 0 {
        return Vec::new();
    }
    let rate = p.row_rate(r);
    let mode = p.row_mode(r);
    // One pass through the loop, as places in `order`.
    let cycle: Vec<usize> = match mode {
        PlayMode::Backward => (0..m).rev().collect(),
        PlayMode::PingPong => (0..m).chain((0..m).rev()).collect(),
        PlayMode::Pendulum if m > 2 => (0..m).chain((1..m - 1).rev()).collect(),
        _ => (0..m).collect(),
    };
    let mut hits = Vec::new();
    let (mut t, mut k, mut at, mut order_now) = (0u64, 0usize, 0usize, Vec::new());
    while t < length {
        let (pass, j) = (k / cycle.len(), k % cycle.len());
        let place = match mode {
            PlayMode::Wander => {
                if k > 0 {
                    let up = hash(seed, k as u64) & 1 == 0;
                    at = match (up, at + 1 < m, at > 0) {
                        (true, true, _) | (false, true, false) => at + 1,
                        (_, _, true) => at - 1,
                        _ => at,
                    };
                }
                at
            }
            PlayMode::Shuffle => {
                if j == 0 {
                    order_now = shuffled(m, hash(seed, pass as u64 ^ 0x5348));
                }
                order_now[j]
            }
            PlayMode::Random => (hash(seed, k as u64) % m as u64) as usize,
            _ => cycle[j],
        };
        let step = order[place];
        let d = row.rate_at(step).unwrap_or(rate).max(1);
        hits.push(Hit {
            step,
            at: t,
            length: d,
            pass,
        });
        t += d;
        k += 1;
    }
    hits
}

/// How late swing makes a step starting `at`: one in the second half of a pair of sixteenths (or
/// eighths) comes as far into the pair as `swing` says (50% half way: straight; 66% a triplet
/// feel); one in the first half is on time.
pub fn swing_late(at: u64, eighths: bool, swing: f32) -> i64 {
    let unit = if eighths { PPQ / 2 } else { PPQ / 4 };
    if swing <= 50.0 || at % (2 * unit) < unit {
        return 0;
    }
    ((swing.clamp(50.0, 75.0) - 50.0) / 50.0 * unit as f32) as i64
}

/// Which rows sound: not muted, and soloed if any is.
fn sounds(p: &Pattern) -> impl Fn(&PatternRow) -> bool {
    let soloed = p.rows.iter().any(|r| r.solo);
    move |r: &PatternRow| !r.mute && (!soloed || r.solo)
}

/// Whether a step's condition lets it play on this pass (`last` the last pass that starts).
fn condition_holds(row: &PatternRow, i: usize, pass: usize, last: usize) -> bool {
    match row.condition_at(i) {
        Condition::Always => true,
        Condition::FirstLoop => pass == 0,
        Condition::LastLoop => pass == last,
        Condition::Recurrence => {
            let (nth, of) = row.every_at(i);
            pass % of as usize == nth as usize - 1
        }
    }
}

/// The notes a pattern plays over `length` ticks.
pub fn pattern_notes(p: &Pattern, length: u64) -> Vec<Note> {
    pattern_played(p, length, 0, &[], 120.0)
}

/// A pattern's notes as they play from `start` (ticks into the song): every row through its
/// loop, each step as its settings say, and what the track's modulators do to each sound (its
/// velocity, pitch, chance), worked out where each step falls in the song. A step's chance is
/// rolled afresh each time round, the same each time the song plays.
pub fn pattern_played(
    p: &Pattern,
    length: u64,
    start: u64,
    mods: &[Modulator],
    tempo: f64,
) -> Vec<Note> {
    let sounds = sounds(p);
    let mut notes = Vec::new();
    for (r, row) in p.rows.iter().enumerate() {
        if row.is_automation() || !sounds(row) {
            continue;
        }
        let hits = row_hits(p, r, length, hash(start, r as u64));
        let last = hits.last().map_or(0, |h| h.pass);
        let mut h = 0;
        while h < hits.len() {
            let (from, hit) = (h, hits[h]);
            let i = hit.step;
            if !row.on(i) {
                h += 1;
                continue;
            }
            // A tie runs it on through the steps it's tied to, which don't play again.
            let mut end = from;
            while row.tie_at(hits[end].step) && end + 1 < hits.len() {
                end += 1;
            }
            h = end + 1;
            // What the modulators on this sound make of this step.
            let (mut dv, mut dp, mut dc) = (0f32, 0f32, 0f32);
            let beats = (start + hit.at) as f64 / PPQ as f64;
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
            let rolled =
                chance >= 100.0 || crate::dsp::modulate::roll((start + hit.at) as i64, r) < chance;
            if !rolled || !condition_holds(row, i, hit.pass, last) {
                continue;
            }
            let velocity = (row.velocity_at(i) as f32 + dv).round().clamp(1.0, 127.0);
            let pitch = (row.pitch_at(i) as i32 + dp.round() as i32).clamp(0, 127) as u8;
            let offset = (hit.length as f64 * row.nudge_at(i) as f64 / 100.0).round() as i64;
            let at = hit.at as i64 + offset + swing_late(hit.at, p.swing_eighths, p.swing);
            let legato = |gate: u8| (p.legato && gate >= 100) as u64;
            if end > from {
                // Tied: one note, through every step before the last, and the last's gate of it.
                let before: u64 = hits[from..end].iter().map(|x| x.length).sum();
                let tail = hits[end];
                let gate = row.gate_at(tail.step);
                let held = (tail.length as f64 * gate as f64 / 100.0).round() as u64;
                notes.push(Note {
                    start: at,
                    length: (before + held + legato(gate)).max(1),
                    pitch,
                    velocity: velocity as u8,
                });
                continue;
            }
            // Its repeats: as many as it has, longer first or shorter first by its
            // distribution, louder or softer through them by its ramp and curve.
            let n = row.repeat_at(i) as usize;
            let q = 2f64.powf(row.spread_at(i) as f64 / 100.0);
            let weights: Vec<f64> = (0..n).map(|j| q.powi(-(j as i32))).collect();
            let total: f64 = weights.iter().sum();
            let gate = row.gate_at(i);
            let (ramp, curve) = (row.ramp_at(i) as f32 / 100.0, row.curve_at(i) as f32);
            // Even repeats as whole ticks, each from the step's start.
            let even = row.spread_at(i) == 0;
            let mut t = at as f64;
            for (j, w) in weights.iter().enumerate() {
                let sub = match even {
                    true => (hit.length / n as u64).max(1) as f64,
                    false => hit.length as f64 * w / total,
                };
                let along = if n > 1 {
                    j as f32 / (n - 1) as f32
                } else {
                    0.0
                };
                let bent = along.powf(2f32.powf(curve / 50.0));
                let v = (velocity * (1.0 + ramp * bent)).round().clamp(1.0, 127.0) as u8;
                let held = ((sub * gate as f64 / 100.0).round() as u64).max(1);
                notes.push(Note {
                    start: t.round() as i64,
                    length: held + legato(gate),
                    pitch,
                    velocity: v,
                });
                t += sub;
            }
        }
    }
    notes.sort_by_key(|n| (n.start, n.pitch));
    notes
}

/// What a pattern's automation rows set over `length` ticks from `start`: each row's target and
/// its values (0 to 1) at the steps that play, held to the next (or sliding to it).
pub fn pattern_automation(
    p: &Pattern,
    length: u64,
    start: u64,
) -> Vec<(RowTarget, Vec<AutoPoint>)> {
    let sounds = sounds(p);
    let mut out = Vec::new();
    for (r, row) in p.rows.iter().enumerate() {
        let Some(target) = row.target else {
            continue;
        };
        if !sounds(row) {
            continue;
        }
        let hits = row_hits(p, r, length, hash(start, r as u64));
        let last = hits.last().map_or(0, |h| h.pass);
        let mut points: Vec<AutoPoint> = Vec::new();
        for hit in &hits {
            let i = hit.step;
            if !row.on(i) || !condition_holds(row, i, hit.pass, last) {
                continue;
            }
            let chance = row.chance_at(i) as f32;
            if chance < 100.0 && crate::dsp::modulate::roll((start + hit.at) as i64, r) >= chance {
                continue;
            }
            let offset = (hit.length as f64 * row.nudge_at(i) as f64 / 100.0).round() as i64;
            let late = swing_late(hit.at, p.swing_eighths, p.swing);
            let tick = (hit.at as i64 + offset + late).max(0) as u64;
            if tick >= length {
                continue;
            }
            points.push(AutoPoint {
                step: !row.slide,
                ..AutoPoint::new(tick, row.value_at(i))
            });
        }
        points.sort_by_key(|p| p.tick);
        points.dedup_by_key(|p| p.tick);
        if !points.is_empty() {
            out.push((target, points));
        }
    }
    out
}

/// The step row `r` is on `at` ticks into a pass of its region that began `start` ticks into the
/// song, as it plays (its own rate, loop and way through).
pub fn row_step_at(p: &Pattern, r: usize, at: u64, start: u64) -> Option<usize> {
    row_hits(p, r, at + 1, hash(start, r as u64))
        .into_iter()
        .rfind(|h| h.at <= at && at < h.at + h.length)
        .map(|h| h.step)
}

/// The step (as played) row `r` is on `at` ticks into a pass of its region that began `start`
/// ticks into the song.
pub fn row_hit_at(p: &Pattern, r: usize, at: u64, start: u64) -> Option<Hit> {
    row_hits(p, r, at + 1, hash(start, r as u64))
        .into_iter()
        .rfind(|h| h.at <= at && at < h.at + h.length)
}
