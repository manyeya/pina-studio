//! Chords: their qualities and symbols, read from names and numerals, and named back.

use super::*;

/// A chord's kind, as the intervals over its root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quality {
    Maj,
    Min,
    Dim,
    Aug,
    Sus2,
    Sus4,
    Maj6,
    Min6,
    Maj7,
    Min7,
    Dom7,
    HalfDim,
    Dim7,
    MinMaj7,
    Add9,
    MinAdd9,
    Maj9,
    Min9,
    Dom9,
    Dom7b9,
    Dom7s9,
    Min11,
    Dom13,
    Maj7s11,
    Dom7sus4,
}

impl Quality {
    /// Semitones over the root, lowest first; past 12 are the extensions (9, 11, 13).
    pub fn intervals(self) -> &'static [u8] {
        match self {
            Quality::Maj => &[0, 4, 7],
            Quality::Min => &[0, 3, 7],
            Quality::Dim => &[0, 3, 6],
            Quality::Aug => &[0, 4, 8],
            Quality::Sus2 => &[0, 2, 7],
            Quality::Sus4 => &[0, 5, 7],
            Quality::Maj6 => &[0, 4, 7, 9],
            Quality::Min6 => &[0, 3, 7, 9],
            Quality::Maj7 => &[0, 4, 7, 11],
            Quality::Min7 => &[0, 3, 7, 10],
            Quality::Dom7 => &[0, 4, 7, 10],
            Quality::HalfDim => &[0, 3, 6, 10],
            Quality::Dim7 => &[0, 3, 6, 9],
            Quality::MinMaj7 => &[0, 3, 7, 11],
            Quality::Add9 => &[0, 4, 7, 14],
            Quality::MinAdd9 => &[0, 3, 7, 14],
            Quality::Maj9 => &[0, 4, 7, 11, 14],
            Quality::Min9 => &[0, 3, 7, 10, 14],
            Quality::Dom9 => &[0, 4, 7, 10, 14],
            Quality::Dom7b9 => &[0, 4, 7, 10, 13],
            Quality::Dom7s9 => &[0, 4, 7, 10, 15],
            Quality::Min11 => &[0, 3, 7, 10, 14, 17],
            Quality::Dom13 => &[0, 4, 7, 10, 14, 21],
            Quality::Maj7s11 => &[0, 4, 7, 11, 18],
            Quality::Dom7sus4 => &[0, 5, 7, 10],
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Quality::Maj => "",
            Quality::Min => "m",
            Quality::Dim => "dim",
            Quality::Aug => "aug",
            Quality::Sus2 => "sus2",
            Quality::Sus4 => "sus4",
            Quality::Maj6 => "6",
            Quality::Min6 => "m6",
            Quality::Maj7 => "maj7",
            Quality::Min7 => "m7",
            Quality::Dom7 => "7",
            Quality::HalfDim => "m7b5",
            Quality::Dim7 => "dim7",
            Quality::MinMaj7 => "mMaj7",
            Quality::Add9 => "add9",
            Quality::MinAdd9 => "madd9",
            Quality::Maj9 => "maj9",
            Quality::Min9 => "m9",
            Quality::Dom9 => "9",
            Quality::Dom7b9 => "7b9",
            Quality::Dom7s9 => "7#9",
            Quality::Min11 => "m11",
            Quality::Dom13 => "13",
            Quality::Maj7s11 => "maj7#11",
            Quality::Dom7sus4 => "7sus4",
        }
    }

    /// Its third is minor.
    pub fn minor(self) -> bool {
        self.intervals().contains(&3) && !self.intervals().contains(&4)
    }
}

