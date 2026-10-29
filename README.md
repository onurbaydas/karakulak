# Karakulak

**A Linux distribution where the browser is the operating system.**

[Türkçe](README.tr.md)

Karakulak boots straight into Firefox. There is no desktop environment: tabs are
the taskbar, and every tool (file manager, settings, system tray) is a browser
extension.

> *Karakulak* means "black ear" in Turkish. The world knows this animal by the
> name we gave it: the caracal.

## Status

Early development. v0.1 targets an installable ISO that boots into fullscreen Firefox.

## v0.1 scope

- Minimal Debian (trixie) based ISO: install to disk, or try it live first
- Auto-login, then Firefox fullscreen inside [cage](https://github.com/cage-kiosk/cage) (Wayland)
- Firefox from Mozilla's official APT repository
- First-run welcome page in Firefox to choose a privacy level: standard, strict or paranoid
- `userChrome.css` theme with tabs as the taskbar

Later: Rust system daemon with native messaging, system extensions
(file manager and others), custom boot screen.

## Building

Requires a Debian based host (or container) with root access.

```sh
sudo apt install live-build
cargo build --release --manifest-path daemon/Cargo.toml
sudo install -D daemon/target/release/karakulakd config/includes.chroot/usr/bin/karakulakd
lb config
sudo lb build
```

The ISO ends up in the repository root. Every push to `main` also builds an ISO
in GitHub Actions; download it from the run's artifacts.

Test it in a VM:

```sh
qemu-system-x86_64 -enable-kvm -m 4G -cdrom live-image-amd64.hybrid.iso
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Commits must be signed off (DCO).

## License

[GPL-3.0](LICENSE)
