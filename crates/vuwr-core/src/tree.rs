//! The expandable tree.
//!
//! The first tree was a stack of sheets: descending replaced the view and
//! Esc popped back. That reads fine in a terminal and badly in a window,
//! where people expect to open a node and still see its neighbours.
//!
//! This flattens the document into the rows currently visible, given a set
//! of expanded paths. Both frontends render the same rows; expanding is
//! just adding a path to the set.

use std::collections::BTreeSet;

use crate::node::{Node, PathSeg};

/// What a row shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// A leaf: a number, string, boolean, null, or text.
    Scalar,
    /// A container, and whether it is open.
    Container { expanded: bool },
}

/// The type of a value, for colouring. Frontends map these to their own
/// palettes rather than core deciding what "blue" means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Null,
    Bool,
    Number,
    String,
    Array,
    Object,
    Element,
    Comment,
    Text,
    Other,
}

impl ValueKind {
    pub fn of(node: &Node) -> Self {
        match node {
            Node::Null => Self::Null,
            Node::Bool(_) => Self::Bool,
            Node::Number(_) => Self::Number,
            Node::Str(_) => Self::String,
            Node::Array(_) => Self::Array,
            Node::Map(_) => Self::Object,
            Node::Element(_) => Self::Element,
            Node::Comment(_) => Self::Comment,
            // CDATA is text as far as a reader is concerned; only the
            // escaping differs.
            Node::Text(_) | Node::CData(_) => Self::Text,
            _ => Self::Other,
        }
    }
}

/// One visible line of the tree.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeRow {
    /// How to address this node for editing.
    pub path: Vec<PathSeg>,
    /// Nesting level, for indentation.
    pub depth: usize,
    /// The key, index or tag on the left.
    pub label: String,
    /// The value, or a summary for a container.
    pub summary: String,
    pub kind: RowKind,
    pub value: ValueKind,
    /// True when this row's key repeats one already used in its parent.
    /// Legal JSON, and almost always a mistake: later keys win silently in
    /// most parsers, so one of the values is dead.
    pub duplicate: bool,
}

impl TreeRow {
    pub fn is_container(&self) -> bool {
        matches!(self.kind, RowKind::Container { .. })
    }

    pub fn is_expanded(&self) -> bool {
        matches!(self.kind, RowKind::Container { expanded: true })
    }
}

/// Which paths are open. Paths rather than indices, so the right nodes
/// stay open when the document changes underneath.
#[derive(Debug, Clone, Default)]
pub struct Expansion {
    open: BTreeSet<Vec<PathSeg>>,
}

impl Expansion {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self, path: &[PathSeg]) -> bool {
        self.open.contains(path)
    }

    /// Toggle a path. Returns true if it is now open.
    pub fn toggle(&mut self, path: &[PathSeg]) -> bool {
        if self.open.remove(path) {
            false
        } else {
            self.open.insert(path.to_vec());
            true
        }
    }

    pub fn open(&mut self, path: &[PathSeg]) {
        self.open.insert(path.to_vec());
    }

    pub fn close(&mut self, path: &[PathSeg]) {
        self.open.remove(path);
    }

    pub fn collapse_all(&mut self) {
        self.open.clear();
    }

    /// Open every container in the document.
    pub fn expand_all(&mut self, root: &Node) {
        self.open.clear();
        let mut path = Vec::new();
        collect_containers(root, &mut path, &mut self.open);
    }

    /// The starting point: the top level is listed, and nothing below it
    /// is open. The root itself is never drawn, so its children are always
    /// visible; this exists so callers do not have to know that.
    pub fn expand_root(&mut self, _root: &Node) {
        self.open.clear();
        self.open.insert(Vec::new());
    }
}

fn collect_containers(node: &Node, path: &mut Vec<PathSeg>, out: &mut BTreeSet<Vec<PathSeg>>) {
    if !is_container(node) {
        return;
    }
    out.insert(path.clone());
    for (seg, child) in children(node) {
        path.push(seg);
        collect_containers(child, path, out);
        path.pop();
    }
}

fn is_container(node: &Node) -> bool {
    match node {
        Node::Array(_) | Node::Map(_) => true,
        // An element with only text is a leaf: showing its text as a child
        // row would double every value in an XML document.
        Node::Element(e) => e.children.iter().any(|c| matches!(c, Node::Element(_))),
        _ => false,
    }
}

/// A node's addressable children, paired with the step that reaches them.
fn children(node: &Node) -> Vec<(PathSeg, &Node)> {
    match node {
        Node::Map(m) => m
            .entries
            .iter()
            .map(|(k, v)| (PathSeg::Key(k.clone()), v))
            .collect(),
        Node::Array(a) => a
            .items
            .iter()
            .enumerate()
            .map(|(i, v)| (PathSeg::Index(i), v))
            .collect(),
        Node::Element(e) => e
            .children
            .iter()
            .filter(|c| matches!(c, Node::Element(_)))
            .enumerate()
            .map(|(i, c)| (PathSeg::Index(i), c))
            .collect(),
        _ => Vec::new(),
    }
}

