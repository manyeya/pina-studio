//! Fitting audio to a new length, as Logic's Flex and Ableton's Warp do, three ways:
//!
//! - Polyphonic, the default: Rubber Band's finer engine (R3), the best open stretcher, run
//!   offline over the whole file so it hears all of it first, its hits' times pinned. Around
//!   each hit, the recording itself: the 25 ms before (where a phase vocoder smears the hit
//!   ahead of itself, a pre-echo) and its first 20 ms, blended into the stretch either side. On
//!   real drum loops at 80 and 120 BPM from 95 that leaves the attacks as recorded, the hits
//!   within a millisecond or two of their times, and as quiet before them as the recording is,
//!   where Signalsmith Stretch had 2 to 3 times its level there. Chords, voices and mixes are
//!   R3's, whole.
//! - Slicing: cut at the hits, each played as recorded at its new time, as a drum loop is chopped
//!   on an MPC: slower leaves gaps between the hits, faster cuts their tails.
//! - Speed: as a turntable or a sampler, faster is higher; nothing is stretched.

use serde::{Deserialize, Serialize};

use crate::engine::{AudioData, resample};

/// How an audio region is fitted to its length.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum Flex {
    #[default]
    Polyphonic,
    Slicing,
    Speed,
}

impl Flex {
    pub const ALL: [Flex; 3] = [Flex::Polyphonic, Flex::Slicing, Flex::Speed];

    pub fn name(self) -> &'static str {
        match self {
            Flex::Polyphonic => "Polyphonic",
            Flex::Slicing => "Slicing",
            Flex::Speed => "Speed",
        }
    }

    pub fn is_default(&self) -> bool {
        *self == Flex::Polyphonic
    }
}

/// Around a hit, as recorded: this long before it and of its attack, in seconds (each kept
/// under a third of the way to the hits either side); then blended into the stretch over this
/// long after, and from it over this long before.
const BEFORE: f64 = 0.025;
const ATTACK: f64 = 0.020;
const INTO_STRETCH: f64 = 0.004;
const FROM_STRETCH: f64 = 0.002;

/// `data` (at `rate`) made `ratio` times as long (2 is half speed), moved by `semitones` (its
/// length kept; Speed ignores it, as its pitch goes with its speed).
pub fn render(data: &AudioData, ratio: f64, semitones: i8, flex: Flex, rate: f64) -> AudioData {
    let input = &data.channels;
    let channels = match flex {
        _ if input.is_empty() || input[0].is_empty() => input.clone(),
        Flex::Speed => input
            .iter()
            .map(|c| fit(resample(c, rate, rate * ratio), out_len(c.len(), ratio)))
            .collect(),
        Flex::Slicing => slicing(input, ratio, semitones, rate),
        Flex::Polyphonic => polyphonic(input, ratio, semitones, rate),
    };
    AudioData { channels }
}

fn out_len(frames: usize, ratio: f64) -> usize {
    (frames as f64 * ratio).round() as usize
}

/// Exactly `len` long: cut, or padded with silence.
fn fit(mut v: Vec<f32>, len: usize) -> Vec<f32> {
    v.resize(len, 0.0);
    v
}

