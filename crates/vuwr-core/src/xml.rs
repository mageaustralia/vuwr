//! XML parsing and serialization with format-preserving round-trips.
//!
//! The parser preserves comments, attribute order, text content (including
//! whitespace), processing instructions, and the XML declaration.
//! Self-closing tags (`<br/>`) are distinguished from empty elements
//! (`<br></br>`).

use crate::Error;
use crate::node::{Attr, Element, Node, PathSeg, XmlDecl};

/// A parsed XML document.
#[derive(Debug, Clone)]
pub struct XmlDoc {
    children: Vec<Node>,
    /// The table's shape, worked out once.
    ///
    /// Finding it means walking every row — 2,277 of them in a feed — and
    /// a cell lookup needs it. Recomputing per cell took seven seconds to
    /// draw one screen. Cleared by [`XmlDoc::root_mut`], which is the only
    /// way the document changes.
    shape: std::cell::RefCell<Option<TableShape>>,
}

/// Where the rows are and what the columns are called.
#[derive(Debug, Clone)]
struct TableShape {
    /// Path from the root to the element whose children are the rows.
    parent: Vec<PathSeg>,
    /// Column names: the union of every row's fields, first seen first.
    headers: Vec<String>,
    /// Each row's position among the parent's children, so a lookup does
    /// not re-scan them.
    row_positions: Vec<usize>,
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
        Ok(XmlDoc {
            children,
            shape: std::cell::RefCell::new(None),
        })
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
        // Any edit can add or remove a field, which changes the columns.
        self.shape.replace(None);
        let idx = self
            .children
            .iter()
            .position(|n| matches!(n, Node::Element(_)))
            .unwrap_or(0);
        &mut self.children[idx]
    }

    /// The path from the root to the element whose children are the rows.
    ///
    /// Feeds wrap their records: `<rss><channel><item>…` puts the
    /// repeating part two levels down. Looking only at the document
    /// element's children made such a file one row of 2,277 columns all
    /// called `item`. So descend through single-child wrappers and take
    /// the level that actually repeats.
    pub fn table_parent_path(&self) -> Option<Vec<PathSeg>> {
        let Node::Element(root) = self.root() else {
            return None;
        };
        let mut path = Vec::new();
        let mut current = root;
        // The deepest level seen that could be a table, in case nothing
        // below it repeats: a one-record feed is still a table.
        let mut fallback = None;

        loop {
            let kids = element_children(current);
            let uniform = !kids.is_empty() && kids.iter().all(|k| k.tag == kids[0].tag);
            // Something that repeats is the table, and the shallowest such
            // level wins.
            if uniform && kids.len() > 1 {
                return Some(path);
            }
            // One uniform child that carries fields of its own is a table
            // of one row — but keep looking deeper first, in case the real
            // repetition is further down. The shallowest candidate is
            // kept, so `<rows><row><name>…` gives rows of `row`, not of
            // `name`.
            if uniform
                && fallback.is_none()
                && (!element_children(kids[0]).is_empty() || !kids[0].attributes.is_empty())
            {
                fallback = Some(path.clone());
            }
            // Only a lone child is a wrapper; several different tags are
            // the record itself, and there is nowhere further to go.
            if kids.len() != 1 {
                return fallback;
            }
            path.push(PathSeg::Index(0));
            current = kids[0];
        }
    }

    /// The rows of table view.
    ///
    /// Whitespace between elements parses as `Text` nodes, and comments as
    /// `Comment`; neither is a row, so both are skipped — otherwise a
    /// pretty-printed file's rows are all off by one.
    pub fn row_elements(&self) -> Vec<&Element> {
        let Some(path) = self.table_parent_path() else {
            return Vec::new();
        };
        match self.root().get_at(&path) {
            Some(Node::Element(parent)) => element_children(parent),
            _ => Vec::new(),
        }
    }

    /// Run `f` against the cached shape, without copying it.
    ///
    /// Returning a clone copied one `usize` per row — 2,277 of them — on
    /// every cell lookup, which is most of what made a feed crawl.
    fn with_shape<R>(&self, f: impl FnOnce(&TableShape) -> R) -> Option<R> {
        self.ensure_shape();
        self.shape.borrow().as_ref().map(f)
    }

    /// How many rows the table has, without building a list of them.
    pub fn row_count(&self) -> usize {
        self.with_shape(|s| s.row_positions.len()).unwrap_or(0)
    }

    /// Work the shape out if it is not already known.
    fn ensure_shape(&self) {
        if self.shape.borrow().is_some() {
            return;
        }
        if let Some(shape) = self.compute_shape() {
            self.shape.replace(Some(shape));
        }
    }

    /// The table's shape, computed once and cached.
    fn compute_shape(&self) -> Option<TableShape> {
        let parent_path = self.table_parent_path()?;
        let Some(Node::Element(parent)) = self.root().get_at(&parent_path) else {
            return None;
        };
        let row_positions: Vec<usize> = parent
            .children
            .iter()
            .enumerate()
            .filter(|(_, c)| matches!(c, Node::Element(_)))
            .map(|(i, _)| i)
            .collect();

        let mut seen = std::collections::BTreeSet::new();
        let mut headers = Vec::new();
        for &i in &row_positions {
            let Node::Element(row) = &parent.children[i] else {
                continue;
            };
            for (name, _, _, _) in &row.attributes {
                if seen.insert(name.clone()) {
                    headers.push(name.clone());
                }
            }
        }
        for &i in &row_positions {
            let Node::Element(row) = &parent.children[i] else {
                continue;
            };
            for child in element_children(row) {
                if seen.insert(child.tag.clone()) {
                    headers.push(child.tag.clone());
                }
            }
        }

        Some(TableShape {
            parent: parent_path,
            headers,
            row_positions,
        })
    }

    /// The element for one row, without re-scanning the others.
    fn row_at(&self, row: usize) -> Option<&Element> {
        let (parent, position) =
            self.with_shape(|s| (s.parent.clone(), s.row_positions.get(row).copied()))?;
        let position = position?;
        match self.root().get_at(&parent) {
            Some(Node::Element(parent)) => match parent.children.get(position) {
                Some(Node::Element(e)) => Some(e),
                _ => None,
            },
            _ => None,
        }
    }

    /// Column headers for table view.
    ///
    /// The union of every row's field names, in the order first seen —
    /// not just the first row's. Records have optional fields: an item
    /// with no `g:sale_price` would otherwise shift every later value one
    /// column left, silently filing gtins under brands.
    pub fn table_headers(&self) -> Vec<String> {
        self.with_shape(|s| s.headers.clone()).unwrap_or_default()
    }

    /// The value at `(row, col)` under [`XmlDoc::table_headers`], found by
    /// name rather than position, so a missing field leaves a gap instead
    /// of shifting the row.
    pub fn table_cell(&self, row: usize, col: usize) -> Option<String> {
        let name = self.with_shape(|s| s.headers.get(col).cloned())??;
        let elem = self.row_at(row)?;
        Some(field_of(elem, &name).unwrap_or_default())
    }

    /// True when the cell's text lives in a CDATA section.
    ///
    /// Content there is already literal, so encoding it on the way in
    /// would double the escaping.
    pub fn cell_is_cdata(&self, row: usize, col: usize) -> bool {
        let Some(Some(name)) = self.with_shape(|s| s.headers.get(col).cloned()) else {
            return false;
        };
        let Some(elem) = self.row_at(row) else {
            return false;
        };
        element_children(elem)
            .into_iter()
            .find(|c| c.tag == name)
            .is_some_and(|c| c.children.iter().any(|k| matches!(k, Node::CData(_))))
    }

    /// The path addressing the cell at `(row, col)`: an attribute of the
    /// row element, or the text of one of its child elements.
    pub fn cell_path(&self, row: usize, col: usize) -> Option<crate::node::NodePath> {
        let name = self.with_shape(|s| s.headers.get(col).cloned())??;
        let mut prefix = self.with_shape(|s| s.parent.clone())?;
        let elem = self.row_at(row)?;

        if elem.attributes.iter().any(|(n, _, _, _)| *n == name) {
            prefix.push(PathSeg::Index(row));
            prefix.push(PathSeg::Attr(name));
            return Some(prefix);
        }
        // Fields are addressed by name, so the index is the position of
        // *this row's* matching child, not the column number.
        let child_idx = element_children(elem).iter().position(|c| c.tag == name)?;
        prefix.push(PathSeg::Index(row));
        prefix.push(PathSeg::Index(child_idx));
        prefix.push(PathSeg::Text);
        Some(prefix)
    }

    /// Re-indent the document.
    ///
    /// The parser preserves whatever layout a file had, which is the point
    /// of this tool; this is the deliberate opposite. Elements holding
    /// text are left on one line: breaking them would insert whitespace
    /// *into* the text, changing what the document says. That is why XML
    /// reformatting is conservative where JSON's is not.
    pub fn reformat(&mut self, style: crate::Layout) {
        self.shape.replace(None);
        let indent = match style {
            crate::Layout::Compact => None,
            crate::Layout::Pretty | crate::Layout::Smart => Some("  "),
        };
        for child in &mut self.children {
            relayout(child, indent, 0);
        }
        // Between top-level nodes: a declaration and the root element on
        // one line is legal but unreadable.
        if indent.is_some() {
            let mut spaced = Vec::new();
            for (i, child) in self.children.drain(..).enumerate() {
                if i > 0 && !matches!(child, Node::Text(_)) {
                    spaced.push(Node::Text("\n".to_string()));
                }
                if !matches!(child, Node::Text(_)) {
                    spaced.push(child);
                }
            }
            self.children = spaced;
        } else {
            self.children.retain(|c| !is_whitespace_text(c));
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for child in &self.children {
            serialize_node(child, &mut out);
        }
        out
    }
}

