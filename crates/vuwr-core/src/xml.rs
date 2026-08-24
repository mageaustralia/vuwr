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
        let (children, _rest) = parse_children(text.trim_start())?;
        Ok(XmlDoc { children })
    }

    pub fn root(&self) -> &Node {
        self.children.first().expect("XML document has no children")
    }

    pub fn root_mut(&mut self) -> &mut Node {
        self.children
            .first_mut()
            .expect("XML document has no children")
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

fn parse_children(input: &str) -> Result<(Vec<Node>, &str), Error> {
    let mut nodes = Vec::new();
    let mut rest = input;
    loop {
        if rest.is_empty() || rest.starts_with("</") {
            break;
        }
        let (node, r) = parse_node(rest)?;
        nodes.push(node);
        rest = r;
    }
    Ok((nodes, rest))
}

fn parse_node(input: &str) -> Result<(Node, &str), Error> {
    if input.starts_with("<?xml") || input.starts_with("<?XML") {
        parse_xml_decl(input)
    } else if input.starts_with("<!--") {
        parse_comment(input)
    } else if input.starts_with("<?") {
        parse_pi(input)
    } else if input.starts_with('<') {
        parse_element(input)
    } else {
        let end = input.find('<').unwrap_or(input.len());
        let text = input[..end].to_string();
        Ok((Node::Text(text), &input[end..]))
    }
}

fn parse_xml_decl(input: &str) -> Result<(Node, &str), Error> {
    let end = input.find("?>").ok_or(Error::UnclosedQuote { offset: 0 })?;
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

fn parse_comment(input: &str) -> Result<(Node, &str), Error> {
    let end = input
        .find("-->")
        .ok_or(Error::UnclosedQuote { offset: 0 })?;
    let text = input[4..end].to_string();
    Ok((Node::Comment(text), &input[end + 3..]))
}

fn parse_pi(input: &str) -> Result<(Node, &str), Error> {
    let end = input.find("?>").ok_or(Error::UnclosedQuote { offset: 0 })?;
    let content = &input[2..end];
    let rest = &input[end + 2..];
    let parts: Vec<&str> = content.splitn(2, ' ').collect();
    let target = parts[0].to_string();
    let data = parts.get(1).unwrap_or(&"").to_string();
    Ok((Node::ProcessingInstruction { target, data }, rest))
}

fn parse_element(input: &str) -> Result<(Node, &str), Error> {
    if !input.starts_with('<') {
        return Err(Error::InvalidUtf8);
    }
    let gt_offset = find_tag_end(&input[1..])?; // index of > in input[1..]
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
        let (children, rest) = parse_children(rest)?;
        let rest = rest.trim_start();
        if rest.starts_with("</") {
            let close_end = rest.find('>').ok_or(Error::UnclosedQuote { offset: 0 })?;
            Ok((
                Node::Element(Element {
                    tag: tag.clone(),
                    attributes: attrs,
                    children,
                    self_closing: false,
                }),
                &rest[close_end + 1..],
            ))
        } else {
            Ok((
                Node::Element(Element {
                    tag,
                    attributes: attrs,
                    children,
                    self_closing: true,
                }),
                rest,
            ))
        }
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
