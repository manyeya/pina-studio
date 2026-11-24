use super::*;

#[test]
fn grooves_are_named_by_the_playbook_whose_beat_is_nearest() {
    let (id, score) = nearest("X...X...X...X...", "....x.......x...", "..x...x...x...x.");
    assert_eq!(id, "house");
    assert!(score > 0.8);
    let (id, _) = nearest("x.....x...x.....", "........X.......", "x.x.x.x.x.x.x.x.");
    assert_eq!(id, "trap");
    // A book with no beat (lofi) is never the nearest.
    let (id, _) = nearest("x.......x.......", "....x.......x...", "x.x.x.x.x.x.x.x.");
    assert_ne!(id, "lofi");
}
