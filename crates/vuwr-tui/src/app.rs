//! Application state and key handling for the TUI.
//!
//! Key scheme: vim-flavoured, arrows always work. `i`/`Enter` edit a cell,
//! `:` opens the command line (`w`, `q`, `q!`, `wq`), `u`/`Ctrl-R` are
//! undo/redo, `gg`/`G` jump, PageUp/PageDown scroll a viewport.
//! Tab cycles view modes (table/tree). In tree mode, Enter drills down
//! into a nested value, Esc drills up.

use std::fs;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vuwr_core::{CsvDoc, Document, GridState, Node};

pub enum Mode {
    Normal,
    /// Inline edit of the cursor cell; `buf` is committed as a `SetCell`.
    Edit {
        buf: String,
    },
    /// The `:` command line.
    Command {
        buf: String,
    },
}

/// Which view we're showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Table,
    Tree,
}

pub struct App {
    pub doc: Document,
    pub grid: GridState,
    pub mode: Mode,
    pub view: ViewMode,
    pub status: String,
    pub dirty: bool,
    pub quit: bool,
    pub path: PathBuf,
    widths: Vec<usize>,
    pending_g: bool,
    viewport_rows: usize,
    /// For tree view: the keys visible in the current node (or column
    /// headers for table view).
    pub tree_keys: Vec<String>,
    /// For tree view: summary strings for each visible row.
    pub tree_summaries: Vec<String>,
}

impl App {
    pub fn new(path: PathBuf, doc: Document) -> App {
        let (view, widths, tree_keys, tree_summaries) = if doc.is_json() || doc.is_xml() {
            (ViewMode::Tree, vec![], vec![], vec![])
        } else {
            (
                ViewMode::Table,
                compute_widths(doc.as_csv().expect("CSV only")),
                vec![],
                vec![],
            )
        };
        let mut app = App {
            doc,
            grid: GridState::new(),
            mode: Mode::Normal,
            view,
            status: String::new(),
            dirty: false,
            quit: false,
            path,
            widths,
            pending_g: false,
            viewport_rows: 10,
            tree_keys,
            tree_summaries,
        };
        if app.view == ViewMode::Tree {
            app.rebuild_tree();
        }
        app
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// For table view: returns (headers, row_count, column_count).
    pub fn table_dims(&self) -> (Vec<String>, usize, usize) {
        match self.view {
            ViewMode::Table => match self.doc.sheet() {
                Some(sheet) => {
                    let (rows, cols) = sheet.dims();
                    (sheet.headers(), rows, cols)
                }
                None => (Vec::new(), 0, 0),
            },
            ViewMode::Tree => (self.tree_keys.clone(), self.tree_summaries.len(), 1),
        }
    }

    /// The display text of one cell.
    pub fn table_cell(&self, row: usize, col: usize) -> Option<String> {
        match self.view {
            ViewMode::Table => self.doc.sheet()?.cell(row, col).map(|v| escape(&v)),
            ViewMode::Tree => self.tree_summaries.get(row).cloned(),
        }
    }

    /// True when column names are carried separately from the rows, so the
    /// renderer must draw a header row. CSV's header is its own first row.
    pub fn has_separate_header(&self) -> bool {
        self.doc
            .sheet()
            .map(|s| !s.header_is_first_row())
            .unwrap_or(false)
    }

    pub fn view_mode(&self) -> ViewMode {
        self.view
    }

    /// True if Tab can cycle to another view mode.
    pub fn can_cycle_view(&self) -> bool {
        self.doc.is_json() || self.doc.is_xml()
    }

    /// Rendered width of each column (table mode only).
    pub fn widths(&self) -> &[usize] {
        &self.widths
    }

    pub fn set_viewport_rows(&mut self, rows: usize) {
        self.viewport_rows = rows.max(1);
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.try_quit();
            return;
        }
        if matches!(self.mode, Mode::Normal) {
            self.normal_key(key);
        } else if matches!(self.mode, Mode::Edit { .. }) {
            self.edit_key(key);
        } else {
            self.command_key(key);
        }
    }

