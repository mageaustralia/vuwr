# vuwr — a fast, editable viewer for CSV / JSON / XML

**Date:** 2026-08-24
**Status:** Design approved, not yet implemented
**Name:** `vuwr` — free on crates.io, npm, and as a shell binary. `dv` is taken on crates.io.

## Problem

Opening a CSV means launching LibreOffice, which is slow, and which silently
mangles data on the way in and out (leading zeros stripped, numeric-looking
strings reformatted, dates reinterpreted). Opening JSON or XML means a text
editor plus a separate `jq` or `xmllint` invocation to check validity. There is
no single fast tool that opens all three, lets you make a quick edit, and writes
the file back without rewriting parts you did not touch.

VisiData is the closest existing answer and the model for the interaction
design, but it is Python (not a single binary) and GPLv3.

## Goals

- Open CSV, JSON and XML from one static binary with no runtime dependencies
- Browse and reshape: navigate, search, filter, sort, reorder/hide columns
- **Edit and save**: cells, rows, columns, with undo/redo
- **Never corrupt data**: no silent type coercion; save preserves formatting
- Terminal UI and native GUI from the same binary
- Built-in validation, replacing `jq empty` and `xmllint --noout`

## Non-Goals

- Multi-GB files. The in-memory model caps useful size in the low hundreds of MB.
- Formulas, charts, or anything spreadsheet-like beyond a data grid.
- Being a paid product at launch. See Licensing for how that option is kept open.
- Two-pane compare/diff. Useful, but a separate tool.

## Decisions

### Language: Rust

Rust over Nim and Go. The project depends on `ratatui`, `egui`/`eframe`,
`csv`, and `quick-xml`; in Nim or Go, several of these would have to be written
from scratch. Rust 1.96 is installed; there is no Go toolchain on the machine.

### Architecture: pure core, three frontends

```
vuwr/
├── crates/
│   ├── vuwr-core/     document model, loaders, edit ops, undo, view state   [native + wasm]
│   ├── vuwr-tui/      ratatui frontend                                       [native]
│   ├── vuwr-gui/      eframe/egui frontend                                   [native + wasm]
│   └── vuwr-cli/      arg parsing, filesystem, frontend selection            [native]
└── (vuwr-web/         trunk shell — later phase)                             [wasm]
```

**`vuwr-core` performs no I/O.** It takes bytes and returns bytes:

```rust
Document::parse(bytes: &[u8], hint: FormatHint) -> Result<Document>
document.apply(op: EditOp) -> Result<()>
document.undo() / document.redo()
document.serialize() -> Vec<u8>
```

Filesystem access lives in the frontends: `std::fs` on native, File System
Access API / drag-drop / download blob on web. This boundary is what makes the
core portable to wasm and to a future native mobile UI, and it makes the core
testable without touching disk.

**Consequence:** lazy streaming of a file from disk can never live inside the
core. Given the in-memory decision below, this costs nothing now, but it closes
the door on a multi-GB tier without a core redesign.

### Frontend selection

`--gui` and `--tui` force a frontend. Otherwise: stdout is a TTY → TUI, else
GUI. So `vuwr file.csv` in a terminal is the TUI, and launching from Finder
opens a window. Expected binary size ~20–25MB, statically linked.

### Document model

One `Node` tree — scalar, list, map — carrying source-fidelity metadata
(original quoting, indentation, key order, XML comments and attribute order).
CSV is the degenerate case: a list of maps over a fixed key set. This is why one
table widget renders all three formats without knowing which was loaded.

### View modes

Three modes over any document, cycled by key, with per-format defaults:

| Format  | Default | Also available                        |
|---------|---------|---------------------------------------|
| CSV/TSV | table   | text                                  |
| JSON    | tree    | table (array-of-objects), text        |
| XML     | tree    | table (repeated sibling elements), text |

Table mode is enabled for JSON/XML only when the shape fits — an array whose
elements are objects. Otherwise the toggle is greyed out.

Tree navigation uses drill-down: a nested value renders as a summary cell
(`{city,zip}`, `[3]`), Enter descends into it as a new sheet, Esc pops back.

### No type inference on CSV

CSV values are strings, always. `007` stays `007`; `1.50` does not become
`1.5`; date-like strings are never reformatted. Sorting prompts for numeric vs
lexical rather than guessing. JSON and XML retain their native types.

This is the single most important data-integrity rule in the project and the
main reason to use `vuwr` over a spreadsheet.

### Editing

Every mutation is an `EditOp` (`SetCell`, `InsertRow`, `DeleteRow`,
`InsertColumn`, `DeleteColumn`, `RenameKey`, `MoveColumn`, …) with a computed
inverse, pushed onto an undo stack. Because all mutation funnels through one
enum, the GUI cannot diverge from the TUI in what it permits, and undo/redo is
free.

### Save: preserve formatting

Editing one cell produces a one-line git diff. Key order, indent style, quoting
style, line endings, trailing newline, XML comments, attribute order and the XML
declaration are all sniffed from the input and reproduced on save. This rules
out a naive `serde_json::Value` round-trip; the document model carries the
formatting metadata itself.

