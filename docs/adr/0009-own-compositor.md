# 0009. Our own Wayland compositor in Rust

- Status: accepted
- Date: 2026-10-29

## Context

cage shows one application and stretches it across all screens. Karakulak needs
a lock screen that Firefox cannot bypass, one Firefox window per screen, and a
brightness fallback for screens whose hardware control does not work.

## Decision

Write `karakulak-compositor` in Rust with Smithay, starting from its smallvil
and anvil examples. Every window fills a screen (maximized, so Firefox keeps its
toolbar). A screen without a window gets one from Firefox (`--new-window`).
It runs on real screens through DRM/KMS with several GPUs, or as a window inside
another desktop for development (`./dev.sh session`). karakulakd talks to it over
a Unix socket (`KARAKULAK_SOCKET`).

## Consequences

- We own the code that decides what is on screen, which the lock screen needs.
- Smithay's API changes often; the dependency is pinned to a commit.
- The binary is built on Debian in CI so it links against the image's libraries.
