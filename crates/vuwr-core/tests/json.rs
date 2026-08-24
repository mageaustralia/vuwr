//! JSON-specific tests: tree structure, table eligibility, drill-down.

use vuwr_core::{Document, FormatHint, Node};

#[test]
fn json_auto_detected() {
    let doc = Document::parse(b"{}", FormatHint::Auto).unwrap();
    assert!(doc.is_json());
    let doc = Document::parse(b"a,b\n1,2\n", FormatHint::Auto).unwrap();
    assert!(doc.is_csv());
}

#[test]
fn json_table_eligible_array_of_objects() {
    let doc = Document::parse(b"[{\"a\":1},{\"a\":2}]", FormatHint::Auto).unwrap();
    assert!(doc.json_table_eligible());
}

#[test]
fn json_not_table_eligible_array_of_scalars() {
    let doc = Document::parse(b"[1,2,3]", FormatHint::Auto).unwrap();
    assert!(!doc.json_table_eligible());
}

#[test]
fn json_not_table_eligible_nested_objects() {
    let doc = Document::parse(b"[{\"a\":1},{\"b\":2}]", FormatHint::Auto).unwrap();
    assert!(!doc.json_table_eligible());
}

#[test]
fn json_roundtrip_preserves_key_order() {
    let doc = Document::parse(b"{\"z\":1,\"a\":2}", FormatHint::Auto).unwrap();
    let out = doc.serialize();
    assert_eq!(out, b"{\"z\":1,\"a\":2}");
}

#[test]
fn json_roundtrip_preserves_trailing_comma() {
    let doc = Document::parse(b"{\"a\":1,}", FormatHint::Auto).unwrap();
    let out = doc.serialize();
    assert_eq!(out, b"{\"a\":1,}");
}

#[test]
fn json_roundtrip_preserves_indentation() {
    let input = "{\n  \"a\": 1,\n  \"b\": [2, 3]\n}";
    let doc = Document::parse(input.as_bytes(), FormatHint::Auto).unwrap();
    let out = doc.serialize();
    assert_eq!(out, input.as_bytes());
}

#[test]
fn json_tree_structure_map() {
    let doc = Document::parse(b"{\"name\":\"Alice\",\"age\":30}", FormatHint::Auto).unwrap();
    let json = doc.as_json().unwrap();
    match json.root() {
        Node::Map(m) => {
            assert_eq!(m.entries.len(), 2);
            assert_eq!(m.entries[0].0, "name");
            assert_eq!(m.entries[1].0, "age");
        }
        _ => panic!("expected map"),
    }
}

#[test]
fn json_tree_structure_array() {
    let doc = Document::parse(b"[1,2,3]", FormatHint::Auto).unwrap();
    let json = doc.as_json().unwrap();
    match json.root() {
        Node::Array(a) => assert_eq!(a.items.len(), 3),
        _ => panic!("expected array"),
    }
}

#[test]
fn json_nested_drill_down() {
    let doc = Document::parse(
        b"{\"person\":{\"name\":\"Alice\"},\"items\":[1,2]}",
        FormatHint::Auto,
    )
    .unwrap();
    let json = doc.as_json().unwrap();
    match json.root() {
        Node::Map(m) => {
            // First entry is a nested object
            match &m.entries[0].1 {
                Node::Map(inner) => assert_eq!(inner.entries[0].0, "name"),
                _ => panic!("expected nested map"),
            }
            // Second entry is an array
            match &m.entries[1].1 {
                Node::Array(a) => assert_eq!(a.items.len(), 2),
                _ => panic!("expected array"),
            }
        }
        _ => panic!("expected map"),
    }
}
