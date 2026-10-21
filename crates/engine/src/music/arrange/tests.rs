use super::*;
use crate::music::compose::{self, ChordPart, DrumPart};

fn sketch() -> Project {
    let mut p = Project::new(48_000);
    p.key = Some("A minor".into());
    p.genre = Some("trap".into());
    for (name, instrument) in [
        ("Drums", Instrument::Drums),
        ("808", Instrument::Synth),
        ("Bells", Instrument::Synth),
        ("Pad", Instrument::Synth),
    ] {
        p.tracks.push(Track {
            name: name.into(),
            instrument: Some(instrument),
            ..Default::default()
        });
    }
    compose::set_chords(&mut p, Some("i | VI | III | VII"), None, None).unwrap();
    compose::write_drums(
        &mut p,
        DrumPart {
            track: 0,
            start: None,
            bars: None,
            style: None,
            fill_every: Some(0),
            seed: Some(1),
        },
    )
    .unwrap();
    for t in 1..4 {
        compose::write_chords(
            &mut p,
            ChordPart {
                track: t,
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
fn a_loop_becomes_a_song() {
    let mut p = sketch();
    let out = arrange(
        &mut p,
        Arrangement {
            from: None,
            bars: None,
            sections: None,
            roles: None,
        },
    )
    .unwrap();
    // Trap's playbook: 84 bars, its sections marked.
    assert_eq!(out["bars"], 84);
    assert_eq!(p.markers.len(), 7);
    assert_eq!(p.markers[1].name, "Hook");
    assert_eq!(p.markers[1].tick, 8 * 3840);
    // The intro has no drums; the hook does, all 16 bars, and the chords run the whole song.
    let drums_at = |p: &Project, bar: u64| {
        p.tracks[0]
            .regions
            .iter()
            .any(|r| r.start <= bar * 3840 && bar * 3840 < r.start + r.length)
    };
    assert!(!drums_at(&p, 2));
    assert!(drums_at(&p, 8) && drums_at(&p, 23));
    assert_eq!(p.chords.last().map(|c| c.start + c.length), Some(84 * 3840));
    // The song's quiet parts play softer, and lower.
    let intro = p.tracks[2].regions.iter().find(|r| r.start == 0).unwrap();
    assert!(intro.velocity < 0);
    let level = |t: usize, tick: u64| {
        p.tracks[t]
            .automation
            .volume
            .iter()
            .rev()
            .find(|x| x.tick <= tick)
            .unwrap()
            .value
    };
    assert!(level(2, 0) < level(2, 8 * 3840));
    // Sections can be given, by role or track.
    let out = arrange(
        &mut p,
        Arrangement {
            from: Some(8 * 3840),
            bars: Some(4),
            sections: Some(vec![
                SectionIn {
                    name: "A".into(),
                    bars: 4,
                    parts: vec!["0".into()],
                    energy: None,
                },
                SectionIn {
                    name: "B".into(),
                    bars: 8,
                    parts: vec!["drums".into(), "lead".into()],
                    energy: Some(5),
                },
            ]),
            roles: None,
        },
    )
    .unwrap();
    assert_eq!(out["sections"][1]["playing"].as_array().unwrap().len(), 2);
    assert_eq!(out["silent"].as_array().unwrap().len(), 2);
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
    assert_eq!(out["done"].as_array().unwrap().len(), 4);
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
    // The beat before the bar is silent but for the FX, and the drums fill the bar before.
    let gap = 2 * 3840 - 960;
    for t in 0..4 {
        assert!(
            compose::heard_notes(&p, t)
                .iter()
                .all(|n| n.start < gap as i64 || n.start >= 2 * 3840),
            "{t}"
        );
    }
    let fill = p.tracks[0]
        .regions
        .iter()
        .find(|r| r.name == "Fill")
        .unwrap();
    assert_eq!(fill.start, 3840);
    std::fs::remove_dir_all(&dir).ok();
}
