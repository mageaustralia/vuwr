//! Turning parse errors into positions a person can act on.

use vuwr_core::{Document, FormatHint, line_col};

#[test]
fn line_col_is_one_based_and_counts_characters() {
    let src = b"abc\ndefgh\nij";
    assert_eq!(line_col(src, 0), (1, 1), "start of file");
    assert_eq!(line_col(src, 2), (1, 3));
    assert_eq!(line_col(src, 4), (2, 1), "first byte of line 2");
    assert_eq!(line_col(src, 10), (3, 1));
    assert_eq!(line_col(src, 999), (3, 3), "past the end clamps");

    // Columns count characters, not bytes: é is two bytes.
    let uni = "café,x".as_bytes();
    assert_eq!(line_col(uni, 6), (1, 6), "the comma is the 5th char");
}

#[test]
fn parse_errors_report_a_usable_position() {
    let src = b"{\n  \"a\": 1,\n  \"b\": ,\n}";
    let Err(e) = Document::parse(src, FormatHint::Json) else {
        panic!("expected a parse error");
    };
    let located = e.located(src);
    assert!(located.starts_with("3:"), "points at line 3: {located}");
    assert!(located.contains("unexpected token"), "{located}");
}

#[test]
fn errors_without_a_position_still_render() {
    let Err(e) = Document::parse(&[b'{', 0xFF], FormatHint::Json) else {
        panic!("expected a parse error");
    };
    assert_eq!(e.located(&[b'{', 0xFF]), "input is not valid UTF-8");
}
