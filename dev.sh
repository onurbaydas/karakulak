#!/bin/sh
# Try Karakulak on your own machine.
#
#   ./dev.sh            Firefox with the panel extension
#   ./dev.sh welcome    Firefox with the welcome extension
#   ./dev.sh session    the whole session: Karakulak's compositor in a window,
#                       fullscreen Firefox with the panel inside it
#
# Builds karakulakd and the extension, registers karakulakd with Firefox and
# uses a separate profile (~/.cache/karakulak-dev), so your normal Firefox is
# untouched and the dev profile keeps its settings between runs.
set -e

root=$(cd "$(dirname "$0")" && pwd)
name="${1:-panel}"
session=false
if [ "$name" = "session" ]; then
	session=true
	name=panel
fi

"$root/daemon/dev-install.sh"
"$root/extensions/build.sh" "$name"

set -- npx --yes web-ext@8 run \
	--source-dir "$root/extensions/$name/ext" \
	--firefox-profile "$HOME/.cache/karakulak-dev" \
	--profile-create-if-missing \
	--keep-profile-changes

if $session; then
	cargo build --release --manifest-path "$root/compositor/Cargo.toml"
	exec "$root/compositor/target/release/karakulak-compositor" -- "$@"
fi
exec "$@"
