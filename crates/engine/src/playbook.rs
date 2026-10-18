//! What a genre is, for an agent that knows the words but can't hear: its tempo and swing, the
//! parts it's made of and the sounds they take, how its songs are laid out, and where its mixes
//! sit. Mix targets are measured from released songs (`analysis::Spectrum`, dB of the total).

use serde::Serialize;

/// A part a genre's songs have.
#[derive(Serialize, Clone, Copy, Debug)]
pub struct Part {
    /// "drums", "perc", "bass", "chords", "pad", "lead", "counter", "arp", "fx", "vocals",
    /// "texture".
    pub role: &'static str,
    /// What to look for: words for `samples_find`, plugin presets and Surge XT's patch folders.
    pub sound: &'static str,
    /// How to write it with tempo's commands.
    pub write: &'static str,
}

/// A section of a song: how long, which parts play, and how hard (1 to 5).
#[derive(Serialize, Clone, Copy, Debug)]
pub struct Section {
    pub name: &'static str,
    pub bars: u64,
    pub parts: &'static [&'static str],
    pub energy: u8,
}

#[derive(Serialize, Clone, Copy, Debug)]
pub struct Playbook {
    pub id: &'static str,
    pub name: &'static str,
    /// The tempos it's made at, bpm.
    pub tempo: (f64, f64),
    /// Its keys are mostly minor.
    pub minor: bool,
    /// `write_drums` style, and the swing it has (50 straight).
    pub groove: &'static str,
    pub swing: f32,
    /// Words `suggest_progression` takes for it.
    pub harmony: &'static str,
    pub parts: &'static [Part],
    pub sections: &'static [Section],
    /// Integrated loudness of a finished master, LUFS.
    pub lufs: f32,
    /// Sub, bass, low mid, mid, presence and air of a finished mix, dB of the total.
    pub spectrum: [f32; 6],
    /// What makes it sound like itself, and the mistakes that make it sound amateur.
    pub notes: &'static [&'static str],
}

const DRUMS: &str = "drums";

macro_rules! part {
    ($role:expr, $sound:expr, $write:expr) => {
        Part {
            role: $role,
            sound: $sound,
            write: $write,
        }
    };
}

macro_rules! section {
    ($name:expr, $bars:expr, $energy:expr, [$($p:expr),*]) => {
        Section { name: $name, bars: $bars, parts: &[$($p),*], energy: $energy }
    };
}

