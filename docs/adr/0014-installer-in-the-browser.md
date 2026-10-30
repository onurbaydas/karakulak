# 0014. Installing from a page in Firefox

- Status: accepted
- Date: 2026-10-31

## Context

Karakulak installed through debian-installer's text screens, the only part
of the system outside the browser. The live system already has everything
the installed one needs: the same packages, plus a few live-only ones
(live-boot, live-config, user-setup) that live-build lists in
`live/filesystem.packages-remove` on the ISO.

The options were:

1. Calamares, as Debian's own live images use. A Qt program: UI outside the
   browser.
2. A page that drives debian-installer. d-i is meant to be driven by its own
   screens or a full preseed; much of it would still run outside our control.
3. Copy the live system's root to the disk ourselves and set it up, as
   Calamares' "unpackfs" does.

## Decision

Option 3. The panel extension's `install.html` asks for a disk, a name, a user
name, a password and a computer name. karakulakd writes the request to a file
only the user can read and starts `/usr/sbin/karakulak-install` (Rust, in the
daemon crate) as root through `sudo systemd-run`: the live user may use sudo
without a password, and as a systemd unit the installer keeps running when
the page closes. Progress goes to `/run/karakulak-install.json`.

The installer erases the whole disk: GPT with a 512 MiB EFI partition (UEFI)
or a 1 MiB BIOS boot partition, the rest ext4. It copies
`/run/live/rootfs/filesystem.squashfs`, purges the live-only packages,
creates the user with the live user's groups, copies language, keyboard,
time zone and Wi-Fi networks from the live system, and points the
auto-login at the new user. GRUB comes from the `-bin` packages for both
firmware types plus `grub-efi-amd64-signed` and `shim-signed` for Secure
Boot; `grub2-common` keeps `update-grub` running on kernel updates.

`tests/disk-install-test.sh` installs to a virtual disk on every build
(kernel option `karakulak.install-test`) and boots the result.

## Consequences

- Installation happens in the browser, and is tested on every build.
- Only whole-disk installs: no dual boot, no encryption, no manual
  partitions. Those need a partition editor page; until then
  debian-installer stays in the boot menu.
- GRUB's own packages (`grub-pc`, `grub-efi-amd64`) are not installed, so
  GRUB itself is not reinstalled to the disk when it is updated; its
  modules and configuration are.
