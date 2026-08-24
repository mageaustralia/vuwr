//! Search and filter over a [`Sheet`].
//!
//! Lives in core so the TUI, the GUI and a future mobile UI share one
//! definition of what "matching" means, rather than each rolling its own.

use regex::{Regex, RegexBuilder};

use crate::Error;
use crate::sheet::Sheet;

/// A compiled search.
#[derive(Debug, Clone)]
pub struct Search {
    re: Regex,
    pattern: String,
}

impl Search {
    /// Compile `pattern`.
    ///
    /// Case handling is "smart": a pattern typed in lower case matches
    /// case-insensitively, and one containing an upper-case letter is
    /// taken literally. Typing `alice` should find `Alice`; deliberately
    /// typing `Alice` should not find `alice`.
    pub fn new(pattern: &str) -> Result<Search, Error> {
        let has_upper = pattern.chars().any(|c| c.is_uppercase());
        let re = RegexBuilder::new(pattern)
            .case_insensitive(!has_upper)
            .build()
            .map_err(|e| Error::InvalidRegex(e.to_string()))?;
        Ok(Search {
            re,
            pattern: pattern.to_string(),
        })
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    pub fn matches(&self, text: &str) -> bool {
        self.re.is_match(text)
    }

    /// The rows with a match in any column.
    pub fn filter_rows(&self, sheet: &dyn Sheet, skip_header: bool) -> Vec<usize> {
        let (rows, cols) = sheet.dims();
        let first = usize::from(skip_header && sheet.header_is_first_row());
        // The header is not data, so it is never filtered away — losing it
        // would leave a grid of unlabelled columns.
        let mut out: Vec<usize> = (0..first).collect();
        out.extend((first..rows).filter(|&r| {
            (0..cols).any(|c| sheet.cell(r, c).is_some_and(|text| self.matches(&text)))
        }));
        out
    }

    /// The columns whose *name* matches.
    pub fn filter_columns(&self, sheet: &dyn Sheet) -> Vec<usize> {
        sheet
            .headers()
            .iter()
            .enumerate()
            .filter(|(_, h)| self.matches(h))
            .map(|(i, _)| i)
            .collect()
    }

    /// The next matching cell from `from`, exclusive, wrapping around.
    ///
    /// Returns `None` only when nothing in the sheet matches, so `n` on a
    /// single match stays put rather than reporting failure.
    pub fn find_from(
        &self,
        sheet: &dyn Sheet,
        from: (usize, usize),
        forward: bool,
    ) -> Option<(usize, usize)> {
        let (rows, cols) = sheet.dims();
        if rows == 0 || cols == 0 {
            return None;
        }
        let total = rows * cols;
        let start = from.0 * cols + from.1;
        for step in 1..=total {
            let idx = if forward {
                (start + step) % total
            } else {
                (start + total - (step % total)) % total
            };
            let (r, c) = (idx / cols, idx % cols);
            if sheet.cell(r, c).is_some_and(|t| self.matches(&t)) {
                return Some((r, c));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, FormatHint};

    fn doc(src: &str) -> Document {
        Document::parse(src.as_bytes(), FormatHint::Csv).unwrap()
    }

    #[test]
    fn smart_case_matches_lowercase_loosely_and_uppercase_exactly() {
        assert!(Search::new("alice").unwrap().matches("Alice"));
        assert!(!Search::new("Alice").unwrap().matches("alice"));
    }

    #[test]
    fn invalid_pattern_reports_rather_than_panicking() {
        assert!(matches!(
            Search::new("[unclosed"),
            Err(Error::InvalidRegex(_))
        ));
    }

    #[test]
    fn filter_keeps_the_header_row() {
        let d = doc("name,age\nAlice,30\nBob,25\n");
        let rows = Search::new("bob")
            .unwrap()
            .filter_rows(d.sheet().unwrap(), true);
        assert_eq!(rows, vec![0, 2], "header plus the matching row");
    }

    #[test]
    fn filter_columns_matches_names() {
        let d = doc("name,age,city\nA,1,X\n");
        let cols = Search::new("name|city")
            .unwrap()
            .filter_columns(d.sheet().unwrap());
        assert_eq!(cols, vec![0, 2]);
    }

    #[test]
    fn find_wraps_around() {
        let d = doc("a\nx\ny\nx\n");
        let sheet = d.sheet().unwrap();
        let s = Search::new("x").unwrap();
        assert_eq!(s.find_from(sheet, (0, 0), true), Some((1, 0)));
        assert_eq!(s.find_from(sheet, (1, 0), true), Some((3, 0)));
        // Past the last match, wrap to the first.
        assert_eq!(s.find_from(sheet, (3, 0), true), Some((1, 0)));
        // Backwards.
        assert_eq!(s.find_from(sheet, (3, 0), false), Some((1, 0)));
    }

    #[test]
    fn find_returns_none_only_when_nothing_matches() {
        let d = doc("a\nx\n");
        assert_eq!(
            Search::new("zzz")
                .unwrap()
                .find_from(d.sheet().unwrap(), (0, 0), true),
            None
        );
    }
}
