use super::*;
use crate::song::project::PPQ;

fn n(start: i64, length: u64, pitch: u8) -> Note {
    Note {
        start,
        length,
        pitch,
        velocity: 100,
    }
}

#[test]
fn keys_read_and_write() {
    let am = Key {
        root: 9,
        minor: true,
    };
    for s in ["A minor", "Am", "a min", "A Minor"] {
        assert_eq!(Key::parse(s), Some(am), "{s}");
    }
    assert_eq!(Key::parse("Eb major").unwrap().name(), "Eb major");
    assert_eq!(Key::parse("F#").unwrap().name(), "F# major");
    assert_eq!(Key::parse("Bbm").unwrap().short(), "Bb min");
    assert_eq!(Key::parse("H"), None);
    assert!(am.contains(72) && am.contains(69) && !am.contains(73));
    assert_eq!(Key::all().count(), 24);
}

#[test]
fn detects_the_key_of_some_notes() {
    // An A minor arpeggio and scale, leaning on A, C and E.
    let notes = [
        (69, 960),
        (72, 480),
        (76, 480),
        (71, 240),
        (74, 240),
        (77, 240),
        (79, 240),
        (69, 960),
        (64, 480),
    ];
    assert_eq!(detect(notes), Key::parse("A minor"));
    // A C major scale, starting and ending on C.
    let c = [60, 62, 64, 65, 67, 69, 71, 72, 60, 67, 64];
    assert_eq!(detect(c.map(|p| (p, 480))), Key::parse("C major"));
    assert_eq!(detect([]), None);
}

#[test]
fn quantize_with_strength_and_swing() {
    let g = PPQ as i64 / 4;
    let mut notes = vec![n(g + 30, 100, 60), n(2 * g - 50, 100, 62)];
    NoteEdit::Quantize {
        grid: g as u64,
        strength: 1.0,
        swing: 50.0,
    }
    .apply(&mut notes, &[]);
    assert_eq!((notes[0].start, notes[1].start), (g, 2 * g));

    let mut notes = vec![n(g + 40, 100, 60)];
    NoteEdit::Quantize {
        grid: g as u64,
        strength: 0.5,
        swing: 50.0,
    }
    .apply(&mut notes, &[]);
    assert_eq!(notes[0].start, g + 20);

    // 66% swing: the odd step lands about a third of a step late.
    let mut notes = vec![n(g, 100, 60), n(2 * g, 100, 60)];
    NoteEdit::Quantize {
        grid: g as u64,
        strength: 1.0,
        swing: 66.0,
    }
    .apply(&mut notes, &[]);
    assert_eq!(notes[0].start, g + (g as f64 * 0.32).round() as i64);
    assert_eq!(notes[1].start, 2 * g);
}

#[test]
fn legato_chop_and_arpeggiate() {
    let mut notes = vec![n(0, 100, 60), n(480, 100, 62), n(960, 100, 64)];
    NoteEdit::Legato.apply(&mut notes, &[]);
    assert_eq!(
        notes.iter().map(|n| n.length).collect::<Vec<_>>(),
        [480, 480, 100]
    );

    let mut notes = vec![n(0, 1000, 60)];
    NoteEdit::Chop { grid: 480 }.apply(&mut notes, &[]);
    assert_eq!(
        notes
            .iter()
            .map(|n| (n.start, n.length))
            .collect::<Vec<_>>(),
        [(0, 480), (480, 480), (960, 40)]
    );

    let mut notes = vec![n(0, 960, 64), n(0, 960, 60), n(0, 960, 67)];
    NoteEdit::Arpeggiate { grid: 240 }.apply(&mut notes, &[]);
    let mut by_pitch: Vec<(u8, i64)> = notes.iter().map(|n| (n.pitch, n.start)).collect();
    by_pitch.sort();
    assert_eq!(by_pitch, [(60, 0), (64, 240), (67, 480)]);
}

