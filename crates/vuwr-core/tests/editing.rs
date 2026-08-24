//! Editing through the Sheet view: JSON and XML, not just CSV.

use vuwr_core::{Document, FormatHint};

fn s(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap()
}

#[test]
fn json_cell_edit_writes_through_and_undoes() {
    let src = r#"[{"name":"Alice","age":30},{"name":"Bob","age":25}]"#;
    let mut doc = Document::parse(src.as_bytes(), FormatHint::Auto).unwrap();

    doc.set_cell(0, 0, "Alicia").unwrap();
    assert_eq!(
        s(doc.serialize()),
        r#"[{"name":"Alicia","age":30},{"name":"Bob","age":25}]"#
    );

    assert!(doc.undo());
    assert_eq!(
        s(doc.serialize()),
        src,
        "undo must restore the source exactly"
    );
    assert!(doc.redo());
    assert_eq!(
        s(doc.serialize()),
        r#"[{"name":"Alicia","age":30},{"name":"Bob","age":25}]"#
    );
}

/// JSON has real types, so editing a number must not quietly turn it into
/// a string — that would change the document's meaning, not just its text.
#[test]
fn json_edit_preserves_the_existing_type() {
    let mut doc = Document::parse(br#"[{"n":30,"b":true,"s":"x"}]"#, FormatHint::Auto).unwrap();

    doc.set_cell(0, 0, "31").unwrap(); // number stays a number
    doc.set_cell(0, 1, "false").unwrap(); // bool stays a bool
    doc.set_cell(0, 2, "42").unwrap(); // string stays a string

    assert_eq!(s(doc.serialize()), r#"[{"n":31,"b":false,"s":"42"}]"#);
}

/// A value that is not valid for the old type becomes a string, visibly,
/// rather than being rejected or silently coerced.
#[test]
fn json_edit_falls_back_to_string_when_type_does_not_fit() {
    let mut doc = Document::parse(br#"[{"n":30}]"#, FormatHint::Auto).unwrap();
    doc.set_cell(0, 0, "n/a").unwrap();
    assert_eq!(s(doc.serialize()), r#"[{"n":"n/a"}]"#);
}

#[test]
fn json_edit_preserves_surrounding_formatting() {
    let src = "[\n  {\n    \"a\": 1,\n    \"b\": 2\n  }\n]";
    let mut doc = Document::parse(src.as_bytes(), FormatHint::Auto).unwrap();
    doc.set_cell(0, 1, "3").unwrap();
    assert_eq!(
        s(doc.serialize()),
        "[\n  {\n    \"a\": 1,\n    \"b\": 3\n  }\n]"
    );
}

#[test]
fn xml_attribute_edit_writes_through_and_undoes() {
    let src = "<?xml version=\"1.0\"?>\n<items>\n  <item name=\"a\" qty=\"1\"/>\n  <item name=\"b\" qty=\"2\"/>\n</items>";
    let mut doc = Document::parse(src.as_bytes(), FormatHint::Auto).unwrap();

    doc.set_cell(1, 0, "beta").unwrap();
    assert!(s(doc.serialize()).contains("<item name=\"beta\" qty=\"2\"/>"));
    // The declaration, indentation and the untouched row all survive.
    assert!(s(doc.serialize()).starts_with("<?xml version=\"1.0\"?>\n<items>\n  <item name=\"a\""));

    assert!(doc.undo());
    assert_eq!(s(doc.serialize()), src);
}

#[test]
fn xml_child_element_text_edit() {
    let src = "<rows><row><name>Alice</name></row></rows>";
    let mut doc = Document::parse(src.as_bytes(), FormatHint::Auto).unwrap();
    doc.set_cell(0, 0, "Bob").unwrap();
    assert_eq!(
        s(doc.serialize()),
        "<rows><row><name>Bob</name></row></rows>"
    );
    assert!(doc.undo());
    assert_eq!(s(doc.serialize()), src);
}

/// One interface for every format: the frontend asks the sheet, not the
/// document's type.
#[test]
fn sheet_is_uniform_across_formats() {
    type Case = (
        &'static str,
        &'static [u8],
        (usize, usize),
        Vec<&'static str>,
    );
    let cases: Vec<Case> = vec![
        ("csv", b"name,age\nAlice,30\n", (2, 2), vec!["name", "age"]),
        (
            "json",
            br#"[{"name":"Alice","age":30}]"#,
            (1, 2),
            vec!["name", "age"],
        ),
        (
            "xml",
            b"<rows><row name=\"Alice\" age=\"30\"/></rows>",
            (1, 2),
            vec!["name", "age"],
        ),
    ];
    for (label, src, dims, headers) in cases {
        let doc = Document::parse(src, FormatHint::Auto).unwrap();
        let sheet = doc.sheet().unwrap_or_else(|| panic!("{label}: no sheet"));
        assert_eq!(sheet.dims(), dims, "{label} dims");
        assert_eq!(sheet.headers(), headers, "{label} headers");
        assert_eq!(
            sheet.cell(dims.0 - 1, 0).as_deref(),
            Some("Alice"),
            "{label}"
        );
    }
}

#[test]
fn non_table_shaped_json_has_no_sheet() {
    let doc = Document::parse(br#"{"a":{"b":1}}"#, FormatHint::Auto).unwrap();
    assert!(doc.sheet().is_none());
}
