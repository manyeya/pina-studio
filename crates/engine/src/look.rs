//! How a track looks: from what it plays, its role, and from that its icon and colour family.
//! A track's own `icon` and `color`, chosen by a person or an agent, take their place.
//!
//! Its role comes first from what it sounds like (its samples' and audio's analysis), then from
//! the plugin it plays, its name, and its notes; Logic takes it from a patch's metadata, FL Studio
//! from what you pick. Tracks that play alike share a colour family, each in a shade of its own,
//! so a session reads at a glance: drums warm, bass deep violet, keys teal, vocals pink.

use std::path::Path;

use crate::{
    project::{Content, Instrument, Track},
    samples::Category,
};

/// A colour family: what tracks that play alike share.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    Drums,
    Percussion,
    Bass,
    Keys,
    Guitar,
    Strings,
    Brass,
    Woodwind,
    Synth,
    Pad,
    Bells,
    Vocals,
    Fx,
    Texture,
    /// No role it says: audio and MIDI not yet known, and the marks (a star, a heart).
    Other,
}

impl Family {
    pub fn name(self) -> &'static str {
        match self {
            Family::Drums => "Drums",
            Family::Percussion => "Percussion",
            Family::Bass => "Bass",
            Family::Keys => "Keys",
            Family::Guitar => "Guitars",
            Family::Strings => "Strings",
            Family::Brass => "Brass",
            Family::Woodwind => "Woodwind",
            Family::Synth => "Synths",
            Family::Pad => "Pads",
            Family::Bells => "Bells",
            Family::Vocals => "Vocals",
            Family::Fx => "Effects",
            Family::Texture => "Texture",
            Family::Other => "Other",
        }
    }
}

/// A track icon: its id (what a project keeps), what it's called, the family it stands for, the
/// group the picker shows it in, and words a search finds it by.
pub struct IconDef {
    pub id: &'static str,
    pub name: &'static str,
    pub family: Family,
    pub group: &'static str,
    pub words: &'static str,
}

