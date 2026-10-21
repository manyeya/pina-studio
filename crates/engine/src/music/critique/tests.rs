use super::*;
use crate::{
    music::compose::{self, BassPart, ChordPart, DrumPart},
    song::project::{Region, Track},
};

fn song() -> Project {
    let mut p = Project::new(48_000);
    p.key = Some("A minor".into());
    p.tempo = 140.0;
    for (name, i) in [
        ("Drums", Instrument::Drums),
        ("808", Instrument::Synth),
        ("Keys", Instrument::Synth),
    ] {
        p.tracks.push(Track {
            name: name.into(),
            instrument: Some(i),
            ..Default::default()
        });
    }
    compose::set_chords(&mut p, Some("i | VI | III | VII"), None, Some(8)).unwrap();
    compose::write_drums(
        &mut p,
        DrumPart {
            track: 0,
            start: None,
            bars: None,
            style: Some("trap".into()),
            fill_every: None,
            seed: Some(3),
        },
    )
    .unwrap();
    compose::write_bass(
        &mut p,
        BassPart {
            track: 1,
            start: None,
            bars: None,
            style: Some(crate::music::theory::BassStyle::Kick),
            register: None,
            seed: None,
            kick_track: None,
        },
    )
    .unwrap();
    compose::write_chords(
        &mut p,
        ChordPart {
            track: 2,
            start: None,
            bars: None,
            chords: None,
            voicing: None,
            rhythm: None,
            register: None,
            seed: None,
        },
    )
    .unwrap();
    p
}

fn has(issues: &[Issue], area: &str, words: &str) -> bool {
    issues
        .iter()
        .any(|i| i.area == area && i.what.contains(words))
}

#[test]
fn a_written_song_is_clean_where_tempo_wrote_it() {
    let issues = music(&song());
    assert!(!issues.iter().any(|i| i.area == "harmony"), "{issues:#?}");
    assert!(
        !issues
            .iter()
            .any(|i| i.area == "rhythm" && i.what.contains("flam")),
        "{issues:#?}"
    );
    // But it's a loop on placeholder sounds with no genre.
    assert!(has(&issues, "sound", "sine"));
    assert!(has(&issues, "setup", "genre"));
    assert!(has(&issues, "arrangement", "a loop"));
}

#[test]
fn wrong_notes_clashes_and_flams_are_heard() {
    let mut p = song();
    // A Bb (out of A minor) held over every bar, a semitone over the A in the keys.
    let wrong: Vec<Note> = (0..8)
        .map(|b| Note {
            start: b * 3840,
            length: 3840,
            pitch: 70,
            velocity: 90,
        })
        .collect();
    p.tracks.push(Track {
        name: "Lead".into(),
        instrument: Some(Instrument::Synth),
        ..Default::default()
    });
    p.tracks[3].regions.push(Region {
        start: 0,
        length: 8 * 3840,
        content: Content::Midi {
            notes: wrong,
            controls: Vec::new(),
        },
        ..Default::default()
    });
    // The 808 a little late on every kick.
    if let Content::Midi { notes, .. } = &mut p.tracks[1].regions[0].content {
        notes.iter_mut().for_each(|n| n.start += 40);
    }
    let issues = critique(&p, None);
    assert!(has(&issues, "harmony", "outside the key"), "{issues:#?}");
    assert!(has(&issues, "rhythm", "just off a kick"), "{issues:#?}");
    assert_eq!(issues[0].level, "fix");
}

#[test]
fn a_long_song_that_never_changes_is_flat() {
    let mut p = song();
    p.genre = Some("trap".into());
    crate::music::arrange::copy_bars(&mut p, 0, 8, 8 * 3840, None).unwrap();
    crate::music::arrange::copy_bars(&mut p, 0, 16, 16 * 3840, None).unwrap();
    let issues = music(&p);
    assert!(
        has(&issues, "arrangement", "nothing comes or goes"),
        "{issues:#?}"
    );
    assert!(has(&issues, "arrangement", "no sections"));
    // Arranged, it isn't.
    let mut p = song();
    p.genre = Some("trap".into());
    crate::music::arrange::arrange(
        &mut p,
        crate::music::arrange::Arrangement {
            from: None,
            bars: Some(8),
            sections: None,
            roles: None,
        },
    )
    .unwrap();
    let issues = music(&p);
    assert!(
        !has(&issues, "arrangement", "nothing comes or goes"),
        "{issues:#?}"
    );
    assert!(has(&issues, "arrangement", "no transitions"));
}

#[test]
fn bars_read_as_runs() {
    assert_eq!(bars(&[0, 1, 2, 4, 8, 9]), "bars 1-3, 5, 9-10");
    assert_eq!(bars(&[3]), "bar 4");
}