    fn normal_key(&mut self, key: KeyEvent) {
        let was_pending_g = self.pending_g;
        self.pending_g = false;
        match key.code {
            KeyCode::Char('q') => self.try_quit(),
            KeyCode::Char(':') => self.mode = Mode::Command { buf: String::new() },
            KeyCode::Tab if self.can_cycle_view() => self.cycle_view(),
            // `c` replaces the cell, `i`/Enter edit what is already there.
            // Without a replace key the only way to clear a long value was
            // to hold Backspace.
            KeyCode::Char('c') if self.view == ViewMode::Table => {
                if self.doc.sheet().is_some() {
                    self.mode = Mode::Edit { buf: String::new() };
                } else {
                    self.status = "this view is not editable".into();
                }
            }
            KeyCode::Char('i') | KeyCode::Enter if self.view == ViewMode::Table => {
                // Anything with a table view is editable: CSV cells, JSON
                // values, XML attributes and element text all go through
                // the same Sheet::set_cell.
                if self.doc.sheet().is_some() {
                    self.start_edit()
                } else {
                    self.status = "this view is not editable".into();
                }
            }
            KeyCode::Enter if self.view == ViewMode::Tree => self.tree_drill(),
            KeyCode::Esc if self.view == ViewMode::Tree => {
                self.grid.drill_up();
                self.rebuild_tree();
            }
            KeyCode::Char('u') => {
                if self.doc.undo() {
                    self.dirty = true;
                    self.clamp_cursor();
                }
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.doc.redo() {
                    self.dirty = true;
                    self.clamp_cursor();
                }
            }
            KeyCode::Char('g') if was_pending_g => {
                let (rows, cols) = self.grid_dims();
                self.grid.move_to(0, self.grid.cursor.1, rows, cols)
            }
            KeyCode::Char('g') => self.pending_g = true,
            KeyCode::Char('G') => {
                let (rows, cols) = self.grid_dims();
                self.grid
                    .move_to(usize::MAX, self.grid.cursor.1, rows, cols)
            }
            KeyCode::Left | KeyCode::Char('h') => {
                let (rows, cols) = self.grid_dims();
                self.grid.move_by(0, -1, rows, cols)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let (rows, cols) = self.grid_dims();
                self.grid.move_by(1, 0, rows, cols)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let (rows, cols) = self.grid_dims();
                self.grid.move_by(-1, 0, rows, cols)
            }
            KeyCode::Right | KeyCode::Char('l') => {
                let (rows, cols) = self.grid_dims();
                self.grid.move_by(0, 1, rows, cols)
            }
            KeyCode::PageDown => {
                let (rows, cols) = self.grid_dims();
                self.grid
                    .move_by(self.viewport_rows as isize, 0, rows, cols)
            }
            KeyCode::PageUp => {
                let (rows, cols) = self.grid_dims();
                self.grid
                    .move_by(-(self.viewport_rows as isize), 0, rows, cols)
            }
            KeyCode::Home => {
                let (rows, cols) = self.grid_dims();
                self.grid.move_to(self.grid.cursor.0, 0, rows, cols)
            }
            KeyCode::End => {
                let (rows, cols) = self.grid_dims();
                self.grid
                    .move_to(self.grid.cursor.0, usize::MAX, rows, cols)
            }
            _ => {}
        }
    }

    /// (rows, cols) of the current grid, adapting to view mode.
    fn grid_dims(&self) -> (usize, usize) {
        match self.view {
            ViewMode::Table => {
                if let Some(csv) = self.doc.as_csv() {
                    (csv.height(), csv.width())
                } else {
                    (0, 0)
                }
            }
            ViewMode::Tree => (self.tree_summaries.len(), self.tree_keys.len().max(1)),
        }
    }

    fn cycle_view(&mut self) {
        if !self.doc.is_json() && !self.doc.is_xml() {
            return;
        }
        self.view = match self.view {
            ViewMode::Tree => {
                let eligible = if self.doc.is_json() {
                    self.doc.json_table_eligible()
                } else {
                    self.doc.xml_table_eligible()
                };
                if eligible {
                    self.rebuild_table_widths();
                    ViewMode::Table
                } else {
                    self.status = "table view not available (repeated structure required)".into();
                    ViewMode::Tree
                }
            }
            ViewMode::Table => {
                self.rebuild_tree();
                ViewMode::Tree
            }
        };
        self.grid.cursor = (0, 0);
        self.grid.offset = (0, 0);
    }

    /// Size JSON/XML table columns to their contents, the way CSV columns
    /// already are. These used to be pinned at 3 characters, so anything
    /// wider than `abc` — every header longer than three letters included —
    /// was clipped away entirely.
    fn rebuild_table_widths(&mut self) {
        let (headers, row_count, col_count) = self.table_dims();
        if col_count == 0 {
            self.widths = Vec::new();
            return;
        }
        let mut widths: Vec<usize> = headers
            .iter()
            .map(|h| h.chars().count().clamp(3, 40))
            .collect();
        for r in 0..row_count.min(1000) {
            for (c, w) in widths.iter_mut().enumerate().take(col_count) {
                if let Some(text) = self.table_cell(r, c) {
                    *w = (*w).max(text.chars().count()).min(40);
                }
            }
        }
        self.widths = widths;
        self.tree_keys = headers;
    }

