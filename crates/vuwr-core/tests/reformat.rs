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

/// CSV's shape is its content — rows and columns, nothing to re-indent.
#[test]
fn csv_has_no_layout_to_change() {
    let mut csv = Document::parse(b"a,b\n1,2\n", FormatHint::Csv).unwrap();
    assert!(csv.reformat(Layout::Pretty).is_err());
}

// --- XML ---

fn xml(src: &str) -> Document {
    Document::parse(src.as_bytes(), FormatHint::Xml).unwrap()
}

#[test]
fn xml_pretty_indents_nested_elements() {
    let mut d = xml("<r><a><b/></a></r>");
    d.reformat(Layout::Pretty).unwrap();
    assert_eq!(text(&d), "<r>\n  <a>\n    <b/>\n  </a>\n</r>");
}

#[test]
fn xml_compact_removes_layout_whitespace() {
    let mut d = xml("<r>\n  <a>\n    <b/>\n  </a>\n</r>");
    d.reformat(Layout::Compact).unwrap();
    assert_eq!(text(&d), "<r><a><b/></a></r>");
}

/// The important restraint: an element holding text must not be broken
/// across lines, because the newlines would become part of the text.
#[test]
fn xml_reformatting_never_touches_text_content() {
    let src = "<r><name>Alice Smith</name></r>";
    let mut d = xml(src);
    d.reformat(Layout::Pretty).unwrap();
    assert!(
        text(&d).contains("<name>Alice Smith</name>"),
        "text elements stay on one line: {}",
        text(&d)
    );
}

#[test]
fn xml_reformatting_leaves_cdata_alone() {
    let src = "<r><d><![CDATA[ spaced  content ]]></d></r>";
    let mut d = xml(src);
    d.reformat(Layout::Pretty).unwrap();
    assert!(
        text(&d).contains("<![CDATA[ spaced  content ]]>"),
        "{}",
        text(&d)
    );
}

#[test]
fn xml_reformatting_is_undoable() {
    let src = "<r><a/></r>";
    let mut d = xml(src);
    d.reformat(Layout::Pretty).unwrap();
    assert_ne!(text(&d), src);
    assert!(d.undo());
    assert_eq!(text(&d), src);
}

#[test]
fn xml_reformatting_preserves_the_declaration_and_attributes() {
    let mut d = xml("<?xml version=\"1.0\"?><r a=\"1\"><b c='2'/></r>");
    d.reformat(Layout::Pretty).unwrap();
    let out = text(&d);
    assert!(out.starts_with("<?xml version=\"1.0\"?>\n"), "{out}");
    assert!(out.contains("a=\"1\""), "{out}");
    assert!(out.contains("c='2'"), "single quotes survive: {out}");
}

/// Reformatting changes layout, never content: re-compacting must give
/// back what compacting the original gives.
#[test]
fn xml_reformatting_never_changes_the_data() {
    let src = "<r><a x=\"1\"><b>text</b><c/></a></r>";
    for style in [Layout::Pretty, Layout::Smart, Layout::Compact] {
        let mut d = xml(src);
        d.reformat(style).unwrap();
        let mut round = Document::parse(&d.serialize(), FormatHint::Xml).unwrap();
        round.reformat(Layout::Compact).unwrap();
        assert_eq!(text(&round), src, "{style:?} changed the document");
    }
}
