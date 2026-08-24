# vuwr on the web

The same core and the same GUI as the desktop build, compiled to
WebAssembly and rendered to a canvas. There is no browser-specific
behaviour: `vuwr-web` is only an entry point.

Files are read in the tab. Nothing is uploaded, and there is no server
component — the page is static.

## Build

Requires the `wasm32-unknown-unknown` target and a `wasm-bindgen` CLI
whose version matches the `wasm-bindgen` crate pinned in
`crates/vuwr-web/Cargo.toml`. They must match exactly or the browser
rejects the generated bindings.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.122   # only if you lack it

./web/build.sh
```

## Run

Module scripts need a real origin, so `file://` will not work:

```sh
python3 -m http.server -d web 8080
# then open http://localhost:8080
```

## Gotchas paid for already

**Do not set `default-features = false` on `eframe`.** It builds, renders,
and responds to the mouse — but keyboard input never reaches the app, and
nothing reports an error. Menus open on click while Escape does not close
them, which looks like an application bug and is not one.

**The browser caches `vuwr_bg.wasm` hard.** `index.html` passes an explicit
cache-busted URL to `init()` for that reason. Without it a rebuilt bundle
is indistinguishable from a code change that did not take, which is a
genuinely confusing hour.

## Size

The bundle is around 4 MB, nearly all of it egui's renderer and the
bundled fonts. `wasm-opt -Oz` takes a further chunk off if you have
binaryen installed; it is not required to build.
