//! MIDI files studied: a bass in G# minor (its name and its notes agreeing), chords going round a
//! progression, a beat's drums on their steps, a melody's shape; found by role and key, moved
//! into another; and the library's real files read (`PINA_EVAL_MIDI`).

use super::*;
use crate::song::{
    midi_file::{DRUMS, MidiFile, Part},
    project::Note,
};

const BAR: i64 = (PPQ * 4) as i64;
const STEP: i64 = BAR / 16;

fn note(start: i64, steps: i64, pitch: u8, velocity: u8) -> Note {
    Note {
        start,
        length: (steps * STEP) as u64,
        pitch,
        velocity,
    }
}

fn file(parts: Vec<Part>) -> MidiFile {
    MidiFile {
        parts,
        tempo: Some(122.0),
        ..Default::default()
    }
}

fn part(name: &str, channel: u8, notes: Vec<Note>) -> Part {
    Part {
        name: name.into(),
        channel,
        program: None,
        notes,
        controls: Vec::new(),
    }
}

/// Every MIDI file under `PINA_EVAL_MIDI`, studied, a line each; what couldn't be read said.
#[test]
#[ignore]
fn study_real_files() {
    let Ok(dir) = std::env::var("PINA_EVAL_MIDI") else {
        return;
    };
    let root = Path::new(&dir);
    let (mut read, mut failed) = (0, Vec::new());
    for path in midi_files(root) {
        match study_file(&path, root, 0, 0) {
            Ok(m) => {
                read += 1;
                println!(
                    "{:<44} | {:<6} | {:<9} {:<14} | {}",
                    m.name.chars().take(44).collect::<String>(),
                    m.study.role,
                    m.study.key.clone().unwrap_or("-".into()),
                    m.study.key_from,
                    m.study.summary
                );
            }
            Err(e) => failed.push(e),
        }
    }
    println!("read {read}, failed {}: {failed:?}", failed.len());
}

#[test]
fn names_say_keys_and_tempos_as_packs_write_them() {
    let k = |s: &str| from_name(s).0.map(|k| k.name());
    assert_eq!(k("PML BS Kila - G#m").as_deref(), Some("G# minor"));
    assert_eq!(
        k("PML_AH2_Bass_Loop_001_Dele_122BPM_G#m").as_deref(),
        Some("G# minor")
    );
    assert_eq!(
        k("TPS - Savannah - Piano Loop 01 Ebmin (120 BPM)").as_deref(),
        Some("Eb minor")
    );
    assert_eq!(k("Lead F# minor 124 bpm").as_deref(), Some("F# minor"));
    assert_eq!(
        from_name("PML_AH2_Bass_Loop_001_Dele_122BPM_G#m").1,
        Some(122.0)
    );
    assert_eq!(
        from_name("TPS - Savannah - Piano Loop 01 Ebmin (120 BPM)").1,
        Some(120.0)
    );
    assert_eq!(from_name("PML DRM Closed Hat Lost"), (None, None));
}

#[test]
fn a_bass_line_reads_as_a_syncopated_one_bar_loop_in_its_key() {
    // G#1 on the 1st and 4th sixteenths, D#2 on the 8th, F#1 on the 11th: eight bars of it.
    let mut notes = Vec::new();
    for b in 0..8 {
        let at = b * BAR;
        for &(s, len, p) in &[(0, 2, 32u8), (3, 2, 32), (7, 2, 39), (10, 2, 30)] {
            notes.push(note(at + s * STEP, len, p, 100));
        }
    }
    let s = study(&file(vec![part("", 0, notes)]), "Test BS Kila - G#m");
    assert_eq!(
        (s.role.as_str(), s.key.as_deref()),
        ("bass", Some("G# minor"))
    );
    assert_eq!(s.key_from, "name and notes");
    assert_eq!((s.bars, s.loop_bars, s.tempo), (8, 1, Some(122.0)));
    let b = &s.parts[0];
    assert_eq!((b.low.as_str(), b.high.as_str()), ("F#1", "Eb2"));
    assert_eq!(b.max_polyphony, 1);
    assert!(b.steps[0] == 1.0 && b.steps[3] == 1.0 && b.steps[1] == 0.0);
    assert!(b.syncopation > 0.4, "{}", b.syncopation);
    assert!(
        s.traits.contains(&"syncopated".to_string()),
        "{:?}",
        s.traits
    );
    // One note at a time: its bars' roots, as the key's chords.
    assert_eq!(s.chords[0], "G#m");
    assert!(
        s.summary.contains("1-bar loop") && s.summary.contains("bass in G# minor"),
        "{}",
        s.summary
    );
}

