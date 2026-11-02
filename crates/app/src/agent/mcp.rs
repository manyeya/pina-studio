//! `pina mcp`: an MCP server on stdin and stdout, so agents that speak MCP can make music with
//! Pina Studio. Its tools are the command line's, and edits reach the app when it has the project open.

use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
};

use pina_engine::song::commands::{Command, DESCRIBE};
use serde_json::{Value, json};

use crate::{agent::listen, agent::produce, cli, cli::server::Request};

const INSTRUCTIONS: &str = "Pina Studio is a DAW. A project is a folder; call `describe` first for the \
commands and how time and levels work, `show` to see a project, `edit` to change it one command at \
a time, and `analyze` to hear it as numbers (loudness, peaks, balance). To make a song that sounds \
finished, work as a producer and write it yourself, every note (`edit` with add_midi_region, \
add_pattern_region, set_notes): there are no templates to call. To write a song like a reference \
(a recording, or a picture of its spectrogram), `reference_study` or `reference_create` it, write \
it all yourself from its brief, then `reference_compare`. The person's sample library has MIDI \
files from their packs (chords, basslines, melodies, drum patterns): `midi_find` and `midi_study` \
them for ideas, and bring one in with the `import_midi` edit (`fit` moves it into the song's key) \
when it suits. When they give you MIDI (paths in their message), read it with `midi_study` (`notes` \
true) and several with `midi_group`. Asked for something like it, write your own notes in that \
style, in the song's key unless they say, and `midi_write` it with `like` set to what they gave you: \
it appears in the Agent panel for them to hear and drag in, it doesn't touch the song. Read the \
`likeness` it answers, rewrite until it's right, and never copy their notes. Plan the whole song, give each \
section its own parts, fills and transitions, `sounds` and samples for real instruments (or Pina Studio's own synth: `set_synth`, a patch \
shaped with all it has: wavetables and where through them, warps, unison, analog-modelled filters \
with drive, envelopes, LFOs and a matrix that can also ride the track's effects, and `describe` \
lists it all), \
`critique` after every step and fix what it says, `mix` and `master` to finish. `mix` sets \
every channel by listening (EQ, compressor, saturator, sends, levels): a starting point, not the \
end. Then shape each part's built-in effects for this song, and use all they have: the \
saturator's bands (split where the part needs it) and styles (warm and clean tape and tube, \
transformer, crunch and heavy amp, rectify, foldback, lo-fi) with drive, tone, dynamics, \
feedback tuned to the key and mix; the compressor's styles, parallel mix, auto release, spectral \
and sidechain filter; the EQ's dynamic, spectral and mid/side bands, sidechains, analog \
character and phase; the limiter's styles. Stack them where a part wants it (`\"which\": 1` on \
set_eq, set_comp, set_saturator or set_limiter adds a second): an EQ after the saturator to tame \
what it adds, a fast compressor then a slow one, two saturators in series, a clipper-style limiter \
before the bus. And use the tools (`set_fx`, each stacked anywhere in a chain, any number): `filter` \
for sweeps, wahs and rhythm (an LFO in time on the cutoff, or the sound's level on it; ladder, comb \
or formant for character), `motion` for width and life (chorus, ensemble, flanger, phaser, vibrato, \
tremolo, auto-pan), `shaper` for a hit's punch or tail whatever its level, `gate` to close what \
rings or bleeds, `de_esser` on anything sung, `stereo` for each band's width with its lows mono, \
`multiband` to glue a bus or lift a part's quiet detail, `tune` on any voice (its `notes` the \
song's key), `pitch` for octaves and harmonies, `tape_stop` to throw a transition. \
Automate them too (`set_automation` with `\"param\": \"fx\"`, the `fx` and the `knob` by name, in its \
own units; `\"param\": \"synth\"` and `knob` an id for the synth's): a filter opening into the drop, \
a reverb's mix thrown on the last word, a wavetable's position across a section. \
`learn` sets one effect by listening as a start. `critique` is your ears: trust it over your guesses. The person may have \
picked sounds, instruments and effects for the song (`palette`): build with those first, and with \
only those if they say so. Playbooks are the person's own notes on a genre: read or follow one only \
when they ask you to, never on your own; the genre, layout and sound are otherwise yours to decide.";

/// A list of paths given to a tool.
fn strings(args: &Value, key: &str) -> Vec<PathBuf> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .filter_map(Value::as_str)
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

