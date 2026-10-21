use super::*;

#[test]
fn grooves_are_named_by_their_nearest() {
    let (id, score) = nearest("X...X...X...X...", "....x.......x...", "..x...x...x...x.");
    assert_eq!(id, "house");
    assert!(score > 0.8);
    let (id, _) = nearest("x.....x...x.....", "........X.......", "x.x.x.x.x.x.x.x.");
    assert_eq!(id, "trap");
}
