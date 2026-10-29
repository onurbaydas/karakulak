# 0007. Sign Karakulak extensions on AMO

- Status: accepted
- Date: 2026-10-29

## Context

Every Karakulak tool is a Firefox extension. Release Firefox refuses unsigned
extensions, even when installed by enterprise policy. ESR and Developer Edition
allow unsigned ones but lag behind or are less stable.

## Decision

Stay on release Firefox. Sign our extensions on addons.mozilla.org as unlisted
(not published in the store) with `extensions/sign.sh`, commit the signed
`.xpi`, and force-install it through `/etc/firefox/policies/policies.json`.

## Consequences

- Needs a Mozilla account and API key held by the maintainer.
- Every signing needs a new extension version.
- Mozilla reviews can, in rare cases, delay or reject a version.
