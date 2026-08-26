//! Line-at-a-time syntax highlighting for text view.
//!
//! Per line rather than per document, so a frontend can colour only the
//! rows it draws — a 100 MB file must not be tokenised to show forty
//! lines. That costs a little accuracy: a string spanning several lines
//! is coloured as if each line stood alone. In the formats this tool
//! opens that is rare, and cheap beats exact for something redrawn every
//! frame.

/// What a run of characters is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Plain,
    /// An object key, or an XML attribute name.
    Key,
    Str,
    Number,
    Keyword,
    Punctuation,
    /// XML tag name, including the angle brackets.
    Tag,
    Comment,
    /// `<![CDATA[` and `]]>`, and entity references.
    Escape,
}

/// A run of one kind, as a byte range into the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub token: Token,
}

/// Which grammar to colour by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grammar {
    Json,
    Xml,
    /// CSV is not coloured: every cell is text, and tinting some of them
    /// would imply a type the format does not have.
    None,
}

/// Colour one line.
pub fn highlight(line: &str, grammar: Grammar) -> Vec<Span> {
    match grammar {
        Grammar::Json => json(line),
        Grammar::Xml => xml(line),
        Grammar::None => Vec::new(),
    }
}

fn push(spans: &mut Vec<Span>, start: usize, end: usize, token: Token) {
    if end > start {
        spans.push(Span { start, end, token });
    }
}

fn json(line: &str) -> Vec<Span> {
    let b = line.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' => {
                let start = i;
                let end = string_end(line, i);
                // A string followed by a colon is a key, and worth its own
                // colour: it is the part you scan for.
                let mut j = end;
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                let token = if b.get(j) == Some(&b':') {
                    Token::Key
                } else {
                    Token::Str
                };
                push(&mut spans, start, end, token);
                i = end;
            }
            b'0'..=b'9' | b'-' => {
                let start = i;
                while i < b.len()
                    && (b[i].is_ascii_digit() || matches!(b[i], b'-' | b'+' | b'.' | b'e' | b'E'))
                {
                    i += 1;
                }
                push(&mut spans, start, i, Token::Number);
            }
            b't' | b'f' | b'n' => {
                let start = i;
                let rest = &line[i..];
                let word = ["true", "false", "null"]
                    .into_iter()
                    .find(|w| rest.starts_with(w));
                match word {
                    Some(w) => {
                        i += w.len();
                        push(&mut spans, start, i, Token::Keyword);
                    }
                    None => i += 1,
                }
            }
            b'{' | b'}' | b'[' | b']' | b':' | b',' => {
                push(&mut spans, i, i + 1, Token::Punctuation);
                i += 1;
            }
            _ => i += 1,
        }
    }
    spans
}

/// The offset just past the string starting at `at`, honouring escapes.
fn string_end(line: &str, at: usize) -> usize {
    let b = line.as_bytes();
    let mut i = at + 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    b.len()
}

