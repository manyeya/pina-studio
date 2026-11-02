use super::*;
use crate::{
    music::{arrange, compose},
    song::project::{Marker, Region, Track},
};

/// Eight bars at 140 written by hand: kick, snare and hats with a player's velocities, an 808
/// on every kick at its chord's root, and the chords held on keys.
fn song() -> Project {
    let mut p = Project::new(48_000);
    p.key = Some("A minor".into());
    p.tempo = 140.0;
    compose::set_chords(&mut p, Some("i | VI | III | VII"), None, Some(8)).unwrap();
    let note = |start: i64, length: u64, pitch: u8, velocity: u8| Note {
        start,
        length,
        pitch,
        velocity,
    };
    let (kicks, roots) = ([0i64, 7, 10], [33u8, 29, 36, 31]);
    let chords = [[57u8, 60, 64], [57, 60, 65], [55, 60, 64], [55, 59, 62]];
    let (mut drums, mut bass, mut keys) = (Vec::new(), Vec::new(), Vec::new());
    for bar in 0..8i64 {
        let at = bar * 3840;
        for (k, &s) in kicks.iter().enumerate() {
            drums.push(note(at + s * 240, 120, 36, [112, 96, 104][k]));
            let next = kicks.get(k + 1).map_or(16, |&n| n);
            bass.push(note(
                at + s * 240,
                ((next - s) * 240 - 20) as u64,
                roots[bar as usize % 4],
                100,
            ));
        }
        drums.push(note(at + 8 * 240, 120, 38, 108));
        for e in 0..8 {
            drums.push(note(at + e * 480, 60, 42, [92, 58, 74, 60][e as usize % 4]));
        }
        for &n in &chords[bar as usize % 4] {
            keys.push(note(at, 3700, n, 84));
        }
    }
    for (name, i, notes) in [
        ("Drums", Instrument::Drums, drums),
        ("808", Instrument::Synth, bass),
        ("Keys", Instrument::Synth, keys),
    ] {
        p.tracks.push(Track {
            name: name.into(),
            instrument: Some(i),
            regions: vec![Region {
                start: 0,
                length: 8 * 3840,
                content: Content::Midi {
                    notes,
                    controls: Vec::new(),
                },
                ..Default::default()
            }],
            ..Default::default()
        });
    }
    p
}

fn has(issues: &[Issue], area: &str, words: &str) -> bool {
    issues
        .iter()
        .any(|i| i.area == area && i.what.contains(words))
}

