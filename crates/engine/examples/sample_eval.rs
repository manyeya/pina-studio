//! Measures the sample analysis against what packs' file names say: category (with names, and
//! from the audio alone), loop or one-shot, pitch, tempo, key, how alike the embedding's
//! neighbours are, duplicates by fingerprint, and speed.
//!
//!     cargo run --release -p tempo-engine --example sample_eval -- [options] [pack folders]
//!
//! It always measures MusicRadar's free "90s Hip Hop" pack, where tempo keeps it
//! (`~/Music/tempo/Samples/MusicRadar 90s Hip Hop`); folders of the other free SampleRadar
//! packs `truth` knows the names of can be given too, each measured on its own.
//!
//! - `--list`: every file's result too.
//! - `--scan`: times a whole-library scan of the 90s pack on every core.
//! - `--time`: times analysis file by file on one core.
//! - `--tempo`, `--keys`: tempo, or key, alone, for files whose names give one.
//! - `--fit <model.rs>`: fits the classifiers, the embedding and the traits' statistics on the
//!   labelled packs given (the 90s pack is held out, only measured) and writes them to
//!   `model.rs`. `LOPO=1` also scores each pack by a model fitted on the others; `CONFUSION=1`
//!   and `KNN=1` print what the held-out pack's sounds are taken for, and their neighbours.
//! - `--cache <file>`: keeps every file's measures there, to fit again without analysing again.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

use tempo_engine::samples::{
    Analysis, Category, Kind, NOTES,
    analyze::{analyze, trait_inputs},
    classify, features,
    library::{Library, audio_files},
    note_name,
};

/// What a file's name and place say it is.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct Truth {
    pack: String,
    kind: Option<Kind>,
    /// The categories it may fairly be called, the first the likeliest; none when unsure.
    cats: Vec<Category>,
    bpm: Option<f32>,
    /// A MIDI note, or only a pitch class.
    note: Option<i32>,
    class: Option<u8>,
    /// A key's (or chord's) root, and whether it's minor when known.
    root: Option<u8>,
    minor: Option<bool>,
    /// The 90s Hip Hop pack: measured, never fitted on.
    held_out: bool,
}

/// A pitch class from a note name's start: "C", "C#", "Db", "a#".
fn pitch_class(s: &str) -> Option<(u8, usize)> {
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
    match chars.next() {
        Some('#') => Some(((base + 1) % 12, 2)),
        Some('b') => Some(((base + 11) % 12, 2)),
        _ => Some((base, 1)),
    }
}

/// A chord name's root and whether it's minor: "Am9", "Cmaj13", "Emin7th", "F#".
fn chord(s: &str) -> Option<(u8, Option<bool>)> {
    let (root, used) = pitch_class(s)?;
    let rest = &s[used..];
    let minor = if rest.starts_with("min") || (rest.starts_with('m') && !rest.starts_with("maj")) {
        Some(true)
    } else if rest.is_empty()
        || rest.starts_with("maj")
        || rest.starts_with(|c: char| c.is_ascii_digit())
    {
        Some(false)
    } else {
        None
    };
    Some((root, minor))
}

/// A note with its octave: "C#1" (C1 is MIDI 24).
fn note(s: &str) -> Option<i32> {
    let (class, used) = pitch_class(s)?;
    let octave: i32 = s[used..].parse().ok()?;
    Some(12 * (octave + 1) + class as i32)
}

/// Categories from words in a name, for the training packs' plainly named hits.
fn word_cats(name: &str) -> Vec<Category> {
    use Category::*;
    let n = name.to_lowercase().replace(['_', '-'], " ");
    let has = |w: &str| {
        n.split(|c: char| !c.is_alphanumeric())
            .map(|x| x.trim_end_matches(|c: char| c.is_ascii_digit()))
            .any(|x| x == w || x == format!("{w}s") || (w.len() >= 4 && x.starts_with(w)))
    };
    let table: &[(&[&str], &[Category])] = &[
        (
            &["closed hat", "closedhat", "closed hi hat", "closed hihat"],
            &[HatClosed],
        ),
        (
            &["open hat", "openhat", "open hi hat", "open hihat"],
            &[HatOpen],
        ),
        (&["rimshot", "rim"], &[Rim]),
        (&["kick", "bass drum"], &[Kick]),
        (&["snare"], &[Snare]),
        (&["clap"], &[Clap]),
        (&["snap"], &[Snap]),
        (&["ride"], &[Ride]),
        (&["crash"], &[Crash]),
        (&["cymbal"], &[Crash, Ride]),
        (&["tom", "timbale"], &[Tom]),
        (&["shaker", "maraca", "marcas", "rachet"], &[Shaker]),
        (&["tamb", "tambourine"], &[Tambourine]),
        (&["hat", "hihat", "hi hat"], &[HatClosed, HatOpen]),
        (
            &[
                "conga",
                "bongo",
                "cowbell",
                "clave",
                "perc",
                "cajon",
                "djmbe",
                "darbuka",
                "dumbek",
                "tabla",
                "udu",
                "frame drum",
                "hide drum",
                "talking drum",
                "metalic",
                "tin",
                "block",
                "bell",
            ],
            &[Perc],
        ),
    ];
    for (words, cats) in table {
        if words.iter().any(|w| {
            if w.contains(' ') {
                n.contains(w)
            } else {
                has(w)
            }
        }) {
            return cats.to_vec();
        }
    }
    Vec::new()
}

fn bpm_in(s: &str) -> Option<f32> {
    let lower = s.to_lowercase();
    if let Some(i) = lower.find("bpm") {
        let digits: String = lower[..i]
            .trim_end_matches([' ', '_'])
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return digits.parse().ok();
    }
    None
}

