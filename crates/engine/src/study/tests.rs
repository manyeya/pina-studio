//! A song of tempo's own, rendered, so what it is is known: its tempo, key, chords, groove and
//! form are what the study should find.

use std::path::Path;

use super::*;
use crate::{
    audio::engine::Engine,
    music::compose,
    song::project::{Content, Instrument, Marker, Note, Project, Region, Track},
};

const RATE: f64 = 44_100.0;

/// Pop at 124 in A minor, written by hand: Am F C G, an intro of chords alone, verses with drums
/// and bass, choruses (F G C Am) with a lead on top, an outro; rendered. (Pop's groove, not
/// house's: the built-in kit's clap and open hat are the same white noise, so no ear could tell
/// house's apart.)
fn song() -> (Vec<f32>, Vec<f32>, Project) {
    let mut p = Project::new(RATE as u32);
    p.set_tempo(124.0);
    p.key = Some("A minor".into());
    let bar = p.bar() as i64;
    // Each section: its name, bars, chords, and which of drums, bass, keys, lead play.
    let verse = ["Am", "F", "C", "G"];
    let chorus = ["F", "G", "C", "Am"];
    let sections = [
        ("Intro", 8, verse, [false, false, true, false]),
        ("Verse", 16, verse, [true, true, true, false]),
        ("Chorus", 16, chorus, [true, true, true, true]),
        ("Verse 2", 16, verse, [true, true, true, false]),
        ("Chorus 2", 16, chorus, [true, true, true, true]),
        ("Outro", 8, verse, [false, false, true, false]),
    ];
    let shape = |c: &str| -> (u8, [u8; 3]) {
        match c {
            "Am" => (45, [57, 60, 64]),
            "F" => (41, [57, 60, 65]),
            "C" => (48, [55, 60, 64]),
            _ => (43, [55, 59, 62]),
        }
    };
    let note = |start: i64, length: i64, pitch: u8, velocity: u8| Note {
        start,
        length: length as u64,
        pitch,
        velocity,
    };
    let mut parts: [Vec<Note>; 4] = Default::default();
    let mut at = 0i64;
    for (name, bars, chords, playing) in sections {
        p.markers.push(Marker {
            tick: at as u64,
            name: name.into(),
            ..Default::default()
        });
        for b in 0..bars {
            let t = at + b * bar;
            let c = chords[b as usize % 4];
            compose::set_chords(&mut p, Some(c), Some(t as u64), Some(1)).unwrap();
            let (root, triad) = shape(c);
            if playing[0] {
                for s in [0, 8, 10] {
                    parts[0].push(note(t + s * 240, 120, 36, 110));
                }
                for s in [4, 12] {
                    parts[0].push(note(t + s * 240, 120, 38, 104));
                }
                for e in 0..8 {
                    parts[0].push(note(t + e * 480, 60, 42, if e % 2 == 0 { 84 } else { 62 }));
                }
            }
            if playing[1] {
                parts[1].push(note(t, bar - 40, root, 100));
            }
            if playing[2] {
                for n in triad {
                    parts[2].push(note(t, bar - 40, n, 88));
                }
            }
            if playing[3] {
                for (s, k) in [(0, 2), (3, 1), (6, 0), (8, 1), (10, 2), (12, 1), (14, 0)] {
                    parts[3].push(note(t + s * 240, 200, triad[k] + 12, 100));
                }
            }
        }
        at += bars * bar;
    }
    for ((name, i), notes) in [
        ("Drums", Instrument::Drums),
        ("Bass", Instrument::Synth),
        ("Keys", Instrument::Synth),
        ("Lead", Instrument::Synth),
    ]
    .into_iter()
    .zip(parts)
    {
        p.tracks.push(Track {
            name: name.into(),
            instrument: Some(i),
            regions: vec![Region {
                start: 0,
                length: at as u64,
                content: Content::Midi {
                    notes,
                    controls: Vec::new(),
                },
                ..Default::default()
            }],
            ..Default::default()
        });
    }
    // The lead louder, so the chorus is the loudest part.
    p.tracks[3].volume_db = 3.0;
    let (mut engine, _) = Engine::load(&p, Path::new("."), RATE).unwrap();
    let seconds = p.samples(p.end(), RATE) as f64 / RATE + 1.0;
    let [l, r] = engine.render((seconds * RATE) as u64);
    (l, r, p)
}

