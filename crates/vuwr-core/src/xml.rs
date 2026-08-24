//! XML parsing and serialization with format-preserving round-trips.
//!
//! The parser preserves comments, attribute order, text content (including
//! whitespace), processing instructions, and the XML declaration.
//! Self-closing tags (`<br/>`) are distinguished from empty elements
//! (`<br></br>`).

use crate::Error;
use crate::node::{Attr, Element, Node, XmlDecl};

/// A parsed XML document.
#[derive(Debug, Clone)]
pub struct XmlDoc {
    children: Vec<Node>,
}

impl XmlDoc {
    pub fn parse(bytes: &[u8]) -> Result<XmlDoc, Error> {
        let text = std::str::from_utf8(bytes).map_err(|_| Error::InvalidUtf8)?;
        let (children, _rest) = parse_children(text, text.trim_start())?;
        // `root`/`root_mut` index into this, so an empty document would
        // panic. Reject it here instead.
        if children.is_empty() {
            return Err(Error::UnexpectedEof {
                offset: bytes.len(),
            });
        }
        Ok(XmlDoc { children })
    }

    /// The document element.
    ///
    /// This is the first *element* child, not the first child: a document
    /// opening with `<?xml ... ?>` or a comment keeps those in `children`
    /// for round-trip fidelity, and returning the declaration here made
    /// every real-world file look like it had no table shape.
    pub fn root(&self) -> &Node {
        self.children
            .iter()
            .find(|n| matches!(n, Node::Element(_)))
            .or_else(|| self.children.first())
            .expect("parse rejects an empty document")
    }

    pub fn root_mut(&mut self) -> &mut Node {
        let idx = self
            .children
            .iter()
            .position(|n| matches!(n, Node::Element(_)))
            .unwrap_or(0);
        &mut self.children[idx]
    }

