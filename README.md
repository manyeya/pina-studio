# tempo

A DAW in the shape of Logic Pro, written in Rust: gpui for the UI, VST3 plugins. macOS first.

## Layout

- `crates/engine`: no UI. The project model (`project.rs`), the engine that plays and bounces it
  (`engine.rs`), the built-in synth, and VST3 hosting (`plugin/`).
- `crates/engine` also has the live `player.rs` (keeps the engine in step with edits) and
  `session.rs` (an open project and its undo history).
- `crates/engine` also has MIDI input (`midi.rs`), audio recording (`record.rs`), the plugin
  scan (`scan.rs`), edits as data (`commands.rs`, described in `commands.md`) and measuring
  audio (`analysis.rs`).
- `crates/app`: the gpui UI and the `tempo` binary: `workspace.rs` (the window, shortcuts, drags),
  `tracks.rs` (headers, ruler, regions, automation), `pianoroll.rs`, `takes.rs` (take folders), `record.rs` (recording
  takes), `strip.rs` (channel strips), `menu.rs` (plugin and bus menus), `theme.rs`,
  `server.rs` (the project's socket), `cli.rs` (the commands for agents), `mcp.rs`, and `tests.rs`, which drives the window
  headless with simulated mouse and keys.

## Run

    cargo run -p tempo -- examples/demo                     # open a project in the app
    cargo run -p tempo -- render examples/demo demo.wav     # bounce a project
    cargo run -p tempo -- play examples/demo                # play it through the default output
    cargo run -p tempo-engine --example play_vst -- "/path/to/Some Synth.vst3"

`play` sets the output device to the project's sample rate, as Logic does. `play_vst` loads a
plugin, opens its editor and plays an arpeggio through it.

In the app, as in Logic:

- Space plays and stops, return goes to the start, R records onto the selected track (MIDI from
  any MIDI keyboard or musical typing on an instrument track, the default input on an audio
  track).
- ⌘T splits the selected region at the playhead, delete deletes it, ⌘Z and ⇧⌘Z undo and redo,
  ⌥⌘N adds a track and ⌘⌫ deletes the selected one, ⌘S saves.
- X shows the mixer, P the piano roll (or double-click a MIDI region), A automation, I the
  inspector; ⌘K turns on musical typing (A to K play a C major scale from C3, W E T Y U the
  sharps, Z and X change octave).
- Drag regions to move them (across tracks too) and their edges to trim. In the piano roll,
  double-click to add a note, drag it to move it, drag its right edge to resize it.
- Recording over a region of the same kind makes a take folder, as in Logic. Double-click it to
  see its takes; swipe across a take to comp that stretch from it, click one to use all of it,
  and Flatten to turn the comp back into plain regions.
- With automation showing, click a lane to add a point, drag points, option-click to delete
  one; each track header switches between volume and pan.
- Drag faders, pan knobs and the tempo; option-click a fader or knob to reset it. Click or drag
  in the ruler to move the playhead; ⌘-scroll zooms.
- On a channel strip, click a plugin to open its editor; click an empty slot, or right-click any,
  to choose a plugin. "+ Send" sends the track to a bus, making an aux for a new one; drag a send
  for its level, right-click it to remove it.

Plugins are found in `/Library/Audio/Plug-Ins/VST3`, `~/Library/Audio/Plug-Ins/VST3` and the
folders in `TEMPO_VST3_PATH` (colon-separated). Each is first opened in a child process (`tempo
__scan`), so a broken one can't crash the app, and the results are cached in
`~/Library/Caches/tempo/plugins.json`. Saving writes each plugin's settings to `Plugin States/`
in the project. Plugin latency is compensated, as Logic does.

Builds need only the Command Line Tools: gpui compiles its Metal shaders at launch.

## Agents

tempo is made to be driven by agents as well as people. `tempo describe` prints every command an
agent can send, with examples. An agent can:

- Read a project with `tempo show <project>`.
- Change it one command at a time with `tempo do <project> '<json>'`.
- Measure it with `tempo analyze` (LUFS, peaks, clipping, low/mid/high balance, for the mix and
  each track), since it can't listen.

When the project is open in the app, commands go to the app over a socket. Each lands on screen as
one undoable edit, and `tempo save` saves it. Otherwise the folder is edited directly.

`tempo mcp` is the same as an MCP server on stdio. Its tools are `describe`, `show`, `edit`,
`analyze`, `render`, `transport` and `plugins`. For Claude Code:

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

An instrument is `"synth"` (built in) or `{ "vst3": { "path": …, "class": … } }`; a track without
one is an audio track. Audio files must be WAV at the project's sample rate, for now. Tracks can
have `"sends": [{ "bus": 1, "level_db": -6 }]`, played by an aux in `"auxes": [{ "bus": 1, "name":
"Reverb", "inserts": […] }]`.

The engine's tests that need a real plugin are opt-in:
`TEMPO_TEST_VST3="/path/to/Some Synth.vst3" cargo test -p tempo-engine -- --ignored`.