/// Every track icon, in the picker's order.
pub const ICONS: &[IconDef] = &[
    IconDef {
        id: "kick",
        name: "Kick",
        family: Family::Drums,
        group: "Drums",
        words: "bd bass drum boom",
    },
    IconDef {
        id: "snare",
        name: "Snare",
        family: Family::Drums,
        group: "Drums",
        words: "sd rim backbeat",
    },
    IconDef {
        id: "clap",
        name: "Clap",
        family: Family::Drums,
        group: "Drums",
        words: "hands snap",
    },
    IconDef {
        id: "hihat",
        name: "Hi-hat",
        family: Family::Drums,
        group: "Drums",
        words: "hat hats closed open hh",
    },
    IconDef {
        id: "cymbal",
        name: "Cymbal",
        family: Family::Drums,
        group: "Drums",
        words: "crash ride splash china",
    },
    IconDef {
        id: "tom",
        name: "Tom",
        family: Family::Drums,
        group: "Drums",
        words: "toms floor rack",
    },
    IconDef {
        id: "drums",
        name: "Drum kit",
        family: Family::Drums,
        group: "Drums",
        words: "kit beat break loop drums",
    },
    IconDef {
        id: "sticks",
        name: "Sticks",
        family: Family::Drums,
        group: "Drums",
        words: "rim sidestick stick click",
    },
    IconDef {
        id: "conga",
        name: "Hand drum",
        family: Family::Percussion,
        group: "Percussion",
        words: "perc conga bongo djembe tabla hand",
    },
    IconDef {
        id: "shaker",
        name: "Shaker",
        family: Family::Percussion,
        group: "Percussion",
        words: "maraca shaker egg cabasa",
    },
    IconDef {
        id: "tambourine",
        name: "Tambourine",
        family: Family::Percussion,
        group: "Percussion",
        words: "tamb jingle",
    },
    IconDef {
        id: "cowbell",
        name: "Cowbell",
        family: Family::Percussion,
        group: "Percussion",
        words: "bell agogo block woodblock",
    },
    IconDef {
        id: "808",
        name: "808",
        family: Family::Bass,
        group: "Bass",
        words: "sub boom 808s low",
    },
    IconDef {
        id: "bass",
        name: "Bass",
        family: Family::Bass,
        group: "Bass",
        words: "bass low clef",
    },
    IconDef {
        id: "synth-bass",
        name: "Synth bass",
        family: Family::Bass,
        group: "Bass",
        words: "reese wobble sub synth bass",
    },
    IconDef {
        id: "piano",
        name: "Piano",
        family: Family::Keys,
        group: "Keys",
        words: "grand upright acoustic piano",
    },
    IconDef {
        id: "keys",
        name: "Keys",
        family: Family::Keys,
        group: "Keys",
        words: "rhodes ep electric piano wurli keyboard",
    },
    IconDef {
        id: "organ",
        name: "Organ",
        family: Family::Keys,
        group: "Keys",
        words: "hammond b3 church pipes",
    },
    IconDef {
        id: "chords",
        name: "Chords",
        family: Family::Keys,
        group: "Keys",
        words: "chord stab progression harmony",
    },
    IconDef {
        id: "guitar",
        name: "Guitar",
        family: Family::Guitar,
        group: "Guitars",
        words: "acoustic nylon steel strum",
    },
    IconDef {
        id: "guitar-electric",
        name: "Electric guitar",
        family: Family::Guitar,
        group: "Guitars",
        words: "electric guitar riff lick",
    },
    IconDef {
        id: "strings",
        name: "Strings",
        family: Family::Strings,
        group: "Strings",
        words: "violin viola cello orchestra ensemble",
    },
    IconDef {
        id: "harp",
        name: "Harp",
        family: Family::Strings,
        group: "Strings",
        words: "harp lyre",
    },
    IconDef {
        id: "brass",
        name: "Brass",
        family: Family::Brass,
        group: "Brass & wind",
        words: "trumpet horn trombone section",
    },
    IconDef {
        id: "sax",
        name: "Sax",
        family: Family::Brass,
        group: "Brass & wind",
        words: "saxophone alto tenor",
    },
    IconDef {
        id: "flute",
        name: "Flute",
        family: Family::Woodwind,
        group: "Brass & wind",
        words: "woodwind flute piccolo recorder pan",
    },
    IconDef {
        id: "synth",
        name: "Synth",
        family: Family::Synth,
        group: "Synths",
        words: "synth analog poly moog",
    },
    IconDef {
        id: "lead",
        name: "Lead",
        family: Family::Synth,
        group: "Synths",
        words: "lead saw melody hook topline",
    },
    IconDef {
        id: "arp",
        name: "Arp",
        family: Family::Synth,
        group: "Synths",
        words: "arpeggio arp sequence",
    },
    IconDef {
        id: "pluck",
        name: "Pluck",
        family: Family::Synth,
        group: "Synths",
        words: "pluck stab short",
    },
    IconDef {
        id: "pad",
        name: "Pad",
        family: Family::Pad,
        group: "Synths",
        words: "pad atmos atmosphere ambient wash",
    },
    IconDef {
        id: "bells",
        name: "Bells",
        family: Family::Bells,
        group: "Bells",
        words: "bell chime glock celesta music box",
    },
    IconDef {
        id: "mallets",
        name: "Mallets",
        family: Family::Bells,
        group: "Bells",
        words: "marimba xylophone vibes vibraphone kalimba",
    },
    IconDef {
        id: "vocals",
        name: "Vocals",
        family: Family::Vocals,
        group: "Vocals",
        words: "vox vocal voice lead vocal singer rap",
    },
    IconDef {
        id: "mic",
        name: "Mic",
        family: Family::Vocals,
        group: "Vocals",
        words: "mic recording studio condenser",
    },
    IconDef {
        id: "choir",
        name: "Choir",
        family: Family::Vocals,
        group: "Vocals",
        words: "choir backing harmony ensemble",
    },
    IconDef {
        id: "chop",
        name: "Vocal chop",
        family: Family::Vocals,
        group: "Vocals",
        words: "chop chopped slice glitch",
    },
    IconDef {
        id: "fx",
        name: "FX",
        family: Family::Fx,
        group: "Effects",
        words: "fx effect sfx sparkle",
    },
    IconDef {
        id: "riser",
        name: "Riser",
        family: Family::Fx,
        group: "Effects",
        words: "riser build sweep uplifter",
    },
    IconDef {
        id: "impact",
        name: "Impact",
        family: Family::Fx,
        group: "Effects",
        words: "impact hit boom downlifter crash",
    },
    IconDef {
        id: "reverse",
        name: "Reverse",
        family: Family::Fx,
        group: "Effects",
        words: "reverse swell reversed cymbal",
    },
    IconDef {
        id: "vinyl",
        name: "Vinyl",
        family: Family::Texture,
        group: "Texture",
        words: "vinyl crackle record dust lofi",
    },
    IconDef {
        id: "texture",
        name: "Texture",
        family: Family::Texture,
        group: "Texture",
        words: "texture noise ambience field atmos",
    },
    IconDef {
        id: "foley",
        name: "Foley",
        family: Family::Texture,
        group: "Texture",
        words: "foley footsteps found sound",
    },
    IconDef {
        id: "audio",
        name: "Audio",
        family: Family::Other,
        group: "General",
        words: "audio wave recording take",
    },
    IconDef {
        id: "midi",
        name: "MIDI",
        family: Family::Other,
        group: "General",
        words: "midi notes melody",
    },
    IconDef {
        id: "sampler",
        name: "Sampler",
        family: Family::Other,
        group: "General",
        words: "sampler pads mpc",
    },
    IconDef {
        id: "loop",
        name: "Loop",
        family: Family::Other,
        group: "General",
        words: "loop repeat cycle",
    },
    IconDef {
        id: "reference",
        name: "Reference",
        family: Family::Other,
        group: "General",
        words: "reference headphones mix check",
    },
    IconDef {
        id: "click",
        name: "Click",
        family: Family::Other,
        group: "General",
        words: "click metronome tempo",
    },
    IconDef {
        id: "star",
        name: "Star",
        family: Family::Other,
        group: "Marks",
        words: "star favourite main",
    },
    IconDef {
        id: "heart",
        name: "Heart",
        family: Family::Other,
        group: "Marks",
        words: "heart love keep",
    },
    IconDef {
        id: "flame",
        name: "Flame",
        family: Family::Other,
        group: "Marks",
        words: "flame fire hot energy",
    },
    IconDef {
        id: "bolt",
        name: "Bolt",
        family: Family::Other,
        group: "Marks",
        words: "bolt zap lightning energy",
    },
];

