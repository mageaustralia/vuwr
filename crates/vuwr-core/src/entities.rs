//! XML entity references, decoded for reading and encoded for writing.
//!
//! A feed's description is often escaped HTML: `&lt;p&gt;Text&lt;/p&gt;`.
//! Reading that is unpleasant and editing it is worse, so it is decoded
//! for display and re-encoded on the way back in.
//!
//! The document still stores the raw text, so a value nobody edits keeps
//! its exact bytes. Only an edited value is re-encoded, and re-encoding is
//! not always byte-identical — `&#39;` and `&apos;` both decode to `'` and
//! both encode back to `&apos;`. That is a change the user asked for by
//! editing, not one we made behind their back.

/// Decode the entity references in `text`.
///
/// Unknown entities are left exactly as written: `&nbsp;` has no meaning
/// in XML without a DTD, and inventing one would change the document.
pub fn decode(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'&' {
            let ch = text[i..].chars().next().expect("char boundary");
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        match text[i..].find(';').filter(|end| *end <= 12) {
            Some(rel) => {
                let entity = &text[i + 1..i + rel];
                match resolve(entity) {
                    Some(c) => {
                        out.push(c);
                        i += rel + 1;
                    }
                    None => {
                        out.push('&');
                        i += 1;
                    }
                }
            }
            None => {
                out.push('&');
                i += 1;
            }
        }
    }
    out
}

fn resolve(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let digits = entity.strip_prefix('#')?;
            let code = match digits
                .strip_prefix('x')
                .or_else(|| digits.strip_prefix('X'))
            {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse::<u32>().ok()?,
            };
            char::from_u32(code)
        }
    }
}

/// Encode the characters that cannot appear literally in XML text.
///
/// Only the five that matter: encoding more would rewrite text the user
/// did not touch. Quotes are left alone in content, where they are legal.
pub fn encode(text: &str) -> String {
    if !text.contains(['&', '<', '>']) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_five_named_entities_decode() {
        assert_eq!(decode("&lt;p&gt;hi&lt;/p&gt;"), "<p>hi</p>");
        assert_eq!(decode("a &amp; b"), "a & b");
        assert_eq!(decode("&quot;q&quot; &apos;a&apos;"), "\"q\" 'a'");
    }

    #[test]
    fn numeric_references_decode_in_both_bases() {
        assert_eq!(decode("&#169;"), "©");
        assert_eq!(decode("&#x2014;"), "—");
        assert_eq!(decode("it&#039;s"), "it's");
    }

    /// `&nbsp;` has no meaning in XML without a DTD, so guessing one would
    /// change the document.
    #[test]
    fn unknown_entities_are_left_alone() {
        assert_eq!(decode("a&nbsp;b"), "a&nbsp;b");
        assert_eq!(decode("Q&A"), "Q&A");
        assert_eq!(decode("a & b"), "a & b");
        assert_eq!(decode("&notanentity"), "&notanentity");
    }

    #[test]
    fn encoding_covers_what_cannot_be_literal() {
        assert_eq!(encode("<p>a & b</p>"), "&lt;p&gt;a &amp; b&lt;/p&gt;");
        assert_eq!(encode("plain text"), "plain text");
    }

    /// Decoding then encoding must not lose anything, though it may
    /// normalise how a character was spelled.
    #[test]
    fn a_round_trip_preserves_meaning() {
        for original in [
            "&lt;p&gt;The ALU Power string&lt;/p&gt;",
            "a &amp; b &lt; c",
            "plain",
        ] {
            assert_eq!(encode(&decode(original)), original);
        }
        // `&#39;` normalises to a literal apostrophe, which is legal in
        // content: the meaning survives, the spelling does not.
        assert_eq!(decode("it&#039;s"), "it's");
        assert_eq!(encode("it's"), "it's");
    }

    #[test]
    fn decoding_leaves_ordinary_text_untouched() {
        let text = "no entities here at all";
        assert_eq!(decode(text), text);
    }
}