#[test]
fn chords_go_round_their_progression_with_numerals() {
    // Am, F, C, G held a bar each: a pad (long notes, three at a time).
    let voicings = [[57u8, 60, 64], [53, 57, 60], [55, 60, 64], [55, 59, 62]];
    let notes: Vec<Note> = voicings
        .iter()
        .enumerate()
        .flat_map(|(b, v)| v.iter().map(move |&p| note(b as i64 * BAR, 16, p, 80)))
        .collect();
    let s = study(&file(vec![part("", 0, notes)]), "Bed 01");
    assert_eq!(s.role, "pad");
    assert_eq!(s.key.as_deref(), Some("A minor"));
    assert_eq!(s.chords, ["Am", "F", "C", "G"]);
    assert_eq!(s.progression, "i | VI | III | VII");
    assert_eq!(s.parts[0].max_polyphony, 3);
    assert!(
        s.parts[0].contour.is_none(),
        "chords have no melody's shape"
    );
}

/// Two bars of a beat: kick on the beats, snare on two and four, closed hats off the beats.
fn beat() -> Vec<Note> {
    let mut notes = Vec::new();
    for b in 0..2 {
        let at = b * BAR;
        for s in [0, 4, 8, 12] {
            notes.push(note(at + s * STEP, 1, 36, 120));
        }
        for s in [4, 12] {
            notes.push(note(at + s * STEP, 1, 38, 110));
        }
        for s in (2..16).step_by(4) {
            notes.push(note(at + s * STEP, 1, 42, 70));
        }
    }
    notes
}

#[test]
fn a_beat_is_each_drums_pattern_and_a_melody_its_shape() {
    let notes = beat();
    let drums = study(&file(vec![part("Beat", DRUMS, notes)]), "Groove");
    assert_eq!(drums.role, "drums");
    let rows = &drums.parts[0].drums;
    let row = |n: &str| rows.iter().find(|r| r.0 == n).map(|r| r.1.clone());
    assert_eq!(row("kick").as_deref(), Some("9000 9000 9000 9000"));
    assert_eq!(row("snare").as_deref(), Some("0000 9000 0000 9000"));
    assert_eq!(row("closed hat").as_deref(), Some("0090 0090 0090 0090"));
    assert!(drums.key.is_none());
    // The same beat on channel 1, as packs often write it: a drum file by its name and notes,
    // put on drums to come in on them.
    let mut on_one = file(vec![part("Beat", 0, beat())]);
    let s = study(&on_one, "Drum Loop 01");
    assert_eq!(s.role, "drums");
    mark_drums(&mut on_one, &s);
    assert!(on_one.parts[0].is_drums());
    // A melody climbing by step: its shape.
    let tune: Vec<Note> = [60u8, 62, 64, 65, 67, 69, 71, 72]
        .iter()
        .enumerate()
        .map(|(i, &p)| note(i as i64 * 4 * STEP, 3, p, 90))
        .collect();
    let lead = study(&file(vec![part("", 0, tune)]), "LD Rise - C");
    let c = lead.parts[0].contour.clone().expect("a melody's shape");
    assert_eq!((c.steps, c.leaps, c.span), (1.0, 0.0, 12));
    assert_eq!(c.direction, "climbs");
    assert_eq!(lead.role, "lead");
}

