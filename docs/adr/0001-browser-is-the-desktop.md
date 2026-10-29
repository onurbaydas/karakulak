# 0001. The browser is the desktop

- Status: accepted
- Date: 2026-10-29

## Context

A modern browser with extensions already covers most of what users do. Other
browser-centric systems (ChromeOS) are tied to one vendor; privacy distributions
(Tails) still ship a full desktop.

## Decision

Karakulak has no desktop environment. Firefox runs fullscreen and every system
tool is a browser extension.

## Consequences

- Extensions need a bridge for system access (see a future ADR on the daemon).
- Native GUI applications are out of scope.