pub const PLAYBOOKS: &[Playbook] = &[
    Playbook {
        id: "lofi",
        name: "Lo-fi hip-hop",
        tempo: (70.0, 90.0),
        minor: true,
        groove: "lofi",
        swing: 60.0,
        harmony: "lofi",
        parts: &[
            part!(
                DRUMS,
                "dusty boom bap kit, soft kick, brushy snare, vinyl",
                "write_drums lofi, fill_every 8; velocities soft"
            ),
            part!(
                "chords",
                "rhodes, electric piano, felt piano, jazz guitar",
                "write_chords voicing open, rhythm push; 7ths and 9ths"
            ),
            part!(
                "bass",
                "upright bass, warm sub, muted electric bass",
                "write_bass syncopated or roots, register 28-48"
            ),
            part!(
                "lead",
                "muted trumpet, flute, soft bell, vocal chop",
                "write_melody hook, density 0.4; sparse, behind the beat"
            ),
            part!(
                "texture",
                "vinyl crackle, rain, tape hiss, room tone",
                "a long audio region under everything, -24 dB"
            ),
        ],
        sections: &[
            section!("Intro", 4, 1, ["chords", "texture"]),
            section!("A", 16, 3, [DRUMS, "chords", "bass", "texture"]),
            section!("B", 16, 4, [DRUMS, "chords", "bass", "lead", "texture"]),
            section!("Break", 4, 2, ["chords", "lead", "texture"]),
            section!("A2", 16, 4, [DRUMS, "chords", "bass", "lead", "texture"]),
            section!("Outro", 4, 1, ["chords", "texture"]),
        ],
        lufs: -13.0,
        spectrum: [-6.0, -4.0, -6.0, -13.5, -20.0, -25.0],
        notes: &[
            "Dark and warm: roll off the top (high shelf -3 to -6 dB at 8 kHz) and saturate the chords.",
            "Swing 58-62 and soft hats; nothing quantized hard.",
            "Chords carry it: 9ths and 11ths, pushed an eighth early.",
            "Mono below 120 Hz; no huge sub.",
        ],
    },
    Playbook {
        id: "boom_bap",
        name: "Boom bap",
        tempo: (84.0, 96.0),
        minor: true,
        groove: "boom_bap",
        swing: 58.0,
        harmony: "boom_bap",
        parts: &[
            part!(
                DRUMS,
                "punchy breakbeat kit, crunchy snare, hard kick",
                "write_drums boom_bap, fill_every 8"
            ),
            part!(
                "chords",
                "dusty piano, jazz chops, horns, soul sample",
                "write_chords rhythm stabs or comp"
            ),
            part!(
                "bass",
                "upright, sub bass, electric bass",
                "write_bass roots or syncopated"
            ),
            part!("lead", "horn stab, vocal chop, flute", "write_melody riff"),
            part!("fx", "scratch, vinyl stop", "transition at section starts"),
        ],
        sections: &[
            section!("Intro", 4, 2, ["chords"]),
            section!("Verse", 16, 3, [DRUMS, "chords", "bass"]),
            section!("Hook", 8, 4, [DRUMS, "chords", "bass", "lead"]),
            section!("Verse 2", 16, 3, [DRUMS, "chords", "bass"]),
            section!("Hook 2", 8, 5, [DRUMS, "chords", "bass", "lead", "fx"]),
            section!("Outro", 4, 2, ["chords", "bass"]),
        ],
        lufs: -10.0,
        spectrum: [-6.0, -3.5, -6.5, -13.0, -18.0, -22.0],
        notes: &[
            "The kick and snare are loud and dry; the snare cracks around 200 Hz and 5 kHz.",
            "Leave a hole in the mids (1-3 kHz) for a rapper.",
            "Loop-based: change it by taking parts away, not adding new ones.",
        ],
    },
    Playbook {
        id: "trap",
        name: "Trap",
        tempo: (130.0, 160.0),
        minor: true,
        groove: "trap",
        swing: 50.0,
        harmony: "trap dark",
        parts: &[
            part!(
                DRUMS,
                "808 trap kit, crisp clap, hard kick, rolling hats",
                "write_drums trap (hats roll), fill_every 8"
            ),
            part!(
                "bass",
                "808, long 808 with glide, distorted 808",
                "write_bass kick (follows the kick), register 26-43; saturate it"
            ),
            part!(
                "lead",
                "bell, pluck, dark piano, flute, choir",
                "write_melody riff or hook in a high register, density 0.6"
            ),
            part!(
                "pad",
                "dark pad, strings, choir",
                "write_chords voicing spread, rhythm held, -6 dB under the lead"
            ),
            part!(
                "fx",
                "riser, reverse cymbal, impact",
                "transition riser into the hook, impact on its downbeat"
            ),
        ],
        sections: &[
            section!("Intro", 8, 2, ["lead", "pad"]),
            section!("Hook", 16, 5, [DRUMS, "bass", "lead", "pad", "fx"]),
            section!("Verse", 16, 3, [DRUMS, "bass", "lead"]),
            section!("Hook 2", 16, 5, [DRUMS, "bass", "lead", "pad", "fx"]),
            section!("Bridge", 8, 2, ["lead", "pad"]),
            section!("Hook 3", 16, 5, [DRUMS, "bass", "lead", "pad"]),
            section!("Outro", 4, 1, ["lead"]),
        ],
        lufs: -8.5,
        spectrum: [-2.0, -7.0, -9.5, -14.5, -18.0, -20.0],
        notes: &[
            "The 808 is the bass: its root on the song's chord, on the kick, held to the next hit. Never a kick and an 808 a few ticks apart.",
            "Hats at half time feel: eighths with 32nd rolls at bar ends.",
            "The melody is a short, dark loop: 2 bars, minor, in a high register clear of the 808.",
            "Big sub, mono; drums and 808 loudest, melody under them.",
        ],
    },
    Playbook {
        id: "drill",
        name: "UK / NY drill",
        tempo: (138.0, 146.0),
        minor: true,
        groove: "drill",
        swing: 50.0,
        harmony: "drill dark tense",
        parts: &[
            part!(
                DRUMS,
                "drill kit, sliding 808, rim, crisp snare",
                "write_drums drill"
            ),
            part!(
                "bass",
                "sliding 808, glide 808",
                "write_bass kick, register 26-43, notes that slide between roots"
            ),
            part!(
                "lead",
                "dark piano, strings, choir, bell",
                "write_melody riff, minor or phrygian (bII)"
            ),
            part!("pad", "dark strings, choir", "write_chords voicing spread"),
        ],
        sections: &[
            section!("Intro", 8, 2, ["lead", "pad"]),
            section!("Hook", 16, 5, [DRUMS, "bass", "lead", "pad"]),
            section!("Verse", 16, 4, [DRUMS, "bass", "lead"]),
            section!("Hook 2", 16, 5, [DRUMS, "bass", "lead", "pad"]),
            section!("Outro", 4, 1, ["lead"]),
        ],
        lufs: -8.5,
        spectrum: [-2.0, -7.0, -9.5, -14.5, -18.0, -20.0],
        notes: &[
            "The snare lands late (on the third beat's back half), the kick syncopated: it shouldn't sound like trap.",
            "Slides between 808 notes are the genre: glide on.",
        ],
    },
    Playbook {
        id: "rnb",
        name: "R&B",
        tempo: (60.0, 100.0),
        minor: false,
        groove: "rnb",
        swing: 60.0,
        harmony: "rnb smooth",
        parts: &[
            part!(
                DRUMS,
                "R&B kit, soft snap, rim, tight kick",
                "write_drums rnb"
            ),
            part!(
                "chords",
                "rhodes, wurlitzer, soft synth keys, guitar",
                "write_chords voicing open, rhythm push; maj9, m9, 13"
            ),
            part!("bass", "synth bass, sub, 808 soft", "write_bass syncopated"),
            part!(
                "pad",
                "warm pad, strings",
                "write_chords voicing spread, held, quiet"
            ),
            part!(
                "lead",
                "vocal, vocal chop, synth lead",
                "write_melody hook, density 0.6"
            ),
            part!("counter", "guitar, bells, strings", "write_melody counter"),
        ],
        sections: &[
            section!("Intro", 4, 2, ["chords", "pad"]),
            section!("Verse", 16, 3, [DRUMS, "chords", "bass"]),
            section!("Pre", 8, 3, [DRUMS, "chords", "bass", "pad"]),
            section!(
                "Chorus",
                16,
                5,
                [DRUMS, "chords", "bass", "pad", "lead", "counter"]
            ),
            section!("Verse 2", 16, 3, [DRUMS, "chords", "bass", "counter"]),
            section!(
                "Chorus 2",
                16,
                5,
                [DRUMS, "chords", "bass", "pad", "lead", "counter"]
            ),
            section!("Outro", 8, 2, ["chords", "pad"]),
        ],
        lufs: -10.5,
        spectrum: [-5.0, -4.0, -7.0, -13.0, -17.5, -20.0],
        notes: &[
            "Extended chords everywhere; voice-leading smooth (tempo voices them so).",
            "Laid back: swing and the snare a touch late.",
            "Space: the vocal (or lead) owns 1-5 kHz.",
        ],
    },
    Playbook {
        id: "neo_soul",
        name: "Neo-soul",
        tempo: (70.0, 95.0),
        minor: false,
        groove: "neo_soul",
        swing: 62.0,
        harmony: "neo_soul",
        parts: &[
            part!(
                DRUMS,
                "live kit, rim, brushes, dusty",
                "write_drums neo_soul"
            ),
            part!(
                "chords",
                "rhodes, wurlitzer, organ",
                "write_chords voicing rootless or open, rhythm comp"
            ),
            part!(
                "bass",
                "electric bass, synth bass",
                "write_bass syncopated or walking"
            ),
            part!("counter", "guitar, horns", "write_melody counter"),
            part!("lead", "vocal, flute, synth lead", "write_melody hook"),
        ],
        sections: &[
            section!("Intro", 4, 2, ["chords"]),
            section!("Verse", 16, 3, [DRUMS, "chords", "bass"]),
            section!(
                "Chorus",
                16,
                4,
                [DRUMS, "chords", "bass", "lead", "counter"]
            ),
            section!("Verse 2", 16, 3, [DRUMS, "chords", "bass", "counter"]),
            section!(
                "Chorus 2",
                16,
                5,
                [DRUMS, "chords", "bass", "lead", "counter"]
            ),
            section!("Outro", 8, 2, ["chords", "bass"]),
        ],
        lufs: -11.0,
        spectrum: [-6.0, -4.0, -6.0, -13.0, -17.5, -21.0],
        notes: &[
            "Drunk feel: heavy swing (60-65), ghost notes on the snare.",
            "ii-V's, 9ths, 11ths, 13ths; chromatic approach chords.",
        ],
    },
    Playbook {
        id: "pop",
        name: "Pop",
        tempo: (95.0, 128.0),
        minor: false,
        groove: "pop",
        swing: 50.0,
        harmony: "pop",
        parts: &[
            part!(
                DRUMS,
                "pop kit, big clap, punchy kick",
                "write_drums pop, fill_every 8"
            ),
            part!(
                "chords",
                "piano, guitar, synth keys",
                "write_chords rhythm pulse or push"
            ),
            part!(
                "bass",
                "synth bass, electric bass",
                "write_bass octaves or roots"
            ),
            part!("pad", "lush pad, strings", "write_chords voicing spread"),
            part!(
                "lead",
                "vocal, synth lead, pluck",
                "write_melody hook, the chorus a step higher"
            ),
            part!(
                "fx",
                "riser, impact, reverse",
                "transition into each chorus"
            ),
        ],
        sections: &[
            section!("Intro", 4, 2, ["chords"]),
            section!("Verse", 8, 2, [DRUMS, "chords", "bass"]),
            section!("Pre", 8, 3, [DRUMS, "chords", "bass", "pad"]),
            section!(
                "Chorus",
                8,
                5,
                [DRUMS, "chords", "bass", "pad", "lead", "fx"]
            ),
            section!("Verse 2", 8, 3, [DRUMS, "chords", "bass", "lead"]),
            section!("Pre 2", 8, 3, [DRUMS, "chords", "bass", "pad"]),
            section!(
                "Chorus 2",
                8,
                5,
                [DRUMS, "chords", "bass", "pad", "lead", "fx"]
            ),
            section!("Bridge", 8, 2, ["chords", "pad"]),
            section!(
                "Chorus 3",
                16,
                5,
                [DRUMS, "chords", "bass", "pad", "lead", "fx"]
            ),
            section!("Outro", 4, 1, ["chords"]),
        ],
        lufs: -9.0,
        spectrum: [-5.5, -4.5, -7.5, -13.0, -16.0, -18.0],
        notes: &[
            "The chorus is the loudest, widest, highest moment; get there in under a minute.",
            "Bright and clear on top, tight low end.",
        ],
    },
    Playbook {
        id: "house",
        name: "House",
        tempo: (118.0, 128.0),
        minor: true,
        groove: "house",
        swing: 54.0,
        harmony: "house",
        parts: &[
            part!(
                DRUMS,
                "909 house kit, open hat, clap, shaker",
                "write_drums house"
            ),
            part!(
                "chords",
                "house stab, organ, piano, M1 piano",
                "write_chords rhythm offbeat or stabs, voicing close"
            ),
            part!(
                "bass",
                "house bass, sub, organ bass",
                "write_bass offbeat or octaves"
            ),
            part!("pad", "warm pad, strings", "write_chords voicing spread"),
            part!("lead", "vocal chop, pluck, synth lead", "write_melody riff"),
            part!(
                "fx",
                "riser, white noise sweep, crash",
                "transition riser before the drop"
            ),
        ],
        sections: &[
            section!("Intro", 16, 2, [DRUMS]),
            section!("Build", 16, 3, [DRUMS, "bass", "chords"]),
            section!("Drop", 32, 5, [DRUMS, "bass", "chords", "lead"]),
            section!("Breakdown", 16, 2, ["pad", "chords", "fx"]),
            section!("Drop 2", 32, 5, [DRUMS, "bass", "chords", "lead", "pad"]),
            section!("Outro", 16, 2, [DRUMS]),
        ],
        lufs: -8.5,
        spectrum: [-4.0, -4.0, -8.0, -13.5, -16.5, -18.5],
        notes: &[
            "Four on the floor; the bass between kicks (offbeat), never on them.",
            "Long DJ-friendly intro and outro of drums only.",
            "Sidechain the bass and pads to the kick.",
        ],
    },
    Playbook {
        id: "deep_house",
        name: "Deep house",
        tempo: (118.0, 124.0),
        minor: true,
        groove: "house",
        swing: 56.0,
        harmony: "deep_house",
        parts: &[
            part!(
                DRUMS,
                "deep house kit, soft kick, shaker, rim",
                "write_drums house"
            ),
            part!(
                "chords",
                "deep chord stab, rhodes, organ",
                "write_chords voicing open, rhythm offbeat; m9, m11"
            ),
            part!(
                "bass",
                "deep sub bass, rubbery bass",
                "write_bass syncopated or offbeat"
            ),
            part!("pad", "warm pad, atmosphere", "write_chords voicing spread"),
            part!(
                "lead",
                "vocal chop, soft pluck",
                "write_melody riff, density 0.4"
            ),
        ],
        sections: &[
            section!("Intro", 16, 2, [DRUMS]),
            section!("Groove", 32, 3, [DRUMS, "bass", "chords"]),
            section!("Break", 16, 2, ["pad", "chords"]),
            section!("Groove 2", 32, 4, [DRUMS, "bass", "chords", "lead", "pad"]),
            section!("Outro", 16, 2, [DRUMS, "bass"]),
        ],
        lufs: -9.5,
        spectrum: [-4.0, -4.0, -7.0, -13.5, -18.0, -21.0],
        notes: &[
            "Warm, rounded, never harsh; minor 9 and 11 chords.",
            "Groove over drops: change by filtering and adding/removing parts.",
        ],
    },
    Playbook {
        id: "techno",
        name: "Techno",
        tempo: (125.0, 140.0),
        minor: true,
        groove: "techno",
        swing: 50.0,
        harmony: "techno",
        parts: &[
            part!(
                DRUMS,
                "techno kit, rumble kick, rim, open hat",
                "write_drums techno"
            ),
            part!("bass", "rumble, acid bass, sub", "write_bass offbeat"),
            part!("lead", "acid, sequence, stab", "write_melody riff or arp"),
            part!("texture", "atmosphere, noise, drone", "a long audio region"),
        ],
        sections: &[
            section!("Intro", 16, 2, [DRUMS]),
            section!("Build", 16, 3, [DRUMS, "bass"]),
            section!("Peak", 32, 5, [DRUMS, "bass", "lead", "texture"]),
            section!("Break", 16, 2, ["texture", "lead"]),
            section!("Peak 2", 32, 5, [DRUMS, "bass", "lead", "texture"]),
            section!("Outro", 16, 2, [DRUMS]),
        ],
        lufs: -8.0,
        spectrum: [-3.5, -4.0, -8.5, -14.0, -17.0, -19.0],
        notes: &[
            "One idea, changed slowly: filter, decay and layers over 16-bar steps.",
            "The kick is everything; nothing else under 100 Hz on its beats.",
        ],
    },
    Playbook {
        id: "afrobeats",
        name: "Afrobeats",
        tempo: (98.0, 115.0),
        minor: false,
        groove: "afrobeats",
        swing: 55.0,
        harmony: "afrobeats",
        parts: &[
            part!(
                DRUMS,
                "afrobeats kit, shaker, conga, rim, talking drum",
                "write_drums afrobeats"
            ),
            part!(
                "chords",
                "guitar, rhodes, synth keys, marimba",
                "write_chords rhythm stabs or comp"
            ),
            part!("bass", "sub, round bass", "write_bass syncopated"),
            part!("lead", "vocal, sax, pluck, flute", "write_melody hook"),
            part!(
                "perc",
                "conga, shaker, cowbell",
                "write_drums afrobeats on a percussion kit"
            ),
        ],
        sections: &[
            section!("Intro", 8, 2, ["chords", "perc"]),
            section!("Verse", 16, 3, [DRUMS, "chords", "bass", "perc"]),
            section!("Chorus", 16, 5, [DRUMS, "chords", "bass", "lead", "perc"]),
            section!("Verse 2", 16, 3, [DRUMS, "chords", "bass", "perc"]),
            section!("Chorus 2", 16, 5, [DRUMS, "chords", "bass", "lead", "perc"]),
            section!("Outro", 8, 2, ["chords", "perc"]),
        ],
        lufs: -9.0,
        spectrum: [-4.5, -4.0, -7.5, -13.5, -18.0, -20.5],
        notes: &[
            "The percussion's interplay is the song: syncopated, layered, swung a little.",
            "Bright guitars or keys in small stabs; space for the vocal.",
        ],
    },
    Playbook {
        id: "amapiano",
        name: "Amapiano",
        tempo: (110.0, 116.0),
        minor: true,
        groove: "amapiano",
        swing: 54.0,
        harmony: "amapiano",
        parts: &[
            part!(
                DRUMS,
                "amapiano kit, shaker, soft kick, clap, rim",
                "write_drums amapiano"
            ),
            part!(
                "bass",
                "log drum, sub",
                "write_bass syncopated or kick, register 30-50; the log drum is the lead bass"
            ),
            part!(
                "chords",
                "jazzy piano, rhodes, organ",
                "write_chords voicing open, rhythm comp; maj9, m9"
            ),
            part!("pad", "warm pad, strings", "write_chords voicing spread"),
            part!(
                "lead",
                "vocal, sax, flute, whistle",
                "write_melody hook, density 0.5"
            ),
            part!("perc", "shaker, conga, wood block", "pattern regions"),
        ],
        sections: &[
            section!("Intro", 16, 2, ["chords", "perc"]),
            section!("Groove", 32, 4, [DRUMS, "chords", "bass", "perc"]),
            section!("Break", 16, 2, ["chords", "pad"]),
            section!(
                "Drop",
                32,
                5,
                [DRUMS, "chords", "bass", "pad", "lead", "perc"]
            ),
            section!("Outro", 16, 2, [DRUMS, "chords"]),
        ],
        lufs: -8.5,
        spectrum: [-1.5, -8.0, -10.0, -15.5, -19.0, -19.5],
        notes: &[
            "The log drum: deep, pitched, syncopated, loud; the sub is most of the mix.",
            "Jazzy piano chords, long songs, slow builds.",
        ],
    },
    Playbook {
        id: "dnb",
        name: "Drum and bass",
        tempo: (170.0, 176.0),
        minor: true,
        groove: "dnb",
        swing: 50.0,
        harmony: "dnb liquid",
        parts: &[
            part!(
                DRUMS,
                "dnb break, tight snare, punchy kick",
                "write_drums dnb, fill_every 8"
            ),
            part!(
                "bass",
                "reese, sub, neuro bass",
                "write_bass roots or syncopated"
            ),
            part!(
                "pad",
                "lush pad, strings, atmosphere",
                "write_chords voicing spread"
            ),
            part!("lead", "vocal, pluck, piano", "write_melody hook"),
            part!("fx", "riser, impact", "transition into the drop"),
        ],
        sections: &[
            section!("Intro", 16, 2, ["pad"]),
            section!("Build", 16, 3, [DRUMS, "pad", "lead"]),
            section!("Drop", 32, 5, [DRUMS, "bass", "pad", "fx"]),
            section!("Break", 16, 2, ["pad", "lead"]),
            section!("Drop 2", 32, 5, [DRUMS, "bass", "pad", "lead", "fx"]),
            section!("Outro", 16, 2, [DRUMS]),
        ],
        lufs: -8.0,
        spectrum: [-3.5, -4.5, -8.5, -13.5, -16.5, -18.5],
        notes: &[
            "The snare on 2 and 4 at 174 is the backbone; the bass carries the drop.",
            "Sub mono, the reese's mids wide.",
        ],
    },
    Playbook {
        id: "reggaeton",
        name: "Reggaeton",
        tempo: (88.0, 100.0),
        minor: true,
        groove: "reggaeton",
        swing: 50.0,
        harmony: "reggaeton",
        parts: &[
            part!(
                DRUMS,
                "dembow kit, kick, snare, rim, hat",
                "write_drums reggaeton"
            ),
            part!("bass", "808, sub", "write_bass kick"),
            part!(
                "chords",
                "pluck, guitar, synth keys",
                "write_chords rhythm stabs"
            ),
            part!("lead", "vocal, pluck, lead", "write_melody hook"),
        ],
        sections: &[
            section!("Intro", 4, 2, ["chords"]),
            section!("Verse", 16, 3, [DRUMS, "bass", "chords"]),
            section!("Chorus", 8, 5, [DRUMS, "bass", "chords", "lead"]),
            section!("Verse 2", 16, 3, [DRUMS, "bass", "chords"]),
            section!("Chorus 2", 8, 5, [DRUMS, "bass", "chords", "lead"]),
            section!("Outro", 4, 1, ["chords"]),
        ],
        lufs: -8.5,
        spectrum: [-3.5, -4.0, -8.0, -13.5, -17.0, -19.5],
        notes: &[
            "The dembow (3+3+2) never stops in a verse or chorus.",
            "Big, simple 808 on the kick.",
        ],
    },
    Playbook {
        id: "edm",
        name: "EDM / future bass",
        tempo: (128.0, 150.0),
        minor: false,
        groove: "pop",
        swing: 50.0,
        harmony: "future_bass emotional",
        parts: &[
            part!(
                DRUMS,
                "edm kit, big clap, punchy kick, snare roll",
                "write_drums pop, fill_every 8"
            ),
            part!(
                "chords",
                "supersaw, future bass chords, wobble chords",
                "write_chords voicing spread, rhythm pulse"
            ),
            part!("bass", "sub, saw bass", "write_bass octaves or roots"),
            part!("lead", "pluck, synth lead, vocal chop", "write_melody hook"),
            part!(
                "fx",
                "riser, impact, white noise",
                "transition riser into each drop, impact on it"
            ),
        ],
        sections: &[
            section!("Intro", 8, 2, ["chords"]),
            section!("Verse", 16, 3, [DRUMS, "chords", "bass"]),
            section!("Build", 8, 4, [DRUMS, "chords", "lead", "fx"]),
            section!("Drop", 16, 5, [DRUMS, "chords", "bass", "lead", "fx"]),
            section!("Break", 8, 2, ["chords"]),
            section!("Build 2", 8, 4, [DRUMS, "chords", "lead", "fx"]),
            section!("Drop 2", 16, 5, [DRUMS, "chords", "bass", "lead", "fx"]),
            section!("Outro", 8, 1, ["chords"]),
        ],
        lufs: -7.5,
        spectrum: [-4.5, -4.5, -8.0, -13.0, -15.5, -17.0],
        notes: &[
            "Builds rise for 8 bars (snare roll, riser, filter opening), then a gap, then the drop.",
            "Wide, bright and loud; sidechain everything to the kick.",
        ],
    },
    Playbook {
        id: "cinematic",
        name: "Cinematic",
        tempo: (60.0, 100.0),
        minor: true,
        groove: "rock",
        swing: 50.0,
        harmony: "cinematic epic",
        parts: &[
            part!(
                "pad",
                "strings, choir, brass, orchestral",
                "write_chords voicing spread, rhythm held"
            ),
            part!("bass", "low strings, contrabass, sub", "write_bass roots"),
            part!(
                "lead",
                "piano, solo strings, horn",
                "write_melody hook, density 0.5"
            ),
            part!(
                "perc",
                "taiko, epic drums, impacts",
                "write_drums rock on an epic kit"
            ),
            part!(
                "fx",
                "riser, impact, braam, reverse",
                "transitions at each section"
            ),
        ],
        sections: &[
            section!("Opening", 8, 1, ["pad"]),
            section!("Theme", 16, 2, ["pad", "lead", "bass"]),
            section!("Build", 16, 3, ["pad", "lead", "bass", "perc"]),
            section!("Climax", 16, 5, ["pad", "lead", "bass", "perc", "fx"]),
            section!("Resolve", 8, 1, ["pad", "lead"]),
        ],
        lufs: -14.0,
        spectrum: [-6.0, -5.0, -6.0, -12.5, -18.0, -21.0],
        notes: &[
            "Dynamics are the point: quiet openings, a loud climax.",
            "Long reverbs; layers added one at a time.",
        ],
    },
    Playbook {
        id: "ambient",
        name: "Ambient",
        tempo: (60.0, 90.0),
        minor: false,
        groove: "lofi",
        swing: 50.0,
        harmony: "dreamy ambient",
        parts: &[
            part!(
                "pad",
                "evolving pad, atmosphere, drone",
                "write_chords voicing spread, held, 2 bars a chord"
            ),
            part!(
                "texture",
                "field recording, noise, granular",
                "a long audio region"
            ),
            part!(
                "lead",
                "bell, piano, soft pluck",
                "write_melody counter or hook with density 0.2"
            ),
            part!("bass", "sub drone", "write_bass roots"),
        ],
        sections: &[
            section!("Open", 16, 1, ["pad", "texture"]),
            section!("Bloom", 32, 3, ["pad", "texture", "lead", "bass"]),
            section!("Fade", 16, 1, ["pad", "texture"]),
        ],
        lufs: -16.0,
        spectrum: [-7.0, -5.5, -5.5, -12.0, -18.0, -20.5],
        notes: &["Slow change and space; long reverbs, no hard transients."],
    },
];

