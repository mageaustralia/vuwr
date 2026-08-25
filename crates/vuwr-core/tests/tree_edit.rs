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
    let mut s = session("{\n  \"color\": true,\n  \"color\": \"gold\"\n}");
    s.execute(Command::Lint);
    let found = s.lint_results().expect("linted");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].line, 3);
    assert!(found[0].message.contains("duplicate key 'color'"));
}

#[test]
fn a_clean_document_has_no_diagnostics() {
    let mut s = session(r#"{"a":1,"b":2}"#);
    s.execute(Command::Lint);
    assert_eq!(s.lint_results(), Some(&[][..]));
}

/// "Show me" has to actually show you: revealing switches to the view
/// where an offset means something and puts the cursor on that line.
#[test]
fn revealing_a_diagnostic_goes_to_its_line() {
    let mut s = session("{\n  \"color\": true,\n  \"color\": \"gold\"\n}");
    s.execute(Command::Lint);
    let offset = s.lint_results().expect("linted")[0].offset;
    s.reveal(offset);
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

fn csv_session(src: &str) -> Session {
    Session::new(Document::parse(src.as_bytes(), FormatHint::Csv).unwrap())
}

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
/// Linting is asked for, not automatic — the scan re-reads the whole
/// document, which is a visible hitch on a large one after every edit.
#[test]
fn linting_happens_when_asked_and_its_findings_expire_on_a_change() {
    let mut s = session("{\n  \"a\": 1,\n  \"b\": 2\n}");
    assert!(s.lint_results().is_none(), "nothing until asked");
    s.execute(Command::Lint);
    assert_eq!(s.lint_results(), Some(&[][..]), "clean to begin with");
    assert!(s.status.contains("no problems"), "{}", s.status);

    // Rename `b` to `a`, creating a duplicate.
    s.grid.cursor = (1, 0);
    s.execute(Command::RenameKey);
    for _ in 0.."b".len() {
        s.input_backspace();
    }
    s.input_char('a');
    s.input_submit();

    assert!(
        s.lint_results().is_none(),
        "the old findings were about the old bytes"
    );
    s.execute(Command::Lint);
    let found = s.lint_results().expect("linted");
    assert_eq!(found.len(), 1, "the new duplicate is reported: {found:?}");
    assert!(found[0].message.contains("duplicate key 'a'"));
    assert!(s.status.contains("1 problem"), "{}", s.status);
}

#[test]
fn linting_is_clean_again_after_an_undo() {
    let mut s = session("{\n  \"a\": 1,\n  \"b\": 2\n}");
    s.grid.cursor = (1, 0);
    s.execute(Command::RenameKey);
    for _ in 0.."b".len() {
        s.input_backspace();
    }
    s.input_char('a');
    s.input_submit();
    s.execute(Command::Lint);
    assert_eq!(s.lint_results().map(<[_]>::len), Some(1));

    s.execute(Command::Undo);
    s.execute(Command::Lint);
    assert_eq!(
        s.lint_results(),
        Some(&[][..]),
        "undo puts the document back"
    );
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

// --- The value a text-view line belongs to ---

fn block_session() -> Session {
    let src = "<rss>\n  <item>\n    <d><![CDATA[<p>one\n<li>two</li>\n</p>]]></d>\n\
               <e>short</e>\n  </item>\n</rss>";
    let mut s = xml_session(src);
    s.execute(Command::ViewText);
    s
}

/// A description is one value and reads as one thing, but in the source it
/// is several lines. The lines inside it belong to it.
#[test]
fn a_line_inside_an_element_knows_the_block_it_belongs_to() {
    let mut s = block_session();
    s.grid.cursor = (3, 0); // `<li>two</li>`, inside the CDATA
    assert_eq!(s.value_block(), Some((2, 4)));
    // Markup inside CDATA is content, not tags: the block is `<d>`, not
    // the `<li>` the line happens to start with.
    assert!(s.table_cell(2, 0).unwrap().contains("<d>"));
}

/// A field on its own line is its own value: marking it must not light
/// up the whole `<item>` around it.
#[test]
fn a_field_line_is_not_swallowed_by_its_parent() {
    let mut s = block_session();
    s.grid.cursor = (1, 0); // `<item>` — which is a block
    assert_eq!(s.value_block(), Some((1, 6)));
    s.grid.cursor = (5, 0); // `<e>short</e>` — which is not
    assert_eq!(s.value_block(), None);
}

/// A value that fits on one line is its own block, and marking it adds
/// nothing over the cursor already being there.
#[test]
fn a_one_line_value_has_no_block() {
    let mut s = block_session();
    s.grid.cursor = (5, 0); // `<e>short</e>`
    assert_eq!(s.value_block(), None);
}

/// Editing the block hands over the source as written — a CDATA section
/// holds its markup literally, and re-encoding it would rewrite the file.
#[test]
fn the_block_editor_opens_on_the_value_and_writes_it_back() {
    let mut s = block_session();
    s.grid.cursor = (3, 0);
    let text = s.large_edit_text().expect("a block to edit");
    // The value, not its wrapper: the tags cannot be broken by an edit.
    assert!(text.starts_with("<p>one"), "{text}");
    assert!(!text.contains("<![CDATA["), "{text}");
    assert!(!text.contains("</d>"), "{text}");

    s.commit_large_edit(&text.replace("two", "three"));
    let out = String::from_utf8(s.doc.serialize()).unwrap();
    assert!(out.contains("<li>three</li>"), "{out}");
    assert!(out.contains("<![CDATA["), "the wrapper survived: {out}");
    assert!(!out.contains("&lt;li&gt;"), "the CDATA was encoded: {out}");
}

/// A feed puts encoded HTML inside CDATA. Reading that as `&lt;p&gt;` is
/// no use to anybody, so it is decoded to edit and encoded on the way
/// back — the wrapper and the tags around it untouched either way.
#[test]
fn a_block_of_encoded_markup_is_edited_decoded() {
    let src = "<rss>\n  <item>\n    <description><![CDATA[&lt;p&gt;One\n&lt;p&gt;Two]]></description>\n  </item>\n</rss>";
    let mut s = xml_session(src);
    s.execute(Command::ViewText);
    s.grid.cursor = (3, 0);

    let text = s.large_edit_text().expect("a block to edit");
    assert_eq!(text, "<p>One\n<p>Two", "decoded to read: {text:?}");

    s.commit_large_edit("<p>One\n<p>Three & more");
    let out = String::from_utf8(s.doc.serialize()).unwrap();
    assert!(out.contains("&lt;p&gt;Three &amp; more"), "{out}");
    assert!(out.contains("<![CDATA["), "the wrapper survived: {out}");
    assert!(out.contains("<description>"), "the tags survived: {out}");
}

/// Editing a line inside a CDATA section must not encode it: the markup
/// there is content the file holds literally.
#[test]
fn editing_a_line_inside_cdata_leaves_its_markup_alone() {
    // One line, so it is edited in place rather than as a block — which
    // is the path that used to encode the markup CDATA holds literally.
    let mut s = xml_session("<r>\n  <d><![CDATA[<p>two</p>]]></d>\n</r>");
    s.execute(Command::ViewText);
    s.grid.cursor = (1, 0);
    s.execute(Command::ReplaceCell);
    for c in "  <d><![CDATA[<p>three</p>]]></d>".chars() {
        s.input_char(c);
    }
    s.input_submit();
    let out = String::from_utf8(s.doc.serialize()).unwrap();
    assert!(
        out.contains("<p>three</p>"),
        "status={} out={out}",
        s.status
    );
    assert!(!out.contains("&lt;p&gt;"), "encoded content: {out}");
}

/// Clicking where you want to type is how every editor works.
#[test]
fn the_caret_can_be_put_where_it_was_clicked() {
    let mut s = block_session();
    s.grid.cursor = (5, 0);
    s.execute(Command::EditCell);
    s.set_entry_caret(3);
    assert_eq!(s.entry_caret(), 3);
    s.set_entry_caret(usize::MAX);
    assert_eq!(s.entry_caret(), s.entry().unwrap().1.len(), "clamped");
}

/// A channel holds its own `<title>` beside the items. Taking every
/// element at that level as a row made the feed one row of two columns,
/// `title` and `item`; the rows are the tag that repeats.
#[test]
fn metadata_beside_the_records_is_not_a_row() {
    let src = "<rss><channel><title>Feed</title>\
               <item><id>1</id><name>One</name></item>\
               <item><id>2</id><name>Two</name></item>\
               <item><id>3</id><name>Three</name></item></channel></rss>";
    let mut s = xml_session(src);
    s.execute(Command::ViewTable);
    let (headers, rows, cols) = s.table_dims();
    assert_eq!(rows, 3, "one row per item");
    assert_eq!(headers, vec!["id", "name"], "and no `title` column");
    assert_eq!(cols, 2);
    assert_eq!(s.table_cell(2, 1).as_deref(), Some("Three"));
}

/// Widening works on an XML sheet, not only on CSV.
#[test]
fn an_xml_column_can_be_widened() {
    let src = "<rss><channel><item><a>1</a></item><item><a>2</a></item></channel></rss>";
    let mut s = xml_session(src);
    s.execute(Command::ViewTable);
    s.grid.cursor = (0, 0);
    let before = s.widths()[0];
    s.execute(Command::WidenColumn);
    assert!(s.widths()[0] > before, "{:?} -> {:?}", before, s.widths());
}

/// Numeric columns are set against the right edge, so the digits line up.
/// CSV keeps its headings in row 0, and sampling those said every column
/// was text.
#[test]
fn a_numeric_column_is_recognised_past_the_heading_row() {
    let mut s = csv_session("sku,qty,price\nA-1,142,64.20\nB-2,61,22.10\n");
    s.execute(Command::ViewTable);
    assert!(!s.column_is_numeric(0), "sku is text");
    assert!(s.column_is_numeric(1), "qty is numeric");
    assert!(s.column_is_numeric(2), "price is numeric");
}

/// A unit or a thousands separator does not stop it reading as a number,
/// but a word does.
#[test]
fn numeric_detection_allows_units_and_rejects_words() {
    let mut s = csv_session("a,b\n1,299.00 AUD\n2,\"1,099.00 AUD\"\n");
    s.execute(Command::ViewTable);
    assert!(s.column_is_numeric(1));

    let mut s = csv_session("a,b\n1,in stock\n2,out of stock\n");
    s.execute(Command::ViewTable);
    assert!(!s.column_is_numeric(1));
}

/// A date starts with digits and is not a number: scanning only the
/// leading digits right-aligned every date column.
#[test]
fn a_date_column_is_not_numeric() {
    let mut s = csv_session("sku,updated\nA-1,2026-08-19\nB-2,2026-08-21\n");
    s.execute(Command::ViewTable);
    assert!(!s.column_is_numeric(1));
}

/// The inspector reads a row downwards, which is the only way to see a
/// feed's twenty-third column in a window that shows five.
#[test]
fn the_inspector_reads_the_row_under_the_cursor() {
    let src = "<rss><channel>\
               <item><g:id>A-1</g:id><title>Trailhead Daypack</title><link>https://example.com/a</link><g:price>129.00</g:price></item>\
               <item><g:id>B-2</g:id><title>Kettle Set</title><link>https://example.com/b</link><g:price>54.50</g:price></item>\
               </channel></rss>";
    let mut s = xml_session(src);
    s.execute(Command::ViewTable);
    s.grid.cursor = (1, 0);

    let it = s.inspector();
    assert_eq!(it.meta, "Row 2 of 2");
    assert_eq!(it.title, "Kettle Set", "named by a field with a name in it");
    let keys: Vec<&str> = it.fields.iter().map(|f| f.key.as_str()).collect();
    assert_eq!(keys, vec!["g:id", "title", "link", "g:price"]);
    assert_eq!(it.fields[1].value, "Kettle Set");

    // Kinds are for colour only — nothing is coerced by looking at it.
    use vuwr_core::FieldKind;
    assert_eq!(it.fields[2].kind, FieldKind::Url);
    assert_eq!(it.fields[3].kind, FieldKind::Number);
    assert_eq!(it.fields[0].kind, FieldKind::Text);
}

/// Outside a table there is no record, so it falls back to the one value
/// the cursor is on rather than showing nothing.
#[test]
fn the_inspector_falls_back_to_the_value_under_the_cursor() {
    let mut s = session(r#"{"a":1,"b":2}"#);
    s.execute(Command::ViewTree);
    s.grid.cursor = (1, 0);
    let it = s.inspector();
    assert_eq!(it.fields.len(), 1);
    assert_eq!(it.fields[0].key, "b");
}

// --- Selection while editing ---

fn editing(src: &str) -> Session {
    let mut s = csv_session(src);
    s.execute(Command::ViewTable);
    s.grid.cursor = (1, 1);
    s.execute(Command::EditCell);
    s
}

/// Select all, then type: the value is replaced, not appended to. Without
/// a selection there is nothing for ⌘A, ⌘C or ⌘X to act on.
#[test]
fn selecting_everything_and_typing_replaces_the_value() {
    let mut s = editing("a,b\n1,old value\n");
    s.select_all();
    assert_eq!(s.selected_text().as_deref(), Some("old value"));
    s.input_char('n');
    assert_eq!(s.entry().unwrap().1, "n");
    assert!(s.selected_text().is_none(), "typing collapses it");
}

/// Copy takes the selection; with none, it takes the whole value — the
/// rule an address bar follows.
#[test]
fn copy_takes_the_selection_or_the_whole_value() {
    let mut s = editing("a,b\n1,hello\n");
    assert_eq!(s.entry_text().as_deref(), Some("hello"));
    s.input_home();
    s.input_select_right();
    s.input_select_right();
    assert_eq!(s.entry_text().as_deref(), Some("he"));
}

#[test]
fn cutting_removes_the_selection_and_hands_it_back() {
    let mut s = editing("a,b\n1,hello\n");
    s.input_home();
    s.input_select_right();
    s.input_select_right();
    assert_eq!(s.input_cut().as_deref(), Some("he"));
    assert_eq!(s.entry().unwrap().1, "llo");
    assert_eq!(s.entry_caret(), 0);
}

/// A paste lands at the caret, over whatever was selected.
#[test]
fn pasting_replaces_the_selection() {
    let mut s = editing("a,b\n1,hello\n");
    s.select_all();
    s.input_text("goodbye");
    assert_eq!(s.entry().unwrap().1, "goodbye");
}

/// Backspace with a selection deletes the selection, not one character.
#[test]
fn backspace_deletes_the_selection() {
    let mut s = editing("a,b\n1,hello\n");
    s.input_select_home();
    s.input_backspace();
    assert_eq!(s.entry().unwrap().1, "");
}

/// A plain move drops the selection; a shifted one extends it.
#[test]
fn moving_collapses_the_selection_and_shift_extends_it() {
    let mut s = editing("a,b\n1,hello\n");
    s.select_all();
    s.input_left();
    assert!(s.selected_text().is_none());
    s.input_select_end();
    assert_eq!(s.selected_text().as_deref(), Some("hello"));
}

/// Double-click takes a word, and an identifier is one word rather than
/// three: `SKU-1001` and `g:price` hold together.
#[test]
fn selecting_a_word_keeps_an_identifier_whole() {
    let mut s = editing("a,b\n1,SKU-1001 spare\n");
    s.select_word_at(2);
    assert_eq!(s.selected_text().as_deref(), Some("SKU-1001"));

    s.select_word_at(9);
    assert_eq!(s.selected_text().as_deref(), Some("spare"));

    // A click in the space between words takes the space.
    s.select_word_at(8);
    assert_eq!(s.selected_text().as_deref(), Some(" "));
}

/// Dragging keeps the anchor where the press landed.
#[test]
fn extending_moves_the_caret_and_keeps_the_anchor() {
    let mut s = editing("a,b\n1,hello\n");
    s.set_entry_caret(1);
    s.extend_entry_selection(4);
    assert_eq!(s.selected_text().as_deref(), Some("ell"));
}

// --- Putting a column away ---

/// Hiding is a view. The column leaves the table and the document keeps
/// it: what you save is what you opened.
#[test]
fn a_hidden_column_leaves_the_table_and_not_the_file() {
    let src = "sku,name,qty\nA-1,Widget,7\n";
    let mut s = csv_session(src);
    s.execute(Command::ViewTable);
    s.toggle_column(1);

    let (headers, _, cols) = s.table_dims();
    assert_eq!(headers, vec!["sku", "qty"]);
    assert_eq!(cols, 2);
    assert_eq!(s.table_cell(1, 1).as_deref(), Some("7"), "qty moved left");
    assert_eq!(String::from_utf8(s.doc.serialize()).unwrap(), src);

    s.execute(Command::ShowAllColumns);
    assert_eq!(s.table_dims().2, 3);
}

/// The display and the document disagree about what column 1 is once
/// something is hidden. An edit has to follow the document, or it writes
/// to the wrong field.
#[test]
fn editing_past_a_hidden_column_writes_to_the_right_field() {
    let mut s = csv_session("sku,name,qty\nA-1,Widget,7\n");
    s.execute(Command::ViewTable);
    s.toggle_column(1); // away with `name`
    s.grid.cursor = (1, 1); // which is now `qty`
    s.execute(Command::EditCell);
    s.select_all();
    s.input_char('9');
    s.input_submit();

    let out = String::from_utf8(s.doc.serialize()).unwrap();
    assert_eq!(out, "sku,name,qty\nA-1,Widget,9\n", "{out}");
}

/// Sorting reorders by a document column, so it has to map too.
#[test]
fn sorting_past_a_hidden_column_sorts_the_right_one() {
    let mut s = csv_session("sku,name,qty\nA,Zeta,2\nB,Alpha,1\n");
    s.execute(Command::ViewTable);
    s.toggle_column(1);
    s.grid.cursor = (1, 1); // `qty`
    s.execute(Command::Sort);
    assert_eq!(s.table_cell(1, 1).as_deref(), Some("1"));
    assert_eq!(s.table_cell(1, 0).as_deref(), Some("B"));
}

/// An empty table is not a view of anything.
#[test]
fn the_last_column_stays() {
    let mut s = csv_session("sku,name\nA-1,Widget\n");
    s.execute(Command::ViewTable);
    s.toggle_column(0);
    s.toggle_column(1);
    assert_eq!(s.table_dims().2, 1);
    assert!(s.status.contains("last column"), "{}", s.status);
}

// --- Outliers, reported and never rewritten ---

/// A column of numbers with one `129,00` in it will sort wrong, and
/// whatever wrote that row had a different idea of the format. Lint says
/// so; nothing is changed.
#[test]
fn a_value_that_disagrees_with_its_column_is_reported() {
    let mut rows = String::from("sku,price\n");
    for i in 0..20 {
        rows.push_str(&format!("SKU-{i},{}.00\n", 100 + i));
    }
    rows.push_str("SKU-99,\"129,00\"\n");
    let before = rows.clone();

    let mut s = csv_session(&rows);
    s.execute(Command::ViewTable);
    s.execute(Command::Lint);
    let found = s.lint_results().expect("linted");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].message.contains("price reads as a number"));
    assert!(found[0].message.contains("129,00"));
    assert_eq!(found[0].line, 22, "the row it is on");

    // Reporting is not rewriting.
    assert_eq!(String::from_utf8(s.doc.serialize()).unwrap(), before);
}

/// A column that is genuinely mixed is a choice, not a mistake.
#[test]
fn a_mixed_column_says_nothing() {
    let mut rows = String::from("sku,note\n");
    for i in 0..20 {
        rows.push_str(&format!(
            "SKU-{i},{}\n",
            if i % 2 == 0 { "12" } else { "n/a" }
        ));
    }
    let mut s = csv_session(&rows);
    s.execute(Command::ViewTable);
    s.execute(Command::Lint);
    assert_eq!(s.lint_results(), Some(&[][..]));
}

/// Too few rows to have an opinion about.
#[test]
fn a_short_column_says_nothing() {
    let mut s = csv_session("sku,price\nA,1.00\nB,2.00\nC,\"3,00\"\n");
    s.execute(Command::ViewTable);
    s.execute(Command::Lint);
    assert_eq!(s.lint_results(), Some(&[][..]));
}

/// `:scheme` with no name says what there is; with one, it takes.
#[test]
fn the_scheme_command_lists_and_chooses() {
    let mut s = session(r#"{"a":1}"#);
    s.execute(Command::OpenPalette);
    for c in "scheme".chars() {
        s.input_char(c);
    }
    s.input_submit();
    assert!(s.status.contains("Gruvbox dark"), "{}", s.status);
    assert_eq!(s.scheme(), vuwr_core::Scheme::Vuwr, "listing changes nothing");

    s.execute(Command::OpenPalette);
    for c in "scheme gruvbox-dark".chars() {
        s.input_char(c);
    }
    let effect = s.input_submit();
    assert_eq!(s.scheme(), vuwr_core::Scheme::GruvboxDark);
    assert!(
        matches!(effect, vuwr_core::Effect::SchemeChanged(_)),
        "the frontend has to be told: {effect:?}"
    );

    s.execute(Command::OpenPalette);
    for c in "scheme nope".chars() {
        s.input_char(c);
    }
    s.input_submit();
    assert!(s.status.contains("no scheme"), "{}", s.status);
    assert_eq!(s.scheme(), vuwr_core::Scheme::GruvboxDark, "unchanged");
}
