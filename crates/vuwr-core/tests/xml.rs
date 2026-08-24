//! XML-specific tests: tree structure, table eligibility, roundtrip.

use vuwr_core::{Document, FormatHint, Node};

#[test]
fn xml_auto_detected() {
    let doc = Document::parse(b"<root/>", FormatHint::Auto).unwrap();
    assert!(doc.is_xml());
}

#[test]
fn xml_roundtrip_preserves_comments() {
    let doc = Document::parse(b"<root><!-- keep --><child/></root>", FormatHint::Auto).unwrap();
    let out = doc.serialize();
    assert_eq!(out, b"<root><!-- keep --><child/></root>");
}

#[test]
fn xml_roundtrip_preserves_attribute_order() {
    let doc = Document::parse(b"<root z=\"1\" a=\"2\"/>", FormatHint::Auto).unwrap();
    let out = doc.serialize();
    assert_eq!(out, b"<root z=\"1\" a=\"2\"/>");
}

#[test]
fn xml_roundtrip_preserves_self_closing() {
    let doc = Document::parse(b"<br/>", FormatHint::Auto).unwrap();
    let out = doc.serialize();
    assert_eq!(out, b"<br/>");
}

#[test]
fn xml_roundtrip_preserves_empty_element() {
    let doc = Document::parse(b"<p></p>", FormatHint::Auto).unwrap();
    let out = doc.serialize();
    assert_eq!(out, b"<p></p>");
}

#[test]
fn xml_roundtrip_preserves_xml_decl() {
    let doc = Document::parse(
        b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<root/>",
        FormatHint::Auto,
    )
    .unwrap();
    let out = doc.serialize();
    assert_eq!(out, b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<root/>");
}

#[test]
fn xml_tree_structure() {
    let doc = Document::parse(b"<root><child>text</child></root>", FormatHint::Auto).unwrap();
    let xml = doc.as_xml().unwrap();
    match xml.root() {
        Node::Element(e) => {
            assert_eq!(e.tag, "root");
            assert_eq!(e.children.len(), 1);
            if let Node::Element(child) = &e.children[0] {
                assert_eq!(child.tag, "child");
            } else {
                panic!("expected child element");
            }
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn xml_table_eligible_repeated_siblings() {
    let doc = Document::parse(
        b"<items><item name=\"a\"/><item name=\"b\"/></items>",
        FormatHint::Auto,
    )
    .unwrap();
    assert!(doc.xml_table_eligible());
}

#[test]
fn xml_not_table_eligible_different_tags() {
    let doc = Document::parse(b"<root><a/><b/></root>", FormatHint::Auto).unwrap();
    assert!(!doc.xml_table_eligible());
}
