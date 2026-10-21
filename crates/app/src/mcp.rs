//! `pina mcp`: an MCP server on stdin and stdout, so agents that speak MCP can make music with
//! Pina Studio. Its tools are the command line's, and edits reach the app when it has the project open.

use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
};

use pina_engine::commands::{Command, DESCRIBE};
use serde_json::{Value, json};

use crate::{cli, listen, produce, server::Request};

const INSTRUCTIONS: &str = "Pina Studio is a DAW. A project is a folder; call `describe` first for the \
commands and how time and levels work, `show` to see a project, `edit` to change it one command at \
a time, and `analyze` to hear it as numbers (loudness, peaks, balance). To make a song that sounds \
finished, work as a producer: `playbook` for the genre, `theory` for chords, write parts with the \
write_* commands (Pina Studio gets the notes right), `sounds` for real instruments, `critique` after \
every step and fix what it says, `arrange` and `transition` for the song's shape, `mix` and \
`master` to finish. `critique` is your ears: trust it over your guesses. The person may have \
picked sounds, instruments and effects for the song (`palette`): build with those first, and with \
only those if they say so.";

fn tools() -> Value {
    let project = json!({ "type": "string", "description": "The project folder." });
    json!([
        {
            "name": "describe",
            "description": "The command reference: every edit, with examples, and how ticks, levels and notes work.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "show",
            "description": "A project's tracks, regions, notes, mixer, sends and auxes, as JSON.",
            "inputSchema": { "type": "object", "properties": { "project": project }, "required": ["project"] }
        },
        {
            "name": "edit",
            "description": "Applies one command (see `describe`) to a project, live in the app if it's open there. Makes the project if it doesn't exist.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "command": { "type": "object", "description": "A command, e.g. {\"op\": \"add_track\", \"name\": \"Bass\", \"instrument\": \"synth\"}." }
                },
                "required": ["project", "command"]
            }
        },
        {
            "name": "analyze",
            "description": "Measures a project (the mix, its sections, then each track alone) or an audio file (WAV, AIFF, FLAC, MP3): peak, RMS, LUFS, crest, clipping, spectrum, stereo, the notes sounding and their key. `from_bar` and `to_bar` (from 1, both in) measure just those bars of a project.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "A project folder or an audio file." },
                    "from_bar": { "type": "integer", "minimum": 1 },
                    "to_bar": { "type": "integer", "minimum": 1 }
                },
                "required": ["path"]
            }
        },
        {
            "name": "listen",
            "description": "What each track sounds like on its own (or `track`): rendered alone over `from_bar`-`to_bar` (eight bars from where it starts, if not given) and heard by the sample library's model: `sounds_like` (its guess at what the sound is: pad 70% · strings 20%), `character` (dark, punchy, wide, distorted, lo-fi…), attack and decay, brightness, the spectrum, width, loudness and the key it sounds in; and `issues`: a default patch, a sound unlike its name or part (strings that sound like a synth), a fader doing the sound's job, rumble under a part, a wide low end, a loop in another key. Use after choosing or changing a sound, before mixing. Measurements and a model's guesses, not listening.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": { "type": "string" },
                    "track": { "type": "integer", "minimum": 0 },
                    "from_bar": { "type": "integer", "minimum": 1 },
                    "to_bar": { "type": "integer", "minimum": 1 }
                },
                "required": ["project"]
            }
        },
        {
            "name": "render",
            "description": "Bounces a project to a WAV file; `from_bar` and `to_bar` (from 1, both in) bounce just those bars.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "out": { "type": "string", "description": "The .wav to write." },
                    "from_bar": { "type": "integer", "minimum": 1 },
                    "to_bar": { "type": "integer", "minimum": 1 }
                },
                "required": ["project", "out"]
            }
        },
        {
            "name": "bounce",
            "description": "Bounces the song to files, as a person would from the Bounce sheet: the mix, each track as a stem (`what`: \"stems\", `tracks` to choose; through the master with `master`), or each marked section (`what`: \"sections\"); in `formats` (wav, aiff, flac, mp3, aac, alac, midi; several at once), `bits` (16, 24, 32), `rate`; levelled to a loudness (`lufs`, under `ceiling` dBFS, -1 by default) or a `peak`; with its reverb tail (`tail`, on by default); `from_bar`/`to_bar` for part of it. Into `out` (the song's Bounces folder by default). Answers with each file and its loudness and peak.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "what": { "type": "string", "enum": ["mix", "stems", "sections"] },
                    "tracks": { "type": "array", "items": { "type": "integer" } },
                    "master": { "type": "boolean" },
                    "formats": { "type": "array", "items": { "type": "string", "enum": ["wav", "aiff", "flac", "mp3", "aac", "alac", "midi"] } },
                    "bits": { "type": "integer", "enum": [16, 24, 32] },
                    "rate": { "type": "integer" },
                    "lufs": { "type": "number" },
                    "ceiling": { "type": "number" },
                    "peak": { "type": "number" },
                    "kbps": { "type": "integer" },
                    "tail": { "type": "boolean" },
                    "from_bar": { "type": "integer", "minimum": 1 },
                    "to_bar": { "type": "integer", "minimum": 1 },
                    "out": { "type": "string" },
                    "name": { "type": "string" }
                },
                "required": ["project"]
            }
        },
        {
            "name": "critique",
            "description": "A producer's notes on the song, worst first, each with where and the command that fixes it: wrong notes and semitone clashes, a bass off its chords, kick and bass flamming, robotic velocities, missing swing, placeholder sounds, a song that never changes or starts with everything, a hook weaker than its verse, no transitions; and (unless `quick`) from a render: clipping, loudness, tonal balance against the genre or reference, low end in the wrong parts, parts too loud for their role, stereo trouble. `quick` skips the render (fast: use it after each edit). `from_bar`/`to_bar` limit the render.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "quick": { "type": "boolean" },
                    "from_bar": { "type": "integer", "minimum": 1 },
                    "to_bar": { "type": "integer", "minimum": 1 }
                },
                "required": ["project"]
            }
        },
        {
            "name": "study",
            "description": "Studies an existing song from its audio file (WAV, AIFF, FLAC, MP3): who it is (tags; online, its year and genre), its tempo and bars, key (and changes), chords beat by beat with numerals, its form (intro, verse, pre-chorus, chorus, bridge, outro, each with its bars, energy, loudness and the chord loop it goes round), its groove (kick, snare and hats on a 16-step grid, swing, syncopation, nearest Pina Studio groove), what plays in it (with confidence), its mix and dynamics overall and by section, the playbook genres it's nearest, and (online, from LRCLIB) its lyrics: each line in its section, rhyme schemes, syllables, hooks, point of view and recurring words. Interpret its theme, story and imagery from the lyrics yourself. `playbook: true` also saves a playbook made from it, to write songs like it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file": { "type": "string" },
                    "online": { "type": "boolean", "description": "Look up its lyrics, year and genre by its title and artist (default true; no audio leaves the Mac)." },
                    "playbook": { "type": "boolean" }
                },
                "required": ["file"]
            }
        },
        {
            "name": "playbook",
            "description": "What a genre is: tempo range, swing, key, its parts with the sounds they take and how to write them, its sections (bars, parts, energy), progressions, mix targets (LUFS and spectrum) and what makes it sound right or amateur, with the workflow. Without `genre`, the genres there are. Playbooks are the person's JSON files (`edit` says where): follow them as written.",
            "inputSchema": { "type": "object", "properties": { "genre": { "type": "string" } } }
        },
        {
            "name": "theory",
            "description": "Chords for a key: its scale, its chords by degree, and progressions from the library that suit `words` (a genre or mood: \"lofi\", \"dark sad\", \"gospel\"), as numerals and as chords in the key, for set_chords.",
            "inputSchema": {
                "type": "object",
                "properties": { "key": { "type": "string", "description": "\"A minor\", \"Eb major\"." }, "words": { "type": "string" } },
                "required": ["key"]
            }
        },
        {
            "name": "reference",
            "description": "Measures a finished song (an audio file) to aim the mix at: loudness, crest, spectrum, width, key. With `project`, makes it the song's reference (critique and master aim at it) and says how the song differs, band by band.",
            "inputSchema": {
                "type": "object",
                "properties": { "file": { "type": "string" }, "project": project },
                "required": ["file"]
            }
        },
        {
            "name": "sounds",
            "description": "Real sounds for a part (`role`: drums, perc, bass, chords, pad, lead, counter, arp, fx, vocals, texture), for `genre`, matching `words` (\"warm rhodes\", \"dark 808\", \"brass\"): every installed instrument's presets that might suit it (Apple's General MIDI synth's 128 instruments among them), each auditioned (played a phrase for the part and heard by the sample library's model) and ranked by how it sounds, not its name: `best` first, each with what was heard (`sounds`), a `fit` 0-1 and the commands that load it; then each instrument's, and samples from the library. With `project` and `track`, the commands are ready for that track. A first ask can take a minute (what's heard is kept); presets not heard yet are heard next time. After loading one, `listen` to it in the song.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "role": { "type": "string" },
                    "genre": { "type": "string" },
                    "words": { "type": "string" },
                    "project": project,
                    "track": { "type": "integer", "minimum": 0 }
                },
                "required": ["role"]
            }
        },
        {
            "name": "mix",
            "description": "Mixes tracks (all with regions, or `track`) by measuring them first: each part's own sound rendered and measured, then its channel set from that: a low cut under its lowest note, mud and harshness cut only as far as it has them (harshness tamed dynamically, spectrally on a lead), air where it lacks it, its low end mono; parts making room for each other (the bass dipping at the kick's own frequency as it hits; in genres that pump, the bass and pads ducking under the kick; chords and pads dipping their presence while the lead plays), compression set from each part's level, a reverb (Dragonfly's if installed, its pre-delay and decay by tempo and genre) and a tempo-synced delay, the drums glued on a bus, then each part's level against the drums. Says what it did and why, track by track. `role` overrides the part; `levels: false` leaves the faders. Then `listen` and `critique`, and `master`.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "track": { "type": "integer", "minimum": 0 },
                    "role": { "type": "string" },
                    "levels": { "type": "boolean" }
                },
                "required": ["project"]
            }
        },
        {
            "name": "master",
            "description": "Masters the song: the master's EQ brings its tonal balance (sub, bass, low mid, mid, presence, air) halfway to the reference's, else the genre's, in broad moves; then a limiter, its gain set and measured again until the song is at `lufs` (the reference's, else the genre's, else -10), peaks under -0.5 dBFS. Mix first.",
            "inputSchema": {
                "type": "object",
                "properties": { "project": project, "lufs": { "type": "number" } },
                "required": ["project"]
            }
        },
        {
            "name": "transport",
            "description": "Drives the app that has the project open: play, stop, locate (to a tick), save, undo, redo, or launch (a track's Live Loops cell for a scene, or stop it without one).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "action": { "type": "string", "enum": ["play", "stop", "locate", "save", "undo", "redo", "launch"] },
                    "tick": { "type": "integer", "minimum": 0 },
                    "track": { "type": "integer", "minimum": 0 },
                    "scene": { "type": "integer", "minimum": 0 }
                },
                "required": ["project", "action"]
            }
        },
        {
            "name": "plugins",
            "description": "The plugins installed (VST3, CLAP, VST2 and Audio Units), each as set_instrument and set_insert take it: format, path, class and (for some formats) uid.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "params",
            "description": "A plugin's parameters (id, name, value 0-1 and as the plugin shows it, default, unit, steps and each step's name), for set_plugin_param and plugin automation, which take a parameter by id or name and a value 0-1 or as text (\"850 Hz\").",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "channel": { "description": "{\"track\": i}, {\"aux\": i} or \"master\"." },
                    "plugin": { "description": "\"instrument\" or {\"insert\": i}." }
                },
                "required": ["project", "channel", "plugin"]
            }
        },
        {
            "name": "presets",
            "description": "A plugin's presets (its own programs, and preset files of its format), for set_preset; or, with `save`, saves its settings as a preset file of that name and says where.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "channel": { "description": "{\"track\": i}, {\"aux\": i} or \"master\"." },
                    "plugin": { "description": "\"instrument\" or {\"insert\": i}." },
                    "save": { "type": "string", "description": "A name to save its settings as." }
                },
                "required": ["project", "channel", "plugin"]
            }
        },
        {
            "name": "samples_find",
            "description": "Finds samples in the library (analysed for kind, category, pitch, key, tempo, envelope, spectrum and traits), best first, each with why it matched and, for a one-shot, a sampler zone to play it in tune. `query` is words (\"dusty keys loop in C minor 95 bpm\", \"punchy dark kick\") or a query object: text, kind (one_shot|loop), categories, traits, key, bpm, tonal, min_seconds, max_seconds, folder, like (a path: sounds like it), aspects ({timbre, pitch, envelope, spectrum} weights for like), with (paths it should go with), limit. `project` fills the key and tempo from a song; with a key, a pitched drum's zone comes tuned onto it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "description": "Words, or a query object." },
                    "project": project
                },
                "required": ["query"]
            }
        },
        {
            "name": "samples_similar",
            "description": "The samples most like one, weighing timbre, pitch, envelope and spectrum as `aspects` says (each 0-1).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "aspects": { "type": "object" },
                    "limit": { "type": "integer", "minimum": 1 }
                },
                "required": ["path"]
            }
        },
        {
            "name": "samples_kit",
            "description": "Builds a kit that goes together (kick, snare, clap, hats, perc, 808, keys by default): each pick fits the key and tempo and the picks before it (no clashes, one sound, one pack where it can), with alternatives. Each drum is judged as its kind (a closed hat tight, not ringing in reverb; a kick without a long tail; a snare that doesn't boom; the 808 a one-shot with a clear note, to play the bassline on), the kit's character kept to the roles it suits (a dark kit's hats stay bright), and with a key, a pitched drum's zone comes tuned onto it (a kick on the root or fifth). `seed` is words or {key, bpm, text, traits, like, folder, roles, alternatives, keep}; `project` fills the key and tempo from a song.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "seed": { "description": "Words, or a kit seed object." },
                    "project": project
                }
            }
        },
        {
            "name": "samples_put",
            "description": "Puts a sample into a song, made to fit it: a loop on an audio track (`track`, else a new one) at `at` ticks, stretched to the song's tempo, moved into its key if a small move fits, looping; a one-shot into a sampler (`track`'s, else a new sampler track) as a zone that plays it in tune (a drum on its note, a pitched sound across the keys from its own note).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "project": project,
                    "track": { "type": "integer", "minimum": 0 },
                    "at": { "type": "integer", "minimum": 0 }
                },
                "required": ["path", "project"]
            }
        },
        {
            "name": "palette",
            "description": "The sounds, instruments and effects the person picked for this song in Pina Studio, for you to make it with: each sound's file and what it is (one-shot or loop, its category, note or key, Pina Studio); each instrument and effect with how to put it in (`use`: a plugin as set_instrument and set_insert take it, or Pina Studio's own command); the part each is for if they said (`role`), and whether it's in the song yet (`in_song`). `only` means they want these and no others of each kind they picked. Check it before choosing sounds, instruments or effects yourself.",
            "inputSchema": {
                "type": "object",
                "properties": { "project": project },
                "required": ["project"]
            }
        },
        {
            "name": "samples_library",
            "description": "The sample library: `status` (its folders and what's in them), `scan` (analyse what's new or changed), `add` or `remove` a folder (`path`), or `info` on a sample (`path`).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["status", "scan", "add", "remove", "info"] },
                    "path": { "type": "string" }
                },
                "required": ["action"]
            }
        }
    ])
}

