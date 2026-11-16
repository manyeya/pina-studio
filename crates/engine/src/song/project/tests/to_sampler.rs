//! Audio regions made Sampler tracks: whole, or sliced at their hits.

use super::*;

/// At 120 bpm and 48 kHz a beat is half a second, 24,000 frames: a two-beat region from bar 2,
/// playing a 4 s file from half a second in.
fn song() -> Project {
    let mut r = audio(PPQ * 4, PPQ * 2, 24_000);
    r.gain_db = -3.0;
    project(vec![r], false)
}

#[test]
fn an_audio_region_becomes_a_sampler_track_playing_it_where_it_was() {
    let mut p = song();
    let t = p.sampler_from_audio((0, 0), 192_000, None).unwrap();
    assert_eq!(t, 1);
    assert!(p.tracks[0].regions[0].mute, "the audio is muted, kept");
    let Some(Instrument::Sampler(s)) = &p.tracks[1].instrument else {
        panic!("a sampler")
    };
    // What the region plays, across the keys, at its gain; stopping with its note.
    let z = &s.zones[0];
    assert_eq!((z.note, z.high, z.root), (0, Some(127), Some(SAMPLER_ROOT)));
    assert!((z.start - 0.125).abs() < 1e-4 && (z.end.unwrap() - 0.375).abs() < 1e-4);
    assert!(z.gate && z.gain_db == -3.0);
    assert_eq!(s.voice.mode, SampleMode::Repitch, "played as recorded");
    // One note on its root, as long as the region, where it was.
    let r = &p.tracks[1].regions[0];
    assert_eq!((r.start, r.length), (PPQ * 4, PPQ * 2));
    let Content::Midi { notes, .. } = &r.content else {
        panic!()
    };
    assert_eq!(
        notes
            .iter()
            .map(|n| (n.start, n.length, n.pitch))
            .collect::<Vec<_>>(),
        [(0, PPQ * 2, SAMPLER_ROOT)]
    );
}

#[test]
fn an_audio_region_is_sliced_at_its_hits_into_a_sampler_track() {
    let mut p = song();
    // Hits before and after what it plays are left out; its start is always a slice.
    let cuts = [0, 48_000, 60_000, 100_000];
    p.sampler_from_audio((0, 0), 192_000, Some(&cuts)).unwrap();
    let Some(Instrument::Sampler(s)) = &p.tracks[1].instrument else {
        panic!("a sampler")
    };
    let zones: Vec<(u8, f32, Option<f32>)> =
        s.zones.iter().map(|z| (z.note, z.start, z.end)).collect();
    assert_eq!(
        zones,
        [
            (FIRST_SLICE, 0.125, Some(0.25)),
            (FIRST_SLICE + 1, 0.25, Some(0.3125)),
            (FIRST_SLICE + 2, 0.3125, Some(0.375)),
        ]
    );
    assert!(s.zones.iter().all(|z| !z.gate), "one-shots");
    // Each slice's note where its hit played: a beat in, a beat and a half in.
    let Content::Midi { notes, .. } = &p.tracks[1].regions[0].content else {
        panic!()
    };
    let at: Vec<(i64, u64, u8)> = notes.iter().map(|n| (n.start, n.length, n.pitch)).collect();
    assert_eq!(
        at,
        [
            (0, PPQ, FIRST_SLICE),
            (PPQ as i64, PPQ / 2, FIRST_SLICE + 1),
            (PPQ as i64 * 3 / 2, PPQ / 2, FIRST_SLICE + 2),
        ]
    );
}

#[test]
fn a_stretched_looped_region_sliced_keeps_its_speed_and_its_loops() {
    let mut p = song();
    let r = &mut p.tracks[0].regions[0];
    // Twice as long as recorded, its loops dragged out to two bars.
    if let Content::Audio { stretch, .. } = &mut r.content {
        *stretch = 2.0;
    }
    (r.looped, r.loop_length, r.transpose) = (true, Some(PPQ * 8), 3);
    p.sampler_from_audio((0, 0), 192_000, Some(&[36_000]))
        .unwrap();
    let Some(Instrument::Sampler(s)) = &p.tracks[1].instrument else {
        panic!("a sampler")
    };
    // Each slice at half speed, so it lasts as long as its place; in the key it was moved to.
    assert_eq!(s.voice.mode, SampleMode::Stretch);
    assert!((s.voice.speed - 0.5).abs() < 0.01, "{}", s.voice.speed);
    assert_eq!(s.voice.transpose, 3.0);
    let r = &p.tracks[1].regions[0];
    assert_eq!((r.looped, r.loop_length), (true, Some(PPQ * 8)));
}

#[test]
fn a_loop_stopped_by_the_next_region_stops_there_on_its_sampler_track_too() {
    let mut p = song();
    p.tracks[0].regions[0].looped = true;
    let mut next = audio(PPQ * 12, PPQ, 0);
    next.name = "next".into();
    p.tracks[0].regions.push(next);
    p.sampler_from_audio((0, 0), 192_000, None).unwrap();
    // Its loops ran to bar 4's start: on its own track, they still do.
    let r = &p.tracks[1].regions[0];
    assert_eq!((r.looped, r.loop_length), (true, Some(PPQ * 8)));
    // Looping to the end, with nothing after it, it loops to the end there too.
    let mut q = song();
    q.tracks[0].regions[0].looped = true;
    q.sampler_from_audio((0, 0), 192_000, None).unwrap();
    assert_eq!(q.tracks[1].regions[0].loop_length, None);
}
