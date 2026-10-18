//! Writing music that's right, so an agent (which can't hear) picks and tempo makes it correct:
//! chords from symbols or roman numerals, progressions by genre and mood, voicings led smoothly
//! from chord to chord, basslines and melodies that follow the harmony, and drum grooves by
//! genre with their swing, ghosts, rolls, variations and fills. All of it the same every time
//! for a seed.

use serde::{Deserialize, Serialize};

use crate::{music::Key, project::Note};

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
const SUFFIXES: &[(&str, Quality)] = &[
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

const SHARP: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
const FLAT: [&str; 12] = [
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
fn pitch_class(s: &str) -> Option<(u8, &str)> {
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
const MAJOR: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
const MINOR: [u8; 7] = [0, 2, 3, 5, 7, 8, 10];

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

/// A progression from the library: its numerals, bar by bar ("ii9 V13 | Imaj9"), for a minor or
/// major key, and the genres and moods it suits.
pub struct Progression {
    pub numerals: &'static str,
    pub minor: bool,
    pub suits: &'static [&'static str],
}

/// Progressions producers use, by what they suit.
pub const PROGRESSIONS: &[Progression] = &[
    // Lo-fi, neo-soul, jazz-hop, R&B.
    Progression {
        numerals: "ii9 | V13 | Imaj9 | vi9",
        minor: false,
        suits: &[
            "lofi", "neo_soul", "jazz", "rnb", "chill", "smooth", "dreamy",
        ],
    },
    Progression {
        numerals: "IVmaj9 | iii7 | vi9 | ii9 V7b9",
        minor: false,
        suits: &["lofi", "neo_soul", "rnb", "jazz", "warm", "chill"],
    },
    Progression {
        numerals: "IVmaj7 | III7 | vi7 | v7 I7",
        minor: false,
        suits: &["rnb", "neo_soul", "smooth", "lofi", "funk"],
    },
    Progression {
        numerals: "Imaj9 | vi9 | ii9 | V13",
        minor: false,
        suits: &["rnb", "smooth", "neo_soul", "gospel", "warm"],
    },
    Progression {
        numerals: "VImaj9 | V7b9 | i9 | vii9 III7b9",
        minor: true,
        suits: &["lofi", "neo_soul", "jazz", "dark", "moody", "chill"],
    },
    Progression {
        numerals: "i9 | iv9 | VII9 | IIImaj9",
        minor: true,
        suits: &["lofi", "rnb", "neo_soul", "chill", "moody", "boom_bap"],
    },
    Progression {
        numerals: "i9 | VImaj7 | iv9 | V7b9",
        minor: true,
        suits: &["rnb", "jazz", "sad", "moody", "lofi"],
    },
    // Hip-hop, trap, drill.
    Progression {
        numerals: "i | VI | III | VII",
        minor: true,
        suits: &["trap", "pop", "epic", "emotional", "reggaeton", "cinematic"],
    },
    Progression {
        numerals: "i | iv | VI | V",
        minor: true,
        suits: &["trap", "dark", "drill", "boom_bap"],
    },
    Progression {
        numerals: "i | VI | iv | V",
        minor: true,
        suits: &["dark", "cinematic", "trap", "sad"],
    },
    Progression {
        numerals: "i | i | VI | VII",
        minor: true,
        suits: &["drill", "dark", "trap", "tense"],
    },
    Progression {
        numerals: "i | bII | i | bII",
        minor: true,
        suits: &["drill", "dark", "tense", "trap", "phrygian"],
    },
    Progression {
        numerals: "i7 | iv7 | v7 | i7",
        minor: true,
        suits: &["boom_bap", "jazz", "lofi", "moody"],
    },
    Progression {
        numerals: "i | VII | VI | V",
        minor: true,
        suits: &["dramatic", "sad", "flamenco", "trap", "cinematic"],
    },
    // Pop, EDM, house.
    Progression {
        numerals: "I | V | vi | IV",
        minor: false,
        suits: &["pop", "happy", "edm", "uplifting", "afrobeats", "rock"],
    },
    Progression {
        numerals: "vi | IV | I | V",
        minor: false,
        suits: &["pop", "emotional", "edm", "sad", "future_bass", "rock"],
    },
    Progression {
        numerals: "I | vi | IV | V",
        minor: false,
        suits: &["pop", "happy", "doo_wop", "uplifting"],
    },
    Progression {
        numerals: "IVmaj7 | V | iii7 | vi",
        minor: false,
        suits: &[
            "pop",
            "emotional",
            "future_bass",
            "house",
            "uplifting",
            "jpop",
        ],
    },
    Progression {
        numerals: "i9 | iv9 | i9 | iv9",
        minor: true,
        suits: &["deep_house", "house", "techno", "hypnotic", "minimal"],
    },
    Progression {
        numerals: "i | VII | VI | VII",
        minor: true,
        suits: &["house", "trance", "epic", "edm", "dnb"],
    },
    Progression {
        numerals: "i | i | i | i",
        minor: true,
        suits: &["techno", "hypnotic", "minimal", "drone"],
    },
    // Afrobeats, amapiano.
    Progression {
        numerals: "ii7 | V7 | Imaj7 | IVmaj7",
        minor: false,
        suits: &["afrobeats", "amapiano", "soulful", "warm"],
    },
    Progression {
        numerals: "ii9 | V9 | Imaj9 | IVmaj9",
        minor: false,
        suits: &["amapiano", "afrobeats", "jazz", "soulful", "deep_house"],
    },
    Progression {
        numerals: "i9 | VImaj9 | iv9 | VII9",
        minor: true,
        suits: &["amapiano", "deep_house", "afro_house", "moody"],
    },
    Progression {
        numerals: "vi | IV | I | V",
        minor: false,
        suits: &["afrobeats", "dancehall", "pop"],
    },
    // Gospel.
    Progression {
        numerals: "Imaj7 | iii7 | IVmaj7 #iv°7 | V7sus4 V7",
        minor: false,
        suits: &["gospel", "rnb", "soulful", "neo_soul"],
    },
    Progression {
        numerals: "IVmaj9 | iii7 vi7 | ii9 | V13",
        minor: false,
        suits: &["gospel", "neo_soul", "rnb", "jazz"],
    },
    // Cinematic, dnb, reggaeton, dreamy.
    Progression {
        numerals: "VI | VII | i | i",
        minor: true,
        suits: &["cinematic", "epic", "uplifting", "trailer"],
    },
    Progression {
        numerals: "i9 | VImaj7 | III | VII",
        minor: true,
        suits: &["dnb", "liquid", "emotional", "future_bass"],
    },
    Progression {
        numerals: "i | iv | VII | III",
        minor: true,
        suits: &["reggaeton", "latin", "dancehall", "pop"],
    },
    Progression {
        numerals: "Imaj7 | IVmaj7 | Imaj7 | IVmaj7",
        minor: false,
        suits: &["dreamy", "ambient", "chill", "lofi", "indie"],
    },
    Progression {
        numerals: "Imaj9 | IVmaj7#11 | Imaj9 | IVmaj7#11",
        minor: false,
        suits: &["dreamy", "lydian", "ambient", "film"],
    },
];

/// A seeded random number, the same every time for a seed.
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03)
    }

    pub fn bits(&mut self) -> u64 {
        // SplitMix64.
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// 0 to n-1.
    pub fn below(&mut self, n: usize) -> usize {
        (self.bits() % n.max(1) as u64) as usize
    }

    /// 0 to 1.
    pub fn unit(&mut self) -> f32 {
        (self.bits() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }
}

/// A progression for a key, a genre or mood words ("lofi", "dark sad"): the library's that suit
/// them, one picked by `seed`; any of the key's kind if none says so.
pub fn suggest(key: &Key, words: &str, seed: u64) -> &'static Progression {
    let words: Vec<String> = words
        .to_lowercase()
        .replace(['-', ' '], "_")
        .split(['_', ','])
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    let joined = words.join("_");
    let fits = |p: &&Progression| p.minor == key.minor;
    let score = |p: &Progression| {
        p.suits
            .iter()
            .filter(|s| joined.contains(**s) || words.iter().any(|w| s.starts_with(w.as_str())))
            .count()
    };
    let best = PROGRESSIONS
        .iter()
        .filter(fits)
        .map(score)
        .max()
        .unwrap_or(0);
    let pool: Vec<&Progression> = PROGRESSIONS
        .iter()
        .filter(fits)
        .filter(|p| score(p) == best)
        .collect();
    pool[Rng::new(seed).below(pool.len())]
}

