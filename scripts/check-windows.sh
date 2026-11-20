#!/bin/sh
# Pina Studio checked for Windows on a Mac, before anything's pushed: every crate and its tests,
# compiled for x86_64-pc-windows-msvc with Microsoft's headers (cargo-xwin, clang-cl and
# lld-link from LLVM). A type-check: nothing's linked or run, so the C libraries that only build
# on Windows itself (LAME, whisper.cpp) are skipped, and Rubber Band's build script, which picks
# its target by the computer it runs on, is pointed at Windows. It fails on any error, and on
# any warning a Windows build has that a Mac's doesn't.
#
#   brew install llvm lld; rustup target add x86_64-pc-windows-msvc; cargo install cargo-xwin   (once)
#   scripts/check-windows.sh
set -eu
cd "$(dirname "$0")/.."
export PATH="$(brew --prefix llvm)/bin:$(brew --prefix lld)/bin:$PATH"
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
# The patch below gets written into Cargo.lock: put back as it was, however the check ends.
cp Cargo.lock target/Cargo.lock.before-windows-check 2>/dev/null || { mkdir -p target && cp Cargo.lock target/Cargo.lock.before-windows-check; }
trap 'cp target/Cargo.lock.before-windows-check Cargo.lock' EXIT
RB=$(ls -d "$HOME"/.cargo/registry/src/*/rubberband-sys-0.1.4 | head -1)
PATCH="$PWD/target/xwin-patch/rubberband-sys"
rm -rf "$PATCH" && mkdir -p "$(dirname "$PATCH")" && cp -R "$RB" "$PATCH"
sed -i '' 's/cfg!(target_os = "\([a-z]*\)")/(std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("\1"))/g; s/cfg!(target_arch = "\([a-z0-9_]*\)")/(std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("\1"))/g' "$PATCH/build.rs"
OUT=$(DOCS_RS=1 cargo xwin check --target x86_64-pc-windows-msvc --all-targets -p pina -p pina-engine \
  --config "patch.crates-io.rubberband-sys.path=\"$PATCH\"" --message-format short 2>&1) || { echo "$OUT" | grep -E "error" ; exit 1; }
WARN=$(echo "$OUT" | grep -E "^crates/.*warning" | sort -u || true)
[ -z "$WARN" ] || { echo "$WARN"; echo "Windows-only warnings: fix them before pushing."; exit 1; }
echo "Windows: compiles, no warnings."
