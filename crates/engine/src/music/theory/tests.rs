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
fn voicings_move_little_from_chord_to_chord() {
    let am = key("A minor");
    let spans = lay_out("i9 | iv9 | VII9 | IIImaj9", &am, 0, 4, 3840).unwrap();
    let mut prev: Option<Vec<u8>> = None;
    for s in &spans {
        let v = voice(&s.chord, prev.as_deref(), Voicing::Rootless, (55, 79));
        assert!(v.iter().all(|&p| (55..=79).contains(&p)), "{v:?}");
        assert!(v.iter().all(|&p| s.chord.has(p)));
        if let Some(p) = &prev {
            // Each voice steps a few semitones, not leaps.
            let moved: u32 = v
                .iter()
                .zip(p)
                .map(|(a, b)| (*a as i32 - *b as i32).unsigned_abs())
                .sum();
            assert!(moved <= 4 * 5, "{p:?} to {v:?}");
        }
        prev = Some(v);
    }
    // Open: the root an octave or so under the rest.
    let v = voice(
        &Chord::parse("Fmaj9").unwrap(),
        None,
        Voicing::Open,
        (58, 79),
    );
    assert_eq!(v[0] % 12, 5);
    assert!(v[1] >= v[0] + 3);
}

#[test]
fn basslines_follow_the_chords() {
    let am = key("A minor");
    let spans = lay_out("Fmaj9 | E7b9 | Am9 | Gm9 C7b9", &am, 0, 4, 3840).unwrap();
    for style in [
        BassStyle::Roots,
        BassStyle::RootFifth,
        BassStyle::Walking,
        BassStyle::Syncopated,
        BassStyle::Octaves,
        BassStyle::Offbeat,
    ] {
        let notes = bassline(&spans, style, (28, 50), 960, 0, &[], &am, 7);
        assert!(!notes.is_empty(), "{style:?}");
        assert!(
            notes.iter().all(|n| (28..=50).contains(&n.pitch)),
            "{style:?}"
        );
        // On each chord's downbeat, its root.
        for s in &spans {
            if let Some(n) = notes.iter().find(|n| n.start as u64 == s.start)
                && style != BassStyle::Offbeat
            {
                assert_eq!(n.pitch % 12, s.chord.root, "{style:?} at {}", s.start);
            }
        }
    }
    // On the kick: a note at each kick, the chord's root.
    let kicks = [(0, 110), (1440, 90), (3840, 110), (7680, 110)];
    let notes = bassline(&spans, BassStyle::Kick, (24, 43), 960, 0, &kicks, &am, 1);
    assert_eq!(
        notes.iter().map(|n| n.start).collect::<Vec<_>>(),
        [0, 1440, 3840, 7680]
    );
    assert_eq!(notes[2].pitch % 12, 4);
}

#[test]
fn melodies_land_on_chord_tones_and_stay_in_range() {
    let am = key("A minor");
    let spans = lay_out("i9 | iv9 | VII9 | IIImaj9", &am, 0, 8, 3840).unwrap();
    for style in [
        MelodyStyle::Hook,
        MelodyStyle::Riff,
        MelodyStyle::Arp,
        MelodyStyle::Counter,
    ] {
        for seed in 0..5 {
            let notes = melody(&spans, &am, style, (62, 86), 0.7, 960, 0, 8, seed);
            assert!(!notes.is_empty(), "{style:?}");
            assert!(notes.iter().all(|n| (62..=86).contains(&n.pitch)));
            // Every note on a beat is a tone of the chord there.
            for n in notes.iter().filter(|n| n.start % 960 == 0) {
                let s = spans
                    .iter()
                    .find(|s| s.start as i64 <= n.start && n.start < s.end as i64)
                    .unwrap();
                assert!(
                    s.chord.has(n.pitch),
                    "{style:?} seed {seed}: {} on {}",
                    n.pitch,
                    s.chord.name()
                );
            }
            // And everything else is in the key, or a chord tone.
            for n in &notes {
                let s = spans
                    .iter()
                    .find(|s| s.start as i64 <= n.start && n.start < s.end as i64)
                    .unwrap();
                assert!(
                    am.contains(n.pitch) || s.chord.has(n.pitch),
                    "{style:?}: {}",
                    n.pitch
                );
            }
        }
    }
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