/// A chord and when it plays, in ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub start: u64,
    pub end: u64,
    pub chord: Chord,
}

/// A progression written out: bars split by "|", each bar's chords sharing it equally; chords as
/// symbols or numerals. Repeated to fill `bars` bars from `start`.
pub fn lay_out(
    text: &str,
    key: &Key,
    start: u64,
    bars: u64,
    bar: u64,
) -> Result<Vec<Span>, String> {
    let groups: Vec<Vec<Chord>> = text
        .split('|')
        .map(|g| {
            g.split_whitespace()
                .map(|c| read_chord(c, key).ok_or(format!("{c:?} isn't a chord tempo reads")))
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<_, _>>()?;
    let groups: Vec<Vec<Chord>> = groups.into_iter().filter(|g| !g.is_empty()).collect();
    if groups.is_empty() {
        return Err("no chords".into());
    }
    let mut out: Vec<Span> = Vec::new();
    for b in 0..bars {
        let g = &groups[b as usize % groups.len()];
        let each = bar / g.len() as u64;
        for (i, c) in g.iter().enumerate() {
            let s = start + b * bar + i as u64 * each;
            let e = if i + 1 == g.len() {
                start + (b + 1) * bar
            } else {
                s + each
            };
            match out.last_mut() {
                // The same chord again carries on.
                Some(last) if last.chord == *c && last.end == s => last.end = e,
                _ => out.push(Span {
                    start: s,
                    end: e,
                    chord: *c,
                }),
            }
        }
    }
    Ok(out)
}

/// How a chord is spread over the keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Voicing {
    /// Its notes close together.
    Close,
    /// Its root low, the rest close above: a pianist's left hand and right.
    #[default]
    Open,
    /// Without its root, for when a bass plays it.
    Rootless,
    /// Root, third and fifth only.
    Triad,
    /// Spread over two octaves, for pads and strings.
    Spread,
}

/// Every way to place `classes` (in order, lowest first) as pitches within `lo..=hi`.
fn placements(classes: &[u8], lo: u8, hi: u8) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for r in 0..classes.len() {
        let order: Vec<u8> = classes[r..].iter().chain(&classes[..r]).copied().collect();
        for octave in 0..11u8 {
            let mut v = Vec::new();
            let mut at = octave as i32 * 12 + order[0] as i32;
            v.push(at);
            for &pc in &order[1..] {
                let mut p = (at / 12) * 12 + pc as i32;
                while p <= at {
                    p += 12;
                }
                v.push(p);
                at = p;
            }
            if v.iter().all(|&p| p >= lo as i32 && p <= hi as i32) {
                out.push(v.into_iter().map(|p| p as u8).collect());
            }
        }
    }
    out
}

/// How far one voicing moves to another: each voice's step, by their places.
fn movement(a: &[u8], b: &[u8]) -> u32 {
    let n = a.len().min(b.len());
    let mut cost: u32 = (0..n)
        .map(|i| (a[i] as i32 - b[i] as i32).unsigned_abs())
        .sum();
    cost += (a.len().abs_diff(b.len()) as u32) * 6;
    cost
}

/// A chord voiced in `register` (lowest and highest pitch for its upper notes), led from `prev`
/// by the smallest moves; with none before, centred.
pub fn voice(chord: &Chord, prev: Option<&[u8]>, voicing: Voicing, register: (u8, u8)) -> Vec<u8> {
    let iv = chord.quality.intervals();
    let pcs = |ivs: &[u8]| -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        for i in ivs {
            let pc = (chord.root + i) % 12;
            if !out.contains(&pc) {
                out.push(pc);
            }
        }
        out
    };
    // The upper notes: the colours first; the fifth goes when there are many.
    let mut upper: Vec<u8> = match voicing {
        Voicing::Triad => pcs(&iv[..3.min(iv.len())]),
        _ => pcs(&iv[1..]),
    };
    if matches!(voicing, Voicing::Triad | Voicing::Close) {
        upper = pcs(iv);
        if voicing == Voicing::Triad {
            upper = pcs(&iv[..3.min(iv.len())]);
        }
    }
    let fifth = (chord.root + 7) % 12;
    if upper.len() > 4 && upper.contains(&fifth) {
        upper.retain(|&p| p != fifth);
    }
    upper.truncate(5);
    let (lo, hi) = register;
    let centre = (lo as i32 + hi as i32) / 2;
    let candidates = placements(&upper, lo, hi);
    let previous_upper: Option<Vec<u8>> = prev.map(|p| {
        let mut p = p.to_vec();
        p.sort_unstable();
        // The upper notes of what came before (its low root left out).
        if p.len() > upper.len() {
            p.split_off(p.len() - upper.len())
        } else {
            p
        }
    });
    let pick = candidates
        .into_iter()
        .min_by_key(|v| match &previous_upper {
            Some(p) => {
                movement(v, p) * 4
                    + (v.iter().map(|&x| x as i32).sum::<i32>() / v.len() as i32 - centre)
                        .unsigned_abs()
            }
            None => {
                (v.iter().map(|&x| x as i32).sum::<i32>() / v.len() as i32 - centre).unsigned_abs()
                    * 4
            }
        })
        .unwrap_or_else(|| {
            upper
                .iter()
                .map(|&pc| lo + (pc + 12 - lo % 12) % 12)
                .collect()
        });
    let mut notes = pick;
    match voicing {
        Voicing::Open => {
            // The root (or slash bass) an octave or so under, in the left hand's place.
            let bass = chord.bass.unwrap_or(chord.root);
            let low = lo.saturating_sub(12).max(36);
            let mut b = low - low % 12 + bass;
            if b < low {
                b += 12;
            }
            while b + 3 > notes[0] {
                b -= 12;
            }
            notes.insert(0, b);
        }
        Voicing::Spread => {
            // Root low, fifth, then the rest an octave up.
            let low = lo.saturating_sub(12).max(36);
            let mut root = low - low % 12 + chord.root;
            if root < low {
                root += 12;
            }
            let mut spread = vec![root, root + 7];
            spread.extend(
                notes
                    .iter()
                    .map(|&n| if n < root + 12 { n + 12 } else { n }),
            );
            spread.sort_unstable();
            spread.dedup();
            notes = spread.into_iter().filter(|&n| n <= 108).collect();
        }
        _ => {}
    }
    notes
}

