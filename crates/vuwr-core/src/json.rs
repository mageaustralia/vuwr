//! JSON parsing and serialization with format-preserving round-trips.
//!
//! The parser builds a `Node` tree carrying source-fidelity metadata:
//! key order, indentation style, trailing commas, and native types.
//! Serialization reproduces the original formatting byte-for-byte unless
//! an edit changed it.

use crate::Error;
use crate::node::{Array, Map, Node};

/// Indentation style sniffed from the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentStyle {
    /// Compact: no whitespace between tokens.
    Compact,
    /// Spaces: 2 or 4 spaces per level.
    Spaces(u8),
    /// Tabs: one tab per level.
    Tabs,
}

/// How [`JsonDoc::reformat`] should lay a document out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Every collection on one line, no spaces.
    Compact,
    /// Every collection broken across lines.
    Pretty,
    /// Collections containing only scalars stay on one line; anything with
    /// a collection inside it breaks.
    Smart,
}

fn relayout(node: &mut Node, style: Layout) {
    match node {
        Node::Array(a) => {
            for item in a.items.iter_mut() {
                relayout(item, style);
            }
            a.inline = inline_for(style, a.items.iter().all(is_scalar));
            a.spaced = a.inline && style != Layout::Compact;
            a.trailing_comma = false;
        }
        Node::Map(m) => {
            for (_, v) in m.entries.iter_mut() {
                relayout(v, style);
            }
            let flat = m.entries.iter().all(|(_, v)| is_scalar(v));
            m.inline = inline_for(style, flat);
            m.spaced = m.inline && style != Layout::Compact;
            m.trailing_comma = false;
        }
        _ => {}
    }
}

fn inline_for(style: Layout, all_scalars: bool) -> bool {
    match style {
        Layout::Compact => true,
        Layout::Pretty => false,
        Layout::Smart => all_scalars,
    }
}

fn is_scalar(node: &Node) -> bool {
    !matches!(node, Node::Array(_) | Node::Map(_))
}

/// A parsed JSON document.
#[derive(Debug, Clone)]
pub struct JsonDoc {
    root: Node,
    indent: IndentStyle,
}

impl JsonDoc {
    pub fn parse(bytes: &[u8]) -> Result<JsonDoc, Error> {
        let text = std::str::from_utf8(bytes).map_err(|_| Error::InvalidUtf8)?;
        let indent = sniff_indent(text);
        let (node, _rest) = parse_value(text, text.trim_start())?;
        Ok(JsonDoc { root: node, indent })
    }

    pub fn root(&self) -> &Node {
        &self.root
    }

    pub fn root_mut(&mut self) -> &mut Node {
        &mut self.root
    }

    pub fn indent(&self) -> IndentStyle {
        self.indent
    }

    /// Re-lay-out the whole document.
    ///
    /// The parser preserves whatever layout the file had, which is the
    /// point of this tool; this is the deliberate opposite, invoked by a
    /// person who has asked for it. `Smart` keeps leaf collections on one
    /// line and breaks the rest, which is what people usually mean by
    /// "readable but not sprawling".
    pub fn reformat(&mut self, style: Layout) {
        self.indent = match style {
            Layout::Compact => IndentStyle::Compact,
            Layout::Pretty | Layout::Smart => IndentStyle::Spaces(2),
        };
        relayout(&mut self.root, style);
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::new();
        serialize_node(&self.root, &mut out, 0, self.indent);
        out
    }
}

fn serialize_node(node: &Node, out: &mut Vec<u8>, depth: usize, indent: IndentStyle) {
    let pretty = indent != IndentStyle::Compact;
    match node {
        Node::Null => out.extend_from_slice(b"null"),
        Node::Bool(b) => {
            if *b {
                out.extend_from_slice(b"true");
            } else {
                out.extend_from_slice(b"false");
            }
        }
        Node::Number(s) => out.extend_from_slice(s.as_bytes()),
        Node::Str(s) => serialize_string(s, out),
        Node::Array(arr) => {
            out.push(arr.open as u8);
            if arr.items.is_empty() {
                out.push(arr.close as u8);
                return;
            }
            // If the original was inline (no newlines), keep it inline
            // even in pretty mode — this preserves compact inner arrays.
            let expand = pretty && !arr.inline;
            for (i, item) in arr.items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                if expand {
                    out.push(b'\n');
                    write_indent(out, depth + 1, indent);
                } else if arr.spaced && i > 0 {
                    out.push(b' ');
                }
                serialize_node(item, out, depth + 1, indent);
            }
            if arr.trailing_comma {
                out.push(b',');
            }
            if expand {
                out.push(b'\n');
                write_indent(out, depth, indent);
            }
            out.push(arr.close as u8);
        }
        Node::Map(map) => {
            out.push(map.open as u8);
            if map.entries.is_empty() {
                out.push(map.close as u8);
                return;
            }
            let expand = pretty && !map.inline;
            for (i, (key, val)) in map.entries.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                if expand {
                    out.push(b'\n');
                    write_indent(out, depth + 1, indent);
                } else if map.spaced && i > 0 {
                    out.push(b' ');
                }
                serialize_string(key, out);
                if expand {
                    out.extend_from_slice(b": ");
                } else {
                    out.push(b':');
                }
                serialize_node(val, out, depth + 1, indent);
            }
            if map.trailing_comma {
                out.push(b',');
            }
            if expand {
                out.push(b'\n');
                write_indent(out, depth, indent);
            }
            out.push(map.close as u8);
        }
        _ => {} // XML-only nodes ignored in JSON context
    }
}

