//! Re-laying-out a document. The parser preserves whatever layout a file
//! had; this is the deliberate opposite, and only happens when asked.

use vuwr_core::{Document, FormatHint, Layout};

fn text(doc: &Document) -> String {
    String::from_utf8(doc.serialize()).unwrap()
}

fn json(src: &str) -> Document {
    Document::parse(src.as_bytes(), FormatHint::Json).unwrap()
}

#[test]
fn compact_puts_everything_on_one_line() {
    let mut d = json("{\n  \"a\": 1,\n  \"b\": [1, 2]\n}");
    d.reformat(Layout::Compact).unwrap();
    assert_eq!(text(&d), r#"{"a":1,"b":[1,2]}"#);
}

#[test]
fn pretty_breaks_every_collection() {
    let mut d = json(r#"{"a":1,"b":[1,2]}"#);
    d.reformat(Layout::Pretty).unwrap();
    assert_eq!(
        text(&d),
        "{\n  \"a\": 1,\n  \"b\": [\n    1,\n    2\n  ]\n}"
    );
}

/// Smart is the one people actually want: readable without sprawling a
/// short array over four lines.
#[test]
fn smart_keeps_scalar_collections_on_one_line() {
    let mut d = json(r#"{"name":"a","tags":["x","y"],"meta":{"deep":{"k":1}}}"#);
    d.reformat(Layout::Smart).unwrap();
    let out = text(&d);
    assert!(
        out.contains(r#""tags": ["x", "y"]"#),
        "leaf array stays inline:\n{out}"
    );
    assert!(out.contains("\"meta\": {\n"), "nested map breaks:\n{out}");
}

#[test]
fn reformatting_is_undoable() {
    let src = "{\n  \"a\": 1\n}";
    let mut d = json(src);
    d.reformat(Layout::Compact).unwrap();
    assert_eq!(text(&d), r#"{"a":1}"#);
    assert!(d.undo());
    assert_eq!(text(&d), src, "undo restores the original layout exactly");
}

#[test]
fn reformatting_never_changes_the_data() {
    let src = r#"{"n":30,"s":"30","b":true,"z":null,"a":[1,{"k":"v"}]}"#;
    for style in [Layout::Compact, Layout::Pretty, Layout::Smart] {
        let mut d = json(src);
        d.reformat(style).unwrap();
        // Re-parsing the output and compacting it must give back the
        // original compact form: layout changed, values did not.
        let mut round = Document::parse(&d.serialize(), FormatHint::Json).unwrap();
        round.reformat(Layout::Compact).unwrap();
        assert_eq!(text(&round), src, "{style:?} changed the data");
    }
}

#[test]
fn csv_and_xml_have_no_layout_to_change() {
    let mut csv = Document::parse(b"a,b\n1,2\n", FormatHint::Csv).unwrap();
    assert!(csv.reformat(Layout::Pretty).is_err());

    let mut xml = Document::parse(b"<r><a/></r>", FormatHint::Xml).unwrap();
    assert!(
        xml.reformat(Layout::Pretty).is_err(),
        "reflowing XML would move text nodes, which changes meaning"
    );
}
