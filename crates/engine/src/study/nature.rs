//! What a file is, before anything is said about it: a whole song, a loop or a stem, and what's
//! in it. The sample library's models listen (a few windows of a long file, so a stem and a song
//! can be told apart), its name is a strong word on top, and the rest of the study then says
//! only what can be true of such a file: drums have no key, a bass line has notes and not
//! chords, a piano has no groove. Measured on 50 labelled loops: no loop without drums is told
//! it has them.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{Element, Groove, Study};
use crate::{
    music::Key,
    samples::{Category, analyze, classify, rhythm},
};

/// A file's length past which it's taken for a song, unless it's all one instrument.
const LOOP_SECONDS: f32 = 45.0;
/// How much of a long file each window hears.
const WINDOW_SECONDS: f32 = 16.0;

/// How far a finding can be trusted.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Sure {
    /// Two things that don't depend on each other agree.
    Solid,
    /// One says so, and nothing against it counts for much.
    #[default]
    Likely,
    /// A guess: shown with what else it could be.
    Unsure,
}

/// What's in a file, which decides what can be said about it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Makeup {
    /// Several things at once: anything can be in it.
    #[default]
    Mix,
    /// Drums alone: no key, no chords.
    Drums,
    /// One line, a note at a time (a bass): notes, not chords.
    Line,
    /// One instrument: its chords or its melody, no groove.
    Instrument,
    /// A voice.
    Voice,
    /// Noise, effects, atmosphere.
    Texture,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Nature {
    pub makeup: Makeup,
    /// A whole song, not a part of one.
    pub song: bool,
    /// What to call it: "Piano loop", "Vocal stem", "Full song".
    pub name: String,
    /// What it is played on, when it's one thing: "Piano", "Bass".
    pub instrument: String,
    /// The part it plays, as playbooks name parts.
    pub role: String,
    pub sure: Sure,
    pub why: Vec<String>,
    /// What was listened for and isn't there.
    pub absent: Vec<String>,
}

/// What's known before the audio's measured: from its name, and what it is.
#[derive(Clone, Debug, Default)]
pub struct Given {
    /// A tempo in its name, and whether that's only a bare number the audio has to agree with.
    pub tempo: Option<(f32, bool)>,
    pub key: Option<Key>,
    /// It's a loop, so it should last a whole number of bars.
    pub looped: bool,
    /// One line, a note at a time: what's read from it is notes, not chords.
    pub line: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Family {
    Drums,
    Bass,
    Voice,
    Pitched,
    Noise,
}

fn family(c: Category) -> Family {
    use Category::*;
    match c {
        Kick | Snare | Clap | Rim | Snap | HatClosed | HatOpen | Ride | Crash | Tom | Shaker
        | Tambourine | Perc | Drums => Family::Drums,
        Bass808 | Bass => Family::Bass,
        Vocal => Family::Voice,
        Fx | Reverse | Texture | Foley | Other => Family::Noise,
        _ => Family::Pitched,
    }
}

fn word(c: Category, piano: bool) -> &'static str {
    use Category::*;
    match c {
        Keys if piano => "Piano",
        Keys => "Keys",
        Pad => "Pad",
        Lead => "Lead",
        Pluck => "Pluck",
        Bell => "Bell",
        Strings => "Strings",
        Brass => "Brass",
        Woodwind => "Woodwind",
        Guitar => "Guitar",
        Synth => "Synth",
        Chord => "Chord",
        Vocal => "Vocal",
        Bass => "Bass",
        Bass808 => "808",
        Kick => "Kick",
        Snare => "Snare",
        Clap => "Clap",
        Rim => "Rim",
        Snap => "Snap",
        HatClosed | HatOpen => "Hat",
        Ride => "Ride",
        Crash => "Crash",
        Tom => "Tom",
        Shaker => "Shaker",
        Tambourine => "Tambourine",
        Perc => "Percussion",
        Drums => "Drum",
        Fx | Reverse => "Effect",
        _ => "Texture",
    }
}

fn role(c: Category) -> &'static str {
    use Category::*;
    match (family(c), c) {
        (Family::Drums, Drums | Kick | Snare | Clap | Rim | Snap) => "drums",
        (Family::Drums, _) => "perc",
        (Family::Bass, _) => "bass",
        (Family::Voice, _) => "vocals",
        (Family::Noise, _) => "texture",
        (_, Pad) => "pad",
        (_, Lead | Synth | Bell | Brass | Woodwind) => "lead",
        _ => "chords",
    }
}

