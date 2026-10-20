#!/bin/sh
# Builds Pina Studio.app (scripts/mac-app.sh: the release program, its icon, Info.plist and
# entitlements, signed) and installs it in /Applications (or the folder given). For a build to
# give to someone else, scripts/release-mac.sh makes the installer.
#
#   scripts/bundle.sh [folder] [icon.icns]
set -eu
cd "$(dirname "$0")/.."
DEST="${1:-/Applications}"
ICON="${2:-crates/app/icon/pina.icns}"

cargo build --release -p pina
APP="$DEST/Pina Studio.app"
scripts/mac-app.sh target/release/pina "$APP" "$ICON"
# Finder and the Dock read the icon again.
touch "$APP"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$APP"
echo "$APP"
# tempo, as it was called before: its app is left for you to put away (it may be open).
[ -d "$DEST/tempo.app" ] && echo "$DEST/tempo.app is the old tempo: drag it to the Trash once it's closed." || true