#[test]
fn a_song_is_studied_as_it_was_made() {
    let (l, r, p) = song();
    let s = study_audio(&l, &r, RATE as f32).unwrap();
    // Tempo, to within a beat a minute, and 4/4.
    assert!((s.tempo.bpm - 124.0).abs() < 1.5, "{} BPM", s.tempo.bpm);
    assert_eq!(s.tempo.time_signature, [4, 4]);
    // Its key.
    assert_eq!(s.key.name, "A minor");
    // Its chords: the verse goes round Am F C G.
    let verse = s
        .sections
        .iter()
        .find(|x| x.name.starts_with("Verse"))
        .unwrap_or_else(|| panic!("{:#?}", s.sections));
    assert_eq!(verse.chords, "Am | F | C | G", "{:#?}", s.sections);
    assert_eq!(verse.numerals, "i | VI | III | VII");
    // Its form: sections where they were made, the choruses named.
    let bar = 60.0 / 124.0 * 4.0;
    let starts: Vec<(String, f32)> = s
        .sections
        .iter()
        .map(|x| (x.name.clone(), x.start / bar))
        .collect();
    let near = |name: &str, at: f32| {
        starts
            .iter()
            .any(|(n, b)| n.starts_with(name) && (b - at).abs() <= 1.5)
    };
    assert!(near("Intro", 0.0), "{starts:?}");
    assert!(near("Verse", 8.0), "{starts:?}");
    assert!(near("Chorus", 24.0) && near("Chorus", 56.0), "{starts:?}");
    // Its groove: the kick on the one, the snare on two and four. (Which of Pina Studio's grooves
    // it's nearest isn't asked: the built-in kit's hats and snare are the same white noise,
    // which no ear could tell apart.)
    let hit = |p: &str, step: usize| p.chars().nth(step).is_some_and(|c| c != '.');
    assert!(hit(&s.groove.kick, 0), "{:?}", s.groove);
    assert!(
        hit(&s.groove.snare, 4) && hit(&s.groove.snare, 12),
        "{:?}",
        s.groove
    );
    assert!(s.genres.len() >= 3 && s.genres.windows(2).all(|w| w[0].score >= w[1].score));
    assert!(s.instruments.iter().any(|e| e.role == "drums"));
    let _ = p;
}

#[test]
fn lyrics_name_the_chorus_and_a_study_becomes_a_playbook() {
    let (l, r, _) = song();
    let mut s = study_audio(&l, &r, RATE as f32).unwrap();
    s.identity.title = "Test Song".into();
    let bar = 60.0 / 124.0 * 4.0;
    // The chorus's line, sung three times in each chorus.
    let mut lines = vec![(Some(9.0 * bar), "walking down the road tonight".to_string())];
    for chorus in [24.0f32, 56.0] {
        for k in 0..3 {
            lines.push((
                Some((chorus + 1.0 + k as f32 * 4.0) * bar),
                "hold on to the light".to_string(),
            ));
        }
    }
    attach_lyrics(&mut s, &lines, "test");
    let l = s.lyrics.as_ref().unwrap();
    assert_eq!(l.hooks, ["hold on to the light"]);
    let line = l.lines.iter().find(|x| x.text.starts_with("hold")).unwrap();
    assert!(
        s.sections[line.section.unwrap()].name.starts_with("Chorus"),
        "{:#?}",
        s.sections
    );
    assert_eq!(s.instruments[0].role, "vocals");
    let book = to_playbook(&s, &[]);
    assert_eq!(book.name, "Like Test Song");
    assert!(book.tempo.0 <= 124.0 && book.tempo.1 >= 124.0);
    assert!(book.sections.len() >= 4 && book.parts.iter().any(|p| p.role == "drums"));
    // Its parts say what the song does, never a command or another genre's recipe.
    let drums = book.parts.iter().find(|p| p.role == "drums").unwrap();
    assert!(
        drums.write.starts_with("as the song: kick "),
        "{}",
        drums.write
    );
    assert!(book.parts.iter().all(|p| !p.write.contains("write_")));
    assert!(
        book.notes
            .iter()
            .any(|n| n.contains("hold on to the light"))
    );
}

