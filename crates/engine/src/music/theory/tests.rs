use super::*;

fn key(s: &str) -> Key {
    Key::parse(s).unwrap()
}

#[test]
fn chords_read_and_write_both_ways() {
    let c = Chord::parse("Fmaj9").unwrap();
    assert_eq!((c.root, c.quality), (5, Quality::Maj9));
    assert_eq!(c.classes(), [5, 9, 0, 4, 7]);
    for s in ["Am9", "E7b9", "Bbm7", "F#m7b5", "Dsus4", "C/E", "Gm", "A13"] {
        let c = Chord::parse(s).unwrap();
        assert_eq!(
            Chord::parse(&c.name_in(Some(&key("C minor")))).unwrap(),
            c,
            "{s}"
        );
    }
    assert_eq!(
        Chord::parse("Bb").unwrap().name_in(Some(&key("F major"))),
        "Bb"
    );
    assert!(Chord::parse("H7").is_none());
    // Numerals: in A minor the sixth is F, the seventh G; in C major V7b9 is G7b9.
    let am = key("A minor");
    assert_eq!(numeral("VImaj9", &am).unwrap().name(), "Fmaj9");
    assert_eq!(numeral("V7b9", &am).unwrap().name(), "E7b9");
    assert_eq!(numeral("vii9", &am).unwrap().name(), "Gm9");
    assert_eq!(numeral("bII", &am).unwrap().name(), "Bb");
    assert_eq!(numeral("ii9", &key("C major")).unwrap().name(), "Dm9");
    assert_eq!(numeral("#iv°7", &key("C major")).unwrap().name(), "F#dim7");
}

#[test]
fn progressions_suit_the_words_and_lay_out_by_bar() {
    let am = key("A minor");
    let p = suggest(&am, "lofi", 3);
    assert!(p.minor && p.suits.contains(&"lofi"));
    assert!(
        suggest(&key("C major"), "afrobeats", 1)
            .suits
            .contains(&"afrobeats")
    );
    let spans = lay_out("VImaj9 | V7b9 | i9 | vii9 III7b9", &am, 0, 8, 3840).unwrap();
    let names: Vec<String> = spans.iter().map(|s| s.chord.name()).collect();
    assert_eq!(names[..5], ["Fmaj9", "E7b9", "Am9", "Gm9", "C7b9"]);
    assert_eq!(
        (spans[3].start, spans[3].end, spans[4].start),
        (11520, 13440, 13440)
    );
    assert_eq!(spans.len(), 10);
    // A chord held over the bar line is one.
    assert_eq!(lay_out("Am | Am | F", &am, 0, 3, 3840).unwrap().len(), 2);
    assert!(lay_out("Am | Xq", &am, 0, 2, 3840).is_err());
}

#[test]
fn grooves_have_their_feel_fills_and_rolls() {
    let (steps, rows) = drums(groove("boom bap"), 4, 4, 1);
    assert_eq!(steps, 16);
    let kick = &rows.iter().find(|r| r.0 == 36).unwrap().1;
    assert!(kick[0] > 90 && kick.len() == 64);
    // The fill: the fourth bar's back half rises (on snare or toms), and a crash would follow.
    let at = |i: usize| rows.iter().map(|r| r.1[i]).max().unwrap();
    assert!(at(62) > at(56) && at(56) >= 70);
    // Trap: on a 32nd grid, its hats rolled at the end of bar two.
    let (steps, rows) = drums(groove("trap"), 2, 0, 2);
    assert_eq!(steps, 32);
    let hats = &rows.iter().find(|r| r.0 == 42).unwrap().1;
    assert!(hats[56..64].iter().filter(|&&v| v > 0).count() >= 4);
    assert_eq!(groove("deep house").id, "house");
    assert_eq!(groove("unknown").id, "boom_bap");
}

#[test]
fn notes_are_named_as_chords() {
    let w = |pcs: &[u8]| pcs.iter().map(|&p| (p, 1.0)).collect::<Vec<_>>();
    assert_eq!(
        name_notes(&w(&[9, 0, 4, 7]), Some(9)).unwrap().name(),
        "Am7"
    );
    assert_eq!(
        name_notes(&w(&[5, 9, 0, 4]), Some(5)).unwrap().name(),
        "Fmaj7"
    );
    assert_eq!(
        name_notes(&w(&[7, 11, 2, 5]), Some(7)).unwrap().name(),
        "G7"
    );
}