/// What a file is, from its name and folders, pack by pack.
fn truth(path: &Path) -> Truth {
    use Category::*;
    let p = path.to_string_lossy().to_string();
    let name = path.file_stem().unwrap().to_string_lossy().to_string();
    let parent = path
        .parent()
        .and_then(|d| d.file_name())
        .map(|d| d.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut t = Truth::default();
    let set = |t: &mut Truth, kind: Option<Kind>, cats: &[Category]| {
        t.kind = kind;
        t.cats = cats.to_vec();
    };
    let starts = |w: &str| name.to_lowercase().starts_with(&w.to_lowercase());
    let has = |w: &str| name.to_lowercase().contains(&w.to_lowercase());
    if p.contains("90s Hip Hop") {
        t.pack = "90s".into();
        t.held_out = true;
        t.bpm = bpm_in(&p.rsplit('/').take(2).collect::<Vec<_>>().join("/"));
        if p.contains("/Drum_Hits/") {
            let n = name.to_lowercase();
            let cats: &[Category] = if n.contains("kick") {
                &[Kick]
            } else if n.contains("snare") {
                &[Snare]
            } else if n.contains("clap") {
                &[Clap]
            } else if n.contains("hat") {
                &[HatClosed, HatOpen]
            } else if n.contains("cymbal") {
                &[Crash, Ride]
            } else if n.contains("rim") {
                &[Rim]
            } else if n.contains("perc") {
                &[Perc]
            } else if n.contains("shaker") || n.contains("shalker") {
                &[Shaker]
            } else if n.contains("tamb") {
                &[Tambourine]
            } else if n.contains("sub") {
                &[Bass808]
            } else if n.starts_with("rev") {
                &[Reverse]
            } else if n.contains("huh") {
                &[Vocal]
            } else {
                &[]
            };
            set(&mut t, Some(Kind::OneShot), cats);
            t.bpm = None;
        } else if p.contains("/Hits/") {
            let cats: &[Category] = if starts("Bass") {
                &[Bass]
            } else if starts("Keys") {
                &[Keys]
            } else if starts("Stab") {
                &[Chord]
            } else if starts("Strings") {
                &[Strings]
            } else if starts("Guitar") {
                &[Guitar]
            } else if starts("Synth") {
                &[Synth]
            } else if starts("Kick") {
                &[Kick]
            } else {
                &[]
            };
            set(&mut t, Some(Kind::OneShot), cats);
        } else if p.contains("/Multis/") {
            set(&mut t, Some(Kind::OneShot), &[Bass]);
            t.note = name.rsplit('_').next().and_then(note);
        } else if p.contains("/Loops/") {
            let cats: &[Category] = if starts("Bass") {
                &[Bass]
            } else if starts("Guitar") {
                &[Guitar]
            } else if starts("Horn") {
                &[Brass]
            } else if starts("Keys") {
                &[Keys]
            } else if starts("String") {
                &[Strings]
            } else {
                &[]
            };
            set(&mut t, Some(Kind::Loop), cats);
            if name.starts_with("Keys_01") {
                t.root = Some(0);
            }
        } else if p.contains("/Bonus_Beats/") {
            set(&mut t, Some(Kind::Loop), &[Drums]);
        } else if p.contains("/Beats/") {
            t.bpm = bpm_in(&parent);
            let cats: &[Category] = if has("Conga") || has("Perc") {
                &[Perc]
            } else if has("Shaker") {
                &[Shaker]
            } else if has("Huh") {
                &[Vocal]
            } else if has("Drums_Sub") {
                &[]
            } else if has("Drums") || has("FullMix") {
                &[Drums]
            } else if has("Bass") {
                &[Bass]
            } else if has("Guitar") {
                &[Guitar]
            } else if has("Org") || has("EPO") || has("_EP") || has("Piano") || has("Keys") {
                &[Keys]
            } else if has("Atmos") {
                &[Texture, Pad]
            } else if has("Flute") {
                &[Woodwind]
            } else if has("Horn") {
                &[Brass]
            } else {
                &[]
            };
            let kind = if has("BassHit") || has("Gong") {
                None
            } else {
                Some(Kind::Loop)
            };
            set(&mut t, kind, cats);
        } else if p.contains("/Special_Sauce/") {
            let cats: &[Category] = if starts("Crackle") || name == "Noise" || name == "Old_Wax" {
                &[Texture]
            } else if starts("Rev") {
                &[Reverse]
            } else {
                &[]
            };
            set(&mut t, None, cats);
        }
        return t;
    }
    if p.contains("musicradar-808-samples") {
        t.pack = "808 drums".into();
        if p.contains("/Hits/") {
            let code = parent
                .rsplit('[')
                .next()
                .unwrap_or("")
                .trim_end_matches(']')
                .to_string();
            let cats: &[Category] = match code.as_str() {
                "BD" => &[Kick],
                "CP" => &[Clap],
                "CL" | "HC-MC-LC" | "CB" => &[Perc],
                "CH" => &[HatClosed],
                "OH" => &[HatOpen],
                "CY" => &[Crash, Ride],
                "MA" => &[Shaker],
                "RS" => &[Rim],
                "SD" => &[Snare],
                "LT-MT-HT" => &[Tom],
                _ => &[],
            };
            set(&mut t, Some(Kind::OneShot), cats);
        } else {
            t.bpm = bpm_in(&parent);
            let cats: &[Category] = if has("_BD_") {
                &[Kick]
            } else if has("_Hats_") {
                &[HatClosed, HatOpen]
            } else if has("_SD_") {
                &[Snare]
            } else if has("_Toms_") {
                &[Tom]
            } else if has("_Perc_") {
                &[Perc]
            } else {
                &[]
            };
            set(&mut t, Some(Kind::Loop), cats);
        }
    } else if p.contains("808-weight") {
        t.pack = "808 weight".into();
        if has("OneShot") {
            set(&mut t, Some(Kind::OneShot), &[Bass808]);
        }
    } else if p.contains("essential-drumkit") {
        t.pack = "drum kits".into();
        let cats = word_cats(&name);
        set(&mut t, Some(Kind::OneShot), &cats);
    } else if p.contains("hardware-drum-machine") || p.contains("Drum Hardware") {
        t.pack = "drum machines".into();
        if p.contains("/Hits/") {
            let cats = word_cats(&name);
            set(&mut t, Some(Kind::OneShot), &cats);
        } else {
            t.bpm = bpm_in(&name);
            set(&mut t, Some(Kind::Loop), &[Drums]);
        }
    } else if p.contains("musicradar-trap-samples") {
        t.pack = "trap".into();
        t.bpm = bpm_in(&name);
        if p.contains("/Drum_Hits/") {
            let cats = if has("Snare_Clap") {
                vec![Snare, Clap]
            } else if has("Snare_Fx") {
                vec![]
            } else {
                word_cats(&name)
            };
            set(&mut t, Some(Kind::OneShot), &cats);
        } else if p.contains("/Stabs/") {
            set(
                &mut t,
                Some(Kind::OneShot),
                if has("Vocal") {
                    &[Vocal]
                } else {
                    &[Chord, Synth]
                },
            );
        } else if p.contains("/FX/") {
            set(&mut t, None, &[Fx]);
        } else if p.contains("/Beats/") {
            set(&mut t, Some(Kind::Loop), &[Drums]);
        } else if p.contains("/Loops/") {
            set(
                &mut t,
                Some(Kind::Loop),
                if starts("Bass") {
                    &[Bass808, Bass]
                } else {
                    &[Synth]
                },
            );
        }
    } else if p.contains("World Percussion") {
        t.pack = "world perc".into();
        t.bpm = bpm_in(&parent);
        let cats: &[Category] = if has("Maraca") || has("Gourd") {
            &[Shaker, Perc]
        } else if has("TempleBell") {
            &[Bell, Perc]
        } else if has("HandDrum") || has("Guiro") || has("StringDrum") {
            &[Perc]
        } else {
            &[]
        };
        set(&mut t, Some(Kind::Loop), cats);
    } else if p.contains("musicradar-lofi-samples") {
        t.pack = "lo-fi".into();
        let kit_bpm = p.split('/').find_map(|part| {
            part.strip_suffix(')')
                .and_then(|s| s.rsplit('(').next())
                .and_then(|s| s.parse::<f32>().ok())
        });
        if p.contains("Lo-Fi DrumLoops") {
            t.bpm = name
                .rsplit('(')
                .next()
                .and_then(|s| s.trim_end_matches(')').parse().ok());
            set(&mut t, Some(Kind::Loop), &[Drums]);
        } else if p.contains("Multi") {
            t.class = pitch_class(&name)
                .filter(|(_, used)| *used == name.len())
                .map(|(c, _)| c);
            let cats: &[Category] = if p.contains("Bass") {
                &[Bass]
            } else if p.contains("IceChime") {
                &[Bell]
            } else if p.contains("FXPad") {
                &[Pad]
            } else if p.contains("Lead") || p.contains("Beeper") {
                &[Lead, Pluck, Synth]
            } else {
                &[Synth, Bass, Lead]
            };
            set(&mut t, Some(Kind::OneShot), cats);
        } else if p.contains("/DrumHits/") {
            let cats = word_cats(&name);
            set(&mut t, Some(Kind::OneShot), &cats);
        } else if p.contains("ConstructionKits") {
            t.bpm = kit_bpm;
            let cats: &[Category] = if name == "DrumLoop" {
                &[Drums]
            } else if name == "Lead" {
                &[Lead, Synth]
            } else if starts("Bass") {
                &[Bass]
            } else {
                &[]
            };
            set(&mut t, Some(Kind::Loop), cats);
        } else if p.contains("SoundFX") {
            set(&mut t, None, &[Fx]);
        }
    } else if p.contains("Mammoth Synth Chords") {
        t.pack = "synth chords".into();
        let tail = name.rsplit('-').next().unwrap_or("");
        if p.contains("Chord Hits") {
            if let Some((root, minor)) = chord(tail) {
                t.root = Some(root);
                t.minor = minor;
            }
            set(&mut t, Some(Kind::OneShot), &[Chord, Synth, Pad]);
        } else {
            t.bpm = bpm_in(&parent);
            t.root = pitch_class(tail)
                .filter(|(_, u)| *u == tail.len())
                .map(|(c, _)| c);
            set(&mut t, Some(Kind::Loop), &[Synth, Chord, Pad]);
        }
    } else if p.contains("jazz-keyboard") {
        t.pack = "jazz keys".into();
        let tail = name.rsplit('-').next().unwrap_or("");
        if p.contains("Chord Hits") {
            if let Some((root, minor)) = chord(tail) {
                t.root = Some(root);
                t.minor = minor;
            }
            set(&mut t, Some(Kind::OneShot), &[Keys, Chord]);
        } else {
            t.bpm = bpm_in(&parent);
            t.root = pitch_class(tail)
                .filter(|(_, u)| *u == tail.len())
                .map(|(c, _)| c);
            set(&mut t, Some(Kind::Loop), &[Keys]);
        }
    } else if p.contains("Glitch Vocals") {
        t.pack = "vocals".into();
        t.bpm = bpm_in(&parent);
        if p.contains("VocPad") || p.contains("Vocal Pad") {
            let tail = name.rsplit('-').next().unwrap_or("");
            if let Some((root, minor)) = chord(tail) {
                t.root = Some(root);
                t.minor = minor;
            }
        }
        set(&mut t, Some(Kind::Loop), &[Vocal]);
    } else if p.contains("Analogue_Drums") {
        t.pack = "analogue drums".into();
        if p.contains("/Hits/") && !p.contains("Volca Kick") {
            let cats = word_cats(&name);
            set(&mut t, Some(Kind::OneShot), &cats);
        } else if p.contains("/Loops/") {
            t.bpm = bpm_in(&name);
            let cats: &[Category] = if p.contains("Volca Kick") {
                &[Kick]
            } else {
                &[Drums]
            };
            set(&mut t, Some(Kind::Loop), cats);
        }
    } else if p.contains("Synth Impacts") {
        t.pack = "impacts".into();
        set(&mut t, Some(Kind::OneShot), &[Fx]);
    } else if p.contains("Hip-Hop Essentials") {
        t.pack = "hh essentials".into();
        t.bpm = bpm_in(&name);
        if p.contains("/Beats/") {
            set(
                &mut t,
                Some(Kind::Loop),
                if starts("Beatbox") {
                    &[Vocal, Drums]
                } else {
                    &[Drums]
                },
            );
        } else if p.contains("/Drum Hits/") {
            let cats: Vec<Category> = match parent.as_str() {
                "Claps" => vec![Clap],
                "Cymbals" => vec![Crash, Ride],
                "Hats" => vec![HatClosed, HatOpen],
                "Kcks" => vec![Kick],
                "Percussion" => vec![Perc],
                "Snares" => vec![Snare],
                "Oberheim DX" => word_cats(&name),
                _ => vec![],
            };
            set(&mut t, Some(Kind::OneShot), &cats);
        } else if p.contains("/Loops/") {
            let cats: &[Category] = if starts("Bass") {
                &[Bass]
            } else if starts("Piano") {
                &[Keys]
            } else if starts("Synth") {
                &[Synth]
            } else {
                &[]
            };
            set(&mut t, Some(Kind::Loop), cats);
        } else if p.contains("/Multis/") {
            set(&mut t, Some(Kind::OneShot), &[Bass]);
            t.class = name
                .rsplit(' ')
                .next()
                .and_then(pitch_class)
                .map(|(c, _)| c);
        } else if p.contains("/Single Shots/") {
            let cats: &[Category] = if starts("Bass") {
                &[Bass]
            } else if starts("Stab") {
                &[Chord]
            } else if starts("Sub") {
                &[Bass808]
            } else {
                &[]
            };
            set(&mut t, Some(Kind::OneShot), cats);
        }
    } else if p.contains("musicradar-hiphop-samples") {
        t.pack = "hip hop".into();
        t.bpm = p.split('/').find_map(bpm_in);
        // "BasslineK02 100C-01": the key's root after the tempo.
        t.root = name
            .split(' ')
            .nth(1)
            .map(|w| w.trim_start_matches(|c: char| c.is_ascii_digit()))
            .and_then(|w| w.split('-').next())
            .and_then(|w| {
                pitch_class(w)
                    .filter(|(_, u)| *u == w.len())
                    .map(|(c, _)| c)
            });
        let cats: &[Category] = match parent.as_str() {
            "Bass" => &[Bass],
            "Beats" => {
                t.root = None;
                &[Drums]
            }
            "Guitar" => &[Guitar],
            "Keys" => &[Keys],
            "Synth" => &[Synth],
            _ => &[],
        };
        set(&mut t, Some(Kind::Loop), cats);
    } else if p.contains("musicradar-soul-funk-samples") {
        t.pack = "soul funk".into();
        t.bpm = p.split('/').find_map(bpm_in);
        // "SF_MRonBass90A-01", "SF_Organ120P1A-01": the key's root after the tempo.
        let stem = name.split('-').next().unwrap_or("");
        let last = stem.chars().last().unwrap_or(' ');
        let p_lower = parent.to_lowercase();
        let cats: &[Category] = if p_lower.starts_with("bass") {
            &[Bass]
        } else if p_lower.starts_with("beats") {
            &[Drums]
        } else if p_lower.starts_with("guitar") {
            &[Guitar]
        } else if p_lower.contains("rhodes") || p_lower.contains("organ") {
            &[Keys]
        } else {
            &[]
        };
        if !p_lower.starts_with("beats") {
            t.root = pitch_class(&last.to_string()).map(|(c, _)| c);
        }
        set(&mut t, Some(Kind::Loop), cats);
    } else if p.contains("musicradar-bass-guitar-samples") {
        t.pack = "bass guitar".into();
        let kind = if parent.contains("tones") {
            Some(Kind::OneShot)
        } else {
            None
        };
        set(&mut t, kind, &[Bass]);
        if let Some(key) = parent
            .strip_prefix("bass080")
            .or_else(|| parent.strip_prefix("bass110"))
            && let Some((root, minor)) = chord(key)
        {
            t.root = Some(root);
            t.minor = minor.filter(|&m| m);
        }
    } else if p.contains("MIDI Orchestra") {
        t.pack = "orchestra".into();
        let instrument: &[Category] =
            if has("Brass") || has("Horn") || has("Trumpet") || has("Trombone") {
                &[Brass]
            } else if has("String") || has("Stings") {
                &[Strings]
            } else if has("Flute") || has("Wind") {
                &[Woodwind]
            } else {
                &[]
            };
        if p.contains("/CEG_Multies/") {
            set(&mut t, Some(Kind::OneShot), instrument);
            t.class = name
                .rsplit('_')
                .next()
                .and_then(pitch_class)
                .map(|(c, _)| c);
        } else if p.contains("/Chords/") {
            let cats: &[Category] = if parent.contains("Horns") {
                &[Brass, Chord]
            } else if parent.contains("Strings") {
                &[Strings, Chord]
            } else {
                &[Strings, Brass, Chord]
            };
            set(&mut t, Some(Kind::OneShot), cats);
            let tail = name.rsplit('_').next().unwrap_or("");
            if let Some((root, _)) = pitch_class(tail) {
                t.root = Some(root);
                t.minor = Some(tail.ends_with('m'));
            }
        } else {
            t.bpm = bpm_in(&name);
            set(&mut t, Some(Kind::Loop), instrument);
        }
    } else if p.contains("noise-hiss-crackle") {
        t.pack = "noise".into();
        if [
            "Vinyl_Noise",
            "Tape_Noise",
            "White_Noise_Raw",
            "Filtered_Noise",
            "Processed_Noise",
        ]
        .contains(&parent.as_str())
        {
            set(&mut t, None, &[Texture]);
        }
    }
    t
}

/// One file's results.
#[derive(serde::Serialize, serde::Deserialize)]
struct Row {
    path: PathBuf,
    truth: Truth,
    named: Analysis,
    blind: Analysis,
    millis: f32,
    feats: Option<(Vec<f32>, [f32; 10])>,
}

/// Analyses every file on every core, with its name and without.
fn run(files: &[PathBuf], fit: bool) -> Vec<Row> {
    let next = AtomicUsize::new(0);
    let rows = Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = files.get(i) else { break };
                    let start = Instant::now();
                    let Ok((channels, rate)) = tempo_engine::engine::read_audio(path) else {
                        eprintln!("can't read {}", path.display());
                        continue;
                    };
                    let hint = path.to_string_lossy().to_lowercase();
                    let named = analyze(&channels, rate, &hint);
                    let millis = start.elapsed().as_secs_f32() * 1000.0;
                    let blind = analyze(&channels, rate, "");
                    let feats = fit.then(|| {
                        let f = features::extract(&channels, rate, None);
                        let loud = if blind.kind == Kind::OneShot {
                            f.loudness_short
                        } else {
                            f.loudness_long
                        };
                        (classify::vector(&f), trait_inputs(&f, loud))
                    });
                    let t = truth(path);
                    rows.lock().unwrap().push(Row {
                        path: path.clone(),
                        truth: t,
                        named,
                        blind,
                        millis,
                        feats,
                    });
                }
            });
        }
    });
    let mut rows = rows.into_inner().unwrap();
    rows.sort_by(|a, b| a.path.cmp(&b.path));
    rows
}

