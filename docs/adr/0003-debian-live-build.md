# 0003. Minimal Debian base built with live-build

- Status: accepted
- Date: 2026-10-29

## Context

Building from scratch (LFS) is a large ongoing maintenance cost. Karakulak's value
is the browser-centric experience, not the base system.

## Decision

Use Debian stable (trixie) with only the packages Karakulak needs, built into a
live ISO with `live-build`. Firefox comes from Mozilla, not Debian.

## Consequences

- Stable, security-maintained base with little effort.
- Debian's slower package cadence matters less since Firefox is separate.