fn serialize_string(s: &str, out: &mut Vec<u8>) {
    out.push(b'"');
    // Iterate chars, not bytes: escaping per-byte would split a multi-byte
    // character into individual `\u00xx` escapes and corrupt the text.
    for ch in s.chars() {
        match ch {
            '"' => out.extend_from_slice(b"\\\""),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\n' => out.extend_from_slice(b"\\n"),
            '\r' => out.extend_from_slice(b"\\r"),
            '\t' => out.extend_from_slice(b"\\t"),
            // Other C0 control characters have no short escape.
            c if (c as u32) < 0x20 => {
                out.extend_from_slice(format!("\\u{:04x}", c as u32).as_bytes())
            }
            // Everything else, including non-ASCII, is emitted literally as
            // UTF-8. A source that spelled it `\uXXXX` round-trips to the
            // literal character: semantically identical, not byte-identical.
            c => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    out.push(b'"');
}

fn write_indent(out: &mut Vec<u8>, depth: usize, indent: IndentStyle) {
    match indent {
        IndentStyle::Compact => {}
        IndentStyle::Spaces(n) => {
            for _ in 0..depth {
                for _ in 0..n {
                    out.push(b' ');
                }
            }
        }
        IndentStyle::Tabs => {
            for _ in 0..depth {
                out.push(b'\t');
            }
        }
    }
}

fn sniff_indent(text: &str) -> IndentStyle {
    // Look at the first indented line to determine the style.
    let lines: Vec<&str> = text.lines().collect();
    // If everything is on one line, it's compact
    if lines.len() <= 1 {
        return IndentStyle::Compact;
    }
    for line in &lines {
        let trimmed = line.trim_start();
        // Do not skip bracket-only lines: the first indented line is at
        // depth 1 whatever it contains, and skipping to a deeper one
        // reports the unit as a multiple of itself.
        if trimmed.is_empty() {
            continue;
        }
        let indent_len = line.len() - trimmed.len();
        if indent_len == 0 {
            continue;
        }
        let indent_bytes = &line.as_bytes()[..indent_len];
        if indent_bytes == &b"\t"[..] {
            return IndentStyle::Tabs;
        }
        let spaces = indent_bytes.iter().take_while(|&&b| b == b' ').count();
        if (2..=8).contains(&spaces) {
            return IndentStyle::Spaces(spaces as u8);
        }
    }
    IndentStyle::Spaces(2) // default
}

// --- Parser ---
//
// Every parser function receives the full document text plus the remaining
// suffix, so an error can name an absolute byte offset. Phase 5 (live lint
// and `--check`) turns those offsets into line/column positions.

/// Absolute byte offset of `rest` within `full`. Sound because every parser
/// function only ever receives a suffix of `full`.
fn offset_of(full: &str, rest: &str) -> usize {
    full.len() - rest.len()
}

fn parse_value<'a>(full: &str, input: &'a str) -> Result<(Node, &'a str), Error> {
    let input = input.trim_start();
    if input.is_empty() {
        return Err(Error::UnexpectedEof { offset: full.len() });
    }
    match input.as_bytes()[0] {
        b'"' => {
            let (s, rest) = parse_string(full, input)?;
            Ok((Node::Str(s), rest))
        }
        b'{' => parse_map(full, input),
        b'[' => parse_array(full, input),
        b't' if input.starts_with("true") => Ok((Node::Bool(true), &input[4..])),
        b'f' if input.starts_with("false") => Ok((Node::Bool(false), &input[5..])),
        b'n' if input.starts_with("null") => Ok((Node::Null, &input[4..])),
        b'-' | b'0'..=b'9' => parse_number(full, input),
        _ => Err(Error::UnexpectedToken {
            offset: offset_of(full, input),
        }),
    }
}

/// Read exactly four hex digits at `at` (a byte index into `input`).
fn parse_hex4(input: &str, at: usize, base: usize) -> Result<u32, Error> {
    let hex = input
        .get(at..at + 4)
        .ok_or(Error::UnexpectedEof { offset: base + at })?;
    u32::from_str_radix(hex, 16).map_err(|_| Error::InvalidEscape { offset: base + at })
}

fn parse_string<'a>(full: &str, input: &'a str) -> Result<(String, &'a str), Error> {
    let base = offset_of(full, input);
    let bytes = input.as_bytes();
    match bytes.first() {
        Some(b'"') => {}
        Some(_) => return Err(Error::UnexpectedToken { offset: base }),
        None => return Err(Error::UnexpectedEof { offset: base }),
    }
    let mut i = 1;
    let mut result = String::new();
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return Ok((result, &input[i + 1..])),
            b'\\' => {
                i += 1;
                let Some(&esc) = bytes.get(i) else {
                    return Err(Error::UnexpectedEof { offset: base + i });
                };
                match esc {
                    b'"' => {
                        result.push('"');
                        i += 1;
                    }
                    b'\\' => {
                        result.push('\\');
                        i += 1;
                    }
                    b'/' => {
                        result.push('/');
                        i += 1;
                    }
                    b'n' => {
                        result.push('\n');
                        i += 1;
                    }
                    b'r' => {
                        result.push('\r');
                        i += 1;
                    }
                    b't' => {
                        result.push('\t');
                        i += 1;
                    }
                    b'b' => {
                        result.push('\u{0008}');
                        i += 1;
                    }
                    b'f' => {
                        result.push('\u{000C}');
                        i += 1;
                    }
                    b'u' => {
                        let hi = parse_hex4(input, i + 1, base)?;
                        i += 5;
                        let ch = if (0xD800..0xDC00).contains(&hi) {
                            // High surrogate: a matching low surrogate must
                            // follow, or the pair cannot form a character.
                            if bytes.get(i) != Some(&b'\\') || bytes.get(i + 1) != Some(&b'u') {
                                return Err(Error::InvalidEscape { offset: base + i });
                            }
                            let lo = parse_hex4(input, i + 2, base)?;
                            if !(0xDC00..0xE000).contains(&lo) {
                                return Err(Error::InvalidEscape { offset: base + i });
                            }
                            i += 6;
                            let cp = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                            char::from_u32(cp).ok_or(Error::InvalidEscape { offset: base + i })?
                        } else {
                            char::from_u32(hi).ok_or(Error::InvalidEscape { offset: base + i })?
                        };
                        result.push(ch);
                    }
                    _ => return Err(Error::InvalidEscape { offset: base + i }),
                }
            }
            _ => {
                // Any other character is literal — including multi-byte
                // UTF-8, which must be pushed whole rather than per byte.
                let ch = input[i..]
                    .chars()
                    .next()
                    .expect("i is on a char boundary inside the string");
                result.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    Err(Error::UnclosedQuote { offset: base })
}

fn parse_number<'a>(full: &str, input: &'a str) -> Result<(Node, &'a str), Error> {
    let base = offset_of(full, input);
    let bytes = input.as_bytes();
    let mut i = 0;
    if bytes[i] == b'-' {
        i += 1;
    }
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i == 0 || (i == 1 && bytes[0] == b'-') {
        return Err(Error::UnexpectedToken { offset: base });
    }
    Ok((Node::Number(input[..i].to_string()), &input[i..]))
}