/// Every way a chord's kind is written, longest first so "maj7" isn't read as "maj".
pub(crate) const SUFFIXES: &[(&str, Quality)] = &[
    ("maj7#11", Quality::Maj7s11),
    ("mmaj7", Quality::MinMaj7),
    ("mMaj7", Quality::MinMaj7),
    ("m(maj7)", Quality::MinMaj7),
    ("7sus4", Quality::Dom7sus4),
    ("madd9", Quality::MinAdd9),
    ("maj13", Quality::Maj9),
    ("maj9", Quality::Maj9),
    ("maj7", Quality::Maj7),
    ("min11", Quality::Min11),
    ("min9", Quality::Min9),
    ("min7", Quality::Min7),
    ("min6", Quality::Min6),
    ("m7b5", Quality::HalfDim),
    ("dim7", Quality::Dim7),
    ("add9", Quality::Add9),
    ("sus2", Quality::Sus2),
    ("sus4", Quality::Sus4),
    ("sus", Quality::Sus4),
    ("7b9", Quality::Dom7b9),
    ("7#9", Quality::Dom7s9),
    ("m11", Quality::Min11),
    ("m13", Quality::Min11),
    ("m9", Quality::Min9),
    ("m7", Quality::Min7),
    ("m6", Quality::Min6),
    ("M7", Quality::Maj7),
    ("Δ7", Quality::Maj7),
    ("Δ", Quality::Maj7),
    ("-7", Quality::Min7),
    ("ø7", Quality::HalfDim),
    ("ø", Quality::HalfDim),
    ("°7", Quality::Dim7),
    ("°", Quality::Dim),
    ("dim", Quality::Dim),
    ("aug", Quality::Aug),
    ("min", Quality::Min),
    ("maj", Quality::Maj),
    ("13", Quality::Dom13),
    ("11", Quality::Dom7sus4),
    ("9", Quality::Dom9),
    ("7", Quality::Dom7),
    ("6", Quality::Maj6),
    ("+", Quality::Aug),
    ("m", Quality::Min),
    ("-", Quality::Min),
    ("", Quality::Maj),
];

pub(crate) const SHARP: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

pub(crate) const FLAT: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
];

/// A chord: its root (a pitch class, 0 is C), its kind, and a bass under it if not its root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub root: u8,
    pub quality: Quality,
    pub bass: Option<u8>,
}

/// A note name's pitch class, and what follows it: "Eb7" is 3 and "7".
pub(crate) fn pitch_class(s: &str) -> Option<(u8, &str)> {
    let mut chars = s.chars();
    let base = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest = chars.as_str();
    let (shift, rest) = match rest.chars().next() {
        Some('#' | '♯') => (1, &rest[rest.chars().next().unwrap().len_utf8()..]),
        Some('b' | '♭') => (-1, &rest[rest.chars().next().unwrap().len_utf8()..]),
        _ => (0, rest),
    };
    Some((((base + shift + 12) % 12) as u8, rest))
}

impl Chord {
    /// Reads "Am9", "Fmaj9", "E7b9", "Bb", "F#m7b5", "Dsus4", "C/E", "Gø".
    pub fn parse(s: &str) -> Option<Chord> {
        let s = s.trim();
        let (body, bass) = match s.split_once('/') {
            Some((b, over)) => (b, Some(pitch_class(over)?.0)),
            None => (s, None),
        };
        let (root, rest) = pitch_class(body)?;
        let quality = SUFFIXES
            .iter()
            .find(|(text, _)| rest == *text)
            .map(|(_, q)| *q)?;
        Some(Chord {
            root,
            quality,
            bass: bass.filter(|b| *b != root),
        })
    }

    /// "Am9", spelled with flats in flat keys.
    pub fn name_in(&self, key: Option<&Key>) -> String {
        let flats = key.is_some_and(|k| {
            let (root, minor) = (k.root, k.minor);
            match minor {
                true => [0, 2, 5, 7, 10, 3].contains(&root),
                false => [5, 10, 3, 8, 1].contains(&root),
            }
        }) || matches!(self.root, 3 | 8 | 10) && key.is_none();
        let names = if flats { FLAT } else { SHARP };
        let mut s = format!("{}{}", names[self.root as usize], self.quality.suffix());
        if let Some(b) = self.bass {
            s.push('/');
            s.push_str(names[b as usize]);
        }
        s
    }

    pub fn name(&self) -> String {
        self.name_in(None)
    }

    /// Its pitch classes, root first.
    pub fn classes(&self) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        for i in self.quality.intervals() {
            let pc = (self.root + i) % 12;
            if !out.contains(&pc) {
                out.push(pc);
            }
        }
        if let Some(b) = self.bass
            && !out.contains(&b)
        {
            out.push(b);
        }
        out
    }

    pub fn has(&self, pitch: u8) -> bool {
        self.classes().contains(&(pitch % 12))
    }

    /// Its third (or what stands for it in a sus chord) and seventh: the notes that say what it
    /// is, which a line moving between chords leads by.
    pub fn guide_tones(&self) -> Vec<u8> {
        let iv = self.quality.intervals();
        iv.iter()
            .filter(|i| matches!(i, 2..=5 | 9..=11))
            .map(|i| (self.root + i) % 12)
            .collect()
    }
}

/// Degrees of the major and natural minor scales, in semitones.
pub(crate) const MAJOR: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];

pub(crate) const MINOR: [u8; 7] = [0, 2, 3, 5, 7, 8, 10];

