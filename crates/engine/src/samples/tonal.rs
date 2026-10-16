//! The notes a sample plays. Each frame's spectral peaks are grouped into notes one at a time,
//! the strongest harmonic series first, each note's harmonics taken out before the next is
//! looked for: so a C's third harmonic counts for the C, not as a G of its own (folding every
//! bin into its pitch class pushes the fifth up and hears C blues as G minor). From the notes:
//! the pitch classes (chroma), the bass line (each frame's lowest note), how much of the sound
//! is notes at all (tonal), and the key.

use super::dsp::{Spectra, parabolic};

/// What the notes come to.
#[derive(Clone, Debug, Default)]
pub struct Tonal {
    /// The notes' pitch classes, C to B, summing to 1 (or all 0, with no notes).
    pub chroma: [f32; 12],
    /// The same for each frame's lowest note: the bass line.
    pub bass: [f32; 12],
    /// The bass at its start (its first eighth): a loop most often starts on its home chord.
    pub first: [f32; 12],
    /// 0 to 1: how much of its energy is in notes' harmonics.
    pub tonal: f32,
    /// How many notes sound at once, on average, where any do.
    pub polyphony: f32,
}

/// A peak in a frame's spectrum: its frequency, magnitude (what's left of it as notes take
/// their harmonics), and power.
#[derive(Clone, Copy)]
struct Peak {
    hz: f32,
    amp: f32,
    power: f32,
}

/// The lowest and highest fundamentals looked for.
const LOWEST: f32 = 27.0;
const HIGHEST: f32 = 2100.0;
/// Harmonics are looked for up to here.
const TOP: f32 = 5000.0;

/// The frame size for a rate: about 170 ms, fine enough to part a bass line's semitones.
pub fn frame_size(rate: f32) -> usize {
    ((rate * 0.17) as usize).next_power_of_two()
}

/// The notes in `x` between `from` and `to` (samples), from up to `frames` frames spread evenly.
pub fn analyze(x: &[f32], rate: f32, from: usize, to: usize, frames: usize) -> Tonal {
    let size = frame_size(rate);
    let bin = rate / size as f32;
    let (from, to) = (from.min(x.len()), to.min(x.len()));
    if to <= from {
        return Tonal::default();
    }
    let span = to - from;
    let count = (span / (size / 2)).clamp(1, frames.max(1));
    let mut spectra = Spectra::new(size);
    let mut power = vec![0f32; size / 2 + 1];
    let starts: Vec<isize> = (0..count)
        .map(|i| {
            if count == 1 {
                // One frame: centred on the sound, if it's shorter than a frame.
                from as isize + (span as isize - size as isize).min(0) / 2
            } else {
                (from + i * (span - size.min(span)) / (count - 1)) as isize
            }
        })
        .collect();
    let mut frames_power = Vec::with_capacity(count);
    for &start in &starts {
        spectra.power(x, start, &mut power);
        frames_power.push(power.clone());
    }
    let energy: Vec<f32> = frames_power
        .iter()
        .map(|p| p[(LOWEST / bin) as usize..].iter().sum())
        .collect();
    let loudest = energy.iter().fold(0f32, |m, &e| m.max(e));
    let mut out = Tonal::default();
    if loudest <= 0.0 {
        return out;
    }
    let (mut explained, mut total, mut notes_sum, mut note_frames) = (0f32, 0f32, 0f32, 0f32);
    let opening = from as isize + (span / 8).max(size / 2) as isize;
    for ((p, &e), &start) in frames_power.iter().zip(&energy).zip(&starts) {
        total += e;
        if e < loudest * 1e-5 {
            continue;
        }
        let mut peaks = peaks(p, bin);
        let notes = notes(&mut peaks, bin, e);
        if notes.is_empty() {
            continue;
        }
        note_frames += 1.0;
        notes_sum += notes.len() as f32;
        let mut lowest = (f32::MAX, 0.0, 0usize);
        for &(hz, salience, power) in &notes {
            let class = pitch_class(hz);
            out.chroma[class] += salience;
            explained += power;
            if hz < lowest.0 {
                lowest = (hz, salience, class);
            }
        }
        out.bass[lowest.2] += lowest.1;
        if start < opening || out.first.iter().all(|&v| v == 0.0) {
            out.first[lowest.2] += lowest.1;
        }
    }
    out.tonal = (explained / total.max(1e-20)).clamp(0.0, 1.0);
    out.polyphony = if note_frames > 0.0 {
        notes_sum / note_frames
    } else {
        0.0
    };
    normalise(&mut out.chroma);
    normalise(&mut out.bass);
    normalise(&mut out.first);
    out
}

