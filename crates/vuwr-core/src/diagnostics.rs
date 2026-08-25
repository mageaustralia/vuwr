//! Problems worth telling someone about.
//!
//! Distinct from a parse error, which stops everything: a diagnostic is
//! about a document that parsed fine and is still probably wrong.
//!
//! These are found by scanning the source text rather than the parsed
//! tree, because a position is the useful part — "duplicate key" without
//! a line number leaves you hunting. The tree has no offsets (it is built
//! to preserve layout, not to record where things came from), so the scan
//! runs over the bytes the document would serialize to, which is exactly
//! what text view shows.

use crate::line_col;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    /// Byte offset into the source this was scanned from.
    pub offset: usize,
    pub line: usize,
    pub column: usize,
    /// Where to go when somebody asks to be shown this.
    ///
    /// Separate from `line`/`column`, which say where it *is* for the
    /// message. A problem with a value has no byte offset worth jumping
    /// to — the value is a cell, and the place to see it is the table.
    /// Carrying only an offset meant the outliers all pointed at byte
    /// zero, and "Show me" went to line 1 every time.
    pub place: Place,
}

/// Where a diagnostic can be shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// A byte offset into the document's text.
    Text(usize),
    /// A cell, by its row and column in the document.
    Cell { row: usize, column: usize },
}

impl Diagnostic {
    /// `line:column: message`, the form an editor can jump from.
    pub fn located(&self) -> String {
        format!("{}:{}: {}", self.line, self.column, self.message)
    }
}

/// Scan JSON text for problems a parser will not stop for.
///
/// Two things today:
///
/// - **Duplicate keys.** Valid JSON, and most parsers keep the last one
///   silently, so the earlier value is dead — the kind of bug that costs
///   an afternoon.
/// - **Trailing commas.** *Not* valid JSON, and `jq` and friends reject
///   the file outright. vuwr's own parser accepts one and writes it back
///   where it found it, so that a file carrying one can still be opened
///   and fixed rather than merely refused. That leniency belongs in the
///   reader, not in the verdict, so it is reported here as an error.
pub fn scan_json(source: &[u8]) -> Vec<Diagnostic> {
    let Ok(text) = std::str::from_utf8(source) else {
        return Vec::new();
    };
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    // One set of seen keys per open object. Arrays push a marker so their
    // strings are values, never keys.
    let mut stack: Vec<Option<Vec<(String, usize)>>> = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                stack.push(Some(Vec::new()));
                i += 1;
            }
            b'[' => {
                stack.push(None);
                i += 1;
            }
            b'}' | b']' => {
                // Whatever precedes the bracket, ignoring whitespace: a
                // comma there closes nothing.
                let mut back = i;
                while back > 0 && bytes[back - 1].is_ascii_whitespace() {
                    back -= 1;
                }
                if back > 0 && bytes[back - 1] == b',' {
                    let at = back - 1;
                    let (line, column) = line_col(source, at);
                    let closer = bytes[i] as char;
                    out.push(Diagnostic {
                        severity: Severity::Error,
                        message: format!(
                            "trailing comma before '{closer}' — not valid JSON; \
                             most parsers reject the whole file"
                        ),
                        offset: at,
                        line,
                        column,
                        place: Place::Text(at),
                    });
                }
                stack.pop();
                i += 1;
            }
            b'"' => {
                let start = i;
                let Some((value, end)) = scan_string(text, i) else {
                    break;
                };
                i = end;
                // A string is a key only if the next thing is a colon and
                // we are directly inside an object.
                let mut j = i;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                let is_key = bytes.get(j) == Some(&b':');
                if is_key && let Some(Some(seen)) = stack.last_mut() {
                    if let Some((_, first)) = seen.iter().find(|(k, _)| k == &value) {
                        let (line, column) = line_col(source, start);
                        let (first_line, _) = line_col(source, *first);
                        out.push(Diagnostic {
                            severity: Severity::Warning,
                            message: format!(
                                "duplicate key '{value}' — also at line {first_line}; \
                                 most parsers keep only the last one"
                            ),
                            offset: start,
                            line,
                            column,
                            place: Place::Text(start),
                        });
                    } else {
                        seen.push((value, start));
                    }
                }
            }
            _ => i += 1,
        }
    }
    out
}

/// Values that disagree with the rest of their column.
///
/// A column of two thousand numbers and one `129,00` is telling you
/// something: that row will sort wrong, and whatever wrote it had a
/// different idea of the format. This *reports* those; it does not offer
/// to fix them, because the fix is a guess about what somebody meant and
/// rewriting data on a guess is what this tool exists not to do.
///
/// Only where a column is overwhelmingly one shape — nine in ten, with at
/// least ten values to go on — so a genuinely mixed column says nothing.
pub fn scan_columns(sheet: &dyn crate::Sheet) -> Vec<Diagnostic> {
    let (rows, cols) = sheet.dims();
    let first = usize::from(sheet.header_is_first_row());
    let headers = sheet.headers();
    let mut out = Vec::new();

    for col in 0..cols {
        let mut numeric = 0usize;
        let mut odd: Vec<(usize, String)> = Vec::new();
        for row in first..rows {
            let Some(value) = sheet.cell(row, col) else {
                continue;
            };
            let text = value.trim().to_string();
            if text.is_empty() {
                continue;
            }
            if reads_as_number(&text) {
                numeric += 1;
            } else {
                odd.push((row, text));
            }
        }
        let total = numeric + odd.len();
        if total < 10 || odd.is_empty() {
            continue;
        }
        // Nine in ten, and no more than a handful disagreeing: past that
        // it is a mixed column, which is a choice rather than a mistake.
        if numeric * 10 < total * 9 || odd.len() > total / 10 {
            continue;
        }
        let name = headers
            .get(col)
            .cloned()
            .unwrap_or_else(|| format!("column {}", col + 1));
        for (row, value) in odd.into_iter().take(20) {
            out.push(Diagnostic {
                severity: Severity::Warning,
                message: format!(
                    "{name} reads as a number in {numeric} of {total} rows, but row {} is {value:?} \
                     — it will not sort as a number",
                    row + 1
                ),
                offset: 0,
                line: row + 1,
                column: col + 1,
                place: Place::Cell { row, column: col },
            });
        }
    }
    out
}