#[test]
fn a_file_name_says_who_and_what() {
    let id = |name: &str| {
        let i = super::identity(Path::new(name));
        (i.artist, i.title)
    };
    assert_eq!(
        id("PLXYBOII_017 - SARII WAY (feat. Dolly) [-2st].mp3"),
        ("PLXYBOII".into(), "SARII WAY (feat. Dolly)".into())
    );
    assert_eq!(
        id("03. Artist - Song.wav"),
        ("Artist".into(), "Song".into())
    );
    assert_eq!(
        id("50 Cent - In Da Club.mp3"),
        ("50 Cent".into(), "In Da Club".into())
    );
    assert_eq!(
        id("Just a title.flac"),
        (String::new(), "Just a title".into())
    );
}

#[test]
fn saving_a_songs_book_again_keeps_what_the_person_made_of_it() {
    let book = |json: &str| serde_json::from_str::<Playbook>(json).unwrap();
    let had = book(include_str!("../music/playbook/fixtures/house.json"));
    let mut had = Playbook {
        name: "My House".into(),
        notes: vec![
            "Studied from an old file.".into(),
            "Keep the hats dry.".into(),
        ],
        ..had
    };
    had.parts[0].write = "my own words".into();
    let mut made = had.clone();
    made.id = "like_it".into();
    made.name = "Like It".into();
    made.notes = vec!["Studied from the file.".into()];
    made.parts[0].write = String::new();
    let up = brought_up_to_date(made, &had);
    assert_eq!(
        (up.id.as_str(), up.name.as_str()),
        (had.id.as_str(), "My House")
    );
    assert_eq!(up.notes, ["Studied from the file.", "Keep the hats dry."]);
    assert_eq!(up.parts[0].write, "my own words");
}

#[test]
fn a_loud_held_note_doesnt_hide_the_chords_under_it() {
    // A loud G5 wavering over four bars of Em and four of C, the way a piano loop can be: the
    // top note is in both chords, and what changes is underneath it.
    let rate = 44_100.0;
    let tone = |f: f32, t: f32| (std::f32::consts::TAU * f * t).sin();
    let x: Vec<f32> = (0..(rate * 16.0) as usize)
        .map(|i| {
            let t = i as f32 / rate;
            let low: &[f32] = match (t / 8.0) as u32 {
                0 => &[164.81, 246.94],
                _ => &[130.81, 196.0],
            };
            let waver = 1.0 + 0.5 * tone(6.0, t);
            0.5 * waver * tone(783.99, t) + low.iter().map(|f| 0.09 * tone(*f, t)).sum::<f32>()
        })
        .collect();
    let given = nature::Given {
        tempo: Some((120.0, false)),
        key: crate::music::Key::parse("E minor"),
        looped: true,
        line: false,
    };
    let s = study_given(&x, &x, rate, &given).unwrap();
    let at = |t: f32| {
        s.chords
            .iter()
            .find(|c| c.start <= t && c.end > t)
            .map(|c| c.chord.clone())
            .unwrap_or_default()
    };
    assert!(at(4.0).starts_with('E'), "{:?}", s.chords);
    assert!(at(12.0).starts_with('C'), "{:?}", s.chords);
}

#[test]
fn a_bass_line_reads_as_its_notes() {
    let rate = 44_100.0;
    let x: Vec<f32> = (0..(rate * 8.0) as usize)
        .map(|i| {
            let t = i as f32 / rate;
            let f = if t < 4.0 { 61.74 } else { 49.0 };
            let mut v = 0.0;
            for h in 1..5 {
                v += (std::f32::consts::TAU * f * h as f32 * t).sin() / h as f32;
            }
            0.3 * v
        })
        .collect();
    let given = nature::Given {
        tempo: Some((120.0, false)),
        key: crate::music::Key::parse("B minor"),
        looped: true,
        line: true,
    };
    let s = study_given(&x, &x, rate, &given).unwrap();
    assert!(s.chords[0].chord.starts_with('B'), "{:?}", s.chords);
    assert!(
        s.chords.last().unwrap().chord.starts_with('G'),
        "{:?}",
        s.chords
    );
}
