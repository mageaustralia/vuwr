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

/// One step in a path to a node.
///
/// Paths are how edits address a value in a tree, the way `(row, column)`
/// addresses one in a sheet. `Index` covers both array items and an
/// element's *element* children — whitespace and comments are not
/// addressable, so indices match what the table view shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSeg {
    Key(String),
    Index(usize),
    /// An XML attribute. Only valid as the final step.
    Attr(String),
    /// An XML element's text content. Only valid as the final step.
    ///
    /// Text is not reachable by `Index`, which walks element children only
    /// so that indices line up with the rows the table view shows.
    Text,
}

pub type NodePath = Vec<PathSeg>;

impl Node {
    /// Replace the node at `path`, returning what was there.
    ///
    /// The returned value is exactly what `set_at` needs to put it back,
    /// which is what makes undo byte-exact.
    pub fn set_at(&mut self, path: &[PathSeg], value: Node) -> Result<Node, crate::Error> {
        let Some((last, parents)) = path.split_last() else {
            return Ok(std::mem::replace(self, value));
        };
        let mut node = self;
        for seg in parents {
            node = node.child_mut(seg)?;
        }
        match last {
            PathSeg::Attr(name) => {
                let Node::Element(e) = node else {
                    return Err(crate::Error::NoSuchPath);
                };
                let attr = e
                    .attributes
                    .iter_mut()
                    .find(|(k, _, _)| k == name)
                    .ok_or(crate::Error::NoSuchPath)?;
                let old = std::mem::replace(
                    &mut attr.1,
                    match value {
                        Node::Str(s) => s,
                        other => other.scalar_text(),
                    },
                );
                Ok(Node::Str(old))
            }
            PathSeg::Text => {
                let Node::Element(e) = node else {
                    return Err(crate::Error::NoSuchPath);
                };
                let old: String = e
                    .children
                    .iter()
                    .filter_map(|c| match c {
                        Node::Text(t) => Some(t.as_str()),
                        _ => None,
                    })
                    .collect();
                let text = match value {
                    Node::Text(t) | Node::Str(t) => t,
                    other => other.scalar_text(),
                };
                // Replace the first text child and drop any others, so
                // repeated edits do not accumulate fragments.
                let mut replaced = false;
                e.children.retain_mut(|c| match c {
                    Node::Text(t) if !replaced => {
                        *t = text.clone();
                        replaced = true;
                        true
                    }
                    Node::Text(_) => false,
                    _ => true,
                });
                if !replaced {
                    e.children.push(Node::Text(text));
                    e.self_closing = false;
                }
                Ok(Node::Text(old))
            }
            seg => {
                let slot = node.child_mut(seg)?;
                Ok(std::mem::replace(slot, value))
            }
        }
    }

    /// The node at `path`, if it exists.
    pub fn get_at(&self, path: &[PathSeg]) -> Option<&Node> {
        let mut node = self;
        for seg in path {
            node = match (node, seg) {
                (Node::Map(m), PathSeg::Key(k)) => {
                    m.entries.iter().find(|(key, _)| key == k).map(|(_, v)| v)?
                }
                (Node::Array(a), PathSeg::Index(i)) => a.items.get(*i)?,
                (Node::Element(e), PathSeg::Index(i)) => e
                    .children
                    .iter()
                    .filter(|c| matches!(c, Node::Element(_)))
                    .nth(*i)?,
                _ => return None,
            };
        }
        Some(node)
    }

    fn child_mut(&mut self, seg: &PathSeg) -> Result<&mut Node, crate::Error> {
        match (self, seg) {
            (Node::Map(m), PathSeg::Key(k)) => m
                .entries
                .iter_mut()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v)
                .ok_or(crate::Error::NoSuchPath),
            (Node::Array(a), PathSeg::Index(i)) => {
                a.items.get_mut(*i).ok_or(crate::Error::NoSuchPath)
            }
            // Only element children are addressable, so an index means the
            // n-th element, skipping whitespace text and comments.
            (Node::Element(e), PathSeg::Index(i)) => e
                .children
                .iter_mut()
                .filter(|c| matches!(c, Node::Element(_)))
                .nth(*i)
                .ok_or(crate::Error::NoSuchPath),
            _ => Err(crate::Error::NoSuchPath),
        }
    }

    /// The text of a scalar node, as it would appear in a cell.
    pub fn scalar_text(&self) -> String {
        match self {
            Node::Null => "null".to_string(),
            Node::Bool(b) => b.to_string(),
            Node::Number(n) => n.clone(),
            Node::Str(s) => s.clone(),
            Node::Text(t) => t.clone(),
            _ => String::new(),
        }
    }
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