fn normalise(x: &mut [f32; 12]) {
    let sum: f32 = x.iter().sum();
    if sum > 0.0 {
        x.iter_mut().for_each(|v| *v /= sum);
    }
}

fn pitch_class(hz: f32) -> usize {
    let midi = 69.0 + 12.0 * (hz / 440.0).log2();
    (midi.round() as i32).rem_euclid(12) as usize
}

/// A frame's peaks that stand clear of the noise around them (8 dB over their neighbourhood).
fn peaks(p: &[f32], bin: f32) -> Vec<Peak> {
    let lo = ((LOWEST * 0.97 / bin).floor() as usize).max(9);
    let hi = ((TOP / bin) as usize).min(p.len().saturating_sub(9));
    let strongest = p[lo.min(hi)..hi].iter().fold(0f32, |m, &v| m.max(v));
    let floor = strongest * 1e-5;
    let mut out = Vec::new();
    for k in lo..hi {
        let v = p[k];
        if v < floor || v < p[k - 1] || v <= p[k + 1] || v < p[k - 2] || v <= p[k + 2] {
            continue;
        }
        let around: f32 = p[k - 8..k - 2].iter().chain(&p[k + 3..k + 9]).sum::<f32>() / 12.0;
        if v < 6.0 * around {
            continue;
        }
        let shift = parabolic(
            p[k - 1].max(1e-30).ln(),
            v.max(1e-30).ln(),
            p[k + 1].max(1e-30).ln(),
        );
        out.push(Peak {
            hz: (k as f32 + shift) * bin,
            amp: v.sqrt(),
            power: p[k - 2..=k + 2].iter().sum(),
        });
    }
    out
}

/// The peak nearest `hz` within its tolerance (wider for higher harmonics, which stretch), if
/// any is left.
fn find(peaks: &[Peak], hz: f32, harmonic: usize, bin: f32) -> Option<usize> {
    let cents = 35.0 + 8.0 * harmonic.saturating_sub(4) as f32;
    let tolerance = (hz * (cents / 1200.0 * std::f32::consts::LN_2)).max(0.6 * bin);
    let i = peaks.partition_point(|p| p.hz < hz - tolerance);
    peaks[i..]
        .iter()
        .enumerate()
        .take_while(|(_, p)| p.hz <= hz + tolerance)
        .filter(|(_, p)| p.amp > 0.0)
        .min_by(|a, b| (a.1.hz - hz).abs().total_cmp(&(b.1.hz - hz).abs()))
        .map(|(j, _)| i + j)
}

