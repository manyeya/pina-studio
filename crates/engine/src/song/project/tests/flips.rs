//! Flips made into a Sampler and its notes.

use super::*;
use crate::lyrics::flips::{Flip, Hit};

fn hit(from: f32, to: f32, beat: f32, beats: f32, pitch: i8, reverse: bool) -> Hit {
    Hit {
        from,
        to,
        beat,
        beats,
        pitch,
        reverse,
        words: "go".into(),
    }
}

#[test]
fn a_flip_is_a_sampler_a_chop_a_zone_and_a_looping_region_playing_them_in_place() {
    let mut p = project(vec![audio(0, PPQ * 16, 0)], false);
    let flip = Flip {
        name: "Let It Burn".into(),
        style: "chipmunk".into(),
        says: "go go burn".into(),
        why: String::new(),
        bars: 2,
        pitch: 5,
        hits: vec![
            hit(1.0, 2.0, 0.0, 1.0, 0, false),
            // The same chop again: the same zone.
            hit(1.0, 2.0, 1.0, 0.5, 0, false),
            // An octave down and backwards: a zone of its own.
            hit(1.0, 2.0, 3.5, 0.5, -12, true),
            hit(4.0, 5.0, 4.0, 2.0, 0, false),
        ],
    };
    let vocals = std::path::Path::new("Audio Files/Stems/a/vocals.wav");
    let t = p.flip_words((0, 0), vocals, 10.0, &flip, PPQ * 8).unwrap();
    assert_eq!(t, 1);
    assert_eq!(p.tracks[t].name, "Let It Burn");
    let Some(Instrument::Sampler(s)) = &p.tracks[t].instrument else {
        panic!()
    };
    assert_eq!(s.voice.transpose, 5.0, "pitched up, sped up");
    assert_eq!(s.zones.len(), 3);
    let z = &s.zones;
    assert!(
        z.iter()
            .all(|z| z.file == vocals && z.gate && z.choke == Some(CHOP_CHOKE))
    );
    assert_eq!((z[0].start, z[0].end, z[0].root), (0.1, Some(0.2), None));
    // Its note an octave over its root: it plays an octave down.
    assert_eq!((z[1].note, z[1].root, z[1].reverse), (37, Some(49), true));
    assert_eq!((z[2].start, z[2].end), (0.4, Some(0.5)));
    let r = &p.tracks[t].regions[0];
    assert_eq!((r.start, r.length), (PPQ * 8, PPQ * 8));
    assert_eq!((r.looped, r.loop_length), (true, Some(PPQ * 32)));
    let Content::Midi { notes, .. } = &r.content else {
        panic!()
    };
    let played: Vec<(i64, u64, u8)> = notes.iter().map(|n| (n.start, n.length, n.pitch)).collect();
    let q = PPQ as i64;
    assert_eq!(
        played,
        [
            (0, PPQ, 36),
            (q, PPQ / 2, 36),
            (q * 7 / 2, PPQ / 2, 37),
            (q * 4, PPQ * 2, 38)
        ]
    );
    // The vocal it came from plays on, unmuted: the flip goes with it.
    assert!(!p.tracks[0].regions[0].mute);
    let empty = Flip {
        hits: Vec::new(),
        ..flip
    };
    assert_eq!(p.flip_words((0, 0), vocals, 10.0, &empty, 0), None);
}

#[test]
fn slices_go_onto_a_sampler_a_zone_each_named_played_where_they_play_now() {
    // 120 bpm at 48 kHz: a beat is half a second. The region plays its file from the start, at
    // bar 2 (beat 4).
    let mut p = project(vec![audio(PPQ * 4, PPQ * 16, 0)], false);
    let slices = vec![
        (0.5, 0.75, "Kick".to_string()),
        (1.5, 1.6, "Snare".to_string()),
        (2.0, 2.4, "Chord Am".to_string()),
    ];
    let file = std::path::Path::new("a.wav");
    let t = p
        .slices_to_sampler((0, 0), file, 8.0, &slices, "a Slices", PPQ * 16)
        .unwrap();
    let Some(Instrument::Sampler(s)) = &p.tracks[t].instrument else {
        panic!()
    };
    let names: Vec<&str> = s.zones.iter().map(|z| z.name.as_str()).collect();
    assert_eq!(names, ["Kick", "Snare", "Chord Am"]);
    assert_eq!((s.zones[1].start, s.zones[1].end), (0.1875, Some(0.2)));
    assert!(
        s.zones
            .iter()
            .all(|z| z.choke == Some(CHOP_CHOKE) && !z.gate)
    );
    let r = &p.tracks[t].regions[0];
    // The first at 0.5 s: beat 5, a beat into its bar, so a beat into the region at bar 5.
    assert_eq!((r.start, r.length), (PPQ * 16, PPQ * 8));
    let Content::Midi { notes, .. } = &r.content else {
        panic!()
    };
    let at: Vec<(i64, u64)> = notes.iter().map(|n| (n.start, n.length)).collect();
    let q = PPQ as i64;
    assert_eq!(at, [(q, PPQ / 2), (q * 3, PPQ / 5), (q * 4, PPQ * 4 / 5)]);
    assert!(!p.tracks[0].regions[0].mute, "the beat plays on");
}