/// Whether a value reads as a number.
///
/// Digits, an optional minus, an optional decimal part, and commas only
/// where they group digits in threes. That last rule is the point: with
/// commas simply stripped, `129,00` becomes `12900` and reads as a
/// number — which is exactly the row worth flagging, and exactly the row
/// that would then be right-aligned as though it were fine.
pub(crate) fn reads_as_number(text: &str) -> bool {
    let head = text.split_whitespace().next().unwrap_or_default();
    let head = head.strip_prefix('-').unwrap_or(head);
    if head.is_empty() {
        return false;
    }
    let (int, frac) = match head.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (head, None),
    };
    if let Some(frac) = frac
        && (frac.is_empty() || !frac.chars().all(|c| c.is_ascii_digit()))
    {
        return false;
    }
    let groups: Vec<&str> = int.split(',').collect();
    if groups
        .iter()
        .any(|g| g.is_empty() || !g.chars().all(|c| c.is_ascii_digit()))
    {
        return false;
    }
    if groups.len() > 1 && (groups[0].len() > 3 || groups[1..].iter().any(|g| g.len() != 3)) {
        return false;
    }
    true
}

/// Read the string starting at `at`, returning its contents and the offset
/// just past the closing quote. Escapes are skipped, not decoded: only the
/// identity of the key matters here.
fn scan_string(text: &str, at: usize) -> Option<(String, usize)> {
    let bytes = text.as_bytes();
    let mut i = at + 1;
    let mut value = String::new();
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return Some((value, i + 1)),
            b'\\' => {
                // Keep the escape as written; two keys that differ only in
                // how they are escaped are not worth calling duplicates.
                value.push('\\');
                if let Some(&next) = bytes.get(i + 1) {
                    value.push(next as char);
                }
                i += 2;
            }
            _ => {
                let ch = text[i..].chars().next()?;
                value.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// vuwr reads a trailing comma so the file can be opened and fixed.
    /// Every other tool rejects the file, so the check has to say so.
    #[test]
    fn a_trailing_comma_is_an_error() {
        for src in [br#"{"a":1,}"#.as_slice(), b"[1,2,]", b"[\n  1,\n]"] {
            let found = scan_json(src);
            assert_eq!(found.len(), 1, "{}", String::from_utf8_lossy(src));
            assert_eq!(found[0].severity, Severity::Error);
            assert!(found[0].message.contains("trailing comma"));
        }
    }

    #[test]
    fn a_comma_between_values_is_fine() {
        for src in [
            br#"{"a":1,"b":2}"#.as_slice(),
            b"[1, 2]",
            b"[[1],[2]]",
            b"[]",
            b"{}",
        ] {
            assert!(
                scan_json(src).is_empty(),
                "{}",
                String::from_utf8_lossy(src)
            );
        }
    }

    /// A comma inside a string is text, not syntax.
    #[test]
    fn a_bracket_inside_a_string_is_not_a_closer() {
        assert!(scan_json(br#"{"a":"x,]"}"#).is_empty());
    }

    #[test]
    fn a_duplicate_key_is_reported_with_a_position() {
        let src = b"{\n  \"a\": 1,\n  \"color\": true,\n  \"color\": \"gold\"\n}";
        let found = scan_json(src);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 4, "the second occurrence is the problem");
        assert!(found[0].message.contains("duplicate key 'color'"));
        assert!(
            found[0].message.contains("line 3"),
            "and it names where the first one was: {}",
            found[0].message
        );
    }

    #[test]
    fn distinct_keys_are_not_reported() {
        assert!(scan_json(br#"{"a":1,"b":2,"c":3}"#).is_empty());
    }

    /// The same key in *different* objects is not a duplicate.
    #[test]
    fn keys_are_scoped_to_their_object() {
        assert!(scan_json(br#"{"a":{"k":1},"b":{"k":2}}"#).is_empty());
    }

    /// Strings inside arrays are values, and repeat freely.
    #[test]
    fn repeated_strings_in_arrays_are_not_keys() {
        assert!(scan_json(br#"{"xs":["k","k","k"]}"#).is_empty());
    }

    /// A value that happens to equal a key must not be mistaken for one.
    #[test]
    fn values_are_not_mistaken_for_keys() {
        assert!(scan_json(br#"{"a":"a","b":"a"}"#).is_empty());
    }

    #[test]
    fn several_duplicates_are_all_reported() {
        let found = scan_json(br#"{"a":1,"a":2,"a":3,"b":1,"b":2}"#);
        assert_eq!(found.len(), 3, "two extra a's and one extra b");
    }

    #[test]
    fn braces_inside_strings_do_not_confuse_the_scan() {
        assert!(scan_json(br#"{"a":"}{","b":"[]"}"#).is_empty());
        assert_eq!(scan_json(br#"{"a":"}{","a":1}"#).len(), 1);
    }

    #[test]
    fn escaped_quotes_do_not_end_a_string_early() {
        assert!(scan_json(br#"{"a":"say \"hi\"","b":2}"#).is_empty());
    }

    #[test]
    fn malformed_input_returns_what_it_found_rather_than_looping() {
        let _ = scan_json(br#"{"a":1,"a":"#);
        let _ = scan_json(b"{\"unterminated");
    }
}