/// Groups a frame's peaks into notes, strongest first: (frequency, salience, power).
fn notes(peaks: &mut [Peak], bin: f32, energy: f32) -> Vec<(f32, f32, f32)> {
    let mut notes = Vec::new();
    let mut first = 0.0;
    for _ in 0..6 {
        let mut best: Option<(usize, f32)> = None;
        for (i, p) in peaks.iter().enumerate() {
            if p.amp <= 0.0 || p.hz < LOWEST || p.hz > HIGHEST {
                continue;
            }
            let mut score = 0.0;
            for h in 1..=10usize {
                let f = p.hz * h as f32;
                if f > TOP {
                    break;
                }
                if let Some(j) = find(peaks, f, h, bin) {
                    score += peaks[j].amp / (h as f32).sqrt();
                }
            }
            if best.is_none_or(|b| score > b.1) {
                best = Some((i, score));
            }
        }
        let Some((i, score)) = best else { break };
        if notes.is_empty() {
            first = score;
        } else if score < 0.15 * first {
            break;
        }
        // Take its harmonics: all of a harmonic about as strong as its neighbours, only the
        // excess of one much stronger (another note's, sharing it).
        let f0 = peaks[i].hz;
        let mut matched: Vec<(usize, usize)> = Vec::new();
        for h in 1..=10usize {
            let f = f0 * h as f32;
            if f > TOP {
                break;
            }
            if let Some(j) = find(peaks, f, h, bin) {
                matched.push((h, j));
            }
        }
        let amps: Vec<f32> = matched.iter().map(|&(_, j)| peaks[j].amp).collect();
        let (mut salience, mut power) = (0.0, 0.0);
        for (m, &(h, j)) in matched.iter().enumerate() {
            let expected = if h == 1 {
                f32::MAX
            } else {
                let before = amps[m - 1];
                let after = amps.get(m + 1).copied().unwrap_or(before);
                before.min(after)
            };
            let amp = peaks[j].amp;
            let taken = if amp > 2.0 * expected { expected } else { amp };
            let share = taken / amp;
            salience += taken * taken;
            power += peaks[j].power * share * share;
            peaks[j].amp = amp - taken;
        }
        if power < 0.003 * energy {
            break;
        }
        notes.push((f0, salience.sqrt(), power));
    }
    notes
}

/// Major and minor key profiles (Temperley's, from the Kostka-Payne corpus), tonic first.
const MAJOR: [f32; 12] = [
    0.748, 0.060, 0.488, 0.082, 0.670, 0.460, 0.096, 0.715, 0.104, 0.366, 0.057, 0.400,
];
const MINOR: [f32; 12] = [
    0.712, 0.084, 0.474, 0.618, 0.049, 0.460, 0.105, 0.747, 0.404, 0.067, 0.133, 0.330,
];

/// Pearson correlation of two pitch-class profiles.
pub fn correlate(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    let (ma, mb) = (a.iter().sum::<f32>() / 12.0, b.iter().sum::<f32>() / 12.0);
    let (mut num, mut da, mut db) = (0.0, 0.0, 0.0);
    for i in 0..12 {
        num += (a[i] - ma) * (b[i] - mb);
        da += (a[i] - ma).powi(2);
        db += (b[i] - mb).powi(2);
    }
    if da > 0.0 && db > 0.0 {
        num / (da * db).sqrt()
    } else {
        0.0
    }
}

/// How much the bass line counts towards the key: its notes on the key's strong degrees (the
/// tonic, fifth, fourth) say more about the key than any chord over them. And its first notes
/// more again.
pub const BASS: f32 = 1.0;
pub const FIRST: f32 = 0.5;

/// The key the notes fit best: its tonic (0 is C), whether it's minor, and how well the chroma
/// fits it (-1 to 1).
pub fn key(t: &Tonal) -> Option<(u8, bool, f32)> {
    key_weighted(t, BASS, FIRST, ROOT)
}

/// How much the bass line's own share of a key's tonic counts: the note a line keeps coming
/// back to is most often home.
pub const ROOT: f32 = 0.5;

/// `key`, with the bass line, its start and the tonic's share of it weighted as given.
pub fn key_weighted(
    t: &Tonal,
    bass_weight: f32,
    first_weight: f32,
    root_weight: f32,
) -> Option<(u8, bool, f32)> {
    if t.chroma.iter().sum::<f32>() <= 0.0 {
        return None;
    }
    let mut best = (0u8, false, f32::MIN, 0.0);
    for tonic in 0..12 {
        for (profile, minor) in [(&MAJOR, false), (&MINOR, true)] {
            let rotated: [f32; 12] = std::array::from_fn(|i| profile[(i + 12 - tonic) % 12]);
            let fit = correlate(&t.chroma, &rotated);
            let on = |x: &[f32; 12]| (0..12).map(|i| x[i] * rotated[i]).sum::<f32>() / 0.75;
            let score = fit
                + bass_weight * on(&t.bass)
                + first_weight * on(&t.first)
                + root_weight * (t.bass[tonic] + t.first[tonic]);
            if score > best.2 {
                best = (tonic as u8, minor, score, fit);
            }
        }
    }
    Some((best.0, best.1, best.3))
}