fn pct(n: usize, d: usize) -> String {
    if d == 0 {
        "-".into()
    } else {
        format!("{:.1}% ({n}/{d})", 100.0 * n as f32 / d as f32)
    }
}

/// Category accuracy, overall and per category, and what each was taken for.
fn categories(rows: &[&Row], blind: bool, title: &str) {
    let mut confusion: HashMap<Category, HashMap<Category, usize>> = HashMap::new();
    let (mut right, mut total, mut drum_right, mut drum_total) = (0, 0, 0, 0);
    for r in rows {
        let Some(&first) = r.truth.cats.first() else {
            continue;
        };
        let got = if blind {
            r.blind.category
        } else {
            r.named.category
        };
        let ok = r.truth.cats.contains(&got);
        total += 1;
        right += ok as usize;
        if first.is_drum() && first != Category::Drums && r.truth.kind == Some(Kind::OneShot) {
            drum_total += 1;
            drum_right += ok as usize;
        }
        *confusion.entry(first).or_default().entry(got).or_default() += 1;
    }
    println!(
        "\n{title}: {}   drum one-shots: {}",
        pct(right, total),
        pct(drum_right, drum_total)
    );
    let mut cats: Vec<&Category> = confusion.keys().collect();
    cats.sort_by_key(|c| Category::ALL.iter().position(|x| x == *c));
    for c in cats {
        let row = &confusion[c];
        let n: usize = row.values().sum();
        let accepted: usize = rows
            .iter()
            .filter(|r| r.truth.cats.first() == Some(c))
            .filter(|r| {
                r.truth.cats.contains(&if blind {
                    r.blind.category
                } else {
                    r.named.category
                })
            })
            .count();
        let mut taken: Vec<(&Category, &usize)> = row.iter().collect();
        taken.sort_by(|a, b| b.1.cmp(a.1));
        let taken: Vec<String> = taken
            .iter()
            .map(|(c, n)| format!("{} {n}", c.name()))
            .collect();
        println!(
            "  {:<11} {:>14}   as: {}",
            c.name(),
            pct(accepted, n),
            taken.join(", ")
        );
    }
}