    /// The rows of table view: the element children of the document
    /// element. Whitespace between elements parses as `Text` nodes, and
    /// comments as `Comment`; neither is a row, so both are skipped —
    /// otherwise a pretty-printed file's rows are all off by one.
    pub fn row_elements(&self) -> Vec<&Element> {
        match self.root() {
            Node::Element(root) => root
                .children
                .iter()
                .filter_map(|c| match c {
                    Node::Element(e) => Some(e),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Column headers for table view: the first row's attribute names,
    /// then the tags of its child elements.
    pub fn table_headers(&self) -> Vec<String> {
        let rows = self.row_elements();
        let Some(first) = rows.first() else {
            return Vec::new();
        };
        let mut headers: Vec<String> = first.attributes.iter().map(|(k, _, _)| k.clone()).collect();
        headers.extend(first.children.iter().filter_map(|c| match c {
            Node::Element(e) => Some(e.tag.clone()),
            _ => None,
        }));
        headers
    }

    /// The path addressing the cell at `(row, col)`: an attribute of the
    /// row element, or the text of one of its child elements.
    pub fn cell_path(&self, row: usize, col: usize) -> Option<crate::node::NodePath> {
        use crate::node::PathSeg;
        let rows = self.row_elements();
        let elem = rows.get(row)?;
        if let Some((name, _, _)) = elem.attributes.get(col) {
            return Some(vec![PathSeg::Index(row), PathSeg::Attr(name.clone())]);
        }
        let child_idx = col - elem.attributes.len();
        // The child must exist for the path to be writable.
        elem.children
            .iter()
            .filter(|c| matches!(c, Node::Element(_)))
            .nth(child_idx)?;
        Some(vec![
            PathSeg::Index(row),
            PathSeg::Index(child_idx),
            PathSeg::Text,
        ])
    }

    /// The value at `(row, col)` under [`XmlDoc::table_headers`].
    /// Attribute columns come first, then child-element text.
    pub fn table_cell(&self, row: usize, col: usize) -> Option<String> {
        let rows = self.row_elements();
        let elem = rows.get(row)?;
        if let Some((_, value, _)) = elem.attributes.get(col) {
            return Some(value.clone());
        }
        let child_idx = col - elem.attributes.len();
        let child = elem
            .children
            .iter()
            .filter_map(|c| match c {
                Node::Element(e) => Some(e),
                _ => None,
            })
            .nth(child_idx)?;
        // An empty element (`<name/>`) has no children at all — indexing
        // child.children[0] here used to panic.
        Some(
            child
                .children
                .iter()
                .filter_map(|c| match c {
                    Node::Text(t) => Some(t.as_str()),
                    _ => None,
                })
                .collect::<String>(),
        )
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for child in &self.children {
            serialize_node(child, &mut out);
        }
        out
    }
}

fn serialize_node(node: &Node, out: &mut Vec<u8>) {
    match node {
        Node::XmlDecl(decl) => {
            out.extend_from_slice(b"<?xml");
            out.extend_from_slice(format!(" version=\"{}\"", decl.version).as_bytes());
            if let Some(enc) = &decl.encoding {
                out.extend_from_slice(format!(" encoding=\"{}\"", enc).as_bytes());
            }
            if let Some(sa) = &decl.standalone {
                out.extend_from_slice(format!(" standalone=\"{}\"", sa).as_bytes());
            }
            out.extend_from_slice(b"?>");
        }
        Node::Comment(text) => {
            out.extend_from_slice(b"<!--");
            out.extend_from_slice(text.as_bytes());
            out.extend_from_slice(b"-->");
        }
        Node::Text(text) => {
            out.extend_from_slice(text.as_bytes());
        }
        Node::ProcessingInstruction { target, data } => {
            out.extend_from_slice(format!("<?{}", target).as_bytes());
            if !data.is_empty() {
                out.push(b' ');
                out.extend_from_slice(data.as_bytes());
            }
            out.extend_from_slice(b"?>");
        }
        Node::Element(elem) => {
            out.push(b'<');
            out.extend_from_slice(elem.tag.as_bytes());
            for (name, value, q) in &elem.attributes {
                out.push(b' ');
                out.extend_from_slice(name.as_bytes());
                out.extend_from_slice(format!("={}{}{}", q, value, q).as_bytes());
            }
            if elem.self_closing {
                out.extend_from_slice(b"/>");
            } else {
                out.push(b'>');
                for child in &elem.children {
                    serialize_node(child, out);
                }
                out.extend_from_slice(b"</");
                out.extend_from_slice(elem.tag.as_bytes());
                out.push(b'>');
            }
        }
        _ => {}
    }
}

// --- Parser ---

/// Absolute byte offset of `rest` within `full`. Sound because every
/// parser function only ever receives a suffix of `full`.
fn offset_of(full: &str, rest: &str) -> usize {
    full.len() - rest.len()
}

fn parse_children<'a>(full: &str, input: &'a str) -> Result<(Vec<Node>, &'a str), Error> {
    let mut nodes = Vec::new();
    let mut rest = input;
    loop {
        if rest.is_empty() || rest.starts_with("</") {
            break;
        }
        let (node, r) = parse_node(full, rest)?;
        nodes.push(node);
        rest = r;
    }
    Ok((nodes, rest))
}

fn parse_node<'a>(full: &str, input: &'a str) -> Result<(Node, &'a str), Error> {
    if input.starts_with("<?xml") || input.starts_with("<?XML") {
        parse_xml_decl(full, input)
    } else if input.starts_with("<!--") {
        parse_comment(full, input)
    } else if input.starts_with("<?") {
        parse_pi(full, input)
    } else if input.starts_with('<') {
        parse_element(full, input)
    } else {
        let end = input.find('<').unwrap_or(input.len());
        let text = input[..end].to_string();
        Ok((Node::Text(text), &input[end..]))
    }
}

fn parse_xml_decl<'a>(full: &str, input: &'a str) -> Result<(Node, &'a str), Error> {
    let end = input.find("?>").ok_or(Error::UnexpectedEof {
        offset: offset_of(full, input),
    })?;
    let content = &input[5..end];
    let rest = &input[end + 2..];

    let mut version = String::new();
    let mut encoding = None;
    let mut standalone = None;

    for attr in parse_attrs(content) {
        match attr.0.as_str() {
            "version" => version = attr.1,
            "encoding" => encoding = Some(attr.1),
            "standalone" => standalone = Some(attr.1),
            _ => {}
        }
    }

    Ok((
        Node::XmlDecl(XmlDecl {
            version,
            encoding,
            standalone,
        }),
        rest,
    ))
}

fn parse_comment<'a>(full: &str, input: &'a str) -> Result<(Node, &'a str), Error> {
    let end = input.find("-->").ok_or(Error::UnexpectedEof {
        offset: offset_of(full, input),
    })?;
    let text = input[4..end].to_string();
    Ok((Node::Comment(text), &input[end + 3..]))
}

