# 0012. Karakulak's own APT repository

- Status: accepted
- Date: 2026-10-30

## Context

The compositor, karakulakd, the extensions and Karakulak's settings only
reached people inside a new ISO. Installed systems need fixes too.

## Decision

Everything Karakulak adds to Debian is one package, `karakulak`
(`scripts/build-deb.sh`), installed in the image and published in a signed
APT repository on GitHub Pages (`https://onurbaydas.github.io/karakulak/apt`,
branch `gh-pages`, `scripts/update-apt-repo.sh`). Images trust it through
`/etc/apt/sources.list.d/karakulak.sources` with the key in
`/usr/share/keyrings/karakulak-archive-keyring.asc`. A version tag (`v0.9.0`)
makes CI publish the package and a GitHub release with the ISO,
`SHA256SUMS` and its signature. Builds from `main` carry a version below any
release (`0.0.<date>~<commit>`), so installed systems move to releases.

## Consequences

- "Update now" and the daily unattended upgrades bring Karakulak's updates.
- The signing key (ed25519) lives in the `APT_SIGNING_KEY` secret and the
  maintainer's backup; losing it means shipping a new key in an update first.
- Files under `/etc` in the package are conffiles: local edits are kept on
  upgrade (`--force-confold`).
