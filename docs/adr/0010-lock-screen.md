# 0010. Lock screen held by the compositor

- Status: accepted
- Date: 2026-10-30

## Context

Everything the user sees should be a browser page, including the lock screen.
A page alone cannot lock anything: Ctrl+T or closing the tab gets past it.

## Decision

The compositor holds the lock. While locked it unmaps every window and shows
only windows with app id `karakulak-lock`: a second Firefox started with
`--kiosk --name karakulak-lock` and an empty profile, showing the lock
extension's page at a fixed extension UUID. The page closes any other tab.
The password goes page → karakulakd → compositor, which checks it with PAM
(service `karakulak`, hand-written FFI) on a worker thread. The compositor
locks at session start, on logind's PrepareForSleep and on request.

## Consequences

- Escaping the lock page leads to an empty browser profile, not the session.
- Any process of the user may ask to lock; only the right password unlocks.
- Locking before sleep is not yet backed by a logind delay inhibitor, so the
  screen may show for a moment on resume.