/// How chords are played in time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChordRhythm {
    /// Held for as long as each lasts.
    #[default]
    Held,
    /// Held, each coming an eighth early: pushed, as in neo-soul and pop.
    Push,
    /// Short hits on the beat and the and of two.
    Stabs,
    /// A player comping: on one, the and of two, and a short one on four.
    Comp,
    /// Eighths, as a pop or trance piano pulses.
    Pulse,
    /// On the offbeats, as house stabs are.
    Offbeat,
    /// Up its notes in sixteenths.
    Arp,
}

/// A note at a tick, as a region wants it.
fn note(start: u64, length: u64, pitch: u8, velocity: u8) -> Note {
    Note {
        start: start as i64,
        length: length.max(1),
        pitch,
        velocity: velocity.clamp(1, 127),
    }
}

/// Chords played: each voiced from the one before, in a rhythm, humanized a little by `seed`.
/// Ticks are from `origin` (the region's start).
pub fn play_chords(
    spans: &[Span],
    voicing: Voicing,
    rhythm: ChordRhythm,
    register: (u8, u8),
    beat: u64,
    origin: u64,
    seed: u64,
) -> Vec<Note> {
    let mut rng = Rng::new(seed);
    let mut prev: Option<Vec<u8>> = None;
    let mut out = Vec::new();
    let eighth = beat / 2;
    let sixteenth = beat / 4;
    for (i, s) in spans.iter().enumerate() {
        let v = voice(&s.chord, prev.as_deref(), voicing, register);
        prev = Some(v.clone());
        let (start, end) = (s.start - origin, s.end - origin);
        // A player leans on some chords more than others.
        let vel =
            |rng: &mut Rng, base: u8| base.saturating_add(rng.below(15) as u8).saturating_sub(7);
        let hit = |out: &mut Vec<Note>, at: u64, len: u64, base: u8, rng: &mut Rng| {
            for (k, &p) in v.iter().enumerate() {
                // A player's hand spreads a chord a hair, low to high.
                let roll = (k as u64) * 4;
                out.push(note(
                    at + roll,
                    len.saturating_sub(roll + 10),
                    p,
                    vel(rng, base),
                ));
            }
        };
        match rhythm {
            ChordRhythm::Held => hit(&mut out, start, end - start, 92, &mut rng),
            ChordRhythm::Push => {
                let at = if i == 0 || start < eighth {
                    start
                } else {
                    start - eighth
                };
                let to = if i + 1 < spans.len() {
                    end - eighth
                } else {
                    end
                };
                hit(&mut out, at, to - at, 92, &mut rng)
            }
            ChordRhythm::Stabs | ChordRhythm::Comp | ChordRhythm::Pulse | ChordRhythm::Offbeat => {
                // Hits within each beat-span of the chord, as (offset in beats, length, velocity).
                let pattern: &[(f32, f32, u8)] = match rhythm {
                    ChordRhythm::Stabs => &[(0.0, 0.4, 100), (1.5, 0.4, 84)],
                    ChordRhythm::Comp => &[(0.0, 1.35, 96), (1.5, 0.45, 74), (3.0, 0.4, 66)],
                    ChordRhythm::Pulse => &[
                        (0.0, 0.45, 100),
                        (0.5, 0.45, 72),
                        (1.0, 0.45, 88),
                        (1.5, 0.45, 72),
                        (2.0, 0.45, 96),
                        (2.5, 0.45, 72),
                        (3.0, 0.45, 88),
                        (3.5, 0.45, 72),
                    ],
                    _ => &[
                        (0.5, 0.35, 96),
                        (1.5, 0.35, 88),
                        (2.5, 0.35, 96),
                        (3.5, 0.35, 88),
                    ],
                };
                let bar = beat * 4;
                let mut b0 = start;
                while b0 < end {
                    for &(at, len, base) in pattern {
                        let t = b0 + (at * beat as f32) as u64;
                        if t >= start && t < end {
                            let l = ((len * beat as f32) as u64).min(end - t);
                            hit(&mut out, t, l, base, &mut rng);
                        }
                    }
                    b0 += bar;
                }
            }
            ChordRhythm::Arp => {
                let mut ups: Vec<u8> = v.clone();
                ups.sort_unstable();
                let mut t = start;
                let mut k = 0;
                while t < end {
                    let p = ups[k % ups.len()];
                    out.push(note(
                        t,
                        sixteenth - 10,
                        p,
                        vel(&mut rng, if k % 4 == 0 { 96 } else { 78 }),
                    ));
                    t += sixteenth;
                    k += 1;
                }
            }
        }
    }
    out
}

/// How a bass plays the chords.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BassStyle {
    /// Each chord's root, held.
    #[default]
    Roots,
    /// Root on one, fifth on three.
    RootFifth,
    /// Quarter notes walking to each next root.
    Walking,
    /// Eighths jumping octaves, disco and house.
    Octaves,
    /// On the offbeats, as house bass is.
    Offbeat,
    /// Funk and neo-soul: on one, the and of two, three, and a half step into the next root.
    Syncopated,
    /// On the kick, its root held to the next: how a trap 808 plays.
    Kick,
}