fn tools() -> Value {
    let project = json!({ "type": "string", "description": "The project folder." });
    let mut all = json!([
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
            "description": "A producer's notes on the song, worst first, each with where and the command that fixes it: wrong notes and semitone clashes, a bass off its chords, kick and bass flamming, robotic velocities, missing swing, placeholder sounds, a song that never changes or starts with everything, a hook weaker than its verse, no transitions; and (unless `quick`) from a render: clipping, loudness, tonal balance against the reference (or the person's chosen playbook), low end in the wrong parts, parts too loud for their role, stereo trouble. `quick` skips the render (fast: use it after each edit). `from_bar`/`to_bar` limit the render. A song set up to be like a reference (reference_create) is also compared with the reference's spectrum on each full critique: its likeness, and its biggest differences as notes with the bars and tracks to change.",
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
            "description": "Studies an existing song, loop or stem from its audio file (WAV, AIFF, FLAC, MP3). `nature` says what the file is, what's not in it, and how sure; everything else says only what can be true of it (a drum loop has no key, a piano loop no groove). `tempo.why` and `key.why` say where each comes from. Who it is (tags; online, its year and genre), its tempo and bars, key (and changes), chords beat by beat with numerals, its form (intro, verse, pre-chorus, chorus, bridge, outro, each with its bars, energy, loudness and the chord loop it goes round), its groove (kick, snare and hats on a 16-step grid, swing, syncopation, nearest Pina Studio groove), what plays in it (with confidence), its mix and dynamics overall and by section, the person's playbooks it's nearest (if they have any), and (online, from LRCLIB) its lyrics: each line in its section, rhyme schemes, syllables, hooks, point of view and recurring words. Interpret its theme, story and imagery from the lyrics yourself. `playbook: true` also saves a playbook made from it, to write songs like it: only when the person asks for one.",
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
            "description": "One of the person's own genre playbooks, by id or name: tempo range, swing, key, its parts with the sounds they take and how to write them, its sections (bars, parts, energy), progressions, mix targets (LUFS and spectrum) and their notes. Without `genre`, the ones they have (maybe none). Use it only when the person asks you to use a playbook; then follow it as written. Never reach for one on your own.",
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
            "description": "Mixes tracks (all with regions, or `track`) by measuring them first: each part's own sound rendered and measured, then its channel set from that: a low cut under its lowest note, mud and harshness cut only as far as it has them (harshness tamed dynamically, spectrally on a lead), air where it lacks it, its low end mono; parts making room for each other (the bass dipping at the kick's own frequency as it hits; in genres that pump, the bass and pads ducking under the kick; chords and pads dipping their presence while the lead plays), compression set from each part's level, Pina Studio's own reverb on an aux (plate, chamber, hall, room or cathedral by genre, its pre-delay and decay by tempo and genre, ducking where voices must stay clear) and its own tempo-synced delay (tape, ping-pong, dub or digital by genre, ducked), the drums glued on a bus, then each part's level against the drums. Compressors and saturators are set by listening: the saturator's bands, styles, drive and tone for each part in the song's genre (a bass's fundamental clean and its harmonics driven, drums in three bands, a lead ringing on the key in electronic genres), drums far between soft and loud compressed in parallel, an analog EQ's colour where the genre is warm. Says what it did and why, track by track (`saturator`: its reasons). `role` overrides the part; `levels: false` leaves the faders. Then `listen` and `critique`, and `master`.",
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
            "name": "learn",
            "description": "One built-in effect set by listening, as the app's Learn button sets it, on `track` (the master without one). `effect`: `comp` (style, threshold, ratio, attack and release for what it plays; drums far between soft and loud compressed in parallel), `sat` (where it splits into bands, each band's style, drive, tone, dynamics and any ringing tuned to the key, for the part in the song's genre, as loud as it went in; with its reasons) or `limit` (style, release and gain to come out at `lufs`, -14 by default). Answers the settings without setting them; `apply: true` sets them. A starting point: change what it answers with set_comp, set_saturator or set_limiter to suit the song.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "track": { "type": "integer", "minimum": 0 },
                    "effect": { "type": "string", "enum": ["comp", "sat", "limit"] },
                    "lufs": { "type": "number" },
                    "apply": { "type": "boolean" }
                },
                "required": ["project", "effect"]
            }
        },
        {
            "name": "master",
            "description": "Masters the song: the master's EQ brings its tonal balance (sub, bass, low mid, mid, presence, air) halfway to the reference's (else the person's chosen playbook's), in broad moves; then the master's built-in limiter, set by listening (its style for the material, its release to the tempo, its gain found with the limiter itself) so the song is at `lufs` (the reference's, else the person's chosen playbook's, else -10), true peaks under -1 dBTP, and measured again. Mix first.",
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
    ]);
    if let Value::Array(list) = &mut all {
        list.extend(reference_tools(&project));
    }
    all
}

