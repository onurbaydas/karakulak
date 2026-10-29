# 0004. cage as the session compositor

- Status: accepted
- Date: 2026-10-29

## Context

Firefox needs a display server. A full compositor or desktop adds surface we
do not use.

## Decision

Auto-login on tty1 and run Firefox inside `cage`, a Wayland kiosk compositor
that shows one fullscreen application.

## Consequences

- Minimal graphics stack.
- Firefox popup windows and multiple windows behave like kiosk mode; revisit if
  that becomes limiting.