/// The pitch for `pc` nearest `near`, within `lo..=hi`.
fn nearest(pc: u8, near: i32, lo: u8, hi: u8) -> u8 {
    let mut best = None;
    for octave in 0..11 {
        let p = octave * 12 + pc as i32;
        if p < lo as i32 || p > hi as i32 {
            continue;
        }
        if best.is_none_or(|b: i32| (p - near).abs() < (b - near).abs()) {
            best = Some(p);
        }
    }
    best.unwrap_or((lo as i32 / 12) * 12 + pc as i32) as u8
}

/// A bassline for the chords. `kicks` (ticks from `origin`) are where the kick plays, for the
/// Kick style. Ticks are from `origin`.
#[allow(clippy::too_many_arguments)]
pub fn bassline(
    spans: &[Span],
    style: BassStyle,
    register: (u8, u8),
    beat: u64,
    origin: u64,
    kicks: &[(u64, u8)],
    key: &Key,
    seed: u64,
) -> Vec<Note> {
    let mut rng = Rng::new(seed);
    let (lo, hi) = register;
    let mut last = (lo as i32 + hi as i32) / 2 - 4;
    let mut out = Vec::new();
    let bar = beat * 4;
    let chord_at = |t: u64| {
        spans
            .iter()
            .find(|s| s.start - origin <= t && t < s.end - origin)
    };
    let push = |out: &mut Vec<Note>, last: &mut i32, t: u64, len: u64, pc: u8, vel: u8| {
        let p = nearest(pc, *last, lo, hi);
        *last = p as i32;
        out.push(note(t, len, p, vel));
    };
    match style {
        BassStyle::Kick => {
            for (i, &(t, v)) in kicks.iter().enumerate() {
                let Some(s) = chord_at(t) else { continue };
                let next = kicks.get(i + 1).map_or(s.end - origin, |k| k.0);
                let len = (next - t).min(bar).saturating_sub(20);
                let root = s.chord.bass.unwrap_or(s.chord.root);
                push(&mut out, &mut last, t, len, root, v.max(80));
            }
        }
        _ => {
            for (i, s) in spans.iter().enumerate() {
                let (start, end) = (s.start - origin, s.end - origin);
                let root = s.chord.bass.unwrap_or(s.chord.root);
                let fifth = (s.chord.root + 7) % 12;
                let third = (s.chord.root + if s.chord.quality.minor() { 3 } else { 4 }) % 12;
                let next_root = spans
                    .get(i + 1)
                    .map_or(root, |n| n.chord.bass.unwrap_or(n.chord.root));
                // The note under the next root, a half step: up into it or down.
                let approach = if rng.chance(0.5) {
                    (next_root + 11) % 12
                } else {
                    (next_root + 1) % 12
                };
                let v = |rng: &mut Rng, base: u8| {
                    base.saturating_add(rng.below(7) as u8).saturating_sub(3)
                };
                match style {
                    BassStyle::Roots => push(
                        &mut out,
                        &mut last,
                        start,
                        end - start - 20,
                        root,
                        v(&mut rng, 100),
                    ),
                    BassStyle::RootFifth
                    | BassStyle::Walking
                    | BassStyle::Syncopated
                    | BassStyle::Octaves
                    | BassStyle::Offbeat => {
                        // Bar by bar within the chord (a chord of half a bar gets its half).
                        let mut b0 = start;
                        while b0 < end {
                            let within = |t: u64| t >= start && t < end;
                            let last_bar = b0 + bar >= end;
                            let pattern: Vec<(f32, f32, u8, u8)> = match style {
                                BassStyle::RootFifth => {
                                    vec![(0.0, 1.8, root, 100), (2.0, 1.8, fifth, 88)]
                                }
                                BassStyle::Walking => {
                                    let tone = if rng.chance(0.5) { third } else { fifth };
                                    let step = if key.contains(fifth + 2) {
                                        (fifth + 2) % 12
                                    } else {
                                        fifth
                                    };
                                    vec![
                                        (0.0, 0.92, root, 100),
                                        (1.0, 0.92, tone, 84),
                                        (2.0, 0.92, step, 88),
                                        (3.0, 0.92, if last_bar { approach } else { fifth }, 80),
                                    ]
                                }
                                BassStyle::Syncopated => {
                                    let mut p = vec![
                                        (0.0, 1.35, root, 104),
                                        (1.5, 0.4, if rng.chance(0.5) { third } else { fifth }, 84),
                                        (2.0, 0.9, fifth, 92),
                                    ];
                                    p.push((3.5, 0.4, if last_bar { approach } else { root }, 80));
                                    p
                                }
                                BassStyle::Octaves => (0..8)
                                    .map(|k| {
                                        (
                                            k as f32 * 0.5,
                                            0.42,
                                            root,
                                            if k % 2 == 0 { 100 } else { 86 },
                                        )
                                    })
                                    .collect(),
                                _ => (0..4).map(|k| (k as f32 + 0.5, 0.42, root, 96)).collect(),
                            };
                            for (at, len, pc, base) in pattern {
                                let t = b0 + (at * beat as f32) as u64;
                                if within(t) {
                                    let l = ((len * beat as f32) as u64).min(end - t);
                                    let octave_up = matches!(style, BassStyle::Octaves)
                                        && (at * 2.0) as u32 % 2 == 1;
                                    let before = last;
                                    push(&mut out, &mut last, t, l, pc, v(&mut rng, base));
                                    if octave_up && let Some(n) = out.last_mut() {
                                        n.pitch = (n.pitch + 12).min(hi);
                                        last = before;
                                    }
                                }
                            }
                            b0 += bar;
                        }
                    }
                    BassStyle::Kick => unreachable!(),
                }
            }
        }
    }
    out
}

/// How a melody moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MelodyStyle {
    /// A hook: a motif, said again changed, answered, and resolved; chord tones where it lands.
    #[default]
    Hook,
    /// A one-bar riff, its notes following each chord.
    Riff,
    /// Up and down each chord in sixteenths.
    Arp,
    /// Long notes moving by the chords' thirds and sevenths: a counter-line or a pad's top.
    Counter,
}

/// Rhythms a melody's bar can take: (start, length) in sixteenths.
const MOTIFS: &[&[(u8, u8)]] = &[
    &[(0, 3), (3, 1), (4, 2), (6, 2), (8, 4)],
    &[(0, 2), (2, 2), (4, 3), (7, 1), (8, 6)],
    &[(2, 2), (4, 2), (6, 4), (10, 2), (12, 4)],
    &[(0, 1), (1, 1), (2, 2), (4, 4), (10, 2), (12, 2)],
    &[(0, 6), (6, 2), (8, 2), (10, 6)],
    &[(0, 3), (3, 3), (6, 2), (8, 3), (11, 5)],
    &[(4, 2), (6, 2), (8, 2), (10, 2), (12, 4)],
    &[(0, 2), (3, 2), (6, 2), (8, 8)],
];

