# 0011. System settings without password dialogs

- Status: accepted
- Date: 2026-10-30

## Context

Changing the language, time zone, keyboard, running updates or toggling the
firewall needs privileges. Desktops show a polkit password dialog for that;
Karakulak has no UI outside the browser to show one.

## Decision

karakulakd uses systemd's services (localed, timedated, systemd units) over
D-Bus. A polkit rule grants exactly these actions, without a password, to a
local, active user in group `sudo`: locale and keyboard, time zone, starting
`karakulak-update.service`, starting/stopping `nftables.service`. Updates run
as root inside that unit (`apt-get update && full-upgrade`); security updates
also install daily through unattended-upgrades.

## Consequences

- Someone at an unlocked session can change these few settings; everything
  else still needs a password (and a terminal).
- Turning the firewall off lasts until the next boot.
