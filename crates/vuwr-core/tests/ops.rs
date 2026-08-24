//! Edit ops and undo/redo over the public `Document` API.

use vuwr_core::{Cell, Document, EditOp, Error, FormatHint};

fn doc(input: &str) -> Document {
    Document::parse(input.as_bytes(), FormatHint::Csv).unwrap()
}

/// Apply an op, then undo: the original bytes must be restored exactly.
fn apply_then_undo(input: &str, op: EditOp) {
    let mut d = doc(input);
    d.apply(op).unwrap();
    assert!(d.undo());
    assert_eq!(
        d.serialize(),
        input.as_bytes(),
        "undo did not restore bytes"
    );
}

#[test]
fn set_cell_edits_and_undoes() {
    let mut d = doc("a,b\n1,2\n");
    d.apply(EditOp::SetCell {
        row: 1,
        column: 0,
        value: "9".into(),
    })
    .unwrap();
    assert_eq!(d.serialize(), b"a,b\n9,2\n");
    assert!(d.undo());
    assert_eq!(d.serialize(), b"a,b\n1,2\n");
    assert!(d.redo());
    assert_eq!(d.serialize(), b"a,b\n9,2\n");
}

#[test]
fn set_cell_undo_restores_quoting_flag() {
    // The original cell was quoted; undoing a SetCell must bring the quotes
    // back even though the new value did not need them.
    apply_then_undo(
        "\"007\",x\n",
        EditOp::SetCell {
            row: 0,
            column: 0,
            value: "8".into(),
        },
    );
}

#[test]
fn set_cell_pads_ragged_row() {
    let mut d = doc("a,b\n1\n");
    d.apply(EditOp::SetCell {
        row: 1,
        column: 2,
        value: "x".into(),
    })
    .unwrap();
    assert_eq!(d.serialize(), b"a,b\n1,,x\n");
}

#[test]
fn insert_and_delete_row() {
    apply_then_undo(
        "a\n1\n2\n",
        EditOp::InsertRow {
            at: 1,
            cells: vec![Cell::unquoted("x")],
        },
    );
    apply_then_undo("a\n1\n2\n", EditOp::DeleteRow { at: 1 });

    let mut d = doc("a\n1\n2\n");
    d.apply(EditOp::DeleteRow { at: 1 }).unwrap();
    assert_eq!(d.serialize(), b"a\n2\n");
}

#[test]
fn insert_and_delete_column() {
    let mut d = doc("a,b\n1,2\n");
    d.apply(EditOp::InsertColumn {
        at: 1,
        cells: vec![Cell::unquoted("new"), Cell::unquoted("")],
    })
    .unwrap();
    assert_eq!(d.serialize(), b"a,new,b\n1,,2\n");
    assert!(d.undo());
    assert_eq!(d.serialize(), b"a,b\n1,2\n");

    d.apply(EditOp::DeleteColumn { at: 0 }).unwrap();
    assert_eq!(d.serialize(), b"b\n2\n");
    assert!(d.undo());
    assert_eq!(d.serialize(), b"a,b\n1,2\n");
}

#[test]
fn delete_column_undo_is_ragged_exact() {
    // Only rows that actually had the column get it back.
    apply_then_undo("a,b\n1\n", EditOp::DeleteColumn { at: 1 });
}

#[test]
fn move_column_swaps_and_undoes() {
    let mut d = doc("a,b,c\n1,2,3\n");
    d.apply(EditOp::MoveColumn { from: 0, to: 2 }).unwrap();
    assert_eq!(d.serialize(), b"b,c,a\n2,3,1\n");
    assert!(d.undo());
    assert_eq!(d.serialize(), b"a,b,c\n1,2,3\n");
}

#[test]
fn move_column_rejects_ragged_sheet() {
    let mut d = doc("a,b\n1\n");
    let err = d.apply(EditOp::MoveColumn { from: 0, to: 1 }).unwrap_err();
    assert_eq!(err, Error::RaggedRows);
}

#[test]
fn out_of_range_errors_leave_document_untouched() {
    let mut d = doc("a,b\n1,2\n");
    assert!(matches!(
        d.apply(EditOp::DeleteRow { at: 5 }),
        Err(Error::RowOutOfRange { .. })
    ));
    assert!(matches!(
        d.apply(EditOp::DeleteColumn { at: 5 }),
        Err(Error::ColumnOutOfRange { .. })
    ));
    assert!(matches!(
        d.apply(EditOp::InsertColumn {
            at: 0,
            cells: vec![Cell::unquoted("only one")],
        }),
        Err(Error::ColumnLengthMismatch { .. })
    ));
    assert_eq!(d.serialize(), b"a,b\n1,2\n");
    // A failed op must not land on the undo stack.
    assert!(!d.undo());
}

#[test]
fn new_edit_clears_redo_stack() {
    let mut d = doc("a\n1\n");
    d.apply(EditOp::DeleteRow { at: 1 }).unwrap();
    assert!(d.undo());
    d.apply(EditOp::DeleteRow { at: 0 }).unwrap();
    assert!(!d.redo());
}
