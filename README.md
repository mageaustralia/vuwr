# vuwr

A fast, editable viewer for CSV, JSON and XML. One static binary, a
terminal UI, a native window and a browser build — all over the same core.

It exists because opening a CSV meant launching LibreOffice, which is slow
and silently mangles data on the way through, and checking a JSON or XML
file meant a text editor plus `jq` or `xmllint`.

## What it does

- Opens CSV, TSV, JSON and XML, in a terminal or a window
- **Edits** cells, JSON values, XML attributes and element text, with
  undo/redo
- **Never coerces your data.** CSV values stay strings: `007` stays `007`,
  `1.50` does not become `1.5`, and dates are never reinterpreted. JSON
  keeps its real types, and editing a number leaves it a number.
- **Saves without reformatting.** Key order, indentation, quoting style,
  line endings, XML comments and attribute order all survive, so editing
  one cell gives you a one-line `git diff`.
- Search, filter, marks, frozen columns
- Validates: `vuwr --check` replaces `jq empty` and `xmllint --noout`, and
  covers CSV too

## Install

```sh
cargo build --release      # target/release/vuwr
```

## Use

```sh
vuwr data.csv              # terminal UI
vuwr --gui data.json       # native window
cat data.csv | vuwr        # reads stdin
vuwr data.csv | head       # piped output writes the document through

vuwr --check *.json        # exit 0 valid, 1 invalid, 2 unreadable
vuwr --licenses            # notices for the bundled fonts
```

Press `?` for the keys. The bar along the bottom lists the ones that
matter in the current view.

| | |
|---|---|
| `h j k l` / arrows | move |
| `Space` / `b` | page down / up |
| `1` `2` `3` | table / tree / text view |
| `i` / `c` | edit / replace the cell |
| `u` / `Ctrl-R` | undo / redo |
| `/` `n` `N` | search, next, previous |
| `&` / `r` | filter rows / clear the filter |
| `m` `M` `Ctrl-E` | mark, clear marks, print marked rows and exit |
| `f` | freeze columns left of the cursor |
| `:w` `:q` `:wq` | write, quit, write and quit |

## In the browser

See [`web/README.md`](web/README.md). The same core and GUI compiled to
WebAssembly; files are read in the tab and nothing is uploaded.

## Layout

```
crates/vuwr-core/   documents, edit ops, undo, session — no I/O, wasm-clean
crates/vuwr-tui/    ratatui frontend
crates/vuwr-gui/    egui frontend (native + web)
crates/vuwr-cli/    the binary
crates/vuwr-web/    the browser entry point
```

`vuwr-core` decides all behaviour; the frontends only map input and draw.
That is deliberate — the design and its reasoning are in
[`docs/superpowers/specs/2026-08-24-vuwr-design.md`](docs/superpowers/specs/2026-08-24-vuwr-design.md).

## Licence

MIT OR Apache-2.0.

egui bundles fonts under the SIL Open Font License and the Ubuntu Font
Licence, which require their notices to travel with the software. They are
embedded in the binary: `vuwr --licenses`, or Help → Acknowledgements in
the GUI.