fn parse_map<'a>(full: &str, input: &'a str) -> Result<(Node, &'a str), Error> {
    let base = offset_of(full, input);
    if input.as_bytes()[0] != b'{' {
        return Err(Error::UnexpectedToken { offset: base });
    }
    let mut rest = &input[1..];
    let mut entries = Vec::new();

    rest = rest.trim_start();
    if let Some(rest) = rest.strip_prefix('}') {
        return Ok((
            Node::Map(Map {
                open: '{',
                close: '}',
                entries,
                trailing_comma: false,
                inline: true,
                spaced: false,
            }),
            rest,
        ));
    }

    // The raw span from just after the opening bracket, keeping the
    // whitespace on both sides: `{\n  "a": 1\n}` is not inline, but
    // trimming first hides both newlines and collapses it to one line.
    let content_start = &input[1..];
    #[allow(unused_assignments)]
    let mut trailing_comma = false;
    loop {
        rest = rest.trim_start();
        // Without this, a truncated object walks off the end: `parse_string`
        // would be handed an empty slice on every iteration.
        if rest.is_empty() {
            return Err(Error::UnexpectedEof { offset: full.len() });
        }
        let (key, r) = parse_string(full, rest)?;
        rest = r.trim_start();
        if !rest.starts_with(':') {
            return Err(Error::UnexpectedToken {
                offset: offset_of(full, rest),
            });
        }
        rest = rest[1..].trim_start();
        let (val, r) = parse_value(full, rest)?;
        entries.push((key, val));
        rest = r.trim_start();
        let has_comma = rest.starts_with(',');
        if has_comma {
            rest = &rest[1..];
        }
        trailing_comma = has_comma;
        rest = rest.trim_start();
        if rest.starts_with('}') {
            break;
        }
        if !has_comma {
            return Err(Error::UnexpectedToken {
                offset: offset_of(full, rest),
            });
        }
    }
    // Check if content between { and } had any newlines or spaces after commas
    let content_len = content_start.len() - rest.len();
    let content_bytes = content_start.as_bytes();
    let inline = !content_bytes[..content_len].contains(&b'\n');
    let spaced = inline && content_bytes[..content_len].windows(2).any(|w| w == b", ");

    Ok((
        Node::Map(Map {
            open: '{',
            close: '}',
            entries,
            trailing_comma,
            inline,
            spaced,
        }),
        &rest[1..],
    ))
}

