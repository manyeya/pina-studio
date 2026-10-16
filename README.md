# tempo

A DAW with Logic Pro's structure and FL Studio's speed, written in Rust: gpui for the UI, VST3,
CLAP, VST2 and Audio Unit plugins, and agents as first-class collaborators. macOS first.

## Layout

- `crates/engine`: no UI. The project model (`project.rs`), the engine that plays and bounces it
  (`engine.rs`), the built-in synth, and plugin hosting (`plugin/`: one interface, with
  `vst3/`, `clap.rs`, `vst2.rs` and `au.rs` behind it).
- `crates/engine` also has the live `player.rs` (keeps the engine in step with edits) and
  `session.rs` (an open project and its undo history).
- `crates/engine` also has the built-in drum kit (`drums.rs`), the channel EQ, compressor and
  saturator (written in Faust, in `faust/*.dsp`, compiled to Rust in `src/faust/` and run by
  `dsp.rs`), keys, scales and note edits like quantize and legato (`music.rs`), MIDI input
  (`midi.rs`), audio recording (`record.rs`), the plugin scan (`scan.rs`), edits as data
  (`commands.rs`, described in `commands.md`) and measuring audio (`analysis.rs`).
- `crates/app`: the gpui UI and the `tempo` binary: `workspace.rs` (the window, shortcuts, drags),
  `topbar.rs` (transport and display), `inspector.rs` (the side panel: inspector, browser,
  agent), `tracks.rs` (tools, markers, ruler, headers, regions, automation), `editor.rs` (the
  editor under the tracks) with `pianoroll.rs`, `steps.rs` (the step sequencer), `score.rs`,
  `takes.rs` (take folders), `liveloops.rs` and `browser.rs` (the sample browser, a page of
  its own (⇧F) and a compact list beside the tracks to drag from (F): search, filters, the sound
  map, kits, adding samples to the song); `mixer.rs`, `strip.rs` (channel strips),
  `record.rs` (recording takes), `menu.rs`, `text.rs` (the text field names are typed in),
  `theme.rs` (colors, Geist, icons, controls),
  `server.rs` (the project's socket), `cli.rs` (the commands for agents), `mcp.rs`, and
  `tests.rs`, which drives the window headless with simulated mouse and keys.

## Run

    cargo run -p tempo -- examples/midnight-drive           # open a project in the app
    cargo run -p tempo -- examples/demo                     # a smaller one, with audio
    cargo run -p tempo -- render examples/demo demo.wav     # bounce a project
    cargo run -p tempo -- play examples/demo                # play it through the default output
    cargo run -p tempo-engine --example play_vst -- "/path/to/Some Synth.vst3"

`play` sets the output device to the project's sample rate, as Logic does. `play_vst` loads a
plugin, opens its editor and plays an arpeggio through it.

`examples/midnight-drive` was made entirely with `tempo do` commands, as an agent would make it.
So is Corner Store Blues, a 95 BPM boom-bap beat on real samples (MusicRadar's free 90s Hip Hop
pack, downloaded on first run: they can't be passed on, so they aren't here) with Surge XT and
Dragonfly Room Reverb installed:

    cargo build -p tempo && TEMPO=target/debug/tempo python3 examples/corner-store-blues/build.py

It goes in `~/Music/tempo/Corner Store Blues`: sampler kits on swung step patterns with aliases,
a Moog multisample bass and an 808 on the keys loop's roots, the loop flexed to half time in the
break, a Surge XT pad whose filter opens through each hook and a gliding sine whistle, drums
glued in a stack, two reverb sends, Live Loops scenes, and the master clipped by the saturator
to about -13.5 LUFS.

In the app, mostly as in Logic:

- Space plays and stops, return goes to the start, R records onto the armed track: the selected
  one, until a track's R button arms another (click it again to disarm). An instrument track
  records MIDI from any MIDI keyboard or musical typing; an audio track records the input its strip
  picks (any channel or pair of the default input device). C turns the cycle on and off, K the
  metronome; the cycle can also be swept out (or clicked on and off) in the strip along the
  ruler's top.
- Song plays the arrangement; Pattern (as in FL Studio) loops the selected region alone.
- The display shows the position, the time, the tempo (drag it), the time signature (pick one:
  4/4, 3/4, 6/8, 7/8…) and the key: worked out from the notes, or chosen from its menu. Choosing
  one moves the music into it (each step of the scale onto the new scale's, so major to minor
  changes the mode; drums and audio stay), unless the menu's last row is turned off. The piano
  roll shades the rows out of the key and marks it down its keyboard, notes are named as the key
  spells them, and the score shows its signature. The window narrows to 1120 points with all of
  it showing.
- In Pattern mode, selecting another region plays it at once, from the same place in its loop.
- Markers sit over the ruler: double-click to add one (and type its name), drag to move it, click
  to go there, double-click to rename it, right-click for a quick name or to delete it.
- Double-click a track's name, or a strip's name plate in the mixer, to rename it; a region's name
  is in the inspector. Enter keeps it, escape doesn't, clicking elsewhere keeps it too.
- Over the track headers: the tools (pointer; pencil, which draws a region in an empty lane and a
  note in the piano roll; scissors, which cut a region or note where clicked; eraser), and what
  edits snap to.
- ⌘T splits the selected region at the playhead, delete deletes it, ⌘Z and ⇧⌘Z undo and redo,
  ⌥⌘N adds a track and ⌘⌫ deletes the selected one, ⌘S saves.
- X shows the mixer, P the piano roll (or double-click a MIDI region), A automation lanes, I the
  side panel, F the sample browser beside the tracks (click a sound to hear it, ↑ ↓ for the next,
  drag it onto a track or double-click it to add it, More Like It for what sounds like it) and ⇧F
  its page (filters, the sound map, kits, the details; drag a sound out of it and the tracks come
  back to drop it on); ⌘K turns on musical typing (A to K play a C major scale from C3, W E T Y U the
  sharps, Z and X change octave). ⌘B exports the song to `Bounces/` in the project.
- The side panel's Inspector shows the selected region with its playback settings, as Logic's:
  Loop (it plays over and over until the next region on its track, its repeats drawn after it),
  and for MIDI and patterns Quantize, Transpose and Velocity, which change what plays but not
  the notes ("Apply to notes" writes them in). Then Smart Controls (a plugin instrument's filter
  and envelope, else the channel's EQ and compressor; turning them is an undoable edit) and the
  channel strips. Browser lists instruments and effects to put on the selected track; Agent shows
  how to connect one and everything it did.
- Drag regions to move them (across tracks too) and their edges to trim; option-drag one to copy
  it, option-shift-drag for an alias: a linked copy, its name in italics with a link, that plays
  the same notes, so editing either edits both (its place, length and playback settings are its
  own; "Make unique" in the inspector, splitting it or trimming its start unlinks it). Option-drag
  an audio region's end to flex it (Flex Time: slower or faster, same pitch, by Signalsmith
  Stretch). In
  the piano roll, double-click to add a note, drag it to move it, drag its right edge to resize
  it, drag its dot in the velocity lane; its tools quantize (grid, strength, swing), humanize,
  make legato, chop and arpeggiate; Ghost shows another track's notes behind.
- Recording over a region of the same kind makes a take folder, as in Logic. Double-click it to
  see its takes; swipe across a take to comp that stretch from it, click one to use all of it,
  and Flatten to turn the comp back into plain regions.
- With automation showing, each track has a lane under it: click to add a point, drag points,
  option-click to delete one. Its menu picks what it shows: volume, pan, or any parameter of the
  track's plugins (type to search; those with automation are marked). Its mode, as in Logic: Read
  plays the automation (faders, pan knobs, Smart Controls and plugin editors follow it), Touch and
  Latch also record what you move while the song plays (until you let go, or until it stops),
  and Off ignores it.
- Drag faders, pan knobs and the tempo; option-click a fader or knob to reset it. Click or drag
  in the ruler to move the playhead; ⌘-scroll zooms.
- The "+" over the track headers adds a software instrument (the sine synth), a drum kit (built
  in, on the General MIDI drum notes) or an audio track. ⌥⌘P makes a step pattern region on the
  selected instrument track; double-click one for the step sequencer: click steps on, option-click
  for an accent, "+ Sound" for more rows. Patterns loop over their region.
- N shows the score: the selected MIDI region as notation on a grand staff, in time with the
  tracks; click a note to select and hear it, delete to delete it.
- L shows Live Loops: a grid of loop cells, a row per track and a column per scene. Click an
  empty cell to copy the track's selected region into it; click a cell to launch it (on the next
  bar, or at once, starting playback) and again to stop it; right-click to empty it; the buttons
  under the grid launch a scene. A launched cell plays instead of the track's regions.
