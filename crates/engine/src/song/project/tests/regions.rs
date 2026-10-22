//! Regions: split, trimmed, stretched with the tempo, aliased.

use super::*;

#[test]
fn audio_that_follows_the_tempo_keeps_its_beats() {
    let mut p = Project::new(48_000);
    p.tempo = 100.0;
    // A loop at 90 stretched to 100 BPM, following; a recording that doesn't.
    let mut follows = audio(0, PPQ * 4, 0);
    if let Content::Audio {
        stretch, follow, ..
    } = &mut follows.content
    {
        (*stretch, *follow) = (0.9, true);
    }
    p.tracks.push(Track {
        regions: vec![follows, audio(0, PPQ * 4, 0)],
        ..Default::default()
    });
    let stretch = |p: &Project, r: usize| match p.tracks[0].regions[r].content {
        Content::Audio { stretch, .. } => stretch,
        _ => unreachable!(),
    };
    if let Content::Audio { warps, .. } = &mut p.tracks[0].regions[0].content {
        warps.push([1000, 900]);
    }
    p.set_tempo(120.0);
    assert_eq!((stretch(&p, 0), stretch(&p, 1)), (0.75, 1.0));
    // Its warp markers move with it: a beat still lands on its beat.
    let warp = match &p.tracks[0].regions[0].content {
        Content::Audio { warps, .. } => warps[0],
        _ => unreachable!(),
    };
    assert_eq!(warp, [1000, 750]);
    assert_eq!(p.tracks[0].regions[0].length, PPQ * 4);
    // Back where it was, exactly.
    p.set_tempo(100.0);
    assert_eq!(stretch(&p, 0), 0.9);
}

#[test]
fn splitting_midi_keeps_notes_with_the_half_they_start_in() {
    let mut p = project(
        vec![midi(960, 1920, &[(0, 60), (900, 62), (960, 64)])],
        true,
    );
    assert_eq!(p.split_region((0, 0), 960), None); // at the region's start: nothing to split
    let at = p.split_region((0, 0), 1920).unwrap();
    let (a, b) = (p.region((0, 0)), p.region(at));
    assert_eq!(
        (a.start, a.length, b.start, b.length),
        (960, 960, 1920, 960)
    );
    let pitches = |r: &Region| match &r.content {
        Content::Midi { notes, .. } => notes.iter().map(|n| (n.start, n.pitch)).collect::<Vec<_>>(),
        _ => unreachable!(),
    };
    assert_eq!(pitches(a), [(0, 60), (900, 62)]);
    assert_eq!(pitches(b), [(0, 64)]);
}

#[test]
fn splitting_midi_keeps_each_controller_where_it_stood() {
    let cc = |start, value| ControlPoint {
        start,
        kind: ControlValue::Cc { number: 1, value },
    };
    let bend = |start, value| ControlPoint {
        start,
        kind: ControlValue::PitchBend { value },
    };
    let mut p = Project::new(48_000);
    p.tracks.push(Track {
        regions: vec![Region {
            start: 0,
            length: 4 * PPQ,
            content: Content::Midi {
                notes: vec![],
                controls: vec![
                    cc(0, 10),
                    bend(100, 500),
                    cc(PPQ as i64, 90),
                    cc(3 * PPQ as i64, 127),
                ],
            },
            ..Default::default()
        }],
        ..Default::default()
    });
    let second = p.split_region((0, 0), 2 * PPQ).unwrap();
    let controls = |at| match &p.region(at).content {
        Content::Midi { controls, .. } => controls.clone(),
        _ => unreachable!(),
    };
    assert_eq!(
        controls((0, 0)),
        [cc(0, 10), bend(100, 500), cc(PPQ as i64, 90)]
    );
    // The second half starts with the wheel at 90 and the bend where it was.
    let mut start = controls(second);
    start.sort_by_key(|c| (c.start, matches!(c.kind, ControlValue::PitchBend { .. })));
    assert_eq!(start, [cc(0, 90), bend(0, 500), cc(PPQ as i64, 127)]);
}

#[test]
fn splitting_audio_moves_the_offset() {
    let mut p = project(vec![audio(0, 1920, 100)], false);
    let at = p.split_region((0, 0), 960).unwrap();
    assert!(matches!(
        p.region(at).content,
        Content::Audio { offset: 24_100, .. }
    ));
}

#[test]
fn trimming_the_start_keeps_content_in_place() {
    let mut p = project(vec![midi(960, 1920, &[(0, 60), (480, 62)])], true);
    p.trim_start((0, 0), 1440);
    let r = p.region((0, 0));
    assert_eq!((r.start, r.length), (1440, 1440));
    let Content::Midi { notes, .. } = &r.content else {
        unreachable!()
    };
    assert_eq!((notes[0].start, notes[1].start), (-480, 0));
    p.trim_start((0, 0), 960); // back again: the hidden note returns
    let Content::Midi { notes, .. } = &p.region((0, 0)).content else {
        unreachable!()
    };
    assert_eq!(notes[0].start, 0);

    // Audio can't start before its file.
    let mut p = project(vec![audio(1920, 960, 12_000)], false);
    p.trim_start((0, 0), 0);
    let r = p.region((0, 0));
    assert_eq!((r.start, r.length), (1440, 1440));
    assert!(matches!(r.content, Content::Audio { offset: 0, .. }));
}

#[test]
fn aliases_share_their_notes_until_unlinked() {
    let mut p = project(vec![midi(0, 960, &[(0, 60)])], true);
    let alias = p.alias((0, 0), 0, 3840).unwrap();
    assert!(p.region(alias).link.is_some());
    assert_eq!(p.region(alias).link, p.region((0, 0)).link);
    assert_eq!(p.region(alias).start, 3840);
    let mut links = p.link_contents();
    // Editing the alias edits the original, and the other way round.
    p.notes_mut(alias).unwrap()[0].pitch = 64;
    p.sync_links(&mut links);
    assert_eq!(p.notes_mut((0, 0)).unwrap()[0].pitch, 64);
    p.notes_mut((0, 0)).unwrap()[0].pitch = 67;
    p.sync_links(&mut links);
    assert_eq!(p.notes_mut(alias).unwrap()[0].pitch, 67);
    // Its place and playback settings are its own.
    p.tracks[0].regions[1].transpose = 12;
    p.sync_links(&mut links);
    assert_eq!(p.region((0, 0)).transpose, 0);
    // Splitting makes regions of their own; one left alone isn't an alias.
    p.split_region(alias, 3840 + 480).unwrap();
    p.sync_links(&mut links);
    assert!(p.tracks[0].regions.iter().all(|r| r.link.is_none()));
    // Audio has no aliases.
    let mut p = project(vec![audio(0, 960, 0)], false);
    assert!(p.alias((0, 0), 0, 960).is_err());
}
