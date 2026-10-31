#!/bin/sh
# Build the website demo: the panel's pages with made-up data in place of
# karakulakd (the panel's "demo" feature), around extensions/panel/demo/index.html.
# The result is a static folder; GitHub Pages serves it at /demo/.
#
# Usage: scripts/build-demo.sh <output folder>
set -e

root=$(cd "$(dirname "$0")/.." && pwd)
out=$(mkdir -p "$1" && cd "$1" && pwd)
ext="$root/extensions/panel/ext"

wasm-pack build "$root/extensions/panel" --target web --release --no-typescript --no-pack \
	--out-dir "$out/pkg" -- --features demo
rm -f "$out/pkg/.gitignore"
cp "$ext"/*.html "$ext"/*.css "$ext/boot.js" "$ext/logo.svg" "$ext/icon.svg" "$out/"
cp "$root/extensions/panel/demo/index.html" "$out/"
echo "Built: $out"
