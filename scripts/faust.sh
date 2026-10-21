#!/bin/sh
# Compiles the channel strip's Faust effects, the compressor and saturator
# (crates/engine/faust/*.dsp), to Rust, into crates/engine/src/faust/. The generated code is
# checked in, so building Pina Studio doesn't need Faust: run this after changing a .dsp
# (brew install faust).
set -e
cd "$(dirname "$0")/.."
FAUST=${FAUST:-$(command -v faust || echo "$HOME/.local/faust/bin/faust")}
for dsp in crates/engine/faust/*.dsp; do
  name=$(basename "$dsp" .dsp)
  class="$(printf '%s' "$name" | awk '{ print toupper(substr($0, 1, 1)) substr($0, 2) }')Dsp"
  "$FAUST" -lang rust -ftz 2 -cn "$class" -o "crates/engine/src/faust/$name.rs" "$dsp"
  # Faust's flush-to-zero writes `(-x.to_bits()...)`, negating the bits, not the number: the bits
  # of -0.0 overflow (a panic in a debug build) and some numbers flush when they shouldn't.
  sed -i '' 's/(-\(fTemp[0-9]*\)\.to_bits()/((-\1).to_bits()/g' "crates/engine/src/faust/$name.rs"
  echo "$dsp -> crates/engine/src/faust/$name.rs ($class)"
done