/// A melody over the chords. `density` (0 to 1) thins its notes; `register` is its range.
#[allow(clippy::too_many_arguments)]
pub fn melody(
    spans: &[Span],
    key: &Key,
    style: MelodyStyle,
    register: (u8, u8),
    density: f32,
    beat: u64,
    origin: u64,
    bars: u64,
    seed: u64,
) -> Vec<Note> {
    let mut rng = Rng::new(seed);
    let (lo, hi) = register;
    let sixteenth = beat / 4;
    let bar = beat * 4;
    let chord_at = |t: u64| {
        spans
            .iter()
            .find(|s| s.start - origin <= t && t < s.end - origin)
    };
    let scale: Vec<u8> = (0..12u8).filter(|&p| key.contains(p)).collect();
    let mut out = Vec::new();
    let mut last = (lo as i32 + hi as i32) / 2;
    match style {
        MelodyStyle::Arp => {
            let mut t = 0;
            let mut k = 0usize;
            while t < bars * bar {
                if let Some(s) = chord_at(t) {
                    let mut tones: Vec<u8> = s
                        .chord
                        .classes()
                        .iter()
                        .map(|&pc| nearest(pc, (lo as i32 + hi as i32) / 2, lo, hi))
                        .collect();
                    tones.sort_unstable();
                    let up_down: Vec<u8> = tones
                        .iter()
                        .chain(
                            tones
                                .iter()
                                .rev()
                                .skip(1)
                                .take(tones.len().saturating_sub(2)),
                        )
                        .copied()
                        .collect();
                    let p = up_down[k % up_down.len()];
                    if rng.unit() <= density.max(0.3) {
                        out.push(note(
                            t,
                            sixteenth - 10,
                            p,
                            if k.is_multiple_of(4) { 96 } else { 76 },
                        ));
                    }
                }
                t += sixteenth;
                k += 1;
            }
        }
        MelodyStyle::Counter => {
            for s in spans {
                let guide = s.chord.guide_tones();
                let pick = guide
                    .iter()
                    .map(|&pc| nearest(pc, last, lo, hi))
                    .min_by_key(|&p| (p as i32 - last).abs())
                    .unwrap_or_else(|| nearest(s.chord.root, last, lo, hi));
                last = pick as i32;
                out.push(note(s.start - origin, s.end - s.start - 30, pick, 84));
            }
        }
        MelodyStyle::Hook | MelodyStyle::Riff => {
            // The motif: a bar's rhythm, and the steps it takes (in scale steps from where it
            // lands).
            let motif = MOTIFS[rng.below(MOTIFS.len())];
            let steps: Vec<i32> = motif.iter().map(|_| rng.below(5) as i32 - 2).collect();
            let answer = MOTIFS[rng.below(MOTIFS.len())];
            let to_scale = |p: i32| -> i32 {
                // The scale's pitch at or under p.
                (0..12)
                    .map(|d| p - d)
                    .find(|&q| scale.contains(&(q.rem_euclid(12) as u8)))
                    .unwrap_or(p)
            };
            let step_from = |p: i32, n: i32| -> i32 {
                let mut q = to_scale(p);
                let dir = n.signum();
                for _ in 0..n.abs() {
                    q += dir;
                    while !scale.contains(&(q.rem_euclid(12) as u8)) {
                        q += dir;
                    }
                }
                q
            };
            for b in 0..bars {
                let b0 = b * bar;
                // Phrases of four bars: A, A changed, A, an answer that resolves.
                let phrase = b % 4;
                let rhythm = match (style, phrase) {
                    (MelodyStyle::Hook, 3) => answer,
                    _ => motif,
                };
                for (k, &(at, len)) in rhythm.iter().enumerate() {
                    if style == MelodyStyle::Hook
                        && phrase != 3
                        && rng.unit() > density.clamp(0.2, 1.0) + 0.25
                    {
                        continue;
                    }
                    let t = b0 + at as u64 * sixteenth;
                    let Some(s) = chord_at(t) else { continue };
                    let strong = at % 4 == 0;
                    let pitch = if strong || k == 0 {
                        // On the beat: the chord's tone nearest the line.
                        let target = step_from(
                            last,
                            steps[k % steps.len()] + if phrase == 1 { 1 } else { 0 },
                        );
                        s.chord
                            .classes()
                            .iter()
                            .map(|&pc| nearest(pc, target, lo, hi))
                            .min_by_key(|&p| (p as i32 - target).abs())
                            .unwrap_or(target as u8) as i32
                    } else {
                        // Between: a step of the scale.
                        step_from(last, steps[k % steps.len()].signum()).clamp(lo as i32, hi as i32)
                    };
                    // A phrase ends on the chord's root or third, held.
                    let last_note = phrase == 3 && k + 1 == rhythm.len();
                    let pitch = if last_note {
                        let resolve = [
                            s.chord.root,
                            (s.chord.root + if s.chord.quality.minor() { 3 } else { 4 }) % 12,
                        ];
                        resolve
                            .iter()
                            .map(|&pc| nearest(pc, pitch, lo, hi))
                            .min_by_key(|&p| (p as i32 - pitch).abs())
                            .unwrap() as i32
                    } else {
                        pitch
                    };
                    let pitch = pitch.clamp(lo as i32, hi as i32) as u8;
                    last = pitch as i32;
                    let vel = if strong { 98 } else { 82 } + rng.below(7) as u8;
                    out.push(note(t, len as u64 * sixteenth - 20, pitch, vel));
                }
            }
        }
    }
    out
}

/// A drum in a kit, on its General MIDI note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drum {
    Kick,
    Snare,
    Clap,
    Rim,
    HatClosed,
    HatOpen,
    Shaker,
    Perc,
    TomHigh,
    TomMid,
    TomLow,
    Crash,
    Ride,
}

impl Drum {
    pub fn note(self) -> u8 {
        match self {
            Drum::Kick => 36,
            Drum::Snare => 38,
            Drum::Clap => 39,
            Drum::Rim => 37,
            Drum::HatClosed => 42,
            Drum::HatOpen => 46,
            Drum::Shaker => 70,
            Drum::Perc => 56,
            Drum::TomHigh => 48,
            Drum::TomMid => 45,
            Drum::TomLow => 41,
            Drum::Crash => 49,
            Drum::Ride => 51,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Drum::Kick => "kick",
            Drum::Snare => "snare",
            Drum::Clap => "clap",
            Drum::Rim => "rim",
            Drum::HatClosed => "closed hat",
            Drum::HatOpen => "open hat",
            Drum::Shaker => "shaker",
            Drum::Perc => "perc",
            Drum::TomHigh => "high tom",
            Drum::TomMid => "mid tom",
            Drum::TomLow => "low tom",
            Drum::Crash => "crash",
            Drum::Ride => "ride",
        }
    }
}

