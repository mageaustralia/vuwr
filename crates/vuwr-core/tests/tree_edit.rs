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

// --- Clipboard ---

use vuwr_core::Effect;

/// Core does no I/O, so copying asks the frontend to do it.
#[test]
fn copying_asks_the_frontend_for_the_clipboard() {
    let mut s = session(r#"{"a":"hello"}"#);
    match s.execute(Command::Copy) {
        Effect::Copy(text) => assert_eq!(text, "hello"),
        other => panic!("expected a copy effect, got {other:?}"),
    }
}

/// A container copies as JSON, which is what you would paste elsewhere.
#[test]
fn copying_a_container_yields_json() {
    let mut s = session(r#"{"o":{"x":1}}"#);
    match s.execute(Command::Copy) {
        Effect::Copy(text) => assert_eq!(text, r#"{"x":1}"#),
        other => panic!("got {other:?}"),
    }
}

#[test]
fn pasting_replaces_the_value_under_the_cursor() {
    let mut s = session(r#"{"a":"old"}"#);
    s.paste("new");
    assert_eq!(
        String::from_utf8(s.doc.serialize()).unwrap(),
        r#"{"a":"new"}"#
    );
    assert!(s.dirty);
}

/// Mid-edit, a paste goes in at the caret rather than replacing anything.
#[test]
fn pasting_while_typing_inserts_at_the_caret() {
    let mut s = session(r#"{"a":"ac"}"#);
    s.execute(Command::EditCell);
    s.input_left(); // between a and c
    s.paste("b");
    s.input_submit();
    assert_eq!(
        String::from_utf8(s.doc.serialize()).unwrap(),
        r#"{"a":"abc"}"#
    );
}

/// A cell holds one value, so pasting a block would silently flatten it.
#[test]
fn pasting_several_lines_into_a_cell_is_refused() {
    let src = r#"{"a":"old"}"#;
    let mut s = session(src);
    s.paste("one\ntwo");
    assert_eq!(String::from_utf8(s.doc.serialize()).unwrap(), src);
    assert!(s.status.contains("several lines"), "{}", s.status);
}

/// Newlines are dropped rather than refused mid-edit, where a line break
/// would end the edit anyway.
#[test]
fn pasting_several_lines_while_typing_strips_the_breaks() {
    let mut s = session(r#"{"a":""}"#);
    s.execute(Command::EditCell);
    s.paste("one\ntwo");
    s.input_submit();
    assert_eq!(
        String::from_utf8(s.doc.serialize()).unwrap(),
        r#"{"a":"onetwo"}"#
    );
}

#[test]
fn pasting_nothing_does_nothing() {
    let src = r#"{"a":"old"}"#;
    let mut s = session(src);
    s.paste("");
    assert_eq!(String::from_utf8(s.doc.serialize()).unwrap(), src);
}

/// The GUI says Save; a terminal still answers to `:w`. Both must reach
/// the same command, or the two frontends have diverged in vocabulary.
#[test]
fn save_answers_to_both_vocabularies() {
    for name in ["save", "w", "write", "w!"] {
        assert_eq!(Command::from_name(name), Some(Command::Save), ":{name}");
    }
    for name in ["save-quit", "wq", "x"] {
        assert_eq!(
            Command::from_name(name),
            Some(Command::SaveAndQuit),
            ":{name}"
        );
    }
    assert_eq!(Command::from_name("save-as"), Some(Command::SaveAs));
    assert_eq!(Command::from_name("open"), Some(Command::Open));
}

// --- Editing an XML element's text from the tree ---
//
// The large-value editor opened empty on a `<description>` holding a
// paragraph of escaped HTML, and saving would have replaced the element
// with a bare string, losing the tag.

fn xml_session(src: &str) -> Session {
    Session::new(Document::parse(src.as_bytes(), FormatHint::Xml).unwrap())
}

#[test]
fn an_elements_text_is_what_the_editor_opens_on() {
    let mut s = xml_session("<r><description><![CDATA[Some long text]]></description></r>");
    s.grid.cursor = (0, 0);
    assert_eq!(s.large_edit_text().as_deref(), Some("Some long text"));
}

#[test]
fn plain_text_elements_open_too() {
    let mut s = xml_session("<r><title>Racquet</title></r>");
    s.grid.cursor = (0, 0);
    assert_eq!(s.large_edit_text().as_deref(), Some("Racquet"));
}

/// The element must survive: writing the node itself replaced the whole
/// `<description>` with a string.
#[test]
fn saving_changes_the_text_and_keeps_the_element() {
    let mut s = xml_session("<r><description>old</description></r>");
    s.grid.cursor = (0, 0);
    s.commit_large_edit("new");
    assert_eq!(
        String::from_utf8(s.doc.serialize()).unwrap(),
        "<r><description>new</description></r>"
    );
}

#[test]
fn saving_keeps_a_cdata_section_a_cdata_section() {
    let mut s = xml_session("<r><d><![CDATA[old]]></d></r>");
    s.grid.cursor = (0, 0);
    s.commit_large_edit("new &lt;p&gt;markup&lt;/p&gt;");
    assert_eq!(
        String::from_utf8(s.doc.serialize()).unwrap(),
        "<r><d><![CDATA[new &lt;p&gt;markup&lt;/p&gt;]]></d></r>",
        "rewriting it as plain text would change how the document escapes"
    );
}

#[test]
fn editing_an_elements_text_undoes_exactly() {
    let src = "<r><d><![CDATA[old]]></d></r>";
    let mut s = xml_session(src);
    s.grid.cursor = (0, 0);
    s.commit_large_edit("new");
    assert!(s.doc.undo());
    assert_eq!(String::from_utf8(s.doc.serialize()).unwrap(), src);
}

/// The inline editor reads the same value, so `i` and F2 agree.
#[test]
fn the_inline_editor_sees_the_same_text() {
    let mut s = xml_session("<r><d>hello</d></r>");
    s.grid.cursor = (0, 0);
    s.execute(Command::EditCell);
    assert_eq!(s.entry().map(|(_, b)| b.to_string()), Some("hello".into()));
}

/// A container has no single value, so the editor declines rather than
/// opening on nothing.
#[test]
fn a_container_has_nothing_to_open() {
    let mut s = xml_session("<r><group><a/></group></r>");
    s.grid.cursor = (0, 0);
    assert_eq!(s.large_edit_text(), None);
}

/// Multi-line text is exactly what this editor is for.
#[test]
fn multi_line_text_round_trips_through_the_editor() {
    let mut s = xml_session("<r><d><![CDATA[one]]></d></r>");
    s.grid.cursor = (0, 0);
    s.commit_large_edit("one\ntwo\nthree");
    assert_eq!(
        String::from_utf8(s.doc.serialize()).unwrap(),
        "<r><d><![CDATA[one\ntwo\nthree]]></d></r>"
    );
}

// --- Escaped markup is decoded for editing ---
//
// A feed's description is escaped HTML. Reading `&lt;p&gt;` is unpleasant
// and editing it is worse, so it is decoded for the editor and re-encoded
// on the way back.

#[test]
fn escaped_markup_is_decoded_for_editing() {
    let mut s = xml_session("<r><d>&lt;p&gt;Hello&lt;/p&gt;</d></r>");
    s.grid.cursor = (0, 0);
    assert_eq!(s.large_edit_text().as_deref(), Some("<p>Hello</p>"));
}

#[test]
fn typing_markup_is_encoded_on_the_way_back() {
    let mut s = xml_session("<r><d>&lt;p&gt;old&lt;/p&gt;</d></r>");
    s.grid.cursor = (0, 0);
    s.commit_large_edit("<p>new & improved</p>");
    assert_eq!(
        String::from_utf8(s.doc.serialize()).unwrap(),
        "<r><d>&lt;p&gt;new &amp; improved&lt;/p&gt;</d></r>"
    );
}

/// A CDATA section is already literal, so encoding it would double the
/// escaping.
#[test]
fn cdata_content_is_not_double_encoded() {
    let mut s = xml_session("<r><d><![CDATA[&lt;p&gt;old&lt;/p&gt;]]></d></r>");
    s.grid.cursor = (0, 0);
    let shown = s.large_edit_text().unwrap();
    assert_eq!(shown, "<p>old</p>", "decoded for reading");
    s.commit_large_edit(&shown);
    assert_eq!(
        String::from_utf8(s.doc.serialize()).unwrap(),
        "<r><d><![CDATA[<p>old</p>]]></d></r>",
        "a CDATA section holds it literally, without escaping"
    );
}

/// A value nobody edits keeps its exact bytes: decoding is a display
/// concern, not a rewrite.
#[test]
fn untouched_values_keep_their_original_escaping() {
    let src = "<r><a>it&#039;s</a><b>&lt;x&gt;</b></r>";
    let s = xml_session(src);
    assert_eq!(String::from_utf8(s.doc.serialize()).unwrap(), src);
}

/// The inline editor cannot show a paragraph. It used to refuse and tell
/// the user to press F2; now it opens the editor that can hold it.
#[test]
fn a_long_value_hands_over_to_the_larger_editor() {
    let long = "line one\nline two\nline three";
    let mut s = xml_session(&format!("<r><d>{long}</d></r>"));
    s.grid.cursor = (0, 0);
    assert!(s.value_needs_more_room());
    assert_eq!(s.execute(Command::EditCell), Effect::EditLarge);
    assert!(!s.is_entering_text(), "the inline editor must not open");
}

#[test]
fn a_short_value_still_edits_inline() {
    let mut s = xml_session("<r><d>short</d></r>");
    s.grid.cursor = (0, 0);
    s.execute(Command::EditCell);
    assert!(s.is_entering_text());
}

// --- Copy, and what the tree shows ---

/// Copy value returned nothing for an XML element: `scalar_text` has no
/// case for one, the same gap that made the editor open empty.
#[test]
fn copying_an_xml_element_yields_its_text() {
    let mut s = xml_session("<r><d>hello</d></r>");
    s.grid.cursor = (0, 0);
    match s.execute(Command::Copy) {
        Effect::Copy(text) => assert_eq!(text, "hello"),
        other => panic!("expected a copy effect, got {other:?}"),
    }
}

/// Copy gives what the editor would open on, so the two agree.
#[test]
fn copying_decodes_escaped_markup() {
    let mut s = xml_session("<r><d>&lt;p&gt;Hello&lt;/p&gt;</d></r>");
    s.grid.cursor = (0, 0);
    match s.execute(Command::Copy) {
        Effect::Copy(text) => assert_eq!(text, "<p>Hello</p>"),
        other => panic!("got {other:?}"),
    }
    assert_eq!(s.large_edit_text().as_deref(), Some("<p>Hello</p>"));
}

/// And the tree shows the same thing, rather than making you read
/// `&lt;p&gt;`.
#[test]
fn the_tree_shows_decoded_text() {
    let s = xml_session("<r><d>&lt;p&gt;Hello&lt;/p&gt;</d></r>");
    assert_eq!(s.tree_rows[0].summary, "<p>Hello</p>");
}

/// Decoding is a display concern: the file keeps its own bytes.
#[test]
fn showing_decoded_text_does_not_rewrite_the_document() {
    let src = "<r><d>&lt;p&gt;Hello&lt;/p&gt;</d></r>";
    let s = xml_session(src);
    let _ = &s.tree_rows;
    assert_eq!(String::from_utf8(s.doc.serialize()).unwrap(), src);
}

#[test]
fn copying_a_container_still_yields_json() {
    let mut s = session(r#"{"o":{"x":1}}"#);
    match s.execute(Command::Copy) {
        Effect::Copy(text) => assert_eq!(text, r#"{"x":1}"#),
        other => panic!("got {other:?}"),
    }
}

// --- Long values open the editor that can hold them ---

/// Anything past a line's worth opens the larger editor rather than
/// telling the user to go and find F2.
#[test]
fn a_long_value_opens_the_larger_editor_by_itself() {
    let long = "x".repeat(Session::INLINE_LIMIT + 1);
    let mut s = xml_session(&format!("<r><d>{long}</d></r>"));
    s.grid.cursor = (0, 0);
    assert_eq!(s.execute(Command::EditCell), Effect::EditLarge);
    assert!(!s.is_entering_text(), "and not inline");
}

/// The rule is whether the value fits the view, not a fixed count: the
/// same value edits inline in a wide window and opens the box in a narrow
/// one.
#[test]
fn the_threshold_follows_the_view_width() {
    let value = "x".repeat(60);
    let src = format!("<r><d>{value}</d></r>");

    let mut wide = xml_session(&src);
    wide.set_viewport_cols(200);
    wide.grid.cursor = (0, 0);
    assert_eq!(wide.execute(Command::EditCell), Effect::None);
    assert!(wide.is_entering_text(), "room to edit in place");

    let mut narrow = xml_session(&src);
    narrow.set_viewport_cols(40);
    narrow.grid.cursor = (0, 0);
    assert_eq!(narrow.execute(Command::EditCell), Effect::EditLarge);
}

#[test]
fn anything_with_a_newline_opens_the_larger_editor() {
    let mut s = xml_session("<r><d>one\ntwo</d></r>");
    s.grid.cursor = (0, 0);
    assert_eq!(s.execute(Command::EditCell), Effect::EditLarge);
}

/// F2 asks for it directly, and says so when there is nothing to edit.
#[test]
fn asking_for_the_larger_editor_on_a_container_reports_why() {
    let mut s = session(r#"{"o":{"x":1}}"#);
    assert_eq!(s.execute(Command::EditLarge), Effect::None);
    assert!(s.status.contains("nothing here"), "{}", s.status);
}

#[test]
fn replace_also_routes_long_values_to_the_larger_editor() {
    let long = "x".repeat(200);
    let mut s = xml_session(&format!("<r><d>{long}</d></r>"));
    s.grid.cursor = (0, 0);
    assert_eq!(s.execute(Command::ReplaceCell), Effect::EditLarge);
}

// --- Anything that does not fit gets the larger editor ---

/// A URL cut off at the column's width needs the bigger editor as much as
/// a paragraph does. A fixed character limit missed exactly that: a
/// 70-character link sat under 80 and opened inline, where it could not
/// be read.
#[test]
fn a_value_wider_than_its_column_opens_the_larger_editor() {
    let url = "https://www.example.com/media/catalog/product/a/l/alu-power.jpg";
    let src = format!("<rows><row><id>1</id><link>{url}</link></row></rows>");
    let mut s = xml_session(&src);
    s.execute(Command::ViewTable);
    s.grid.cursor = (0, 1);

    assert!(
        url.chars().count() < Session::INLINE_LIMIT,
        "under the old limit"
    );
    assert!(
        s.value_needs_more_room(),
        "but wider than its column, so it still needs room"
    );
    assert_eq!(s.execute(Command::EditCell), Effect::EditLarge);
}

/// Something that does fit its column still edits in place.
#[test]
fn a_value_that_fits_its_column_edits_inline() {
    let mut s = xml_session("<rows><row><id>1</id><n>short</n></row></rows>");
    s.execute(Command::ViewTable);
    s.grid.cursor = (0, 1);
    assert!(!s.value_needs_more_room());
    assert_eq!(s.execute(Command::EditCell), Effect::None);
    assert!(s.is_entering_text());
}

/// The editor opens on decoded text in the table too — it only decoded in
/// the tree, so editing a description from table view showed `&lt;p&gt;`.
#[test]
fn the_table_editor_opens_on_decoded_text() {
    let mut s = xml_session("<rows><row><d>&lt;p&gt;Hello&lt;/p&gt;</d></row></rows>");
    s.execute(Command::ViewTable);
    s.grid.cursor = (0, 0);
    assert_eq!(s.large_edit_text().as_deref(), Some("<p>Hello</p>"));
}

/// And writing back from the table encodes again.
#[test]
fn editing_from_the_table_encodes_on_the_way_back() {
    let mut s = xml_session("<rows><row><d>&lt;p&gt;old&lt;/p&gt;</d></row></rows>");
    s.execute(Command::ViewTable);
    s.grid.cursor = (0, 0);
    s.commit_large_edit("<p>new</p>");
    assert_eq!(
        String::from_utf8(s.doc.serialize()).unwrap(),
        "<rows><row><d>&lt;p&gt;new&lt;/p&gt;</d></row></rows>"
    );
}

/// JSON strings have no entities, and decoding one would eat a literal
/// `&amp;` the user actually typed.
#[test]
fn json_values_are_not_entity_decoded() {
    let mut s = session(r#"[{"a":"x &amp; y"}]"#);
    s.execute(Command::ViewTable);
    s.grid.cursor = (0, 0);
    assert_eq!(s.large_edit_text().as_deref(), Some("x &amp; y"));
}

/// The detail pane shows the same text the editor would open on.
#[test]
fn the_detail_pane_shows_decoded_text_too() {
    let mut s = xml_session("<rows><row><d>&lt;b&gt;bold&lt;/b&gt;</d></row></rows>");
    s.execute(Command::ViewTable);
    s.grid.cursor = (0, 0);
    assert_eq!(s.detail_text().as_deref(), Some("<b>bold</b>"));
}

/// Typing markup into a table cell must not put raw `<p>` into the file:
/// that is not a formatting slip, it is invalid XML.
#[test]
fn typing_markup_into_a_table_cell_cannot_break_the_document() {
    let mut s = xml_session("<rows><row><d>old</d></row></rows>");
    s.execute(Command::ViewTable);
    s.grid.cursor = (0, 0);
    s.commit_large_edit("<p>new & shiny</p>");

    let out = String::from_utf8(s.doc.serialize()).unwrap();
    assert_eq!(
        out,
        "<rows><row><d>&lt;p&gt;new &amp; shiny&lt;/p&gt;</d></row></rows>"
    );
    // The proof that matters: it still parses.
    assert!(Document::parse(out.as_bytes(), FormatHint::Xml).is_ok());
}

/// A CDATA cell holds its content literally, so it must not be encoded.
#[test]
fn a_cdata_cell_is_written_literally() {
    let mut s = xml_session("<rows><row><d><![CDATA[old]]></d></row></rows>");
    s.execute(Command::ViewTable);
    s.grid.cursor = (0, 0);
    s.commit_large_edit("<p>new</p>");
    assert_eq!(
        String::from_utf8(s.doc.serialize()).unwrap(),
        "<rows><row><d><![CDATA[<p>new</p>]]></d></row></rows>"
    );
}

/// The inline editor writes through the same path.
#[test]
fn inline_table_edits_encode_too() {
    let mut s = xml_session("<rows><row><n>a</n></row></rows>");
    s.execute(Command::ViewTable);
    s.grid.cursor = (0, 0);
    s.execute(Command::ReplaceCell);
    for c in "a<b".chars() {
        s.input_char(c);
    }
    s.input_submit();
    assert_eq!(
        String::from_utf8(s.doc.serialize()).unwrap(),
        "<rows><row><n>a&lt;b</n></row></rows>"
    );
}

/// CSV and JSON are untouched by any of this.
#[test]
fn csv_cells_are_written_verbatim() {
    let mut s = Session::new(Document::parse(b"a\nx\n", FormatHint::Csv).unwrap());
    s.grid.cursor = (1, 0);
    s.commit_large_edit("<p>& literal</p>");
    assert_eq!(
        String::from_utf8(s.doc.serialize()).unwrap(),
        "a\n<p>& literal</p>\n"
    );
}

// --- Decoded text view ---

#[test]
fn text_view_can_show_the_source_rather_than_the_markup() {
    let mut s = xml_session("<r><d>&lt;p&gt;Hi&lt;/p&gt;</d></r>");
    s.execute(Command::ViewText);
    assert!(
        s.table_cell(0, 0).unwrap().contains("<p>Hi</p>"),
        "decoded by default: {:?}",
        s.table_cell(0, 0)
    );

    s.execute(Command::ToggleDecoded);
    assert!(
        s.table_cell(0, 0).unwrap().contains("&lt;p&gt;"),
        "the source when asked"
    );
}

/// Decoding is a view. Nothing is written, so the file is untouched.
#[test]
fn decoding_the_view_does_not_change_the_file() {
    let src = "<r><d>&lt;p&gt;Hi&lt;/p&gt;</d></r>";
    let mut s = xml_session(src);
    s.execute(Command::ViewText);
    assert_eq!(String::from_utf8(s.doc.serialize()).unwrap(), src);
}

/// A line edited while decoded is encoded again on the way back, so the
/// document stays valid.
#[test]
fn editing_a_decoded_line_encodes_it_again() {
    let mut s = xml_session("<r><d>&lt;p&gt;old&lt;/p&gt;</d></r>");
    s.execute(Command::ViewText);
    s.grid.cursor = (0, 0);

    // Text view edits its line in place, which is the path a user takes.
    let shown = s.table_cell(0, 0).unwrap();
    s.execute(Command::ReplaceCell);
    for c in shown.replace("old", "new").chars() {
        s.input_char(c);
    }
    s.input_submit();

    let out = String::from_utf8(s.doc.serialize()).unwrap();
    assert!(
        out.contains("&lt;p&gt;new&lt;/p&gt;"),
        "encoded again: {out}"
    );
    assert!(
        Document::parse(out.as_bytes(), FormatHint::Xml).is_ok(),
        "and still valid"
    );
}

#[test]
fn toggling_back_shows_the_markup_again() {
    let mut s = xml_session("<r><d>&lt;p&gt;Hi&lt;/p&gt;</d></r>");
    s.execute(Command::ViewText);
    s.execute(Command::ToggleDecoded);
    s.execute(Command::ToggleDecoded);
    assert!(s.table_cell(0, 0).unwrap().contains("<p>Hi</p>"));
}

/// Only XML has entities; saying so beats a toggle that does nothing.
#[test]
fn the_decoded_toggle_is_refused_for_other_formats() {
    let mut s = session(r#"{"a":1}"#);
    let before = s.decoded_text;
    s.execute(Command::ToggleDecoded);
    assert_eq!(s.decoded_text, before, "nothing to toggle");
    assert!(s.status.contains("only XML"), "{}", s.status);
}

/// Diagnostics are cached because finding them serialises the document,
/// and the bar that shows them asks every frame. The cache must not
/// outlive a change.
#[test]
fn diagnostics_refresh_after_an_edit() {
    let mut s = session("{\n  \"a\": 1,\n  \"b\": 2\n}");
    assert!(s.diagnostics().is_empty(), "clean to begin with");

    // Rename `b` to `a`, creating a duplicate.
    s.grid.cursor = (1, 0);
    s.execute(Command::RenameKey);
    for _ in 0.."b".len() {
        s.input_backspace();
    }
    s.input_char('a');
    s.input_submit();

    let found = s.diagnostics();
    assert_eq!(found.len(), 1, "the new duplicate is reported: {found:?}");
    assert!(found[0].message.contains("duplicate key 'a'"));
}

#[test]
fn diagnostics_clear_again_on_undo() {
    let mut s = session("{\n  \"a\": 1,\n  \"b\": 2\n}");
    s.grid.cursor = (1, 0);
    s.execute(Command::RenameKey);
    for _ in 0.."b".len() {
        s.input_backspace();
    }
    s.input_char('a');
    s.input_submit();
    assert_eq!(s.diagnostics().len(), 1);

    s.execute(Command::Undo);
    assert!(s.diagnostics().is_empty(), "undo puts the document back");
}

/// The table showed raw entity references while the tree beside it showed
/// them decoded — the same value spelled two ways in one window.
#[test]
fn the_table_shows_decoded_text_like_the_tree() {
    let src = "<rss><channel><item><d>&lt;p&gt;Hi&lt;/p&gt;</d></item>\
               <item><d>&lt;p&gt;There&lt;/p&gt;</d></item></channel></rss>";
    let mut s = xml_session(src);
    s.execute(Command::ViewTable);
    assert_eq!(s.table_cell(0, 0).as_deref(), Some("<p>Hi</p>"));

    s.execute(Command::ToggleDecoded);
    assert_eq!(s.table_cell(0, 0).as_deref(), Some("&lt;p&gt;Hi&lt;/p&gt;"));
}

/// Editing in the cell starts from what the cell shows. It used to start
/// from the raw text while the commit encoded what came back, so `&lt;`
/// became `&amp;lt;` on the way out.
#[test]
fn editing_a_table_cell_does_not_double_encode_it() {
    let src = "<rss><channel><item><d>&lt;p&gt;Hi&lt;/p&gt;</d></item>\
               <item><d>x</d></item></channel></rss>";
    let mut s = xml_session(src);
    s.execute(Command::ViewTable);
    s.grid.cursor = (0, 0);
    s.execute(Command::EditCell);
    s.input_submit();
    let out = String::from_utf8(s.doc.serialize()).unwrap();
    assert!(out.contains("&lt;p&gt;Hi&lt;/p&gt;"), "{out}");
    assert!(!out.contains("&amp;lt;"), "double-encoded: {out}");
}
