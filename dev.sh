#!/bin/sh
# Try an extension in your own Firefox: builds karakulakd and the extension,
# registers karakulakd with Firefox, then opens Firefox with the extension
# loaded. Uses a separate profile (~/.cache/karakulak-dev), so your normal
# Firefox is untouched and the dev profile keeps its settings between runs.
#
# Usage: ./dev.sh            (panel)
#        ./dev.sh welcome
set -e

name="${1:-panel}"
root=$(cd "$(dirname "$0")" && pwd)

"$root/daemon/dev-install.sh"
"$root/extensions/build.sh" "$name"

exec npx --yes web-ext@8 run \
	--source-dir "$root/extensions/$name/ext" \
	--firefox-profile "$HOME/.cache/karakulak-dev" \
	--profile-create-if-missing \
	--keep-profile-changes