pub fn icon(id: &str) -> Option<&'static IconDef> {
    ICONS.iter().find(|i| i.id == id)
}

/// Where a track's look came from, for saying why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Chosen: by a person, or an agent.
    Chosen,
    /// Its samples or audio, by their analysis.
    Sound,
    /// The plugin it plays.
    Plugin,
    /// Its name.
    Name,
    /// Its notes: how low, how many at once.
    Notes,
    /// Only what kind of track it is.
    Kind,
}

impl Source {
    pub fn says(self) -> &'static str {
        match self {
            Source::Chosen => "chosen",
            Source::Sound => "from its sound",
            Source::Plugin => "from its plugin",
            Source::Name => "from its name",
            Source::Notes => "from its notes",
            Source::Kind => "from its kind",
        }
    }
}

/// A track's look, before colour: its icon and the family it's coloured in, and where they came
/// from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub icon: &'static str,
    pub family: Family,
    pub source: Source,
}

/// What a sample's analysed category looks like.
pub fn category_icon(c: Category) -> &'static str {
    match c {
        Category::Kick => "kick",
        Category::Snare => "snare",
        Category::Clap => "clap",
        Category::Rim | Category::Snap => "sticks",
        Category::HatClosed | Category::HatOpen => "hihat",
        Category::Ride | Category::Crash => "cymbal",
        Category::Tom => "tom",
        Category::Shaker => "shaker",
        Category::Tambourine => "tambourine",
        Category::Perc => "conga",
        Category::Drums => "drums",
        Category::Bass808 => "808",
        Category::Bass => "bass",
        Category::Keys => "keys",
        Category::Pad => "pad",
        Category::Lead => "lead",
        Category::Pluck => "pluck",
        Category::Bell => "bells",
        Category::Strings => "strings",
        Category::Brass => "brass",
        Category::Woodwind => "flute",
        Category::Guitar => "guitar",
        Category::Synth => "synth",
        Category::Chord => "chords",
        Category::Vocal => "vocals",
        Category::Fx => "fx",
        Category::Reverse => "reverse",
        Category::Texture => "texture",
        Category::Foley => "foley",
        Category::Other => "audio",
    }
}

fn is_drum(c: Category) -> bool {
    c.is_drum() || matches!(c, Category::Drums | Category::Shaker | Category::Tambourine)
}

