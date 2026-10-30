# Changelog

What changed in each version, for people using Karakulak. The roadmap with
what is still to come is in [ROADMAP.md](ROADMAP.md) (Turkish).

## 0.9.1

- CI installs the package on Debian before building the ISO, so clashing files
  fail within minutes.
- The boot test checks that the panel extension talks to karakulakd.
- Extensions are signed only for releases (Mozilla rate-limits uploads).
- Firewall rules moved to `/etc/karakulak/nftables.conf`; automatic update
  settings to `52karakulak-auto-upgrades` (both clashed with Debian's files).
- GitHub Actions updated to their current versions.

## 0.9.0: Release readiness

- Updates for installed systems: the `karakulak` package and a signed APT
  repository at <https://onurbaydas.github.io/karakulak/apt>.
- Releases carry the ISO, `SHA256SUMS` and its signature.
- Dependency audit on every build; DDC/CI no longer pulls in a vulnerable
  YAML parser. See [docs/security-review.md](docs/security-review.md).
- Help page, problem report (a local file, nothing is sent), text size and
  high contrast.
- Smaller image; Intel Wi-Fi, Realtek, Atheros and laptop sound firmware added.

## 0.8.0: Control and privacy

- Task manager, notifications (USB, updates, battery, downloads), keyboard
  shortcuts page (Super+/), file search and image preview.
- Guest session: an empty Firefox in memory, deleted at the end.
- Browse through Tor; backup and restore of the Firefox profile and settings.

## 0.7.0: Privacy and hardware

- Brightness through the backlight or the monitor (DDC/CI) first, GPU gamma
  otherwise; screens power off when idle; locking before sleep is guaranteed.
- Per-network random Wi-Fi addresses, WireGuard VPN, sound device selection,
  fractional scaling, English interface, magnifier, Bluetooth pairing codes.

## 0.6.0: Daily use and testing

- `vm.sh`, a boot test in CI, volume and brightness keys, touchpad tapping,
  idle dimming and locking, low battery warning, screenshots, night light,
  recommended extensions.

## 0.5.0: Settings

- Language, keyboard, time zone, privacy level, updates, firewall, printers.

## 0.4.0: Look

- Windows side by side (Super+arrows), theme with the tab bar at the bottom,
  new tab page, logo and boot splash.

## 0.3.0: Files

- File manager with USB sticks, terminal, screen resolution and scale.

## 0.2.0: Controlling the computer

- karakulakd, the system panel, Karakulak's own Rust compositor with a lock
  screen and one Firefox window per screen.

## 0.1.0: It boots

- Debian-based ISO, installable or live, that starts straight into Firefox;
  first-run privacy choice.