fn xml(line: &str) -> Vec<Span> {
    let b = line.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if line[i..].starts_with("<![CDATA[") {
            let open = i + "<![CDATA[".len();
            push(&mut spans, i, open, Token::Escape);
            match line[open..].find("]]>") {
                Some(rel) => {
                    let close = open + rel;
                    push(&mut spans, open, close, Token::Str);
                    push(&mut spans, close, close + 3, Token::Escape);
                    i = close + 3;
                }
                // Unterminated on this line: the rest is content.
                None => {
                    push(&mut spans, open, b.len(), Token::Str);
                    i = b.len();
                }
            }
        } else if line[i..].starts_with("<!--") {
            let end = line[i..].find("-->").map_or(b.len(), |r| i + r + 3);
            push(&mut spans, i, end, Token::Comment);
            i = end;
        } else if b[i] == b'<' {
            let end = line[i..].find('>').map_or(b.len(), |r| i + r + 1);
            // Inside a tag, quoted runs are attribute values.
            let mut j = i;
            while j < end {
                if b[j] == b'"' || b[j] == b'\'' {
                    let q = b[j];
                    let mut k = j + 1;
                    while k < end && b[k] != q {
                        k += 1;
                    }
                    let k = (k + 1).min(end);
                    push(&mut spans, j, k, Token::Str);
                    j = k;
                } else {
                    let start = j;
                    while j < end && b[j] != b'"' && b[j] != b'\'' {
                        j += 1;
                    }
                    push(&mut spans, start, j, Token::Tag);
                }
            }
            i = end;
        } else if b[i] == b'&' {
            // An entity reference: `&amp;`, `&#169;`.
            let end = line[i..]
                .find(';')
                .filter(|r| *r <= 12)
                .map_or(i + 1, |r| i + r + 1);
            push(&mut spans, i, end, Token::Escape);
            i = end;
        } else {
            let start = i;
            while i < b.len() && b[i] != b'<' && b[i] != b'&' {
                i += 1;
            }
            push(&mut spans, start, i, Token::Plain);
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(line: &str, g: Grammar) -> Vec<(&str, Token)> {
        highlight(line, g)
            .into_iter()
            .map(|s| (&line[s.start..s.end], s.token))
            .collect()
    }

    #[test]
    fn json_keys_and_values_differ() {
        let got = kinds(r#"{"name": "Alice", "age": 30}"#, Grammar::Json);
        assert!(got.contains(&("\"name\"", Token::Key)), "{got:?}");
        assert!(got.contains(&("\"Alice\"", Token::Str)), "{got:?}");
        assert!(got.contains(&("30", Token::Number)), "{got:?}");
    }

    #[test]
    fn json_keywords_are_their_own_kind() {
        let got = kinds("[true, false, null]", Grammar::Json);
        assert_eq!(
            got.iter().filter(|(_, t)| *t == Token::Keyword).count(),
            3,
            "{got:?}"
        );
    }

    /// A colon inside a string must not turn the next thing into a key.
    #[test]
    fn a_colon_inside_a_string_does_not_make_a_key() {
        let got = kinds(r#"{"a": "x: y"}"#, Grammar::Json);
        assert!(got.contains(&("\"x: y\"", Token::Str)), "{got:?}");
    }

    #[test]
    fn escaped_quotes_do_not_end_a_json_string() {
        let got = kinds(r#"{"a": "say \"hi\""}"#, Grammar::Json);
        assert!(got.contains(&(r#""say \"hi\"""#, Token::Str)), "{got:?}");
    }

    #[test]
    fn xml_tags_attributes_and_text() {
        let got = kinds(r#"<a href="x">text</a>"#, Grammar::Xml);
        assert!(
            got.iter()
                .any(|(s, t)| *t == Token::Tag && s.contains("<a")),
            "{got:?}"
        );
        assert!(got.contains(&("\"x\"", Token::Str)), "{got:?}");
        assert!(got.contains(&("text", Token::Plain)), "{got:?}");
    }

    #[test]
    fn cdata_is_marked_and_its_contents_are_text() {
        let got = kinds("<a><![CDATA[<b>&amp;]]></a>", Grammar::Xml);
        assert!(got.contains(&("<![CDATA[", Token::Escape)), "{got:?}");
        assert!(
            got.contains(&("<b>&amp;", Token::Str)),
            "contents are not markup: {got:?}"
        );
        assert!(got.contains(&("]]>", Token::Escape)), "{got:?}");
    }

    #[test]
    fn entities_outside_cdata_are_marked() {
        let got = kinds("<a>&amp; &#169;</a>", Grammar::Xml);
        assert!(got.contains(&("&amp;", Token::Escape)), "{got:?}");
        assert!(got.contains(&("&#169;", Token::Escape)), "{got:?}");
    }

    #[test]
    fn comments_are_one_run() {
        let got = kinds("<!-- note --><a/>", Grammar::Xml);
        assert!(got.contains(&("<!-- note -->", Token::Comment)), "{got:?}");
    }

    /// Spans must stay inside the line and never overlap, or a renderer
    /// slicing by them will panic on a char boundary.
    #[test]
    fn spans_are_ordered_and_in_range() {
        for (line, g) in [
            (r#"{"a": [1, "b", true], "c": null}"#, Grammar::Json),
            (r#"<a b='1' c="2"><![CDATA[x]]>&amp;</a>"#, Grammar::Xml),
            ("unterminated \"string", Grammar::Json),
            ("<unclosed tag", Grammar::Xml),
            ("", Grammar::Json),
        ] {
            let spans = highlight(line, g);
            let mut last = 0;
            for s in &spans {
                assert!(s.start >= last, "overlap in {line:?}: {spans:?}");
                assert!(s.end <= line.len(), "past the end of {line:?}");
                assert!(line.is_char_boundary(s.start) && line.is_char_boundary(s.end));
                last = s.end;
            }
        }
    }

    #[test]
    fn csv_is_left_alone() {
        assert!(highlight("a,b,c", Grammar::None).is_empty());
    }
}
