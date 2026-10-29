# 0002. Firefox from Mozilla, privacy chosen at first run

- Status: accepted
- Date: 2026-10-29

## Context

Chromium's Manifest V3 limits content blockers. Firefox keeps blocking
`webRequest` and supports `userChrome.css`. Hardened forks (LibreWolf, Mullvad
Browser) bake in one privacy profile and lag upstream releases.

## Decision

Ship Firefox from Mozilla's official APT repository. On first run, a built-in
welcome extension asks for a privacy level (standard, strict, paranoid) and
applies it through the `browser.privacy` API. Telemetry and studies are off at
every level via enterprise policy.

## Consequences

- Fast Firefox updates, independent of Debian's ESR cadence.
- We maintain our own privacy presets.
