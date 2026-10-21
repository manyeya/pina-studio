use super::*;

#[test]
fn colours_round_trip_and_shades_differ() {
    for c in [0xd97757, 0x3ab7ff, 0x808080] {
        let (h, s, l) = to_hsl(c);
        let back = hsl(h, s, l);
        let near =
            |a: u32, b: u32, shift: u32| ((a >> shift) & 255).abs_diff((b >> shift) & 255) <= 1;
        assert!(
            near(c, back, 16) && near(c, back, 8) && near(c, back, 0),
            "{c:06x} {back:06x}"
        );
    }
    let base = family_color(Family::Drums);
    let shades: HashSet<u32> = (0..6).map(|n| shade(base, n)).collect();
    assert_eq!(shades.len(), 6);
    assert!(look::ICONS.iter().all(|i| shape(i.id) != shape("nothing")));
}
