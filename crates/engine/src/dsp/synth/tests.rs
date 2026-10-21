use super::*;

#[test]
fn plays_from_the_note_offset_and_releases() {
    let mut s = Synth::new(48_000.0);
    let (mut l, mut r) = (vec![0f32; 4800], vec![0f32; 4800]);
    let on = Note {
        offset: 100,
        pitch: 69,
        velocity: 1.0,
    };
    let off = Note {
        offset: 2000,
        pitch: 69,
        velocity: 0.0,
    };
    s.process([&mut l, &mut r], 4800, &[on, off]);
    assert!(l[..101].iter().all(|&x| x == 0.0));
    assert!(l[200..2000].iter().any(|&x| x.abs() > 0.1));
    // Release is 50 ms (2400 samples): silent after.
    assert!(l[4500..].iter().all(|&x| x == 0.0));
    assert_eq!(l, r);
}
