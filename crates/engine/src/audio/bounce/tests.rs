use super::*;
use crate::song::project::{Instrument, Marker, Note, PPQ, Region, Track};

/// Two synth tracks, a bar each: a C on the first, an E on the second.
fn song() -> Project {
    let mut p = Project::new(48_000);
    for (name, pitch) in [("Keys", 60u8), ("Lead", 64)] {
        p.tracks.push(Track {
            name: name.into(),
            instrument: Some(Instrument::Synth),
            regions: vec![Region {
                start: 0,
                length: PPQ * 4,
                content: Content::Midi {
                    notes: vec![Note {
                        start: 0,
                        length: PPQ * 2,
                        pitch,
                        velocity: 100,
                    }],
                    controls: Vec::new(),
                },
                ..Default::default()
            }],
            ..Default::default()
        });
    }
    p.markers = vec![
        Marker {
            tick: 0,
            name: "A".into(),
        },
        Marker {
            tick: PPQ * 2,
            name: "B".into(),
        },
    ];
    p.assign_ids();
    p
}

fn dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("pina-bounce-test-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_mix_is_rendered_at_its_rate_with_its_tail_and_levelled() {
    let p = song();
    let job = Job {
        name: "Mix".into(),
        track: None,
        range: None,
        master: true,
    };
    let bar = 2.0; // a 4/4 bar at 120 BPM
    let [l, _] = render(&p, &dir(), &job, 44_100, false, &|_| {}).unwrap();
    assert_eq!(l.len(), (bar * 44_100.0) as usize);
    // With the tail: as long as the synth's release rings, no more than ten seconds over.
    let [l, _] = render(&p, &dir(), &job, 44_100, true, &|_| {}).unwrap();
    assert!(
        l.len() >= (bar * 44_100.0) as usize && l.len() < ((bar + 10.0) * 44_100.0) as usize,
        "{}",
        l.len()
    );
    let peak = |x: &[f32]| x.iter().fold(0f32, |m, s| m.max(s.abs()));
    let g = level_gain(&l, &l, 44_100, Level::Peak { db: -1.0 });
    assert!((20.0 * (peak(&l) * 10f32.powf(g / 20.0)).log10() + 1.0).abs() < 0.01);
    // Loudness never pushes the peaks past the ceiling.
    let g = level_gain(
        &l,
        &l,
        44_100,
        Level::Loudness {
            lufs: 0.0,
            ceiling: -1.0,
        },
    );
    assert!(20.0 * (peak(&l) * 10f32.powf(g / 20.0)).log10() <= -0.99);
}

#[test]
fn a_stem_is_its_track_alone_and_sections_split_at_markers() {
    let p = song();
    let one = arranged(
        &p,
        &Job {
            name: "Keys".into(),
            track: Some(0),
            range: None,
            master: false,
        },
    );
    assert!(!one.tracks[0].mute && one.tracks[1].mute);
    assert_eq!(
        sections(&p),
        [
            ("A".to_string(), 0, PPQ * 2),
            ("B".to_string(), PPQ * 2, PPQ * 4)
        ]
    );
    assert_eq!(tracks_with_regions(&p), [0, 1]);
}

#[test]
fn files_are_written_in_every_format_asked() {
    let p = song();
    let out = dir().join("out");
    let opts = Options {
        formats: vec![
            Format::Wav,
            Format::Mp3 { kbps: 192 },
            Format::Flac,
            Format::Aiff,
            Format::Aac { kbps: 256 },
            Format::Midi,
        ],
        bits: Bits::I16,
        rate: 48_000,
        level: Level::Peak { db: -1.0 },
        tail: false,
        dither: true,
    };
    let job = Job {
        name: "Song".into(),
        track: None,
        range: None,
        master: true,
    };
    let done = bounce(&p, &dir(), &job, &opts, &out, &|_| {}).unwrap();
    assert_eq!(done.files.len(), 6);
    for f in &done.files {
        assert!(std::fs::metadata(f).unwrap().len() > 30, "{}", f.display());
    }
    let wav = hound::WavReader::open(out.join("Song.wav")).unwrap();
    assert_eq!(
        (wav.spec().bits_per_sample, wav.spec().sample_rate),
        (16, 48_000)
    );
    assert!((done.analysis.peak_db[0].max(done.analysis.peak_db[1]) + 1.0).abs() < 0.05);
    // The MIDI file: a header, a conductor and the two tracks.
    let mid = std::fs::read(out.join("Song.mid")).unwrap();
    assert_eq!(&mid[..4], b"MThd");
    assert_eq!(u16::from_be_bytes([mid[10], mid[11]]), 3);
}

#[test]
fn halving_keeps_what_is_under_the_new_nyquist() {
    // 1 kHz at 96 kHz, halved: still 1 kHz at 48, as loud.
    let x: Vec<f32> = (0..9600)
        .map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 1000.0 / 96_000.0).sin())
        .collect();
    let y = halve(&x);
    assert_eq!(y.len(), 4800);
    let peak = y[100..4700].iter().fold(0f32, |m, s| m.max(s.abs()));
    assert!((peak - 1.0).abs() < 0.02, "{peak}");
}
