use super::*;
use crate::song::project::Track;

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
fn a_song_is_written_from_its_chords() {
    let mut p = song();
    let out = set_chords(&mut p, Some("i9 | VImaj9 | III | VII"), None, Some(8)).unwrap();
    assert_eq!(out["chords"][1], "bar 2: Fmaj9");
    assert_eq!(p.chords.len(), 8);
    // A second pass over bars 5 to 6 replaces only those.
    set_chords(&mut p, Some("Dm7 | E7"), Some(4 * 3840), None).unwrap();
    let names: Vec<&str> = p.chords.iter().map(|c| c.chord.as_str()).collect();
    assert_eq!(names, ["Am9", "Fmaj9", "C", "G", "Dm7", "E7", "C", "G"]);

    let out = write_chords(
        &mut p,
        ChordPart {
            track: 0,
            start: None,
            bars: None,
            chords: None,
            voicing: None,
            rhythm: Some(ChordRhythm::Push),
            register: None,
            seed: None,
        },
    )
    .unwrap();
    assert_eq!(out["region"], json!([0, 0]));
    assert_eq!(p.tracks[0].regions[0].length, 8 * 3840);
    let drums = write_drums(
        &mut p,
        DrumPart {
            track: 2,
            start: None,
            bars: None,
            style: Some("trap".into()),
            fill_every: None,
            seed: Some(1),
        },
    )
    .unwrap();
    assert_eq!(drums["groove"], "trap");
    assert!(drums["fills"].is_string());
    // Trap's open hat is on the kit; nothing's skipped.
    assert!(drums["not_played"].is_null(), "{drums}");
    let out = write_bass(
        &mut p,
        BassPart {
            track: 1,
            start: None,
            bars: None,
            style: Some(BassStyle::Kick),
            register: None,
            seed: None,
            kick_track: None,
        },
    )
    .unwrap();
    assert!(out["notes"].as_u64().unwrap() >= 16);
    // Each 808 note starts with a kick, on its chord's root.
    let kicks: Vec<i64> = heard_notes(&p, 2)
        .iter()
        .filter(|n| n.pitch == 36)
        .map(|n| n.start)
        .collect();
    for n in heard_notes(&p, 1) {
        assert!(kicks.contains(&n.start));
        let chord = chord_track(&p, n.start as u64, n.start as u64 + 1)[0].chord;
        assert_eq!(n.pitch % 12, chord.root);
    }
    write_melody(
        &mut p,
        MelodyPart {
            track: 3,
            start: None,
            bars: None,
            style: None,
            register: None,
            density: None,
            seed: Some(4),
        },
    )
    .unwrap();
    assert!(!heard_notes(&p, 3).is_empty());
    // Written again, a part replaces itself.
    let again = write_chords(
        &mut p,
        ChordPart {
            track: 0,
            start: None,
            bars: None,
            chords: None,
            voicing: Some(Voicing::Spread),
            rhythm: None,
            register: None,
            seed: None,
        },
    )
    .unwrap();
    assert_eq!(again["replaced"], 1);
    assert_eq!(p.tracks[0].regions.len(), 1);
    // Drums don't go on a synth, nor chords on a kit.
    assert!(
        write_drums(
            &mut p,
            DrumPart {
                track: 0,
                start: None,
                bars: None,
                style: None,
                fill_every: None,
                seed: None
            }
        )
        .is_err()
    );
    assert!(
        write_chords(
            &mut p,
            ChordPart {
                track: 2,
                start: None,
                bars: None,
                chords: None,
                voicing: None,
                rhythm: None,
                register: None,
                seed: None
            }
        )
        .is_err()
    );
}

#[test]
fn parts_are_commands() {
    use crate::song::commands::Command;
    let c: Command =
        serde_json::from_str(r#"{"op": "write_drums", "track": 2, "style": "trap"}"#).unwrap();
    assert!(matches!(c, Command::WriteDrums(DrumPart { track: 2, .. })));
    let c: Command =
        serde_json::from_str(r#"{"op": "write_bass", "track": 1, "style": "kick"}"#).unwrap();
    assert!(matches!(
        c,
        Command::WriteBass(BassPart {
            style: Some(BassStyle::Kick),
            ..
        })
    ));
    assert!(
        serde_json::from_str::<Command>(r#"{"op": "write_chords", "track": 0, "colour": 1}"#)
            .is_err()
    );
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
        })
        .chain([53, 57, 60, 64].iter().map(|&pitch| Note {
            start: 3840,
            length: 3840,
            pitch,
            velocity: 90,
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
    let out = write_bass(
        &mut p,
        BassPart {
            track: 1,
            start: None,
            bars: Some(2),
            style: None,
            register: None,
            seed: None,
            kick_track: None,
        },
    )
    .unwrap();
    assert_eq!(out["notes"], 2);
}