fn kinds(rows: &[&Row]) {
    let (mut named, mut blind, mut total) = (0, 0, 0);
    let mut wrong = Vec::new();
    for r in rows {
        let Some(k) = r.truth.kind else { continue };
        total += 1;
        named += (r.named.kind == k) as usize;
        blind += (r.blind.kind == k) as usize;
        if r.blind.kind != k {
            wrong.push(format!(
                "{} ({:.1}s)",
                r.path.file_name().unwrap().to_string_lossy(),
                r.blind.seconds
            ));
        }
    }
    println!(
        "\nLoop or one-shot: with names {}, audio only {}",
        pct(named, total),
        pct(blind, total)
    );
    if !wrong.is_empty() {
        println!(
            "  audio-only misses: {}",
            wrong
                .iter()
                .take(20)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

fn pitches(rows: &[&Row]) {
    let (mut within, mut octave, mut total, mut classes, mut class_total) = (0, 0, 0, 0, 0);
    let mut misses = Vec::new();
    let mut by_pack: HashMap<String, (usize, usize)> = HashMap::new();
    for r in rows {
        let p = r.blind.pitch.as_ref();
        if let Some(want) = r.truth.note {
            total += 1;
            let entry = by_pack.entry(r.truth.pack.clone()).or_default();
            entry.1 += 1;
            match p {
                Some(p) => {
                    let off = (p.note as f32 + p.cents / 100.0) - want as f32;
                    if off.abs() <= 0.5 {
                        within += 1;
                        entry.0 += 1;
                    } else if ((off.abs() - 12.0).abs() <= 0.5) || ((off.abs() - 24.0).abs() <= 0.5)
                    {
                        octave += 1;
                        misses.push(format!(
                            "{} heard {} {:+.0}c",
                            r.path.file_stem().unwrap().to_string_lossy(),
                            note_name(p.note),
                            p.cents
                        ));
                    } else {
                        misses.push(format!(
                            "{} heard {} {:+.0}c",
                            r.path.file_stem().unwrap().to_string_lossy(),
                            note_name(p.note),
                            p.cents
                        ));
                    }
                }
                None => misses.push(format!(
                    "{}: none",
                    r.path.file_stem().unwrap().to_string_lossy()
                )),
            }
        }
        if let Some(want) = r.truth.class {
            class_total += 1;
            if p.is_some_and(|p| p.note % 12 == want) {
                classes += 1;
            }
        }
    }
    println!(
        "\nPitch (audio only): notes within ±50 cents {}, an octave or two off {}; pitch classes {}",
        pct(within, total),
        pct(octave, total),
        pct(classes, class_total)
    );
    for (pack, (n, d)) in &by_pack {
        println!("  {pack}: {}", pct(*n, *d));
    }
    if !misses.is_empty() {
        println!(
            "  misses: {}",
            misses
                .iter()
                .take(30)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
}

fn tempos(rows: &[&Row]) {
    let mut by_pack: HashMap<String, [usize; 5]> = HashMap::new();
    let mut misses = Vec::new();
    for r in rows {
        let Some(want) = r.truth.bpm else { continue };
        if r.truth.kind != Some(Kind::Loop) {
            continue;
        }
        let e = by_pack.entry(r.truth.pack.clone()).or_default();
        e[4] += 1;
        let got = r.blind.bpm.or_else(|| {
            // Called a one-shot: what tempo would it have had?
            let (c, rate) = tempo_engine::engine::read_audio(&r.path).ok()?;
            let f = features::extract(&c, rate, None);
            f.tempo.map(|t| t.bpm)
        });
        let Some(got) = got else {
            e[3] += 1;
            misses.push(format!(
                "{}: none",
                r.path.file_name().unwrap().to_string_lossy()
            ));
            continue;
        };
        let near = |k: f32| ((got - want * k) / (want * k)).abs() < 0.02;
        if near(1.0) {
            e[0] += 1;
        } else if near(0.5) || near(2.0) {
            e[1] += 1;
            misses.push(format!(
                "{}: {got:.1} (half/double)",
                r.path.file_name().unwrap().to_string_lossy()
            ));
        } else {
            e[2] += 1;
            misses.push(format!(
                "{}: {got:.1}",
                r.path.file_name().unwrap().to_string_lossy()
            ));
        }
    }
    println!("\nTempo of loops named with one (audio only, within 2%):");
    let mut packs: Vec<_> = by_pack.iter().collect();
    packs.sort_by_key(|p| p.0.clone());
    let mut sum = [0usize; 5];
    for (pack, e) in packs {
        println!(
            "  {pack:<16} right {:>16}  half/double {:>14}  other {:>3}  none {:>3}",
            pct(e[0], e[4]),
            pct(e[1], e[4]),
            e[2],
            e[3]
        );
        for i in 0..5 {
            sum[i] += e[i];
        }
    }
    println!(
        "  {:<16} right {:>16}  half/double {:>14}  other {:>3}  none {:>3}",
        "all",
        pct(sum[0], sum[4]),
        pct(sum[1], sum[4]),
        sum[2],
        sum[3]
    );
    if !misses.is_empty() {
        println!(
            "  misses: {}",
            misses
                .iter()
                .take(40)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
}

fn keys(rows: &[&Row]) {
    let (mut right, mut total, mut relative, mut mode_right, mut mode_total) = (0, 0, 0, 0, 0);
    let mut by_pack: HashMap<String, (usize, usize)> = HashMap::new();
    for r in rows {
        let Some(root) = r.truth.root else { continue };
        let e = by_pack.entry(r.truth.pack.clone()).or_default();
        total += 1;
        e.1 += 1;
        let Some(k) = &r.blind.key else { continue };
        if k.tonic == root {
            right += 1;
            e.0 += 1;
            if let Some(m) = r.truth.minor {
                mode_total += 1;
                mode_right += (k.minor == m) as usize;
            }
        } else if (k.minor && (k.tonic + 3) % 12 == root)
            || (!k.minor && (k.tonic + 9) % 12 == root)
        {
            relative += 1;
        }
    }
    println!(
        "\nKey (audio only) where the name gives a root: tonic right {}, its relative {}; mode right where the tonic is {}",
        pct(right, total),
        pct(relative, total),
        pct(mode_right, mode_total)
    );
    for (pack, (n, d)) in &by_pack {
        println!("  {pack}: {}", pct(*n, *d));
    }
    println!("\nKeys of the 90s pack's melodic loops (audio only):");
    for r in rows {
        if r.truth.pack != "90s"
            || r.blind.kind != Kind::Loop
            || r.truth.cats.first().is_none_or(|c| c.is_drum())
        {
            continue;
        }
        let chroma: Vec<String> = {
            let mut order: Vec<usize> = (0..12).collect();
            order.sort_by(|&a, &b| r.blind.chroma[b].total_cmp(&r.blind.chroma[a]));
            order
                .iter()
                .take(4)
                .map(|&i| format!("{} {:.0}%", NOTES[i], r.blind.chroma[i] * 100.0))
                .collect()
        };
        println!(
            "  {:<34} {:<10} {:<9} fit {:>5}  tonal {:.2}  top: {}",
            r.path.file_stem().unwrap().to_string_lossy(),
            r.blind.category.name(),
            r.blind
                .key
                .as_ref()
                .map_or("-".to_string(), |k| k.name.clone()),
            r.blind
                .key
                .as_ref()
                .map_or("-".to_string(), |k| format!("{:.2}", k.confidence)),
            r.blind.tonal,
            chroma.join(", ")
        );
    }
}

/// How often a one-shot's five nearest neighbours (by embedding) share its category family.
fn neighbours(rows: &[&Row]) {
    let family = |c: Category| match c {
        Category::HatOpen => Category::HatClosed,
        Category::Ride => Category::Crash,
        c => c,
    };
    let shots: Vec<&&Row> = rows
        .iter()
        .filter(|r| r.truth.kind == Some(Kind::OneShot) && !r.truth.cats.is_empty())
        .collect();
    let (mut shared, mut total) = (0, 0);
    let mut per: HashMap<Category, (usize, usize)> = HashMap::new();
    for a in &shots {
        let mut near: Vec<(f32, &&&Row)> = shots
            .iter()
            .filter(|b| b.path != a.path)
            .map(|b| {
                (
                    a.blind
                        .embedding
                        .iter()
                        .zip(&b.blind.embedding)
                        .map(|(x, y)| x * y)
                        .sum::<f32>(),
                    b,
                )
            })
            .collect();
        near.sort_by(|x, y| y.0.total_cmp(&x.0));
        let mine = family(a.truth.cats[0]);
        let e = per.entry(mine).or_default();
        for (_, b) in near.iter().take(5) {
            total += 1;
            e.1 += 1;
            if family(b.truth.cats[0]) == mine {
                shared += 1;
                e.0 += 1;
            }
        }
    }
    println!(
        "\nEmbedding: one-shots' 5 nearest neighbours sharing their category: {}",
        pct(shared, total)
    );
    let mut cats: Vec<_> = per.iter().collect();
    cats.sort_by_key(|c| Category::ALL.iter().position(|x| x == c.0));
    let parts: Vec<String> = cats
        .iter()
        .map(|(c, (n, d))| format!("{} {:.0}%", c.name(), 100.0 * *n as f32 / *d as f32))
        .collect();
    println!("  {}", parts.join(", "));
}

fn duplicates(rows: &[&Row]) {
    let mut by: HashMap<u64, Vec<&PathBuf>> = HashMap::new();
    for r in rows {
        by.entry(r.blind.fingerprint).or_default().push(&r.path);
    }
    let groups: Vec<&Vec<&PathBuf>> = by.values().filter(|g| g.len() > 1).collect();
    let mut same = 0;
    for g in &groups {
        let read = |p: &PathBuf| tempo_engine::engine::read_audio(p).ok().map(|a| a.0);
        let first = read(g[0]);
        if g[1..].iter().all(|p| read(p) == first) {
            same += 1;
        }
    }
    println!(
        "\nFingerprints: {} files, {} shared by more than one file ({} of them identical audio)",
        rows.len(),
        groups.len(),
        same
    );
    for g in groups.iter().take(10) {
        let names: Vec<String> = g
            .iter()
            .map(|p| {
                p.strip_prefix(p.ancestors().nth(3).unwrap())
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        println!("  {}", names.join(" = "));
    }
}

fn speed(rows: &[&Row]) {
    let mut shots: Vec<f32> = rows
        .iter()
        .filter(|r| r.blind.kind == Kind::OneShot && r.blind.seconds < 4.0)
        .map(|r| r.millis)
        .collect();
    let mut loops: Vec<f32> = rows
        .iter()
        .filter(|r| r.blind.kind == Kind::Loop)
        .map(|r| r.millis * 8.0 / r.blind.seconds.max(1.0))
        .collect();
    let stats = |v: &mut Vec<f32>| {
        v.sort_by(|a, b| a.total_cmp(b));
        if v.is_empty() {
            return "-".to_string();
        }
        format!(
            "median {:.1} ms, 95th {:.1} ms, max {:.1} ms",
            v[v.len() / 2],
            v[v.len() * 95 / 100],
            v[v.len() - 1]
        )
    };
    println!(
        "\nSpeed (reading and analysing, every core busy): one-shots {}; loops per 8 s {}",
        stats(&mut shots),
        stats(&mut loops)
    );
}

fn list(rows: &[&Row]) {
    for r in rows {
        let a = &r.blind;
        println!(
            "{:<60} truth {:<22} named {:<11} blind {:<11} {:.2} {:?} bpm {:?} bars {:?} pitch {} key {} tonal {:.2} traits {}",
            r.path
                .to_string_lossy()
                .rsplit('/')
                .take(2)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("/"),
            r.truth
                .cats
                .iter()
                .map(|c| c.name())
                .collect::<Vec<_>>()
                .join("|"),
            r.named.category.name(),
            a.category.name(),
            a.confidence,
            a.kind,
            a.bpm,
            a.bars,
            a.pitch.map_or("-".into(), |p| format!(
                "{}{:+.0}c({:.2})",
                note_name(p.note),
                p.cents,
                p.confidence
            )),
            a.key.as_ref().map_or("-".into(), |k| k.name.clone()),
            a.tonal,
            a.traits.join(",")
        );
    }
}

/// A fitted model: standardisation, an optional hidden layer (ReLU), and a softmax.
#[derive(Clone)]
struct Net {
    mean: Vec<f32>,
    scale: Vec<f32>,
    hidden: Vec<Vec<f32>>,
    hidden_bias: Vec<f32>,
    weights: Vec<Vec<f32>>,
    bias: Vec<f32>,
}

impl Net {
    fn standardise(&self, x: &[f32]) -> Vec<f32> {
        x.iter()
            .zip(&self.mean)
            .zip(&self.scale)
            .map(|((v, mu), s)| ((v - mu) / s).clamp(-6.0, 6.0))
            .collect()
    }

    /// The hidden layer's output (or the input, with none), and the logits.
    fn forward(&self, z: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let h: Vec<f32> = if self.hidden.is_empty() {
            z.to_vec()
        } else {
            self.hidden
                .iter()
                .zip(&self.hidden_bias)
                .map(|(w, b)| (b + w.iter().zip(z).map(|(a, v)| a * v).sum::<f32>()).max(0.0))
                .collect()
        };
        let logits = self
            .weights
            .iter()
            .zip(&self.bias)
            .map(|(w, b)| b + w.iter().zip(&h).map(|(a, v)| a * v).sum::<f32>())
            .collect();
        (h, logits)
    }

    fn probs(&self, x: &[f32]) -> Vec<f32> {
        let (_, l) = self.forward(&self.standardise(x));
        let top = l.iter().fold(f32::MIN, |a, &v| a.max(v));
        let e: Vec<f32> = l.iter().map(|v| (v - top).exp()).collect();
        let s: f32 = e.iter().sum();
        e.iter().map(|v| v / s).collect()
    }
}

/// Several nets fitted from different starts, their probabilities averaged.
struct Ensemble(Vec<Net>);

impl Ensemble {
    fn predict(&self, x: &[f32]) -> usize {
        let mut sum = vec![0f32; self.0[0].bias.len()];
        for n in &self.0 {
            for (s, p) in sum.iter_mut().zip(n.probs(x)) {
                *s += p;
            }
        }
        (0..sum.len())
            .max_by(|&a, &b| sum[a].total_cmp(&sum[b]))
            .unwrap()
    }
}

fn fit_ensemble(xs: &[Vec<f32>], ys: &[usize], classes: usize, hidden: usize) -> Ensemble {
    let count = if hidden == 0 { 1 } else { 5 };
    let nets: Vec<Net> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..count)
            .map(|seed| scope.spawn(move || fit_net(xs, ys, classes, hidden, seed as u64)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    Ensemble(nets)
}

/// Eigenvalues and eigenvectors (as columns) of a symmetric matrix, by Jacobi rotations.
fn jacobi(mut a: Vec<Vec<f64>>) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = a.len();
    let mut v: Vec<Vec<f64>> = (0..n)
        .map(|i| (0..n).map(|j| (i == j) as u8 as f64).collect())
        .collect();
    for _ in 0..60 {
        let off: f64 = (0..n)
            .flat_map(|i| (i + 1..n).map(move |j| (i, j)))
            .map(|(i, j)| a[i][j] * a[i][j])
            .sum();
        if off < 1e-18 {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                if a[p][q].abs() < 1e-20 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for row in a.iter_mut() {
                    let (kp, kq) = (row[p], row[q]);
                    row[p] = c * kp - s * kq;
                    row[q] = s * kp + c * kq;
                }
                let (row_p, row_q) = (a[p].clone(), a[q].clone());
                for (k, (pk, qk)) in row_p.into_iter().zip(row_q).enumerate() {
                    a[p][k] = c * pk - s * qk;
                    a[q][k] = s * pk + c * qk;
                }
                for row in v.iter_mut() {
                    let (kp, kq) = (row[p], row[q]);
                    row[p] = c * kp - s * kq;
                    row[q] = s * kp + c * kq;
                }
            }
        }
    }
    ((0..n).map(|i| a[i][i]).collect(), v)
}

/// A linear discriminant projection to `dims` dimensions (rows of the result): within-class
/// scatter whitened (regularised by `reg`), then the directions the classes' (balanced) means
/// spread along most.
fn lda(zs: &[Vec<f32>], ys: &[usize], classes: usize, dims: usize, reg: f64) -> Vec<Vec<f32>> {
    let d = zs[0].len();
    let mut means = vec![vec![0f64; d]; classes];
    let mut counts = vec![0f64; classes];
    for (z, &y) in zs.iter().zip(ys) {
        counts[y] += 1.0;
        for (m, v) in means[y].iter_mut().zip(z) {
            *m += *v as f64;
        }
    }
    for (m, n) in means.iter_mut().zip(&counts) {
        m.iter_mut().for_each(|v| *v /= n.max(1.0));
    }
    let grand: Vec<f64> = (0..d)
        .map(|j| means.iter().map(|m| m[j]).sum::<f64>() / classes as f64)
        .collect();
    let mut within = vec![vec![0f64; d]; d];
    for (z, &y) in zs.iter().zip(ys) {
        let diff: Vec<f64> = (0..d).map(|j| z[j] as f64 - means[y][j]).collect();
        // Each class counts the same, however many it has.
        let w = 1.0 / (counts[y] * classes as f64);
        for i in 0..d {
            for j in 0..d {
                within[i][j] += w * diff[i] * diff[j];
            }
        }
    }
    for (i, row) in within.iter_mut().enumerate() {
        row[i] += reg;
    }
    let mut between = vec![vec![0f64; d]; d];
    for m in &means {
        let diff: Vec<f64> = (0..d).map(|j| m[j] - grand[j]).collect();
        for i in 0..d {
            for j in 0..d {
                between[i][j] += diff[i] * diff[j] / classes as f64;
            }
        }
    }
    let (values, vectors) = jacobi(within);
    // Whitening: W = U Λ^-1/2.
    let w: Vec<Vec<f64>> = (0..d)
        .map(|i| {
            (0..d)
                .map(|k| vectors[i][k] / values[k].max(1e-9).sqrt())
                .collect()
        })
        .collect();
    let wbw: Vec<Vec<f64>> = (0..d)
        .map(|a| {
            (0..d)
                .map(|b| {
                    let mut s = 0.0;
                    for i in 0..d {
                        let wia = w[i][a];
                        if wia == 0.0 {
                            continue;
                        }
                        for j in 0..d {
                            s += wia * between[i][j] * w[j][b];
                        }
                    }
                    s
                })
                .collect()
        })
        .collect();
    let (values, vectors) = jacobi(wbw);
    let mut order: Vec<usize> = (0..d).collect();
    order.sort_by(|&a, &b| values[b].total_cmp(&values[a]));
    order
        .iter()
        .take(dims)
        .map(|&k| {
            (0..d)
                .map(|i| (0..d).map(|j| w[i][j] * vectors[j][k]).sum::<f64>() as f32)
                .collect()
        })
        .collect()
}

/// A small deterministic random source for initialising and shuffling.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Fits a classifier (classes balanced, L2-regularised) by Adam on mini-batches: a softmax
/// regression with `hidden` 0, else one hidden ReLU layer of that many units.
fn fit_net(xs: &[Vec<f32>], ys: &[usize], classes: usize, hidden: usize, seed: u64) -> Net {
    let n = xs.len();
    let d = xs[0].len();
    let mean: Vec<f32> = (0..d)
        .map(|j| xs.iter().map(|x| x[j]).sum::<f32>() / n as f32)
        .collect();
    let scale: Vec<f32> = (0..d)
        .map(|j| {
            ((xs.iter().map(|x| (x[j] - mean[j]).powi(2)).sum::<f32>() / n as f32).sqrt()).max(1e-3)
        })
        .collect();
    let mut rng = Rng(0x9e3779b97f4a7c15 ^ (seed.wrapping_add(1).wrapping_mul(0xbf58476d1ce4e5b9)));
    let width = if hidden == 0 { d } else { hidden };
    let init = |rng: &mut Rng, fan_in: usize| {
        (rng.next() * 2.0 - 1.0) * (6.0 / fan_in as f32).sqrt() * 0.5
    };
    let mut net = Net {
        mean,
        scale,
        hidden: (0..hidden)
            .map(|_| (0..d).map(|_| init(&mut rng, d)).collect())
            .collect(),
        hidden_bias: vec![0.01; hidden],
        weights: (0..classes)
            .map(|_| {
                (0..width)
                    .map(|_| {
                        if hidden == 0 {
                            0.0
                        } else {
                            init(&mut rng, width)
                        }
                    })
                    .collect()
            })
            .collect(),
        bias: vec![0.0; classes],
    };
    let z: Vec<Vec<f32>> = xs.iter().map(|x| net.standardise(x)).collect();
    let mut counts = vec![0usize; classes];
    ys.iter().for_each(|&y| counts[y] += 1);
    let weight: Vec<f32> = ys
        .iter()
        .map(|&y| (n as f32 / (classes as f32 * counts[y] as f32)).sqrt())
        .collect();
    // Adam state for every parameter, flattened in the order: hidden, hidden bias, weights, bias.
    let count = hidden * d + hidden + classes * width + classes;
    let (mut m1, mut m2) = (vec![0f32; count], vec![0f32; count]);
    let (lr, l2, b1, b2) = (
        if hidden == 0 { 0.03 } else { 0.004 },
        if hidden == 0 { 0.003 } else { 0.0005 },
        0.9f32,
        0.999f32,
    );
    let epochs = if hidden == 0 { 300 } else { 160 };
    let batch = 64;
    let mut order: Vec<usize> = (0..n).collect();
    let mut t = 0;
    for _ in 0..epochs {
        for i in (1..n).rev() {
            let j = (rng.next() * (i + 1) as f32) as usize % (i + 1);
            order.swap(i, j);
        }
        for chunk in order.chunks(batch) {
            let mut g = vec![0f32; count];
            let wsum: f32 = chunk.iter().map(|&i| weight[i]).sum();
            for &i in chunk {
                let (h, logits) = net.forward(&z[i]);
                let top = logits.iter().fold(f32::MIN, |a, &v| a.max(v));
                let exp: Vec<f32> = logits.iter().map(|l| (l - top).exp()).collect();
                let s: f32 = exp.iter().sum();
                let base = hidden * d + hidden;
                let mut dh = vec![0f32; width];
                for c in 0..classes {
                    let e = (exp[c] / s - (c == ys[i]) as u8 as f32) * weight[i] / wsum;
                    for k in 0..width {
                        g[base + c * width + k] += e * h[k];
                        dh[k] += e * net.weights[c][k];
                    }
                    g[base + classes * width + c] += e;
                }
                if hidden > 0 {
                    for k in 0..hidden {
                        if h[k] <= 0.0 {
                            continue;
                        }
                        for j in 0..d {
                            g[k * d + j] += dh[k] * z[i][j];
                        }
                        g[hidden * d + k] += dh[k];
                    }
                }
            }
            t += 1;
            let (c1, c2) = (1.0 - b1.powi(t), 1.0 - b2.powi(t));
            let mut idx = 0;
            let mut step = |p: &mut f32, grad: f32, decay: bool, idx: usize| {
                let grad = grad + if decay { l2 * *p } else { 0.0 };
                m1[idx] = b1 * m1[idx] + (1.0 - b1) * grad;
                m2[idx] = b2 * m2[idx] + (1.0 - b2) * grad * grad;
                *p -= lr * (m1[idx] / c1) / ((m2[idx] / c2).sqrt() + 1e-8);
            };
            for row in net.hidden.iter_mut() {
                for p in row.iter_mut() {
                    step(p, g[idx], true, idx);
                    idx += 1;
                }
            }
            for p in net.hidden_bias.iter_mut() {
                step(p, g[idx], false, idx);
                idx += 1;
            }
            for row in net.weights.iter_mut() {
                for p in row.iter_mut() {
                    step(p, g[idx], true, idx);
                    idx += 1;
                }
            }
            for p in net.bias.iter_mut() {
                step(p, g[idx], false, idx);
                idx += 1;
            }
        }
    }
    net
}

fn floats(v: &[f32]) -> String {
    let parts: Vec<String> = v.iter().map(|x| format!("{x:.5}")).collect();
    format!("&[{}]", parts.join(", "))
}

fn rows_code(rows: &[Vec<f32>]) -> String {
    if rows.is_empty() {
        return "&[]".into();
    }
    let rows: Vec<String> = rows
        .iter()
        .map(|r| format!("        {},", floats(r)))
        .collect();
    format!("&[\n{}\n    ]", rows.join("\n"))
}

fn net_code(name: &str, classes: &[Category], e: &Ensemble) -> String {
    let nets: Vec<String> = e.0.iter().map(|m| one_net(classes, m)).collect();
    format!("pub static {name}: &[Net] = &[\n{}];\n", nets.join(""))
}

fn one_net(classes: &[Category], m: &Net) -> String {
    let cats: Vec<String> = classes.iter().map(|c| format!("Category::{c:?}")).collect();
    format!(
        "    Net {{\n    classes: &[{}],\n    mean: {},\n    scale: {},\n    hidden: {},\n    hidden_bias: {},\n    weights: {},\n    bias: {},\n}},\n",
        cats.join(", "),
        floats(&m.mean),
        floats(&m.scale),
        rows_code(&m.hidden),
        floats(&m.hidden_bias),
        rows_code(&m.weights),
        floats(&m.bias)
    )
}

fn fit(rows: &[Row], out: &Path) {
    let train: Vec<&Row> = rows
        .iter()
        .filter(|r| !r.truth.held_out && r.feats.is_some())
        .collect();
    println!("fitting on {} files", train.len());
    // Loop or one-shot.
    let kinds: Vec<(&Vec<f32>, usize)> = train
        .iter()
        .filter_map(|r| {
            r.truth
                .kind
                .map(|k| (&r.feats.as_ref().unwrap().0, (k == Kind::Loop) as usize))
        })
        .collect();
    let kind = fit_ensemble(
        &kinds.iter().map(|k| k.0.clone()).collect::<Vec<_>>(),
        &kinds.iter().map(|k| k.1).collect::<Vec<_>>(),
        2,
        0,
    );
    // Categories, one model for one-shots and one for loops.
    let mut code = String::from(
        "//! The fitted constants the analysis uses: written by `examples/sample_eval.rs --fit` from\n//! MusicRadar's free SampleRadar packs (808 drums, 808 weight, essential drum kits, drum\n//! machines, analogue drums, trap, world percussion, lo-fi, synth chords, jazz keys, glitch\n//! vocals, synth impacts, hip-hop essentials, hip hop, bass guitar, MIDI orchestra, noise and\n//! crackle, soul and funk), the 90s Hip Hop pack held out.\n// Fitted numbers, some near a constant by chance.\n#![allow(clippy::approx_constant)]\n\nuse super::Category;\n\n/// A classifier over a standardised feature vector: a hidden ReLU layer (if it has one), then\n/// a softmax.\npub struct Net {\n    pub classes: &'static [Category],\n    pub mean: &'static [f32],\n    pub scale: &'static [f32],\n    pub hidden: &'static [&'static [f32]],\n    pub hidden_bias: &'static [f32],\n    pub weights: &'static [&'static [f32]],\n    pub bias: &'static [f32],\n}\n\n/// Loop (the second class) or one-shot.\n",
    );
    code += &net_code("KIND", &[], &kind);
    let mut one_shot_nets: Option<Ensemble> = None;
    for (kind, name) in [(Kind::OneShot, "ONE_SHOT"), (Kind::Loop, "LOOP")] {
        let labelled: Vec<(&Vec<f32>, Category)> = train
            .iter()
            .filter(|r| r.truth.kind.unwrap_or(r.blind.kind) == kind)
            .filter_map(|r| {
                let x = &r.feats.as_ref().unwrap().0;
                let first = *r.truth.cats.first()?;
                // A hat not named open or closed: open if it rings (decay, ln ms, past 150 ms).
                let c = if kind == Kind::OneShot
                    && r.truth.cats == [Category::HatClosed, Category::HatOpen]
                    && x[2] > 151f32.ln()
                {
                    Category::HatOpen
                } else {
                    first
                };
                Some((x, c))
            })
            .collect();
        let mut classes: Vec<Category> = Vec::new();
        for (_, c) in &labelled {
            if !classes.contains(c) {
                classes.push(*c);
            }
        }
        // Too few to learn from: left to names.
        classes.retain(|c| labelled.iter().filter(|(_, x)| x == c).count() >= 5);
        classes.sort_by_key(|c| Category::ALL.iter().position(|x| x == c));
        let used: Vec<&(&Vec<f32>, Category)> = labelled
            .iter()
            .filter(|(_, c)| classes.contains(c))
            .collect();
        let xs: Vec<Vec<f32>> = used.iter().map(|(x, _)| (*x).clone()).collect();
        let ys: Vec<usize> = used
            .iter()
            .map(|(_, c)| classes.iter().position(|x| x == c).unwrap())
            .collect();
        println!(
            "{name}: {} files, classes {:?}",
            xs.len(),
            classes.iter().map(|c| c.name()).collect::<Vec<_>>()
        );
        // Five nets of 16 hidden units: as well as any size tried on the held-out pack, and
        // averaging them steadies what one alone gets right by luck.
        let hidden = 16;
        let m = fit_ensemble(&xs, &ys, classes.len(), hidden);
        let predict = |m: &Ensemble, x: &[f32]| m.predict(x);
        let right = xs
            .iter()
            .zip(&ys)
            .filter(|(x, y)| predict(&m, x) == **y)
            .count();
        println!("  training accuracy {}", pct(right, xs.len()));
        // The held-out pack, from its audio alone (its kind as named).
        let held: Vec<&Row> = rows
            .iter()
            .filter(|r| {
                r.truth.held_out
                    && r.truth.kind == Some(kind)
                    && !r.truth.cats.is_empty()
                    && r.feats.is_some()
            })
            .collect();
        let (mut ok, mut drums_ok, mut drums) = (0, 0, 0);
        let mut confusion: HashMap<Category, HashMap<Category, usize>> = HashMap::new();
        for r in &held {
            let got = classes[predict(&m, &r.feats.as_ref().unwrap().0)];
            let hit = r.truth.cats.contains(&got);
            *confusion
                .entry(r.truth.cats[0])
                .or_default()
                .entry(got)
                .or_default() += 1;
            ok += hit as usize;
            if r.truth.cats[0].is_drum() && r.truth.cats[0] != Category::Drums {
                drums += 1;
                drums_ok += hit as usize;
            }
        }
        println!(
            "  held-out 90s pack: {}, its drums {}",
            pct(ok, held.len()),
            pct(drums_ok, drums)
        );
        if std::env::var("CONFUSION").is_ok() {
            for (truth, row) in &confusion {
                let mut row: Vec<_> = row.iter().collect();
                row.sort_by(|a, b| b.1.cmp(a.1));
                let row: Vec<String> = row
                    .iter()
                    .map(|(c, n)| format!("{} {n}", c.name()))
                    .collect();
                println!("    {:<11} {}", truth.name(), row.join(", "));
            }
        }
        if std::env::var("LOPO").is_ok() {
            // Each pack predicted by a model fitted on the others.
            let packs: Vec<&String> = {
                let mut p: Vec<&String> = used
                    .iter()
                    .map(|u| {
                        &train
                            .iter()
                            .find(|r| std::ptr::eq(&r.feats.as_ref().unwrap().0, u.0))
                            .unwrap()
                            .truth
                            .pack
                    })
                    .collect();
                p.sort();
                p.dedup();
                p
            };
            let pack_of: Vec<&String> = used
                .iter()
                .map(|u| {
                    &train
                        .iter()
                        .find(|r| std::ptr::eq(&r.feats.as_ref().unwrap().0, u.0))
                        .unwrap()
                        .truth
                        .pack
                })
                .collect();
            let (mut all_right, mut all_n) = (0, 0);
            let mut confused: HashMap<(usize, usize), usize> = HashMap::new();
            for pack in packs {
                let (tx, ty): (Vec<Vec<f32>>, Vec<usize>) = xs
                    .iter()
                    .zip(&ys)
                    .zip(&pack_of)
                    .filter(|(_, p)| **p != pack)
                    .map(|((x, y), _)| (x.clone(), *y))
                    .unzip();
                let m = fit_ensemble(&tx, &ty, classes.len(), hidden);
                let held: Vec<(&Vec<f32>, &usize)> = xs
                    .iter()
                    .zip(&ys)
                    .zip(&pack_of)
                    .filter(|(_, p)| **p == pack)
                    .map(|(xy, _)| xy)
                    .collect();
                let right = held.iter().filter(|(x, y)| predict(&m, x) == **y).count();
                for (x, y) in &held {
                    *confused.entry((**y, predict(&m, x))).or_default() += 1;
                }
                all_right += right;
                all_n += held.len();
                println!("  {pack} held out: {}", pct(right, held.len()));
            }
            println!("  all packs held out in turn: {}", pct(all_right, all_n));
            for (c, class) in classes.iter().enumerate() {
                let n: usize = confused
                    .iter()
                    .filter(|((t, _), _)| *t == c)
                    .map(|(_, v)| v)
                    .sum();
                let mut row: Vec<(usize, usize)> = confused
                    .iter()
                    .filter(|((t, _), _)| *t == c)
                    .map(|((_, p), v)| (*p, *v))
                    .collect();
                row.sort_by_key(|r| std::cmp::Reverse(r.1));
                let row: Vec<String> = row
                    .iter()
                    .take(5)
                    .map(|(p, v)| format!("{} {v}", classes[*p].name()))
                    .collect();
                println!("    {:<11} {n:>4}: {}", class.name(), row.join(", "));
            }
        }
        code += &format!(
            "\n/// Categories of {}.\n",
            if kind == Kind::OneShot {
                "one-shots"
            } else {
                "loops"
            }
        );
        code += &net_code(name, &classes, &m);
        if kind == Kind::OneShot {
            one_shot_nets = Some(m);
        }
    }
    // The embedding: a linear discriminant projection of the standardised feature vector, so
    // what's near is what's alike in the ways that tell sounds apart.
    let labelled: Vec<(&Vec<f32>, Category)> = train
        .iter()
        .filter_map(|r| {
            r.truth
                .cats
                .first()
                .map(|c| (&r.feats.as_ref().unwrap().0, *c))
        })
        .collect();
    let d = labelled[0].0.len();
    let n = labelled.len() as f32;
    let mean: Vec<f32> = (0..d)
        .map(|j| labelled.iter().map(|e| e.0[j]).sum::<f32>() / n)
        .collect();
    let scale: Vec<f32> = (0..d)
        .map(|j| {
            (labelled
                .iter()
                .map(|e| (e.0[j] - mean[j]).powi(2))
                .sum::<f32>()
                / n)
                .sqrt()
                .max(1e-3)
        })
        .collect();
    let nets = one_shot_nets.as_ref().unwrap();
    // The standardised vector, and the one-shot nets' hidden layers after it.
    let standard = |x: &[f32]| -> Vec<f32> {
        let mut z: Vec<f32> = x
            .iter()
            .zip(&mean)
            .zip(&scale)
            .map(|((v, m), s)| ((v - m) / s).clamp(-6.0, 6.0))
            .collect();
        z.extend(nets.0.iter().flat_map(|n| n.forward(&n.standardise(x)).0));
        z
    };
    let mut cats: Vec<Category> = labelled.iter().map(|l| l.1).collect();
    cats.sort_by_key(|c| Category::ALL.iter().position(|x| x == c));
    cats.dedup();
    let zs: Vec<Vec<f32>> = labelled.iter().map(|l| standard(l.0)).collect();
    let ys: Vec<usize> = labelled
        .iter()
        .map(|l| cats.iter().position(|c| *c == l.1).unwrap())
        .collect();
    let projection = lda(&zs, &ys, cats.len(), 32, 0.5);
    let embed = |x: &[f32]| -> Vec<f32> {
        let z = standard(x);
        let mut e: Vec<f32> = projection
            .iter()
            .map(|p| p.iter().zip(&z).map(|(a, b)| a * b).sum())
            .collect();
        let norm = e.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-9);
        e.iter_mut().for_each(|v| *v /= norm);
        e
    };
    // How it does on the held-out pack's one-shots, against the embedding the analysis has now.
    let family = |c: Category| match c {
        Category::HatOpen => Category::HatClosed,
        Category::Ride => Category::Crash,
        c => c,
    };
    // Within each pack, how often a one-shot's 5 nearest share its category.
    let knn = |group: &[&Row], embed: &dyn Fn(&Row) -> Vec<f32>| -> (usize, usize) {
        let vectors: Vec<Vec<f32>> = group.iter().map(|r| embed(r)).collect();
        let mut shared = 0;
        let mut per: HashMap<Category, (usize, usize, HashMap<Category, usize>)> = HashMap::new();
        for (i, a) in vectors.iter().enumerate() {
            let mut near: Vec<(f32, usize)> = vectors
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(j, b)| (a.iter().zip(b).map(|(x, y)| x * y).sum(), j))
                .collect();
            near.sort_by(|x, y| y.0.total_cmp(&x.0));
            let mine = family(group[i].truth.cats[0]);
            let e = per.entry(mine).or_default();
            for (_, j) in near.iter().take(5) {
                let theirs = family(group[*j].truth.cats[0]);
                e.1 += 1;
                if theirs == mine {
                    shared += 1;
                    e.0 += 1;
                } else {
                    *e.2.entry(theirs).or_default() += 1;
                }
            }
        }
        if group[0].truth.held_out && std::env::var("KNN").is_ok() {
            for (c, (n, d, others)) in &per {
                let mut o: Vec<_> = others.iter().collect();
                o.sort_by(|a, b| b.1.cmp(a.1));
                let o: Vec<String> = o
                    .iter()
                    .take(4)
                    .map(|(c, n)| format!("{} {n}", c.name()))
                    .collect();
                println!(
                    "    {:<11} {:>5.1}% of {d}: else {}",
                    c.name(),
                    100.0 * *n as f32 / *d as f32,
                    o.join(", ")
                );
            }
        }
        (shared, group.len() * 5)
    };
    let mut packs: Vec<&String> = rows.iter().map(|r| &r.truth.pack).collect();
    packs.sort();
    packs.dedup();
    let (mut tn, mut td, mut fnn, mut fd) = (0, 0, 0, 0);
    for pack in packs {
        let group: Vec<&Row> = rows
            .iter()
            .filter(|r| {
                &r.truth.pack == pack
                    && r.truth.kind == Some(Kind::OneShot)
                    && !r.truth.cats.is_empty()
                    && r.feats.is_some()
            })
            .collect();
        if group.len() < 20 {
            continue;
        }
        let now = knn(&group, &|r: &Row| r.blind.embedding.clone());
        let fitted = knn(&group, &|r: &Row| embed(&r.feats.as_ref().unwrap().0));
        if group[0].truth.held_out {
            println!(
                "embedding, {pack} (held out): now {}, fitted {}",
                pct(now.0, now.1),
                pct(fitted.0, fitted.1)
            );
        } else {
            (tn, td, fnn, fd) = (tn + now.0, td + now.1, fnn + fitted.0, fd + fitted.1);
        }
    }
    println!(
        "embedding, the other packs each by itself: now {}, fitted {}",
        pct(tn, td),
        pct(fnn, fd)
    );
    code += &format!(
        "\n/// The embedding: the feature vector standardised (centres, scales), the one-shot nets' hidden\n/// layers after it, then projected (a linear discriminant analysis of the packs' categories).\npub static EMBED_MEAN: &[f32] = {};\npub static EMBED_SCALE: &[f32] = {};\npub static EMBED_PROJECTION: &[&[f32]] = {};\n",
        floats(&mean),
        floats(&scale),
        rows_code(&projection)
    );
    // Per category, the median and spread (interquartile range / 1.35) of the trait inputs.
    code += "\n/// Per category: the median and spread of what traits are judged by (see\n/// `analyze::trait_inputs`).\npub static TRAIT_STATS: &[(Category, [(f32, f32); 10])] = &[\n";
    let mut cats: Vec<Category> = Vec::new();
    for r in &train {
        if let Some(c) = r.truth.cats.first()
            && !cats.contains(c)
        {
            cats.push(*c);
        }
    }
    cats.sort_by_key(|c| Category::ALL.iter().position(|x| x == c));
    cats.push(Category::Other);
    for c in cats {
        let of: Vec<&[f32; 10]> = train
            .iter()
            .filter(|r| c == Category::Other || r.truth.cats.first() == Some(&c))
            .map(|r| &r.feats.as_ref().unwrap().1)
            .collect();
        if of.len() < 5 {
            continue;
        }
        let stats: Vec<String> = (0..10)
            .map(|j| {
                let mut v: Vec<f32> = of.iter().map(|x| x[j]).collect();
                v.sort_by(|a, b| a.total_cmp(b));
                let q = |p: f32| v[((v.len() - 1) as f32 * p) as usize];
                format!(
                    "({:.3}, {:.3})",
                    q(0.5),
                    ((q(0.75) - q(0.25)) / 1.35).max(0.05)
                )
            })
            .collect();
        code += &format!(
            "    (\n        Category::{c:?},\n        [{}],\n    ),\n",
            stats.join(", ")
        );
    }
    code += "];\n";
    std::fs::write(out, code).unwrap();
    println!("wrote {}", out.display());
}

/// How long analysing takes on one core, file by file (read, then analyse).
fn time_check(files: &[PathBuf]) {
    let (mut shots, mut loops) = (Vec::new(), Vec::new());
    for path in files {
        let start = Instant::now();
        let Ok(a) = tempo_engine::samples::analyze::analyze_file(path) else {
            continue;
        };
        let ms = start.elapsed().as_secs_f32() * 1000.0;
        match a.kind {
            Kind::OneShot if a.seconds < 4.0 => shots.push(ms),
            Kind::Loop => loops.push(ms * 8.0 / a.seconds.max(1.0)),
            _ => {}
        }
    }
    let stats = |v: &mut Vec<f32>| {
        v.sort_by(|a, b| a.total_cmp(b));
        if v.is_empty() {
            return "-".to_string();
        }
        format!(
            "median {:.1} ms, 95th {:.1} ms, max {:.1} ms ({} files)",
            v[v.len() / 2],
            v[v.len() * 95 / 100],
            v[v.len() - 1],
            v.len()
        )
    };
    println!(
        "one core: one-shots {}; loops per 8 s {}",
        stats(&mut shots),
        stats(&mut loops)
    );
}

/// Key alone, from the audio, for every file whose name gives a root: how often the tonic is
/// right, for a range of weights on the bass line and its start.
fn key_check(files: &[PathBuf]) {
    let labelled: Vec<(&PathBuf, Truth)> = files
        .iter()
        .map(|f| (f, truth(f)))
        .filter(|(_, t)| t.root.is_some())
        .collect();
    let next = AtomicUsize::new(0);
    let out = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..std::thread::available_parallelism().map_or(4, |n| n.get()) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((path, t)) = labelled.get(i) else {
                        break;
                    };
                    let Ok((c, rate)) = tempo_engine::engine::read_audio(path) else {
                        continue;
                    };
                    let f = features::extract(&c, rate, None);
                    out.lock().unwrap().push((t.clone(), f.tonal));
                }
            });
        }
    });
    let out = out.into_inner().unwrap();
    // How far off the tonic is, when it is, and where the chroma's strongest class sits.
    let mut packs: Vec<String> = out.iter().map(|o| o.0.pack.clone()).collect();
    packs.sort();
    packs.dedup();
    for pack in &packs {
        let mut off = [0usize; 12];
        let mut top = [0usize; 12];
        let mut bass = [0usize; 12];
        for (t, tonal) in out.iter().filter(|o| &o.0.pack == pack) {
            let root = t.root.unwrap() as usize;
            if let Some((tonic, _, _)) = tempo_engine::samples::tonal::key(tonal) {
                off[(tonic as usize + 12 - root) % 12] += 1;
            }
            let strongest = (0..12)
                .max_by(|&a, &b| tonal.chroma[a].total_cmp(&tonal.chroma[b]))
                .unwrap();
            top[(strongest + 12 - root) % 12] += 1;
            let low = (0..12)
                .max_by(|&a, &b| tonal.bass[a].total_cmp(&tonal.bass[b]))
                .unwrap();
            bass[(low + 12 - root) % 12] += 1;
        }
        println!(
            "{pack}: key-root {off:?}\n    strongest class-root {top:?}\n    strongest bass-root {bass:?}"
        );
    }
    let chords = out.iter().filter(|(t, _)| t.kind == Some(Kind::OneShot));
    let (n, right) = chords.fold((0, 0), |(n, r), (t, tonal)| {
        (
            n + 1,
            r + (tempo_engine::samples::tonal::chord(tonal).map(|c| c.0) == t.root) as usize,
        )
    });
    println!("chord roots of one-shots: {}", pct(right, n));
    for (bass, first, root) in [
        (1.0, 0.5, 0.0),
        (1.0, 0.5, 0.5),
        (1.0, 0.5, 1.0),
        (1.0, 0.5, 2.0),
        (0.5, 0.5, 1.0),
        (0.0, 0.0, 1.0),
        (0.0, 0.0, 2.0),
        (1.0, 1.0, 1.0),
    ] {
        {
            let mut by: HashMap<String, (usize, usize, usize)> = HashMap::new();
            for (t, tonal) in &out {
                let e = by.entry(t.pack.clone()).or_default();
                e.2 += 1;
                if let Some((tonic, minor, _)) =
                    tempo_engine::samples::tonal::key_weighted(tonal, bass, first, root)
                    && Some(tonic) == t.root
                {
                    e.0 += 1;
                    if t.minor == Some(minor) {
                        e.1 += 1;
                    }
                }
            }
            let mut packs: Vec<_> = by.into_iter().collect();
            packs.sort();
            let all = packs
                .iter()
                .fold((0, 0), |a, (_, e)| (a.0 + e.0, a.1 + e.2));
            let parts: Vec<String> = packs
                .iter()
                .map(|(p, e)| format!("{p} {:.0}%", 100.0 * e.0 as f32 / e.2 as f32))
                .collect();
            println!(
                "bass {bass} first {first} root {root}: {}  [{}]",
                pct(all.0, all.1),
                parts.join(", ")
            );
        }
    }
}