    fn rebuild_tree(&mut self) {
        self.tree_keys.clear();
        self.tree_summaries.clear();
        if self.doc.is_json() {
            let node = self.current_json_node().clone();
            build_tree_view(&node, &mut self.tree_keys, &mut self.tree_summaries);
        } else if self.doc.is_xml() {
            let node = self.current_xml_node().clone();
            build_xml_tree_view(&node, &mut self.tree_keys, &mut self.tree_summaries);
        }
    }

    /// The JSON node we're currently viewing (respects drill-down stack).
    fn current_json_node(&self) -> &Node {
        let json = self.doc.as_json().expect("JSON doc");
        let mut node = json.root();
        for entry in &self.grid.drill_stack {
            node = match node {
                Node::Map(m) => m
                    .entries
                    .get(entry.parent_row)
                    .map(|(_, v)| v)
                    .unwrap_or(node),
                Node::Array(a) => a.items.get(entry.parent_row).unwrap_or(node),
                _ => node,
            };
        }
        node
    }

    /// The XML node we're currently viewing (respects drill-down stack).
    fn current_xml_node(&self) -> &Node {
        let xml = self.doc.as_xml().expect("XML doc");
        let mut node = xml.root();
        for entry in &self.grid.drill_stack {
            node = match node {
                Node::Element(e) => e.children.get(entry.parent_row).unwrap_or(node),
                _ => node,
            };
        }
        node
    }

    fn tree_drill(&mut self) {
        let node = if self.doc.is_json() {
            self.current_json_node()
        } else {
            self.current_xml_node()
        };
        let (row, _col) = self.grid.cursor;
        let child = match node {
            Node::Map(m) => m.entries.get(row).map(|(_, v)| v),
            Node::Array(a) => a.items.get(row),
            Node::Element(e) => e.children.get(row),
            _ => None,
        };
        match child {
            Some(Node::Map(_)) | Some(Node::Array(_)) | Some(Node::Element(_)) => {
                self.grid.drill_down();
                self.rebuild_tree();
            }
            _ => {
                self.start_edit();
            }
        }
    }

    fn start_edit(&mut self) {
        let (r, c) = self.grid.cursor;
        let buf = match self.view {
            ViewMode::Table => self
                .doc
                .sheet()
                .and_then(|s| s.cell(r, c))
                .unwrap_or_default(),
            ViewMode::Tree => {
                let node = if self.doc.is_json() {
                    self.current_json_node()
                } else {
                    self.current_xml_node()
                };
                match node {
                    Node::Map(m) => m
                        .entries
                        .get(r)
                        .map(|(_, v)| node_to_edit_string(v))
                        .unwrap_or_default(),
                    Node::Array(a) => a.items.get(r).map(node_to_edit_string).unwrap_or_default(),
                    Node::Element(e) => e
                        .children
                        .get(r)
                        .map(node_to_edit_string)
                        .unwrap_or_default(),
                    _ => String::new(),
                }
            }
        };
        self.mode = Mode::Edit { buf };
    }

    fn edit_key(&mut self, key: KeyEvent) {
        let Mode::Edit { buf } = &mut self.mode else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                let (row, column) = self.grid.cursor;
                let value = std::mem::take(buf);
                self.mode = Mode::Normal;
                match self.doc.set_cell(row, column, &value) {
                    Ok(()) => {
                        self.dirty = true;
                        // JSON/XML edits can change a cell's rendered width.
                        if self.doc.sheet().is_some() && !self.doc.is_csv() {
                            self.rebuild_table_widths();
                        }
                    }
                    Err(e) => self.status = e.to_string(),
                }
            }
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                buf.push(c);
            }
            _ => {}
        }
    }

    fn command_key(&mut self, key: KeyEvent) {
        let Mode::Command { buf } = &mut self.mode else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                let cmd = std::mem::take(buf);
                self.mode = Mode::Normal;
                self.run_command(&cmd);
            }
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                buf.push(c);
            }
            _ => {}
        }
    }

    fn run_command(&mut self, cmd: &str) {
        match cmd.trim() {
            "w" => self.save(),
            "q" => self.try_quit(),
            "q!" => self.quit = true,
            "wq" => {
                self.save();
                if !self.dirty {
                    self.quit = true;
                }
            }
            other => self.status = format!("unknown command :{other}"),
        }
    }

    fn save(&mut self) {
        match fs::write(&self.path, self.doc.serialize()) {
            Ok(()) => {
                self.dirty = false;
                self.status = format!("wrote {}", self.path.display());
            }
            Err(e) => self.status = format!("save failed: {e}"),
        }
    }

    fn try_quit(&mut self) {
        if self.dirty {
            self.status = "unsaved changes — :q! to discard, :wq to save and quit".into();
        } else {
            self.quit = true;
        }
    }

    fn clamp_cursor(&mut self) {
        let (rows, cols) = self.grid_dims();
        let (r, c) = self.grid.cursor;
        self.grid.move_to(r, c, rows, cols);
    }
}

