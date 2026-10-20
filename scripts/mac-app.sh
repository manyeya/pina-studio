#!/bin/sh
# Makes Pina Studio.app from a built `pina` program: its Info.plist, icon and entitlements, signed with
# the hardened runtime: by a Developer ID if PINA_SIGN_ID names one ("Developer ID Application:
# …"), else ad hoc (fine on this Mac; elsewhere Gatekeeper asks). scripts/bundle.sh (this Mac)
# and scripts/release-mac.sh (the installer) both make it here.
#
#   scripts/mac-app.sh <pina program> <path/to/Pina Studio.app> [icon.icns]
set -eu
cd "$(dirname "$0")/.."
BIN="$1"
APP="$2"
ICON="${3:-crates/app/icon/pina.icns}"
ID="${PINA_BUNDLE_ID:-studio.pina.app}"
VERSION="${PINA_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}"
YEAR=$(date +%Y)

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/pina"
cp "$ICON" "$APP/Contents/Resources/pina.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Pina Studio</string>
  <key>CFBundleDisplayName</key><string>Pina Studio</string>
  <key>CFBundleIdentifier</key><string>$ID</string>
  <key>CFBundleExecutable</key><string>pina</string>
  <key>CFBundleIconFile</key><string>pina</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>NSHumanReadableCopyright</key><string>© $YEAR Pina Studio</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.music</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <key>NSMicrophoneUsageDescription</key><string>Pina Studio records from your audio inputs.</string>
</dict>
</plist>
PLIST
codesign --force --options runtime --entitlements scripts/pina.entitlements \
  ${PINA_SIGN_ID:+--timestamp} --sign "${PINA_SIGN_ID:--}" "$APP"