/// Where its hard hits start, in frames, each a millisecond early so nothing of it is lost:
/// drum hits, plucks and stabs, not the soft swell of a pad or a bowed note. Found where the
/// energy of the audio's slope (its highs, where hits are sharpest) jumps 8 dB over the 20 ms
/// before it, and by at least a twentieth of the loudest's level (so a swell from silence, steep
/// in decibels as it starts, isn't one), within 45 dB of the loudest, 50 ms apart at least.
pub fn onsets(channels: &[Vec<f32>], rate: f64) -> Vec<usize> {
    let n = channels.first().map_or(0, Vec::len);
    let hop = ((rate / 200.0) as usize).max(1);
    if n < hop * 8 {
        return Vec::new();
    }
    let mono: Vec<f32> = (0..n)
        .map(|i| channels.iter().map(|c| c[i]).sum::<f32>() / channels.len() as f32)
        .collect();
    let slope: Vec<f32> = mono.windows(2).map(|w| w[1] - w[0]).collect();
    let frames = slope.len() / hop;
    let rms: Vec<f32> = (0..frames)
        .map(|f| {
            let part = &slope[f * hop..((f + 2) * hop).min(slope.len())];
            (part.iter().map(|s| s * s).sum::<f32>() / part.len() as f32).sqrt()
        })
        .collect();
    let db: Vec<f32> = rms.iter().map(|r| 20.0 * (r + 1e-6).log10()).collect();
    let loudest = db.iter().copied().fold(f32::MIN, f32::max);
    let top = rms.iter().copied().fold(0f32, f32::max);
    // How far it rises over the 20 ms before, in decibels and in level.
    let before = |v: &[f32], f: usize, silence: f32| {
        let part = &v[f.saturating_sub(5)..f.saturating_sub(1)];
        match part.is_empty() {
            true => silence,
            false => part.iter().sum::<f32>() / part.len() as f32,
        }
    };
    let now = |v: &[f32], f: usize| v[f].max(v.get(f + 1).copied().unwrap_or(f32::MIN));
    let rise: Vec<f32> = (0..frames)
        .map(|f| now(&db, f) - before(&db, f, loudest - 60.0))
        .collect();
    let lift: Vec<f32> = (0..frames)
        .map(|f| now(&rms, f) - before(&rms, f, 0.0))
        .collect();
    let gap = (0.05 * rate) as usize;
    let pre = (0.001 * rate) as usize;
    let mut hits: Vec<usize> = Vec::new();
    for f in 0..frames {
        let peak = (f.saturating_sub(2)..(f + 3).min(frames)).all(|g| rise[g] <= rise[f]);
        if rise[f] < 8.0 || lift[f] < top * 0.05 || now(&db, f) < loudest - 45.0 {
            continue;
        }
        if !peak {
            continue;
        }
        // To the sample: where it first reaches a fifth of its peak.
        let from = (f * hop).saturating_sub(hop);
        let to = ((f + 4) * hop).min(n);
        let top = mono[from..to].iter().fold(0f32, |m, s| m.max(s.abs()));
        let first = (from..to)
            .find(|&i| mono[i].abs() >= top * 0.2)
            .unwrap_or(f * hop);
        let at = first.saturating_sub(pre);
        if hits.last().is_none_or(|&h| at >= h + gap) {
            hits.push(at);
        }
    }
    hits
}

/// Rubber Band's finer engine, offline: the whole input studied, then stretched by `ratio` and
/// moved by `pitch` (2 is an octave up), with `keys` (input frame, output frame) pinned.
fn rubber_band(
    input: &[Vec<f32>],
    ratio: f64,
    pitch: f64,
    rate: f64,
    keys: &[(usize, usize)],
) -> Vec<Vec<f32>> {
    use rubberband::{Options, Stretcher};
    let n = input[0].len();
    let options = Options::PROCESS_OFFLINE
        | Options::ENGINE_FINER
        | Options::TRANSIENTS_CRISP
        | Options::THREADING_NEVER;
    let mut st = Stretcher::new(rate as u32, input.len() as u32, options, ratio, pitch);
    st.set_expected_input_duraction(n as u32);
    if !keys.is_empty() {
        st.set_key_frame_map(keys.iter().map(|&(i, o)| (i as u32, o as u32)).collect());
    }
    let whole: Vec<&[f32]> = input.iter().map(|c| &c[..]).collect();
    st.study(&whole, true);
    let mut out: Vec<Vec<f32>> = vec![Vec::with_capacity(out_len(n, ratio) + 4096); input.len()];
    let drain = |st: &mut rubberband::Stretcher, out: &mut Vec<Vec<f32>>| {
        while let Some(got) = st.available().filter(|&a| a > 0) {
            let mut block = vec![vec![0f32; got as usize]; out.len()];
            let mut refs: Vec<&mut [f32]> = block.iter_mut().map(|b| &mut b[..]).collect();
            let read = st.retrieve(&mut refs) as usize;
            for (o, b) in out.iter_mut().zip(&block) {
                o.extend_from_slice(&b[..read]);
            }
        }
    };
    const BLOCK: usize = 8192;
    let mut at = 0;
    while at < n {
        let end = (at + BLOCK).min(n);
        let part: Vec<&[f32]> = input.iter().map(|c| &c[at..end]).collect();
        st.process(&part, end == n);
        drain(&mut st, &mut out);
        at = end;
    }
    drain(&mut st, &mut out);
    out.into_iter().map(|c| fit(c, out_len(n, ratio))).collect()
}

