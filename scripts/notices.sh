#!/bin/sh
# THIRD-PARTY-NOTICES.md: every open-source crate in Pina Studio's builds (macOS, Windows, the web),
# under its licence with that licence's full text (cargo-about, from about.toml and about.hbs),
# then what isn't a crate (scripts/notices-more.md). Run it again whenever a dependency changes,
# with scripts/credits.py (the About page's list).
#
#   cargo install cargo-about --features cli     (once)
#   scripts/notices.sh
set -eu
cd "$(dirname "$0")/.."
cargo about generate about.hbs -o THIRD-PARTY-NOTICES.md
cat scripts/notices-more.md >> THIRD-PARTY-NOTICES.md
echo "THIRD-PARTY-NOTICES.md: $(grep -c '^- \[' THIRD-PARTY-NOTICES.md) entries"