#[test]
fn files_are_found_by_role_and_key_and_moved_into_it() {
    let g = |s: &str| Key::parse(s).unwrap();
    assert_eq!(transpose_to(&g("G# minor"), &g("A minor")), 1);
    assert_eq!(transpose_to(&g("A minor"), &g("C major")), 0);
    assert_eq!(transpose_to(&g("B minor"), &g("G minor")), -4);
    let sample = |name: &str, role: &str, key: &str| MidiSample {
        path: PathBuf::from(format!("/m/{name}.mid")),
        name: name.into(),
        folder: "Pack".into(),
        modified: 0,
        size: 0,
        study: MidiStudy {
            role: role.into(),
            key: Some(key.into()),
            parts: vec![PartStudy {
                role: role.into(),
                ..Default::default()
            }],
            summary: format!("a {role}"),
            ..Default::default()
        },
    };
    let lib = vec![
        sample("BS Kila - G#m", "bass", "G# minor"),
        sample("PD Newt - Am", "pad", "A minor"),
        sample("BS Toro - Am", "bass", "A minor"),
    ];
    let hits = find(
        &lib,
        &MidiQuery {
            role: Some("bass".into()),
            key: Some("A minor".into()),
            ..Default::default()
        },
    );
    assert_eq!(hits.iter().map(|h| h.index).collect::<Vec<_>>(), [2, 0]);
    assert_eq!((hits[0].transpose, hits[1].transpose), (0, 1));
    let words = find(
        &lib,
        &MidiQuery {
            words: "pad".into(),
            ..Default::default()
        },
    );
    assert_eq!(words[0].index, 1);
}

mod written_and_grouped {
    use super::super::*;
    use crate::song::midi_file::{DRUMS, Part};

    const BAR: u64 = PPQ * 4;

    fn file(notes: &str, key: &str) -> MidiFile {
        MidiFile {
            parts: vec![Part {
                name: "Keys".into(),
                channel: 0,
                program: None,
                notes: parse_notes(notes, BAR).unwrap(),
                controls: Vec::new(),
            }],
            tempo: Some(124.0),
            tempos: Vec::new(),
            time_signature: Some([4, 4]),
            key: Key::parse(key),
            name: None,
        }
    }