- ⇧⌘D makes a summing stack of the selected track: a stack channel (in the mixer) it plays
  into, its name shown on the track. A strip's "→" row routes a track into any stack or aux, or
  back to Stereo Out.
- On a channel strip, click a plugin to open its editor; click an empty slot, or right-click any,
  to choose a plugin or the built-in Channel EQ (low cut, two bells, high shelf), Compressor and
  Saturator (drive, tone, mix, output). The master takes plugins too, after its built-ins. Click a
  strip's EQ curve to shape it in the mixer, where the selected channel's chain shows on the
  right: drag the EQ's handles (double-click one to flatten it), turn the compressor's knobs and
  watch its gain reduction, drive the saturator and see its curve. "Send" sends the track to a
  bus, making an aux for a new one; drag a send for its level, right-click it to switch it before
  the fader (amber) or after, or to remove it. Meters are stereo, with peak holds.

Plugins are VST3, CLAP, VST2 and Audio Units. They're found in `VST3`, `CLAP` and `VST` under
`/Library/Audio/Plug-Ins` and `~/Library/Audio/Plug-Ins`, the folders in `TEMPO_PLUGIN_PATH`
(colon-separated), and, for Audio Units, wherever macOS has them registered. Each bundle is
first opened in a child process (`tempo __scan`), so a broken one can't crash the app, and the
results are cached in `~/Library/Caches/tempo/plugins.json`. A plugin slot's menu lists them all
(with their format, since one plugin is often installed in several) and the presets of the one
in it. Saving writes each plugin's settings to `Plugin States/` in the project. Plugin latency is
compensated, as Logic does. If the output device can't run at the project's sample rate (a
Bluetooth speaker at 44.1 kHz), playback runs at the device's.

