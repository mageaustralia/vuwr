//! Undo and redo, in every view, for every format.
//!
//! There were plenty of undo tests — a hundred-odd assertions — but each
//! covered one operation, and redo was named seven times in the whole
//! suite. Nothing checked the thing a reader actually relies on: that an
//! edit made anywhere can be taken back and put again, and that what
//! comes back is the file byte for byte.
//!
//! Byte for byte is the point. A document that undoes to something
//! *equivalent* has still rewritten the file, and this tool's promise is
//! that it does not.

use vuwr_core::{Command, Document, FormatHint, Session, ViewMode};

const CSV: &str = "sku,city\nA1,Sydney\nA2,Perth\n";
const JSON: &str = "[\n  {\"sku\": \"A1\", \"city\": \"Sydney\"},\n  \
                    {\"sku\": \"A2\", \"city\": \"Perth\"}\n]\n";
const XML: &str = "<r>\n  <item><sku>A1</sku><city>Sydney</city></item>\n  \
                   <item><sku>A2</sku><city>Perth</city></item>\n</r>\n";

fn text(s: &Session) -> String {
    String::from_utf8(s.doc.serialize()).unwrap()
}

/// Put the cursor somewhere editable in this view, and return whether
/// there was anywhere to put it.
fn place_cursor(s: &mut Session) -> bool {
    match s.view_mode() {
        ViewMode::Table => {
            let (_, rows, cols) = s.table_dims();
            if rows == 0 || cols == 0 {
                return false;
            }
            // Past the header, where a CSV keeps its column names.
            let row = usize::from(!s.has_separate_header()).min(rows - 1);
            s.grid.cursor = (row, cols - 1);
            true
        }
        ViewMode::Tree => {
            // The first leaf: a container has no value of its own.
            match s.tree_rows.iter().position(|r| !r.is_container()) {
                Some(row) => {
                    s.grid.cursor = (row, 0);
                    true
                }
                None => false,
            }
        }
        ViewMode::Text => {
            // A line holding a value, rather than one of pure structure.
            let (_, lines, _) = s.table_dims();
            (0..lines).any(|line| {
                s.grid.cursor = (line, 0);
                s.table_cell(line, 0).is_some_and(|l| l.contains("Sydney"))
            })
        }
    }
}

/// Edit whatever the cursor is on, replacing its text.
fn edit_here(s: &mut Session, replacement: &str) {
    s.execute(Command::EditCell);
    s.select_all();
    s.input_text(replacement);
    s.input_submit();
}

/// What to type in each view: the text view edits a whole source line, so
/// it has to be given one that keeps the document well formed.
fn replacement(view: ViewMode, format: &str) -> String {
    match (view, format) {
        (ViewMode::Text, "csv") => "A1,Hobart".to_string(),
        (ViewMode::Text, "json") => "  {\"sku\": \"A1\", \"city\": \"Hobart\"},".to_string(),
        (ViewMode::Text, "xml") => "  <item><sku>A1</sku><city>Hobart</city></item>".to_string(),
        _ => "Hobart".to_string(),
    }
}

#[test]
fn an_edit_undoes_and_redoes_in_every_view_of_every_format() {
    let formats: [(&str, FormatHint, &str); 3] = [
        ("csv", FormatHint::Csv, CSV),
        ("json", FormatHint::Json, JSON),
        ("xml", FormatHint::Xml, XML),
    ];
    let views = [
        (ViewMode::Table, Command::ViewTable),
        (ViewMode::Tree, Command::ViewTree),
        (ViewMode::Text, Command::ViewText),
    ];

    // Collected rather than asserted one at a time: a matrix that stops
    // at the first failure tells you about one cell when you want the
    // shape of the damage.
    let mut covered = Vec::new();
    let mut wrong: Vec<String> = Vec::new();
    for (name, hint, src) in formats {
        for (view, to_view) in views {
            let mut s = Session::new(Document::parse(src.as_bytes(), hint).unwrap());
            if !s.available_views().contains(&view) {
                continue;
            }
            s.execute(to_view);
            assert_eq!(s.view_mode(), view, "{name}: could not reach {view:?}");
            if !place_cursor(&mut s) {
                continue;
            }
            let at = format!("{name}/{view:?}");
            covered.push(at.clone());

            let before = text(&s);
            if before != src {
                wrong.push(format!("{at}: changed by merely opening it"));
                continue;
            }

            edit_here(&mut s, &replacement(view, name));
            let after = text(&s);
            if after == before {
                wrong.push(format!("{at}: the edit did nothing — {}", s.status));
                continue;
            }
            // The edit landed where it was aimed, rather than somewhere
            // else that happens to differ.
            if !after.contains("Hobart") {
                wrong.push(format!("{at}: edited something else"));
                continue;
            }

            if !s.doc.undo() {
                wrong.push(format!("{at}: undo refused"));
                continue;
            }
            if text(&s) != before {
                wrong.push(format!("{at}: undo was not exact"));
                continue;
            }
            if !s.doc.redo() {
                wrong.push(format!("{at}: redo refused"));
                continue;
            }
            if text(&s) != after {
                wrong.push(format!("{at}: redo was not exact"));
                continue;
            }
            // And back again, because one round trip can hide a state
            // that only the second exposes.
            if !s.doc.undo() || text(&s) != before {
                wrong.push(format!("{at}: the second undo differed"));
            }
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));

    // Every pair that exists must have been exercised, or a view quietly
    // dropping out of `available_views` would empty this test in silence.
    assert_eq!(
        covered,
        [
            "csv/Table",
            "csv/Text",
            "json/Table",
            "json/Tree",
            "json/Text",
            "xml/Table",
            "xml/Tree",
            "xml/Text",
        ],
        "the matrix has holes"
    );
}

