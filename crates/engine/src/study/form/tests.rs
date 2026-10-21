use super::*;

fn bar(chroma_root: u8, timbre: f32, level: f32) -> Bar {
    let mut c = [0.05f32; 12];
    for iv in [0u8, 4, 7] {
        c[((chroma_root + iv) % 12) as usize] = 1.0;
    }
    Bar {
        chroma: c,
        timbre: vec![timbre, -timbre, timbre * 0.5, 1.0],
        level,
    }
}

#[test]
fn a_pop_song_is_read_as_its_sections() {
    // Intro 4, verse 8, chorus 8, verse 8, chorus 8, bridge 4, chorus 8, outro 4.
    let mut bars = Vec::new();
    let plan: [(&str, usize); 8] = [
        ("i", 4),
        ("v", 8),
        ("c", 8),
        ("v", 8),
        ("c", 8),
        ("b", 4),
        ("c", 8),
        ("o", 4),
    ];
    for (kind, len) in plan {
        for k in 0..len {
            bars.push(match kind {
                "i" => bar(9, -2.0, -30.0),
                "v" => bar(if k % 2 == 0 { 9 } else { 2 }, 0.0, -20.0),
                "c" => bar(if k % 2 == 0 { 5 } else { 0 }, 2.0, -12.0),
                "b" => bar(7, -1.0, -22.0),
                _ => bar(9, -2.5, -32.0),
            });
        }
    }
    let parts = form(&bars);
    let named: Vec<(String, usize)> = parts.iter().map(|p| (p.name.clone(), p.from)).collect();
    let names: Vec<&str> = named.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "Intro", "Verse", "Chorus", "Verse 2", "Chorus 2", "Bridge", "Chorus 3", "Outro"
        ],
        "{named:?}"
    );
    assert_eq!(
        named.iter().map(|(_, f)| *f).collect::<Vec<_>>(),
        [0, 4, 12, 20, 28, 36, 40, 48]
    );
    // The choruses share a letter, and are the loudest.
    let chorus: Vec<&Part> = parts
        .iter()
        .filter(|p| p.name.starts_with("Chorus"))
        .collect();
    assert!(
        chorus
            .iter()
            .all(|p| p.label == chorus[0].label && p.energy > 0.8)
    );
}
