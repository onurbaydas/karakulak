# 0013. Screen sharing through wlr-screencopy and xdg-desktop-portal-wlr

- Status: accepted
- Date: 2026-10-30

## Context

Video call sites (Meet, Zoom, Discord) share a screen through WebRTC. On
Wayland, Firefox asks xdg-desktop-portal for a PipeWire stream, and a
portal backend that knows the compositor fills it. Karakulak's compositor
had no way to hand out its screens.

The options were:

1. Our own portal backend with the PipeWire stream inside the compositor
   (like niri or GNOME). It is the most efficient path (GPU buffers, no
   copies), but means a PipeWire dependency and a lot of new code.
2. The `ext-image-copy-capture` protocol, which Smithay supports. Debian 13's
   xdg-desktop-portal-wlr (0.7.1) does not speak it yet.
3. The `wlr-screencopy-unstable-v1` protocol, which xdg-desktop-portal-wlr
   0.7.1 uses.

## Decision

The compositor implements `wlr-screencopy-unstable-v1` (version 3) with
shared-memory buffers (`compositor/src/screencopy.rs`). Each requested frame
is drawn once more into memory, the same way as a screenshot, and copied
into the client's buffer. The image ships xdg-desktop-portal and
xdg-desktop-portal-wlr; `XDG_CURRENT_DESKTOP=Karakulak` selects them via
`/usr/share/xdg-desktop-portal/karakulak-portals.conf`. The compositor puts
`WAYLAND_DISPLAY` into the D-Bus activation environment so the portal finds
it.

## Consequences

- Screen sharing works with Debian's packages and little code.
- Every shared frame is read back to the CPU; the portal is limited to
  30 frames per second to keep that cheap.
- There is no screen picker yet: the portal shares the first screen.
- When Debian ships xdg-desktop-portal-wlr 0.8, moving to
  `ext-image-copy-capture` (and GPU buffers) needs no changes outside the
  compositor.