/// Rubber Band with the hits pinned, and (unmoved in pitch) the recording around each.
fn polyphonic(input: &[Vec<f32>], ratio: f64, semitones: i8, rate: f64) -> Vec<Vec<f32>> {
    let n = input[0].len();
    let len = out_len(n, ratio);
    let place = |i: usize| out_len(i, ratio);
    let hits = onsets(input, rate);
    let keys: Vec<(usize, usize)> = hits
        .iter()
        .filter(|&&h| h > 0)
        .map(|&h| (h, place(h)))
        .collect();
    let pitch = 2f64.powf(semitones as f64 / 12.0);
    let mut out = rubber_band(input, ratio, pitch, rate, &keys);
    // Moved in pitch, the recording would be in the old key.
    if semitones != 0 || ratio == 1.0 {
        return out;
    }
    let frames = |s: f64| (s * rate) as usize;
    let (before, attack, after, into) = (
        frames(BEFORE),
        frames(ATTACK),
        frames(INTO_STRETCH),
        frames(FROM_STRETCH),
    );
    for (k, &h) in hits.iter().enumerate() {
        let o = place(h);
        // Not into the hits either side, here or there.
        let prev = k.checked_sub(1).map(|p| hits[p]);
        let (prev_in, prev_out) = prev.map_or((h, o), |p| (h - p, o - place(p)));
        let next_in = hits.get(k + 1).map_or(n, |&x| x) - h;
        let next_out = hits.get(k + 1).map_or(len, |&x| place(x)) - o;
        let back = before.min(prev_in / 3).min(prev_out / 3);
        let keep = attack
            .min(next_in.saturating_sub(after))
            .min(next_out.saturating_sub(after));
        if keep == 0 {
            continue;
        }
        let (o0, h0) = (o - back, h - back);
        for (side, src) in out.iter_mut().zip(input) {
            if h0 >= into && o0 >= into {
                for j in 0..into {
                    let w = (j + 1) as f32 / (into + 1) as f32;
                    let (oi, ii) = (o0 - into + j, h0 - into + j);
                    side[oi] = side[oi] * (1.0 - w) + src[ii] * w;
                }
            }
            let span = (back + keep).min(len - o0).min(n - h0);
            side[o0..o0 + span].copy_from_slice(&src[h0..h0 + span]);
            for j in 0..after {
                let (oi, ii) = (o + keep + j, h + keep + j);
                if oi >= len || ii >= n {
                    break;
                }
                let w = (j + 1) as f32 / (after + 1) as f32;
                side[oi] = src[ii] * (1.0 - w) + side[oi] * w;
            }
        }
    }
    out
}

