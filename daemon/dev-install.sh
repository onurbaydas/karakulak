#!/bin/sh
# Build karakulakd and register it with your own Firefox for development.
# Undo with: rm ~/.mozilla/native-messaging-hosts/karakulakd.json
set -e

dir=$(cd "$(dirname "$0")" && pwd)
cargo build --release --manifest-path "$dir/Cargo.toml"

hosts="$HOME/.mozilla/native-messaging-hosts"
mkdir -p "$hosts"
sed "s|/usr/bin/karakulakd|$dir/target/release/karakulakd|" \
	"$dir/../config/includes.chroot/usr/lib/mozilla/native-messaging-hosts/karakulakd.json" \
	> "$hosts/karakulakd.json"
echo "Registered: $hosts/karakulakd.json"
