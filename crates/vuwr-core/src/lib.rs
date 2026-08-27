//! Document model, loaders, edit ops and undo for vuwr.
//!
//! This crate performs **no I/O** — it takes bytes and returns bytes, which is
//! what makes it portable to `wasm32-unknown-unknown` and to future native
//! mobile UIs. Do not add dependencies that touch the filesystem, threads
//! (`rayon`), `std::time::Instant` (use `web-time`), or `memmap2`. CI checks
//! this crate against the wasm target on every push.

// No `unsafe` in the core, ever. It parses whatever a stranger's file
// happens to contain, and it is the crate other people would embed — the
// two places where a memory bug costs the most. This used to live in the
// manifest; cargo cannot merge a crate's own lints with the workspace's,
// so it is stated here where it is also visible while reading the code.
#![forbid(unsafe_code)]

mod command;
mod csv;
mod diagnostics;
mod entities;
pub mod json;
mod links;
pub mod node;
mod ops;
mod scheme;
mod search;
mod session;
mod sheet;
mod sort;
mod syntax;
mod tree;
mod view;
mod xml;

pub use command::Command;
pub use csv::{Cell, CsvDoc, LineEnding, Row};
pub use diagnostics::{Diagnostic, Place, Severity, scan_columns, scan_double_encoding, scan_json};
pub use entities::{decode, encode};
pub use json::{JsonDoc, Layout};
pub use links::{as_link, links};
pub use node::{Array, Element, Map, Node, NodePath, PathSeg, XmlDecl};
pub use ops::EditOp;
pub use scheme::{Ground, Rgb, Scheme};
pub use search::Search;
pub use session::{
    Effect, Entry, Field, FieldKind, Inspector, Mode, NewNode, PromptKind, Session, SortSpec,
    ViewMode, escape, path_label,
};
pub use sheet::Sheet;
pub use sort::{SortDirection, SortKind, natural_cmp, sort_rows};
pub use syntax::{Grammar, Span, Token, highlight};
pub use tree::{Expansion, RowKind, TreeRow, ValueKind};
pub use view::GridState;
pub use xml::XmlDoc;

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
    /// JSON.
    Json,
    /// XML.
    Xml,
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
    /// JSON/XML parse error: unexpected token.
    UnexpectedToken {
        offset: usize,
    },
    /// Input ended in the middle of a value.
    UnexpectedEof {
        offset: usize,
    },
    /// A `\` escape inside a JSON string is malformed.
    InvalidEscape {
        offset: usize,
    },
    /// This format's editing support is not implemented yet. Returned
    /// rather than silently discarding the edit.
    EditNotSupported {
        format: &'static str,
    },
    /// An XML element was never closed.
    UnclosedTag {
        tag: String,
        offset: usize,
    },
    /// An XML closing tag names a different element than the open one.
    MismatchedTag {
        opened: String,
        closed: String,
        offset: usize,
    },
    /// An edit addressed a node that does not exist.
    NoSuchPath,
    /// A search pattern would not compile.
    InvalidRegex(String),
    /// The document has no table-shaped view (not an array of objects, not
    /// repeated sibling elements).
    NotTableShaped,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUtf8 => write!(f, "input is not valid UTF-8"),
            Self::UnclosedQuote { .. } => write!(f, "quoted field is never closed"),
            Self::RowOutOfRange { row, len } => {
                write!(f, "row {row} is out of range ({len} rows)")
            }
            Self::ColumnOutOfRange { column, len } => {
                write!(f, "column {column} is out of range ({len} columns)")
            }
            Self::ColumnLengthMismatch { expected, got } => {
                write!(f, "column needs {expected} cells, got {got}")
            }
            Self::RaggedRows => write!(f, "rows have differing widths"),
            Self::EmptyDocument => write!(f, "document has no rows"),
            Self::UnexpectedToken { .. } => write!(f, "unexpected token"),
            Self::UnexpectedEof { .. } => write!(f, "unexpected end of input"),
            Self::InvalidEscape { .. } => write!(f, "invalid escape sequence"),
            Self::EditNotSupported { format } => {
                write!(f, "editing {format} documents is not supported yet")
            }
            Self::UnclosedTag { tag, .. } => write!(f, "<{tag}> is never closed"),
            Self::MismatchedTag { opened, closed, .. } => {
                write!(f, "<{opened}> is closed by </{closed}>")
            }
            Self::NoSuchPath => write!(f, "no such path in the document"),
            Self::InvalidRegex(msg) => write!(f, "bad pattern: {msg}"),
            Self::NotTableShaped => write!(f, "document has no table-shaped view"),
        }
    }
}

