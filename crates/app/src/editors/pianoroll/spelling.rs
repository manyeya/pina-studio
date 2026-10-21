use super::*;

#[test]
fn notes_are_named_as_the_key_spells_them() {
    let key = |s: &str| Key::parse(s);
    assert_eq!(name_in(None, 60), "C3"); // middle C
    assert_eq!(name_in(None, 70), "A♯3");
    assert_eq!(name_in(key("Eb major"), 70), "B♭3");
    assert_eq!(name_in(key("Eb major"), 63), "E♭3");
    assert_eq!(name_in(key("E major"), 70), "A♯3");
    assert_eq!(name_in(key("C major"), 72), "C4");
}

#[test]
fn stamps_stay_in_key() {
    let c = Key::parse("C major");
    assert_eq!(Stamp::Triad.pitches(62, c), [62, 65, 69]); // D minor in C major
    assert_eq!(Stamp::Seventh.pitches(67, c), [67, 71, 74, 77]); // G7
    assert_eq!(Stamp::Triad.pitches(61, c), [61, 65, 68]); // off the scale: major
    assert_eq!(Stamp::Power.pitches(40, None), [40, 47, 52]);
    assert_eq!(Stamp::Note.pitches(60, c), [60]);
}

#[test]
fn velocities_run_cool_to_hot() {
    assert_eq!(velocity_color(0), 0x6366f1);
    assert_eq!(velocity_color(127), 0xff4d6d);
    let mid = velocity_color(64);
    assert!(mid != velocity_color(0) && mid != velocity_color(127));
}