/// Flatten `root` into the rows visible under `expansion`.
pub fn rows(root: &Node, expansion: &Expansion) -> Vec<TreeRow> {
    let mut out = Vec::new();
    let mut path = Vec::new();
    // The root itself is not drawn; its children are the top level.
    walk(root, &mut path, 0, expansion, &mut out);
    out
}

fn walk(
    node: &Node,
    path: &mut Vec<PathSeg>,
    depth: usize,
    expansion: &Expansion,
    out: &mut Vec<TreeRow>,
) {
    let kids = children(node);
    // A key used twice in the same object: legal, and almost certainly a
    // mistake, so it is worth flagging rather than silently showing both.
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut dupes: BTreeSet<&str> = BTreeSet::new();
    if let Node::Map(m) = node {
        for (k, _) in &m.entries {
            if !seen.insert(k.as_str()) {
                dupes.insert(k.as_str());
            }
        }
    }

    for (seg, child) in kids {
        let label = label_of(&seg, child);
        let duplicate = match &seg {
            PathSeg::Key(k) => dupes.contains(k.as_str()),
            _ => false,
        };

        path.push(seg);
        let container = is_container(child);
        let expanded = container && expansion.is_open(path);
        out.push(TreeRow {
            path: path.clone(),
            depth,
            label,
            summary: summarize(child),
            kind: if container {
                RowKind::Container { expanded }
            } else {
                RowKind::Scalar
            },
            value: ValueKind::of(child),
            duplicate,
        });
        if expanded {
            walk(child, path, depth + 1, expansion, out);
        }
        path.pop();
    }
}

/// The key, tag or index a row is drawn with.
fn label_of(seg: &PathSeg, child: &Node) -> String {
    match seg {
        PathSeg::Key(k) => k.clone(),
        PathSeg::Index(i) => match child {
            Node::Element(e) => e.tag.clone(),
            _ => i.to_string(),
        },
        PathSeg::Attr(a) => format!("@{a}"),
        PathSeg::Text => "#text".to_string(),
    }
}

/// One node, as searching sees it.
pub struct Entry {
    pub path: Vec<PathSeg>,
    pub label: String,
    pub summary: String,
}

/// Every node in the document, in the order the tree draws them, open or
/// not.
///
/// Searching has to see what a collapsed node is hiding: a match on a SKU
/// inside item 78 of a feed is the answer the reader wanted, even though
/// nothing on screen is showing it yet. [`rows`] can only report what is
/// already visible, which is why it is not what a search walks.
pub fn flatten(root: &Node) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut path = Vec::new();
    walk_all(root, &mut path, &mut out);
    out
}

fn walk_all(node: &Node, path: &mut Vec<PathSeg>, out: &mut Vec<Entry>) {
    for (seg, child) in children(node) {
        let label = label_of(&seg, child);
        path.push(seg);
        out.push(Entry {
            path: path.clone(),
            label,
            summary: summarize(child),
        });
        walk_all(child, path, out);
        path.pop();
    }
}

/// A node's children as key/value pairs, for the inspector.
///
/// Read from the node itself rather than from the drawn rows, because the
/// record has to be readable whether or not it happens to be open.
pub fn child_fields(node: &Node) -> Vec<(String, String, ValueKind)> {
    children(node)
        .into_iter()
        .map(|(seg, child)| {
            (
                label_of(&seg, child),
                summarize(child),
                ValueKind::of(child),
            )
        })
        .collect()
}

/// The steps to a node's children, in the order [`child_fields`] lists
/// them — so an index into one is an index into the other.
pub fn child_steps(node: &Node) -> Vec<PathSeg> {
    children(node).into_iter().map(|(seg, _)| seg).collect()
}

/// The node a path leads to, if it leads anywhere.
pub fn at<'a>(root: &'a Node, path: &[PathSeg]) -> Option<&'a Node> {
    let mut node = root;
    for seg in path {
        node = children(node)
            .into_iter()
            .find(|(s, _)| s == seg)
            .map(|(_, c)| c)?;
    }
    Some(node)
}

