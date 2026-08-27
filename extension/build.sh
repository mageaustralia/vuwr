#!/usr/bin/env bash
# Build the browser bundle and put it beside the extension's own files.
#
# The extension runs the same wasm the hosted page does; only the way in
# differs. Copied rather than linked, because Chrome loads an unpacked
# extension by reading the directory.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"

"$root/web/build.sh"

# Copied over the top rather than cleared first: the bundle is three
# known files, and nothing here is worth a recursive delete.
mkdir -p "$here/dist"
cp "$root/web/dist/vuwr.js" "$root/web/dist/vuwr_bg.wasm" \
   "$root/web/dist/build.txt" "$here/dist/"

# Keep the version in step with the crate's, so an installed extension
# says which vuwr it is.
version=$(grep -m1 '^version' "$root/Cargo.toml" | cut -d'"' -f2)
python3 - "$here/manifest.json" "$version" <<'PY'
import json, sys
path, version = sys.argv[1], sys.argv[2]
manifest = json.load(open(path))
manifest["version"] = version
json.dump(manifest, open(path, "w"), indent=2)
open(path, "a").write("\n")
PY

echo "extension ready ($version, build $(cat "$here/dist/build.txt"))"
echo "load it: chrome://extensions → Developer mode → Load unpacked → $here"
