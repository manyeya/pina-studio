//! Progressions by genre and mood, and those suggested for a song.

use super::*;

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
