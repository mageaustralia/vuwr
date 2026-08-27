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
- Search, filter, marks, frozen columns, resizable columns, and a
  **Columns** list for putting the ones you are not reading away
- **Find and replace**, with `$1` for what the pattern captured, so
  `(\d+)mm` → `${1} mm` is one replacement rather than a hundred edits.
  Step through the matches or take them all at once, as a single undo. A
  filter narrows what is replaced — usually the point, and said plainly
  beside the prompt while you type, not discovered afterwards.
- **Clickable links.** A value that is a whole URL is drawn underlined and
  opens in a new tab: a plain click in the tree and in the inspector,
  Cmd-click in the table (Ctrl elsewhere), where a plain click has to go
  on selecting the cell. A value that merely mentions an address is not a
  link — following a click on a description would be a surprise. View →
  Clickable links turns it off.
- An **inspector** beside the table: the whole record read downwards, for
  when a row is twenty-three columns wide and the window shows five
- Light and dark in the window, under View → Appearance
- Colour schemes in the terminal, for the file's own text: Gruvbox,
  Solarized, Nord, Monokai, or ours. `:scheme gruvbox-dark`, or `:scheme`
  to list them; `:theme` does the same. Tab at the `:` prompt walks the
  candidates — the commands while you are typing one, then its argument.
  A named scheme brings its own background, so it reads the same whatever
  your terminal is set to
- Validates: `vuwr --check` replaces `jq empty` and `xmllint --noout`, and
  covers CSV too. It also reports what a parser lets through — a trailing
  comma (which vuwr itself reads, so the file can be opened and fixed),
  duplicate JSON keys, a value that disagrees with its column — a price
  column of numbers with one `129,00` in it — and an entity escaped twice
  over, where `&amp;amp;` in the source means the value holds the five
  characters `&amp;` and a URL written that way reaches its consumer with
  its query string broken. It reports those and
  never rewrites them: the fix is a guess about what somebody meant.
  `--strict` makes warnings fail too.
- A **lint** button in the window, and `:lint` in the terminal. Asked for
  rather than run as you type: the scan re-reads the whole document, which
  is 150 ms on a 15 MB file — a hitch after every edit, paid whether or
  not anybody was looking.


## See it

The terminal UI on a sheet — moving, resizing a column, sorting,
filtering, editing a cell in place:

![vuwr in the terminal, on a CSV](docs/media/tui-table.gif)

The same binary on XML: the tree, the table it makes of a feed, and the
text view with the value under the cursor marked out:

![vuwr in the terminal, on an XML feed](docs/media/tui-tree.gif)

The same file as source, coloured by grammar and re-indented in place:

![vuwr's text view in the terminal](docs/media/tui-text.gif)

The window build — the identical core and views, compiled to WebAssembly
and running in a browser tab:

![vuwr in a browser tab](docs/media/web.gif)

Try it without installing anything: **[the hosted build](https://mageaustralia.github.io/vuwr/?sample)**
(`?sample=csv` and `?sample=json` open the other two). Files are read in
the tab; nothing is uploaded.

## Install

Download a binary from [Releases](https://github.com/mageaustralia/vuwr/releases)
— macOS (Apple silicon or Intel), Windows, Linux — unpack it and put
`vuwr` on your `PATH`.

They are not code-signed, so the first run needs a word from you: on macOS
`xattr -d com.apple.quarantine ./vuwr`, and on Windows *More info* →
*Run anyway* at the SmartScreen prompt. There is a `.sha256` beside each
archive if you would rather check than trust.

Or build it:

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
| `%` `.` `a` | find and replace: set up, take this one, take the rest |
| `m` `M` `Ctrl-E` | mark, clear marks, print marked rows and exit |
| `f` | freeze columns left of the cursor |
| `<` `>` `=` | narrow, widen, re-fit the column (or drag its edge) |
| `:lint` | check for problems a parser lets through |
| `:w` `:q` `:wq` | write, quit, write and quit |

## Sample files

`examples/` holds one of each format — a product feed, a stock sheet and a
config file — so there is something to open on a fresh clone:

```sh
vuwr examples/products.xml
vuwr --gui examples/stock.csv
```

## In Chrome

`extension/` is the same wasm as a Chrome extension: navigate to a `.csv`,
`.tsv`, `.json` or `.xml` URL and it opens in vuwr instead of the
browser's own viewer. `./extension/build.sh`, then chrome://extensions →
Developer mode → Load unpacked.

An extension rather than a bookmarklet because the file has to be read
from a page that is not its own origin, and almost nothing on the web
sends the CORS headers that would allow it. An extension page carries host
permissions, so it can — and it sends your cookies, so a feed behind a
login works like any other. vuwr still fetches nothing: it is handed the
bytes. The toolbar button turns the redirect off when you want the file
itself.

## In the browser

See [`web/README.md`](web/README.md). The same core and GUI compiled to
WebAssembly; files are read in the tab and nothing is uploaded. Every push
to `main` deploys it to GitHub Pages.

## Using the core as a library

`vuwr-core` is the whole application minus the drawing, and it is usable
on its own. It performs no I/O — bytes in, bytes out — has one dependency
(`regex`), forbids `unsafe`, and is checked against
`wasm32-unknown-unknown` on every push, so it runs anywhere Rust does.

```rust
use vuwr_core::{Document, FormatHint};

let mut doc = Document::parse(bytes, FormatHint::Auto)?;
doc.set_cell(0, 2, "42")?;          // undoable
let out = doc.serialize();           // untouched bytes stay byte-identical
```

Also worth taking on their own: `highlight()` for syntax spans,
`scan_json()` for lint diagnostics, `natural_cmp`/`sort_rows`, and
`decode`/`encode` for XML entities. `Session` + `Command` + `Effect` is
the frontend-agnostic application itself — the TUI is a 132-line wrapper
around it.

## Layout

```
crates/vuwr-core/   documents, edit ops, undo, session — no I/O, wasm-clean
crates/vuwr-tui/    ratatui frontend
crates/vuwr-gui/    egui frontend (native + web)
crates/vuwr-cli/    the binary
crates/vuwr-web/    the browser entry point
```

`vuwr-core` decides all behaviour; the frontends only map input and draw.
That is deliberate: it is what lets the same views run in a terminal, a
window and a browser tab, and what keeps the two frontends from drifting
apart in behaviour.

## Licence

MIT OR Apache-2.0.

egui bundles fonts under the SIL Open Font License and the Ubuntu Font
Licence, which require their notices to travel with the software. They are
embedded in the binary: `vuwr --licenses`, or Help → Acknowledgements in
the GUI.