/// The tools for songs like a reference: studying one, setting one up, comparing with it.
fn reference_tools(project: &Value) -> Vec<Value> {
    let tools = json!([
        {
            "name": "midi_find",
            "description": "The person's sample library's MIDI files (chords, basslines, melodies, plucks, pads, drum patterns from their packs) that fit: by `words`, `role` (drums, bass, chords, pad, pluck, keys, lead, arp, fx), `key` and `tempo` (the project's own when `project` is given), best first. Each with what it is (its role, key, tempo, bars, progression, a summary of how it plays) and `transpose`, the semitones that move it into the key. Use them for ideas, or bring one in with import_midi.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "words": { "type": "string" },
                    "role": { "type": "string" },
                    "key": { "type": "string" },
                    "tempo": { "type": "number" },
                    "project": project,
                    "limit": { "type": "integer" }
                }
            }
        },
        {
            "name": "midi_group",
            "description": "Several MIDI files heard together: what they share (rhythm on a bar's sixteen steps, the degrees they lean on whatever key they're in, density, note lengths, dynamics, voicing, keys, progressions), how alike they are, each in a line. Give `candidate` (a file) to hear how close that comes to them, with `likeness` scores and advice. Use it before writing something like them, and after to check it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "files": { "type": "array", "items": { "type": "string" } },
                    "candidate": { "type": "string" }
                },
                "required": ["files"]
            }
        },
        {
            "name": "present",
            "description": "Shows the person something in the Studio Agent panel as a component rather than words. It changes nothing in the song. `kind` says which: `plan` (`title`, `steps`: [{text, state}] with state todo, doing or done; send it again with the same `id` as you work and it ticks along in place); `choices` (`title` the question, `options`: [{label, detail, ask}]; clicking one sends `ask` to you, so end your turn and wait for it); `metrics` (`title`, `metrics`: [{label, value, target, state}] with state good, warn or bad); `chords` (`title`, `key`, `chords`: [{name, numeral}], which they can hear); `sections` (`title`, `sections`: [{name, bars, energy}] with energy 0 to 1, optional `bar` where it starts; a click goes there); `next` (`options`: [{label, ask}], two to four things to do next, under your last words).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["plan", "choices", "metrics", "chords", "sections", "next"] },
                    "id": { "type": "string" },
                    "title": { "type": "string" },
                    "key": { "type": "string" },
                    "steps": { "type": "array", "items": { "type": "object" } },
                    "options": { "type": "array", "items": { "type": "object" } },
                    "metrics": { "type": "array", "items": { "type": "object" } },
                    "chords": { "type": "array", "items": { "type": "object" } },
                    "sections": { "type": "array", "items": { "type": "object" } }
                },
                "required": ["kind"]
            }
        },
        {
            "name": "midi_write",
            "description": "Your own notes as a MIDI file beside the song, shown in the Agent panel as a card the person can hear and drag onto the tracks. It does NOT change the song. `parts`, each `{name, notes, drums, program}`: `notes` one note a line as midi_study writes them, `1.3 G#4 2 96` (bar.step from 1 in sixteenths, a step can be fractional like 1.3.5, note name or number, how many sixteenths, velocity). Tempo, key and meter are the song's unless given. Give `like` (the files it's meant to resemble) and it answers how close it came and what's off; rewrite and write it again under a new name until it's right, never copying them note for note.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "name": { "type": "string" },
                    "tempo": { "type": "number" },
                    "key": { "type": "string" },
                    "parts": { "type": "array", "items": { "type": "object" } },
                    "like": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["project", "name", "parts"]
            }
        },
        {
            "name": "midi_study",
            "description": "A MIDI file studied in full, as a musician reads it: its key (from its notes, checked against its name), tempo, meter, chords bar by bar with numerals and the progression they go round, how many bars it loops, its traits; each part's role, range, polyphony, density, where its notes start on a bar's sixteen steps, swing, syncopation, velocity, note lengths and legato, a melody's contour, drums' patterns. With `notes`, every note (bar.step, name, sixteenths, velocity) of its first sixteen bars, to read like a score. Learn from it: its rhythm, its voicings, its shape; write your own on it, or import_midi it.",
            "inputSchema": {
                "type": "object",
                "properties": { "file": { "type": "string" }, "notes": { "type": "boolean" } },
                "required": ["file"]
            }
        },
        {
            "name": "reference_study",
            "description": "Studies a reference song for writing one like it: its audio (WAV, AIFF, FLAC, MP3), or a picture of its spectrogram (PNG, JPEG) when there's no audio. Answers its brief: each section's time (or, a picture's length unknown, its place through it), bars, energy, starts low, mid and high (with a bar's sixteen steps, from audio), balance, held pitched lines, chords (from audio); and every finding marked measured, estimated or unknown, and from what. A picture's plot and colours are found, but its axes can't be read from its pixels: look at its labels and give `seconds` (how long it spans), `low_hz`, `high_hz` and `scale`; `crop` [x, y, width, height] and `colors` correct what was found; `needs` lists what's still missing. Never claim what a picture can't show: chords, instruments, loudness, stereo width, lyrics.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file": { "type": "string" },
                    "crop": { "type": "array", "items": { "type": "integer" }, "minItems": 4, "maxItems": 4 },
                    "seconds": { "type": "number" },
                    "low_hz": { "type": "number" },
                    "high_hz": { "type": "number" },
                    "scale": { "type": "string", "enum": ["linear", "log", "mel"] },
                    "colors": { "type": "string", "enum": ["gray", "gray_inverted", "viridis", "magma", "inferno", "plasma", "jet"] },
                    "db_range": { "type": "number", "description": "How many dB its colours span, if its colour bar says." }
                },
                "required": ["file"]
            }
        },
        {
            "name": "reference_create",
            "description": "Sets up a new song to be like a reference (audio or a spectrogram picture, read as reference_study reads it, with the same reading fields): its tempo, key and meter where the reference says them, the reference and the person's choices kept with it (`style` in their words; how closely to follow its `rhythm`, `form` and `sound`: loose, close or tight). It writes no notes. Answers the project and the brief; then write the whole song yourself from the brief (plan, then every note, new parts, never copying), mark its sections, and call reference_compare.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file": { "type": "string" },
                    "title": { "type": "string" },
                    "style": { "type": "string" },
                    "rhythm": { "type": "string", "enum": ["loose", "close", "tight"] },
                    "form": { "type": "string", "enum": ["loose", "close", "tight"] },
                    "sound": { "type": "string", "enum": ["loose", "close", "tight"] },
                    "folder": { "type": "string", "description": "Where to make it (the person's songs by default)." },
                    "crop": { "type": "array", "items": { "type": "integer" }, "minItems": 4, "maxItems": 4 },
                    "seconds": { "type": "number" },
                    "low_hz": { "type": "number" },
                    "high_hz": { "type": "number" },
                    "scale": { "type": "string", "enum": ["linear", "log", "mel"] },
                    "colors": { "type": "string" },
                    "db_range": { "type": "number" }
                },
                "required": ["file"]
            }
        },
        {
            "name": "reference_compare",
            "description": "Renders a song made to be like a reference and compares it with it, section by section, as music: groove (where starts land, low, mid and high), tonal (pitch classes and held lines), balance (low, mid, high) and energy (each section against the whole), each 0-1; the biggest differences, with the bars and tracks they're in and what to change. With `request` (the person's words: \"closer drums\", \"brighter chorus\"), what it aims at and how the song moved since the last comparison: reached or stalled (stop), or better (another pass may help). Revise only what it names; leave the rest.",
            "inputSchema": {
                "type": "object",
                "properties": { "project": project, "request": { "type": "string" } },
                "required": ["project"]
            }
        }
    ]);
    match tools {
        Value::Array(list) => list,
        _ => Vec::new(),
    }
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

