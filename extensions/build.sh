#!/bin/sh
# Build an extension's Rust code to WebAssembly into extensions/<name>/ext/pkg.
# The ext/ folder is then a complete extension, ready to load or sign.
#
# Usage: extensions/build.sh welcome
set -e

name="$1"
dir=$(cd "$(dirname "$0")" && pwd)/$name

wasm-pack build "$dir" --target web --release --no-typescript --no-pack --out-dir ext/pkg
echo "Built: $dir/ext"