#[test]
fn a_song_written_by_hand_in_key_and_on_the_kick_is_clean() {
    let issues = music(&song());
    assert!(!issues.iter().any(|i| i.area == "harmony"), "{issues:#?}");
    assert!(
        !issues
            .iter()
            .any(|i| i.area == "rhythm" && i.what.contains("flam")),
        "{issues:#?}"
    );
    // But it's a loop on placeholder sounds. No genre is fine: nothing asks for a playbook.
    assert!(has(&issues, "sound", "sine"));
    assert!(!has(&issues, "setup", "genre"));
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
    arrange::copy_bars(&mut p, 0, 8, 8 * 3840, None).unwrap();
    arrange::copy_bars(&mut p, 0, 16, 16 * 3840, None).unwrap();
    let issues = music(&p);
    assert!(
        has(&issues, "arrangement", "nothing comes or goes"),
        "{issues:#?}"
    );
    assert!(has(&issues, "arrangement", "no sections"));
    // Laid out in sections, parts coming and going, it isn't.
    let mut p = song();
    for to in [8, 16] {
        arrange::copy_bars(&mut p, 0, 8, to * 3840, None).unwrap();
    }
    arrange::clear(&mut p, 1, 0, 8 * 3840);
    arrange::clear(&mut p, 0, 16 * 3840, 20 * 3840);
    p.markers = [(0, "Intro"), (8, "Verse"), (16, "Break"), (20, "Drop")]
        .map(|(bar, name)| Marker {
            tick: bar * 3840,
            name: name.into(),
            ..Default::default()
        })
        .to_vec();
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

#[test]
fn a_progression_off_the_key_a_hot_fader_and_an_unmixed_song_are_heard() {
    let mut p = song();
    // The chord track off A minor (its parts follow it, so no note is "wrong"), and a fader
    // pushed to +9 dB.
    compose::set_chords(&mut p, Some("Ebmaj7 | Ebm7 | Abm | Bb"), None, Some(8)).unwrap();
    p.tracks[2].volume_db = 9.4;
    let issues = music(&p);
    assert!(has(&issues, "harmony", "leaves A minor"), "{issues:#?}");
    assert!(has(&issues, "mix", "+9.4 dB"), "{issues:#?}");
    // A sixteen-bar song of four parts with nothing done to any of them isn't mixed yet.
    p.tracks.push(p.tracks[0].clone());
    for t in &mut p.tracks {
        t.regions[0].length = 16 * 3840;
    }
    assert!(has(&music(&p), "mix", "nothing in it is mixed"));
    // The song as written by hand: in key, so the chord check stays quiet.
    let clean = music(&song());
    assert!(!has(&clean, "harmony", "leaves"), "{clean:#?}");
    assert!(!has(&clean, "mix", "fader"));
}

#[test]
fn a_song_of_any_size_with_no_tools_is_told_so() {
    use crate::song::project::{Gate, Unit};
    let mut p = song();
    assert!(!has(&effects(&p), "mix", "no part uses the tools"));
    p.tracks.push(p.tracks[0].clone());
    assert!(has(&effects(&p), "mix", "no part uses the tools"));
    p.tracks[1].more.push(Unit::Gate(Gate::default()));
    assert!(!has(&effects(&p), "mix", "no part uses the tools"));
    // A reverb isn't one.
    p.tracks[1].more = vec![Unit::Reverb(Default::default())];
    assert!(has(&effects(&p), "mix", "no part uses the tools"));
}

#[test]
fn effects_left_as_they_came_or_all_one_colour_are_heard() {
    use crate::song::project::{Comp, SatBand, SatBands, SatStyle, Saturator};
    let mut p = song();
    let n = p.tracks.len();
    assert!(n >= 3);
    // As they come: one Warm Tape band, a compressor untouched.
    p.tracks[0].sat = Some(Saturator::default());
    p.tracks[1].comp = Some(Comp::default());
    let issues = effects(&p);
    assert!(
        has(&issues, "mix", "saturator is as it came"),
        "{issues:#?}"
    );
    assert!(
        has(&issues, "mix", "compressor is as it came"),
        "{issues:#?}"
    );

    // Each set its own way, but every one Warm Tape: one colour.
    let warm = |drive: f32| Saturator {
        bands: SatBands::from_slice(&[SatBand {
            drive_db: drive,
            ..SatBand::default()
        }]),
        ..Saturator::default()
    };
    for (t, tr) in p.tracks.iter_mut().enumerate() {
        tr.sat = Some(warm(3.0 + t as f32));
        tr.comp = None;
    }
    let issues = effects(&p);
    assert!(
        has(&issues, "mix", "Warm Tape and nothing else"),
        "{issues:#?}"
    );

    // The same setting on three: copied.
    for tr in p.tracks.iter_mut().take(3) {
        tr.sat = Some(warm(9.0));
    }
    assert!(has(&effects(&p), "mix", "share one saturator"));

    // Each its own colour: nothing to say.
    let styles = [SatStyle::Heavy, SatStyle::Transformer, SatStyle::WarmTube];
    for (t, tr) in p.tracks.iter_mut().enumerate() {
        tr.sat = Some(Saturator {
            bands: SatBands::from_slice(&[SatBand {
                style: styles[t % 3],
                ..SatBand::default()
            }]),
            ..Saturator::default()
        });
    }
    assert!(effects(&p).is_empty(), "{:#?}", effects(&p));
}