/// Words in names and what they look like, most telling first: "Lead Vox" is vocals.
const NAMED: &[(&[&str], &str)] = &[
    (&["kick", "bd"], "kick"),
    (&["snare", "sd"], "snare"),
    (&["clap", "claps"], "clap"),
    (
        &["rim", "rimshot", "sidestick", "stick", "sticks"],
        "sticks",
    ),
    (&["hat", "hats", "hihat", "hihats", "hh"], "hihat"),
    (&["crash", "ride", "cymbal", "cymbals", "splash"], "cymbal"),
    (&["tom", "toms"], "tom"),
    (
        &["conga", "bongo", "djembe", "tabla", "perc", "percussion"],
        "conga",
    ),
    (&["shaker", "maraca", "maracas", "egg", "cabasa"], "shaker"),
    (&["tamb", "tambourine"], "tambourine"),
    (&["cowbell", "woodblock", "block"], "cowbell"),
    (&["808", "808s", "sub"], "808"),
    (&["choir", "backing", "bvs", "harmonies"], "choir"),
    (&["chop", "chops", "chopped"], "chop"),
    (
        &[
            "vox", "vocal", "vocals", "voice", "sing", "sung", "rap", "adlib", "adlibs",
        ],
        "vocals",
    ),
    (&["piano", "grand", "upright"], "piano"),
    (
        &[
            "rhodes",
            "wurli",
            "wurlitzer",
            "ep",
            "keys",
            "keyboard",
            "clav",
        ],
        "keys",
    ),
    (&["organ", "hammond", "b3"], "organ"),
    (&["guitar", "gtr", "acoustic"], "guitar"),
    (&["electric", "riff"], "guitar-electric"),
    (
        &[
            "strings",
            "string",
            "violin",
            "viola",
            "cello",
            "orchestra",
            "orch",
        ],
        "strings",
    ),
    (&["harp"], "harp"),
    (&["brass", "horn", "horns", "trumpet", "trombone"], "brass"),
    (&["sax", "saxophone"], "sax"),
    (
        &["flute", "woodwind", "clarinet", "oboe", "recorder"],
        "flute",
    ),
    (&["bass", "bassline"], "bass"),
    (&["arp", "arpeggio"], "arp"),
    (&["pluck", "plucks"], "pluck"),
    (&["lead", "melody", "hook", "topline"], "lead"),
    (
        &["pad", "pads", "atmos", "atmosphere", "ambient", "wash"],
        "pad",
    ),
    (
        &[
            "bell",
            "bells",
            "chime",
            "chimes",
            "glock",
            "glockenspiel",
            "celesta",
        ],
        "bells",
    ),
    (
        &[
            "marimba",
            "xylophone",
            "vibes",
            "vibraphone",
            "mallet",
            "mallets",
            "kalimba",
        ],
        "mallets",
    ),
    (&["chord", "chords", "stab", "stabs"], "chords"),
    (&["riser", "sweep", "uplifter", "build"], "riser"),
    (&["impact", "hit", "downlifter"], "impact"),
    (&["reverse", "rev", "reversed"], "reverse"),
    (&["fx", "sfx", "effect", "effects"], "fx"),
    (&["vinyl", "crackle", "dust", "stylus"], "vinyl"),
    (
        &["noise", "texture", "field", "ambience", "room"],
        "texture",
    ),
    (&["foley"], "foley"),
    (
        &[
            "drum", "drums", "beat", "beats", "kit", "break", "breaks", "groove",
        ],
        "drums",
    ),
];

/// Names that say what a track is rather than what it plays ("Synth 2"): its notes say more.
const NAMED_LOOSELY: &[(&[&str], &str)] = &[
    (&["synth", "synths", "inst", "instrument"], "synth"),
    (&["loop", "loops"], "loop"),
];

/// Plugins by name and what they're for.
const PLUGINS: &[(&[&str], &str)] = &[
    (
        &[
            "pianoteq",
            "piano",
            "grand",
            "keyscape",
            "noire",
            "ravenscroft",
        ],
        "piano",
    ),
    (
        &[
            "rhodes",
            "wurli",
            "lounge lizard",
            "electric piano",
            "e-piano",
        ],
        "keys",
    ),
    (&["organ", "b-3", "b3", "vb3"], "organ"),
    (
        &[
            "drum",
            "superior",
            "ezdrummer",
            "bfd",
            "battery",
            "drumlab",
            "addictive",
        ],
        "drums",
    ),
    (&["bass", "trilian", "scarbee", "modo bass"], "bass"),
    (&["guitar", "ample", "shreddage"], "guitar"),
    (
        &["string", "violin", "spitfire", "orchestra", "cello"],
        "strings",
    ),
    (&["brass", "trumpet", "horn"], "brass"),
    (&["choir", "vocal", "voice"], "choir"),
    (
        &[
            "serum",
            "vital",
            "massive",
            "surge",
            "diva",
            "pigments",
            "sylenth",
            "omnisphere",
            "phase plant",
            "dexed",
            "helm",
            "odin",
            "repro",
            "hive",
            "spire",
            "synth",
            "minimoog",
            "prophet",
            "juno",
            "jup",
        ],
        "synth",
    ),
];

