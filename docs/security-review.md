# Security review (v0.9)

What can reach what, and the trade-offs taken on purpose.

## karakulakd

- **Who can call it:** only Karakulak's own extensions (`allowed_extensions` in
  `/usr/lib/mozilla/native-messaging-hosts/karakulakd.json`: panel, lock).
  Web pages cannot use native messaging. It runs as the logged-in user, so it
  can do nothing that user could not do in a terminal.
- **Guest sessions:** `KARAKULAK_GUEST=1` limits it to an allowlist
  (`GUEST_ALLOWED` in `daemon/src/main.rs`); new commands are closed to guests
  until added there.
- **Inputs checked before use:**
  - D-Bus object paths (Bluetooth `/org/bluez/`, udisks `block_devices/`),
    NetworkManager connection UUIDs, PipeWire device names: checked shape.
  - New file and folder names: no `/`, `..`, NUL; copies never overwrite and
    refuse to put a folder inside itself.
  - Language codes: limited characters; keyboard layout and time zone are
    validated by localed and timedated.
  - Stopping programs: never PID 1 or karakulakd itself; the kernel refuses
    other users' programs.
  - Restoring a backup: the archive is listed first and must only hold
    `.karakulak/` and `.config/` entries without `..`.
  - WireGuard files: written to a 0700 folder under `XDG_RUNTIME_DIR` and
    removed after `nmcli` imports them; the key never appears on a command line.
- **Secrets on command lines:** none. Wi-Fi passwords go over D-Bus; AMO keys
  are passed to `web-ext` in the environment.

## Compositor

- **Control socket** (`$XDG_RUNTIME_DIR/karakulak-<wayland socket>.sock`): any
  process of the user can connect. It can lock, set brightness, screens, night
  light and start or end a guest session. Unlocking needs the right password,
  checked with PAM (`/etc/pam.d/karakulak`, with `pam_faildelay` from
  common-auth slowing down guessing).
- **Lock screen:** only windows with app id `karakulak-lock` are shown and get
  input while locked. Escaping the lock page leads to an empty kiosk profile.

## polkit (`/etc/polkit-1/rules.d/50-karakulak.rules`)

For a local, active user in group `sudo`, without a password: language,
keyboard, time zone, starting `karakulak-update.service`, starting and stopping
`nftables.service` and `tor.service`. Accepted trade-off (ADR 0011): software
running as that user can switch the firewall or Tor off until the next boot.

## Dependencies

`cargo audit` runs on every build and fails it on a known vulnerability.
The DDC/CI code moved off `ddc-hi`, whose `mccs-db` pulled in a vulnerable
`serde_yaml`. zbus 5's helper crates are unmaintained (issue #3); not a
vulnerability.
