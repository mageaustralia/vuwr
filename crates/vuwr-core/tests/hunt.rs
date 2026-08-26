//! Features that work alone, used together.
//!
//! Each of these was found by asking what happens when two things that
//! were tested separately meet. Replacing was tested; hiding a column was
//! tested; replacing with a column hidden was not, and it quietly rewrote
//! the column you had put away.

use vuwr_core::{Command, Document, FormatHint, Session, ViewMode};

fn csv(rows: &str) -> Session {
    Session::new(Document::parse(rows.as_bytes(), FormatHint::Csv).unwrap())
}
fn text(s: &Session) -> String {
    String::from_utf8(s.doc.serialize()).unwrap()
}
fn substitute(s: &mut Session, pattern: &str, with: &str) {
    s.execute(Command::Substitute);
    s.select_all();
    s.input_text(pattern);
    s.input_submit();
    s.input_text(with);
    s.input_submit();
}

const STOCK: &str = "sku,city,note\nA1,Sydney,Sydney office\nA2,Perth,Perth office\n";

/// A column you have put away is not one you asked to change.
#[test]
fn replacing_leaves_hidden_columns_alone() {
    let mut s = csv(STOCK);
    s.execute(Command::ViewTable);
    s.grid.cursor = (0, 2);
    s.execute(Command::HideColumn);

    substitute(&mut s, "Sydney", "Hobart");
    assert!(
        s.status.contains("hidden"),
        "the hidden column was not mentioned: {}",
        s.status
    );
    s.execute(Command::SubstituteAll);

    let out = text(&s);
    assert!(
        out.contains("A1,Hobart,"),
        "the visible column was missed: {out}"
    );
    assert!(
        out.contains("Sydney office"),
        "the hidden column was rewritten out of sight: {out}"
    );
}

/// Outside the table the cursor is a node or a line, so a row and a column
/// taken from it name some other cell entirely.
#[test]
fn replacing_is_refused_where_the_cursor_is_not_a_cell() {
    let xml = "<r><item><sku>A1</sku><city>Sydney</city></item>\
               <item><sku>A2</sku><city>Perth</city></item></r>";
    let mut s = Session::new(Document::parse(xml.as_bytes(), FormatHint::Xml).unwrap());
    s.execute(Command::ViewTree);

    // The document has a sheet — it is table-shaped — so the guard has to
    // be about the view, not about whether cells exist somewhere.
    assert!(s.doc.sheet().is_some());
    s.execute(Command::Substitute);
    assert!(
        s.status.contains("table"),
        "replacing armed itself in the tree: {}",
        s.status
    );
    assert!(!s.is_entering_text(), "a prompt opened anyway");

    // And it works once you are in the table.
    s.execute(Command::ViewTable);
    substitute(&mut s, "Sydney", "Hobart");
    s.execute(Command::SubstituteAll);
    assert!(text(&s).contains("Hobart"), "{}", text(&s));
}

/// Sorting reorders the display, not the document.
#[test]
fn replacing_under_a_sort_changes_the_rows_it_names() {
    let mut s = csv(STOCK);
    s.execute(Command::ViewTable);
    s.grid.cursor = (0, 1);
    s.execute(Command::Sort);
    substitute(&mut s, "Perth", "Darwin");
    s.execute(Command::SubstituteAll);
    assert!(text(&s).contains("A2,Darwin,Darwin office"), "{}", text(&s));
}

/// A batch has to redo as cleanly as it undoes.
#[test]
fn a_batch_redoes() {
    let mut s = csv(STOCK);
    s.execute(Command::ViewTable);
    substitute(&mut s, "Sydney", "Hobart");
    s.execute(Command::SubstituteAll);
    let after = text(&s);

    assert!(s.doc.undo());
    assert_eq!(text(&s), STOCK);
    assert!(s.doc.redo());
    assert_eq!(text(&s), after, "redo did not restore the batch");
}

/// Replacing is an edit like any other, so the rest of the file survives
/// it untouched — quoting, line endings and all.
#[test]
fn replacing_leaves_the_rest_of_the_file_as_it_was() {
    let src = "sku,note\r\nA1,\"quoted, value\"\r\nA2,Sydney\r\n";
    let mut s = csv(src);
    s.execute(Command::ViewTable);
    substitute(&mut s, "Sydney", "Hobart");
    s.execute(Command::SubstituteAll);

    let out = text(&s);
    assert!(out.contains("\r\n"), "line endings changed: {out:?}");
    assert!(
        out.contains("\"quoted, value\""),
        "the quoted cell was rewritten: {out:?}"
    );
}

