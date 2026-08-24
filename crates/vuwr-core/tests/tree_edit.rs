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

// --- Diagnostics and renaming, from the editor's point of view ---

use vuwr_core::{Command, Session};

fn session(src: &str) -> Session {
    Session::new(json(src))
}

/// A duplicate key must be reported with somewhere to go, not just noted.
#[test]
fn duplicate_keys_surface_as_diagnostics_with_positions() {
    let s = session("{\n  \"color\": true,\n  \"color\": \"gold\"\n}");
    let found = s.diagnostics();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].line, 3);
    assert!(found[0].message.contains("duplicate key 'color'"));
}

#[test]
fn a_clean_document_has_no_diagnostics() {
    assert!(session(r#"{"a":1,"b":2}"#).diagnostics().is_empty());
}

/// "Show me" has to actually show you: revealing switches to the view
/// where an offset means something and puts the cursor on that line.
#[test]
fn revealing_a_diagnostic_goes_to_its_line() {
    let mut s = session("{\n  \"color\": true,\n  \"color\": \"gold\"\n}");
    let d = s.diagnostics().remove(0);
    s.reveal(d.offset);
    assert_eq!(s.view_mode(), vuwr_core::ViewMode::Text);
    assert_eq!(s.grid.cursor.0, 2, "line 3, zero-indexed");
}

#[test]
fn renaming_a_key_from_the_tree() {
    let mut s = session(r#"{"old":1,"other":2}"#);
    s.execute(Command::RenameKey);
    assert!(s.is_renaming());
    for c in "new".chars() {
        s.input_char(c);
    }
    // The prompt starts with the old name, so clear it first.
    let mut s = session(r#"{"old":1,"other":2}"#);
    s.execute(Command::RenameKey);
    for _ in 0.."old".len() {
        s.input_backspace();
    }
    for c in "new".chars() {
        s.input_char(c);
    }
    s.input_submit();
    assert_eq!(
        String::from_utf8(s.doc.serialize()).unwrap(),
        r#"{"new":1,"other":2}"#
    );
    assert!(!s.is_renaming(), "the flag clears after committing");
}

#[test]
fn cancelling_a_rename_changes_nothing() {
    let src = r#"{"old":1}"#;
    let mut s = session(src);
    s.execute(Command::RenameKey);
    s.input_char('x');
    s.input_cancel();
    assert!(!s.is_renaming());
    assert_eq!(String::from_utf8(s.doc.serialize()).unwrap(), src);
}

#[test]
fn an_empty_key_is_refused() {
    let src = r#"{"old":1}"#;
    let mut s = session(src);
    s.execute(Command::RenameKey);
    for _ in 0.."old".len() {
        s.input_backspace();
    }
    s.input_submit();
    assert_eq!(String::from_utf8(s.doc.serialize()).unwrap(), src);
    assert!(s.status.contains("cannot be empty"), "{}", s.status);
}

/// Array items have no key to rename, and saying so beats doing nothing.
#[test]
fn renaming_an_array_item_reports_why_it_cannot() {
    let mut s = Session::new(json("[1,2]"));
    s.execute(Command::RenameKey);
    assert!(!s.is_renaming());
    assert!(s.status.contains("only object keys"), "{}", s.status);
}