/// The word lists' icon for words, if one says: each word as a whole word.
fn named(words: &str, table: &[(&[&str], &'static str)]) -> Option<&'static str> {
    let lower = words.to_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    table
        .iter()
        .find(|(keys, _)| {
            keys.iter()
                .any(|k| match k.contains(' ') || k.contains('-') {
                    true => lower.contains(k),
                    false => tokens.iter().any(|t| t == k),
                })
        })
        .map(|(_, icon)| *icon)
}

/// The icon its analysed samples or audio say, weighing each by how much it plays: a kit's
/// zones, mostly drums, are a drum kit.
fn by_sound(track: &Track, category: &dyn Fn(&Path) -> Option<Category>) -> Option<&'static str> {
    let mut heard: Vec<(Category, f64)> = Vec::new();
    let mut add = |c: Option<Category>, w: f64| {
        if let Some(c) = c.filter(|c| *c != Category::Other) {
            match heard.iter_mut().find(|(h, _)| *h == c) {
                Some((_, total)) => *total += w,
                None => heard.push((c, w)),
            }
        }
    };
    match &track.instrument {
        Some(Instrument::Sampler(s)) => {
            for z in &s.zones {
                add(category(&z.file), 1.0);
            }
            let zones: f64 = heard.iter().map(|h| h.1).sum();
            let drums: f64 = heard.iter().filter(|h| is_drum(h.0)).map(|h| h.1).sum();
            if heard.len() > 1 && drums >= 0.6 * zones {
                return Some("drums");
            }
        }
        None => {
            for r in &track.regions {
                if let Content::Audio { file, .. } = &r.content {
                    add(category(file), r.length as f64);
                }
            }
        }
        _ => {}
    }
    heard
        .iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(c, _)| category_icon(*c))
}

/// The icon its notes say: low and alone is bass, many at once chords.
fn by_notes(track: &Track) -> Option<&'static str> {
    let notes: Vec<_> = track
        .regions
        .iter()
        .flat_map(|r| match &r.content {
            Content::Midi { notes, .. } => notes.as_slice(),
            _ => &[],
        })
        .collect();
    if notes.is_empty() {
        return None;
    }
    let mut pitches: Vec<u8> = notes.iter().map(|n| n.pitch).collect();
    pitches.sort_unstable();
    let middle = pitches[pitches.len() / 2];
    // How many sound at once, on average, where any does.
    let together = notes
        .iter()
        .map(|n| {
            notes
                .iter()
                .filter(|m| {
                    m.start < n.start + n.length as i64 && n.start < m.start + m.length as i64
                })
                .count()
        })
        .sum::<usize>() as f64
        / notes.len() as f64;
    Some(match (middle, together) {
        (0..48, t) if t < 2.0 => "bass",
        (_, t) if t >= 2.6 => "chords",
        (60.., _) => "lead",
        _ => "synth",
    })
}

/// How a track looks: its own icon if it has one, else what it plays says. `category` gives the
/// analysed category of an audio file (relative to the project), if it's known.
pub fn look(track: &Track, category: &dyn Fn(&Path) -> Option<Category>) -> Look {
    let found = |id: &'static str, source| Look {
        icon: id,
        family: icon(id).map_or(Family::Other, |i| i.family),
        source,
    };
    // Its name refines what its sound says, within the family: keys called "Piano Double" are a
    // piano, vocals called "Vocal Chop" a chop.
    let refined = |i: &'static str| match named(&track.name, NAMED) {
        Some(n) if icon(n).map(|d| d.family) == icon(i).map(|d| d.family) => n,
        _ => i,
    };
    let inferred = by_sound(track, category)
        .map(|i| found(refined(i), Source::Sound))
        .or_else(|| match &track.instrument {
            Some(Instrument::Plugin(p)) => {
                named(&p.class, PLUGINS).map(|i| found(i, Source::Plugin))
            }
            _ => None,
        })
        .or_else(|| named(&track.name, NAMED).map(|i| found(i, Source::Name)))
        .or_else(|| match &track.instrument {
            Some(Instrument::Drums) => Some(found("drums", Source::Kind)),
            Some(Instrument::Synth | Instrument::Plugin(_)) => {
                by_notes(track).map(|i| found(i, Source::Notes))
            }
            _ => None,
        })
        .or_else(|| named(&track.name, NAMED_LOOSELY).map(|i| found(i, Source::Name)))
        .unwrap_or_else(|| {
            found(
                match &track.instrument {
                    None => "audio",
                    Some(Instrument::Sampler(_)) => "sampler",
                    Some(Instrument::Drums) => "drums",
                    _ => "synth",
                },
                Source::Kind,
            )
        });
    match track.icon.as_deref().and_then(icon) {
        // A mark (a star) says nothing of what it plays: its colour still does.
        Some(i) if i.family == Family::Other => Look {
            icon: i.id,
            source: Source::Chosen,
            ..inferred
        },
        Some(i) => Look {
            icon: i.id,
            family: i.family,
            source: Source::Chosen,
        },
        None => inferred,
    }
}