/// What the filter hid stays hidden and unchanged, after the filter goes.
#[test]
fn a_filtered_replacement_holds_up_once_the_filter_is_cleared() {
    let mut s = csv(STOCK);
    s.execute(Command::ViewTable);
    s.execute(Command::Filter);
    s.input_text("Sydney");
    s.input_submit();

    substitute(&mut s, "office", "HQ");
    s.execute(Command::SubstituteAll);
    s.execute(Command::ClearFilter);

    let out = text(&s);
    assert!(out.contains("Sydney HQ"), "{out}");
    assert!(
        out.contains("Perth office"),
        "a hidden row was changed: {out}"
    );
}

/// The view is part of the guard, so it has to be checked where it is.
#[test]
fn the_table_is_where_replacing_is_offered() {
    let mut s = csv(STOCK);
    for (view, cmd, allowed) in [
        (ViewMode::Table, Command::ViewTable, true),
        (ViewMode::Text, Command::ViewText, false),
    ] {
        s.execute(cmd);
        assert_eq!(s.can_substitute(), allowed, "{view:?}");
    }
}

/// Replacing across a feed, which is the document this exists for.
///
/// The first version built its batch from raw `SetCell` ops, which only
/// CSV understands — JSON and XML address their values by node. On an XML
/// feed "replace all" reported nothing and changed nothing.
#[test]
fn replacing_works_on_xml_and_json_too() {
    let cases: [(&str, FormatHint, &str); 2] = [
        (
            "xml",
            FormatHint::Xml,
            "<r>\n<item><sku>A1</sku><city>Sydney</city></item>\n\
             <item><sku>A2</sku><city>Sydney</city></item>\n</r>\n",
        ),
        (
            "json",
            FormatHint::Json,
            "[\n  {\"sku\": \"A1\", \"city\": \"Sydney\"},\n  \
             {\"sku\": \"A2\", \"city\": \"Sydney\"}\n]\n",
        ),
    ];
    for (name, hint, src) in cases {
        let mut s = Session::new(Document::parse(src.as_bytes(), hint).unwrap());
        s.execute(Command::ViewTable);
        substitute(&mut s, "Sydney", "Hobart");
        s.execute(Command::SubstituteAll);

        let out = text(&s);
        assert!(
            !out.contains("Sydney") && out.matches("Hobart").count() == 2,
            "{name}: {out}"
        );
        assert!(
            s.status.contains("replaced 2"),
            "{name} reported {:?}",
            s.status
        );

        // And one undo puts the whole feed back, byte for byte.
        assert!(s.doc.undo());
        assert_eq!(text(&s), src, "{name}: undo was not exact");
    }
}

/// The inspector shows the record, wherever in it the cursor is.
///
/// In the tree it showed the row's own label and summary, so standing on
/// an `<item>` gave a panel reading `item : item` — true, and of no use to
/// anybody. The record is the nearest container either way: standing on a
/// field shows the item that holds it, standing on the item shows the same.
#[test]
fn the_inspector_shows_the_record_in_the_tree() {
    let xml = "<r>\n<item><sku>A1</sku><city>Sydney</city><price>19.95</price></item>\n\
               <item><sku>A2</sku><city>Perth</city><price>29.95</price></item>\n</r>\n";
    let mut s = Session::new(Document::parse(xml.as_bytes(), FormatHint::Xml).unwrap());
    s.execute(Command::ViewTree);

    // On the first item, closed.
    s.grid.cursor = (0, 0);
    let seen = s.inspector();
    let keys: Vec<&str> = seen.fields.iter().map(|f| f.key.as_str()).collect();
    assert_eq!(keys, ["sku", "city", "price"], "{:?}", seen.fields);
    assert_eq!(seen.fields[0].value, "A1");

    // Open it and stand on one of its fields: the same record.
    s.execute(Command::DrillDown);
    s.execute(Command::MoveDown);
    s.execute(Command::MoveDown);
    let inside = s.inspector();
    assert_eq!(
        inside
            .fields
            .iter()
            .map(|f| f.key.as_str())
            .collect::<Vec<_>>(),
        ["sku", "city", "price"],
        "standing on a field showed something else: {:?}",
        inside.fields
    );
}