/// The structural edits, which undo by a different route.
///
/// Setting a value swaps one node; removing, inserting and renaming
/// change the document's shape, and each has its own inverse. They are
/// exercised here through the session, as a reader reaches them.
#[test]
fn structural_edits_undo_and_redo_exactly() {
    let cases: [(&str, FormatHint, &str); 2] = [
        ("json", FormatHint::Json, JSON),
        ("xml", FormatHint::Xml, XML),
    ];
    let mut wrong: Vec<String> = Vec::new();

    for (name, hint, src) in cases {
        type Structural = fn(&mut Session);
        let operations: [(&str, Structural); 3] = [
            ("remove", |s| s.remove_at_cursor()),
            ("duplicate", |s| s.duplicate_at_cursor()),
            ("insert", |s: &mut Session| {
                s.insert_after_cursor(vuwr_core::NewNode::Value);
            }),
        ];
        for (what, apply) in operations {
            let mut s = Session::new(Document::parse(src.as_bytes(), hint).unwrap());
            s.execute(Command::ViewTree);
            // A leaf, so the operation has something ordinary to work on.
            let Some(row) = s.tree_rows.iter().position(|r| !r.is_container()) else {
                wrong.push(format!("{name}: no leaf to {what}"));
                continue;
            };
            s.grid.cursor = (row, 0);

            let before = text(&s);
            apply(&mut s);
            let after = text(&s);
            let at = format!("{name}/{what}");
            if after == before {
                wrong.push(format!("{at}: did nothing — {}", s.status));
                continue;
            }
            if !s.doc.undo() || text(&s) != before {
                wrong.push(format!("{at}: undo was not exact"));
                continue;
            }
            if !s.doc.redo() || text(&s) != after {
                wrong.push(format!("{at}: redo was not exact"));
            }
        }

        // Renaming applies to object keys, which is JSON's business.
        if name == "json" {
            let mut s = Session::new(Document::parse(src.as_bytes(), hint).unwrap());
            s.execute(Command::ViewTree);
            if let Some(row) = s.tree_rows.iter().position(|r| !r.is_container()) {
                s.grid.cursor = (row, 0);
                let before = text(&s);
                s.execute(Command::RenameKey);
                s.select_all();
                s.input_text("code");
                s.input_submit();
                let after = text(&s);
                if after == before {
                    wrong.push(format!("{name}/rename: did nothing — {}", s.status));
                } else if !s.doc.undo() || text(&s) != before {
                    wrong.push(format!("{name}/rename: undo was not exact"));
                } else if !s.doc.redo() || text(&s) != after {
                    wrong.push(format!("{name}/rename: redo was not exact"));
                }
            }
        }
    }

    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

/// Undo runs out, and says so rather than pretending.
#[test]
fn undo_stops_at_the_beginning() {
    let mut s = Session::new(Document::parse(CSV.as_bytes(), FormatHint::Csv).unwrap());
    s.execute(Command::ViewTable);
    assert!(
        !s.doc.can_undo(),
        "something to undo before anything was done"
    );
    assert!(!s.doc.undo(), "undid a document nobody had edited");

    s.grid.cursor = (1, 1);
    edit_here(&mut s, "Hobart");
    assert!(s.doc.undo());
    assert!(!s.doc.undo(), "undo went past the beginning");
    assert_eq!(text(&s), CSV);
}

/// Inserting into XML puts an element there.
///
/// It used to insert a JSON-shaped value, which an element cannot hold: it
/// serialised to nothing and was not a tree row either, so all three of
/// value, object and array reported success over a document that had not
/// changed — and left it marked unsaved for an edit nobody could see.
#[test]
fn inserting_into_xml_adds_an_element() {
    let xml = "<r>\n  <item><sku>A1</sku></item>\n</r>\n";
    for what in [
        vuwr_core::NewNode::Value,
        vuwr_core::NewNode::Object,
        vuwr_core::NewNode::Array,
    ] {
        let mut s = Session::new(Document::parse(xml.as_bytes(), FormatHint::Xml).unwrap());
        s.execute(Command::ViewTree);
        let row = s.tree_rows.iter().position(|r| !r.is_container()).unwrap();
        s.grid.cursor = (row, 0);

        let rows_before = s.tree_rows.len();
        s.insert_after_cursor(what);

        assert_eq!(
            text(&s),
            "<r>\n  <item><sku>A1</sku><sku></sku></item>\n</r>\n",
            "{what:?}: named after the sibling it went in beside"
        );
        assert_eq!(
            s.tree_rows.len(),
            rows_before + 1,
            "{what:?}: nothing appeared in the tree"
        );
        assert!(s.doc.undo());
        assert_eq!(text(&s), xml, "{what:?}: undo was not exact");
    }
}