/// A chord's root and whether it's minor, for a one-shot chord or stab: the bass note it's
/// built on (or its strongest note, if its bass is unclear), minor if its minor third outweighs
/// its major third; with how well its notes fit that key (-1 to 1).
pub fn chord(t: &Tonal) -> Option<(u8, bool, f32)> {
    if t.chroma.iter().sum::<f32>() <= 0.0 {
        return None;
    }
    let weight = |r: usize| 2.0 * t.bass[r] + t.chroma[r] + 0.5 * t.chroma[(r + 7) % 12];
    let root = (0..12).max_by(|&a, &b| weight(a).total_cmp(&weight(b)))?;
    let minor = t.chroma[(root + 3) % 12] > t.chroma[(root + 4) % 12];
    let profile = if minor { &MINOR } else { &MAJOR };
    let rotated: [f32; 12] = std::array::from_fn(|i| profile[(i + 12 - root) % 12]);
    Some((root as u8, minor, correlate(&t.chroma, &rotated)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 44_100.0;

    /// Notes (MIDI) each with eight harmonics falling off as 1/h, as a plain synth or organ.
    fn chord(notes: &[f32], seconds: f32) -> Vec<f32> {
        (0..(seconds * RATE) as usize)
            .map(|i| {
                let t = i as f32 / RATE;
                notes
                    .iter()
                    .map(|n| {
                        let f = 440.0 * 2f32.powf((n - 69.0) / 12.0);
                        (1..=8)
                            .map(|h| (std::f32::consts::TAU * f * h as f32 * t).sin() / h as f32)
                            .sum::<f32>()
                    })
                    .sum::<f32>()
                    * 0.1
            })
            .collect()
    }

    fn top(chroma: &[f32; 12], n: usize) -> Vec<usize> {
        let mut order: Vec<usize> = (0..12).collect();
        order.sort_by(|&a, &b| chroma[b].total_cmp(&chroma[a]));
        order.truncate(n);
        order.sort();
        order
    }

    #[test]
    fn a_single_note_is_one_pitch_class_not_its_harmonics() {
        // C3: its harmonics include G and E, which mustn't read as notes.
        let x = chord(&[48.0], 1.0);
        let t = analyze(&x, RATE, 0, x.len(), 16);
        assert!(t.chroma[0] > 0.9, "{:?}", t.chroma);
        assert!(t.tonal > 0.8, "{}", t.tonal);
        assert_eq!(t.bass[0], 1.0);
    }

    #[test]
    fn a_chord_reads_as_its_notes() {
        // A minor: A2 C4 E4.
        let x = chord(&[45.0, 60.0, 64.0], 1.0);
        let t = analyze(&x, RATE, 0, x.len(), 16);
        assert_eq!(top(&t.chroma, 3), vec![0, 4, 9], "{:?}", t.chroma);
        assert!(t.polyphony > 2.5, "{}", t.polyphony);
        let (tonic, minor, _) = key(&t).unwrap();
        assert_eq!((tonic, minor), (9, true));
    }

    #[test]
    fn a_blues_in_c_is_in_c() {
        // C9 Eb9 F9 G9, a beat each, over their roots: the chords favour G, the bass says C.
        let roots = [36.0, 39.0, 41.0, 43.0];
        let mut x = Vec::new();
        for _ in 0..2 {
            for r in roots {
                // Root, then 3rd, 7th and 9th above it an octave up, as a pianist voices it.
                x.extend(chord(&[r, r + 16.0, r + 22.0, r + 26.0], 0.63));
            }
        }
        let t = analyze(&x, RATE, 0, x.len(), 64);
        let (tonic, _, _) = key(&t).unwrap();
        assert_eq!(tonic, 0, "chroma {:?} bass {:?}", t.chroma, t.bass);
    }

    #[test]
    fn noise_has_no_notes() {
        let mut seed = 7u32;
        let x: Vec<f32> = (0..44_100)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
            })
            .collect();
        let t = analyze(&x, RATE, 0, x.len(), 16);
        assert!(t.tonal < 0.1, "{}", t.tonal);
    }
}