/// One named field of a record: an attribute, or the text of a child
/// element with that tag.
fn field_of(elem: &Element, name: &str) -> Option<String> {
    if let Some((_, value, _, _)) = elem.attributes.iter().find(|(n, _, _, _)| n == name) {
        return Some(value.clone());
    }
    element_children(elem)
        .into_iter()
        .find(|c| c.tag == name)
        .map(|c| c.text_content())
}

/// An element's element children, in order.
fn element_children(e: &Element) -> Vec<&Element> {
    e.children
        .iter()
        .filter_map(|c| match c {
            Node::Element(child) => Some(child),
            _ => None,
        })
        .collect()
}

/// True for a text node that is only whitespace — layout, not content.
fn is_whitespace_text(node: &Node) -> bool {
    matches!(node, Node::Text(t) if t.trim().is_empty())
}

/// Re-indent one element and its descendants.
fn relayout(node: &mut Node, indent: Option<&str>, depth: usize) {
    let Node::Element(e) = node else {
        return;
    };
    // An element with text in it keeps its children exactly: adding
    // newlines around them would add that whitespace to the text.
    let has_text = e
        .children
        .iter()
        .any(|c| matches!(c, Node::Text(t) if !t.trim().is_empty()) || matches!(c, Node::CData(_)));
    if has_text {
        return;
    }

    e.children.retain(|c| !is_whitespace_text(c));
    for child in &mut e.children {
        relayout(child, indent, depth + 1);
    }

    let Some(unit) = indent else {
        return;
    };
    if e.children.is_empty() {
        return;
    }
    let inner = format!("\n{}", unit.repeat(depth + 1));
    let closing = format!("\n{}", unit.repeat(depth));
    let mut spaced = Vec::with_capacity(e.children.len() * 2 + 1);
    for child in e.children.drain(..) {
        spaced.push(Node::Text(inner.clone()));
        spaced.push(child);
    }
    spaced.push(Node::Text(closing));
    e.children = spaced;
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
        Node::CData(text) => {
            out.extend_from_slice(b"<![CDATA[");
            out.extend_from_slice(text.as_bytes());
            out.extend_from_slice(b"]]>");
        }
        Node::Doctype(raw) => {
            out.extend_from_slice(raw.as_bytes());
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
            for (name, value, q, leading) in &elem.attributes {
                // An empty separator would run attributes together, so a
                // single space stands in for one we never saw.
                if leading.is_empty() {
                    out.push(b' ');
                } else {
                    out.extend_from_slice(leading.as_bytes());
                }
                out.extend_from_slice(name.as_bytes());
                out.extend_from_slice(format!("={}{}{}", q, value, q).as_bytes());
            }
            out.extend_from_slice(elem.tag_trailing.as_bytes());
            if elem.self_closing {
                out.extend_from_slice(b"/>");
            } else {
                out.push(b'>');
                for child in &elem.children {
                    serialize_node(child, out);
                }
                out.extend_from_slice(b"</");
                out.extend_from_slice(elem.tag.as_bytes());
                out.extend_from_slice(elem.close_trailing.as_bytes());
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
    // `<?xml` must be the whole target: `<?xml-stylesheet ...?>` is a
    // processing instruction, and treating it as the declaration threw
    // its contents away and wrote back `<?xml version=""?>`.
    if is_xml_declaration(input) {
        parse_xml_decl(full, input)
    } else if input.starts_with("<!--") {
        parse_comment(full, input)
    } else if input.starts_with("<![CDATA[") {
        parse_cdata(full, input)
    } else if input.starts_with("<!DOCTYPE") || input.starts_with("<!doctype") {
        parse_doctype(full, input)
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

/// True for `<?xml ...?>` itself, not for a PI whose target merely starts
/// with those letters.
fn is_xml_declaration(input: &str) -> bool {
    let Some(rest) = input
        .strip_prefix("<?xml")
        .or_else(|| input.strip_prefix("<?XML"))
    else {
        return false;
    };
    matches!(rest.chars().next(), Some(c) if c.is_whitespace()) || rest.starts_with("?>")
}

/// `<![CDATA[ ... ]]>`.
///
/// The contents are held raw. A CDATA section exists precisely so its
/// text is not markup, so touching the escaping would change what the
/// document says.
fn parse_cdata<'a>(full: &str, input: &'a str) -> Result<(Node, &'a str), Error> {
    const OPEN: &str = "<![CDATA[";
    let rest = &input[OPEN.len()..];
    let end = rest.find("]]>").ok_or(Error::UnexpectedEof {
        offset: offset_of(full, input),
    })?;
    Ok((
        Node::CData(rest[..end].to_string()),
        &rest[end + "]]>".len()..],
    ))
}

/// `<!DOCTYPE ...>`, kept verbatim.
///
/// Internal subsets can contain `>` inside brackets, so the scan tracks
/// them rather than stopping at the first one.
fn parse_doctype<'a>(full: &str, input: &'a str) -> Result<(Node, &'a str), Error> {
    let bytes = input.as_bytes();
    let mut depth = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'[' => depth += 1,
            b']' => depth = depth.saturating_sub(1),
            b'>' if depth == 0 => {
                return Ok((Node::Doctype(input[..=i].to_string()), &input[i + 1..]));
            }
            _ => {}
        }
    }
    Err(Error::UnexpectedEof {
        offset: offset_of(full, input),
    })
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

    // Whatever sits between the last attribute and the `>` (or `/>`).
    // Generated feeds contain `<tag >` and `<tag />`, and dropping it
    // rewrites bytes nobody asked us to touch.
    let trimmed = tag_content.trim_end();
    let tag_trailing = tag_content[trimmed.len()..].to_string();

    let (tag, attrs) = parse_tag_and_attrs(tag_content)?;

    if self_closing {
        Ok((
            Node::Element(Element {
                tag,
                attributes: attrs,
                children: Vec::new(),
                self_closing: true,
                tag_trailing,
                close_trailing: String::new(),
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
        let close_raw = &rest[2..close_end];
        let closing = close_raw.trim();
        let close_trailing = close_raw[close_raw.trim_end().len()..].to_string();
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
                tag_trailing,
                close_trailing,
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
        // Keep whatever separated this attribute from the last one: a
        // newline and an indent are as valid as a single space.
        let before = rest;
        rest = rest.trim_start();
        let leading = before[..before.len() - rest.len()].to_string();
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
            attrs.push((name, String::new(), '"', leading));
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
            attrs.push((name, value, quote as char, leading));
            rest = &rest[close + 1..];
        } else {
            let val_end = rest
                .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
                .unwrap_or(rest.len());
            let value = rest[..val_end].to_string();
            attrs.push((name, value, '"', leading));
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