/// In the source view the useful unit is the value, not the line.
///
/// The panel showed the one line the cursor was on, labelled `line 4` —
/// beside the twenty lines already on screen. For a description that runs
/// over twenty lines, the value is the thing worth reading downwards.
#[test]
fn the_inspector_shows_the_whole_value_in_the_source() {
    let xml = "<r>\n<item>\n<sku>A1</sku>\n\
               <description>Line one\nLine two\nLine three</description>\n\
               </item>\n</r>\n";
    let mut s = Session::new(Document::parse(xml.as_bytes(), FormatHint::Xml).unwrap());
    s.execute(Command::ViewText);

    // On the description: the whole of it, named by its tag.
    s.grid.cursor = (3, 0);
    assert!(s.can_inspect());
    let it = s.inspector();
    assert_eq!(it.fields[0].key, "description");
    assert_eq!(it.fields[0].value, "Line one\nLine two\nLine three");

    // On a line that is not inside a value there is nothing to show, and
    // the panel says so rather than repeating the line.
    s.grid.cursor = (2, 0);
    assert!(
        !s.can_inspect(),
        "offered a detail of a self-contained line"
    );

    // And the root element is not a value: its contents are the file.
    s.grid.cursor = (0, 0);
    assert!(!s.can_inspect(), "offered the whole document as one field");
}

/// The table always has a record, so the panel is always worth opening.
#[test]
fn the_table_always_has_something_to_inspect() {
    let mut s = csv(STOCK);
    s.execute(Command::ViewTable);
    assert!(s.can_inspect());
}

/// Clicking a field in the panel acts on that field.
///
/// The panel shows the record wherever in it the cursor happens to be, so
/// acting on "the cursor's value" edited whichever row the cursor had
/// been left on — `sku` when you had double-clicked `price`.
#[test]
fn a_field_in_the_panel_is_the_field_you_clicked() {
    let xml = "<r>\n<item><sku>A1</sku><city>Sydney</city><price>19.95</price></item>\n</r>\n";
    let mut s = Session::new(Document::parse(xml.as_bytes(), FormatHint::Xml).unwrap());
    s.execute(Command::ViewTree);
    s.grid.cursor = (0, 0);

    let fields = s.inspector().fields;
    assert_eq!(fields[2].key, "price");

    // The third field, from the item's own row — which is collapsed, so
    // this has to open it as well as move.
    s.focus_record_field(2);
    let row = &s.tree_rows[s.grid.cursor.0];
    assert_eq!(row.label, "price", "landed on {:?}", row.label);
    assert_eq!(row.summary, "19.95");
}

/// And in the table it is the column, hidden ones skipped.
#[test]
fn a_field_in_the_panel_is_the_column_you_clicked() {
    let mut s = csv(STOCK);
    s.execute(Command::ViewTable);
    s.grid.cursor = (0, 1);
    s.execute(Command::HideColumn);

    // `city` is away, so the second field is `note` — column 2.
    let fields = s.inspector().fields;
    assert_eq!(fields[1].key, "note", "{fields:?}");
    s.focus_record_field(1);
    assert_eq!(s.grid.cursor.1, 2);
}

/// A file opens showing what is in it.
///
/// A feed opened on one closed line reading `channel : <channel>` — true,
/// and no use to anybody. It opens the way down to the first record now,
/// and puts the cursor on it so the panel beside has something to show.
#[test]
fn a_tree_opens_on_its_first_record() {
    let xml = "<rss><channel><title>Feed</title>\
               <item><g:id>A1</g:id><g:price>19.95</g:price></item>\
               <item><g:id>A2</g:id><g:price>29.95</g:price></item></channel></rss>";
    let s = Session::new(Document::parse(xml.as_bytes(), FormatHint::Xml).unwrap());

    let labels: Vec<&str> = s.tree_rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(
        labels,
        ["channel", "title", "item", "g:id", "g:price", "item"],
        "the first item was not opened"
    );
    assert_eq!(
        s.tree_rows[s.grid.cursor.0].label, "item",
        "the cursor is not on the record"
    );
    // Which is what makes the panel useful the moment the file opens.
    assert!(s.can_inspect());
    assert_eq!(s.inspector().fields.len(), 2);
}

/// A document with nothing to open is left alone rather than unrolled.
#[test]
fn a_flat_document_opens_flat() {
    let s = Session::new(Document::parse(br#"{"a":1,"b":2}"#, FormatHint::Json).unwrap());
    assert_eq!(s.tree_rows.len(), 2);
}