/// Reads a roman numeral in a key: "ii9", "V7b9", "Imaj9", "bVII", "iv", "vii°", "iiø7", "III".
/// Upper case is major, lower minor; the numbers and marks after say the rest. In a minor key the
/// degrees are the natural minor's (VI is a minor sixth up), unless marked # or b.
pub fn numeral(s: &str, key: &Key) -> Option<Chord> {
    let s = s.trim();
    let (shift, rest): (i8, &str) = match s.chars().next()? {
        'b' | '♭' => (-1, &s[s.chars().next()?.len_utf8()..]),
        '#' | '♯' => (1, &s[s.chars().next()?.len_utf8()..]),
        _ => (0, s),
    };
    let numerals = ["vii", "iii", "vi", "iv", "ii", "v", "i"];
    let lower = rest.to_lowercase();
    let n = numerals.iter().find(|n| lower.starts_with(*n))?;
    let degree = match *n {
        "i" => 0,
        "ii" => 1,
        "iii" => 2,
        "iv" => 3,
        "v" => 4,
        "vi" => 5,
        _ => 6,
    };
    let written = &rest[..n.len()];
    let upper = written.chars().next()?.is_uppercase();
    let mark = &rest[n.len()..];
    let scale = if key.minor { MINOR } else { MAJOR };
    let root = ((key.root as i8 + scale[degree] as i8 + shift).rem_euclid(12)) as u8;
    let quality = match (upper, mark) {
        (_, "°" | "dim" | "o") => Quality::Dim,
        (_, "°7" | "dim7" | "o7") => Quality::Dim7,
        (_, "ø" | "ø7" | "m7b5") => Quality::HalfDim,
        (_, "+" | "aug") => Quality::Aug,
        (_, "sus4" | "sus") => Quality::Sus4,
        (_, "sus2") => Quality::Sus2,
        (_, "7sus4") => Quality::Dom7sus4,
        (_, "maj7#11") => Quality::Maj7s11,
        (true, "") => Quality::Maj,
        (false, "") => Quality::Min,
        (true, "6") => Quality::Maj6,
        (false, "6") => Quality::Min6,
        (true, "7") => Quality::Dom7,
        (false, "7") => Quality::Min7,
        (true, "maj7" | "M7") => Quality::Maj7,
        (false, "maj7" | "M7") => Quality::MinMaj7,
        (true, "9") => Quality::Dom9,
        (false, "9") => Quality::Min9,
        (true, "maj9") => Quality::Maj9,
        (false, "maj9") => Quality::Min9,
        (true, "add9") => Quality::Add9,
        (false, "add9") => Quality::MinAdd9,
        (_, "7b9") => Quality::Dom7b9,
        (_, "7#9") => Quality::Dom7s9,
        (false, "11") => Quality::Min11,
        (true, "11") => Quality::Dom7sus4,
        (true, "13") => Quality::Dom13,
        (false, "13") => Quality::Min11,
        _ => return None,
    };
    Some(Chord {
        root,
        quality,
        bass: None,
    })
}

/// A chord written either way: "Am9" or (in `key`) "i9".
pub fn read_chord(s: &str, key: &Key) -> Option<Chord> {
    Chord::parse(s).or_else(|| numeral(s, key))
}

/// The chord a set of pitch classes plays best: the qualities that hold the most of them, the
/// lowest note preferred as root. For reading chords from notes when a song has no chord track.
pub fn name_notes(pcs: &[(u8, f32)], lowest: Option<u8>) -> Option<Chord> {
    if pcs.is_empty() {
        return None;
    }
    let candidates = [
        Quality::Maj9,
        Quality::Min9,
        Quality::Dom9,
        Quality::Maj7,
        Quality::Min7,
        Quality::Dom7,
        Quality::HalfDim,
        Quality::Maj,
        Quality::Min,
        Quality::Dim,
        Quality::Sus4,
    ];
    let mut best: Option<(f32, Chord)> = None;
    for root in 0..12u8 {
        for q in candidates {
            let c = Chord {
                root,
                quality: q,
                bass: None,
            };
            let tones = c.classes();
            let held: f32 = pcs
                .iter()
                .filter(|(p, _)| tones.contains(p))
                .map(|(_, w)| w)
                .sum();
            let missed: f32 = pcs
                .iter()
                .filter(|(p, _)| !tones.contains(p))
                .map(|(_, w)| w)
                .sum();
            let absent = tones
                .iter()
                .filter(|t| !pcs.iter().any(|(p, _)| p == *t))
                .count() as f32;
            let mut score = held - missed * 1.5 - absent * 0.35;
            if lowest == Some(root) {
                score += 0.6;
            }
            if best.as_ref().is_none_or(|(s, _)| score > *s) {
                best = Some((score, c));
            }
        }
    }
    best.map(|(_, c)| c)
}
