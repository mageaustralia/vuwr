//! Shared document node tree. One Node tree carries source-fidelity metadata
//! for all formats: JSON, XML, and CSV (degenerate case).

/// XML attribute: (name, value, quote_char).
pub type Attr = (String, String, char);

/// A document node with source-fidelity metadata.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    // --- JSON / generic ---
    Null,
    Bool(bool),
    Number(String),
    Str(String),
    Array(Array),
    Map(Map),
    // --- XML ---
    Element(Element),
    Comment(String),
    Text(String),
    XmlDecl(XmlDecl),
    ProcessingInstruction { target: String, data: String },
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

/// An XML element: tag, attributes (order preserved), children, and
/// whether it was self-closing (`<br/>`).
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub tag: String,
    pub attributes: Vec<Attr>,
    pub children: Vec<Node>,
    pub self_closing: bool,
}

/// XML declaration: `<?xml version="1.0" encoding="UTF-8"?>`.
#[derive(Debug, Clone, PartialEq)]
pub struct XmlDecl {
    pub version: String,
    pub encoding: Option<String>,
    pub standalone: Option<String>,
}

// --- Convenience constructors ---

impl Node {
    pub fn null() -> Self {
        Node::Null
    }

    pub fn bool(b: bool) -> Self {
        Node::Bool(b)
    }

    pub fn number(s: impl Into<String>) -> Self {
        Node::Number(s.into())
    }

    pub fn string(s: impl Into<String>) -> Self {
        Node::Str(s.into())
    }

    pub fn text(s: impl Into<String>) -> Self {
        Node::Text(s.into())
    }

    pub fn comment(s: impl Into<String>) -> Self {
        Node::Comment(s.into())
    }

    pub fn element(tag: impl Into<String>) -> Self {
        Node::Element(Element {
            tag: tag.into(),
            attributes: Vec::new(),
            children: Vec::new(),
            self_closing: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_convenience_constructors() {
        assert_eq!(Node::null(), Node::Null);
        assert_eq!(Node::bool(true), Node::Bool(true));
        assert_eq!(Node::number("42"), Node::Number("42".to_string()));
        assert_eq!(Node::string("hi"), Node::Str("hi".to_string()));
        assert_eq!(Node::text("hello"), Node::Text("hello".to_string()));
        assert_eq!(Node::comment("x"), Node::Comment("x".to_string()));
    }
}