fn path(args: &Value, key: &str) -> Result<PathBuf, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| format!("`{key}` is required"))
}

fn text<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

/// `from_bar` and `to_bar`, if given (one alone is that bar).
fn bar_range(args: &Value) -> Result<Option<(u64, u64)>, String> {
    let bar = |k: &str| args.get(k).and_then(Value::as_u64);
    Ok(match (bar("from_bar"), bar("to_bar")) {
        (None, None) => None,
        (Some(a), None) => Some((a, a)),
        (None, Some(b)) => Some((1, b)),
        (Some(a), Some(b)) => Some((a, b)),
    })
}

fn call(name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "describe" => Ok(Value::String(DESCRIBE.to_string())),
        "show" => cli::show(&path(args, "project")?),
        "edit" => {
            let command: Command =
                serde_json::from_value(args.get("command").cloned().unwrap_or_default())
                    .map_err(|e| format!("not a command: {e}"))?;
            cli::edit_with(&path(args, "project")?, command)
        }
        "analyze" => cli::analyze(&path(args, "path")?, bar_range(args)?),
        "listen" => listen::listen(
            &path(args, "project")?,
            args.get("track")
                .and_then(Value::as_u64)
                .map(|t| t as usize),
            bar_range(args)?,
        ),
        "render" => cli::render(
            &path(args, "project")?,
            &path(args, "out")?,
            bar_range(args)?,
        ),
        "bounce" => cli::bounce_spec(&path(args, "project")?, args),
        "critique" => produce::critique(
            &path(args, "project")?,
            args.get("quick").and_then(Value::as_bool).unwrap_or(false),
            bar_range(args)?,
        ),
        "study" => {
            let s = crate::study_cli::study(
                &path(args, "file")?,
                args.get("online").and_then(Value::as_bool).unwrap_or(true),
            )?;
            let mut out = crate::study_cli::for_agent(&s);
            if args
                .get("playbook")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                let book = pina_engine::study::to_playbook(&s, &pina_engine::playbook::ids());
                let file = pina_engine::playbook::save(&book)?;
                out["playbook"] = json!({ "id": book.id, "name": book.name, "file": file });
            }
            Ok(out)
        }
        "playbook" => produce::playbook(text(args, "genre")),
        "theory" => produce::theory(
            text(args, "key").ok_or("`key` is required")?,
            text(args, "words").unwrap_or(""),
        ),
        "reference" => {
            produce::reference(&path(args, "file")?, text(args, "project").map(Path::new))
        }
        "sounds" => produce::sounds(
            text(args, "role").ok_or("`role` is required")?,
            text(args, "genre"),
            text(args, "words").unwrap_or(""),
            text(args, "project").map(Path::new),
            args.get("track")
                .and_then(Value::as_u64)
                .map(|t| t as usize),
        ),
        "mix" => produce::mix_role(
            &path(args, "project")?,
            args.get("track")
                .and_then(Value::as_u64)
                .map(|t| t as usize),
            text(args, "role"),
            args.get("levels").and_then(Value::as_bool).unwrap_or(true),
        ),
        "master" => produce::master(
            &path(args, "project")?,
            args.get("lufs").and_then(Value::as_f64).map(|l| l as f32),
        ),
        "transport" => {
            let request = match args.get("action").and_then(Value::as_str) {
                Some("play") => Request::Play,
                Some("stop") => Request::Stop,
                Some("save") => Request::Save,
                Some("undo") => Request::Undo,
                Some("redo") => Request::Redo,
                Some("launch") => Request::Launch {
                    track: args
                        .get("track")
                        .and_then(Value::as_u64)
                        .ok_or("launch needs a `track`")? as usize,
                    scene: args
                        .get("scene")
                        .and_then(Value::as_u64)
                        .map(|s| s as usize),
                },
                Some("locate") => Request::Locate {
                    tick: args
                        .get("tick")
                        .and_then(Value::as_u64)
                        .ok_or("locate needs a `tick`")?,
                },
                _ => {
                    return Err(
                        "`action` is one of play, stop, locate, save, undo, redo, launch".into(),
                    );
                }
            };
            cli::app(&path(args, "project")?, request)
        }
        "plugins" => cli::plugins(),
        "params" => {
            let parse = |key: &str| args.get(key).cloned().unwrap_or_default();
            let channel =
                serde_json::from_value(parse("channel")).map_err(|e| format!("`channel`: {e}"))?;
            let plugin =
                serde_json::from_value(parse("plugin")).map_err(|e| format!("`plugin`: {e}"))?;
            cli::params(&path(args, "project")?, channel, plugin)
        }
        "samples_find" => {
            let q = match args.get("query") {
                Some(Value::String(words)) => pina_engine::samples::search::Query {
                    text: words.clone(),
                    ..Default::default()
                },
                Some(v) => {
                    serde_json::from_value(v.clone()).map_err(|e| format!("`query`: {e}"))?
                }
                None => return Err("`query` is required".into()),
            };
            let project = args
                .get("project")
                .and_then(Value::as_str)
                .map(PathBuf::from);
            crate::samples_cli::find(q, project.as_deref())
        }
        "samples_similar" => {
            let aspects = match args.get("aspects") {
                Some(v) => {
                    serde_json::from_value(v.clone()).map_err(|e| format!("`aspects`: {e}"))?
                }
                None => Default::default(),
            };
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;
            crate::samples_cli::similar(&path(args, "path")?, aspects, limit)
        }
        "samples_kit" => {
            let seed = match args.get("seed") {
                Some(Value::String(words)) => pina_engine::samples::search::KitSeed {
                    text: words.clone(),
                    ..Default::default()
                },
                Some(v) => serde_json::from_value(v.clone()).map_err(|e| format!("`seed`: {e}"))?,
                None => Default::default(),
            };
            let project = args
                .get("project")
                .and_then(Value::as_str)
                .map(PathBuf::from);
            crate::samples_cli::kit(seed, project.as_deref())
        }
        "samples_put" => crate::samples_cli::put(
            &path(args, "path")?,
            &path(args, "project")?,
            args.get("track")
                .and_then(Value::as_u64)
                .map(|t| t as usize),
            args.get("at").and_then(Value::as_u64).unwrap_or(0),
        ),
        "palette" => crate::samples_cli::palette(&path(args, "project")?),
        "samples_library" => match args.get("action").and_then(Value::as_str) {
            Some("status") => crate::samples_cli::status(),
            Some("scan") => crate::samples_cli::scan(),
            Some("add") => crate::samples_cli::add_folder(&path(args, "path")?),
            Some("remove") => crate::samples_cli::remove_folder(&path(args, "path")?),
            Some("info") => crate::samples_cli::info(&path(args, "path")?),
            _ => Err("`action` is one of status, scan, add, remove, info".into()),
        },
        "presets" => {
            let parse = |key: &str| args.get(key).cloned().unwrap_or_default();
            let channel =
                serde_json::from_value(parse("channel")).map_err(|e| format!("`channel`: {e}"))?;
            let plugin =
                serde_json::from_value(parse("plugin")).map_err(|e| format!("`plugin`: {e}"))?;
            let project = path(args, "project")?;
            match args.get("save").and_then(Value::as_str) {
                Some(name) => cli::save_preset(&project, channel, plugin, name),
                None => cli::presets(&project, channel, plugin),
            }
        }
        _ => Err(format!("no tool called {name}")),
    }
}

