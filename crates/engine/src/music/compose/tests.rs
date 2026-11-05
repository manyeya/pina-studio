use super::*;
use crate::song::project::{Content, Instrument, Region, Track};

fn song() -> Project {
    let mut p = Project::new(48_000);
    p.key = Some("A minor".into());
    for (name, instrument) in [
        ("Keys", Instrument::Synth),
        ("Bass", Instrument::Synth),
        ("Drums", Instrument::Drums),
        ("Lead", Instrument::Synth),
    ] {
        p.tracks.push(Track {
            name: name.into(),
            instrument: Some(instrument),
            ..Default::default()
        });
    }
    p
}

#[test]
fn the_chord_track_is_set_and_replaced_in_place() {
    let mut p = song();
    let out = set_chords(&mut p, Some("i9 | VImaj9 | III | VII"), None, Some(8)).unwrap();
    assert_eq!(out["chords"][1], "bar 2: Fmaj9");
    assert_eq!(p.chords.len(), 8);
    // A second pass over bars 5 to 6 replaces only those.
    set_chords(&mut p, Some("Dm7 | E7"), Some(4 * 3840), None).unwrap();
    let names: Vec<&str> = p.chords.iter().map(|c| c.chord.as_str()).collect();
    assert_eq!(names, ["Am9", "Fmaj9", "C", "G", "Dm7", "E7", "C", "G"]);
}

#[test]
fn no_part_is_written_from_a_template() {
    // The parts are the agent's to write: there are no commands that write them for it.
    use crate::song::commands::Command;
    for op in [
        "write_drums",
        "write_bass",
        "write_melody",
        "write_chords",
        "arrange",
    ] {
        let json = format!(r#"{{"op": "{op}", "track": 0}}"#);
        assert!(serde_json::from_str::<Command>(&json).is_err(), "{op}");
    }
}

#[test]
fn chords_are_heard_from_the_notes_without_a_chord_track() {
    let mut p = song();
    let notes = [57, 60, 64, 67]
        .iter()
        .map(|&pitch| Note {
            start: 0,
            length: 3840,
            pitch,
            velocity: 90,
            ..Default::default()
        })
        .chain([53, 57, 60, 64].iter().map(|&pitch| Note {
            start: 3840,
            length: 3840,
            pitch,
            velocity: 90,
            ..Default::default()
        }))
        .collect();
    p.tracks[0].regions.push(Region {
        start: 0,
        length: 7680,
        content: Content::Midi {
            notes,
            controls: Vec::new(),
        },
        ..Default::default()
    });
    let heard: Vec<String> = heard_chords(&p, 0, 7680)
        .iter()
        .map(|s| s.chord.name())
        .collect();
    assert_eq!(heard, ["Am7", "Fmaj7"]);
}
