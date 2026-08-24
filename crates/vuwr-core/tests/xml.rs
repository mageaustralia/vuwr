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

// --- Table shape regressions ---
//
// `root()` used to return the first child, which for a declared document is
// the XML declaration — so every real-world file looked shapeless. And
// eligibility counted whitespace `Text` nodes as children, so any
// pretty-printed file was ineligible too.

#[test]
fn table_eligible_with_declaration_and_comments() {
    let src = br#"<?xml version="1.0"?>
<!-- a leading comment -->
<items>
  <item name="a"/>
  <item name="b"/>
</items>"#;
    let doc = Document::parse(src, FormatHint::Auto).unwrap();
    assert!(
        doc.xml_table_eligible(),
        "a declared, pretty-printed document is still table-shaped"
    );
}

#[test]
fn table_headers_and_cells_span_attributes_then_children() {
    let src = br#"<?xml version="1.0"?>
<rows>
  <row id="1" kind="a"><name>Alice</name><note/></row>
  <row id="2" kind="b"><name>Bob</name><note>hi</note></row>
</rows>"#;
    let doc = Document::parse(src, FormatHint::Auto).unwrap();
    let xml = doc.as_xml().unwrap();

    assert_eq!(xml.table_headers(), vec!["id", "kind", "name", "note"]);
    assert_eq!(xml.row_elements().len(), 2);

    assert_eq!(xml.table_cell(0, 0).as_deref(), Some("1"));
    assert_eq!(xml.table_cell(0, 1).as_deref(), Some("a"));
    assert_eq!(xml.table_cell(0, 2).as_deref(), Some("Alice"));
    // `<note/>` is empty: this used to index children[0] and panic.
    assert_eq!(xml.table_cell(0, 3).as_deref(), Some(""));
    assert_eq!(xml.table_cell(1, 3).as_deref(), Some("hi"));

    assert_eq!(xml.table_cell(9, 0), None, "out of range must not panic");
}

#[test]
fn document_element_skips_the_prolog() {
    let doc = Document::parse(br#"<?xml version="1.0"?><r><a/></r>"#, FormatHint::Auto).unwrap();
    match doc.as_xml().unwrap().root() {
        vuwr_core::Node::Element(e) => assert_eq!(e.tag, "r"),
        other => panic!("root should be the document element, got {other:?}"),
    }
}

#[test]
fn empty_document_is_an_error_not_a_panic() {
    assert!(Document::parse(b"   ", FormatHint::Xml).is_err());
}
