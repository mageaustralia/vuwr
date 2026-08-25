#!/usr/bin/env bash
# Build the WebAssembly bundle into web/dist.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

# Keep the CLI and the crate in lockstep: wasm-bindgen rejects a mismatch
# at load time, which is a confusing way to find out.
crate_version="$(cargo tree -p vuwr-web --target wasm32-unknown-unknown 2>/dev/null \
  | grep -m1 -o 'wasm-bindgen v[0-9.]*' | cut -d'v' -f2)"
cli_version="$(wasm-bindgen --version | awk '{print $2}')"
if [[ "$crate_version" != "$cli_version" ]]; then
  echo "wasm-bindgen mismatch: crate $crate_version, CLI $cli_version" >&2
  echo "install the matching CLI: cargo install wasm-bindgen-cli --version $crate_version" >&2
  exit 1
fi

cargo build -p vuwr-web --target wasm32-unknown-unknown --release
wasm-bindgen --target web --no-typescript \
  --out-dir web/dist --out-name vuwr \
  target/wasm32-unknown-unknown/release/vuwr_web.wasm

if command -v wasm-opt >/dev/null; then
  # wasm-bindgen emits a module using proposals binaryen will not accept
  # unless told to — without these it stops at "error validating input".
  # Written to a temporary file and moved only on success, so a failure
  # cannot leave a half-written module behind.
  if wasm-opt -Oz \
      --enable-bulk-memory \
      --enable-mutable-globals \
      --enable-nontrapping-float-to-int \
      --enable-reference-types \
      --enable-sign-ext \
      --enable-simd \
      --enable-multivalue \
      web/dist/vuwr_bg.wasm -o web/dist/vuwr_bg.opt.wasm
  then
    mv web/dist/vuwr_bg.opt.wasm web/dist/vuwr_bg.wasm
    echo "optimised with wasm-opt"
  else
    rm -f web/dist/vuwr_bg.opt.wasm
    echo "wasm-opt failed; shipping the unoptimised module" >&2
  fi
fi

# The id index.html stamps both files with: the module's own content, so
# a browser keeps them until they actually change and never pairs a new
# one with an old one.
shasum -a 256 web/dist/vuwr_bg.wasm | cut -c1-12 > web/dist/build.txt

echo "built web/dist ($(du -h web/dist/vuwr_bg.wasm | cut -f1), build $(cat web/dist/build.txt))"