/// Rendered width per column: the widest value seen in the first 1000 rows
/// (after escaping), clamped to 3..=40.
fn compute_widths(doc: &CsvDoc) -> Vec<usize> {
    let mut widths = vec![3usize; doc.width()];
    for row in doc.rows().iter().take(1000) {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(escape(&cell.value).chars().count()).min(40);
        }
    }
    widths
}

/// Make control characters visible so a cell always renders on one line.
pub(crate) fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Build tree view keys and summaries for a JSON node.
fn build_tree_view(node: &Node, keys: &mut Vec<String>, summaries: &mut Vec<String>) {
    keys.clear();
    summaries.clear();
    match node {
        Node::Map(m) => {
            for (key, val) in &m.entries {
                keys.push(key.clone());
                summaries.push(summarize_node(val));
            }
        }
        Node::Array(arr) => {
            for (i, item) in arr.items.iter().enumerate() {
                keys.push(i.to_string());
                summaries.push(summarize_node(item));
            }
        }
        _ => {
            keys.push("value".to_string());
            summaries.push(summarize_node(node));
        }
    }
}

/// Summarize a node for display: `{key1, key2}` for objects, `[n]` for
/// arrays, the value for scalars.
fn summarize_node(node: &Node) -> String {
    match node {
        Node::Null => "null".to_string(),
        Node::Bool(b) => b.to_string(),
        Node::Number(s) => s.clone(),
        Node::Str(s) => format!("\"{}\"", s),
        Node::Map(m) => {
            if m.entries.is_empty() {
                "{}".to_string()
            } else {
                let keys: Vec<&str> = m.entries.iter().take(3).map(|(k, _)| k.as_str()).collect();
                let more = if m.entries.len() > 3 { ",…" } else { "" };
                format!("{{{},{}}}", keys.join(", "), more)
            }
        }
        Node::Array(arr) => {
            if arr.items.is_empty() {
                "[]".to_string()
            } else {
                format!("[{}]", arr.items.len())
            }
        }
        Node::Element(e) => format!("<{}>", e.tag),
        Node::Comment(text) => format!("<!--{}-->", text.chars().take(20).collect::<String>()),
        Node::Text(s) => s.chars().take(30).collect(),
        Node::XmlDecl(_) => "<?xml?>".to_string(),
        Node::ProcessingInstruction { target, .. } => format!("<?{target}?>"),
    }
}

/// Convert a JSON node to an editable string (for inline editing).
fn node_to_edit_string(node: &Node) -> String {
    match node {
        Node::Null => String::new(),
        Node::Bool(b) => b.to_string(),
        Node::Number(s) => s.clone(),
        Node::Str(s) => s.clone(),
        Node::Text(s) => s.clone(),
        _ => String::new(),
    }
}

/// Build tree view for an XML node.
fn build_xml_tree_view(node: &Node, keys: &mut Vec<String>, summaries: &mut Vec<String>) {
    keys.clear();
    summaries.clear();
    match node {
        Node::Element(e) => {
            for (i, child) in e.children.iter().enumerate() {
                match child {
                    Node::Element(c) => {
                        keys.push(format!("<{}>", c.tag));
                        summaries.push(summarize_xml_element(c));
                    }
                    Node::Comment(text) => {
                        keys.push(format!("<!--{i}-->"));
                        summaries.push(text.trim().to_string());
                    }
                    Node::Text(text) => {
                        keys.push(format!("text{i}"));
                        summaries.push(text.trim().to_string());
                    }
                    Node::XmlDecl(_) => {
                        keys.push("<?xml?>".to_string());
                        summaries.push("declaration".to_string());
                    }
                    Node::ProcessingInstruction { target, .. } => {
                        keys.push(format!("<?{target}?>"));
                        summaries.push(target.clone());
                    }
                    _ => {
                        keys.push(i.to_string());
                        summaries.push(summarize_node(child));
                    }
                }
            }
        }
        _ => {
            keys.push("value".to_string());
            summaries.push(summarize_node(node));
        }
    }
}

/// Summarize an XML element for display.
fn summarize_xml_element(elem: &vuwr_core::Element) -> String {
    if elem.children.is_empty() {
        if elem.attributes.is_empty() {
            "/>".to_string()
        } else {
            let attrs: Vec<&str> = elem
                .attributes
                .iter()
                .take(3)
                .map(|(k, _, _)| k.as_str())
                .collect();
            format!("{}…", attrs.join(", "))
        }
    } else if elem.children.len() == 1 {
        match &elem.children[0] {
            Node::Text(t) => t.chars().take(30).collect(),
            _ => format!("[{}]", elem.children.len()),
        }
    } else {
        format!("[{}]", elem.children.len())
    }
}
