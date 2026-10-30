# Building Karakulak's extensions (for add-on reviewers)

The extensions are written in Rust with [Leptos](https://leptos.dev) and
compiled to WebAssembly. `ext/pkg/` in each submitted extension is generated;
everything else in `ext/` is hand-written. The hand-written JavaScript is only
`boot.js` and `background.js`, which load the WebAssembly and register event
listeners.

## Reproduce the build

On Debian 13 or any Linux with a current Rust toolchain:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
./build.sh panel      # or welcome, lock
```

The result is in `<name>/ext/pkg/`: `karakulak_<name>_bg.wasm` and the
`karakulak_<name>.js` glue that wasm-bindgen generates for it.

## Where things are

| Path | What |
|---|---|
| `<name>/src/` | The extension's Rust code |
| `browser/src/` | Shared helpers: calling `browser.*` APIs, `t()` for Turkish/English |
| `<name>/ext/` | Manifest, HTML, CSS, loaders |

`nativeMessaging` talks to `karakulakd` (the `daemon/` folder of the same
repository), Karakulak's system helper installed with the operating system.
