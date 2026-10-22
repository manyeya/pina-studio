//! The song as a file: saved and read back, its ids, time, key and time signature.

use super::*;

#[test]
fn json_round_trips_and_reads_sparse_tracks() {
    let json = r#"{
        "tempo": 120, "sample_rate": 48000,
        "tracks": [
            { "name": "Keys", "instrument": "synth",
              "regions": [{ "start": 0, "length": 3840,
                            "midi": { "notes": [{ "start": 0, "length": 480, "pitch": 60, "velocity": 100 }] } }] },
            { "name": "Drums", "volume_db": -3,
              "regions": [{ "start": 960, "length": 1920, "audio": { "file": "Audio Files/drums.wav" } }] }
        ]
    }"#;
    let p: Project = serde_json::from_str(json).unwrap();
    assert_eq!(p.tracks[0].instrument, Some(Instrument::Synth));
    assert_eq!(p.tracks[1].instrument, None);
    assert!(matches!(
        p.tracks[1].regions[0].content,
        Content::Audio { offset: 0, .. }
    ));
    assert_eq!(p.end(), 3840);
    let again: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(p, again);
}

#[test]
fn a_follower_follows_its_track_after_the_song_is_saved_and_read() {
    let mut p: Project = serde_json::from_str(
        r#"{"tempo": 120, "sample_rate": 48000,
            "tracks": [{"name": "Kick"}, {"name": "Bass"}]}"#,
    )
    .unwrap();
    for _ in 0..5 {
        next_id();
    }
    p.assign_ids();
    let kick = p.tracks[0].id;
    p.tracks[1].modulators.push(Modulator {
        kind: ModKind::follower(kick),
        routes: vec![],
    });
    let saved = serde_json::to_string(&p.portable()).unwrap();
    let mut read: Project = serde_json::from_str(&saved).unwrap();
    read.assign_ids();
    read.resolve_sources();
    assert_eq!(
        read.tracks[1].modulators[0].kind,
        ModKind::follower(read.tracks[0].id)
    );
}

#[test]
fn a_sidechain_names_its_track_wherever_the_song_goes() {
    // Ids are a session's own: the drums' here aren't the drums' after it's saved and read
    // elsewhere, so a sidechain is saved as the track's number.
    let mut p: Project = serde_json::from_str(
        r#"{"tempo": 120, "sample_rate": 48000,
            "tracks": [{"name": "Drums"}, {"name": "Bass"}, {"name": "Pad"}]}"#,
    )
    .unwrap();
    for _ in 0..5 {
        next_id(); // as an app that's opened other songs has used ids up
    }
    p.assign_ids();
    let drums = p.tracks[0].id;
    p.tracks[2].eq = Some(Eq {
        sidechain: Some(drums),
        ..Eq::default()
    });
    let saved = serde_json::to_string(&p.portable()).unwrap();
    let mut read: Project = serde_json::from_str(&saved).unwrap();
    read.assign_ids();
    read.resolve_sources();
    assert_ne!(read.tracks[0].id, drums);
    assert_eq!(
        read.tracks[2].eq.as_ref().unwrap().sidechain,
        Some(read.tracks[0].id)
    );
    // A command names it by its number, as commands do every track.
    let dir = std::env::temp_dir();
    let set: crate::song::commands::Command = serde_json::from_value(serde_json::json!({
        "op": "set_eq", "channel": { "track": 1 }, "eq": { "sidechain": 0, "bands": [] }
    }))
    .unwrap();
    crate::song::commands::apply(&mut read, &dir, set).unwrap();
    assert_eq!(
        read.tracks[1].eq.as_ref().unwrap().sidechain,
        Some(read.tracks[0].id)
    );
}

#[test]
fn ticks_and_samples() {
    let p = Project::new(48_000);
    assert_eq!(p.samples(PPQ, 48_000.0), 24_000); // a beat at 120 bpm is half a second
    assert_eq!(p.ticks(24_000), PPQ);
}

#[test]
fn time_signatures_set_the_bar() {
    let mut p = Project::new(48_000);
    assert_eq!((p.beat(), p.bar()), (PPQ, 4 * PPQ));
    p.time_signature = [6, 8];
    assert_eq!(
        (p.beat(), p.bar(), p.beats_per_bar()),
        (PPQ / 2, 3 * PPQ, 6)
    );
    p.time_signature = [3, 4];
    assert_eq!(p.bar(), 3 * PPQ);
    let json = serde_json::to_string(&p).unwrap();
    assert!(json.contains("\"time_signature\":[3,4]"));
    assert!(
        !serde_json::to_string(&Project::new(48_000))
            .unwrap()
            .contains("time_signature")
    );
}

#[test]
fn changing_the_key_can_move_the_music_into_it() {
    let mut p = project(vec![midi(0, 960, &[(0, 60), (240, 64), (480, 67)])], true);
    p.add_track(
        1,
        Track {
            instrument: Some(Instrument::Drums),
            regions: vec![midi(0, 960, &[(0, 36)])],
            ..Default::default()
        },
    );
    let key = |s: &str| crate::music::Key::parse(s);
    let pitches = |p: &Project, t: usize| match &p.tracks[t].regions[0].content {
        Content::Midi { notes, .. } => notes.iter().map(|n| n.pitch).collect::<Vec<_>>(),
        _ => unreachable!(),
    };
    assert_eq!(p.song_key(), key("C major")); // a C major chord
    // Just naming it: nothing moves.
    let mut named = p.clone();
    named.set_key(key("D major"), false);
    assert_eq!(pitches(&named, 0), [60, 64, 67]);
    // Moving into D major: a D major chord; into C minor: C minor's. Drums stay.
    let mut d = p.clone();
    d.set_key(key("D major"), true);
    assert_eq!(
        (pitches(&d, 0), d.key.as_deref()),
        (vec![62, 66, 69], Some("D major"))
    );
    assert_eq!(pitches(&d, 1), [36]);
    p.set_key(key("C minor"), true);
    assert_eq!(pitches(&p, 0), [60, 63, 67]);
    // Back to working it out: nothing moves.
    p.set_key(None, true);
    assert_eq!((pitches(&p, 0), p.key.clone()), (vec![60, 63, 67], None));
}
