//! Take folders: recorded, comped, split, trimmed and flattened.

use super::*;

#[test]
fn recording_over_a_region_makes_a_take_folder() {
    let mut p = project(vec![midi(960, 1920, &[(0, 60)])], true);
    let notes = vec![Note {
        start: 0,
        length: 240,
        pitch: 64,
        velocity: 90,
    }];
    let at = p.add_take(
        0,
        1440,
        3360,
        Content::Midi {
            notes,
            controls: Vec::new(),
        },
    );
    assert_eq!(at, (0, 0));
    let r = p.region(at);
    assert_eq!((r.start, r.length), (960, 2400)); // grown to the end of the new take
    assert_eq!(folder(&p, at), (vec![0, 480], vec![(0, 2400, 1)])); // the newest plays
    // Recording somewhere else makes a plain region.
    assert_eq!(
        p.add_take(
            0,
            7680,
            8640,
            Content::Midi {
                notes: vec![],
                controls: Vec::new()
            }
        ),
        (0, 1)
    );
}

#[test]
fn comping_overlays_ranges_and_merges_neighbours() {
    let takes = vec![take(0, 60), take(0, 62), take(0, 64)];
    let region = Region {
        start: 0,
        length: 3840,
        content: Content::Takes {
            takes,
            comp: vec![],
        },
        ..Default::default()
    };
    let mut p = project(vec![region], true);
    p.comp((0, 0), 960, 1920, 0).unwrap();
    assert_eq!(
        folder(&p, (0, 0)).1,
        [(0, 960, 2), (960, 1920, 0), (1920, 3840, 2)]
    );
    p.comp((0, 0), 1920, 2880, 0).unwrap();
    assert_eq!(
        folder(&p, (0, 0)).1,
        [(0, 960, 2), (960, 2880, 0), (2880, 3840, 2)]
    );
    p.comp((0, 0), 0, 3840, 1).unwrap();
    assert_eq!(folder(&p, (0, 0)).1, [(0, 3840, 1)]);
    assert!(p.comp((0, 0), 0, 960, 7).is_err());
}

#[test]
fn take_folders_split_trim_and_flatten() {
    // Take 1 starts at 480 with a note 600 in (at 1080), where the comp plays take 1.
    let mut late = take(480, 62);
    let Content::Midi { notes, .. } = &mut late.content else {
        unreachable!()
    };
    notes[0].start = 600;
    let takes = vec![take(0, 60), late];
    let comp = vec![
        CompPiece { from: 0, take: 0 },
        CompPiece { from: 960, take: 1 },
    ];
    let region = Region {
        start: 0,
        length: 1920,
        content: Content::Takes { takes, comp },
        ..Default::default()
    };
    let mut p = project(vec![region], true);
    let at = p.split_region((0, 0), 1440).unwrap();
    assert_eq!(folder(&p, (0, 0)).1, [(0, 960, 0), (960, 1440, 1)]);
    assert_eq!(folder(&p, at), (vec![-1440, -960], vec![(0, 480, 1)]));

    p.trim_start((0, 0), 480); // takes stay put on the timeline
    assert_eq!(
        folder(&p, (0, 0)),
        (vec![-480, 0], vec![(0, 480, 0), (480, 960, 1)])
    );

    // Flattening keeps what the comp plays: take 1's note (1080, now 600 after the trim), not
    // take 0's (now -480, before the region).
    let flat = p.flatten((0, 0));
    let Content::Midi { notes, .. } = &p.region(flat[0]).content else {
        panic!()
    };
    let kept: Vec<(i64, u8)> = notes.iter().map(|n| (n.start, n.pitch)).collect();
    assert_eq!(kept, [(600, 62)]);
}