pub(crate) fn call(name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "describe" => Ok(Value::String(DESCRIBE.to_string())),
        // The panel draws it from the call itself: there's nothing to do here.
        "present" => Ok(json!({ "shown": true })),
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
            let s = crate::cli::study::study(
                &path(args, "file")?,
                args.get("online").and_then(Value::as_bool).unwrap_or(true),
            )?;
            let mut out = crate::cli::study::for_agent(&s);
            if args
                .get("playbook")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                let book = pina_engine::study::song_book(&s);
                let file = pina_engine::music::playbook::save(&book)?;
                out["playbook"] = json!({ "id": book.id, "name": book.name, "file": file });
            }
            Ok(out)
        }
        "midi_find" => {
            let q = pina_engine::samples::midi::MidiQuery {
                words: text(args, "words").unwrap_or("").to_string(),
                role: text(args, "role").map(str::to_string),
                key: text(args, "key").map(str::to_string),
                tempo: args.get("tempo").and_then(Value::as_f64).map(|t| t as f32),
                limit: args
                    .get("limit")
                    .and_then(Value::as_u64)
                    .map(|n| n as usize),
            };
            cli::midi::find(q, text(args, "project").map(Path::new))
        }
        "midi_group" => {
            let files = strings(args, "files");
            cli::midi::group(&files, text(args, "candidate").map(Path::new))
        }
        "midi_write" => cli::midi::write(
            &path(args, "project")?,
            text(args, "name").unwrap_or("MIDI"),
            args.get("tempo").and_then(Value::as_f64),
            text(args, "key"),
            args.get("parts")
                .and_then(Value::as_array)
                .map_or(&[], Vec::as_slice),
            &strings(args, "like"),
        ),
        "midi_study" => cli::midi::study(
            &path(args, "file")?,
            args.get("notes").and_then(Value::as_bool).unwrap_or(false),
        ),
        "reference_study" => {
            let given = cli::reference::calibration_from_json(args)?;
            let bp = cli::reference::blueprint(&path(args, "file")?, &given)?;
            Ok(cli::reference::for_agent(&bp))
        }
        "reference_create" => {
            use pina_engine::study::reference::Closeness;
            let close = |k: &str| text(args, k).and_then(Closeness::parse).unwrap_or_default();
            let choices = cli::reference::Choices {
                title: text(args, "title").map(str::to_string),
                style: text(args, "style").unwrap_or("").to_string(),
                rhythm: close("rhythm"),
                form: close("form"),
                sound: close("sound"),
            };
            let root = text(args, "folder")
                .map(PathBuf::from)
                .unwrap_or_else(crate::pages::projects::root);
            let given = cli::reference::calibration_from_json(args)?;
            let (dir, bp) = cli::reference::create(&path(args, "file")?, &given, &choices, &root)?;
            let like = cli::project(&dir)?
                .like
                .ok_or("the reference wasn't kept")?;
            Ok(json!({
                "project": dir,
                "brief": cli::reference::for_agent(&bp),
                "next": cli::reference::compose_prompt(&dir, &like, &bp),
            }))
        }
        "reference_compare" => {
            cli::reference::compare(&path(args, "project")?, text(args, "request"))
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
        "learn" => produce::learn_effect(
            &path(args, "project")?,
            args.get("track")
                .and_then(Value::as_u64)
                .map(|t| t as usize),
            text(args, "effect").ok_or("`effect` is required")?,
            args.get("lufs").and_then(Value::as_f64).map(|l| l as f32),
            args.get("apply").and_then(Value::as_bool).unwrap_or(false),
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
            crate::cli::samples::find(q, project.as_deref())
        }
        "samples_similar" => {
            let aspects = match args.get("aspects") {
                Some(v) => {
                    serde_json::from_value(v.clone()).map_err(|e| format!("`aspects`: {e}"))?
                }
                None => Default::default(),
            };
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;
            crate::cli::samples::similar(&path(args, "path")?, aspects, limit)
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
            crate::cli::samples::kit(seed, project.as_deref())
        }
        "samples_put" => crate::cli::samples::put(
            &path(args, "path")?,
            &path(args, "project")?,
            args.get("track")
                .and_then(Value::as_u64)
                .map(|t| t as usize),
            args.get("at").and_then(Value::as_u64).unwrap_or(0),
        ),
        "palette" => crate::cli::samples::palette(&path(args, "project")?),
        "samples_library" => match args.get("action").and_then(Value::as_str) {
            Some("status") => crate::cli::samples::status(),
            Some("scan") => crate::cli::samples::scan(),
            Some("add") => crate::cli::samples::add_folder(&path(args, "path")?),
            Some("remove") => crate::cli::samples::remove_folder(&path(args, "path")?),
            Some("info") => crate::cli::samples::info(&path(args, "path")?),
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
mod tests;