Builds need only the Command Line Tools: gpui compiles its Metal shaders at launch.

The channel EQ, compressor and saturator are Faust (`crates/engine/faust/*.dsp`), using its
libraries: Andy Simper's state-variable filters for the EQ, its soft-knee compressor gain computer,
and antiderivative antialiasing for the saturator, which also runs at twice the sample rate (a
half-band filter designed by `scripts/halfband.py`). The Rust Faust makes of them is checked in
under `crates/engine/src/faust/`, so building tempo doesn't need Faust; after changing a .dsp, run
`scripts/faust.sh` (with `faust` on the path, or in `~/.local/faust/bin`).

## Agents

tempo is made to be driven by agents as well as people. `tempo describe` prints every command an
agent can send, with examples. An agent can:

- Read a project with `tempo show <project>`.
- Change it one command at a time with `tempo do <project> '<json>'`.
- Measure it with `tempo analyze`, since it can't listen: loudness (integrated and loudest),
  peaks, clipping, crest, six-band balance, stereo correlation and width, and the notes sounding
  with the key they fit, for the mix, each section (marker to marker) and each track.
- Find samples that sound good together: `tempo samples find "dusty keys loop in C minor 95 bpm"`
  (or a JSON query), `tempo samples similar <file>`, `tempo samples kit "punchy" --for <project>`
  (a kit fitting the song's key and tempo, each pick going with the rest, with alternatives) and
  `tempo samples info <file>`. Every hit says why it fits ("in C minor", "stretches from 92
  BPM", "same pack as Kick_01"), how far to transpose it if a small move fits better, and, for a
  one-shot, the sampler zone that plays it in tune. `tempo samples put <file> <project>` puts one
  in the song made to fit: a loop stretched to its tempo and moved into its key, a one-shot as a
  sampler zone in tune.
- Play any plugin as a person would: find it with `tempo plugins`, set its parameters by name and
  as it shows them (`"param": "Filter Cutoff", "text": "850 Hz"`), list and load its presets and
  save new ones (`tempo presets`, `set_preset`), automate any parameter, send it controllers,
  pitch bend, pressure and per-note expression with the notes (`set_controls`), and feed a track
  into its sidechain (`set_sidechain`).

When the project is open in the app, commands go to the app over a socket. Each lands on screen as
one undoable edit, and `tempo save` saves it. The agent shows up like a collaborator in a shared
file: its avatar in the top bar, an outline and cursor on what it just changed, a line in the
status bar, and its history in the side panel's Agent tab. Otherwise the folder is edited
directly.

`tempo params <project> <track>` (or `--aux N`, `--master`, with `--insert N`) lists a plugin's
parameters (value and display, default, unit, steps, and a choice's options), for
`set_plugin_param` and plugin automation; `tempo presets` lists its presets, and `--save <name>`
saves its settings as one. Both answer from the app if it's open, else by loading that plugin.

The sample library is the folders in `~/Library/Application Support/tempo/library.json`
(`~/Music/tempo/Samples` to start; `tempo samples add <folder>`), each sample analysed once (what
it is, its note or key, tempo, envelope, spectrum, stereo width, traits like punchy or lo-fi, and
a timbre fingerprint) and indexed in `~/Library/Caches/tempo/samples.json`; `tempo samples scan`
analyses what's new or changed, on every core.

`tempo mcp` is the same as an MCP server on stdio, for agents without a shell (Claude Desktop,
claude.ai and the like): its tools list themselves with typed inputs, and a user can allow them
without allowing a shell. Its tools are `describe`, `show`, `edit`, `analyze`, `render`,
`transport`, `plugins`, `params`, `presets`, `samples_find`, `samples_similar`, `samples_kit`,
`samples_put` and `samples_library`. For Claude Code:

    claude mcp add tempo -- /path/to/tempo mcp

## Projects

A project is a folder holding `project.json` and, for audio regions, `Audio Files/`. Positions
are in ticks, 960 to a quarter note; audio offsets are in samples.

```json
{
  "tempo": 120,
  "sample_rate": 48000,
  "tracks": [
    {
      "name": "Keys",
      "instrument": "synth",
      "volume_db": -6, "pan": -0.2, "mute": false, "solo": false,
      "inserts": [{ "path": "/Library/Audio/Plug-Ins/VST3/Some Reverb.vst3", "class": "Some Reverb" }],
      "regions": [{ "start": 0, "length": 3840,
                    "midi": { "notes": [{ "start": 0, "length": 960, "pitch": 60, "velocity": 100 }] } }]
    },
    {
      "name": "Drums",
      "regions": [{ "start": 0, "length": 7680, "audio": { "file": "Audio Files/drums.wav", "offset": 0 } }]
    }
  ]
}
```

An instrument is `"synth"` (built in) or `{ "plugin": { "format": …, "path": …, "class": …,
"uid": … } }` (as `tempo plugins` lists it; `"vst3"` is still read); a plugin can carry the
`"preset"` tempo loaded, `"params"` tempo sets on it (by id, in the order set) and, as an insert,
the track it hears as a `"sidechain"`. A track without one is an audio track, with an `"input"`.
Audio files are WAV, at any sample rate. A MIDI region can have `"controls"` beside its notes. Tracks can have
`"sends": [{ "bus": 1, "level_db": -6, "pre": false }]`, played by an aux in `"auxes": [{ "bus":
1, "name": "Reverb", "inserts": […] }]`. Tracks, auxes and the master (`master_eq`,
`master_comp`, `master_sat`, `master_inserts`) can have an `"eq"`, a `"comp"` and a `"sat"`. A
track's `"automation"` has `volume`, `pan` and `plugins` lanes and a `mode`. Regions can have a
`"name"`, a `"link"` (aliases share one), `"loop"`, `"transpose"`, `"velocity"` and `"quantize"`.
The project can have a `"time_signature"`, `"markers"`, a `"key"` and a `"cycle"` (see `tempo
describe`).

The engine's tests that need a real plugin are opt-in, each format by its own variable:

    TEMPO_TEST_VST3="/path/Some Synth.vst3" TEMPO_TEST_CLAP="/path/Some Synth.clap" \
    TEMPO_TEST_CLAP_FX="/path/Some Reverb.clap" TEMPO_TEST_VST2="/path/Some Plugin.vst" \
    TEMPO_TEST_AU="aumu SgXT VmbA" cargo test -p tempo-engine -- --ignored

Apple's own Audio Units (AUSampler, AUDelay, AULowpass…) are tested on every run.
