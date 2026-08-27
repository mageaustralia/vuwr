# vuwr as a Chrome extension

Opens a CSV, TSV, JSON or XML file in vuwr when you navigate to one,
instead of the browser's own viewer — a scrollable tree of a feed rather
than an unreadable wall or a "this XML file does not appear to have any
style information associated with it".

## Why an extension and not a bookmarklet

The file has to be read from a page that is not the file's own origin, and
almost nothing on the web sends the CORS headers that would allow that. An
extension page carries the extension's host permissions, so it can read
the file whatever its server says — and it sends your cookies, so a feed
behind a login works like any other.

vuwr itself still fetches nothing. `viewer.js` reads the file and hands
over the bytes; the wasm is given a document, exactly as when one is
dropped on it.

## Build and load

```sh
./extension/build.sh          # builds the wasm and copies it in
```

Then in Chrome: **chrome://extensions** → turn on **Developer mode** →
**Load unpacked** → choose the `extension/` directory.

## Using it

Navigate to any `.csv`, `.tsv`, `.json` or `.xml` URL and it opens in
vuwr. The toolbar button turns the redirect off and on — the badge reads
`off` when it is off — for when you want the file itself. Any page that
fails to load offers a link to the original rather than leaving you with
an empty tab.

## What it does not do

Local files. `file://` URLs need "Allow access to file URLs" on the
extension's own settings page, and even then Chrome does not run the
redirect over them. Dropping the file onto vuwr works, and so does the
desktop build.

Nor does it touch a request a page makes for itself: only a navigation you
make is redirected, so a site fetching its own JSON is left alone.
