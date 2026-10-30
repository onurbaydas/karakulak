#!/bin/sh
# Installs the karakulak package on Debian next to everything it lives with,
# without any prompts. Catches files that clash with other packages (or with
# files their install scripts create) in minutes instead of a whole ISO build.
#
# Usage (as root, on Debian 13): tests/install-test.sh <karakulak .deb>
set -e

deb=$(realpath "$1")
root=$(cd "$(dirname "$0")/.." && pwd)
export DEBIAN_FRONTEND=noninteractive

apt-get update -qq
apt-get install -y -qq ca-certificates >/dev/null
# Firefox comes from Mozilla, as in the image.
cp "$root/config/archives/mozilla.key.chroot" /etc/apt/trusted.gpg.d/mozilla.asc
cp "$root/config/archives/mozilla.list.chroot" /etc/apt/sources.list.d/mozilla.list
cp "$root/config/archives/mozilla.pref.chroot" /etc/apt/preferences.d/mozilla.pref
sed -i 's/^Components: main$/Components: main contrib non-free non-free-firmware/' /etc/apt/sources.list.d/debian.sources
apt-get update -qq

# apt installs the package and its dependencies together; any dpkg question
# fails here because there is no terminal to answer it.
apt-get install -y --no-install-recommends "$deb" </dev/null
dpkg -s karakulak | grep -q "^Status: install ok installed"
echo "karakulak installs cleanly"
