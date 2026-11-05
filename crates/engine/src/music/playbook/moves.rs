//! What a book says to do, a move at a time, each one a thing to ask of a song; and a book's
//! chords and drums as notes, to hear on real instruments.

use super::Playbook;
use crate::{
    music::theory::{self, Chord, Drum},
    song::{
        midi_file::{DRUMS, MidiFile, Part},
        project::{Note, PPQ},
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Form,
    Harmony,
    Rhythm,
    Mix,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Form, Kind::Harmony, Kind::Rhythm, Kind::Mix];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Form => "Form",
            Kind::Harmony => "Harmony",
            Kind::Rhythm => "Rhythm",
            Kind::Mix => "Mix",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Move {
    pub kind: Kind,
    pub title: String,
    pub why: String,
    /// The bars it's about: where, and how many.
    pub at: Option<(u64, u64)>,
    /// What to ask of a song, to do it.
    pub ask: String,
}

fn part_name(role: &str) -> &str {
    match role {
        "perc" => "percussion",
        other => other,
    }
}

/// The moves a book makes: what its sections do, its chords, its rhythm and its mix.
pub fn moves(p: &Playbook) -> Vec<Move> {
    let mut at = 0;
    let starts: Vec<u64> = p
        .sections
        .iter()
        .map(|s| {
            let here = at;
            at += s.bars;
            here
        })
        .collect();
    let total = at;
    let mut out = Vec::new();
    if let Some(i) = p.sections.iter().position(|s| s.name.starts_with("Chorus"))
        && starts[i] >= 8
    {
        let bar = starts[i];
        out.push(Move {
            kind: Kind::Form,
            title: format!("Make them wait {bar} bars for the chorus"),
            why: format!(
                "{} starts at bar {} of {total}. Everything before it is a build.",
                p.sections[i].name,
                bar + 1
            ),
            at: Some((bar, p.sections[i].bars)),
            ask: format!(
                "Lay out my song so the first chorus lands at bar {}",
                bar + 1
            ),
        });
    }
    // The part that comes in last.
    let entries = p
        .parts
        .iter()
        .filter(|part| part.role != "drums")
        .filter_map(|part| {
            let i = p
                .sections
                .iter()
                .position(|s| s.parts.contains(&part.role))?;
            (i > 0).then_some((part, i))
        })
        .max_by_key(|(_, i)| *i);
    if let Some((part, i)) = entries {
        let name = part_name(&part.role);
        out.push(Move {
            kind: Kind::Form,
            title: format!("Hold the {name} back until bar {}", starts[i] + 1),
            why: format!(
                "It first plays in {}, so the song has something new to bring in.",
                p.sections[i].name
            ),
            at: Some((starts[i], p.sections[i].bars)),
            ask: format!("Keep the {name} out of my song until bar {}", starts[i] + 1),
        });
    }
    // Where the bass drops out, between sections that have it.
    let has_bass = p.parts.iter().any(|x| x.role == "bass");
    let inner = p.sections.len().saturating_sub(1);
    let dropped: Vec<usize> = (1..inner)
        .filter(|&i| has_bass && !p.sections[i].parts.iter().any(|x| x == "bass"))
        .collect();
    if let Some(&i) = dropped.first() {
        let word = p.sections[i].name.split(' ').next().unwrap_or("section");
        let word = word.to_lowercase();
        out.push(Move {
            kind: Kind::Form,
            title: format!("Take the bass out of the {word}s"),
            why: format!(
                "The bass stops in {} of {} sections, so what comes after hits harder.",
                dropped.len(),
                p.sections.len()
            ),
            at: Some((starts[i], p.sections[i].bars)),
            ask: format!("Mute the bass in my {word}s"),
        });
    }
    if let Some(last) = p.sections.last().filter(|s| !s.parts.is_empty()) {
        let names: Vec<&str> = last.parts.iter().map(|r| part_name(r)).collect();
        let title = names
            .iter()
            .take(2)
            .copied()
            .collect::<Vec<_>>()
            .join(" and ");
        out.push(Move {
            kind: Kind::Form,
            title: format!("End on {title}"),
            why: format!(
                "The last {} bars strip down to {}.",
                last.bars,
                names.join(", ")
            ),
            at: Some((total - last.bars, last.bars)),
            ask: "Strip my outro down the same way".into(),
        });
    }
    let mut round: Vec<&str> = Vec::new();
    for c in &p.chords {
        if !round.contains(&c.as_str()) {
            round.push(c);
        }
    }
    round.truncate(4);
    out.push(match round.is_empty() {
        true => Move {
            kind: Kind::Harmony,
            title: format!("Write {} chords", p.harmony),
            why: "This book keeps a word for its chords, not the chords themselves.".into(),
            at: None,
            ask: format!("Write me an 8-bar chord loop that feels {}", p.harmony),
        },
        false => Move {
            kind: Kind::Harmony,
            title: format!("Go round {}", round.join(" ")),
            why: "The song's own chords, its eight bars that move the most.".into(),
            at: None,
            ask: format!("Write me an 8-bar chord loop on {}", round.join(" ")),
        },
    });
    if let Some(b) = &p.beat {
        let hits: Vec<String> = b
            .kick
            .chars()
            .enumerate()
            .filter(|(_, c)| !matches!(c, '.' | 'g'))
            .map(|(i, _)| (i + 1).to_string())
            .collect();
        out.push(Move {
            kind: Kind::Rhythm,
            title: format!("Kick on steps {}", hits.join(", ")),
            why: format!(
                "The bar's 16 steps, from the song itself. Swing {:.0}.",
                p.swing
            ),
            at: None,
            ask: format!(
                "Program this drum bar in my song: kick {}, snare {}, hats {}, swing {:.0}",
                b.kick, b.snare, b.hats, p.swing
            ),
        });
    } else {
        out.push(Move {
            kind: Kind::Rhythm,
            title: format!("Use the {} groove", p.groove.replace('_', " ")),
            why: format!(
                "Swing {:.0}, at {:.0} to {:.0} BPM.",
                p.swing, p.tempo.0, p.tempo.1
            ),
            at: None,
            ask: format!("Program the {} groove in my song", p.groove),
        });
    }
    const BANDS: [&str; 6] = ["sub", "bass", "low mids", "mids", "presence", "air"];
    let top = (0..6)
        .max_by(|&a, &b| p.spectrum[a].total_cmp(&p.spectrum[b]))
        .unwrap_or(0);
    out.push(Move {
        kind: Kind::Mix,
        title: format!("Master to {} LUFS with {} on top", p.lufs, BANDS[top]),
        why: format!(
            "Sub {} dB, bass {} dB, air {} dB of the total.",
            p.spectrum[0], p.spectrum[1], p.spectrum[5]
        ),
        at: None,
        ask: format!("Match my mix to {}'s balance and loudness", p.name),
    });
    out
}

const BAR: u64 = PPQ * 4;
const STEP: u64 = BAR / 16;

fn voiced(chord: &Chord, start: u64) -> (Vec<Note>, Note) {
    let length = BAR - STEP / 2;
    let up = chord
        .classes()
        .iter()
        .map(|&c| Note {
            start: start as i64,
            length,
            pitch: 60 + c,
            velocity: 80,
            ..Default::default()
        })
        .collect();
    let bass = Note {
        start: start as i64,
        length,
        pitch: 36 + chord.root,
        velocity: 92,
        ..Default::default()
    };
    (up, bass)
}

fn part(name: &str, channel: u8, program: u8, notes: Vec<Note>) -> Part {
    Part {
        name: name.into(),
        channel,
        program: Some(program),
        notes,
        controls: Vec::new(),
    }
}

/// A book's drums, one row a drum, as steps: its song's own, else its groove's first bar.
pub fn beat_rows(p: &Playbook) -> Vec<(u8, String)> {
    match &p.beat {
        Some(b) => vec![
            (Drum::Kick.note(), b.kick.clone()),
            (Drum::Snare.note(), b.snare.clone()),
            (Drum::HatClosed.note(), b.hats.clone()),
        ],
        None => theory::groove(&p.groove)
            .rows
            .iter()
            .filter_map(|(d, bars)| Some((d.note(), bars.first()?.to_string())))
            .collect(),
    }
}

/// A book as notes: its chords a bar each over its drums, on an electric piano and a bass.
pub fn hearing(p: &Playbook) -> MidiFile {
    let chords: Vec<Chord> = p.chords.iter().filter_map(|c| Chord::parse(c)).collect();
    let bars = chords.len().max(2) as u64;
    let (mut keys, mut bass) = (Vec::new(), Vec::new());
    for (i, c) in chords.iter().enumerate() {
        let (up, low) = voiced(c, i as u64 * BAR);
        keys.extend(up);
        bass.push(low);
    }
    let late = ((p.swing - 50.0).max(0.0) / 50.0 * STEP as f32) as i64;
    let mut hits = Vec::new();
    for bar in 0..bars {
        for (pitch, row) in beat_rows(p) {
            for (i, c) in row.chars().enumerate() {
                let velocity = theory::hit(c);
                if velocity > 0 {
                    let off = if i % 2 == 1 { late } else { 0 };
                    hits.push(Note {
                        start: (bar * BAR + i as u64 * STEP) as i64 + off,
                        length: STEP,
                        pitch,
                        velocity,
                        ..Default::default()
                    });
                }
            }
        }
    }
    let mut parts = vec![part("Drums", DRUMS, 0, hits)];
    if !keys.is_empty() {
        parts.push(part("Keys", 0, 4, keys));
        parts.push(part("Bass", 1, 33, bass));
    }
    MidiFile {
        parts,
        tempo: Some((p.tempo.0 + p.tempo.1) / 2.0),
        tempos: Vec::new(),
        time_signature: Some([4, 4]),
        key: None,
        name: Some(p.name.clone()),
    }
}

/// One chord, a bar long, as notes.
pub fn chord_alone(name: &str, tempo: f64) -> Option<MidiFile> {
    let (keys, bass) = voiced(&Chord::parse(name)?, 0);
    Some(MidiFile {
        parts: vec![part("Keys", 0, 4, keys), part("Bass", 1, 33, vec![bass])],
        tempo: Some(tempo),
        tempos: Vec::new(),
        time_signature: Some([4, 4]),
        key: None,
        name: Some(name.into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::playbook::{Beat, Part as BookPart, Section};

    fn book() -> Playbook {
        let section = |name: &str, bars, parts: &[&str]| Section {
            name: name.into(),
            bars,
            parts: parts.iter().map(|s| s.to_string()).collect(),
            energy: 3,
            lufs: None,
        };
        let role = |r: &str| BookPart {
            role: r.into(),
            sound: String::new(),
            write: String::new(),
        };
        Playbook {
            id: "like".into(),
            name: "Like It".into(),
            tempo: (118.0, 126.0),
            minor: true,
            groove: "house".into(),
            swing: 50.0,
            harmony: "dark".into(),
            parts: vec![role("drums"), role("bass"), role("lead")],
            sections: vec![
                section("Intro", 16, &["drums", "bass"]),
                section("Verse", 8, &["drums", "lead"]),
                section("Chorus", 16, &["drums", "bass", "lead"]),
                section("Outro", 4, &["drums"]),
            ],
            lufs: -13.5,
            spectrum: [-6.0, -3.5, -7.0, -12.5, -18.0, -19.5],
            notes: Vec::new(),
            chords: vec!["Bbm".into(), "Gb".into(), "Ebm".into(), "Ebm".into()],
            numerals: Vec::new(),
            beat: Some(Beat {
                kick: "X...X...X...X...".into(),
                snare: "....X.......X...".into(),
                hats: "x.x.x.x.x.x.x.x.".into(),
            }),
            source: None,
        }
    }

    #[test]
    fn its_moves_come_from_what_it_does() {
        let m = moves(&book());
        let title = |kind| {
            m.iter()
                .filter(move |x| x.kind == kind)
                .map(|x| x.title.as_str())
        };
        let form: Vec<_> = title(Kind::Form).collect();
        assert_eq!(
            form,
            [
                "Make them wait 24 bars for the chorus",
                "Hold the lead back until bar 17",
                "Take the bass out of the verses",
                "End on drums",
            ]
        );
        assert_eq!(title(Kind::Harmony).next(), Some("Go round Bbm Gb Ebm"));
        assert_eq!(
            title(Kind::Rhythm).next(),
            Some("Kick on steps 1, 5, 9, 13")
        );
        let chorus = &m[0];
        assert_eq!(chorus.at, Some((24, 16)));
    }

    #[test]
    fn it_sounds_when_rendered_on_real_instruments() {
        let dir = std::env::temp_dir().join(format!("pina-book-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = hearing(&book());
        let [l, r] =
            crate::song::midi_file::render_all(&[(&file, 0)], 122.0, 48_000.0, &dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let a = crate::music::analysis::analyze(&l, &r, 48_000.0);
        // Four bars at 122, loud enough to hear, with a low end and notes in it.
        assert!(
            (l.len() as f32 / 48_000.0 - 240.0 * 4.0 / 122.0).abs() < 0.5,
            "{}",
            l.len()
        );
        assert!(a.lufs > -40.0, "{}", a.lufs);
        assert!(
            a.spectrum_db.bass > -30.0 && a.spectrum_db.mid > -50.0,
            "{:?}",
            a.spectrum_db
        );
        // And one chord alone.
        let one = chord_alone("Bbm", 122.0).unwrap();
        let dir = std::env::temp_dir().join(format!("pina-chord-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let [l, _] =
            crate::song::midi_file::render_all(&[(&one, 0)], 122.0, 48_000.0, &dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(l.iter().any(|v| v.abs() > 0.01));
    }

    #[test]
    fn a_book_without_its_songs_chords_says_so() {
        let mut b = book();
        b.chords.clear();
        b.beat = None;
        let m = moves(&b);
        assert!(m.iter().any(|x| x.title == "Write dark chords"));
        assert!(m.iter().any(|x| x.title == "Use the house groove"));
    }

    #[test]
    fn it_can_be_heard() {
        let f = hearing(&book());
        assert_eq!(f.parts.len(), 3);
        // Four bars of four kicks.
        let kicks = f.parts[0].notes.iter().filter(|n| n.pitch == 36).count();
        assert_eq!(kicks, 16);
        // Bbm over Bb: its first bar's notes.
        let first: Vec<u8> = f.parts[1]
            .notes
            .iter()
            .filter(|n| n.start == 0)
            .map(|n| n.pitch % 12)
            .collect();
        assert_eq!(first, [10, 1, 5]);
        assert_eq!(f.parts[2].notes[0].pitch % 12, 10);
    }
}