fn parse_array<'a>(full: &str, input: &'a str) -> Result<(Node, &'a str), Error> {
    let base = offset_of(full, input);
    if input.as_bytes()[0] != b'[' {
        return Err(Error::UnexpectedToken { offset: base });
    }
    let mut rest = &input[1..];
    let mut items = Vec::new();

    rest = rest.trim_start();
    if let Some(rest) = rest.strip_prefix(']') {
        return Ok((
            Node::Array(Array {
                open: '[',
                close: ']',
                items,
                trailing_comma: false,
                inline: true,
                spaced: false,
            }),
            rest,
        ));
    }

    // The raw span from just after the opening bracket, keeping the
    // whitespace on both sides: `{\n  "a": 1\n}` is not inline, but
    // trimming first hides both newlines and collapses it to one line.
    let content_start = &input[1..];
    #[allow(unused_assignments)]
    let mut trailing_comma = false;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return Err(Error::UnexpectedEof { offset: full.len() });
        }
        let (val, r) = parse_value(full, rest)?;
        items.push(val);
        rest = r.trim_start();
        let has_comma = rest.starts_with(',');
        if has_comma {
            rest = &rest[1..];
        }
        trailing_comma = has_comma;
        rest = rest.trim_start();
        if rest.starts_with(']') {
            break;
        }
        if !has_comma {
            return Err(Error::UnexpectedToken {
                offset: offset_of(full, rest),
            });
        }
    }
    let content_len = content_start.len() - rest.len();
    let content_bytes = content_start.as_bytes();
    let inline = !content_bytes[..content_len].contains(&b'\n');
    let spaced = inline && content_bytes[..content_len].windows(2).any(|w| w == b", ");

    Ok((
        Node::Array(Array {
            open: '[',
            close: ']',
            items,
            trailing_comma,
            inline,
            spaced,
        }),
        &rest[1..],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(input: &str) {
        let doc = JsonDoc::parse(input.as_bytes()).unwrap();
        assert_eq!(doc.serialize(), input.as_bytes(), "round-trip of {input:?}");
    }

    #[test]
    fn roundtrips_compact() {
        roundtrip("{}");
        roundtrip("[]");
        roundtrip("null");
        roundtrip("true");
        roundtrip("false");
        roundtrip("42");
        roundtrip("-3.14");
        roundtrip("\"hello\"");
        roundtrip("{\"a\":1}");
        roundtrip("[1,2,3]");
    }

    #[test]
    fn roundtrips_pretty() {
        roundtrip("{\n  \"a\": 1,\n  \"b\": [2, 3]\n}");
        roundtrip("[\n  1,\n  2,\n  3\n]");
    }

    #[test]
    fn roundtrips_trailing_comma() {
        roundtrip("{\"a\":1,}");
        roundtrip("[1,2,3,]");
    }

    #[test]
    fn roundtrips_strings() {
        roundtrip("\"say \\\"hi\\\"\"");
        roundtrip("\"line1\\nline2\"");
        roundtrip("\"tab\\there\"");
    }

    #[test]
    fn tree_structure() {
        let doc = JsonDoc::parse(b"{\"name\":\"Alice\",\"age\":30}").unwrap();
        match doc.root() {
            Node::Map(map) => {
                assert_eq!(map.entries.len(), 2);
                assert_eq!(map.entries[0].0, "name");
                assert_eq!(map.entries[1].0, "age");
            }
            _ => panic!("expected map"),
        }
    }

    #[test]
    fn array_of_objects_table_shape() {
        let doc = JsonDoc::parse(b"[{\"a\":1},{\"a\":2}]").unwrap();
        match doc.root() {
            Node::Array(arr) => {
                assert_eq!(arr.items.len(), 2);
                for item in &arr.items {
                    assert!(matches!(item, Node::Map(_)));
                }
            }
            _ => panic!("expected array"),
        }
    }
}