/// A genre's groove: its steps to a bar, swing, and each drum's bars, one character a step:
/// X accent, x hit, o soft, g ghost, . rest. Rows of several bars alternate.
pub struct Groove {
    pub id: &'static str,
    pub steps: u8,
    pub swing: f32,
    pub rows: &'static [(Drum, &'static [&'static str])],
    /// Hats rolled at the end of every other bar (trap, drill).
    pub rolls: bool,
}

pub const GROOVES: &[Groove] = &[
    Groove {
        id: "boom_bap",
        steps: 16,
        swing: 58.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x......x..x.....", "x.........x..x.."]),
            (Drum::Snare, &["....X.......X...", "....X.......X..g"]),
            (Drum::HatClosed, &["x.o.x.o.x.o.x.o.", "x.o.x.o.x.o.x.oo"]),
            (Drum::HatOpen, &["................", "..............o."]),
        ],
    },
    Groove {
        id: "lofi",
        steps: 16,
        swing: 60.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x......o..x.....", "x.........x....."]),
            (Drum::Snare, &["....x.......x...", "....x.......x..g"]),
            (Drum::HatClosed, &["o.g.o.g.o.g.o.g.", "o.g.o.g.o.g.o.gg"]),
            (Drum::Shaker, &["................", "................"]),
        ],
    },
    Groove {
        id: "trap",
        steps: 16,
        swing: 50.0,
        rolls: true,
        rows: &[
            (Drum::Kick, &["x.....x...x.....", "x.........x..x.."]),
            (Drum::Clap, &["........X.......", "........X......."]),
            (Drum::Snare, &["........x.......", "........x......."]),
            (Drum::HatClosed, &["x.x.x.x.x.x.x.x.", "x.x.x.x.x.x.x.x."]),
            (Drum::HatOpen, &["................", "..............o."]),
        ],
    },
    Groove {
        id: "drill",
        steps: 16,
        swing: 50.0,
        rolls: true,
        rows: &[
            (Drum::Kick, &["x.....x.....x...", "..x.....x......."]),
            (Drum::Snare, &["........X....x..", "........X......."]),
            (Drum::HatClosed, &["x..x..x.x..x..x.", "x..x..x.x..x.x.x"]),
            (Drum::Perc, &["...o......o.....", "...o......o....."]),
        ],
    },
    Groove {
        id: "house",
        steps: 16,
        swing: 54.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["X...X...X...X..."]),
            (Drum::Clap, &["....x.......x..."]),
            (Drum::HatOpen, &["..x...x...x...x."]),
            (Drum::HatClosed, &["g.g.g.g.g.g.g.g."]),
            (Drum::Shaker, &["gogogogogogogogo"]),
        ],
    },
    Groove {
        id: "techno",
        steps: 16,
        swing: 50.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["X...X...X...X..."]),
            (Drum::HatOpen, &["..x...x...x...x."]),
            (Drum::HatClosed, &["o.o.o.o.o.o.o.o."]),
            (Drum::Rim, &["......o.......o.", "......o....o..o."]),
            (Drum::Clap, &["....o.......o..."]),
        ],
    },
    Groove {
        id: "afrobeats",
        steps: 16,
        swing: 55.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x..x...x..x.....", "x..x...x..x...x."]),
            (Drum::Rim, &["...x..x...x..x..", "...x..x...x..x.."]),
            (Drum::Clap, &["....x.......x..."]),
            (Drum::Shaker, &["xoxoxoxoxoxoxoxo"]),
            (Drum::Perc, &["..o.....o..o....", "..o.....o...o.o."]),
        ],
    },
    Groove {
        id: "amapiano",
        steps: 16,
        swing: 54.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x.......x.......", "x.......x......."]),
            (Drum::Clap, &["....x.......x..."]),
            (Drum::Shaker, &["xgxgxgxgxgxgxgxg"]),
            (Drum::HatOpen, &["..o...o...o...o."]),
            (Drum::Perc, &["..x..x....x..x..", "..x..x..x...x..x"]),
        ],
    },
    Groove {
        id: "rnb",
        steps: 16,
        swing: 60.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x......x..x.x...", "x.x.......x....."]),
            (Drum::Snare, &["....x.......x...", "....x..g....x..g"]),
            (Drum::HatClosed, &["x.xgx.xgx.xgx.x.", "x.xgx.xgx.xgx.xx"]),
        ],
    },
    Groove {
        id: "pop",
        steps: 16,
        swing: 50.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x.......x.x.....", "x.......x......."]),
            (Drum::Snare, &["....x.......x..."]),
            (Drum::Clap, &["....o.......o..."]),
            (Drum::HatClosed, &["x.o.x.o.x.o.x.o."]),
        ],
    },
    Groove {
        id: "reggaeton",
        steps: 16,
        swing: 50.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x...x...x...x..."]),
            (Drum::Snare, &["...x..x....x..x."]),
            (Drum::HatClosed, &["x.o.x.o.x.o.x.o."]),
        ],
    },
    Groove {
        id: "dnb",
        steps: 16,
        swing: 50.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x.........x.....", "x.......x.x....."]),
            (Drum::Snare, &["....X.......X...", "....X..g....X..."]),
            (Drum::HatClosed, &["x.x.x.x.x.x.x.x."]),
            (Drum::Ride, &["................", "..............o."]),
        ],
    },
    Groove {
        id: "rock",
        steps: 16,
        swing: 50.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x.......x.x.....", "x.....x.x......."]),
            (Drum::Snare, &["....X.......X..."]),
            (Drum::HatClosed, &["x.x.x.x.x.x.x.x."]),
        ],
    },
    Groove {
        id: "neo_soul",
        steps: 16,
        swing: 62.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x......x...x....", "x.x.......x..x.."]),
            (Drum::Rim, &["....x..g....x...", "....x..g....x.gg"]),
            (Drum::HatClosed, &["x.xgx.xgx.xgx.xg"]),
        ],
    },
];

