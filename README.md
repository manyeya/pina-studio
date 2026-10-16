# tempo

A DAW with Logic Pro's structure and FL Studio's speed, written in Rust: gpui for the UI, VST3
plugins, and agents as first-class collaborators. macOS first.

## Layout

- `crates/engine`: no UI. The project model (`project.rs`), the engine that plays and bounces it
  (`engine.rs`), the built-in synth, and VST3 hosting (`plugin/`).
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
  `takes.rs` (take folders) and `liveloops.rs`; `mixer.rs`, `strip.rs` (channel strips),
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

In the app, mostly as in Logic:

- Space plays and stops, return goes to the start, R records onto the armed track: the selected
  one, until a track's R button arms another (click it again to disarm). An instrument track
  records MIDI from any MIDI keyboard or musical typing; an audio track records the input its strip
  picks (any channel or pair of the default input device). C turns the cycle on and off, K the
  metronome; the cycle can also be swept out (or clicked on and off) in the strip along the
  ruler's top.
- Song plays the arrangement; Pattern (as in FL Studio) loops the selected region alone.
- The display shows the position, the time, the tempo (drag it), the time signature (pick one:
  4/4, 3/4, 6/8, 7/8…) and the key: worked out from the notes, or chosen from its menu. The piano
  roll shades the rows out of the key. The window narrows to 1120 points with all of it showing.
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
  side panel; ⌘K turns on musical typing (A to K play a C major scale from C3, W E T Y U the
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

Plugins are found in `/Library/Audio/Plug-Ins/VST3`, `~/Library/Audio/Plug-Ins/VST3` and the
folders in `TEMPO_VST3_PATH` (colon-separated). Each is first opened in a child process (`tempo
__scan`), so a broken one can't crash the app, and the results are cached in
`~/Library/Caches/tempo/plugins.json`. Saving writes each plugin's settings to `Plugin States/`
in the project. Plugin latency is compensated, as Logic does.

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
- Measure it with `tempo analyze` (LUFS, peaks, clipping, low/mid/high balance, for the mix and
  each track), since it can't listen.

When the project is open in the app, commands go to the app over a socket. Each lands on screen as
one undoable edit, and `tempo save` saves it. The agent shows up like a collaborator in a shared
file: its avatar in the top bar, an outline and cursor on what it just changed, a line in the
status bar, and its history in the side panel's Agent tab. Otherwise the folder is edited
directly.

`tempo params <project> <track>` (or `--aux N`, `--master`, with `--insert N`) lists a plugin's
parameters, for `set_plugin_param` and plugin automation; from the app if it's open, else by
loading the plugin.

`tempo mcp` is the same as an MCP server on stdio. Its tools are `describe`, `show`, `edit`,
`analyze`, `render`, `transport`, `plugins` and `params`. For Claude Code:

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

An instrument is `"synth"` (built in) or `{ "vst3": { "path": …, "class": … } }`; a plugin can
carry `"params"` tempo sets on it, by id. A track without one is an audio track, with an
`"input"`. Audio files must be WAV at the project's sample rate, for now. Tracks can have
`"sends": [{ "bus": 1, "level_db": -6, "pre": false }]`, played by an aux in `"auxes": [{ "bus":
1, "name": "Reverb", "inserts": […] }]`. Tracks, auxes and the master (`master_eq`,
`master_comp`, `master_sat`, `master_inserts`) can have an `"eq"`, a `"comp"` and a `"sat"`. A
track's `"automation"` has `volume`, `pan` and `plugins` lanes and a `mode`. Regions can have a
`"name"`, a `"link"` (aliases share one), `"loop"`, `"transpose"`, `"velocity"` and `"quantize"`.
The project can have a `"time_signature"`, `"markers"`, a `"key"` and a `"cycle"` (see `tempo
describe`).

The engine's tests that need a real plugin are opt-in:
`TEMPO_TEST_VST3="/path/to/Some Synth.vst3" cargo test -p tempo-engine -- --ignored`.
