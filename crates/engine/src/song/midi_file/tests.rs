//! MIDI files read: one written by hand (running status, two channels, drums, a note-on of
//! velocity 0, a note left hanging, tempo, meter, key and names), one Pina Studio bounced read
//! back as it was, and both brought into a song that plays them.

use super::*;
use crate::audio::bounce;

/// A track chunk round its events.
fn track(events: &[u8]) -> Vec<u8> {
    let mut out = b"MTrk".to_vec();
    out.extend((events.len() as u32).to_be_bytes());
    out.extend(events);
    out
}

/// A format-0 file at 96 ticks a beat: 100 BPM, 6/8, E minor, named "Groove"; on channel 1 a
/// piano (program 0) plays C4 a beat then (running status) E4 half a beat, ended by a note-on of
/// velocity 0, with a pitch bend; on channel 10 a kick on the beat; G4 never let go.
fn by_hand() -> Vec<u8> {
    let mut ev = vec![];
    ev.extend([0x00, 0xff, 0x03, 6]);
    ev.extend(b"Groove");
    ev.extend([0x00, 0xff, 0x51, 3, 0x09, 0x27, 0xc0]); // 600000 us: 100 BPM
    ev.extend([0x00, 0xff, 0x58, 4, 6, 3, 24, 8]); // 6/8
    ev.extend([0x00, 0xff, 0x59, 2, 1, 1]); // one sharp, minor: E minor
    ev.extend([0x00, 0xc0, 0x00]); // program 0 on channel 1
    ev.extend([0x00, 0x90, 60, 100]);
    ev.extend([0x00, 0x99, 36, 120]);
    ev.extend([0x30, 0xe0, 0x00, 0x60]); // a bend up, half a beat in
    ev.extend([0x30, 0x80, 60, 0]); // C4 off at a beat
    ev.extend([0x00, 0x89, 36, 0]);
    ev.extend([0x00, 0x90, 64, 90]); // E4 on
    ev.extend([0x30, 64, 0]); // running status: E4 off (velocity 0)
    ev.extend([0x00, 67, 80]); // G4 on, never off
    ev.extend([0x60, 0xff, 0x2f, 0x00]);
    let mut out = b"MThd".to_vec();
    out.extend(6u32.to_be_bytes());
    out.extend(0u16.to_be_bytes());
    out.extend(1u16.to_be_bytes());
    out.extend(96u16.to_be_bytes());
    out.extend(track(&ev));
    out
}

#[test]
fn a_file_written_by_hand_reads_note_for_note() {
    let f = read(&by_hand()).unwrap();
    assert_eq!(f.tempo, Some(100.0));
    assert_eq!(f.time_signature, Some([6, 8]));
    assert_eq!(f.key.map(|k| k.name()), Some("E minor".into()));
    assert_eq!(f.name.as_deref(), Some("Groove"));
    assert_eq!(f.parts.len(), 2);
    let piano = f.parts.iter().find(|p| p.channel == 0).unwrap();
    assert_eq!(piano.program, Some(0));
    assert_eq!(piano.name, "Groove · Acoustic Grand Piano");
    let notes: Vec<(i64, u64, u8, u8)> = piano
        .notes
        .iter()
        .map(|n| (n.start, n.length, n.pitch, n.velocity))
        .collect();
    // At PPQ: a beat is 960 ticks. G4, never let go, ends where the track does.
    assert_eq!(
        notes,
        [(0, 960, 60, 100), (960, 480, 64, 90), (1440, 960, 67, 80)]
    );
    assert!(matches!(
        piano.controls[0],
        ControlPoint { start: 480, kind: ControlValue::PitchBend { value } } if value > 3000
    ));
    let drums = f.parts.iter().find(|p| p.is_drums()).unwrap();
    assert_eq!((drums.notes[0].pitch, drums.notes[0].length), (36, 960));
    assert_eq!(drums.name, "Groove · Drums");
    assert!(read(b"RIFF....").is_err());
}