/// Each family's share of what a window heard.
fn share(window: &[(Category, f32)], f: Family) -> f32 {
    window
        .iter()
        .filter(|(c, _)| family(*c) == f)
        .map(|(_, p)| p)
        .sum()
}

fn mean(windows: &[Vec<(Category, f32)>], f: Family) -> f32 {
    windows.iter().map(|w| share(w, f)).sum::<f32>() / windows.len().max(1) as f32
}

/// The likeliest category of a family, across the windows.
fn likeliest(windows: &[Vec<(Category, f32)>], f: Family) -> Option<Category> {
    Category::ALL
        .iter()
        .filter(|c| family(**c) == f)
        .map(|&c| {
            let p: f32 = windows
                .iter()
                .flat_map(|w| w.iter().filter(move |(x, _)| *x == c))
                .map(|(_, p)| p)
                .sum();
            (c, p)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(c, _)| c)
}

/// What a file is, from what its windows heard, how long it is, and the category its own name
/// says. `piano` when its name says piano, not just keys.
fn decide(
    windows: &[Vec<(Category, f32)>],
    seconds: f32,
    named: Option<Category>,
    piano: bool,
    loop_named: bool,
) -> Nature {
    let long = seconds > LOOP_SECONDS;
    let families = [
        Family::Drums,
        Family::Bass,
        Family::Voice,
        Family::Pitched,
        Family::Noise,
    ];
    let (heard, heard_share) = families
        .iter()
        .map(|&f| (f, mean(windows, f)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or((Family::Noise, 0.0));
    let steady = windows
        .iter()
        .map(|w| share(w, heard))
        .fold(1.0f32, f32::min);
    let top = |f: Family| likeliest(windows, f).unwrap_or(Category::Other);
    let drums_share = mean(windows, Family::Drums);
    // The song or mix it is if nothing says otherwise: all of a song's windows hear drums.
    let whole = |why: Vec<String>, sure: Sure| Nature {
        makeup: Makeup::Mix,
        song: long,
        name: if long { "Full song" } else { "Mixed loop" }.into(),
        sure,
        why,
        ..Default::default()
    };
    let (chosen, sure, why): (Category, Sure, Vec<String>) = match named {
        // A song can have any word in its title; a stem's name has to fit its sound.
        Some(c) if long && family(c) != heard && drums_share > 0.45 => {
            return whole(
                vec![
                    format!("{seconds:.0} seconds long"),
                    "Drums and more in every part of it".into(),
                ],
                Sure::Solid,
            );
        }
        Some(c) if family(c) == heard && heard_share >= 0.35 => (
            c,
            Sure::Solid,
            vec![
                "Its file name says so".into(),
                format!("Its sound agrees, {:.0}%", heard_share * 100.0),
            ],
        ),
        Some(c) => (
            c,
            Sure::Likely,
            vec![
                "Its file name says so".into(),
                format!(
                    "By sound alone it came back as {}, {:.0}%",
                    word(top(heard), false).to_lowercase(),
                    heard_share * 100.0
                ),
            ],
        ),
        None => {
            let settled = match heard {
                Family::Drums => !long && heard_share >= 0.7,
                Family::Bass | Family::Voice | Family::Noise => {
                    heard_share >= 0.5 && (!long || steady >= 0.45)
                }
                Family::Pitched => {
                    heard_share >= 0.6 && (!long || steady >= 0.45) && drums_share < 0.2
                }
            };
            if !settled {
                let why = match long {
                    true => vec![
                        format!("{seconds:.0} seconds long"),
                        "Several things heard at once".into(),
                    ],
                    false => vec!["Several things heard at once".into()],
                };
                return whole(why, if long { Sure::Solid } else { Sure::Likely });
            }
            (
                top(heard),
                Sure::Likely,
                vec![format!("By sound, {:.0}%", heard_share * 100.0)],
            )
        }
    };
    let makeup = match family(chosen) {
        Family::Drums => Makeup::Drums,
        Family::Bass => Makeup::Line,
        Family::Voice => Makeup::Voice,
        Family::Pitched => Makeup::Instrument,
        Family::Noise => Makeup::Texture,
    };
    let instrument = word(chosen, piano);
    let noun = if long && !loop_named { "stem" } else { "loop" };
    let looked = [
        ("Drums", Family::Drums),
        ("Bass line", Family::Bass),
        ("Vocals", Family::Voice),
    ];
    let mut absent: Vec<String> = looked
        .iter()
        .filter(|(_, f)| *f != family(chosen) && mean(windows, *f) < 0.15)
        .map(|(name, _)| name.to_string())
        .collect();
    match makeup {
        Makeup::Drums => {
            absent.insert(0, "Key".into());
            absent.insert(1, "Chords".into());
        }
        Makeup::Line | Makeup::Voice | Makeup::Texture => absent.insert(0, "Chords".into()),
        _ => {}
    }
    Nature {
        makeup,
        song: false,
        name: format!("{instrument} {noun}"),
        instrument: match instrument {
            "Drum" => "Drums".into(),
            i => i.into(),
        },
        role: role(chosen).into(),
        sure,
        why,
        absent,
    }
}

/// What the file is, heard and from its name.
pub fn of(l: &[f32], r: &[f32], rate: f64, path: &Path) -> Nature {
    let seconds = l.len() as f32 / rate as f32;
    let width = (rate as f32 * WINDOW_SECONDS) as usize;
    let starts = match l.len() <= width * 3 {
        true => vec![0],
        false => vec![
            l.len() / 6,
            l.len() / 2 - width / 2,
            l.len() * 5 / 6 - width,
        ],
    };
    let windows: Vec<Vec<(Category, f32)>> = starts
        .iter()
        .map(|&s| {
            let e = (s + width.max(l.len().min(width))).min(l.len());
            analyze::hear(&[l[s..e].to_vec(), r[s..e].to_vec()], rate).1
        })
        .collect();
    let name = path
        .file_name()
        .map_or(String::new(), |n| n.to_string_lossy().to_lowercase());
    let hints = classify::hints(&name);
    let named = hints
        .categories
        .iter()
        .copied()
        .filter(|(_, s)| *s >= 0.5)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(c, _)| c);
    decide(
        &windows,
        seconds,
        named,
        name.contains("piano"),
        name.contains("loop"),
    )
}

/// What's known of a file before it's measured.
pub fn given(path: &Path, n: &Nature) -> Given {
    let name = path
        .file_name()
        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
    let lower = name.to_lowercase();
    Given {
        // A bare number in a loop's name is its tempo: packs name them so.
        tempo: rhythm::named_tempo(&lower).map(|(v, weak)| (v, weak && !lower.contains("loop"))),
        key: match n.song {
            true => None,
            false => named_key(&name),
        },
        looped: !n.song,
        line: n.makeup == Makeup::Line,
    }
}

/// A key a file's name gives: "F#min", "Emin", "Fm", "G# minor", "Cmaj". Case counts ("Am" is a
/// key, "am" a word), and a note alone isn't one.
pub fn named_key(name: &str) -> Option<Key> {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    let words: Vec<&str> = stem
        .split(['_', ' ', '-', '(', ')', '[', ']', ',', '•'])
        .filter(|w| !w.is_empty())
        .collect();
    (0..words.len())
        .rev()
        .find_map(|i| key_word(words[i], words.get(i + 1).copied()))
}

fn key_word(word: &str, next: Option<&str>) -> Option<Key> {
    let mut letters = word.chars();
    let note = letters.next().filter(|c| ('A'..='G').contains(c))?;
    let rest = letters.as_str();
    let (flat, rest) = match rest.strip_prefix(['#', '♯']) {
        Some(r) => ("#", r),
        None => match rest.strip_prefix(['b', '♭']) {
            Some(r) => ("b", r),
            None => ("", rest),
        },
    };
    let quality = if rest.is_empty() { next? } else { rest };
    let minor = match quality {
        "m" | "min" | "Min" | "MIN" | "minor" | "Minor" | "MINOR" => true,
        "maj" | "Maj" | "MAJ" | "major" | "Major" | "MAJOR" | "M" => false,
        _ => return None,
    };
    Key::parse(&format!(
        "{note}{flat} {}",
        if minor { "minor" } else { "major" }
    ))
}

/// The whole bars of 4/4 a loop of `seconds` lasts at `bpm`, and what's left over as its tail.
fn whole_bars(seconds: f32, bpm: f32) -> Option<(u32, f32)> {
    [32, 16, 8, 4, 2, 1].iter().find_map(|&n| {
        let length = n as f32 * 240.0 / bpm;
        let over = seconds - length;
        (over.abs() <= length * 0.03 || (over > 0.0 && over <= length * 0.3))
            .then_some((n, over.max(0.0)))
    })
}

/// How a tempo a beat tracker heard stands to another, in words, when it's a simple ratio: the
/// usual way a tracker goes wrong.
fn related(heard: f32, tempo: f32) -> Option<&'static str> {
    [
        (1.0, "the same"),
        (0.5, "half of it"),
        (2.0, "twice it"),
        (2.0 / 3.0, "two thirds of it"),
        (1.5, "one and a half times it"),
        (0.75, "three quarters of it"),
        (4.0 / 3.0, "four thirds of it"),
    ]
    .iter()
    .find(|(r, _)| (heard / tempo / r - 1.0).abs() < 0.02)
    .map(|(_, w)| *w)
}

/// The tempo to go by, if it isn't what the beat tracker heard; how sure; and why.
pub fn tempo(g: &Given, heard: f32, steady: f32, seconds: f32) -> (Option<f32>, Sure, Vec<String>) {
    let length = |v: f32| -> Option<String> {
        whole_bars(seconds, v).map(|(n, tail)| match tail > 0.05 {
            true => format!(
                "{n} bars at {v:.0} is {:.1} seconds, then {tail:.1} seconds of tail",
                n as f32 * 240.0 / v
            ),
            false => format!("It lasts exactly {n} bars at {v:.0}"),
        })
    };
    let fits = |v: f32| g.looped && whole_bars(seconds, v).is_some();
    if let Some((v, weak)) = g.tempo
        && (!weak || (g.looped && (related(heard, v).is_some() || fits(v))))
    {
        let mut why = vec![format!("The file name says {v:.0}")];
        let close = related(heard, v);
        if close == Some("the same") {
            why.push(format!("The beat tracker heard {heard:.0}"));
        }
        if g.looped
            && let Some(l) = length(v)
        {
            why.push(l);
        }
        let agrees = close == Some("the same") || fits(v);
        if close != Some("the same") {
            why.push(match close {
                Some(w) => format!("The beat tracker heard {heard:.0}, {w}, so that's set aside"),
                None => format!("The beat tracker heard {heard:.0}, set aside"),
            });
        }
        return (
            Some(v),
            if agrees { Sure::Solid } else { Sure::Likely },
            why,
        );
    }
    if g.looped {
        // No name to go by: a loop should fill whole bars, at a tempo a tracker could mistake.
        let exact = |v: f32| whole_bars(seconds, v).is_some_and(|(_, tail)| tail <= 0.05);
        if exact(heard) {
            return (
                None,
                Sure::Likely,
                vec![
                    format!("The beat tracker heard {heard:.0}"),
                    length(heard).unwrap_or_default(),
                ],
            );
        }
        let better = [2.0, 0.5, 1.5, 2.0 / 3.0, 4.0 / 3.0, 0.75]
            .iter()
            .map(|r| heard * r)
            .find(|&v| (85.0..=150.0).contains(&v) && exact(v));
        if let Some(v) = better {
            return (
                Some(v),
                Sure::Likely,
                vec![
                    format!("The beat tracker heard {heard:.0}"),
                    length(v).unwrap_or_default(),
                ],
            );
        }
        return (
            None,
            Sure::Unsure,
            vec![
                format!("The beat tracker heard {heard:.0}"),
                "It doesn't fill whole bars at that tempo".into(),
            ],
        );
    }
    (
        None,
        if steady >= 0.7 {
            Sure::Solid
        } else {
            Sure::Likely
        },
        vec![format!("The beat tracker heard {heard:.0}, steadily")],
    )
}

/// How sure a key is, why, and what else it could be.
pub fn key(given: Option<Key>, heard: Key, fit: f32) -> (Sure, Vec<String>, Option<String>) {
    let relative = |k: Key| Key {
        root: (k.root + if k.minor { 3 } else { 9 }) % 12,
        minor: !k.minor,
    };
    let percent = (fit * 100.0).round();
    match given {
        Some(k) if k == heard => (
            Sure::Solid,
            vec![
                format!("The file name says {}", k.name()),
                format!("Its notes fit it, {percent}%"),
            ],
            None,
        ),
        Some(k) if relative(k) == heard => (
            Sure::Solid,
            vec![
                format!("The file name says {}", k.name()),
                format!(
                    "By sound it hears {}, which has the same notes",
                    heard.name()
                ),
            ],
            None,
        ),
        Some(k) => (
            Sure::Likely,
            vec![
                format!("The file name says {}", k.name()),
                format!("By sound it hears {}", heard.name()),
            ],
            None,
        ),
        None if fit >= 0.6 => (
            Sure::Likely,
            vec![format!("Its notes fit it, {percent}%")],
            None,
        ),
        None => (
            Sure::Unsure,
            vec![
                format!("Its notes fit it only {percent}%"),
                format!(
                    "It could be {}, which has the same notes",
                    relative(heard).name()
                ),
            ],
            Some(relative(heard).name()),
        ),
    }
}

/// The study, cut to what's true of what the file is.
pub fn settle(s: &mut Study, nature: Nature) {
    let n = &nature;
    if !n.song {
        s.genres.clear();
    }
    // A song's words mean nothing in a loop: it has parts, not an intro and an outro.
    if s.identity.seconds <= LOOP_SECONDS {
        for x in &mut s.sections {
            x.name = format!("Part {}", x.label);
        }
    }
    if matches!(n.makeup, Makeup::Mix) {
        s.nature = nature;
        return;
    }
    let everywhere: Vec<usize> = (0..s.sections.len()).collect();
    let element = Element {
        name: n.instrument.clone(),
        role: n.role.clone(),
        confidence: match n.sure {
            Sure::Solid => 0.95,
            Sure::Likely => 0.7,
            Sure::Unsure => 0.4,
        },
        about: n.why.join(". "),
        sections: everywhere,
    };
    match n.makeup {
        Makeup::Drums | Makeup::Voice | Makeup::Texture => {
            s.chords.clear();
            for x in &mut s.sections {
                x.chords.clear();
                x.numerals.clear();
            }
        }
        Makeup::Line => {
            // One note isn't a chord: what's kept is the note.
            let mut notes: Vec<super::ChordAt> = Vec::new();
            for c in s.chords.drain(..) {
                let note: String = c
                    .chord
                    .char_indices()
                    .take_while(|&(i, ch)| i == 0 || ch == '#' || ch == 'b')
                    .map(|(_, ch)| ch)
                    .collect();
                match notes.last_mut() {
                    Some(last) if last.chord == note => last.end = c.end,
                    _ => notes.push(super::ChordAt {
                        chord: note,
                        numeral: String::new(),
                        ..c
                    }),
                }
            }
            s.chords = notes;
            for x in &mut s.sections {
                x.numerals.clear();
                x.chords = x
                    .chords
                    .split(" | ")
                    .map(|c| {
                        c.char_indices()
                            .take_while(|&(i, ch)| i == 0 || ch == '#' || ch == 'b')
                            .map(|(_, ch)| ch)
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join(" | ");
            }
        }
        _ => {}
    }
    if matches!(n.makeup, Makeup::Drums | Makeup::Texture) {
        s.key.name.clear();
        s.key.fit = 0.0;
        s.key.changes.clear();
        s.key.other = None;
    }
    if !matches!(n.makeup, Makeup::Drums) {
        s.groove = Groove::none();
    }
    s.instruments = vec![element];
    s.nature = nature;
}

#[cfg(test)]
mod tests {
    use super::*;
    use Category as C;

    fn window(heard: &[(Category, f32)]) -> Vec<(Category, f32)> {
        heard.to_vec()
    }

    #[test]
    fn a_piano_loop_is_a_piano_loop_with_or_without_its_name() {
        let w = [window(&[
            (C::Keys, 0.54),
            (C::Strings, 0.28),
            (C::Synth, 0.02),
        ])];
        let named = decide(&w, 16.0, Some(C::Keys), true, true);
        assert_eq!(named.name, "Piano loop");
        assert_eq!(named.makeup, Makeup::Instrument);
        assert_eq!(named.sure, Sure::Solid);
        assert!(named.absent.contains(&"Drums".to_string()));
        let bare = decide(&w, 16.0, None, false, false);
        assert_eq!(bare.name, "Keys loop");
        assert_eq!(bare.sure, Sure::Likely);
    }

    #[test]
    fn a_song_full_of_drums_is_a_song() {
        let w = [
            window(&[(C::Drums, 0.66), (C::Bass, 0.17)]),
            window(&[(C::Drums, 0.68), (C::Texture, 0.10)]),
            window(&[(C::Drums, 0.65), (C::Bass, 0.20)]),
        ];
        let s = decide(&w, 381.0, None, false, false);
        assert!(s.song);
        assert_eq!(s.makeup, Makeup::Mix);
        assert_eq!(s.name, "Full song");
        // "Bass" in a song's title doesn't make it a bass stem.
        assert!(decide(&w, 381.0, Some(C::Bass), false, false).song);
    }

    #[test]
    fn a_vocal_stem_goes_by_its_name() {
        let w = [
            window(&[(C::Synth, 0.41), (C::Strings, 0.26), (C::Texture, 0.17)]),
            window(&[(C::Synth, 0.25), (C::Texture, 0.20), (C::Guitar, 0.19)]),
            window(&[(C::Synth, 0.35), (C::Texture, 0.24), (C::Strings, 0.16)]),
        ];
        let s = decide(&w, 387.0, Some(C::Vocal), false, false);
        assert_eq!(s.name, "Vocal stem");
        assert_eq!(s.makeup, Makeup::Voice);
        assert_eq!(s.sure, Sure::Likely);
    }

    #[test]
    fn what_can_not_be_told_is_a_mix() {
        let w = [window(&[
            (C::Synth, 0.38),
            (C::Texture, 0.20),
            (C::Bass, 0.15),
        ])];
        let s = decide(&w, 12.0, None, false, false);
        assert_eq!(s.makeup, Makeup::Mix);
        assert_eq!(s.name, "Mixed loop");
    }

    #[test]
    fn a_key_in_a_name() {
        let k = |n: &str| named_key(n).map(|k| k.name());
        assert_eq!(
            k("TPS - Piano Loop 02 F#min (120 BPM).wav").unwrap(),
            "F# minor"
        );
        assert_eq!(k("AFR3_Bass_Loop_Chaos_130_Fm.wav").unwrap(), "F minor");
        assert_eq!(
            k("PML_AH2_Bass_Loop_012_122BPM_Bmin.wav").unwrap(),
            "B minor"
        );
        assert_eq!(k("AFRO LOOP • G minor bass 007.wav").unwrap(), "G minor");
        assert_eq!(k("Lead Cmaj 120.wav").unwrap(), "C major");
        assert_eq!(k("Drum loop_4_120.wav"), None);
        assert_eq!(k("Bass Loop 04 A.wav"), None);
    }

    #[test]
    fn a_tempo_in_a_name_beats_a_tracker_that_is_off_by_a_ratio() {
        let g = Given {
            tempo: Some((122.0, false)),
            looped: true,
            ..Default::default()
        };
        let (v, sure, why) = tempo(&g, 81.3, 1.0, 15.74);
        assert_eq!(v, Some(122.0));
        assert_eq!(sure, Sure::Solid);
        assert!(why.iter().any(|w| w.contains("two thirds")), "{why:?}");
    }

    #[test]
    fn a_bare_number_in_a_name_has_to_fit() {
        let g = Given {
            tempo: Some((130.0, true)),
            looped: true,
            ..Default::default()
        };
        assert_eq!(tempo(&g, 72.9, 1.0, 8.86).0, Some(130.0));
        assert_eq!(tempo(&g, 110.0, 1.0, 11.0).0, None);
    }

    #[test]
    fn a_loop_without_a_name_goes_by_whole_bars() {
        let g = Given {
            looped: true,
            ..Default::default()
        };
        let (v, sure, _) = tempo(&g, 80.0, 1.0, 16.0);
        assert_eq!(v, Some(120.0));
        assert_eq!(sure, Sure::Likely);
    }

    #[test]
    fn a_key_it_cant_settle_says_what_else_it_could_be() {
        let g = Key {
            root: 7,
            minor: false,
        };
        let (sure, _, other) = key(None, g, 0.49);
        assert_eq!(sure, Sure::Unsure);
        assert_eq!(other.as_deref(), Some("E minor"));
    }
}
