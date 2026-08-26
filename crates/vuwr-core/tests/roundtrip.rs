//! Round-trip corpus: every file must survive parse → serialize byte for
//! byte. This is the central promise of the tool — editing one cell must
//! produce a one-line diff, so the untouched representation must be exact.

use std::fs;
use std::path::Path;

use vuwr_core::{Document, FormatHint};

#[test]
fn corpus_files_roundtrip_byte_for_byte() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    assert!(!files.is_empty(), "corpus directory is empty");

    for path in files {
        let bytes = fs::read(&path).unwrap();
        let doc = Document::parse(&bytes, FormatHint::Auto)
            .unwrap_or_else(|e| panic!("{}: parse failed: {e}", path.display()));
        assert_eq!(
            doc.serialize(),
            bytes,
            "{}: round-trip mismatch",
            path.display()
        );
    }
}

/// A file's line endings and its last byte are part of the file.
///
/// JSON is rebuilt from the tree rather than spliced, so it is the one
/// format that has to be told to keep them. It was not: opening a JSON
/// file that ended with a newline — nearly all of them do — and saving it
/// dropped that newline, and on Windows the same save turned every CRLF
/// in the file into an LF. A one-cell edit produced a diff touching every
/// line, which is the thing this tool exists not to do.
#[test]
fn line_endings_and_the_final_newline_survive() {
    let cases: [(&str, FormatHint, &str); 6] = [
        (
            "json, crlf",
            FormatHint::Json,
            "{\r\n  \"a\": 1,\r\n  \"b\": [1, 2]\r\n}\r\n",
        ),
        (
            "json, trailing newline",
            FormatHint::Json,
            "{\n  \"a\": 1\n}\n",
        ),
        ("json, none", FormatHint::Json, "{\n  \"a\": 1\n}"),
        ("json, compact", FormatHint::Json, "{\"a\":1}\n"),
        (
            "xml, crlf",
            FormatHint::Xml,
            "<r>\r\n  <a>1</a>\r\n</r>\r\n",
        ),
        ("csv, crlf", FormatHint::Csv, "a,b\r\n1,2\r\n"),
    ];
    for (name, hint, src) in cases {
        let doc = Document::parse(src.as_bytes(), hint).unwrap();
        let out = String::from_utf8(doc.serialize()).unwrap();
        assert_eq!(out, src, "{name}: not returned as it arrived");
    }
}

/// And a newline inside a value is still escaped, not turned into a real
/// line break — which is what makes swapping the endings safe.
#[test]
fn a_newline_in_a_string_is_not_a_line_ending() {
    let src = "{\r\n  \"a\": \"one\\ntwo\"\r\n}\r\n";
    let doc = Document::parse(src.as_bytes(), FormatHint::Json).unwrap();
    assert_eq!(String::from_utf8(doc.serialize()).unwrap(), src);
}
