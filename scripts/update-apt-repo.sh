#!/bin/sh
# Adds .deb files to Karakulak's APT repository and signs it.
#
# Usage: scripts/update-apt-repo.sh <folder with .deb files> <repository folder>
# The signing key must be in GnuPG already (CI imports APT_SIGNING_KEY).
# Needs apt-ftparchive (apt-utils) and gpg.
set -e

debs="$1"
repo="$2"
dist=trixie
pool="$repo/pool/main"
binary="$repo/dists/$dist/main/binary-amd64"

mkdir -p "$pool" "$binary"
cp "$debs"/*.deb "$pool/"
# Keep the five newest versions so people can still roll back.
# shellcheck disable=SC2012 # package file names are plain
ls -t "$pool"/karakulak_*.deb 2>/dev/null | tail -n +6 | xargs -r rm -f

(cd "$repo" && apt-ftparchive packages pool > "dists/$dist/main/binary-amd64/Packages")
gzip -kf "$binary/Packages"
apt-ftparchive \
	-o APT::FTPArchive::Release::Origin=Karakulak \
	-o APT::FTPArchive::Release::Label=Karakulak \
	-o APT::FTPArchive::Release::Suite="$dist" \
	-o APT::FTPArchive::Release::Codename="$dist" \
	-o APT::FTPArchive::Release::Architectures=amd64 \
	-o APT::FTPArchive::Release::Components=main \
	release "$repo/dists/$dist" > "$repo/dists/$dist/Release"
rm -f "$repo/dists/$dist/InRelease" "$repo/dists/$dist/Release.gpg"
gpg --batch --yes --clearsign -o "$repo/dists/$dist/InRelease" "$repo/dists/$dist/Release"
gpg --batch --yes -abs -o "$repo/dists/$dist/Release.gpg" "$repo/dists/$dist/Release"
cp "$(dirname "$0")/../config/archives/karakulak-archive-key.asc" "$repo/karakulak-archive-key.asc"
