//! CSV parsing and serialization with byte-for-byte round-trip fidelity.
//!
//! The lexer is hand-rolled rather than built on the `csv` crate because
//! `csv` deliberately discards the information this module exists to
//! preserve: which cells were quoted, which delimiter and line ending the
//! file used, and whether it ended with a newline. The grammar is small
//! (RFC 4180 plus `""` escapes) and the round-trip corpus in
//! `tests/corpus/` pins the behaviour.

use crate::{Error, FormatHint};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

impl LineEnding {
    fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::Lf => b"\n",
            Self::CrLf => b"\r\n",
        }
    }
}

/// One cell: its text, and whether the source quoted it. On save a cell is
/// quoted if `quoted` is set **or** its value requires it (contains the
/// delimiter, a quote, or a line break), so edits can never produce invalid
/// CSV, and untouched cells keep their original quoting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub value: String,
    pub quoted: bool,
}

impl Cell {
    pub fn unquoted(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            quoted: false,
        }
    }
}

pub type Row = Vec<Cell>;

/// A parsed CSV sheet. Values are always strings — there is deliberately no
/// type inference (`007` stays `007`). Row 0 is the header row by convention;
/// the model itself does not enforce this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvDoc {
    delimiter: u8,
    line_ending: LineEnding,
    trailing_newline: bool,
    rows: Vec<Row>,
}

impl CsvDoc {
    pub fn parse(bytes: &[u8], hint: FormatHint) -> Result<Self, Error> {
        let text = std::str::from_utf8(bytes).map_err(|_| Error::InvalidUtf8)?;
        let delimiter = match hint {
            FormatHint::Csv => b',',
            FormatHint::Tsv => b'\t',
            // Json/Xml never reach here via `Document::parse`; if a caller
            // uses `CsvDoc::parse` directly with one, sniff like Auto.
            FormatHint::Auto | FormatHint::Json | FormatHint::Xml => sniff_delimiter(text),
        };

        let bytes = text.as_bytes();
        let mut rows: Vec<Row> = Vec::new();
        let mut row: Row = Vec::new();
        // Bytes, not chars: the syntax characters (delimiter, quote, CR, LF)
        // are all ASCII, so they can never appear inside a multi-byte UTF-8
        // sequence. The whole input was already validated as UTF-8.
        let mut cell: Vec<u8> = Vec::new();
        let mut cell_quoted = false;
        let mut in_quotes = false;
        let mut quote_offset = 0;
        let mut line_ending = None;
        let mut i = 0;

        macro_rules! end_cell {
            () => {{
                let value = std::mem::take(&mut cell);
                // Whole input is valid UTF-8, and we only split at ASCII
                // boundaries, so each cell is valid too.
                row.push(Cell {
                    value: String::from_utf8(value).map_err(|_| Error::InvalidUtf8)?,
                    quoted: cell_quoted,
                });
            }};
        }

        while i < bytes.len() {
            let b = bytes[i];
            if in_quotes {
                if b == b'"' {
                    if bytes.get(i + 1) == Some(&b'"') {
                        cell.push(b'"');
                        i += 2;
                    } else {
                        in_quotes = false;
                        i += 1;
                    }
                } else {
                    cell.push(b);
                    i += 1;
                }
                continue;
            }
            match b {
                b'"' if cell.is_empty() && !cell_quoted => {
                    in_quotes = true;
                    cell_quoted = true;
                    quote_offset = i;
                    i += 1;
                }
                b if b == delimiter => {
                    end_cell!();
                    cell_quoted = false;
                    i += 1;
                }
                b'\n' => {
                    if line_ending.is_none() {
                        line_ending = Some(LineEnding::Lf);
                    }
                    end_cell!();
                    cell_quoted = false;
                    rows.push(std::mem::take(&mut row));
                    i += 1;
                }
                b'\r' if bytes.get(i + 1) == Some(&b'\n') => {
                    if line_ending.is_none() {
                        line_ending = Some(LineEnding::CrLf);
                    }
                    end_cell!();
                    cell_quoted = false;
                    rows.push(std::mem::take(&mut row));
                    i += 2;
                }
                _ => {
                    cell.push(b);
                    i += 1;
                }
            }
        }

        if in_quotes {
            return Err(Error::UnclosedQuote {
                offset: quote_offset,
            });
        }
        // Flush a final record that was not terminated by a newline.
        if !row.is_empty() || !cell.is_empty() || cell_quoted {
            end_cell!();
            rows.push(row);
        }

        Ok(Self {
            delimiter,
            line_ending: line_ending.unwrap_or(LineEnding::Lf),
            trailing_newline: bytes.last() == Some(&b'\n'),
            rows,
        })
    }