/// The playbook for a genre's id or name ("lo-fi", "UK drill"), the nearest it knows.
pub fn find(genre: &str) -> Option<&'static Playbook> {
    let g = genre.trim().to_lowercase().replace([' ', '-'], "_");
    let alias = match g.as_str() {
        "lo_fi" | "lofi_hip_hop" | "chillhop" => "lofi",
        "hip_hop" | "hiphop" | "boombap" => "boom_bap",
        "uk_drill" | "ny_drill" => "drill",
        "r&b" | "randb" | "r_n_b" => "rnb",
        "soul" | "neosoul" => "neo_soul",
        "afro_house" | "garage" => "deep_house",
        "afrobeat" | "afropop" | "afro" => "afrobeats",
        "piano" | "yanos" => "amapiano",
        "drum_and_bass" | "drum_n_bass" | "jungle" | "liquid" => "dnb",
        "future_bass" | "electronic" | "dance" => "edm",
        "film" | "score" | "orchestral" | "trailer" => "cinematic",
        "chill" | "drone" => "ambient",
        "latin" | "dembow" => "reggaeton",
        other => other,
    };
    PLAYBOOKS.iter().find(|p| p.id == alias)
}

/// Every genre there's a playbook for, by id.
pub fn ids() -> Vec<&'static str> {
    PLAYBOOKS.iter().map(|p| p.id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_playbook_is_whole() {
        for p in PLAYBOOKS {
            assert_eq!(crate::theory::groove(p.groove).id, p.groove, "{}", p.id);
            assert!(p.tempo.0 < p.tempo.1);
            let roles: Vec<&str> = p.parts.iter().map(|x| x.role).collect();
            // Every section's parts are among the genre's parts.
            for s in p.sections {
                for part in s.parts {
                    assert!(roles.contains(part), "{}: {} in {}", p.id, part, s.name);
                }
                assert!((1..=5).contains(&s.energy) && s.bars % 4 == 0);
            }
            // The harmony words find progressions of its kind.
            let key = crate::music::Key {
                root: 9,
                minor: p.minor,
            };
            let prog = crate::theory::suggest(&key, p.harmony, 0);
            assert_eq!(prog.minor, p.minor);
        }
        assert_eq!(find("Lo-Fi").unwrap().id, "lofi");
        assert_eq!(find("UK drill").unwrap().id, "drill");
        assert!(find("polka").is_none());
    }
}