/// Tempo alone, from the audio, for every loop whose name gives one: right, half or double, or
/// other, pack by pack.
fn tempo_check(files: &[PathBuf]) {
    let loops: Vec<(&PathBuf, Truth)> = files
        .iter()
        .map(|f| (f, truth(f)))
        .filter(|(_, t)| t.bpm.is_some() && t.kind == Some(Kind::Loop))
        .collect();
    let next = AtomicUsize::new(0);
    let out = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..std::thread::available_parallelism().map_or(4, |n| n.get()) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((path, t)) = loops.get(i) else { break };
                    let Ok((c, rate)) = tempo_engine::engine::read_audio(path) else {
                        continue;
                    };
                    let f = features::extract(&c, rate, None);
                    out.lock().unwrap().push((
                        path.to_string_lossy().to_string(),
                        t.pack.clone(),
                        t.bpm.unwrap(),
                        f.tempo.map(|t| t.bpm),
                        f.seconds,
                    ));
                }
            });
        }
    });
    let mut out = out.into_inner().unwrap();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    let mut by: HashMap<String, [usize; 4]> = HashMap::new();
    for (path, pack, want, got, seconds) in &out {
        let e = by.entry(pack.clone()).or_default();
        e[3] += 1;
        let near = |k: f32| got.is_some_and(|g| ((g - want * k) / (want * k)).abs() < 0.02);
        if near(1.0) {
            e[0] += 1;
        } else if near(0.5) || near(2.0) {
            e[1] += 1;
            println!("  half/double {want} -> {got:?} ({seconds:.2}s) {path}");
        } else {
            e[2] += 1;
            println!("  wrong {want} -> {got:?} ({seconds:.2}s) {path}");
        }
    }
    let mut packs: Vec<_> = by.into_iter().collect();
    packs.sort();
    let mut all = [0usize; 4];
    for (pack, e) in packs {
        println!(
            "{pack:<16} right {:>16}  half/double {:>14}  other {}",
            pct(e[0], e[3]),
            pct(e[1], e[3]),
            e[2]
        );
        for i in 0..4 {
            all[i] += e[i];
        }
    }
    println!(
        "{:<16} right {:>16}  half/double {:>14}  other {}",
        "all",
        pct(all[0], all[3]),
        pct(all[1], all[3]),
        all[2]
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut folders: Vec<PathBuf> = Vec::new();
    let (mut fit_to, mut show, mut scan, mut tempo_only) = (None, false, false, false);
    let mut cache: Option<PathBuf> = None;
    let mut keys_only = false;
    let mut time_only = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--fit" => {
                i += 1;
                fit_to = Some(PathBuf::from(&args[i]));
            }
            "--list" => show = true,
            "--scan" => scan = true,
            "--tempo" => tempo_only = true,
            "--keys" => keys_only = true,
            "--time" => time_only = true,
            "--cache" => {
                i += 1;
                cache = Some(PathBuf::from(&args[i]));
            }
            other => folders.push(PathBuf::from(other)),
        }
        i += 1;
    }
    let pack = PathBuf::from(std::env::var("HOME").unwrap())
        .join("Music/tempo/Samples/MusicRadar 90s Hip Hop");
    if !folders.contains(&pack) {
        folders.insert(0, pack.clone());
    }
    let files: Vec<PathBuf> = folders.iter().flat_map(|f| audio_files(f)).collect();
    println!("{} files", files.len());
    if tempo_only {
        tempo_check(&files);
        return;
    }
    if keys_only {
        key_check(&files);
        return;
    }
    if time_only {
        time_check(&files);
        return;
    }
    let start = Instant::now();
    // A cache of every file's measures, to fit again without analysing again.
    let rows = match cache.as_ref().and_then(|c| std::fs::read(c).ok()) {
        Some(bytes) => serde_json::from_slice(&bytes).unwrap(),
        None => {
            let rows = run(&files, fit_to.is_some());
            if let Some(c) = &cache {
                std::fs::write(c, serde_json::to_vec(&rows).unwrap()).unwrap();
            }
            rows
        }
    };
    println!(
        "analysed in {:.1} s (each file twice: with its name and without)",
        start.elapsed().as_secs_f32()
    );
    if let Some(out) = fit_to {
        fit(&rows, &out);
        return;
    }
    let mut packs: Vec<String> = rows.iter().map(|r| r.truth.pack.clone()).collect();
    packs.sort();
    packs.dedup();
    for pack in packs {
        let of: Vec<&Row> = rows.iter().filter(|r| r.truth.pack == pack).collect();
        println!(
            "\n==== {} ({} files)",
            if pack.is_empty() { "unlabelled" } else { &pack },
            of.len()
        );
        if show {
            list(&of);
        }
        categories(&of, false, "Category with names");
        categories(&of, true, "Category from audio alone");
        kinds(&of);
        pitches(&of);
        tempos(&of);
        keys(&of);
        neighbours(&of);
        duplicates(&of);
        speed(&of);
    }
    if scan {
        let dir = std::env::temp_dir().join(format!("tempo-eval-{}", std::process::id()));
        // SAFETY: nothing else in this process reads the environment now.
        unsafe { std::env::set_var("TEMPO_LIBRARY", &dir) };
        let mut library = Library {
            folders: vec![pack],
            samples: Vec::new(),
        };
        let start = Instant::now();
        let report = library.scan(&|_, _| {});
        println!(
            "\nLibrary::scan of the 90s pack: {} samples in {:.1} s ({} failed)",
            report.samples,
            start.elapsed().as_secs_f32(),
            report.failed.len()
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
