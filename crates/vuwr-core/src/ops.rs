//! Edit operations. Every mutation of a document is one of these; each
//! carries enough information for [`CsvDoc::apply`] to compute its exact
//! inverse, which is what makes undo byte-exact.

use crate::Error;
use crate::csv::{Cell, CsvDoc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOp {
    /// Set one cell's text. Short rows are padded with empty cells so a grid
    /// UI can write into a ragged sheet.
    SetCell {
        row: usize,
        column: usize,
        value: String,
    },
    /// Insert a row. Widths need not match the rest of the sheet — ragged
    /// rows round-trip fine.
    InsertRow {
        at: usize,
        cells: Vec<Cell>,
    },
    DeleteRow {
        at: usize,
    },
    /// Insert a column: exactly one cell per row (header cell first). The
    /// inverse is `DeleteColumn`.
    InsertColumn {
        at: usize,
        cells: Vec<Cell>,
    },
    DeleteColumn {
        at: usize,
    },
    /// Swap a column to a new position. Requires a rectangular sheet so the
    /// inverse (`MoveColumn` with the arguments swapped) is exact.
    MoveColumn {
        from: usize,
        to: usize,
    },
    // --- Computed inverses: produced by `apply`, not issued by frontends. ---
    //
    // These exist because the plain ops above cannot express an exact undo:
    // `SetCell` cannot restore a deleted cell's quoting flag, and
    // `InsertColumn` cannot express "restore only the rows that had the
    // column" in a ragged sheet.
    /// Inverse of `SetCell`.
    RestoreCell {
        row: usize,
        column: usize,
        cell: Cell,
    },
    /// Inverse of `DeleteColumn`.
    RestoreColumn {
        at: usize,
        entries: Vec<(usize, Cell)>,
    },
}

impl CsvDoc {
    /// Apply an op, returning its exact inverse. On error the document is
    /// untouched.
    pub(crate) fn apply(&mut self, op: EditOp) -> Result<EditOp, Error> {
        match op {
            EditOp::SetCell { row, column, value } => {
                let len = self.height();
                let r = self
                    .rows_mut()
                    .get_mut(row)
                    .ok_or(Error::RowOutOfRange { row, len })?;
                while r.len() <= column {
                    r.push(Cell::unquoted(""));
                }
                let old = std::mem::replace(&mut r[column], Cell::unquoted(value));
                Ok(EditOp::RestoreCell {
                    row,
                    column,
                    cell: old,
                })
            }
            EditOp::RestoreCell { row, column, cell } => {
                let len = self.height();
                let r = self
                    .rows_mut()
                    .get_mut(row)
                    .ok_or(Error::RowOutOfRange { row, len })?;
                let width = r.len();
                let old = std::mem::replace(
                    r.get_mut(column)
                        .ok_or(Error::ColumnOutOfRange { column, len: width })?,
                    cell,
                );
                Ok(EditOp::RestoreCell {
                    row,
                    column,
                    cell: old,
                })
            }
            EditOp::InsertRow { at, cells } => {
                let len = self.height();
                if at > len {
                    return Err(Error::RowOutOfRange { row: at, len });
                }
                self.rows_mut().insert(at, cells);
                Ok(EditOp::DeleteRow { at })
            }
            EditOp::DeleteRow { at } => {
                let len = self.height();
                if at >= len {
                    return Err(Error::RowOutOfRange { row: at, len });
                }
                let cells = self.rows_mut().remove(at);
                Ok(EditOp::InsertRow { at, cells })
            }
            EditOp::InsertColumn { at, cells } => {
                let len = self.height();
                if len == 0 {
                    return Err(Error::EmptyDocument);
                }
                if cells.len() != len {
                    return Err(Error::ColumnLengthMismatch {
                        expected: len,
                        got: cells.len(),
                    });
                }
                let width = self.width();
                if at > width {
                    return Err(Error::ColumnOutOfRange {
                        column: at,
                        len: width,
                    });
                }
                for (row, cell) in self.rows_mut().iter_mut().zip(cells) {
                    row.insert(at.min(row.len()), cell);
                }
                Ok(EditOp::DeleteColumn { at })
            }
            EditOp::DeleteColumn { at } => {
                let width = self.width();
                if at >= width {
                    return Err(Error::ColumnOutOfRange {
                        column: at,
                        len: width,
                    });
                }
                let mut entries = Vec::new();
                for (i, row) in self.rows_mut().iter_mut().enumerate() {
                    if at < row.len() {
                        entries.push((i, row.remove(at)));
                    }
                }
                Ok(EditOp::RestoreColumn { at, entries })
            }
            EditOp::RestoreColumn { at, entries } => {
                let len = self.height();
                for (row_idx, _) in &entries {
                    if *row_idx >= len {
                        return Err(Error::RowOutOfRange { row: *row_idx, len });
                    }
                }
                for (row_idx, cell) in entries {
                    let row = &mut self.rows_mut()[row_idx];
                    row.insert(at.min(row.len()), cell);
                }
                // Redoing a RestoreColumn is just deleting it again.
                Ok(EditOp::DeleteColumn { at })
            }
            EditOp::MoveColumn { from, to } => {
                let width = self.width();
                if self.rows_mut().iter().any(|r| r.len() != width) {
                    return Err(Error::RaggedRows);
                }
                if from >= width {
                    return Err(Error::ColumnOutOfRange {
                        column: from,
                        len: width,
                    });
                }
                if to >= width {
                    return Err(Error::ColumnOutOfRange {
                        column: to,
                        len: width,
                    });
                }
                for row in self.rows_mut().iter_mut() {
                    let cell = row.remove(from);
                    row.insert(to, cell);
                }
                Ok(EditOp::MoveColumn { from: to, to: from })
            }
        }
    }
}