fn parse_pi<'a>(full: &str, input: &'a str) -> Result<(Node, &'a str), Error> {
    let end = input.find("?>").ok_or(Error::UnexpectedEof {
        offset: offset_of(full, input),
    })?;
    let content = &input[2..end];
    let rest = &input[end + 2..];
    let parts: Vec<&str> = content.splitn(2, ' ').collect();
    let target = parts[0].to_string();
    let data = parts.get(1).unwrap_or(&"").to_string();
    Ok((Node::ProcessingInstruction { target, data }, rest))
}

fn parse_element<'a>(full: &str, input: &'a str) -> Result<(Node, &'a str), Error> {
    let base = offset_of(full, input);
    if !input.starts_with('<') {
        return Err(Error::UnexpectedToken { offset: base });
    }
    let gt_offset =
        find_tag_end(&input[1..]).map_err(|_| Error::UnexpectedEof { offset: full.len() })?; // index of > in input[1..]
    let abs_gt = 1 + gt_offset; // index of > in input
    let tag_content = &input[1..abs_gt]; // between < and >
    let rest = &input[abs_gt + 1..]; // after >

    let self_closing = tag_content.ends_with('/');
    let tag_content = if self_closing {
        &tag_content[..tag_content.len() - 1]
    } else {
        tag_content
    };

    let (tag, attrs) = parse_tag_and_attrs(tag_content)?;

    if self_closing {
        Ok((
            Node::Element(Element {
                tag,
                attributes: attrs,
                children: Vec::new(),
                self_closing: true,
            }),
            rest,
        ))
    } else {
        let (children, rest) = parse_children(full, rest)?;
        let rest = rest.trim_start();
        // A missing closing tag used to be silently rewritten as a
        // self-closing element, so `<a>text` round-tripped as `<a/>` and
        // the content vanished. And any closing tag was accepted, so
        // `<r><a></r>` parsed happily. Both are errors.
        if !rest.starts_with("</") {
            return Err(Error::UnclosedTag { tag, offset: base });
        }
        let close_end = rest
            .find('>')
            .ok_or(Error::UnexpectedEof { offset: full.len() })?;
        let closing = rest[2..close_end].trim();
        if closing != tag {
            return Err(Error::MismatchedTag {
                opened: tag,
                closed: closing.to_string(),
                offset: offset_of(full, rest),
            });
        }
        Ok((
            Node::Element(Element {
                tag,
                attributes: attrs,
                children,
                self_closing: false,
            }),
            &rest[close_end + 1..],
        ))
    }
}

fn find_tag_end(input: &str) -> Result<usize, Error> {
    let bytes = input.as_bytes();
    let mut in_quote = false;
    let mut quote_char = b'"';
    for (i, &b) in bytes.iter().enumerate() {
        if in_quote {
            if b == quote_char {
                in_quote = false;
            }
        } else {
            match b {
                b'"' | b'\'' => {
                    in_quote = true;
                    quote_char = b;
                }
                b'>' => return Ok(i),
                _ => {}
            }
        }
    }
    Err(Error::UnclosedQuote { offset: 0 })
}

