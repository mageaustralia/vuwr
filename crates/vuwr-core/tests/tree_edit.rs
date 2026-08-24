//! Structural tree edits: remove, insert, duplicate, rename. Each must be
//! exactly undoable, since they change the document's shape.

use vuwr_core::{Document, FormatHint, Node, PathSeg};

fn json(src: &str) -> Document {
    Document::parse(src.as_bytes(), FormatHint::Json).unwrap()
}
fn text(d: &Document) -> String {
    String::from_utf8(d.serialize()).unwrap()
}

#[test]
fn removing_a_map_entry_undoes_exactly() {
    let src = r#"{"a":1,"b":2,"c":3}"#;
    let mut d = json(src);
    d.remove_node(&[], 1).unwrap();
    assert_eq!(text(&d), r#"{"a":1,"c":3}"#);
    assert!(d.undo());
    assert_eq!(text(&d), src, "the key comes back in its old position");
}

#[test]
fn removing_an_array_item_undoes_exactly() {
    let mut d = json("[1,2,3]");
    d.remove_node(&[], 0).unwrap();
    assert_eq!(text(&d), "[2,3]");
    assert!(d.undo());
    assert_eq!(text(&d), "[1,2,3]");
}

#[test]
fn inserting_places_the_node_where_asked() {
    let mut d = json(r#"{"a":1,"c":3}"#);
    d.insert_node(&[], 1, Some("b".into()), Node::Number("2".into()))
        .unwrap();
    assert_eq!(text(&d), r#"{"a":1,"b":2,"c":3}"#);
    assert!(d.undo());
    assert_eq!(text(&d), r#"{"a":1,"c":3}"#);
}

#[test]
fn inserting_into_a_nested_container() {
    let mut d = json(r#"{"o":{"x":1}}"#);
    d.insert_node(
        &[PathSeg::Key("o".into())],
        1,
        Some("y".into()),
        Node::Number("2".into()),
    )
    .unwrap();
    assert_eq!(text(&d), r#"{"o":{"x":1,"y":2}}"#);
}

#[test]
fn renaming_a_key_undoes_exactly() {
    let mut d = json(r#"{"old":1}"#);
    d.rename_node(&[], 0, "new".into()).unwrap();
    assert_eq!(text(&d), r#"{"new":1}"#);
    assert!(d.undo());
    assert_eq!(text(&d), r#"{"old":1}"#);
}

/// Duplicating is a read plus an insert, and must not disturb the original.
#[test]
fn duplicating_an_entry() {
    let mut d = json(r#"{"a":1,"b":2}"#);
    let copy = d
        .as_json()
        .unwrap()
        .root()
        .get_at(&[PathSeg::Key("a".into())])
        .unwrap()
        .clone();
    d.insert_node(&[], 1, Some("a copy".into()), copy).unwrap();
    assert_eq!(text(&d), r#"{"a":1,"a copy":1,"b":2}"#);
}

#[test]
fn out_of_range_edits_report_rather_than_panic() {
    let mut d = json(r#"{"a":1}"#);
    assert!(d.remove_node(&[], 9).is_err());
    assert!(
        d.rename_node(&[PathSeg::Key("a".into())], 0, "x".into())
            .is_err()
    );
}

/// XML indices address element children only, so removing index 0 must
/// take the first *element*, not the whitespace before it.
#[test]
fn xml_removal_skips_whitespace_between_elements() {
    let src = "<r>\n  <a/>\n  <b/>\n</r>";
    let mut d = Document::parse(src.as_bytes(), FormatHint::Xml).unwrap();
    d.remove_node(&[], 0).unwrap();
    let out = text(&d);
    assert!(!out.contains("<a/>"), "the first element went: {out}");
    assert!(out.contains("<b/>"), "the second stayed: {out}");
    assert!(d.undo());
    assert_eq!(text(&d), src, "including the whitespace around it");
}
