#!/bin/sh
# Builds tempo.app (scripts/mac-app.sh: the release program, its icon, Info.plist and
# entitlements, signed) and installs it in /Applications (or the folder given). For a build to
# give to someone else, scripts/release-mac.sh makes the installer.
#
#   scripts/bundle.sh [folder] [icon.icns]
set -eu
cd "$(dirname "$0")/.."
DEST="${1:-/Applications}"
ICON="${2:-crates/app/icon/tempo.icns}"

cargo build --release -p tempo
APP="$DEST/tempo.app"
scripts/mac-app.sh target/release/tempo "$APP" "$ICON"
# Finder and the Dock read the icon again.
touch "$APP"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$APP"
echo "$APP"
