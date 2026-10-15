# tempo

A DAW in the shape of Logic Pro, written in Rust: gpui for the UI, VST3 plugins. macOS first.

## Layout

- `crates/engine`: no UI. The project model (`project.rs`), the engine that plays and bounces it
  (`engine.rs`), the built-in synth, and VST3 hosting (`plugin/`).
- `crates/engine` also has the live `player.rs` (keeps the engine in step with edits) and
  `session.rs` (an open project and its undo history).
- `crates/engine` also has MIDI input (`midi.rs`) and audio recording (`record.rs`).
- `crates/app`: the gpui UI and the `tempo` binary: `workspace.rs` (the window, shortcuts, drags),
  `tracks.rs` (headers, ruler, regions, automation), `pianoroll.rs`, `record.rs` (recording
  takes), `strip.rs` (channel strips), `theme.rs`, and `tests.rs`, which drives the window
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
- With automation showing, click a lane to add a point, drag points, option-click to delete
  one; each track header switches between volume and pan.
- Drag faders, pan knobs and the tempo; option-click a fader or knob to reset it. Click or drag
  in the ruler to move the playhead; ⌘-scroll zooms.

Builds need only the Command Line Tools: gpui compiles its Metal shaders at launch.

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
one is an audio track. Audio files must be WAV at the project's sample rate, for now.