#[test]
fn edits_on_a_selection_strum_reverse_flip_glue_and_dynamics() {
    // Legato on the chosen alone, each held to the next note whichever it is.
    let mut notes = vec![n(0, 100, 60), n(480, 100, 62), n(960, 100, 64)];
    NoteEdit::Legato.apply(&mut notes, &[0, 2]);
    assert_eq!(
        notes.iter().map(|n| n.length).collect::<Vec<_>>(),
        [480, 100, 100]
    );

    let mut notes = vec![n(0, 960, 64), n(0, 960, 60), n(0, 960, 67)];
    NoteEdit::Strum { ticks: 30 }.apply(&mut notes, &[]);
    let mut by_pitch: Vec<(u8, i64, u64)> =
        notes.iter().map(|n| (n.pitch, n.start, n.length)).collect();
    by_pitch.sort();
    assert_eq!(by_pitch, [(60, 0, 960), (64, 30, 930), (67, 60, 900)]);

    let mut notes = vec![n(0, 240, 60), n(480, 480, 62)];
    NoteEdit::Reverse.apply(&mut notes, &[]);
    assert_eq!((notes[0].start, notes[1].start), (720, 0));

    let mut notes = vec![n(0, 240, 60), n(240, 240, 64), n(480, 240, 67)];
    NoteEdit::Flip.apply(&mut notes, &[]);
    assert_eq!(
        notes.iter().map(|n| n.pitch).collect::<Vec<_>>(),
        [67, 63, 60]
    );

    let mut notes = vec![
        n(0, 240, 60),
        n(240, 240, 60),
        n(600, 100, 60),
        n(0, 100, 62),
    ];
    NoteEdit::Glue.apply(&mut notes, &[]);
    assert_eq!(
        notes
            .iter()
            .map(|n| (n.start, n.length, n.pitch))
            .collect::<Vec<_>>(),
        [(0, 480, 60), (600, 100, 60), (0, 100, 62)]
    );

    let mut notes = vec![n(0, 1, 60), n(1, 1, 60)];
    notes[0].velocity = 40;
    notes[1].velocity = 120;
    NoteEdit::Dynamics { percent: 50, by: 0 }.apply(&mut notes, &[]);
    assert_eq!((notes[0].velocity, notes[1].velocity), (60, 100));
}

#[test]
fn humanize_stays_close_and_repeats_for_a_seed() {
    let edit = NoteEdit::Humanize {
        timing: 20,
        velocity: 8,
        seed: 7,
    };
    let base: Vec<Note> = (0..32).map(|i| n(1000 + i * 480, 100, 60)).collect();
    let (mut a, mut b) = (base.clone(), base.clone());
    edit.apply(&mut a, &[]);
    edit.apply(&mut b, &[]);
    assert_eq!(a, b);
    assert_ne!(a, base);
    for (x, y) in a.iter().zip(&base) {
        assert!((x.start - y.start).abs() <= 20);
        assert!((x.velocity as i32 - 100).abs() <= 8);
    }
}

#[test]
fn one_note_only() {
    let mut notes = vec![n(0, 100, 60), n(480, 100, 62)];
    NoteEdit::Transpose { semitones: 12 }.apply(&mut notes, &[1]);
    assert_eq!((notes[0].pitch, notes[1].pitch), (60, 74));
    NoteEdit::Velocity { by: 50 }.apply(&mut notes, &[]);
    assert_eq!(notes[0].velocity, 127);
}

#[test]
fn music_moves_to_another_key() {
    let key = |s: &str| Key::parse(s).unwrap();
    // C major to D major: up a tone; to G major: down a fourth, the nearer way.
    assert_eq!(key("C").move_pitch(60, &key("D")), 62);
    assert_eq!(key("C").move_pitch(64, &key("G")), 59);
    // To the parallel minor: E and A and B come down a semitone; C and G stay.
    let minor: Vec<u8> = [60, 62, 64, 65, 67, 69, 71]
        .map(|p| key("C").move_pitch(p, &key("C minor")))
        .to_vec();
    assert_eq!(minor, [60, 62, 63, 65, 67, 68, 70]);
    // A minor to C minor: the same scale steps, a minor third up.
    assert_eq!(key("Am").move_pitch(57, &key("Cm")), 60);
    assert_eq!(key("Am").move_pitch(60, &key("Cm")), 63);
    // A note outside the scale keeps its place: C# in C major is D# in D major.
    assert_eq!(key("C").move_pitch(61, &key("D")), 63);
    // And back again, it's where it was.
    for p in 40..90 {
        let there = key("C").move_pitch(p, &key("E minor"));
        if key("C").contains(p) {
            assert_eq!(key("E minor").move_pitch(there, &key("C")), p);
        }
    }
}

#[test]
fn keys_have_their_signatures_and_spell_by_them() {
    let key = |s: &str| Key::parse(s).unwrap();
    let sig = |s: &str| key(s).signature();
    assert_eq!(
        [sig("C"), sig("G"), sig("D"), sig("F"), sig("Bb"), sig("Eb")],
        [0, 1, 2, -1, -2, -3]
    );
    assert_eq!(
        [
            sig("A minor"),
            sig("E minor"),
            sig("D minor"),
            sig("C minor")
        ],
        [0, 1, -1, -3]
    );
    assert_eq!([sig("F# major"), sig("Db major")], [6, -5]);
    // Bb in F major is a B flat, in the signature; in G major, D# is a sharp outside it.
    assert_eq!(key("F").spell(70), (6, 4, -1));
    assert_eq!(key("F").signature_on(6), -1);
    assert_eq!(key("G").spell(63), (1, 4, 1));
    // In Eb major a black key outside the signature is a flat: Gb.
    assert_eq!(key("Eb").spell(66), (4, 4, -1));
    // F# major spells F as E#.
    assert_eq!(key("F# major").spell(65), (2, 4, 1));
    // C major: plain.
    assert_eq!(key("C").spell(60), (0, 4, 0));
    assert_eq!(key("C").spell(61), (0, 4, 1));
}
