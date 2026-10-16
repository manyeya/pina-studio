#!/bin/sh
# Builds tempo.app: the release binary, its icon and Info.plist, signed ad hoc so macOS lets it
# ask for the microphone; and installs it in /Applications (or the folder given).
#
#   scripts/bundle.sh [folder] [icon.icns]
set -eu
cd "$(dirname "$0")/.."
DEST="${1:-/Applications}"
ICON="${2:-crates/app/icon/tempo.icns}"
ID="${TEMPO_BUNDLE_ID:-dev.tempo.daw}"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

cargo build --release -p tempo
APP="$DEST/tempo.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/tempo "$APP/Contents/MacOS/tempo"
cp "$ICON" "$APP/Contents/Resources/tempo.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>tempo</string>
  <key>CFBundleDisplayName</key><string>tempo</string>
  <key>CFBundleIdentifier</key><string>$ID</string>
  <key>CFBundleExecutable</key><string>tempo</string>
  <key>CFBundleIconFile</key><string>tempo</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.music</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <key>NSMicrophoneUsageDescription</key><string>tempo records from your audio inputs.</string>
</dict>
</plist>
PLIST
codesign --force --sign - "$APP"
# Finder and the Dock read the icon again.
touch "$APP"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$APP"
echo "$APP"
