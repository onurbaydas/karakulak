# 0006. Installable system, live mode as a bonus

- Status: accepted
- Date: 2026-10-29

## Context

Karakulak is meant to be installed and used daily. A live session is still
useful for trying it out.

## Decision

Build the ISO with `live-build --debian-installer live`. The boot menu offers
both "Live" and "Install"; the installer copies the live system to disk. A
preseed sets Turkish defaults and leaves partitioning, user and password
interactive.

## Consequences

- One ISO, one build, two modes.
- The installed system auto-logs in the user created during installation; the
  preseed rewrites the getty autologin entry for that user.
- The privacy level is asked once, on first start after installation (every
  boot in live mode).