/// A one-line stand-in for a value.
pub fn summarize(node: &Node) -> String {
    match node {
        Node::Null => "null".to_string(),
        Node::Bool(b) => b.to_string(),
        Node::Number(n) => n.clone(),
        Node::Str(s) => format!("\"{s}\""),
        Node::Array(a) => format!("[{}]", a.items.len()),
        Node::Map(m) => format!("{{{}}}", m.entries.len()),
        Node::Element(e) => {
            // An element's value is its text, whether written plainly or
            // wrapped in CDATA — a feed that wraps everything in CDATA
            // would otherwise show nothing but empty rows.
            let text = e.text_content();
            let text = text.trim().to_string();
            if is_container(node) {
                format!("<{}>", e.tag)
            } else {
                // Escaped markup is decoded for display: a description
                // reading `&lt;p&gt;` is what people came here to stop
                // squinting at. The document keeps its raw bytes.
                crate::decode(&text)
            }
        }
        Node::Comment(c) => format!("<!--{}-->", c.trim()),
        Node::Text(t) => crate::decode(t.trim()),
        Node::CData(t) => t.trim().to_string(),
        Node::Doctype(raw) => raw.clone(),
        Node::XmlDecl(_) => "<?xml?>".to_string(),
        Node::ProcessingInstruction { target, .. } => format!("<?{target}?>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, FormatHint};

    fn root(src: &str) -> Node {
        let d = Document::parse(src.as_bytes(), FormatHint::Auto).unwrap();
        d.as_json()
            .map(|j| j.root().clone())
            .or_else(|| d.as_xml().map(|x| x.root().clone()))
            .unwrap()
    }

    #[test]
    fn a_collapsed_tree_shows_only_the_top_level() {
        let node = root(r#"{"a":1,"b":{"c":2}}"#);
        let mut ex = Expansion::new();
        ex.expand_root(&node);
        let r = rows(&node, &ex);
        assert_eq!(r.len(), 2, "a and b, but not c");
        assert_eq!(r[0].label, "a");
        assert_eq!(r[1].label, "b");
        assert!(r[1].is_container());
        assert!(!r[1].is_expanded(), "b starts closed");
    }

    #[test]
    fn expanding_reveals_children_in_place() {
        let node = root(r#"{"a":1,"b":{"c":2}}"#);
        let mut ex = Expansion::new();
        ex.expand_root(&node);
        ex.toggle(&[PathSeg::Key("b".into())]);
        let r = rows(&node, &ex);
        assert_eq!(r.len(), 3);
        assert_eq!(r[2].label, "c");
        assert_eq!(r[2].depth, 1, "nested rows are indented");
        assert_eq!(r[0].label, "a", "the neighbours are still there");
    }

    #[test]
    fn expand_all_then_collapse_all() {
        let node = root(r#"{"a":{"b":{"c":1}}}"#);
        let mut ex = Expansion::new();
        ex.expand_all(&node);
        assert_eq!(rows(&node, &ex).len(), 3, "every level visible");
        ex.collapse_all();
        // The root is not drawn, so its children are the top level and
        // stay listed; collapsing closes everything beneath them.
        assert_eq!(rows(&node, &ex).len(), 1, "top level only");
    }

    #[test]
    fn arrays_are_indexed_and_summarised() {
        let node = root(r#"{"xs":[1,2,3]}"#);
        let mut ex = Expansion::new();
        ex.expand_root(&node);
        let r = rows(&node, &ex);
        assert_eq!(r[0].summary, "[3]");
        ex.toggle(&[PathSeg::Key("xs".into())]);
        let r = rows(&node, &ex);
        assert_eq!(r[1].label, "0");
        assert_eq!(r[3].label, "2");
    }

    /// Duplicate keys are legal JSON and nearly always a bug: most parsers
    /// keep the last one silently, so the earlier value is dead.
    #[test]
    fn duplicate_keys_are_flagged() {
        let node = root(r#"{"color":true,"color":"gold","other":1}"#);
        let mut ex = Expansion::new();
        ex.expand_root(&node);
        let r = rows(&node, &ex);
        assert!(r[0].duplicate, "both copies are marked");
        assert!(r[1].duplicate);
        assert!(!r[2].duplicate, "the unique key is not");
    }

    #[test]
    fn value_kinds_are_reported_for_colouring() {
        let node = root(r#"{"n":1,"s":"x","b":true,"z":null,"a":[],"o":{}}"#);
        let mut ex = Expansion::new();
        ex.expand_root(&node);
        let kinds: Vec<ValueKind> = rows(&node, &ex).iter().map(|r| r.value).collect();
        assert_eq!(
            kinds,
            vec![
                ValueKind::Number,
                ValueKind::String,
                ValueKind::Bool,
                ValueKind::Null,
                ValueKind::Array,
                ValueKind::Object
            ]
        );
    }

    /// An element whose only child is text is a leaf: giving it a child row
    /// would double every value in an XML document.
    #[test]
    fn xml_text_only_elements_are_leaves() {
        let node = root("<r><name>Alice</name><kids><k/></kids></r>");
        let mut ex = Expansion::new();
        ex.expand_root(&node);
        let r = rows(&node, &ex);
        assert_eq!(r[0].label, "name");
        assert!(!r[0].is_container(), "text-only element is a leaf");
        assert_eq!(r[0].summary, "Alice");
        assert!(
            r[1].is_container(),
            "an element with element children is not"
        );
    }
}
