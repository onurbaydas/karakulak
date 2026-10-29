# Contributing to Karakulak

Thanks for your interest! Issues and pull requests are welcome in English or Turkish.

## Developer Certificate of Origin

Karakulak uses the [Developer Certificate of Origin](https://developercertificate.org/)
instead of a CLA. Sign off every commit to certify you have the right to submit it
under the project's license (GPL-3.0):

```sh
git commit -s -m "Describe your change"
```

This adds a line like `Signed-off-by: Your Name <you@example.com>`. A CI check
rejects pull requests with unsigned commits. To fix older commits:

```sh
git rebase --signoff main
```

## Workflow

1. Open an issue first for anything larger than a small fix.
2. Keep pull requests focused on one change.
3. Describe how you tested it (VM, real hardware).
4. Significant design decisions get an ADR in [docs/adr/](docs/adr/).

## Extensions

Everything is Rust. Extension UIs use [Leptos](https://leptos.dev) compiled to
WebAssembly. Each extension is a crate in `extensions/<name>/`; its `ext/`
folder holds the manifest, HTML shell, CSS and a two-line loader.

Setup once:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```

Try an extension in Firefox (builds everything, opens a separate dev profile):

```sh
./dev.sh          # panel
./dev.sh welcome
./dev.sh session   # whole session: compositor + fullscreen Firefox + panel
```

Release Firefox only installs signed extensions, so the image ships signed
`.xpi` files from `config/includes.chroot/usr/share/karakulak/extensions/`.
To release a change, bump `version` in `extensions/<name>/ext/manifest.json`.
On every push to `main`, CI signs changed extensions and commits the signed
`.xpi`. Versions held for manual review by Mozilla are picked up on a later run.
Maintainers can do the same locally with `extensions/sign.sh`.

Extensions that talk to the system need `karakulakd`. Register a local build
with your Firefox first:

```sh
daemon/dev-install.sh
```

## Project layout

| Path | What |
|---|---|
| `auto/` | live-build entry scripts (`lb config`, `lb build`, `lb clean`) |
| `config/package-lists/` | Debian packages in the image |
| `config/archives/` | Extra APT repositories (Mozilla) |
| `config/includes.chroot/` | Files copied into the image as-is |
| `config/includes.installer/` | Debian installer preseed |
| `extensions/` | Karakulak's own Firefox extensions (Rust + Leptos): welcome, panel, lock |
| `compositor/` | Karakulak's Wayland compositor (Rust, Smithay) |
| `daemon/` | `karakulakd`, the Rust helper extensions talk to via native messaging |
| `docs/adr/` | Architecture decision records |