fn parse_tag_and_attrs(content: &str) -> Result<(String, Vec<Attr>), Error> {
    let content = content.trim_start();
    let name_end = content
        .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .unwrap_or(content.len());
    let tag = content[..name_end].to_string();
    let attrs = parse_attrs(&content[name_end..]);
    Ok((tag, attrs))
}

fn parse_attrs(content: &str) -> Vec<Attr> {
    let mut attrs = Vec::new();
    let mut rest = content;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() || rest.starts_with('>') || rest.starts_with('/') {
            break;
        }
        let name_end = rest
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(rest.len());
        if name_end == 0 {
            break;
        }
        let name = rest[..name_end].to_string();
        rest = &rest[name_end..];
        if !rest.starts_with('=') {
            attrs.push((name, String::new(), '"'));
            continue;
        }
        rest = &rest[1..];
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let quote = rest.as_bytes()[0];
        if quote == b'"' || quote == b'\'' {
            let close = rest[1..]
                .find(quote as char)
                .map(|i| i + 1)
                .unwrap_or(rest.len());
            let value = rest[1..close].to_string();
            attrs.push((name, value, quote as char));
            rest = &rest[close + 1..];
        } else {
            let val_end = rest
                .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
                .unwrap_or(rest.len());
            let value = rest[..val_end].to_string();
            attrs.push((name, value, '"'));
            rest = &rest[val_end..];
        }
    }
    attrs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(input: &str) {
        let doc = XmlDoc::parse(input.as_bytes()).unwrap();
        assert_eq!(doc.serialize(), input.as_bytes(), "round-trip of {input:?}");
    }

    #[test]
    fn roundtrips_simple() {
        roundtrip("<root/>");
        roundtrip("<root></root>");
        roundtrip("<root>hello</root>");
        roundtrip("<root><child/></root>");
    }

    #[test]
    fn roundtrips_attributes() {
        roundtrip("<root a=\"1\" b=\"2\"/>");
        roundtrip("<root key='value'/>");
    }

    #[test]
    fn roundtrips_comment() {
        roundtrip("<!-- hello -->");
        roundtrip("<root><!-- comment --><child/></root>");
    }

    #[test]
    fn roundtrips_xml_decl() {
        roundtrip("<?xml version=\"1.0\"?>");
        roundtrip("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    }

    #[test]
    fn roundtrips_pi() {
        roundtrip("<?php echo 'hi' ?>");
    }

    #[test]
    fn roundtrips_mixed_content() {
        roundtrip("<root>text <!-- comment --><child/>more text</root>");
    }

    #[test]
    fn preserves_attribute_order() {
        let doc = XmlDoc::parse(b"<root z=\"1\" a=\"2\" m=\"3\"/>").unwrap();
        match doc.root() {
            Node::Element(e) => {
                assert_eq!(e.attributes[0].0, "z");
                assert_eq!(e.attributes[1].0, "a");
                assert_eq!(e.attributes[2].0, "m");
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn preserves_comments() {
        let doc = XmlDoc::parse(b"<root><!-- keep me --></root>").unwrap();
        match doc.root() {
            Node::Element(e) => {
                assert_eq!(e.children.len(), 1);
                assert!(matches!(&e.children[0], Node::Comment(s) if s == " keep me "));
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn self_closing_vs_empty() {
        let doc = XmlDoc::parse(b"<root><br/><p></p></root>").unwrap();
        match doc.root() {
            Node::Element(e) => {
                if let Node::Element(br) = &e.children[0] {
                    assert!(br.self_closing);
                    assert_eq!(br.tag, "br");
                } else {
                    panic!("expected br element");
                }
                if let Node::Element(p) = &e.children[1] {
                    assert!(!p.self_closing);
                    assert_eq!(p.tag, "p");
                } else {
                    panic!("expected p element");
                }
            }
            _ => panic!("expected element"),
        }
    }
}
