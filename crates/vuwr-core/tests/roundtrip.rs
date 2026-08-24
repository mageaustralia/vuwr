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
