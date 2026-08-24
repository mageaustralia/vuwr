//! Document model, loaders, edit ops and undo for vuwr.
//!
//! This crate performs **no I/O** — it takes bytes and returns bytes, which is
//! what makes it portable to `wasm32-unknown-unknown` and to future native
//! mobile UIs. Do not add dependencies that touch the filesystem, threads
//! (`rayon`), `std::time::Instant` (use `web-time`), or `memmap2`. CI checks
//! this crate against the wasm target on every push.

mod csv;
mod ops;

pub use csv::{Cell, CsvDoc, LineEnding, Row};
pub use ops::EditOp;

use std::fmt;

/// Which file format the bytes should be read as. The caller derives this
/// from the file extension or a flag; the core never sees a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatHint {
    /// Sniff the delimiter from the content.
    Auto,
    /// Comma-separated.
    Csv,
    /// Tab-separated.
    Tsv,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Input is not valid UTF-8. Encoding detection/conversion is a later
    /// phase; for now non-UTF-8 files are rejected.
    InvalidUtf8,
    /// A quoted cell was never closed. `offset` is the byte position of the
    /// opening quote.
    UnclosedQuote {
        offset: usize,
    },
    RowOutOfRange {
        row: usize,
        len: usize,
    },
    ColumnOutOfRange {
        column: usize,
        len: usize,
    },
    /// `InsertColumn` requires exactly one cell per row.
    ColumnLengthMismatch {
        expected: usize,
        got: usize,
    },
    /// `MoveColumn` requires a rectangular sheet (every row the same width)
    /// so that undo can restore the original column order exactly.
    RaggedRows,
    /// The document has no rows, so the operation makes no sense.
    EmptyDocument,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidUtf8 => write!(f, "input is not valid UTF-8"),
            Error::UnclosedQuote { offset } => {
                write!(f, "quoted field starting at byte {offset} is never closed")
            }
            Error::RowOutOfRange { row, len } => {
                write!(f, "row {row} is out of range ({len} rows)")
            }
            Error::ColumnOutOfRange { column, len } => {
                write!(f, "column {column} is out of range ({len} columns)")
            }
            Error::ColumnLengthMismatch { expected, got } => {
                write!(f, "column needs {expected} cells, got {got}")
            }
            Error::RaggedRows => write!(f, "rows have differing widths"),
            Error::EmptyDocument => write!(f, "document has no rows"),
        }
    }
}

impl std::error::Error for Error {}

/// A parsed document plus its undo/redo history.
///
/// All mutation goes through [`Document::apply`], which computes an inverse
/// op and pushes it on the undo stack — so undo/redo is exact and frontends
/// cannot diverge in what they permit.
pub struct Document {
    kind: Kind,
    undo: Vec<EditOp>,
    redo: Vec<EditOp>,
}

enum Kind {
    Csv(CsvDoc),
}

impl Document {
    pub fn parse(bytes: &[u8], hint: FormatHint) -> Result<Document, Error> {
        Ok(Document {
            kind: Kind::Csv(CsvDoc::parse(bytes, hint)?),
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }

    /// Byte-for-byte faithful rendering of the document: delimiter, line
    /// endings, per-cell quoting and the trailing newline are all reproduced
    /// from the source unless an edit changed them.
    pub fn serialize(&self) -> Vec<u8> {
        match &self.kind {
            Kind::Csv(doc) => doc.serialize(),
        }
    }

    pub fn apply(&mut self, op: EditOp) -> Result<(), Error> {
        let inverse = self.apply_inner(op)?;
        self.undo.push(inverse);
        self.redo.clear();
        Ok(())
    }

    /// Returns `false` when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        let Some(op) = self.undo.pop() else {
            return false;
        };
        // An op that applied once cannot fail when inverted, but surface the
        // impossible case as "nothing happened" rather than panicking.
        match self.apply_inner(op) {
            Ok(inverse) => self.redo.push(inverse),
            Err(_) => return false,
        }
        true
    }

    /// Returns `false` when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        let Some(op) = self.redo.pop() else {
            return false;
        };
        match self.apply_inner(op) {
            Ok(inverse) => self.undo.push(inverse),
            Err(_) => return false,
        }
        true
    }

    fn apply_inner(&mut self, op: EditOp) -> Result<EditOp, Error> {
        match &mut self.kind {
            Kind::Csv(doc) => doc.apply(op),
        }
    }

    pub fn as_csv(&self) -> Option<&CsvDoc> {
        match &self.kind {
            Kind::Csv(doc) => Some(doc),
        }
    }
}
