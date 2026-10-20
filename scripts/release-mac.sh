#!/bin/sh
# Makes tempo's Mac installer: dist/tempo-<version>.dmg. On it, "Install tempo" (crates/installer)
# with tempo inside it, built for Apple silicon and Intel, the example songs beside it, in a window
# of its own design (scripts/dmg-background.py, scripts/dmg-settings.py).
#
#   scripts/release-mac.sh
#
# Signed with the hardened runtime: by a Developer ID if TEMPO_SIGN_ID names one ("Developer ID
# Application: Name (TEAMID)"), else ad hoc (it installs, but on another Mac Gatekeeper asks
# first; the window says how). Notarized and stapled if TEMPO_NOTARY_PROFILE names a profile
# saved once with `xcrun notarytool store-credentials`. TEMPO_ARCHS picks the architectures
# (default both; "aarch64-apple-darwin" alone is quicker to try).
set -eu
cd "$(dirname "$0")/.."
VERSION="${TEMPO_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}"
export TEMPO_VERSION="$VERSION"
ARCHS="${TEMPO_ARCHS:-aarch64-apple-darwin x86_64-apple-darwin}"
OUT=dist
BUILD="$OUT/build"
DMG="$OUT/tempo-$VERSION.dmg"
rm -rf "$BUILD" "$DMG" "$DMG.sha256"
mkdir -p "$BUILD"

say() { printf '\n\033[1m%s\033[0m\n' "$*"; }

say "Building tempo $VERSION for: $ARCHS"
for t in $ARCHS; do
  cargo build --release -p tempo -p tempo-installer --target "$t"
done
for bin in tempo tempo-installer; do
  lipo -create $(for t in $ARCHS; do printf 'target/%s/release/%s ' "$t" "$bin"; done) -output "$BUILD/$bin"
done
lipo -info "$BUILD/tempo"

say "tempo.app"
scripts/mac-app.sh "$BUILD/tempo" "$BUILD/tempo.app"

say "Install tempo.app"
INSTALLER="$BUILD/Install tempo.app"
mkdir -p "$INSTALLER/Contents/MacOS" "$INSTALLER/Contents/Resources/Examples"
cp "$BUILD/tempo-installer" "$INSTALLER/Contents/MacOS/tempo-installer"
cp crates/app/icon/tempo.icns "$INSTALLER/Contents/Resources/tempo.icns"
ditto "$BUILD/tempo.app" "$INSTALLER/Contents/Resources/tempo.app"
# The example songs, without the bounces they were checked against.
ditto examples/midnight-drive "$INSTALLER/Contents/Resources/Examples/Midnight Drive"
rm -rf "$INSTALLER/Contents/Resources/Examples/Midnight Drive/Bounces"
cat > "$INSTALLER/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Install tempo</string>
  <key>CFBundleDisplayName</key><string>Install tempo</string>
  <key>CFBundleIdentifier</key><string>dev.tempo.installer</string>
  <key>CFBundleExecutable</key><string>tempo-installer</string>
  <key>CFBundleIconFile</key><string>tempo</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
# Inside out: tempo.app was signed as it was made; the installer round it now.
codesign --force --options runtime ${TEMPO_SIGN_ID:+--timestamp} --sign "${TEMPO_SIGN_ID:--}" "$INSTALLER"
codesign --verify --deep --strict "$INSTALLER"

say "The disk image"
UNSIGNED=""
[ -z "${TEMPO_SIGN_ID:-}" ] && UNSIGNED="--unsigned"
python3 scripts/dmg-background.py "$BUILD" "$VERSION" $UNSIGNED
tiffutil -cathidpicheck "$BUILD/background.png" "$BUILD/background@2x.png" -out "$BUILD/background.tiff" >/dev/null
VENV=target/dmgbuild-venv
if [ ! -x "$VENV/bin/dmgbuild" ]; then
  python3 -m venv "$VENV"
  "$VENV/bin/pip" install -q dmgbuild==1.6.5
fi
"$VENV/bin/dmgbuild" -s scripts/dmg-settings.py \
  -D installer="$INSTALLER" \
  -D background="$BUILD/background.tiff" \
  -D volicon=crates/app/icon/tempo.icns \
  "tempo $VERSION" "$DMG"

# What's on it still signed as it was (nothing written onto it on the way).
M=$(mktemp -d)
hdiutil attach "$DMG" -nobrowse -readonly -mountpoint "$M" -quiet
codesign --verify --deep --strict "$M/Install tempo.app"
hdiutil detach "$M" -quiet
if [ -n "${TEMPO_SIGN_ID:-}" ]; then
  codesign --force --timestamp --sign "$TEMPO_SIGN_ID" "$DMG"
fi
if [ -n "${TEMPO_NOTARY_PROFILE:-}" ]; then
  say "Notarizing (a few minutes)"
  xcrun notarytool submit "$DMG" --keychain-profile "$TEMPO_NOTARY_PROFILE" --wait
  xcrun stapler staple "$DMG"
fi
(cd "$OUT" && shasum -a 256 "$(basename "$DMG")" > "$(basename "$DMG").sha256")

say "Done"
ls -lh "$DMG" | awk '{print $5, $9}'
case "${TEMPO_SIGN_ID:-}" in
  "") echo "Signed ad hoc: on another Mac, Gatekeeper asks before it opens (set TEMPO_SIGN_ID and TEMPO_NOTARY_PROFILE to sign and notarize)." ;;
  *) [ -z "${TEMPO_NOTARY_PROFILE:-}" ] && echo "Signed, not notarized: set TEMPO_NOTARY_PROFILE to notarize." ;;
esac
