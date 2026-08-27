//! Finding the links in a piece of text.
//!
//! In core rather than in a frontend because both of them have to agree
//! about where a link ends, and because the rule is fiddly enough to be
//! worth testing on its own: a URL at the end of a sentence is followed
//! by a full stop that is not part of it, and one inside a parenthesis is
//! not followed by the bracket.

/// Where each link sits in `text`, as byte offsets.
pub fn links(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut at = 0usize;
    while at < bytes.len() {
        let Some(start) = next_scheme(text, at) else {
            break;
        };
        let end = end_of_link(text, start);
        if end > start {
            found.push((start, end));
            at = end;
        } else {
            at = start + 1;
        }
    }
    found
}

/// The whole of `text` as a link, if that is all it is.
///
/// The common case by far: a `g:link` column holds one URL and nothing
/// else, and a cell that *is* a link can be treated more simply than a
/// sentence with one in it.
pub fn as_link(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    // Cheap rejection first. A whole-value link must begin with the
    // scheme, and this runs for every cell on screen on every frame — a
    // description is five thousand characters, and scanning all of them
    // for `http` to conclude "no" is work done sixty times a second.
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return None;
    }
    let found = links(trimmed);
    match found.as_slice() {
        [(0, end)] if *end == trimmed.len() => Some(trimmed),
        _ => None,
    }
}

/// The next `http://` or `https://`, at a boundary rather than inside a
/// longer word.
fn next_scheme(text: &str, from: usize) -> Option<usize> {
    let mut at = from;
    while at < text.len() {
        let rest = text.get(at..)?;
        let offset = rest.find("http")?;
        let start = at + offset;
        let tail = text.get(start..)?;
        let scheme_ok = tail.starts_with("https://") || tail.starts_with("http://");
        // Not the tail of a longer word: `nothttp://x` is not a link, and
        // neither is the `http` inside `xhttp`.
        let boundary = start == 0
            || text[..start]
                .chars()
                .next_back()
                .is_some_and(|c| !c.is_alphanumeric() && c != '.' && c != '/');
        if scheme_ok && boundary {
            return Some(start);
        }
        at = start + 4;
    }
    None
}

/// Where the link that starts at `start` ends.
fn end_of_link(text: &str, start: usize) -> usize {
    let mut end = start;
    let mut depth = 0i32;
    for (offset, c) in text[start..].char_indices() {
        // Whitespace, a quote or a tag ends it — the last two because a
        // link in XML text is nearly always inside an attribute or beside
        // markup.
        if c.is_whitespace() || c == '"' || c == '\'' || c == '<' || c == '>' {
            break;
        }
        if c == '(' {
            depth += 1;
        }
        if c == ')' {
            // A closing bracket belongs to the link only if the link
            // opened one: `(see https://example.com)` ends at the URL.
            if depth == 0 {
                break;
            }
            depth -= 1;
        }
        end = start + offset + c.len_utf8();
    }
    // Trailing punctuation belongs to the sentence, not the address.
    while end > start {
        let last = text[start..end].chars().next_back().unwrap();
        if matches!(last, '.' | ',' | ';' | ':' | '!' | '?') {
            end -= last.len_utf8();
        } else {
            break;
        }
    }
    // A scheme with nothing after it is not a link.
    let scheme = if text[start..].starts_with("https://") {
        8
    } else {
        7
    };
    if end <= start + scheme { start } else { end }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<&str> {
        links(text).into_iter().map(|(a, b)| &text[a..b]).collect()
    }

    #[test]
    fn a_bare_url_is_the_whole_of_it() {
        assert_eq!(
            as_link("https://example.com/a?b=1&c=2"),
            Some("https://example.com/a?b=1&c=2")
        );
        // Surrounding space does not stop it being one thing.
        assert_eq!(
            as_link("  https://example.com  "),
            Some("https://example.com")
        );
    }

    #[test]
    fn a_sentence_keeps_its_punctuation() {
        assert_eq!(found("See https://example.com."), ["https://example.com"]);
        assert_eq!(found("(see https://example.com)"), ["https://example.com"]);
        assert_eq!(
            found("https://example.com/a_(b) is the page"),
            ["https://example.com/a_(b)"],
            "a bracket the link opened belongs to it"
        );
    }

    #[test]
    fn markup_and_quotes_end_a_link() {
        assert_eq!(
            found(r#"<a href="https://example.com/x">text</a>"#),
            ["https://example.com/x"]
        );
        assert_eq!(found("<p>https://example.com</p>"), ["https://example.com"]);
    }

    #[test]
    fn several_in_one_value() {
        assert_eq!(
            found("https://a.example.com and https://b.example.com/x"),
            ["https://a.example.com", "https://b.example.com/x"]
        );
    }

    #[test]
    fn things_that_are_not_links() {
        assert!(found("no links here").is_empty());
        assert!(found("nothttp://example.com").is_empty(), "inside a word");
        assert!(found("https://").is_empty(), "a scheme and nothing else");
        assert!(found("ftp://example.com").is_empty(), "only http(s)");
        // A cell holding a sentence is not itself a link.
        assert_eq!(as_link("See https://example.com for more"), None);
    }
}