/// Answers one JSON-RPC message; None for notifications.
pub fn answer(msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let params = msg.get("params").cloned().unwrap_or_default();
    let result = match msg.get("method").and_then(Value::as_str).unwrap_or("") {
        "initialize" => json!({
            "protocolVersion": params.get("protocolVersion").cloned().unwrap_or("2025-06-18".into()),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "pina", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let (text, error) = match call(name, &args) {
                Ok(Value::String(s)) => (s, false),
                Ok(v) => (serde_json::to_string_pretty(&v).unwrap(), false),
                Err(e) => (e, true),
            };
            json!({ "content": [{ "type": "text", "text": text }], "isError": error })
        }
        method => {
            let error = json!({ "code": -32601, "message": format!("no method {method}") });
            return Some(json!({ "jsonrpc": "2.0", "id": id, "error": error }));
        }
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

pub fn serve() -> Result<(), String> {
    let mut out = std::io::stdout();
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => answer(&msg),
            Err(e) => Some(json!({
                "jsonrpc": "2.0", "id": null,
                "error": { "code": -32700, "message": e.to_string() }
            })),
        };
        if let Some(reply) = reply {
            writeln!(out, "{reply}").map_err(|e| e.to_string())?;
            out.flush().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaks_mcp() {
        let init = answer(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-06-18"}}))
        .unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "pina");
        assert_eq!(
            answer(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"})),
            None
        );

        let list = answer(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).unwrap();
        let names: Vec<&str> = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "describe",
                "show",
                "edit",
                "analyze",
                "listen",
                "render",
                "bounce",
                "critique",
                "study",
                "playbook",
                "theory",
                "reference",
                "sounds",
                "mix",
                "master",
                "transport",
                "plugins",
                "params",
                "presets",
                "samples_find",
                "samples_similar",
                "samples_kit",
                "samples_put",
                "palette",
                "samples_library"
            ]
        );

        // Build a little project on disk through the tools, then measure it.
        let dir = std::env::temp_dir().join(format!("pina-test-mcp-{}", std::process::id()));
        let tool = |id: u64, name: &str, args: Value| {
            let r = answer(&json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
                "params": {"name": name, "arguments": args}}))
            .unwrap();
            let text = r["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .to_string();
            (text, r["result"]["isError"].as_bool().unwrap())
        };
        let project = dir.to_str().unwrap();
        let (text, err) = tool(
            3,
            "edit",
            json!({"project": project,
            "command": {"op": "add_track", "name": "Keys", "instrument": "synth"}}),
        );
        assert!(!err, "{text}");
        let (_, err) = tool(
            4,
            "edit",
            json!({"project": project, "command": {"op": "add_midi_region",
            "track": 0, "start": 0, "length": 960,
            "notes": [{"start": 0, "length": 960, "pitch": 69, "velocity": 100}]}}),
        );
        assert!(!err);
        let (text, err) = tool(5, "analyze", json!({"path": project}));
        assert!(!err, "{text}");
        let measured: Value = serde_json::from_str(&text).unwrap();
        assert!(
            measured["mix"]["lufs"].as_f64().unwrap() > -40.0,
            "{measured}"
        );
        assert_eq!(measured["tracks"][0]["name"], "Keys");
        let (text, err) = tool(
            6,
            "edit",
            json!({"project": project, "command": {"op": "delete_track", "track": 5}}),
        );
        assert!(err && text.contains("no track 5"));
        let (text, err) = tool(
            7,
            "transport",
            json!({"project": project, "action": "play"}),
        );
        assert!(err && text.contains("doesn't have"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
