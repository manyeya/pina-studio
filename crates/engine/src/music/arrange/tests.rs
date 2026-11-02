use super::*;
use crate::{
    music::compose,
    song::project::{Pattern, PatternRow},
};

/// Four bars written by hand, as an agent would: a drum pattern, an 808 on the roots, bells and
/// a pad on the chords.
fn sketch() -> Project {
    let mut p = Project::new(48_000);
    p.key = Some("A minor".into());
    compose::set_chords(&mut p, Some("i | VI | III | VII"), None, None).unwrap();
    let row = |pitch: u8, hits: &[usize]| PatternRow {
        pitch,
        cells: (0..16)
            .map(|s| if hits.contains(&s) { 100 } else { 0 })
            .collect(),
        ..Default::default()
    };
    let drums = Content::Pattern(Pattern::new(
        240,
        vec![
            row(36, &[0, 6, 10]),
            row(38, &[4, 12]),
            row(42, &[0, 2, 4, 6, 8, 10, 12, 14]),
        ],
        50.0,
    ));
    let roots = [45u8, 41, 48, 43];
    let chords = [[57u8, 60, 64], [57, 60, 65], [55, 60, 64], [55, 59, 62]];
    let notes = |f: &dyn Fn(usize) -> Vec<u8>| Content::Midi {
        notes: (0..4)
            .flat_map(|bar| {
                f(bar).into_iter().map(move |pitch| Note {
                    start: bar as i64 * 3840,
                    length: 3600,
                    pitch,
                    velocity: 90,
                })
            })
            .collect(),
        controls: Vec::new(),
    };
    for (name, instrument, content) in [
        ("Drums", Instrument::Drums, drums),
        ("808", Instrument::Synth, notes(&|b| vec![roots[b]])),
        (
            "Bells",
            Instrument::Synth,
            notes(&|b| chords[b].map(|n| n + 12).to_vec()),
        ),
        ("Pad", Instrument::Synth, notes(&|b| chords[b].to_vec())),
    ] {
        p.tracks.push(Track {
            name: name.into(),
            instrument: Some(instrument),
            regions: vec![Region {
                start: 0,
                length: 4 * 3840,
                content,
                ..Default::default()
            }],
            ..Default::default()
        });
    }
    p
}

#[test]
fn tracks_know_their_parts() {
    let p = sketch();
    let roles: Vec<&str> = p.tracks.iter().map(role).collect();
    assert_eq!(roles, ["drums", "bass", "lead", "pad"]);
}

#[test]
fn clipping_a_pattern_keeps_its_steps_in_place() {
    let p = sketch();
    let r = &p.tracks[0].regions[0];
    let half = clip(&p, r, 3840 * 2, 3840 * 4).unwrap();
    let whole = r.content.notes(r.length).unwrap();
    let later: Vec<(i64, u8)> = whole
        .iter()
        .filter(|n| n.start >= 7680)
        .map(|n| (n.start - 7680, n.pitch))
        .collect();
    let clipped: Vec<(i64, u8)> = half
        .content
        .notes(half.length)
        .unwrap()
        .iter()
        .map(|n| (n.start, n.pitch))
        .collect();
    assert_eq!(clipped, later);
}

#[test]
fn bars_copied_bring_their_notes_and_chords() {
    let mut p = sketch();
    copy_bars(&mut p, 0, 2, 8 * 3840, Some(vec![1])).unwrap();
    let bass: Vec<(i64, u8)> = compose::heard_notes(&p, 1)
        .iter()
        .filter(|n| n.start >= 8 * 3840)
        .map(|n| (n.start, n.pitch))
        .collect();
    assert_eq!(bass, [(8 * 3840, 45), (9 * 3840, 41)]);
    let at = |tick: u64| {
        p.chords
            .iter()
            .find(|c| c.start == tick)
            .map(|c| c.chord.clone())
    };
    assert_eq!(at(8 * 3840).as_deref(), Some("Am"));
}

#[test]
fn transitions_lead_into_a_bar() {
    let dir = std::env::temp_dir().join(format!("pina-arrange-{}", std::process::id()));
    let mut p = sketch();
    let out = transition(
        &mut p,
        &dir,
        Transition {
            at: 2 * 3840,
            kind: Move::Build,
            length: None,
            track: None,
        },
    )
    .unwrap();
    // A riser, a gap and an impact: sounds, no notes written for the drums.
    assert_eq!(out["done"].as_array().unwrap().len(), 3);
    let fx = p.tracks.iter().position(|t| t.name == "FX").unwrap();
    let riser = &p.tracks[fx].regions[0];
    assert_eq!((riser.start, riser.length), (0, 2 * 3840));
    let Content::Audio { file, .. } = &riser.content else {
        panic!()
    };
    let (l, _, _) = crate::music::analysis::read_wav(&dir.join(file)).unwrap();
    // It rises: the last quarter is louder than the first.
    let energy = |s: &[f32]| s.iter().map(|v| v * v).sum::<f32>();
    assert!(energy(&l[l.len() * 3 / 4..]) > energy(&l[..l.len() / 4]) * 10.0);
    // The beat before the bar is silent but for the FX.
    let gap = 2 * 3840 - 960;
    for t in 0..4 {
        assert!(
            compose::heard_notes(&p, t)
                .iter()
                .all(|n| n.start < gap as i64 || n.start >= 2 * 3840),
            "{t}"
        );
    }
    std::fs::remove_dir_all(&dir).ok();
}
