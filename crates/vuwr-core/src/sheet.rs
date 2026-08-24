//! The table-shaped view of a document.
//!
//! Every format that can be shown as rows and columns implements [`Sheet`]
//! once, here in core. Frontends then have a single code path instead of a
//! `match` per format — the duplication that previously let the TUI carry
//! its own, drifted copy of the XML column logic.
//!
//! This is also the seam a plugin loader hangs off: a format supplied by a
//! script or a dynamically loaded module is just another `Sheet`.

use crate::Error;
use crate::csv::CsvDoc;
use crate::json::JsonDoc;
use crate::node::{Node, PathSeg};
use crate::ops::EditOp;
use crate::xml::XmlDoc;

/// A rows × columns view over a document.
pub trait Sheet {
    /// Column names. For CSV these come from row 0 of the data; for JSON
    /// and XML they are carried separately.
    fn headers(&self) -> Vec<String>;

    /// `(rows, columns)`.
    fn dims(&self) -> (usize, usize);

    /// The display text of one cell, or `None` if out of range.
    fn cell(&self, row: usize, col: usize) -> Option<String>;

    /// Write one cell, returning the op that undoes it.
    ///
    /// The caller (`Document::apply`) owns the undo stack; returning the
    /// inverse rather than pushing it keeps this trait free of history.
    fn set_cell(&mut self, row: usize, col: usize, value: &str) -> Result<EditOp, Error>;

    /// True when the header is row 0 of the data rather than separate
    /// metadata — the renderer needs to know whether to draw a header row.
    fn header_is_first_row(&self) -> bool {
        false
    }
}

impl Sheet for CsvDoc {
    fn headers(&self) -> Vec<String> {
        (0..self.width())
            .map(|c| self.cell(0, c).map(|c| c.value.clone()).unwrap_or_default())
            .collect()
    }

    fn dims(&self) -> (usize, usize) {
        (self.height(), self.width())
    }

    fn cell(&self, row: usize, col: usize) -> Option<String> {
        CsvDoc::cell(self, row, col).map(|c| c.value.clone())
    }

    fn set_cell(&mut self, row: usize, col: usize, value: &str) -> Result<EditOp, Error> {
        self.apply(EditOp::SetCell {
            row,
            column: col,
            value: value.to_string(),
        })
    }

    fn header_is_first_row(&self) -> bool {
        true
    }
}

impl Sheet for JsonDoc {
    fn headers(&self) -> Vec<String> {
        match self.root() {
            Node::Array(a) => match a.items.first() {
                Some(Node::Map(m)) => m.entries.iter().map(|(k, _)| k.clone()).collect(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    fn dims(&self) -> (usize, usize) {
        let rows = match self.root() {
            Node::Array(a) => a.items.len(),
            _ => 0,
        };
        (rows, self.headers().len())
    }

    fn cell(&self, row: usize, col: usize) -> Option<String> {
        let Node::Array(a) = self.root() else {
            return None;
        };
        let Node::Map(m) = a.items.get(row)? else {
            return None;
        };
        m.entries.get(col).map(|(_, v)| v.scalar_text())
    }

    fn set_cell(&mut self, row: usize, col: usize, value: &str) -> Result<EditOp, Error> {
        let key = self.headers().get(col).cloned().ok_or(Error::NoSuchPath)?;
        let path = vec![PathSeg::Index(row), PathSeg::Key(key)];
        // Read the existing value first: its type decides the replacement's.
        let new = typed_replacement(self.root().get_at(&path), value);
        let previous = self.root_mut().set_at(&path, new)?;
        Ok(EditOp::SetNode {
            path,
            value: previous,
        })
    }
}

impl Sheet for XmlDoc {
    fn headers(&self) -> Vec<String> {
        XmlDoc::table_headers(self)
    }

    fn dims(&self) -> (usize, usize) {
        (self.row_elements().len(), self.table_headers().len())
    }

    fn cell(&self, row: usize, col: usize) -> Option<String> {
        self.table_cell(row, col)
    }

    fn set_cell(&mut self, row: usize, col: usize, value: &str) -> Result<EditOp, Error> {
        let path = self.cell_path(row, col).ok_or(Error::NoSuchPath)?;
        let previous = self
            .root_mut()
            .set_at(&path, Node::Str(value.to_string()))?;
        Ok(EditOp::SetNode {
            path,
            value: previous,
        })
    }
}

/// Choose the replacement node for a JSON cell.
///
/// JSON has real types, so an edit should not silently turn `30` into
/// `"30"`. The old value's type is kept when the new text is valid for it;
/// otherwise the value becomes a string, which is visible in the display
/// rather than silent. Changing type deliberately is a separate command.
pub(crate) fn typed_replacement(old: Option<&Node>, value: &str) -> Node {
    match old {
        Some(Node::Number(_)) if value.parse::<f64>().is_ok() => Node::Number(value.to_string()),
        Some(Node::Bool(_)) if value == "true" || value == "false" => Node::Bool(value == "true"),
        Some(Node::Null) if value == "null" => Node::Null,
        _ => Node::Str(value.to_string()),
    }
}
