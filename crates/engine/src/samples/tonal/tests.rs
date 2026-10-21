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
