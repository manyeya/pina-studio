use super::*;

fn chroma(pcs: &[u8]) -> [f32; 12] {
    let mut c = [0.05f32; 12];
    for &p in pcs {
        c[p as usize] = 1.0;
    }
    c
}

#[test]
fn keys_and_chords_are_read_from_pitch_classes() {
    // A minor's scale, its tonic in the bass.
    let (k, fit) = key_of(&chroma(&[9, 11, 0, 2, 4, 5, 7]), &chroma(&[9]));
    assert_eq!(k.name(), "A minor");
    assert!(fit > 0.5);
    // Am, Am, F, F, C, C, G, G: each read, and held across its beats.
    let beats: Vec<[f32; 12]> = [
        [9, 0, 4],
        [9, 0, 4],
        [5, 9, 0],
        [5, 9, 0],
        [0, 4, 7],
        [0, 4, 7],
        [7, 11, 2],
        [7, 11, 2],
    ]
    .iter()
    .map(|c| chroma(c))
    .collect();
    let bass: Vec<[f32; 12]> = [9, 9, 5, 5, 0, 0, 7, 7]
        .iter()
        .map(|&b| chroma(&[b]))
        .collect();
    let found: Vec<String> = chords(&beats, &bass)
        .iter()
        .map(|c| c.map_or("N".into(), |c| c.name()))
        .collect();
    assert_eq!(found, ["Am", "Am", "F", "F", "C", "C", "G", "G"]);
    let am = Key::parse("A minor").unwrap();
    assert_eq!(numeral(&Chord::parse("F").unwrap(), &am), "VI");
    assert_eq!(numeral(&Chord::parse("Dm7").unwrap(), &am), "iv7");
    assert_eq!(numeral(&Chord::parse("E7").unwrap(), &am), "V7");
}

#[test]
fn loops_are_found_in_bars() {
    let bars: Vec<String> = [
        "Am", "F", "C", "G", "Am", "F", "C", "G", "Am", "F", "C", "G",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(loop_of(&bars), ["Am", "F", "C", "G"]);
    let two: Vec<String> = ["Dm", "G", "Dm", "G", "Dm", "G"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(loop_of(&two), ["Dm", "G"]);
}
