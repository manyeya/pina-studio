use pina_engine::song::project::{Content, Instrument, Note, Project, Region, Track};

use super::{Take, Target, put_take, sheet::control_name};

fn note(start: i64, pitch: u8) -> Note {
    Note {
        start,
        length: 240,
        pitch,
        velocity: 100,
        ..Default::default()
    }
}

fn notes_of(r: &Region) -> Vec<(i64, u8)> {
    match &r.content {
        Content::Midi { notes, .. } => notes.iter().map(|n| (n.start, n.pitch)).collect(),
        _ => Vec::new(),
    }
}

#[test]
fn a_take_kept_takes_its_bars_and_leaves_either_side() {
    let mut p = Project::new(48_000);
    let bar = p.bar() as i64;
    // Eight bars of bass, a note at the start of each.
    p.tracks.push(Track {
        id: 7,
        name: "Bass".into(),
        instrument: Some(Instrument::Synth),
        regions: vec![Region {
            start: 0,
            length: 8 * bar as u64,
            content: Content::Midi {
                notes: (0..8).map(|b| note(b * bar, 36 + b as u8)).collect(),
                controls: Vec::new(),
            },
            ..Default::default()
        }],
        ..Default::default()
    });
    let target = Target {
        tracks: vec![7],
        bars: (2, 4),
    };
    let take = Take {
        seed: 3,
        notes: vec![(7, vec![note(2 * bar + 480, 60), note(3 * bar, 62)])],
    };
    put_take(&mut p, &target, &take);
    let mut regions: Vec<&Region> = p.tracks[0].regions.iter().collect();
    regions.sort_by_key(|r| r.start);
    let spans: Vec<(u64, u64)> = regions.iter().map(|r| (r.start, r.length)).collect();
    let b = bar as u64;
    assert_eq!(spans, [(0, 2 * b), (2 * b, 2 * b), (4 * b, 4 * b)]);
    // Before the bars and after them, the bass as it was; in them, the take.
    assert_eq!(notes_of(regions[0]), [(0, 36), (bar, 37)]);
    assert_eq!(notes_of(regions[1]), [(480, 60), (bar, 62)]);
    assert_eq!(regions[1].name, "Composed");
    assert_eq!(notes_of(regions[2]).len(), 4);
    assert_eq!(notes_of(regions[2])[0], (0, 40));
}

#[test]
fn a_take_on_an_empty_track_is_a_region_of_its_own() {
    let mut p = Project::new(48_000);
    let bar = p.bar() as i64;
    p.tracks.push(Track {
        id: 2,
        instrument: Some(Instrument::Synth),
        ..Default::default()
    });
    let take = Take {
        seed: 1,
        notes: vec![(2, vec![note(bar, 50)]), (99, vec![note(bar, 51)])],
    };
    put_take(
        &mut p,
        &Target {
            tracks: vec![2, 99],
            bars: (1, 2),
        },
        &take,
    );
    // A track that's gone since the take was written is left be.
    assert_eq!(p.tracks[0].regions.len(), 1);
    assert_eq!(notes_of(&p.tracks[0].regions[0]), [(0, 50)]);
    assert_eq!(take.count(), 2);
}

#[test]
fn controls_are_named_plainly() {
    assert_eq!(control_name("max_polyphony"), "Most at once");
    assert_eq!(control_name("note_density"), "Density");
    assert_eq!(control_name("key_signature"), "Key signature");
}