impl Error {
    /// The byte offset this error points at, if it has one.
    pub fn offset(&self) -> Option<usize> {
        match self {
            Self::UnclosedQuote { offset }
            | Self::UnexpectedToken { offset }
            | Self::UnexpectedEof { offset }
            | Self::InvalidEscape { offset }
            | Self::UnclosedTag { offset, .. }
            | Self::MismatchedTag { offset, .. } => Some(*offset),
            _ => None,
        }
    }

    /// This error as `line:column: message`, given the source it came
    /// from. A byte offset is useless to a person; a line and column can
    /// be typed into an editor.
    pub fn located(&self, source: &[u8]) -> String {
        match self.offset() {
            Some(offset) => {
                let (line, column) = line_col(source, offset);
                format!("{line}:{column}: {self}")
            }
            None => self.to_string(),
        }
    }
}

/// The 1-based line and column of a byte offset.
///
/// Columns count characters rather than bytes, so a line containing
/// non-ASCII text reports the column a person would count to.
pub fn line_col(source: &[u8], offset: usize) -> (usize, usize) {
    let offset = offset.min(source.len());
    let mut line = 1usize;
    let mut line_start = 0usize;
    for (i, &b) in source[..offset].iter().enumerate() {
        if b == b'\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    let column = String::from_utf8_lossy(&source[line_start..offset])
        .chars()
        .count()
        + 1;
    (line, column)
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
    /// How this document was parsed, so edited source can be re-parsed
    /// the same way rather than re-sniffed into a different format.
    hint: FormatHint,
    /// The source began with a UTF-8 BOM. Stripped before parsing so it
    /// cannot leak into the first key or cell, and re-emitted on save so
    /// the round-trip stays byte-exact.
    bom: bool,
}

enum Kind {
    Csv(CsvDoc),
    Json(JsonDoc),
    Xml(XmlDoc),
}

impl Document {
    /// Parse bytes into a document.
    ///
    /// An explicit `hint` wins over content sniffing — a `.csv` file whose
    /// first cell happens to start with `{` is still a CSV. Only
    /// [`FormatHint::Auto`] sniffs, in which case the first non-whitespace
    /// byte decides: `{`/`[` is JSON, `<` is XML, anything else is CSV.
    pub fn parse(bytes: &[u8], hint: FormatHint) -> Result<Self, Error> {
        // A BOM must not defeat format detection (it is not ASCII
        // whitespace, so it would otherwise sniff as CSV) and must not end
        // up inside the first key or cell.
        let (bom, bytes) = match bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]) {
            Some(rest) => (true, rest),
            None => (false, bytes),
        };

        let kind = match hint {
            FormatHint::Csv | FormatHint::Tsv => Kind::Csv(CsvDoc::parse(bytes, hint)?),
            FormatHint::Json => Kind::Json(JsonDoc::parse(bytes)?),
            FormatHint::Xml => Kind::Xml(XmlDoc::parse(bytes)?),
            FormatHint::Auto => match bytes.iter().find(|&&b| !b.is_ascii_whitespace()) {
                Some(b'{') | Some(b'[') => Kind::Json(JsonDoc::parse(bytes)?),
                Some(b'<') => Kind::Xml(XmlDoc::parse(bytes)?),
                _ => Kind::Csv(CsvDoc::parse(bytes, hint)?),
            },
        };

        Ok(Self {
            kind,
            undo: Vec::new(),
            redo: Vec::new(),
            hint,
            bom,
        })
    }

    /// Byte-for-byte faithful rendering of the document: delimiter, line
    /// endings, per-cell quoting and the trailing newline are all reproduced
    /// from the source unless an edit changed them.
    pub fn serialize(&self) -> Vec<u8> {
        let body = match &self.kind {
            Kind::Csv(doc) => doc.serialize(),
            Kind::Json(doc) => doc.serialize(),
            Kind::Xml(doc) => doc.serialize(),
        };
        if self.bom {
            let mut out = Vec::with_capacity(body.len() + 3);
            out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            out.extend_from_slice(&body);
            out
        } else {
            body
        }
    }

    pub fn apply(&mut self, op: EditOp) -> Result<(), Error> {
        let inverse = self.apply_inner(op)?;
        self.undo.push(inverse);
        self.redo.clear();
        Ok(())
    }

    /// Returns `false` when there is nothing to undo.
    /// Whether there is anything to undo, so a frontend can grey the
    /// control rather than offering an action that does nothing.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether there is anything to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

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
        // Several ops as one, for a bulk edit that has to undo as one.
        if let EditOp::Batch(ops) = op {
            let mut done: Vec<EditOp> = Vec::with_capacity(ops.len());
            for step in ops {
                match self.apply_inner(step) {
                    Ok(inverse) => done.push(inverse),
                    // A batch keeps the promise a single op makes: on
                    // failure the document is as it was. Undoing what has
                    // been applied is the only way to keep it here.
                    Err(e) => {
                        for inverse in done.into_iter().rev() {
                            let _ = self.apply_inner(inverse);
                        }
                        return Err(e);
                    }
                }
            }
            done.reverse();
            return Ok(EditOp::Batch(done));
        }
        // Applies to every format: the op carries a whole document.
        if let EditOp::ReplaceSource { bytes } = op {
            let previous = self.serialize();
            let replacement = Self::parse(&bytes, self.hint)?;
            self.kind = replacement.kind;
            self.bom = replacement.bom;
            return Ok(EditOp::ReplaceSource { bytes: previous });
        }
        match &mut self.kind {
            Kind::Csv(doc) => doc.apply(op),
            Kind::Json(doc) => match op {
                EditOp::RemoveNode { parent, index } => {
                    let (key, value) = doc.root_mut().remove_child(&parent, index)?;
                    Ok(EditOp::InsertNode {
                        parent,
                        index,
                        key,
                        value,
                    })
                }
                EditOp::InsertNode {
                    parent,
                    index,
                    key,
                    value,
                } => {
                    doc.root_mut().insert_child(&parent, index, key, value)?;
                    Ok(EditOp::RemoveNode { parent, index })
                }
                EditOp::RenameNode {
                    parent,
                    index,
                    name,
                } => {
                    let old = doc.root_mut().rename_child(&parent, index, name)?;
                    Ok(EditOp::RenameNode {
                        parent,
                        index,
                        name: old,
                    })
                }
                EditOp::SetNode { path, value } => {
                    let previous = doc.root_mut().set_at(&path, value)?;
                    Ok(EditOp::SetNode {
                        path,
                        value: previous,
                    })
                }
                _ => Err(Error::EditNotSupported { format: "JSON" }),
            },
            Kind::Xml(doc) => match op {
                EditOp::RemoveNode { parent, index } => {
                    let (key, value) = doc.root_mut().remove_child(&parent, index)?;
                    Ok(EditOp::InsertNode {
                        parent,
                        index,
                        key,
                        value,
                    })
                }
                EditOp::InsertNode {
                    parent,
                    index,
                    key,
                    value,
                } => {
                    doc.root_mut().insert_child(&parent, index, key, value)?;
                    Ok(EditOp::RemoveNode { parent, index })
                }
                EditOp::SetNode { path, value } => {
                    let previous = doc.root_mut().set_at(&path, value)?;
                    Ok(EditOp::SetNode {
                        path,
                        value: previous,
                    })
                }
                _ => Err(Error::EditNotSupported { format: "XML" }),
            },
        }
    }

    /// The table view of this document, if it has one.
    ///
    /// One interface for every format, so frontends do not branch per
    /// format — and so a format added later (by a script or a plugin) is
    /// indistinguishable from a built-in one.
    pub fn sheet(&self) -> Option<&dyn Sheet> {
        match &self.kind {
            Kind::Csv(doc) => Some(doc),
            Kind::Json(doc) if self.json_table_eligible() => Some(doc),
            Kind::Xml(doc) if self.xml_table_eligible() => Some(doc),
            _ => None,
        }
    }

    /// Replace the whole document by re-parsing `bytes`, recording undo.
    ///
    /// This is how text view commits an edit: the source is the thing
    /// being edited, so the document is rebuilt from it. A parse failure
    /// changes nothing and reports why.
    pub fn replace_source(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let inverse = self.apply_inner(EditOp::ReplaceSource {
            bytes: bytes.to_vec(),
        })?;
        self.undo.push(inverse);
        self.redo.clear();
        Ok(())
    }

    /// Problems that are legal but probably wrong — duplicate keys and
    /// the like. Empty for formats that have none.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut found = match &self.kind {
            Kind::Json(_) => diagnostics::scan_json(&self.serialize()),
            _ => Vec::new(),
        };
        // Whatever the format, a column that is numbers except for three
        // rows is worth knowing about.
        if let Some(sheet) = self.sheet() {
            found.extend(diagnostics::scan_columns(sheet, &self.serialize()));
        }
        // And a value escaped twice, which a viewer that decodes once
        // shows as markup rather than as the mistake it is.
        if self.is_xml() {
            found.extend(diagnostics::scan_double_encoding(&self.serialize()));
        }
        found
    }

    /// Re-lay-out the document, undoably.
    ///
    /// Only JSON has a layout to change; CSV's shape is its content, and
    /// XML reflowing would move text nodes, which changes meaning.
    pub fn reformat(&mut self, style: Layout) -> Result<(), Error> {
        let bytes = match &self.kind {
            Kind::Json(doc) => {
                let mut copy = doc.clone();
                copy.reformat(style);
                copy.serialize()
            }
            Kind::Xml(doc) => {
                let mut copy = doc.clone();
                copy.reformat(style);
                copy.serialize()
            }
            // CSV's shape is its content: there is no layout to change.
            Kind::Csv(_) => return Err(Error::EditNotSupported { format: "CSV" }),
        };
        self.replace_source(&bytes)
    }

    /// Replace the node at `path`, recording undo.
    pub fn set_node(&mut self, path: &[PathSeg], value: Node) -> Result<(), Error> {
        let inverse = self.apply_inner(EditOp::SetNode {
            path: path.to_vec(),
            value,
        })?;
        self.undo.push(inverse);
        self.redo.clear();
        Ok(())
    }

    /// Remove a node, recording undo.
    ///
    /// `index` is an ordinal among the addressable children, which for XML
    /// means elements only; it is translated to a raw position so the
    /// whitespace around the node is left untouched.
    pub fn remove_node(&mut self, parent: &[PathSeg], index: usize) -> Result<(), Error> {
        let index = self.raw_index(parent, index, false)?;
        let inverse = self.apply_inner(EditOp::RemoveNode {
            parent: parent.to_vec(),
            index,
        })?;
        self.undo.push(inverse);
        self.redo.clear();
        Ok(())
    }

    /// Insert a node, recording undo.
    pub fn insert_node(
        &mut self,
        parent: &[PathSeg],
        index: usize,
        key: Option<String>,
        value: Node,
    ) -> Result<(), Error> {
        let index = self.raw_index(parent, index, true)?;
        let inverse = self.apply_inner(EditOp::InsertNode {
            parent: parent.to_vec(),
            index,
            key,
            value,
        })?;
        self.undo.push(inverse);
        self.redo.clear();
        Ok(())
    }

    /// Translate a child ordinal into the position the edit ops use.
    fn raw_index(
        &self,
        parent: &[PathSeg],
        ordinal: usize,
        for_insert: bool,
    ) -> Result<usize, Error> {
        let root = match &self.kind {
            Kind::Xml(doc) => doc.root(),
            _ => return Ok(ordinal),
        };
        let found = if for_insert {
            root.raw_insert_index(parent, ordinal)
        } else {
            root.raw_child_index(parent, ordinal)
        };
        found.ok_or(Error::NoSuchPath)
    }

    /// Rename a map key, recording undo.
    pub fn rename_node(
        &mut self,
        parent: &[PathSeg],
        index: usize,
        name: String,
    ) -> Result<(), Error> {
        let inverse = self.apply_inner(EditOp::RenameNode {
            parent: parent.to_vec(),
            index,
            name,
        })?;
        self.undo.push(inverse);
        self.redo.clear();
        Ok(())
    }

    /// Write one cell through the table view, recording undo.
    pub fn set_cell(&mut self, row: usize, col: usize, value: &str) -> Result<(), Error> {
        let eligible_json = self.json_table_eligible();
        let eligible_xml = self.xml_table_eligible();
        let inverse = match &mut self.kind {
            Kind::Csv(doc) => doc.set_cell(row, col, value),
            Kind::Json(doc) if eligible_json => doc.set_cell(row, col, value),
            Kind::Xml(doc) if eligible_xml => doc.set_cell(row, col, value),
            _ => Err(Error::NotTableShaped),
        }?;
        self.undo.push(inverse);
        self.redo.clear();
        Ok(())
    }

    /// Set many cells as one edit.
    ///
    /// Through the same per-format path a single cell takes. A batch of
    /// raw `SetCell` ops is CSV's alone — JSON and XML address their
    /// values by node — so one built that way failed on an XML feed with
    /// "editing XML documents is not supported yet", which is exactly the
    /// document replace-all exists for.
    ///
    /// All or nothing: a failure part way puts back what it had done.
    pub fn set_cells(&mut self, edits: &[(usize, usize, String)]) -> Result<usize, Error> {
        let eligible_json = self.json_table_eligible();
        let eligible_xml = self.xml_table_eligible();
        let mut inverses = Vec::with_capacity(edits.len());
        for (row, col, value) in edits {
            let done = match &mut self.kind {
                Kind::Csv(doc) => doc.set_cell(*row, *col, value),
                Kind::Json(doc) if eligible_json => doc.set_cell(*row, *col, value),
                Kind::Xml(doc) if eligible_xml => doc.set_cell(*row, *col, value),
                _ => Err(Error::NotTableShaped),
            };
            match done {
                Ok(inverse) => inverses.push(inverse),
                Err(e) => {
                    for inverse in inverses.into_iter().rev() {
                        let _ = self.apply_inner(inverse);
                    }
                    return Err(e);
                }
            }
        }
        let count = inverses.len();
        // Reversed, so undoing walks back the way it came.
        inverses.reverse();
        self.undo.push(EditOp::Batch(inverses));
        self.redo.clear();
        Ok(count)
    }

    pub fn as_csv(&self) -> Option<&CsvDoc> {
        match &self.kind {
            Kind::Csv(doc) => Some(doc),
            _ => None,
        }
    }

    pub fn as_json(&self) -> Option<&JsonDoc> {
        match &self.kind {
            Kind::Json(doc) => Some(doc),
            _ => None,
        }
    }

    pub fn as_json_mut(&mut self) -> Option<&mut JsonDoc> {
        match &mut self.kind {
            Kind::Json(doc) => Some(doc),
            _ => None,
        }
    }

    pub fn as_xml(&self) -> Option<&XmlDoc> {
        match &self.kind {
            Kind::Xml(doc) => Some(doc),
            _ => None,
        }
    }

    pub fn as_xml_mut(&mut self) -> Option<&mut XmlDoc> {
        match &mut self.kind {
            Kind::Xml(doc) => Some(doc),
            _ => None,
        }
    }

    /// Returns true if this document is a CSV (table-shaped by default).
    pub fn is_csv(&self) -> bool {
        matches!(self.kind, Kind::Csv(_))
    }

    /// Returns true if this document is JSON (tree-shaped by default).
    pub fn is_json(&self) -> bool {
        matches!(self.kind, Kind::Json(_))
    }

    /// Returns true if this document is XML (tree-shaped by default).
    pub fn is_xml(&self) -> bool {
        matches!(self.kind, Kind::Xml(_))
    }

    /// If JSON, returns true when the root is an array of objects
    /// (eligible for table view).
    pub fn json_table_eligible(&self) -> bool {
        match &self.kind {
            Kind::Json(doc) => doc.is_table_shaped(),
            _ => false,
        }
    }

    /// If XML, returns true when the root element has repeated child
    /// elements with the same tag name (eligible for table view).
    pub fn xml_table_eligible(&self) -> bool {
        match &self.kind {
            // The shape only exists when the rows repeat, and it is
            // cached — asking it is O(1), where re-deriving it walked
            // every row on every cell lookup.
            Kind::Xml(doc) => doc.row_count() > 0,
            _ => false,
        }
    }
}
