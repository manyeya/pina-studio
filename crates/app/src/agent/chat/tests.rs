use super::tuning::model_name;

#[test]
fn model_ids_read_as_names() {
    assert_eq!(model_name("claude-opus-5-5"), "Opus 5.5");
    assert_eq!(model_name("claude-sonnet-4-5-20250929"), "Sonnet 4.5");
    assert_eq!(model_name("custom"), "Custom");
}

#[test]
fn every_message_says_the_songs_tempo_meter_key_and_tracks() {
    use pina_engine::song::project::Project;
    let note = |pitch: u8, start: u64| serde_json::json!({ "start": start, "length": 960, "pitch": pitch, "velocity": 100 });
    let mut p: Project = serde_json::from_value(serde_json::json!({
        "tempo": 118.0,
        "sample_rate": 48000,
        "tracks": [
            { "name": "Keys", "instrument": "synth", "regions": [{ "start": 0, "length": 15360,
                "midi": { "notes": [note(57, 0), note(60, 960), note(64, 1920), note(57, 2880)] } }] },
            { "name": "Synth loop", "regions": [{ "start": 0, "length": 3840, "transpose": -9,
                "audio": { "file": "Audio Files/loop.wav" } }] }
        ]
    }))
    .unwrap();
    // No key set: what its notes read as, said so.
    let line = super::asking::song_line(&p);
    assert!(
        line.starts_with("(The song: 118 BPM, 4/4, no key set (its notes read as A minor), 4 bars"),
        "{line}"
    );
    assert!(
        line.contains("0 Keys, 1 Synth loop (audio, 9 semitones down)"),
        "{line}"
    );
    // Set: as set.
    p.key = Some("Bb minor".into());
    assert!(super::asking::song_line(&p).contains("4/4, Bb minor, 4 bars"));
    // Nothing to go on.
    p.tracks.clear();
    p.key = None;
    let empty = super::asking::song_line(&p);
    assert!(
        empty.contains("no key set,") && empty.contains("tracks: none yet"),
        "{empty}"
    );
}
