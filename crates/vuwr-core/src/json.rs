//! JSON parsing and serialization with format-preserving round-trips.
//!
//! The parser builds a `Node` tree carrying source-fidelity metadata:
//! key order, indentation style, trailing commas, and native types.
//! Serialization reproduces the original formatting byte-for-byte unless
//! an edit changed it.

use crate::Error;

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

/// A JSON value with source-fidelity metadata.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Null,
    Bool(bool),
    Number(String),
    Str(String),
    Array(Array),
    Map(Map),
}

/// An array preserves its opening bracket, trailing comma, and indentation.
#[derive(Debug, Clone, PartialEq)]
pub struct Array {
    pub open: char,
    pub close: char,
    pub items: Vec<Node>,
    pub trailing_comma: bool,
    /// True if the original source had this array on a single line.
    pub inline: bool,
    /// True if the original had spaces after commas (`, ` not `,`).
    pub spaced: bool,
}

/// A map preserves key order, trailing comma, and indentation.
#[derive(Debug, Clone, PartialEq)]
pub struct Map {
    pub open: char,
    pub close: char,
    pub entries: Vec<(String, Node)>,
    pub trailing_comma: bool,
    /// True if the original source had this map on a single line.
    pub inline: bool,
    /// True if the original had spaces after commas (`, ` not `,`).
    pub spaced: bool,
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
        let (node, _rest) = parse_value(text.trim_start())?;
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
    }
}

fn serialize_string(s: &str, out: &mut Vec<u8>) {
    out.push(b'"');
    for b in s.bytes() {
        match b {
            b'"' => out.extend_from_slice(b"\\\""),
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\t' => out.extend_from_slice(b"\\t"),
            0x20..=0x7e => out.push(b),
            _ => {
                // Control characters: escape as \uXXXX
                let hex = format!("\\u{:04x}", b);
                out.extend_from_slice(hex.as_bytes());
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
        if trimmed.is_empty() || trimmed == "{" || trimmed == "}" || trimmed == "[" || trimmed == "]" {
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

fn parse_value(input: &str) -> Result<(Node, &str), Error> {
    let input = input.trim_start();
    if input.is_empty() {
        return Err(Error::InvalidUtf8);
    }
    match input.as_bytes()[0] {
        b'"' => {
            let (s, rest) = parse_string(input)?;
            Ok((Node::Str(s), rest))
        }
        b'{' => parse_map(input),
        b'[' => parse_array(input),
        b't' if input.starts_with("true") => Ok((Node::Bool(true), &input[4..])),
        b'f' if input.starts_with("false") => Ok((Node::Bool(false), &input[5..])),
        b'n' if input.starts_with("null") => Ok((Node::Null, &input[4..])),
        b'-' | b'0'..=b'9' => parse_number(input),
        _ => Err(Error::InvalidUtf8),
    }
}

fn parse_string(input: &str) -> Result<(String, &str), Error> {
    let bytes = input.as_bytes();
    if bytes[0] != b'"' {
        return Err(Error::InvalidUtf8);
    }
    let mut i = 1;
    let mut result = String::new();
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return Ok((result, &input[i + 1..])),
            b'\\' => {
                i += 1;
                if i >= bytes.len() {
                    return Err(Error::InvalidUtf8);
                }
                match bytes[i] {
                    b'"' => result.push('"'),
                    b'\\' => result.push('\\'),
                    b'/' => result.push('/'),
                    b'n' => result.push('\n'),
                    b'r' => result.push('\r'),
                    b't' => result.push('\t'),
                    b'b' => result.push('\u{0008}'),
                    b'f' => result.push('\u{000C}'),
                    b'u' => {
                        // Parse 4 hex digits
                        if i + 4 >= bytes.len() {
                            return Err(Error::InvalidUtf8);
                        }
                        let hex: String = input[i + 1..i + 5].to_string();
                        let code = u32::from_str_radix(&hex, 16)
                            .map_err(|_| Error::InvalidUtf8)?;
                        let ch = char::from_u32(code).ok_or(Error::InvalidUtf8)?;
                        result.push(ch);
                        i += 4;
                    }
                    _ => return Err(Error::InvalidUtf8),
                }
            }
            0x20..=0x7e => {
                result.push(bytes[i] as char);
            }
            _ => return Err(Error::InvalidUtf8),
        }
        i += 1;
    }
    Err(Error::UnclosedQuote { offset: 0 })
}

fn parse_number(input: &str) -> Result<(Node, &str), Error> {
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
        return Err(Error::InvalidUtf8);
    }
    let num_str = input[..i].to_string();
    let rest = &input[i..];
    Ok((Node::Number(num_str), rest))
}

fn parse_map(input: &str) -> Result<(Node, &str), Error> {
    let bytes = input.as_bytes();
    if bytes[0] != b'{' {
        return Err(Error::InvalidUtf8);
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

    // Remember the raw slice from after { to find closing }
    let content_start = rest;
    #[allow(unused_assignments)]
    let mut trailing_comma = false;
    loop {
        rest = rest.trim_start();
        let (key, r) = parse_string(rest)?;
        rest = r.trim_start();
        if !rest.starts_with(':') {
            return Err(Error::InvalidUtf8);
        }
        rest = rest[1..].trim_start();
        let (val, r) = parse_value(rest)?;
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
    }
    // Check if content between { and } had any newlines or spaces after commas
    let content_len = content_start.len() - rest.len() - 1;
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

fn parse_array(input: &str) -> Result<(Node, &str), Error> {
    let bytes = input.as_bytes();
    if bytes[0] != b'[' {
        return Err(Error::InvalidUtf8);
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

    let content_start = rest;
    #[allow(unused_assignments)]
    let mut trailing_comma = false;
    loop {
        rest = rest.trim_start();
        let (val, r) = parse_value(rest)?;
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
    }
    let content_len = content_start.len() - rest.len() - 1;
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