    /// Construct a document directly. Primarily for tests and future loaders.
    pub fn from_parts(
        delimiter: u8,
        line_ending: LineEnding,
        trailing_newline: bool,
        rows: Vec<Row>,
    ) -> Self {
        Self {
            delimiter,
            line_ending,
            trailing_newline,
            rows,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for (i, row) in self.rows.iter().enumerate() {
            for (j, cell) in row.iter().enumerate() {
                if j > 0 {
                    out.push(self.delimiter);
                }
                write_cell(&mut out, cell, self.delimiter);
            }
            if i + 1 < self.rows.len() || self.trailing_newline {
                out.extend_from_slice(self.line_ending.as_bytes());
            }
        }
        out
    }

    pub fn delimiter(&self) -> u8 {
        self.delimiter
    }

    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn cell(&self, row: usize, column: usize) -> Option<&Cell> {
        self.rows.get(row)?.get(column)
    }

    pub fn height(&self) -> usize {
        self.rows.len()
    }

    /// The width of the widest row. Rows may be ragged.
    pub fn width(&self) -> usize {
        self.rows.iter().map(Vec::len).max().unwrap_or(0)
    }

    pub(crate) fn rows_mut(&mut self) -> &mut Vec<Row> {
        &mut self.rows
    }
}

fn write_cell(out: &mut Vec<u8>, cell: &Cell, delimiter: u8) {
    let must_quote = cell.quoted
        || cell
            .value
            .bytes()
            .any(|b| b == delimiter || b == b'"' || b == b'\r' || b == b'\n');
    if !must_quote {
        out.extend_from_slice(cell.value.as_bytes());
        return;
    }
    out.push(b'"');
    for &b in cell.value.as_bytes() {
        if b == b'"' {
            out.push(b'"');
        }
        out.push(b);
    }
    out.push(b'"');
}

/// Count candidate delimiters outside quotes in the first chunk of the file
/// and pick the most frequent. Ties and "none found" both resolve to comma.
fn sniff_delimiter(text: &str) -> u8 {
    const CANDIDATES: [u8; 3] = *b",;\t";
    let mut counts = [0usize; 3];
    let mut in_quotes = false;
    for &b in text.as_bytes().iter().take(8192) {
        match b {
            b'"' => in_quotes = !in_quotes,
            b if !in_quotes => {
                if let Some(idx) = CANDIDATES.iter().position(|&c| c == b) {
                    counts[idx] += 1;
                }
            }
            _ => {}
        }
    }
    let mut best = 0;
    for idx in 1..CANDIDATES.len() {
        // Strict `>` keeps the earliest candidate on a tie: comma wins.
        if counts[idx] > counts[best] {
            best = idx;
        }
    }
    CANDIDATES[best]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(input: &str) {
        let doc = CsvDoc::parse(input.as_bytes(), FormatHint::Auto).unwrap();
        assert_eq!(doc.serialize(), input.as_bytes(), "round-trip of {input:?}");
    }

    #[test]
    fn roundtrips() {
        roundtrip("");
        roundtrip("a");
        roundtrip("a,b,c\n1,2,3\n");
        roundtrip("a,b,c\n1,2,3"); // no trailing newline
        roundtrip("name,note\n\"say \"\"hi\"\"\",plain\n");
        roundtrip("a,b\n\"line1\nline2\",x\n");
        roundtrip("a;b;c\n1;2;3\n");
        roundtrip("a\tb\n1\t2\n");
        roundtrip("a,b\r\n1,2\r\n");
        roundtrip("\n"); // one empty row
        roundtrip("a,b\n1\n"); // ragged
        roundtrip("a\n\n"); // trailing blank row is a row, not absorbed
        roundtrip("a,b\n\n1,2\n"); // blank line in the middle
        roundtrip("\"007\",x\n"); // quoting preserved even when unneeded
        roundtrip("héllo,wörld\n");
        roundtrip(",,\n,,\n"); // empty cells
    }

    #[test]
    fn unclosed_quote_is_an_error() {
        let err = CsvDoc::parse(b"a,b\n\"oops", FormatHint::Csv).unwrap_err();
        assert_eq!(err, Error::UnclosedQuote { offset: 4 });
    }

    #[test]
    fn lone_cr_is_data_not_a_line_ending() {
        let doc = CsvDoc::parse(b"a\rb\n", FormatHint::Csv).unwrap();
        assert_eq!(doc.height(), 1);
        assert_eq!(doc.cell(0, 0).unwrap().value, "a\rb");
    }

    #[test]
    fn tsv_hint_forces_tab() {
        let doc = CsvDoc::parse(b"a,b\tc\n", FormatHint::Tsv).unwrap();
        assert_eq!(doc.width(), 2);
        assert_eq!(doc.cell(0, 0).unwrap().value, "a,b");
    }

    #[test]
    fn edit_that_needs_quoting_is_quoted_on_save() {
        let mut doc = CsvDoc::parse(b"a,b\n", FormatHint::Csv).unwrap();
        doc.rows_mut()[0][0] = Cell::unquoted("x,y");
        assert_eq!(doc.serialize(), b"\"x,y\",b\n");
    }

    #[test]
    fn invalid_utf8_is_rejected() {
        let err = CsvDoc::parse(b"a,\xff\n", FormatHint::Csv).unwrap_err();
        assert_eq!(err, Error::InvalidUtf8);
    }
}
