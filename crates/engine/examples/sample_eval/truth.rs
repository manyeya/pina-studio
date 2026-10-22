//! What each labelled file is, read from its pack's folders and its name.

use super::*;

/// What a file's name and place say it is.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Truth {
    pub(crate) pack: String,
    pub(crate) kind: Option<Kind>,
    /// The categories it may fairly be called, the first the likeliest; none when unsure.
    pub(crate) cats: Vec<Category>,
    pub(crate) bpm: Option<f32>,
    /// A MIDI note, or only a pitch class.
    pub(crate) note: Option<i32>,
    pub(crate) class: Option<u8>,
    /// A key's (or chord's) root, and whether it's minor when known.
    pub(crate) root: Option<u8>,
    pub(crate) minor: Option<bool>,
    /// The 90s Hip Hop pack: measured, never fitted on.
    pub(crate) held_out: bool,
}

/// A pitch class from a note name's start: "C", "C#", "Db", "a#".
pub(crate) fn pitch_class(s: &str) -> Option<(u8, usize)> {
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
pub(crate) fn chord(s: &str) -> Option<(u8, Option<bool>)> {
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
pub(crate) fn note(s: &str) -> Option<i32> {
    let (class, used) = pitch_class(s)?;
    let octave: i32 = s[used..].parse().ok()?;
    Some(12 * (octave + 1) + class as i32)
}

/// Categories from words in a name, for the training packs' plainly named hits.
pub(crate) fn word_cats(name: &str) -> Vec<Category> {
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

pub(crate) fn bpm_in(s: &str) -> Option<f32> {
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
pub(crate) fn truth(path: &Path) -> Truth {
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