#[test]
fn a_bounced_song_reads_back_and_comes_into_a_song_that_plays_it() {
    let mut p = Project::new(48_000);
    p.tempo = 97.0;
    p.key = Some("D minor".into());
    let notes: Vec<Note> = (0..8)
        .map(|i| Note {
            start: i * 480,
            length: 400,
            pitch: 62 + (i % 3) as u8 * 2,
            velocity: 70 + i as u8 * 5,
        })
        .collect();
    p.add_track(
        0,
        Track {
            name: "Keys".into(),
            instrument: Some(Instrument::Synth),
            regions: vec![Region {
                start: 3840,
                length: 3840,
                content: Content::Midi {
                    notes: notes.clone(),
                    controls: Vec::new(),
                },
                ..Default::default()
            }],
            ..Default::default()
        },
    );
    let f = read(&bounce::midi(&p, None)).unwrap();
    assert_eq!(f.tempo, Some(97.0));
    let keys = &f.parts[0];
    let back: Vec<(i64, u8, u8)> = keys
        .notes
        .iter()
        .map(|n| (n.start, n.pitch, n.velocity))
        .collect();
    let sent: Vec<(i64, u8, u8)> = notes
        .iter()
        .map(|n| (n.start + 3840, n.pitch, n.velocity))
        .collect();
    assert_eq!(back, sent);
    // Into an empty song: its tempo, a track that plays the part, transposed (drums never).
    let mut song = Project::new(48_000);
    let mut hand = read(&by_hand()).unwrap();
    hand.parts.extend(f.parts.clone());
    let made = song.import_midi(&hand, 7680, None, 2);
    assert_eq!(made, [0, 1, 2]);
    assert_eq!((song.tempo, song.time_signature), (100.0, [6, 8]));
    assert_eq!(
        song.key.as_deref(),
        Some("F# minor"),
        "where its notes went"
    );
    let first = |t: usize| match &song.tracks[t].regions[0].content {
        Content::Midi { notes, .. } => notes[0].pitch,
        _ => panic!(),
    };
    assert_eq!(
        (first(0), first(1)),
        (62, 36),
        "the piano up two, the kick as it was"
    );
    assert_eq!(song.tracks[1].instrument, Some(Instrument::Drums));
    if cfg!(target_os = "macos") {
        let Some(Instrument::Plugin(r)) = &song.tracks[0].instrument else {
            panic!("General MIDI's synth")
        };
        assert_eq!(r.preset.as_ref().unwrap().name, "Acoustic Grand Piano");
    }
    assert_eq!(song.tracks[0].regions[0].start, 7680);
    // A song with something in it keeps its own tempo.
    song.import_midi(&f, 0, None, 0);
    assert_eq!(song.tempo, 100.0);
}

#[test]
fn a_file_is_heard_at_a_songs_tempo() {
    let f = read(&by_hand()).unwrap();
    let dir = std::env::temp_dir();
    let [l, r] = render(&f, 120.0, 0, 48_000.0, &dir).unwrap();
    // One bar of 6/8 at 120 (a quarter note a beat): three quarters' worth, a second and a half.
    assert_eq!(l.len(), 72_000);
    assert_eq!(r.len(), l.len());
    assert!(l.iter().any(|v| v.abs() > 0.01), "it sounds");
}

/// A held note's pitch: the lag it best matches itself at.
fn pitch(x: &[f32], rate: f64) -> f64 {
    let at = |lag: usize| -> f32 { x.iter().zip(&x[lag..]).map(|(a, b)| a * b).sum() };
    let lags = (rate / 1000.0) as usize..(rate / 80.0) as usize;
    let best = lags.max_by(|a, b| at(*a).total_cmp(&at(*b))).unwrap();
    rate / best as f64
}

/// A file of notes at (bar, bars long, pitch).
fn notes(at: &[(u64, u64, u8)]) -> MidiFile {
    let bar = PPQ * 4;
    MidiFile {
        parts: vec![Part {
            name: "Line".into(),
            channel: 0,
            program: None,
            notes: at
                .iter()
                .map(|&(from, long, pitch)| Note {
                    start: (from * bar) as i64,
                    length: long * bar,
                    pitch,
                    velocity: 100,
                })
                .collect(),
            controls: Vec::new(),
        }],
        time_signature: Some([4, 4]),
        ..Default::default()
    }
}

#[test]
fn heard_moved_it_really_is_moved() {
    let dir = std::env::temp_dir();
    let rate = 48_000.0;
    let heard = |up: i32| {
        let [l, _] = render(&notes(&[(0, 1, 57)]), 120.0, up, rate, &dir).unwrap();
        pitch(&l[4_000..40_000], rate)
    };
    let (still, moved) = (heard(0), heard(7));
    assert!((moved / still - 1.4983).abs() < 0.02, "{still} to {moved}");
}

#[test]
fn files_heard_together_go_round_until_the_longest_ends() {
    let dir = std::env::temp_dir();
    let rate = 48_000.0;
    // Four bars with nothing in the second: only the short one, going round, can sound there.
    let (short, long) = (notes(&[(0, 1, 69)]), notes(&[(0, 1, 57), (3, 1, 57)]));
    let [l, _] = render_all(&[(&short, 0), (&long, 0)], 120.0, rate, &dir).unwrap();
    // Four bars at 120 is eight seconds.
    assert!((l.len() as f64 / rate - 8.0).abs() < 0.01, "{}", l.len());
    let second = (rate * 2.4) as usize;
    let top = l[second..second + 20_000]
        .iter()
        .fold(0f32, |m, v| m.max(v.abs()));
    assert!(top > 0.01, "{top}");
}
