//! Document model, loaders, edit ops and undo for vuwr.
//!
//! This crate performs **no I/O** — it takes bytes and returns bytes, which is
//! what makes it portable to `wasm32-unknown-unknown` and to future native
//! mobile UIs. Do not add dependencies that touch the filesystem, threads
//! (`rayon`), `std::time::Instant` (use `web-time`), or `memmap2`. CI checks
//! this crate against the wasm target on every push.

mod csv;
pub mod json;
pub mod node;
mod ops;
mod sheet;
mod view;
mod xml;

pub use csv::{Cell, CsvDoc, LineEnding, Row};
pub use json::JsonDoc;
pub use node::{Array, Element, Map, Node, NodePath, PathSeg, XmlDecl};
pub use ops::EditOp;
pub use sheet::Sheet;
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
    /// An edit addressed a node that does not exist.
    NoSuchPath,
    /// The document has no table-shaped view (not an array of objects, not
    /// repeated sibling elements).
    NotTableShaped,
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
            Error::UnexpectedToken { offset } => {
                write!(f, "unexpected token at byte {offset}")
            }
            Error::UnexpectedEof { offset } => {
                write!(f, "unexpected end of input at byte {offset}")
            }
            Error::InvalidEscape { offset } => {
                write!(f, "invalid escape sequence at byte {offset}")
            }
            Error::EditNotSupported { format } => {
                write!(f, "editing {format} documents is not supported yet")
            }
            Error::NoSuchPath => write!(f, "no such path in the document"),
            Error::NotTableShaped => write!(f, "document has no table-shaped view"),
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
    pub fn parse(bytes: &[u8], hint: FormatHint) -> Result<Document, Error> {
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

        Ok(Document {
            kind,
            undo: Vec::new(),
            redo: Vec::new(),
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
            Kind::Json(doc) => match op {
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
            Kind::Json(doc) => is_array_of_objects(doc.root()),
            _ => false,
        }
    }

    /// If XML, returns true when the root element has repeated child
    /// elements with the same tag name (eligible for table view).
    pub fn xml_table_eligible(&self) -> bool {
        match &self.kind {
            Kind::Xml(doc) => is_repeated_siblings(doc),
            _ => false,
        }
    }
}

/// Check if a node is an array whose every element is an object with
/// the same keys (table-shaped).
fn is_array_of_objects(node: &Node) -> bool {
    match node {
        Node::Array(arr) if !arr.items.is_empty() => {
            let keys = match &arr.items[0] {
                Node::Map(m) => m.entries.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>(),
                _ => return false,
            };
            arr.items.iter().all(|item| match item {
                Node::Map(m) => {
                    m.entries.len() == keys.len()
                        && m.entries.iter().zip(keys.iter()).all(|(a, b)| &a.0 == b)
                }
                _ => false,
            })
        }
        _ => false,
    }
}

/// True when the document element has two or more element children that
/// all share a tag — the shape that maps onto rows.
///
/// Whitespace `Text` and `Comment` children are ignored: a pretty-printed
/// file has text between every element, which used to make it ineligible.
fn is_repeated_siblings(doc: &XmlDoc) -> bool {
    let rows = doc.row_elements();
    !rows.is_empty() && rows.iter().all(|e| e.tag == rows[0].tag)
}
