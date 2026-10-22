//! Tracks: stacks gathered and flattened, regions only where they can play.

use super::*;

#[test]
fn a_stack_gathers_its_tracks_and_flattening_it_lets_them_go() {
    let mut p: Project = serde_json::from_str(
        r#"{"tempo": 120, "sample_rate": 48000, "tracks": [
            {"name": "Kick"}, {"name": "Bass"}, {"name": "Snare"}, {"name": "Pad"}, {"name": "Hats"}]}"#,
    )
    .unwrap();
    p.assign_ids();
    let (bus, moved) = p.add_stack(&[0, 2, 4]);
    let names: Vec<&str> = p.tracks.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Kick", "Snare", "Hats", "Bass", "Pad"]);
    assert_eq!(moved, [0, 3, 1, 4, 2]);
    p.add_shared(bus, ModKind::mac());
    p.flatten_stack(bus);
    assert!(p.auxes.is_empty());
    assert!(
        p.tracks
            .iter()
            .all(|t| t.output.is_none() && t.modulators.is_empty())
    );
}

#[test]
fn regions_only_move_to_tracks_that_can_play_them() {
    let mut p = project(vec![midi(0, 960, &[])], true);
    p.add_track(
        1,
        Track {
            name: "audio".into(),
            ..Default::default()
        },
    );
    p.add_track(
        2,
        Track {
            name: "keys".into(),
            instrument: Some(Instrument::Synth),
            ..Default::default()
        },
    );
    assert_eq!(p.move_region((0, 0), 1, 480), (0, 0));
    assert_eq!(p.move_region((0, 0), 2, 480), (2, 0));
    assert_eq!(p.region((2, 0)).start, 480);
}
