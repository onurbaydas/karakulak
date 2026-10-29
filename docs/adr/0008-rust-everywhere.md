# 0008. Rust everywhere, extension UIs with Leptos

- Status: accepted
- Date: 2026-10-29

## Context

Firefox extensions run HTML, CSS and JavaScript. We want one language for the
whole system.

## Decision

Write extension logic and UI in Rust with Leptos, compiled to WebAssembly with
`wasm-pack`. Only a manifest, an HTML shell, CSS and a minimal JS loader stay
outside Rust. `karakulakd` is Rust too.

## Consequences

- One language and toolchain; shared code in `extensions/browser`.
- Builds need the `wasm32-unknown-unknown` target and `wasm-pack`.
- Extension pages need `'wasm-unsafe-eval'` in their CSP.
- A background script must register its listeners in JS (a few lines) because
  Firefox only wakes it for synchronously registered listeners.
- AMO may ask for source code since `.wasm` is compiled; the repo is public.
