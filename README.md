# tempo

A DAW in the shape of Logic Pro, written in Rust: gpui for the UI, VST3 plugins. macOS first.

## Layout

- `crates/engine`: no UI. Project model, audio graph, VST3 hosting (`src/plugin`), rendering.
- `crates/app`: the gpui UI and the `tempo` binary.

## Run

    cargo run -p tempo                    # arrange window (test waveforms for now)
    cargo run -p tempo-engine --example play_vst -- "/path/to/Some Synth.vst3"

`play_vst` loads the plugin, opens its editor and plays an arpeggio through the default output,
printing the output peak per note.

Builds need only the Command Line Tools: gpui compiles its Metal shaders at launch.