/// Cut at the hits, each slice played as recorded (moved in pitch by speed, as a sampler would)
/// from its hit's new time: what's left of its slot is silence, what's over it fades away.
fn slicing(input: &[Vec<f32>], ratio: f64, semitones: i8, rate: f64) -> Vec<Vec<f32>> {
    let n = input[0].len();
    let len = out_len(n, ratio);
    let mut starts = onsets(input, rate);
    if starts.first() != Some(&0) {
        starts.insert(0, 0);
    }
    let speed = 2f64.powf(semitones as f64 / 12.0);
    let fade = (0.003 * rate) as usize;
    let mut out = vec![vec![0f32; len]; input.len()];
    for (k, &a) in starts.iter().enumerate() {
        let b = starts.get(k + 1).copied().unwrap_or(n);
        let o = out_len(a, ratio);
        let slot = starts.get(k + 1).map_or(len, |&x| out_len(x, ratio)) - o;
        for (side, src) in out.iter_mut().zip(input) {
            let slice = match semitones {
                0 => src[a..b].to_vec(),
                _ => resample(&src[a..b], rate * speed, rate),
            };
            let take = slice.len().min(slot);
            // A slice cut short, or one whose sound goes on into the next, ends in a fade.
            let fades = b < n || take < slice.len();
            for j in 0..take {
                let left = take - j;
                let g = match fades && left <= fade {
                    true => left as f32 / (fade + 1) as f32,
                    false => 1.0,
                };
                side[o + j] = slice[j] * g;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 44_100.0;

    /// Hits of decaying noise at `times` (seconds), `seconds` long.
    fn drums(times: &[f64], seconds: f64) -> Vec<Vec<f32>> {
        let n = (seconds * RATE) as usize;
        let mut v = vec![0f32; n];
        let mut seed = 1u32;
        for &t in times {
            let at = (t * RATE) as usize;
            for j in 0..(0.12 * RATE) as usize {
                if at + j >= n {
                    break;
                }
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (seed >> 9) as f32 / (1 << 23) as f32 * 2.0 - 1.0;
                v[at + j] += noise * 0.8 * (-(j as f32) / (0.02 * RATE as f32)).exp();
            }
        }
        vec![v.clone(), v]
    }

    /// A C minor chord that swells in over a quarter second and holds.
    fn pad(seconds: f64) -> Vec<Vec<f32>> {
        let n = (seconds * RATE) as usize;
        let v: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f64 / RATE;
                let swell = (t / 0.25).min(1.0) as f32;
                // C loudest, then Eb and G.
                [(261.63, 0.3), (311.13, 0.1), (392.0, 0.1)]
                    .iter()
                    .map(|(f, a)| (std::f64::consts::TAU * f * t).sin() as f32 * a)
                    .sum::<f32>()
                    * swell
            })
            .collect();
        vec![v]
    }

    /// The strongest frequency, by a slow scan of the middle of `v`, in Hz.
    fn peak_hz(v: &[f32]) -> f64 {
        let mid = &v[v.len() / 4..v.len() * 3 / 4];
        (200..500)
            .map(|f| {
                let f = f as f64;
                let (mut re, mut im) = (0.0, 0.0);
                for (i, s) in mid.iter().enumerate() {
                    let a = std::f64::consts::TAU * f * i as f64 / RATE;
                    re += *s as f64 * a.cos();
                    im += *s as f64 * a.sin();
                }
                (f, re * re + im * im)
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap()
            .0
    }

    #[test]
    fn hits_are_found_to_the_millisecond_and_swells_are_not_hits() {
        let times = [0.1, 0.35, 0.6, 0.85];
        let found = onsets(&drums(&times, 1.0), RATE);
        assert_eq!(found.len(), 4, "{found:?}");
        for (f, t) in found.iter().zip(times) {
            let ms = (*f as f64 / RATE - t) * 1000.0;
            assert!((-2.0..=0.5).contains(&ms), "{ms} ms off at {t}");
        }
        assert!(onsets(&pad(1.0), RATE).is_empty());
    }

    #[test]
    fn polyphonic_keeps_hits_on_time_and_sharp() {
        let times = [0.1, 0.35, 0.6, 0.85];
        let input = AudioData {
            channels: drums(&times, 1.0),
        };
        for ratio in [0.8, 1.25] {
            let out = render(&input, ratio, 0, Flex::Polyphonic, RATE);
            assert_eq!(out.channels[0].len(), out_len(44_100, ratio));
            let found = onsets(&out.channels, RATE);
            assert_eq!(found.len(), 4, "{ratio}: {found:?}");
            for (f, t) in found.iter().zip(times) {
                let ms = (*f as f64 / RATE - t * ratio) * 1000.0;
                assert!(ms.abs() <= 2.0, "{ratio}: {ms} ms off at {t}");
            }
            // The attacks are the recording's, peak for peak, with nothing smeared ahead of them.
            let peak = |v: &[f32], at: usize| {
                v[at..at + (0.01 * RATE) as usize]
                    .iter()
                    .fold(0f32, |m, s| m.max(s.abs()))
            };
            let a = peak(&input.channels[0], (0.35 * RATE) as usize);
            let b = peak(&out.channels[0], (0.35 * ratio * RATE) as usize);
            assert!((a - b).abs() < 0.02, "{ratio}: {a} {b}");
            let before = |v: &[f32], at: f64| {
                let at = (at * RATE) as usize;
                let part = &v[at - (0.02 * RATE) as usize..at - (0.002 * RATE) as usize];
                (part.iter().map(|s| s * s).sum::<f32>() / part.len() as f32).sqrt()
            };
            for t in &times[1..] {
                let (was, now) = (
                    before(&input.channels[0], *t),
                    before(&out.channels[0], t * ratio),
                );
                assert!(
                    now <= was * 1.5 + 1e-3,
                    "{ratio}: {now} before {t}, {was} recorded"
                );
            }
        }
    }

    #[test]
    fn polyphonic_keeps_a_chord_in_tune_and_moves_it_when_asked() {
        let input = AudioData { channels: pad(1.0) };
        let out = render(&input, 1.3, 0, Flex::Polyphonic, RATE);
        assert_eq!(out.channels[0].len(), out_len(44_100, 1.3));
        assert!((peak_hz(&out.channels[0]) - 261.0).abs() < 3.0);
        // Up three semitones, as long as it was: C becomes Eb.
        let up = render(&input, 1.0, 3, Flex::Polyphonic, RATE);
        assert_eq!(up.channels[0].len(), 44_100);
        assert!((peak_hz(&up.channels[0]) - 311.1).abs() < 4.0);
    }

    #[test]
    fn slicing_plays_each_hit_as_recorded_at_its_new_time() {
        let times = [0.1, 0.35, 0.6, 0.85];
        let input = AudioData {
            channels: drums(&times, 1.0),
        };
        let out = render(&input, 1.5, 0, Flex::Slicing, RATE);
        let src = &input.channels[0];
        let found = onsets(&input.channels, RATE);
        for &h in &found {
            let o = out_len(h, 1.5);
            assert_eq!(&out.channels[0][o..o + 1000], &src[h..h + 1000]);
        }
        // After a slice, its slot is silent.
        let gap = out_len(found[1], 1.5) - 10;
        assert_eq!(out.channels[0][gap], 0.0);
    }

    #[test]
    fn speed_is_a_turntable() {
        let input = AudioData { channels: pad(1.0) };
        let out = render(&input, 1.25, 7, Flex::Speed, RATE);
        assert_eq!(out.channels[0].len(), out_len(44_100, 1.25));
        // A quarter slower: lower by that, the transposition ignored.
        assert!((peak_hz(&out.channels[0]) - 261.63 / 1.25).abs() < 3.0);
    }
}