/// A colour as a project keeps it, "#d97757", read.
pub fn parse_color(s: &str) -> Option<u32> {
    let hex = s.strip_prefix('#')?;
    (hex.len() == 6).then(|| u32::from_str_radix(hex, 16).ok())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Note, Region, Sampler, Zone};

    fn track(name: &str, instrument: Option<Instrument>) -> Track {
        Track {
            name: name.into(),
            instrument,
            ..Default::default()
        }
    }

    #[test]
    fn a_track_looks_like_what_it_plays() {
        let none = |_: &Path| None;
        // A sampler of drum hits is a kit, whatever it's called.
        let kit = track(
            "Untitled",
            Some(Instrument::Sampler(Sampler {
                zones: ["k.wav", "s.wav", "h.wav"]
                    .iter()
                    .map(|f| Zone {
                        file: f.into(),
                        ..Default::default()
                    })
                    .collect(),
            })),
        );
        let heard = |p: &Path| match p.to_str() {
            Some("k.wav") => Some(Category::Kick),
            Some("s.wav") => Some(Category::Snare),
            _ => Some(Category::HatClosed),
        };
        let l = look(&kit, &heard);
        assert_eq!(
            (l.icon, l.family, l.source),
            ("drums", Family::Drums, Source::Sound)
        );
        // Audio: what its regions hold, over what it's called ("Chords" of sung pads is vocals).
        let mut sung = track("Chords", None);
        sung.regions.push(Region {
            length: 3840,
            content: Content::Audio {
                file: "pad.wav".into(),
                offset: 0,
                stretch: 1.0,
                flex: Default::default(),
                follow: false,
                formants: false,
                warps: Vec::new(),
            },
            ..Default::default()
        });
        let vocal = |_: &Path| Some(Category::Vocal);
        assert_eq!(look(&sung, &vocal).icon, "vocals");
        // Its name, within the family its sound says, is more exact.
        sung.name = "Vocal Chop".into();
        assert_eq!(look(&sung, &vocal).icon, "chop");
        sung.name = "Chords".into();
        assert_eq!(look(&sung, &none).icon, "chords");
        // A synth playing low single notes is bass; its name, when it says, comes first.
        let mut low = track("Synth 2", Some(Instrument::Synth));
        low.regions.push(Region {
            length: 3840,
            content: Content::Midi {
                notes: vec![Note {
                    start: 0,
                    length: 960,
                    pitch: 36,
                    velocity: 100,
                }],
                controls: Vec::new(),
            },
            ..Default::default()
        });
        let l = look(&low, &none);
        assert_eq!((l.icon, l.source), ("bass", Source::Notes));
        low.name = "Lead Vox".into();
        assert_eq!(look(&low, &none).icon, "vocals");
        // Chosen wins; a mark keeps the family its sound says.
        low.icon = Some("strings".into());
        assert_eq!(look(&low, &none).family, Family::Strings);
        low.icon = Some("star".into());
        let l = look(&low, &none);
        assert_eq!((l.icon, l.family), ("star", Family::Vocals));
    }

    #[test]
    fn every_icon_is_findable_and_colours_read() {
        assert!(ICONS.iter().all(|i| icon(i.id).is_some()));
        assert_eq!(parse_color("#d97757"), Some(0xd97757));
        assert_eq!(parse_color("d97757"), None);
        assert_eq!(named("Kick 02", NAMED), Some("kick"));
        assert_eq!(named("Kickstart", NAMED), None);
        assert_eq!(named("Serum 2", PLUGINS), Some("synth"));
    }
}
