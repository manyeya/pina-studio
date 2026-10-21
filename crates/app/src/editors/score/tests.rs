use super::*;

#[test]
fn notes_find_their_line_and_value() {
    let c = Key {
        root: 0,
        minor: false,
    };
    assert_eq!(step(60, &c), (28, None)); // middle C, a ledger line under the treble
    assert_eq!(step(64, &c).0, TREBLE_LINE); // E4, the treble's bottom line
    assert_eq!(step(43, &c).0, BASS_LINE); // G2, the bass's bottom line
    assert_eq!(step(61, &c), (28, Some(1))); // C#4
    assert_eq!(step(72, &c).0 - step(60, &c).0, 7); // an octave is seven steps
    // In F major, Bb needs no flat of its own; B natural needs a natural.
    let f = Key::parse("F major").unwrap();
    assert_eq!(step(70, &f), (34, None));
    assert_eq!(step(71, &f), (34, Some(0)));
    // In Eb major, Gb is a G with a flat.
    let eb = Key::parse("Eb major").unwrap();
    assert_eq!(step(66, &eb), (32, Some(-1)));
    assert_eq!(value(PPQ), Value::Quarter);
    assert_eq!(value(PPQ * 7 / 8), Value::Quarter); // detached, still a quarter
    assert_eq!(value(PPQ / 2 + 60), Value::Eighth);
    assert_eq!(value(PPQ * 4), Value::Whole);
}
