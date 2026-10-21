//! A song of tempo's own, rendered, so what it is is known: its tempo, key, chords, groove and
//! form are what the study should find.

use std::path::Path;

use super::*;
use crate::{
    audio::engine::Engine,
    music::arrange::{self, Arrangement, SectionIn},
    music::compose::{self, BassPart, ChordPart, DrumPart, MelodyPart},
    music::theory::{BassStyle, ChordRhythm, MelodyStyle},
    song::project::{Instrument, Project, Track},
};

const RATE: f64 = 44_100.0;

/// Pop at 124 in A minor: Am F C G, an intro of chords alone, verses with drums and bass,
/// choruses (F G C Am) with a lead on top, an outro; rendered. (Pop's groove, not house's: the
/// built-in kit's clap and open hat are the same white noise, so no ear could tell house's
/// apart.)
fn song() -> (Vec<f32>, Vec<f32>, Project) {
    let mut p = Project::new(RATE as u32);
    p.set_tempo(124.0);
    p.key = Some("A minor".into());
    for (name, i) in [
        ("Drums", Instrument::Drums),
        ("Bass", Instrument::Synth),
        ("Keys", Instrument::Synth),
        ("Lead", Instrument::Synth),
    ] {
        p.tracks.push(Track {
            name: name.into(),
            instrument: Some(i),
            ..Default::default()
        });
    }
    compose::set_chords(&mut p, Some("Am | F | C | G"), None, Some(8)).unwrap();
    compose::write_drums(
        &mut p,
        DrumPart {
            track: 0,
            start: None,
            bars: None,
            style: Some("pop".into()),
            fill_every: Some(0),
            seed: Some(1),
        },
    )
    .unwrap();
    compose::write_bass(
        &mut p,
        BassPart {
            track: 1,
            start: None,
            bars: None,
            style: Some(BassStyle::Roots),
            register: None,
            seed: None,
            kick_track: None,
        },
    )
    .unwrap();
    compose::write_chords(
        &mut p,
        ChordPart {
            track: 2,
            start: None,
            bars: None,
            chords: None,
            voicing: None,
            rhythm: Some(ChordRhythm::Held),
            register: None,
            seed: None,
        },
    )
    .unwrap();
    compose::write_melody(
        &mut p,
        MelodyPart {
            track: 3,
            start: None,
            bars: None,
            style: Some(MelodyStyle::Hook),
            register: None,
            density: Some(0.8),
            seed: Some(2),
        },
    )
    .unwrap();
    let section = |name: &str, bars: u64, parts: &[&str]| SectionIn {
        name: name.into(),
        bars,
        parts: parts.iter().map(|s| s.to_string()).collect(),
        energy: None,
    };
    arrange::arrange(
        &mut p,
        Arrangement {
            from: None,
            bars: Some(8),
            sections: Some(vec![
                section("Intro", 8, &["2"]),
                section("Verse", 16, &["0", "1", "2"]),
                section("Chorus", 16, &["0", "1", "2", "3"]),
                section("Verse 2", 16, &["0", "1", "2"]),
                section("Chorus 2", 16, &["0", "1", "2", "3"]),
                section("Outro", 8, &["2"]),
            ]),
            roles: None,
        },
    )
    .unwrap();
    // The choruses go round chords of their own (as choruses do): F G C Am.
    let bar = p.bar();
    for chorus in [24u64, 56] {
        let start = Some(chorus * bar);
        compose::set_chords(&mut p, Some("F | G | C | Am"), start, Some(16)).unwrap();
        compose::write_bass(
            &mut p,
            BassPart {
                track: 1,
                start,
                bars: Some(16),
                style: Some(BassStyle::Roots),
                register: None,
                seed: None,
                kick_track: None,
            },
        )
        .unwrap();
        compose::write_chords(
            &mut p,
            ChordPart {
                track: 2,
                start,
                bars: Some(16),
                chords: None,
                voicing: None,
                rhythm: Some(ChordRhythm::Held),
                register: None,
                seed: None,
            },
        )
        .unwrap();
        compose::write_melody(
            &mut p,
            MelodyPart {
                track: 3,
                start,
                bars: Some(16),
                style: Some(MelodyStyle::Hook),
                register: None,
                density: Some(0.8),
                seed: Some(2),
            },
        )
        .unwrap();
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