/// The groove for a genre or style word, else boom bap.
pub fn groove(style: &str) -> &'static Groove {
    let s = style.to_lowercase().replace(['-', ' '], "_");
    let alias = match s.as_str() {
        "hip_hop" | "hiphop" | "boombap" => "boom_bap",
        "lo_fi" | "lofi_hip_hop" | "chill" => "lofi",
        "uk_drill" => "drill",
        "deep_house" | "afro_house" | "garage" => "house",
        "r&b" | "randb" | "soul" => "rnb",
        "gospel" | "jazz" => "neo_soul",
        "drum_and_bass" | "jungle" | "liquid" => "dnb",
        "edm" | "future_bass" => "pop",
        "dancehall" | "latin" => "reggaeton",
        other => other,
    };
    GROOVES
        .iter()
        .find(|g| g.id == alias)
        .unwrap_or(&GROOVES[0])
}

/// A drum's step velocity, from its character.
fn hit(c: char) -> u8 {
    match c {
        'X' => 120,
        'x' => 100,
        'o' => 72,
        'g' => 42,
        _ => 0,
    }
}

/// A groove written out for `bars` bars: each drum's cells (a velocity a step), its rows' bars
/// alternating, small changes by `seed`, a fill every `fill_every` bars (its last bar), and a
/// crash after each fill. Trap and drill hats get rolls, on a 32nd grid. Answers the steps per
/// bar and the rows by note.
pub fn drums(g: &Groove, bars: u64, fill_every: u64, seed: u64) -> (u8, Vec<(u8, Vec<u8>)>) {
    let mut rng = Rng::new(seed);
    // Rolls need 32nds: every 16th step becomes two.
    let fine = if g.rolls { 2usize } else { 1 };
    let per_bar = g.steps as usize * fine;
    let total = per_bar * bars as usize;
    let mut rows: Vec<(Drum, Vec<u8>)> = Vec::new();
    for (drum, patterns) in g.rows {
        let mut cells = vec![0u8; total];
        for b in 0..bars as usize {
            let pattern: Vec<char> = patterns[b % patterns.len()].chars().collect();
            for (i, &c) in pattern.iter().enumerate().take(g.steps as usize) {
                let mut v = hit(c);
                if v > 0 {
                    // Humans don't hit twice the same.
                    v = (v as i32 + rng.below(9) as i32 - 4).clamp(1, 127) as u8;
                }
                cells[b * per_bar + i * fine] = v;
            }
        }
        rows.push((*drum, cells));
    }
    let row = |rows: &mut Vec<(Drum, Vec<u8>)>, d: Drum| -> usize {
        match rows.iter().position(|(x, _)| *x == d) {
            Some(i) => i,
            None => {
                rows.push((d, vec![0; total]));
                rows.len() - 1
            }
        }
    };
    // Rolls: the last beat of every other bar's hats in 32nds, rising.
    if g.rolls {
        let h = row(&mut rows, Drum::HatClosed);
        for b in (1..bars as usize).step_by(2) {
            let start = b * per_bar + per_bar - 8;
            let kind = rng.below(3);
            for k in 0..8 {
                let on = match kind {
                    0 => true,
                    1 => k % 2 == 0 || k >= 4,
                    _ => k >= 2,
                };
                rows[h].1[start + k] = if on { 60 + (k as u8) * 6 } else { 0 };
            }
        }
    }
    // Fills: the last bar of each run of `fill_every`, its back half a snare (or tom) run.
    if fill_every > 0 {
        let s = row(
            &mut rows,
            if g.rows.iter().any(|r| r.0 == Drum::Snare) {
                Drum::Snare
            } else {
                Drum::Clap
            },
        );
        let toms = [Drum::TomHigh, Drum::TomMid, Drum::TomLow];
        let use_toms = rng.chance(0.5);
        let mut b = fill_every as usize;
        while b <= bars as usize {
            let bar0 = (b - 1) * per_bar;
            let from = bar0 + per_bar / 2;
            // Clear what played there, then the run.
            for (_, cells) in rows.iter_mut() {
                for c in cells[from..bar0 + per_bar].iter_mut() {
                    *c = (*c).min(30);
                }
            }
            let n = (per_bar / 2) / fine;
            for k in 0..n {
                let at = from + k * fine;
                let vel = 70 + (k * 50 / n.max(1)) as u8;
                if use_toms && k >= n / 2 {
                    let t = toms[(k - n / 2) * 3 / (n - n / 2).max(1)];
                    let r = row(&mut rows, t);
                    rows[r].1[at] = vel;
                } else {
                    rows[s].1[at] = vel;
                }
            }
            // A crash on the bar after.
            let after = b * per_bar;
            if after < total {
                let c = row(&mut rows, Drum::Crash);
                rows[c].1[after] = 110;
                let k = row(&mut rows, Drum::Kick);
                rows[k].1[after] = rows[k].1[after].max(110);
            }
            b += fill_every as usize;
        }
    }
    (
        per_bar as u8,
        rows.into_iter()
            .map(|(d, cells)| (d.note(), cells))
            .collect(),
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn key(s: &str) -> Key {
        Key::parse(s).unwrap()
    }

    #[test]
    fn chords_read_and_write_both_ways() {
        let c = Chord::parse("Fmaj9").unwrap();
        assert_eq!((c.root, c.quality), (5, Quality::Maj9));
        assert_eq!(c.classes(), [5, 9, 0, 4, 7]);
        for s in ["Am9", "E7b9", "Bbm7", "F#m7b5", "Dsus4", "C/E", "Gm", "A13"] {
            let c = Chord::parse(s).unwrap();
            assert_eq!(
                Chord::parse(&c.name_in(Some(&key("C minor")))).unwrap(),
                c,
                "{s}"
            );
        }
        assert_eq!(
            Chord::parse("Bb").unwrap().name_in(Some(&key("F major"))),
            "Bb"
        );
        assert!(Chord::parse("H7").is_none());
        // Numerals: in A minor the sixth is F, the seventh G; in C major V7b9 is G7b9.
        let am = key("A minor");
        assert_eq!(numeral("VImaj9", &am).unwrap().name(), "Fmaj9");
        assert_eq!(numeral("V7b9", &am).unwrap().name(), "E7b9");
        assert_eq!(numeral("vii9", &am).unwrap().name(), "Gm9");
        assert_eq!(numeral("bII", &am).unwrap().name(), "Bb");
        assert_eq!(numeral("ii9", &key("C major")).unwrap().name(), "Dm9");
        assert_eq!(numeral("#iv°7", &key("C major")).unwrap().name(), "F#dim7");
    }

    #[test]
    fn progressions_suit_the_words_and_lay_out_by_bar() {
        let am = key("A minor");
        let p = suggest(&am, "lofi", 3);
        assert!(p.minor && p.suits.contains(&"lofi"));
        assert!(
            suggest(&key("C major"), "afrobeats", 1)
                .suits
                .contains(&"afrobeats")
        );
        let spans = lay_out("VImaj9 | V7b9 | i9 | vii9 III7b9", &am, 0, 8, 3840).unwrap();
        let names: Vec<String> = spans.iter().map(|s| s.chord.name()).collect();
        assert_eq!(names[..5], ["Fmaj9", "E7b9", "Am9", "Gm9", "C7b9"]);
        assert_eq!(
            (spans[3].start, spans[3].end, spans[4].start),
            (11520, 13440, 13440)
        );
        assert_eq!(spans.len(), 10);
        // A chord held over the bar line is one.
        assert_eq!(lay_out("Am | Am | F", &am, 0, 3, 3840).unwrap().len(), 2);
        assert!(lay_out("Am | Xq", &am, 0, 2, 3840).is_err());
    }

    #[test]
    fn voicings_move_little_from_chord_to_chord() {
        let am = key("A minor");
        let spans = lay_out("i9 | iv9 | VII9 | IIImaj9", &am, 0, 4, 3840).unwrap();
        let mut prev: Option<Vec<u8>> = None;
        for s in &spans {
            let v = voice(&s.chord, prev.as_deref(), Voicing::Rootless, (55, 79));
            assert!(v.iter().all(|&p| (55..=79).contains(&p)), "{v:?}");
            assert!(v.iter().all(|&p| s.chord.has(p)));
            if let Some(p) = &prev {
                // Each voice steps a few semitones, not leaps.
                let moved: u32 = v
                    .iter()
                    .zip(p)
                    .map(|(a, b)| (*a as i32 - *b as i32).unsigned_abs())
                    .sum();
                assert!(moved <= 4 * 5, "{p:?} to {v:?}");
            }
            prev = Some(v);
        }
        // Open: the root an octave or so under the rest.
        let v = voice(
            &Chord::parse("Fmaj9").unwrap(),
            None,
            Voicing::Open,
            (58, 79),
        );
        assert_eq!(v[0] % 12, 5);
        assert!(v[1] >= v[0] + 3);
    }

    #[test]
    fn basslines_follow_the_chords() {
        let am = key("A minor");
        let spans = lay_out("Fmaj9 | E7b9 | Am9 | Gm9 C7b9", &am, 0, 4, 3840).unwrap();
        for style in [
            BassStyle::Roots,
            BassStyle::RootFifth,
            BassStyle::Walking,
            BassStyle::Syncopated,
            BassStyle::Octaves,
            BassStyle::Offbeat,
        ] {
            let notes = bassline(&spans, style, (28, 50), 960, 0, &[], &am, 7);
            assert!(!notes.is_empty(), "{style:?}");
            assert!(
                notes.iter().all(|n| (28..=50).contains(&n.pitch)),
                "{style:?}"
            );
            // On each chord's downbeat, its root.
            for s in &spans {
                if let Some(n) = notes.iter().find(|n| n.start as u64 == s.start)
                    && style != BassStyle::Offbeat
                {
                    assert_eq!(n.pitch % 12, s.chord.root, "{style:?} at {}", s.start);
                }
            }
        }
        // On the kick: a note at each kick, the chord's root.
        let kicks = [(0, 110), (1440, 90), (3840, 110), (7680, 110)];
        let notes = bassline(&spans, BassStyle::Kick, (24, 43), 960, 0, &kicks, &am, 1);
        assert_eq!(
            notes.iter().map(|n| n.start).collect::<Vec<_>>(),
            [0, 1440, 3840, 7680]
        );
        assert_eq!(notes[2].pitch % 12, 4);
    }

    #[test]
    fn melodies_land_on_chord_tones_and_stay_in_range() {
        let am = key("A minor");
        let spans = lay_out("i9 | iv9 | VII9 | IIImaj9", &am, 0, 8, 3840).unwrap();
        for style in [
            MelodyStyle::Hook,
            MelodyStyle::Riff,
            MelodyStyle::Arp,
            MelodyStyle::Counter,
        ] {
            for seed in 0..5 {
                let notes = melody(&spans, &am, style, (62, 86), 0.7, 960, 0, 8, seed);
                assert!(!notes.is_empty(), "{style:?}");
                assert!(notes.iter().all(|n| (62..=86).contains(&n.pitch)));
                // Every note on a beat is a tone of the chord there.
                for n in notes.iter().filter(|n| n.start % 960 == 0) {
                    let s = spans
                        .iter()
                        .find(|s| s.start as i64 <= n.start && n.start < s.end as i64)
                        .unwrap();
                    assert!(
                        s.chord.has(n.pitch),
                        "{style:?} seed {seed}: {} on {}",
                        n.pitch,
                        s.chord.name()
                    );
                }
                // And everything else is in the key, or a chord tone.
                for n in &notes {
                    let s = spans
                        .iter()
                        .find(|s| s.start as i64 <= n.start && n.start < s.end as i64)
                        .unwrap();
                    assert!(
                        am.contains(n.pitch) || s.chord.has(n.pitch),
                        "{style:?}: {}",
                        n.pitch
                    );
                }
            }
        }
    }

    #[test]
    fn grooves_have_their_feel_fills_and_rolls() {
        let (steps, rows) = drums(groove("boom bap"), 4, 4, 1);
        assert_eq!(steps, 16);
        let kick = &rows.iter().find(|r| r.0 == 36).unwrap().1;
        assert!(kick[0] > 90 && kick.len() == 64);
        // The fill: the fourth bar's back half rises (on snare or toms), and a crash would follow.
        let at = |i: usize| rows.iter().map(|r| r.1[i]).max().unwrap();
        assert!(at(62) > at(56) && at(56) >= 70);
        // Trap: on a 32nd grid, its hats rolled at the end of bar two.
        let (steps, rows) = drums(groove("trap"), 2, 0, 2);
        assert_eq!(steps, 32);
        let hats = &rows.iter().find(|r| r.0 == 42).unwrap().1;
        assert!(hats[56..64].iter().filter(|&&v| v > 0).count() >= 4);
        assert_eq!(groove("deep house").id, "house");
        assert_eq!(groove("unknown").id, "boom_bap");
    }

    #[test]
    fn notes_are_named_as_chords() {
        let w = |pcs: &[u8]| pcs.iter().map(|&p| (p, 1.0)).collect::<Vec<_>>();
        assert_eq!(
            name_notes(&w(&[9, 0, 4, 7]), Some(9)).unwrap().name(),
            "Am7"
        );
        assert_eq!(
            name_notes(&w(&[5, 9, 0, 4]), Some(5)).unwrap().name(),
            "Fmaj7"
        );
        assert_eq!(
            name_notes(&w(&[7, 11, 2, 5]), Some(7)).unwrap().name(),
            "G7"
        );
    }
}