### One command layer

Every action is a `Command` in `vuwr-core`. Keybindings map to commands, GUI
menu items map to the same commands, and the `:` palette lists them. A future
native mobile UI binds to the same layer. `?` shows a help overlay generated
from the command list, so it cannot fall out of date.

### Grid view-state lives in core

Cursor position, selection ranges, column order and widths, scroll offset, and
the drill-down sheet stack live in `vuwr-core`, not in the egui or ratatui
widget. These carry the fiddly edge cases; a native tablet UI should inherit
them rather than reimplement them.

### Validation

Text mode parses on idle (~150ms debounce) and marks errors inline with line and
column, plus a gutter marker and a status-bar message. A headless
`vuwr --check <file>` exits non-zero on invalid input, replacing `jq empty` and
`xmllint --noout` in scripts and CI. CSV validation covers ragged rows, encoding
problems, and BOM detection.

## Portability

### WebAssembly

`eframe` targets `wasm32-unknown-unknown` and renders to a canvas, so `vuwr-gui`
becomes the web build with little extra work. Ratatui does not run in a browser.
The shape is: TUI native-only, GUI native and web, core everywhere.

Dependency hygiene is enforced in CI from the first commit:

```sh
cargo check -p vuwr-core --target wasm32-unknown-unknown
```

Three standing traps in `vuwr-core`: no `memmap2`, no `std::time::Instant` (use
`web-time`), no `rayon` or threads.

Deferred to a later phase: the `vuwr-web` crate, trunk setup, browser file
access.

### Tablet

Not planned, but deliberately not foreclosed. The route is not "make egui feel
native on iPad" — egui draws its own widgets, so IME/on-screen-keyboard
integration, scroll physics, context menus and multi-touch gestures are all
weaker than platform-native. Instead:

```
vuwr-core (pure Rust)
   ├─ UniFFI / C ABI → SwiftUI on iPadOS
   ├─ UniFFI / C ABI → Compose on Android
   ├─ egui              → desktop + web
   └─ ratatui           → terminal
```

All parsing, format-preserving save, edit ops, undo and grid view-state ship to
every platform; only the view layer is rewritten. Tauri v2 (MIT/Apache, mobile
support) is the alternative route, and the wasm work feeds directly into it.

## Licensing

All proposed dependencies are permissive — `ratatui` (MIT), `egui`/`eframe`/
`wgpu` (MIT OR Apache-2.0), `winit` (Apache-2.0), `csv` (Unlicense OR MIT),
`quick-xml` (MIT), `serde`/`serde_json`, `clap`, `web-time` (MIT OR Apache-2.0).
Obligation is attribution only: bundle license texts in an Acknowledgements
screen.

**Rule: no VisiData source, ever.** VisiData is GPLv3. Reimplementing its
interaction design is legal — interfaces and ideas are not copyrightable — but
copying or line-by-line porting its code would relicense this project. The same
applies to vendoring from any copyleft tool.

Enforced in CI from the first commit via `cargo-deny` with a license allowlist
(`MIT`, `Apache-2.0`, `Unlicense`, `BSD-*`), so a copyleft dependency fails the
build the day it is added.

**Copyright ownership.** To keep a future paid build possible, either require a
CLA from outside contributors or accept that permissive licensing lets others
ship it too. Permissive licensing does not prevent selling a packaged build; it
only removes exclusivity. Paid mobile apps sell convenience and distribution,
not secrecy.

## Testing

The central property, because format preservation is the promise most easily
broken:

- **Round-trip:** for a golden corpus of real-world files,
  `parse(bytes).serialize() == bytes`, byte for byte.
- **Undo:** `apply(op)` then `undo()` restores the original bytes exactly.
- **Property tests** (`proptest`) over generated documents for both of the above.
- **TUI snapshots** via `insta` over ratatui's `TestBackend`.
- **GUI:** minimal direct testing; correctness lives in the core.

CI matrix: native build + test, `wasm32-unknown-unknown` check of `vuwr-core`,
`cargo-deny`, clippy, rustfmt.

## Build order

| Phase | Deliverable |
|-------|-------------|
| 0 | Workspace skeleton + CI (wasm check, cargo-deny, clippy, fmt) |
| 1 | `vuwr-core`: CSV parse, preserving serialize, EditOps, undo, round-trip corpus |
| **2** | **`vuwr-tui` table mode: navigate, edit cell, save — first usable tool** |
| 3 | JSON: loader, tree view, drill-down, table-when-shape-fits |
| 4 | XML: loader with comment and attribute-order preservation |
| 5 | Text mode, live lint, `vuwr --check` |
| 6 | `vuwr-gui`: egui frontend, native |
| 7 | wasm build |

Deferred beyond phase 7: tablet UI, compare/diff, plugin loaders, parquet /
sqlite / xlsx.

## Open questions

1. **License choice** — MIT OR Apache-2.0 (Rust convention) versus something
   source-available, in light of the tablet option above.
2. **Keybinding scheme** — vim-flavoured with arrow keys always working is the
   assumption; the concrete map is settled at the start of phase 2.
