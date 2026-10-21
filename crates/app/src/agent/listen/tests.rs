use super::*;

#[test]
fn a_relative_key_sits_with_the_song() {
    let k = |s: &str| Key::parse(s).unwrap();
    assert!(sits_with(k("A minor"), k("C major")));
    assert!(sits_with(k("C major"), k("A minor")));
    assert!(sits_with(k("F minor"), k("F minor")));
    assert!(sits_with(k("F major"), k("A minor")));
    assert!(!sits_with(k("G minor"), k("F minor")));
    assert!(!sits_with(k("E major"), k("C major")));
}
