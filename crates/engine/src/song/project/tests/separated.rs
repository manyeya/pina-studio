//! An audio region's stems on tracks of their own.

use super::*;

#[test]
fn an_audio_regions_stems_go_on_tracks_of_their_own_in_a_stack_playing_as_it_did() {
    let mut r = audio(PPQ * 4, PPQ * 2, 24_000);
    (r.gain_db, r.fade_in, r.looped) = (-3.0, PPQ / 8, true);
    let mut p = project(vec![r.clone()], false);
    p.tracks[0].name = "Song".into();
    let stems = [
        (
            "drums".to_string(),
            PathBuf::from("Audio Files/Stems/a/drums.wav"),
        ),
        (
            "vocals".to_string(),
            PathBuf::from("Audio Files/Stems/a/vocals.wav"),
        ),
    ];
    let at = p.stems_from_audio((0, 0), &stems);
    assert_eq!(at, [1, 2]);
    assert!(p.tracks[0].regions[0].mute, "the region muted, kept");
    for (k, &t) in at.iter().enumerate() {
        let track = &p.tracks[t];
        assert_eq!(track.name, stems[k].0);
        let s = &track.regions[0];
        // As the region was, on its stem.
        assert!(!s.mute);
        assert_eq!(
            (s.start, s.length, s.gain_db, s.fade_in, s.looped),
            (r.start, r.length, r.gain_db, r.fade_in, r.looped)
        );
        let (Content::Audio { file, offset, .. }, Content::Audio { offset: was, .. }) =
            (&s.content, &r.content)
        else {
            panic!()
        };
        assert_eq!((file, offset), (&stems[k].1, was));
    }
    // Together in a stack, named after the track.
    let bus = p.tracks[1].output.unwrap();
    assert_eq!(p.tracks[2].output, Some(bus));
    let stack = p.auxes.iter().find(|a| a.bus == bus).unwrap();
    assert!(stack.stack);
    assert_eq!(stack.name, "Song Stems");
}
