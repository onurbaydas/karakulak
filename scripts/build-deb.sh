#!/bin/sh
# Builds the `karakulak` Debian package: the compositor, karakulakd, the signed
# extensions and every file under config/includes.chroot (session, policies,
# theme, polkit, PAM...). Installed systems get Karakulak's updates through it.
#
# Usage: scripts/build-deb.sh <version> <output folder>
# Expects the binaries built at daemon/target/release/karakulakd and
# compositor/target/release/karakulak-compositor.
set -e

version="$1"
out="$2"
root=$(cd "$(dirname "$0")/.." && pwd)
tree=$(mktemp -d)

cp -a "$root/config/includes.chroot/." "$tree/"
install -D -m 755 "$root/daemon/target/release/karakulakd" "$tree/usr/bin/karakulakd"
install -D -m 755 "$root/compositor/target/release/karakulak-compositor" "$tree/usr/bin/karakulak-compositor"

# Runtime packages: the image's package list, without firmware (hardware
# specific) and comments.
depends=$(grep -v -E '^\s*(#|$)|^firmware-' "$root/config/package-lists/karakulak.list.chroot" | paste -sd, - | sed 's/,/, /g')

mkdir -p "$tree/DEBIAN"
cat > "$tree/DEBIAN/control" <<CONTROL
Package: karakulak
Version: $version
Architecture: amd64
Maintainer: Karakulak <onurbaydas@users.noreply.github.com>
Depends: $depends
Section: x11
Priority: optional
Homepage: https://github.com/onurbaydas/karakulak
Description: Karakulak: the browser is the desktop
 Karakulak's compositor, system helper, Firefox extensions and settings.
CONTROL
(cd "$tree" && find etc -type f | sed 's|^|/|') > "$tree/DEBIAN/conffiles"

mkdir -p "$out"
dpkg-deb --root-owner-group --build "$tree" "$out/karakulak_${version}_amd64.deb"
rm -rf "$tree"