    #[test]
    fn notes_read_as_they_were_written() {
        let notes = parse_notes(
            "1.1 C4 2 96\n1.3.5 Db4 1\n\n2.1 c#-1 4 110\n3.1 60 1 200",
            BAR,
        )
        .unwrap();
        assert_eq!(notes[0].pitch, 60);
        assert_eq!(notes[0].length, PPQ / 2);
        assert_eq!(notes[1].start, ((PPQ / 4 * 5) / 2) as i64);
        assert_eq!(notes[1].pitch, 61);
        assert_eq!(notes[1].velocity, 100);
        assert_eq!(notes[2].pitch, 1);
        assert_eq!(notes[3].velocity, 127);
        // What `notes_text` writes comes back the same.
        let part = Part {
            name: String::new(),
            channel: 0,
            program: None,
            notes: notes.clone(),
            controls: Vec::new(),
        };
        let again = parse_notes(&notes_text(&part, BAR), BAR).unwrap();
        assert_eq!(again[0], notes[0]);
        for bad in ["C4", "0.1 C4", "1.1 H4", "1.0 C4", "1.1 C4 x"] {
            assert!(parse_notes(bad, BAR).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_written_file_reads_back_the_same() {
        let dir = std::env::temp_dir().join(format!("pina-midi-write-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("made.mid");
        let mut f = file("1.1 C4 2 96\n1.3 E4 2 90\n2.1 G4 4 110", "A minor");
        f.parts.push(Part {
            name: "Beat".into(),
            channel: DRUMS,
            program: None,
            notes: parse_notes("1.1 C2 1 120\n1.5 D2 1 100", BAR).unwrap(),
            controls: Vec::new(),
        });
        std::fs::write(&path, f.to_bytes()).unwrap();
        let back = midi_file::open(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(back.parts.len(), 2);
        assert_eq!(back.parts[0].notes, f.parts[0].notes);
        assert!(back.parts[1].is_drums());
        assert_eq!(back.parts[1].notes, f.parts[1].notes);
        assert_eq!(back.tempo.map(|t| t.round()), Some(124.0));
        assert_eq!(back.time_signature, Some([4, 4]));
        assert_eq!(back.key, Key::parse("A minor"));
    }

    #[test]
    fn a_group_says_what_it_shares_and_how_close_another_is() {
        // The same off-beat stab figure, in two keys.
        let a = file(
            "1.3 A3 1 100\n1.7 C4 1 90\n1.11 E4 1 100\n1.15 A3 1 90",
            "A minor",
        );
        let b = file(
            "1.3 D4 1 100\n1.7 F4 1 90\n1.11 A4 1 100\n1.15 D4 1 90",
            "D minor",
        );
        let (sa, sb) = (study(&a, "a"), study(&b, "b"));
        let group = shared(&[(&a, &sa), (&b, &sb)]).unwrap();
        assert_eq!(group.files, 2);
        assert!(group.alike > 0.9, "{}", group.alike);
        assert_eq!(group.steps[2], 1.0);
        assert_eq!(group.steps[0], 0.0);
        // A new one on their rhythm and degrees, in another key: it's one of them.
        let like = file(
            "1.3 E4 1 100\n1.7 G4 1 95\n1.11 B4 1 100\n1.15 E4 1 95",
            "E minor",
        );
        let s = study(&like, "like");
        let near = group.likeness(&like, &s).unwrap();
        assert!(near.score > 0.9 && near.advice.is_empty(), "{near:?}");
        // Whole notes on the beat, nothing like them: it says what's wrong.
        let far = file("1.1 C2 16 40\n2.1 C2 16 40", "C major");
        let s = study(&far, "far");
        let off = group.likeness(&far, &s).unwrap();
        assert!(off.score < near.score - 0.2, "{off:?}");
        assert!(off.advice.iter().any(|a| a.contains("steps 3")), "{off:?}");
        assert!(shared(&[]).is_none());
        // Drums agree with themselves, with no notes of a key to compare.
        let mut beat = file(
            "1.1 C2 1 120\n1.5 D2 1 100\n1.9 C2 1 120\n1.13 D2 1 100",
            "C major",
        );
        beat.parts[0].channel = crate::song::midi_file::DRUMS;
        let s = study(&beat, "beat");
        let drums = shared(&[(&beat, &s)]).unwrap();
        assert!(drums.alike > 0.99, "{}", drums.alike);
    }
}

mod fitting_and_kits {
    use super::super::*;

    /// A file in a pack: its one part, key, tempo, and chords round its loop.
    fn file(
        folder: &str,
        name: &str,
        role: &str,
        key: Option<&str>,
        chords: &[&str],
    ) -> MidiSample {
        MidiSample {
            path: PathBuf::from(format!("/m/{folder}/{name}.mid")),
            name: name.into(),
            folder: folder.into(),
            modified: 0,
            size: 0,
            study: MidiStudy {
                role: role.into(),
                key: key.map(str::to_string),
                tempo: Some(120.0),
                bars: chords.len().max(1) as u64,
                loop_bars: chords.len().max(1) as u64,
                chords: chords.iter().map(|c| c.to_string()).collect(),
                parts: vec![PartStudy {
                    role: role.into(),
                    notes: 8,
                    steps: vec![0.5; 16],
                    velocity: [80, 100, 90],
                    density: 4.0,
                    ..Default::default()
                }],
                ..Default::default()
            },
        }
    }

    fn key(s: &str) -> Option<Key> {
        Key::parse(s)
    }

    #[test]
    fn what_fits_a_song_is_in_its_notes_or_moved_keeping_its_mood() {
        let e_minor = key("E minor");
        let of = |k: &str| file("P", k, "chords", Some(k), &[]);
        // As it is: the key, or its relative's notes.
        assert!(fits(&of("E minor"), e_minor, None, false));
        assert!(fits(&of("G major"), e_minor, None, false));
        assert!(!fits(&of("A minor"), e_minor, None, false));
        // Moved in, a minor file does; a major one would change the mood.
        assert!(fits(&of("A minor"), e_minor, None, true));
        assert!(!fits(&of("C major"), e_minor, None, true));
        // Drums need no key, and a file whose key can't be told can't be said to fit.
        assert!(fits(
            &file("P", "d", "drums", None, &[]),
            e_minor,
            None,
            false
        ));
        assert!(!fits(
            &file("P", "x", "lead", None, &[]),
            e_minor,
            None,
            true
        ));
        // A tempo, or half or double it; a file with none plays at the song's anyway.
        let mut slow = of("E minor");
        slow.study.tempo = Some(62.0);
        assert!(fits(&slow, None, Some(120.0), false));
        slow.study.tempo = Some(90.0);
        assert!(!fits(&slow, None, Some(120.0), false));
        slow.study.tempo = None;
        assert!(fits(&slow, None, Some(120.0), false));
    }

    #[test]
    fn a_file_with_drums_first_still_moves_and_asking_a_key_loses_nothing() {
        let mut song = file("P", "Song", "drums", Some("A minor"), &[]);
        song.study.parts.push(PartStudy {
            role: "bass".into(),
            ..Default::default()
        });
        assert!(!song.study.all_drums());
        assert!(file("P", "d", "drums", None, &[]).study.all_drums());
        let lib = vec![
            file("P", "Beat", "drums", None, &[]),
            file("P", "Bass", "bass", Some("A minor"), &[]),
        ];
        let q = MidiQuery {
            key: Some("E minor".into()),
            ..Default::default()
        };
        // Ranks: the beat, with no key to go on, is last, not gone.
        let hits: Vec<usize> = find(&lib, &q).iter().map(|h| h.index).collect();
        assert_eq!(hits, [1, 0]);
    }

    /// Two packs: one all on Am F C G, the other on another progression in another key.
    fn packs() -> Vec<MidiSample> {
        let go = ["Am", "F", "C", "G"];
        let other = ["Dm", "Dm", "Gm", "A"];
        vec![
            file("Other", "Keys", "chords", Some("D minor"), &other),
            file("Other", "Low", "bass", Some("D minor"), &other),
            file("Other", "Top", "lead", Some("D minor"), &[]),
            file("Other", "Kit", "drums", None, &[]),
            file("Night", "Chords", "chords", Some("A minor"), &go),
            file("Night", "Bass", "bass", Some("A minor"), &go),
            file("Night", "Lead", "lead", Some("A minor"), &[]),
            file("Night", "Beat", "drums", None, &[]),
        ]
    }

    fn picks(k: &MidiKit, lib: &[MidiSample]) -> Vec<String> {
        k.slots
            .iter()
            .filter_map(|s| Some(lib[s.pick.as_ref()?.index].folder.clone()))
            .collect()
    }

    #[test]
    fn a_kit_comes_from_one_pack_and_one_key() {
        let lib = packs();
        let k = kit(
            &lib,
            &MidiKitSeed {
                key: Some("E minor".into()),
                ..Default::default()
            },
        );
        let parts: Vec<&str> = k.slots.iter().map(|s| &s.role[..]).collect();
        assert_eq!(parts, ["drums", "bass", "chords", "lead"]);
        let packs = picks(&k, &lib);
        assert!(packs.iter().all(|p| *p == packs[0]), "{packs:?}");
        assert_eq!(k.key.as_deref(), Some("E minor"));
        // All moved into it, the drums not.
        for s in &k.slots {
            let h = s.pick.as_ref().unwrap();
            let m = &lib[h.index];
            match m.study.key.as_deref().and_then(Key::parse) {
                Some(from) => {
                    assert_eq!(h.transpose, transpose_to(&from, &key("E minor").unwrap()))
                }
                None => assert_eq!(h.transpose, 0),
            }
        }
    }

    #[test]
    fn the_same_roots_beat_the_same_pack_being_all_that_matches() {
        // Chords picked from one pack: of two basses from elsewhere, the one on its roots.
        let go = ["Am", "F", "C", "G"];
        let lib = vec![
            file("One", "Chords", "chords", Some("A minor"), &go),
            file(
                "Two",
                "Wrong",
                "bass",
                Some("A minor"),
                &["Am", "Am", "Am", "Dm"],
            ),
            file("Three", "Right", "bass", Some("C major"), &go),
        ];
        let k = kit(&lib, &MidiKitSeed::default());
        let bass = k.slots.iter().find(|s| s.role == "bass").unwrap();
        assert_eq!(lib[bass.pick.as_ref().unwrap().index].name, "Right");
    }

    #[test]
    fn new_picks_are_another_kit_that_still_holds_together() {
        let lib = packs();
        let names = |variation| {
            let k = kit(
                &lib,
                &MidiKitSeed {
                    variation,
                    ..Default::default()
                },
            );
            let packs = picks(&k, &lib);
            assert!(packs.iter().all(|p| *p == packs[0]), "{packs:?}");
            packs[0].clone()
        };
        assert_ne!(names(0), names(1), "a new kit");
    }

    #[test]
    fn a_locked_part_stays_and_the_rest_follow_it() {
        let lib = packs();
        let k = kit(
            &lib,
            &MidiKitSeed {
                keep: vec![("bass".into(), lib[1].path.clone())],
                variation: 7,
                ..Default::default()
            },
        );
        let bass = k.slots.iter().find(|s| s.role == "bass").unwrap();
        assert_eq!(bass.pick.as_ref().unwrap().index, 1);
        assert_eq!(k.key.as_deref(), Some("D minor"), "its key is the kit's");
        let packs = picks(&k, &lib);
        assert!(packs.iter().all(|p| p == "Other"), "{packs:?}");
    }

    #[test]
    fn only_one_kind_of_chords_and_one_kind_of_melody_play_at_first() {
        let mut lib = packs();
        lib.push(file("Night", "Pad", "pad", Some("A minor"), &[]));
        lib.push(file("Night", "Arp", "arp", Some("A minor"), &[]));
        let k = kit(&lib, &MidiKitSeed::default());
        let on: Vec<&str> = k
            .slots
            .iter()
            .filter(|s| s.on)
            .map(|s| &s.role[..])
            .collect();
        assert_eq!(on, ["drums", "bass", "chords", "lead"]);
        assert_eq!(k.slots.len(), 6);
    }

    #[test]
    fn the_map_gathers_what_plays_alike() {
        // Busy basses and slow pads, and each lands by its own.
        let mut lib = Vec::new();
        for i in 0..12 {
            let mut b = file("P", &format!("b{i}"), "bass", Some("A minor"), &[]);
            b.study.parts[0].steps = (0..16).map(|s| (s % 2) as f32).collect();
            b.study.parts[0].density = 12.0 + i as f32 * 0.1;
            let mut p = file("P", &format!("p{i}"), "pad", Some("A minor"), &[]);
            p.study.parts[0].steps = (0..16).map(|s| (s % 8 == 0) as u8 as f32).collect();
            p.study.parts[0].length = 12.0 + i as f32 * 0.1;
            lib.extend([b, p]);
        }
        let at = places(&lib);
        assert_eq!(at.len(), lib.len());
        let d = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
        let (mut near, mut far) = (0.0, 0.0);
        for i in 0..12 {
            near += d(at[i * 2], at[(i * 2 + 2) % 24]);
            far += d(at[i * 2], at[i * 2 + 1]);
        }
        assert!(far > near * 1.5, "alike {near}, unlike {far}");
    }
}
