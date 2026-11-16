//! Lines of lyrics as loops on the bars.

use super::*;

/// At 120 bpm and 48 kHz a beat is half a second: a region from beat 4 (bar 2) for 16 beats,
/// playing its file from half a second in, so a second `s` of the file plays at `s + 1.5` s.
fn song() -> Project {
    project(vec![audio(PPQ * 4, PPQ * 16, 24_000)], false)
}

#[test]
fn a_line_is_a_loop_from_the_bar_line_before_it_for_its_bars() {
    let mut p = song();
    // Sung 3.2 s to 4.6 s into the file: beats 9.4 to 12.2, so from bar 3 (beat 8) for 2 bars.
    let bars = p.lyric_bars((0, 0), 3.2, 4.6).unwrap();
    assert_eq!(bars, (PPQ * 8, PPQ * 8));
    // Not where the region plays: none.
    assert_eq!(p.lyric_bars((0, 0), 20.0, 21.0), None);
    let track = p.lyrics_track(0);
    assert_eq!((track, p.tracks[1].name.as_str()), (1, "t Lyrics"));
    assert_eq!(p.lyrics_track(0), 1, "the same track again");
    let at = p.lyric_loop((0, 0), bars, "Ain't no holding back", track, PPQ * 32, None);
    let r = p.region(at);
    assert_eq!(at.0, 1);
    assert_eq!(
        (r.start, r.length, r.name.as_str()),
        (PPQ * 32, PPQ * 8, "Ain't no holding back")
    );
    assert_eq!((r.looped, r.loop_length), (true, Some(PPQ * 8)));
    // It plays the file from bar 3's place: 2.5 s in.
    let Content::Audio { offset, .. } = &r.content else {
        panic!()
    };
    assert_eq!(*offset, 120_000);
    // The region it came from is as it was.
    assert_eq!(p.tracks[0].regions.len(), 1);
    assert_eq!(p.tracks[0].regions[0].start, PPQ * 4);
}

#[test]
fn a_file_starting_after_the_bar_line_keeps_its_beats_on_the_bars() {
    // The file starts on beat 5: a line in its first beat loops from bar 2 (beat 4), so the
    // loop starts a beat after where it's put.
    let mut p = project(vec![audio(PPQ * 5, PPQ * 8, 0)], false);
    let bars = p.lyric_bars((0, 0), 0.1, 0.8).unwrap();
    assert_eq!(bars, (PPQ * 4, PPQ * 4));
    let track = p.lyrics_track(0);
    let stem = std::path::Path::new("Audio Files/Stems/a/vocals.wav");
    let at = p.lyric_loop((0, 0), bars, "Oh", track, PPQ * 16, Some(stem));
    let r = p.region(at);
    assert_eq!(
        (r.start, r.length, r.loop_length),
        (PPQ * 17, PPQ * 3, Some(PPQ * 3))
    );
    // Its vocals stem's, not the mix's.
    assert!(matches!(&r.content, Content::Audio { file, .. } if file == stem));
}

#[test]
fn a_line_is_chopped_at_its_words_onto_a_sampler_to_flip() {
    let mut p = song();
    // Sung 3.2 s to 4.6 s into the file, its words at 3.2, 3.6 and 4.1: three chops, from C1,
    // played where they were sung, the bar put at bar 9.
    let stem = std::path::Path::new("Audio Files/Stems/a/vocals.wav");
    let t = p
        .chop_line(
            (0, 0),
            Some(stem),
            &[3.2, 3.6, 4.1],
            4.6,
            "Ain't no holding",
            PPQ * 32,
            240_000,
        )
        .unwrap();
    assert_eq!(p.tracks[t].name, "Ain't no holding");
    let Some(Instrument::Sampler(s)) = &p.tracks[t].instrument else {
        panic!("a sampler")
    };
    assert_eq!(s.zones.len(), 3);
    assert!(s.zones.iter().all(|z| z.file == stem));
    let notes: Vec<u8> = s.zones.iter().map(|z| z.note).collect();
    assert_eq!(notes, [36, 37, 38]);
    // The first word, 3.2 s into the file, sung at beat 9.4: 1.4 beats into bar 3; put at bar 9,
    // as far in.
    let r = &p.tracks[t].regions[0];
    let into = r.start - PPQ * 32;
    assert!(
        (into as i64 - (PPQ as f64 * 1.4) as i64).abs() <= 2,
        "{into}"
    );
    // The region it came from is as it was, not muted.
    assert_eq!(p.tracks[0].regions.len(), 1);
    assert!(!p.tracks[0].regions[0].mute);
}
