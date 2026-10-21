use super::*;
use crate::samples::{KeyGuess, Pitch};

fn key(s: &str) -> Key {
    Key::parse(s).unwrap()
}

#[test]
fn previews_move_into_a_key_the_nearest_way() {
    let note = |n: u8, cents: f32| Analysis {
        category: Category::Bass808,
        pitch: Some(Pitch {
            note: n,
            cents,
            hz: 0.0,
            confidence: 0.9,
        }),
        ..Default::default()
    };
    // G up to C is 5; F# down to C is 6 (not up 6... either way, within six).
    assert_eq!(transpose_to(&note(31, 0.0), key("C minor")), 5.0);
    assert_eq!(transpose_to(&note(42, 0.0), key("C minor")).abs(), 6.0);
    // A little sharp comes down a little.
    assert!((transpose_to(&note(36, 20.0), key("C major")) + 0.2).abs() < 1e-4);
    // A loop in E minor goes to A minor, or to C major's relative.
    let loop_in = |name: &str, tonic: u8, minor: bool| Analysis {
        category: Category::Keys,
        key: Some(KeyGuess {
            tonic,
            minor,
            name: name.into(),
            confidence: 0.9,
        }),
        ..Default::default()
    };
    assert_eq!(
        transpose_to(&loop_in("E minor", 4, true), key("A minor")),
        5.0
    );
    assert_eq!(
        transpose_to(&loop_in("E minor", 4, true), key("C major")),
        5.0
    );
    // Drums stay.
    let kick = Analysis {
        category: Category::Kick,
        ..note(36, 0.0)
    };
    assert_eq!(transpose_to(&kick, key("D minor")), 0.0);
}

#[test]
fn loops_last_their_beats_at_the_songs_tempo() {
    let at = |bpm: f32, seconds: f32| Analysis {
        bpm: Some(bpm),
        seconds,
        ..Default::default()
    };
    // Four beats at 90: a bar at any tempo.
    assert_eq!(
        loop_ticks(&at(90.0, 4.0 * 60.0 / 90.0), 120.0),
        Some(PPQ * 4)
    );
    // Double time at 180 in a song at 90: still four of the song's beats... of 180's eight.
    assert_eq!(
        loop_ticks(&at(180.0, 8.0 * 60.0 / 180.0), 90.0),
        Some(PPQ * 4)
    );
    // Two bars at 120 in a song at 84: slowed, still two bars (not sped up as half time).
    assert_eq!(
        loop_ticks(&at(120.0, 8.0 * 60.0 / 120.0), 84.0),
        Some(PPQ * 8)
    );
    // At 140 in a song at 84: half time, its two bars one of the song's.
    assert_eq!(
        loop_ticks(&at(140.0, 8.0 * 60.0 / 140.0), 84.0),
        Some(PPQ * 4)
    );
    assert_eq!(loop_ticks(&Analysis::default(), 120.0), None);
}

#[test]
fn related_keys_are_the_relative_and_the_neighbours_by_fifths() {
    let names: Vec<String> = related(key("A minor")).iter().map(|k| k.name()).collect();
    assert_eq!(names, ["A minor", "C major", "E minor", "D minor"]);
}
