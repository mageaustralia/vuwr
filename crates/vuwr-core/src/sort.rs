//! Row ordering.
//!
//! Sorting reorders the *view*, not the document: it writes a row order
//! into [`crate::GridState::visible`], the same indirection filtering
//! uses, so the two compose and neither touches the file. A viewer that
//! rewrote the file to show you a sorted view would be a poor bargain.

use crate::sheet::Sheet;

/// How to compare two cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKind {
    /// Byte order, case-insensitively. Always available.
    Lexical,
    /// Numeric where both sides parse as numbers, lexical otherwise.
    Numeric,
    /// Digit runs compare as numbers, so `file2` sorts before `file10`.
    Natural,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

impl SortDirection {
    pub fn flipped(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }
}

/// Compare two cell values, chunking digit runs so `file2` precedes
/// `file10` — the ordering people expect and plain string order does not
/// give.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let mut ai = a.char_indices().peekable();
    let mut bi = b.char_indices().peekable();

    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some((ax, ac)), Some((bx, bc))) => {
                if ac.is_ascii_digit() && bc.is_ascii_digit() {
                    // Take the whole digit run from each side and compare
                    // them as numbers, not character by character.
                    let a_run = digits_at(a, ax);
                    let b_run = digits_at(b, bx);
                    let a_val = a_run.trim_start_matches('0');
                    let b_val = b_run.trim_start_matches('0');
                    let ord = a_val
                        .len()
                        .cmp(&b_val.len())
                        .then_with(|| a_val.cmp(b_val))
                        // All else equal, more leading zeros sorts first,
                        // so `01` and `1` have a stable order.
                        .then_with(|| b_run.len().cmp(&a_run.len()));
                    if ord != Ordering::Equal {
                        return ord;
                    }
                    for _ in 0..a_run.chars().count() {
                        ai.next();
                    }
                    for _ in 0..b_run.chars().count() {
                        bi.next();
                    }
                } else {
                    let ord = ac
                        .to_lowercase()
                        .cmp(bc.to_lowercase())
                        .then_with(|| ac.cmp(&bc));
                    if ord != Ordering::Equal {
                        return ord;
                    }
                    ai.next();
                    bi.next();
                }
            }
        }
    }
}

fn digits_at(s: &str, from: usize) -> &str {
    let end = s[from..]
        .find(|c: char| !c.is_ascii_digit())
        .map_or(s.len(), |i| from + i);
    &s[from..end]
}

fn compare(a: &str, b: &str, kind: SortKind) -> std::cmp::Ordering {
    match kind {
        SortKind::Lexical => a
            .to_lowercase()
            .cmp(&b.to_lowercase())
            .then_with(|| a.cmp(b)),
        SortKind::Numeric => match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
            (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
            // A column of numbers with a stray label still sorts sensibly:
            // the numbers order numerically, the rest fall back to text.
            _ => compare(a, b, SortKind::Lexical),
        },
        SortKind::Natural => natural_cmp(a, b),
    }
}

/// Order `rows` by one column.
///
/// `rows` are source indices, so this composes with a filter: pass the
/// filtered set and only those are ordered. A header carried in row 0
/// stays at the top, since it is a label rather than data.
pub fn sort_rows(
    sheet: &dyn Sheet,
    rows: &[usize],
    column: usize,
    kind: SortKind,
    direction: SortDirection,
) -> Vec<usize> {
    let header_first = sheet.header_is_first_row();
    let mut pinned: Vec<usize> = Vec::new();
    let mut body: Vec<usize> = Vec::new();
    for &r in rows {
        if header_first && r == 0 {
            pinned.push(r);
        } else {
            body.push(r);
        }
    }

    body.sort_by(|&x, &y| {
        let a = sheet.cell(x, column).unwrap_or_default();
        let b = sheet.cell(y, column).unwrap_or_default();
        let ord = compare(&a, &b, kind);
        match direction {
            SortDirection::Ascending => ord,
            SortDirection::Descending => ord.reverse(),
        }
        // Ties keep their original order: sort_by is stable, so equal
        // values stay in file order rather than shuffling.
    });

    pinned.extend(body);
    pinned
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, FormatHint};

    fn doc(src: &str) -> Document {
        Document::parse(src.as_bytes(), FormatHint::Csv).unwrap()
    }

    fn ordered(src: &str, col: usize, kind: SortKind, dir: SortDirection) -> Vec<String> {
        let d = doc(src);
        let sheet = d.sheet().unwrap();
        let rows: Vec<usize> = (0..sheet.dims().0).collect();
        sort_rows(sheet, &rows, col, kind, dir)
            .into_iter()
            .map(|r| sheet.cell(r, col).unwrap_or_default())
            .collect()
    }

    #[test]
    fn natural_order_puts_file2_before_file10() {
        assert_eq!(
            ordered(
                "n\nfile10\nfile2\nfile1\n",
                0,
                SortKind::Natural,
                SortDirection::Ascending
            ),
            vec!["n", "file1", "file2", "file10"]
        );
    }

    /// Plain text order gets this wrong, which is the whole point.
    #[test]
    fn lexical_order_differs_from_natural() {
        assert_eq!(
            ordered(
                "n\nfile10\nfile2\n",
                0,
                SortKind::Lexical,
                SortDirection::Ascending
            ),
            vec!["n", "file10", "file2"]
        );
    }

    #[test]
    fn numeric_order_is_not_string_order() {
        assert_eq!(
            ordered(
                "n\n100\n9\n20\n",
                0,
                SortKind::Numeric,
                SortDirection::Ascending
            ),
            vec!["n", "9", "20", "100"]
        );
    }

    #[test]
    fn descending_reverses_the_body_but_keeps_the_header() {
        assert_eq!(
            ordered(
                "n\na\nc\nb\n",
                0,
                SortKind::Lexical,
                SortDirection::Descending
            ),
            vec!["n", "c", "b", "a"],
            "the header stays on top even descending"
        );
    }

    /// A column of numbers with a stray label must still sort sensibly.
    #[test]
    fn numeric_falls_back_to_text_for_unparseable_values() {
        assert_eq!(
            ordered(
                "n\n10\nn/a\n2\n",
                0,
                SortKind::Numeric,
                SortDirection::Ascending
            ),
            vec!["n", "2", "10", "n/a"]
        );
    }

    #[test]
    fn equal_values_keep_their_original_order() {
        let d = doc("n,v\nx,1\ny,1\nz,1\n");
        let sheet = d.sheet().unwrap();
        let rows: Vec<usize> = (0..4).collect();
        let out = sort_rows(sheet, &rows, 1, SortKind::Lexical, SortDirection::Ascending);
        assert_eq!(out, vec![0, 1, 2, 3], "a stable sort leaves ties alone");
    }

    #[test]
    fn sorting_composes_with_a_filter() {
        let d = doc("n\nc\na\nb\n");
        let sheet = d.sheet().unwrap();
        // Pretend rows 1 and 3 survived a filter.
        let out = sort_rows(
            sheet,
            &[0, 1, 3],
            0,
            SortKind::Lexical,
            SortDirection::Ascending,
        );
        assert_eq!(out, vec![0, 3, 1], "only the given rows are ordered");
    }

    #[test]
    fn natural_compare_handles_leading_zeros_and_equal_runs() {
        use std::cmp::Ordering;
        assert_eq!(natural_cmp("a01", "a1"), Ordering::Less);
        assert_eq!(natural_cmp("a2b", "a2b"), Ordering::Equal);
        assert_eq!(natural_cmp("a", "ab"), Ordering::Less);
    }
}
