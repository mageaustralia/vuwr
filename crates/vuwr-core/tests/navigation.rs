//! Moving about, in every view.
//!
//! The cursor is the thing every other feature is aimed through — an edit,
//! a replacement, the panel, the jump a search makes — and nothing tested
//! its edges. A cursor that runs off the end, or stops one short, is a
//! wrong answer everywhere downstream.

use vuwr_core::{Command, Document, FormatHint, Session, ViewMode};

const CSV: &str = "sku,city\nA1,Sydney\nA2,Perth\nA3,Hobart\nA4,Darwin\n";
const XML: &str = "<r>\n  <item><sku>A1</sku><city>Sydney</city></item>\n  \
                   <item><sku>A2</sku><city>Perth</city></item>\n</r>\n";

fn csv() -> Session {
    Session::new(Document::parse(CSV.as_bytes(), FormatHint::Csv).unwrap())
}

fn views(src: &str, hint: FormatHint) -> Vec<(ViewMode, Session)> {
    [
        (ViewMode::Table, Command::ViewTable),
        (ViewMode::Tree, Command::ViewTree),
        (ViewMode::Text, Command::ViewText),
    ]
    .into_iter()
    .filter_map(|(view, cmd)| {
        let mut s = Session::new(Document::parse(src.as_bytes(), hint).unwrap());
        s.available_views().contains(&view).then(|| {
            s.execute(cmd);
            (view, s)
        })
    })
    .collect()
}

/// The cursor never leaves the document, however hard it is pushed.
#[test]
fn the_cursor_stays_inside_the_document() {
    for (src, hint) in [(CSV, FormatHint::Csv), (XML, FormatHint::Xml)] {
        for (view, mut s) in views(src, hint) {
            let (_, rows, cols) = s.table_dims();

            for _ in 0..50 {
                s.execute(Command::MoveUp);
                s.execute(Command::MoveLeft);
            }
            assert_eq!(s.grid.cursor, (0, 0), "{view:?}: pushed off the top-left");

            for _ in 0..50 {
                s.execute(Command::MoveDown);
                s.execute(Command::MoveRight);
            }
            assert!(
                s.grid.cursor.0 < rows.max(1),
                "{view:?}: row {} of {rows}",
                s.grid.cursor.0
            );
            assert!(
                s.grid.cursor.1 < cols.max(1),
                "{view:?}: column {} of {cols}",
                s.grid.cursor.1
            );

            // Paging cannot escape either.
            for _ in 0..20 {
                s.execute(Command::PageDown);
            }
            assert!(s.grid.cursor.0 < rows.max(1), "{view:?}: paged off the end");
            for _ in 0..20 {
                s.execute(Command::PageUp);
            }
            assert_eq!(s.grid.cursor.0, 0, "{view:?}: paged off the top");
        }
    }
}

/// Top and bottom go where they say.
#[test]
fn go_top_and_go_bottom_reach_the_ends() {
    for (src, hint) in [(CSV, FormatHint::Csv), (XML, FormatHint::Xml)] {
        for (view, mut s) in views(src, hint) {
            let (_, rows, _) = s.table_dims();
            s.execute(Command::GoBottom);
            assert_eq!(s.grid.cursor.0, rows - 1, "{view:?}: bottom");
            s.execute(Command::GoTop);
            assert_eq!(s.grid.cursor.0, 0, "{view:?}: top");
        }
    }
}

/// Left and right reach the ends of a row, and stay there.
#[test]
fn home_and_end_reach_the_ends_of_a_row() {
    let mut s = csv();
    s.execute(Command::ViewTable);
    let (_, _, cols) = s.table_dims();
    s.grid.cursor = (1, 0);

    s.execute(Command::GoRowEnd);
    assert_eq!(s.grid.cursor.1, cols - 1);
    s.execute(Command::GoRowEnd);
    assert_eq!(s.grid.cursor.1, cols - 1, "moved past the end");

    s.execute(Command::GoRowStart);
    assert_eq!(s.grid.cursor.1, 0);
    s.execute(Command::GoRowStart);
    assert_eq!(s.grid.cursor.1, 0, "moved past the start");
}

/// Moving does not change the document.
///
/// Sounds obvious; it is the sort of thing a stray `mark_changed` breaks,
/// and then everything is unsaved for no reason.
#[test]
fn moving_never_makes_the_document_dirty() {
    for (src, hint) in [(CSV, FormatHint::Csv), (XML, FormatHint::Xml)] {
        for (view, mut s) in views(src, hint) {
            for cmd in [
                Command::MoveDown,
                Command::MoveRight,
                Command::MoveUp,
                Command::MoveLeft,
                Command::PageDown,
                Command::PageUp,
                Command::GoBottom,
                Command::GoTop,
                Command::GoRowEnd,
                Command::GoRowStart,
            ] {
                s.execute(cmd);
            }
            assert!(!s.dirty, "{view:?}: moving marked the file unsaved");
            assert_eq!(
                String::from_utf8(s.doc.serialize()).unwrap(),
                src,
                "{view:?}: moving changed the document"
            );
        }
    }
}

/// A view change keeps the cursor somewhere valid.
///
/// The three views count their rows differently — cells, nodes, lines —
/// so a cursor carried across without clamping addresses nothing.
#[test]
fn switching_views_leaves_the_cursor_somewhere_real() {
    let mut s = Session::new(Document::parse(XML.as_bytes(), FormatHint::Xml).unwrap());
    for cmd in [
        Command::ViewText,
        Command::ViewTable,
        Command::ViewTree,
        Command::ViewText,
        Command::ViewTree,
        Command::ViewTable,
    ] {
        s.execute(Command::GoBottom);
        s.execute(cmd);
        let (_, rows, cols) = s.table_dims();
        assert!(
            s.grid.cursor.0 < rows.max(1) && s.grid.cursor.1 < cols.max(1),
            "{:?}: cursor {:?} outside {rows}x{cols}",
            s.view_mode(),
            s.grid.cursor
        );
    }
}
